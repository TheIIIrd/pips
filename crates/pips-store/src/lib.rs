//! Backups of a Pips collection.
//!
//! A backup holds everything on the device that cannot be recomputed: the
//! cards as received, where each one sits (collection, archive, bin), when
//! it came and from whom, its certificate, the person's note, and the
//! profile printed on the back of their own cards. Not the secret key: that
//! is what the 24 words are for.
//!
//! # Encryption
//!
//! The backup is encrypted to its owner: the key is derived from the
//! identity's secret, so the file opens wherever the same 24 words were
//! entered, and nowhere else — no password to choose or forget. A lost file
//! reveals nothing; a lost phone plus the file is recovered from the words.
//!
//! ```text
//! header      "PPBK", version u8 = 1, owner public key [32], nonce [24]
//! sealed      XChaCha20-Poly1305(key, nonce, aad = header, body)
//! key         SHA-256("pips-v1:backup" || seed)
//! ```
//!
//! The owner's public key sits in the clear so the app can say "this is the
//! backup of another key" instead of "damaged".
//!
//! # Body
//!
//! Little-endian; `str8` is a u8 length then UTF-8, `bytes16` a u16 length.
//!
//! ```text
//! created_ms   u64
//! nick         str8                 ≤ 24 bytes
//! network      u8                   0 = no handle
//!   handle     str8                 (cond)
//! link         str8                 card rules
//! count        u32                  ≤ MAX_CARDS
//! entries:
//!   payload    bytes16              a valid card, signature checked
//!   flags      u8                   bit 0 mine, bits 1-2 place (0 collection,
//!                                   1 archive, 2 bin), bit 3 certificate,
//!                                   bit 4 sender, bit 5 binned-at, bit 6
//!                                   issue log; bit 7 zero
//!   received   u64 ms
//!   binned_at  u64 ms               (cond)
//!   certificate [105]               (cond) checked against the card
//!   sender     [32] key + str8 nick (cond)
//!   note       bytes16              ≤ MAX_NOTE, UTF-8
//!   issue log  (cond, own cards only) the author's count of copies issued:
//!     of       u16                  run size, 0 for open
//!     count    u16                  copies issued, numbered 1..=count
//!     copies:  edition u16, day u32, holder [32], nick str8
//! ```
//!
//! The issue log matters as much as the cards: without it a restored
//! device would start numbering copies from 1 again.
//!
//! Decoding is strict like every Pips parser: every length is checked
//! before use, reserved bits must be zero, nothing may follow the last
//! entry, and every card and certificate must verify. A backup is our own
//! file, but it may have travelled through clouds and USB sticks; it is
//! input like any other.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use pips_format::{Card, Handle, Network};
use pips_sign::{Certificate, PublicKey, SecretKey, CERTIFICATE_LEN};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// File signature.
pub const MAGIC: [u8; 4] = *b"PPBK";
/// Format version.
pub const VERSION: u8 = 1;
/// Bytes before the sealed body.
pub const HEADER_LEN: usize = 4 + 1 + 32 + 24;
/// Most cards in one backup.
pub const MAX_CARDS: usize = 100_000;
/// Longest note, in bytes.
pub const MAX_NOTE: usize = 4000;
/// Largest backup file accepted.
pub const MAX_FILE: usize = 64 * 1024 * 1024;

const KEY_DOMAIN: &[u8] = b"pips-v1:backup";
const TAG_LEN: usize = 16;
/// Smallest possible entry: payload length, flags, received, note length.
const MIN_ENTRY: usize = 2 + 1 + 8 + 2;

/// Everything a backup restores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backup {
    /// When it was made, ms since 1970.
    pub created_ms: u64,
    /// The back of the person's own cards.
    pub profile: Profile,
    /// Every card on the device, wherever it sits.
    pub cards: Vec<Entry>,
}

/// The back of the person's own cards.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Profile {
    /// Author name, ≤ 24 bytes.
    pub nick: String,
    /// One social handle.
    pub handle: Option<Handle>,
    /// https address without the scheme, or empty.
    pub link: String,
}

/// Where a card sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// In the collection (or among the person's own cards).
    Collection,
    /// Archived.
    Archive,
    /// In the bin.
    Bin,
}

/// One card and what the device knows about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The card exactly as received.
    pub payload: Vec<u8>,
    /// The person made it.
    pub mine: bool,
    /// Where it sits.
    pub place: Place,
    /// When it arrived, ms since 1970.
    pub received_ms: u64,
    /// When it went to the bin, if it is there.
    pub binned_ms: Option<u64>,
    /// Edition certificate that came with it.
    pub certificate: Option<[u8; CERTIFICATE_LEN]>,
    /// Who passed it on, if not the author.
    pub sender: Option<Sender>,
    /// The person's own note.
    pub note: String,
    /// For own cards: the copies issued so far.
    pub issued: Option<IssueLog>,
}

/// The copies of an own card the author has issued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueLog {
    /// Run size, 0 for an open run.
    pub of: u16,
    /// Every copy, in order: `copies[i].edition == i + 1`.
    pub copies: Vec<Issued>,
}

/// One issued copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issued {
    /// Copy number.
    pub edition: u16,
    /// Days since 1970, UTC.
    pub day: u32,
    /// The holder's key.
    pub holder: [u8; 32],
    /// The holder's nick, as their recipient code gave it.
    pub nick: String,
}

/// Who passed a card on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sender {
    /// Their key.
    pub key: [u8; 32],
    /// The nick they gave, ≤ 24 bytes.
    pub nick: String,
}

/// Why a backup cannot be made or read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Not a Pips backup at all.
    NotABackup,
    /// A backup from a newer version of the format.
    NewerVersion(u8),
    /// The backup of another identity, whose public key this is.
    OtherOwner([u8; 32]),
    /// Ours, but the encrypted part does not check out: damaged in transit.
    Damaged,
    /// Larger than the limits.
    TooLarge,
    /// Decrypted, but the contents break the rules.
    Invalid(&'static str),
}

impl Error {
    /// A stable machine-readable code, for the app to translate.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotABackup => "not_a_backup",
            Self::NewerVersion(_) => "newer_version",
            Self::OtherOwner(_) => "other_owner",
            Self::Damaged => "damaged",
            Self::TooLarge => "too_large",
            Self::Invalid(_) => "invalid_backup",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotABackup => f.write_str("not a Pips backup"),
            Self::NewerVersion(v) => write!(f, "backup format version {v} is newer than this app"),
            Self::OtherOwner(_) => f.write_str("the backup of another key"),
            Self::Damaged => f.write_str("the backup is damaged"),
            Self::TooLarge => f.write_str("the backup is larger than the limits"),
            Self::Invalid(what) => write!(f, "invalid backup: {what}"),
        }
    }
}

impl core::error::Error for Error {}

/// Encrypts a backup to `key`'s owner. `nonce` must be random: fresh bytes
/// from the platform's secure generator, never reused.
///
/// # Errors
///
/// [`Error::Invalid`] if the backup breaks the rules [`open`] enforces, so
/// that a file this function writes always opens; [`Error::TooLarge`].
pub fn seal(backup: &Backup, key: &SecretKey, nonce: [u8; 24]) -> Result<Vec<u8>, Error> {
    let body = Zeroizing::new(encode(backup)?);
    // Prove the rules hold before anything is written.
    decode(&body)?;
    let mut out = Vec::with_capacity(HEADER_LEN + body.len() + TAG_LEN);
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&key.public_key().to_bytes());
    out.extend_from_slice(&nonce);
    let sealed = cipher(key)
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: &body, aad: &out })
        .map_err(|_| Error::TooLarge)?;
    out.extend_from_slice(&sealed);
    if out.len() > MAX_FILE {
        return Err(Error::TooLarge);
    }
    Ok(out)
}

/// Decrypts and checks a backup.
///
/// # Errors
///
/// Any [`Error`]; [`Error::OtherOwner`] carries the key it belongs to.
pub fn open(bytes: &[u8], key: &SecretKey) -> Result<Backup, Error> {
    let owner = owner(bytes)?;
    if owner != key.public_key().to_bytes() {
        return Err(Error::OtherOwner(owner));
    }
    let (header, sealed) = bytes.split_at(HEADER_LEN);
    let nonce = XNonce::from_slice(&header[HEADER_LEN - 24..]);
    let body =
        Zeroizing::new(cipher(key).decrypt(nonce, Payload { msg: sealed, aad: header }).map_err(|_| Error::Damaged)?);
    let backup = decode(&body)?;
    // "Mine" must mean signed by this very key.
    for e in backup.cards.iter().filter(|e| e.mine) {
        let card = pips_format::decode(&e.payload).map_err(|_| Error::Invalid("a card does not decode"))?;
        if pips_sign::verify_card(&card).ok() != Some(key.public_key()) {
            return Err(Error::Invalid("an own card signed by another key"));
        }
    }
    Ok(backup)
}

/// The public key a backup belongs to, read from its header without
/// decrypting anything.
///
/// # Errors
///
/// [`Error::NotABackup`], [`Error::NewerVersion`], [`Error::TooLarge`].
pub fn owner(bytes: &[u8]) -> Result<[u8; 32], Error> {
    if bytes.len() > MAX_FILE {
        return Err(Error::TooLarge);
    }
    if bytes.len() < HEADER_LEN + TAG_LEN || bytes[..4] != MAGIC {
        return Err(Error::NotABackup);
    }
    match bytes[4] {
        VERSION => {}
        v if v > VERSION => return Err(Error::NewerVersion(v)),
        _ => return Err(Error::NotABackup),
    }
    let mut k = [0; 32];
    k.copy_from_slice(&bytes[5..37]);
    Ok(k)
}

fn cipher(key: &SecretKey) -> XChaCha20Poly1305 {
    let mut h = Sha256::new();
    h.update(KEY_DOMAIN);
    h.update(key.seed().as_slice());
    let k = Zeroizing::new(<[u8; 32]>::from(h.finalize()));
    XChaCha20Poly1305::new((&*k).into())
}

// ------------------------------------------------------------------ body --

/// The unencrypted body. Exposed for tools and tests; the app uses [`seal`].
///
/// # Errors
///
/// [`Error::Invalid`] for a field that does not fit its length,
/// [`Error::TooLarge`] for too many cards.
pub fn encode(b: &Backup) -> Result<Vec<u8>, Error> {
    if b.cards.len() > MAX_CARDS {
        return Err(Error::TooLarge);
    }
    let mut w = Vec::new();
    w.extend_from_slice(&b.created_ms.to_le_bytes());
    str8(&mut w, &b.profile.nick)?;
    match &b.profile.handle {
        None => w.push(0),
        Some(h) => {
            w.push(h.network as u8);
            str8(&mut w, &h.text)?;
        }
    }
    str8(&mut w, &b.profile.link)?;
    // At most MAX_CARDS, checked above.
    #[allow(clippy::cast_possible_truncation)]
    w.extend_from_slice(&(b.cards.len() as u32).to_le_bytes());
    for e in &b.cards {
        bytes16(&mut w, &e.payload)?;
        let place = match e.place {
            Place::Collection => 0,
            Place::Archive => 1,
            Place::Bin => 2,
        };
        let flags = u8::from(e.mine)
            | place << 1
            | u8::from(e.certificate.is_some()) << 3
            | u8::from(e.sender.is_some()) << 4
            | u8::from(e.binned_ms.is_some()) << 5
            | u8::from(e.issued.is_some()) << 6;
        w.push(flags);
        w.extend_from_slice(&e.received_ms.to_le_bytes());
        if let Some(t) = e.binned_ms {
            w.extend_from_slice(&t.to_le_bytes());
        }
        if let Some(c) = &e.certificate {
            w.extend_from_slice(c);
        }
        if let Some(s) = &e.sender {
            w.extend_from_slice(&s.key);
            str8(&mut w, &s.nick)?;
        }
        bytes16(&mut w, e.note.as_bytes())?;
        if let Some(log) = &e.issued {
            w.extend_from_slice(&log.of.to_le_bytes());
            let n = u16::try_from(log.copies.len()).map_err(|_| Error::Invalid("too many copies"))?;
            w.extend_from_slice(&n.to_le_bytes());
            for c in &log.copies {
                w.extend_from_slice(&c.edition.to_le_bytes());
                w.extend_from_slice(&c.day.to_le_bytes());
                w.extend_from_slice(&c.holder);
                str8(&mut w, &c.nick)?;
            }
        }
    }
    Ok(w)
}

fn str8(w: &mut Vec<u8>, s: &str) -> Result<(), Error> {
    let n = u8::try_from(s.len()).map_err(|_| Error::Invalid("text too long"))?;
    w.push(n);
    w.extend_from_slice(s.as_bytes());
    Ok(())
}

fn bytes16(w: &mut Vec<u8>, b: &[u8]) -> Result<(), Error> {
    let n = u16::try_from(b.len()).map_err(|_| Error::Invalid("field too long"))?;
    w.extend_from_slice(&n.to_le_bytes());
    w.extend_from_slice(b);
    Ok(())
}

struct Reader<'a>(&'a [u8]);

const SHORT: Error = Error::Invalid("unexpected end");

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if self.0.len() < n {
            return Err(SHORT);
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn u64(&mut self) -> Result<u64, Error> {
        let mut a = [0; 8];
        a.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(a))
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let mut a = [0; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }
    fn bytes16(&mut self) -> Result<&'a [u8], Error> {
        let n = u16::from_le_bytes(self.array()?);
        self.take(usize::from(n))
    }
    fn str8(&mut self) -> Result<String, Error> {
        let n = self.u8()?;
        text(self.take(usize::from(n))?)
    }
}

fn text(b: &[u8]) -> Result<String, Error> {
    core::str::from_utf8(b).map(String::from).map_err(|_| Error::Invalid("text is not UTF-8"))
}

/// Parses and checks an unencrypted body.
///
/// # Errors
///
/// [`Error::Invalid`] naming the broken rule; [`Error::TooLarge`].
pub fn decode(body: &[u8]) -> Result<Backup, Error> {
    let mut r = Reader(body);
    let created_ms = r.u64()?;
    let nick = r.str8()?;
    let handle = match r.u8()? {
        0 => None,
        n => {
            let network = Network::from_byte(n).ok_or(Error::Invalid("unknown network"))?;
            Some(Handle { network, text: r.str8()? })
        }
    };
    let link = r.str8()?;
    let profile = Profile { nick, handle, link };
    check_profile(&profile)?;

    let count = usize::try_from(u32::from_le_bytes(r.array()?)).map_err(|_| Error::TooLarge)?;
    // Both bounds before allocating for `count` entries.
    if count > MAX_CARDS {
        return Err(Error::TooLarge);
    }
    if count.saturating_mul(MIN_ENTRY) > r.0.len() {
        return Err(SHORT);
    }
    let mut cards = Vec::with_capacity(count);
    for _ in 0..count {
        cards.push(entry(&mut r)?);
    }
    if !r.0.is_empty() {
        return Err(Error::Invalid("data after the last card"));
    }
    Ok(Backup { created_ms, profile, cards })
}

fn entry(r: &mut Reader<'_>) -> Result<Entry, Error> {
    let payload = r.bytes16()?.to_vec();
    let card = pips_format::decode(&payload).map_err(|_| Error::Invalid("a card does not decode"))?;
    let author = match pips_sign::verify_card(&card) {
        Ok(k) => Some(k),
        Err(pips_sign::Error::Unsigned) => None,
        Err(_) => return Err(Error::Invalid("a card signature does not verify")),
    };
    let flags = r.u8()?;
    if flags & 0b1000_0000 != 0 {
        return Err(Error::Invalid("reserved flag bits"));
    }
    let place = match (flags >> 1) & 0b11 {
        0 => Place::Collection,
        1 => Place::Archive,
        2 => Place::Bin,
        _ => return Err(Error::Invalid("unknown place")),
    };
    let received_ms = r.u64()?;
    let binned_ms = if flags & 1 << 5 != 0 { Some(r.u64()?) } else { None };
    if binned_ms.is_some() != (place == Place::Bin) {
        return Err(Error::Invalid("bin time without the bin, or the reverse"));
    }
    let certificate = if flags & 1 << 3 != 0 {
        let c: [u8; CERTIFICATE_LEN] = r.array()?;
        let cert = Certificate::from_bytes(&c).map_err(|_| Error::Invalid("a certificate does not decode"))?;
        cert.verify(&card).map_err(|_| Error::Invalid("a certificate does not match its card"))?;
        Some(c)
    } else {
        None
    };
    let sender = if flags & 1 << 4 != 0 {
        let key = r.array()?;
        PublicKey::from_bytes(&key).map_err(|_| Error::Invalid("a sender key is not a key"))?;
        let nick = r.str8()?;
        if nick.len() > pips_format::limits::MAX_AUTHOR {
            return Err(Error::Invalid("a sender nick is too long"));
        }
        Some(Sender { key, nick })
    } else {
        None
    };
    let note = r.bytes16()?;
    if note.len() > MAX_NOTE {
        return Err(Error::Invalid("a note is too long"));
    }
    let mine = flags & 1 != 0;
    if mine && author.is_none() {
        return Err(Error::Invalid("an own card without a signature"));
    }
    let issued = if flags & 1 << 6 != 0 {
        if !mine {
            return Err(Error::Invalid("an issue log on someone else's card"));
        }
        Some(issue_log(r)?)
    } else {
        None
    };
    Ok(Entry { payload, mine, place, received_ms, binned_ms, certificate, sender, note: text(note)?, issued })
}

fn issue_log(r: &mut Reader<'_>) -> Result<IssueLog, Error> {
    let of = u16::from_le_bytes(r.array()?);
    let n = usize::from(u16::from_le_bytes(r.array()?));
    // Each copy takes at least 2 + 4 + 32 + 1 bytes: bound before allocating.
    if n.saturating_mul(39) > r.0.len() {
        return Err(SHORT);
    }
    if of != 0 && n > usize::from(of) {
        return Err(Error::Invalid("more copies than the run"));
    }
    let mut copies = Vec::with_capacity(n);
    for i in 0..n {
        let edition = u16::from_le_bytes(r.array()?);
        if usize::from(edition) != i + 1 {
            return Err(Error::Invalid("copies out of order"));
        }
        let day = u32::from_le_bytes(r.array()?);
        let holder = r.array()?;
        PublicKey::from_bytes(&holder).map_err(|_| Error::Invalid("a holder key is not a key"))?;
        let nick = r.str8()?;
        if nick.len() > pips_format::limits::MAX_AUTHOR {
            return Err(Error::Invalid("a holder nick is too long"));
        }
        copies.push(Issued { edition, day, holder, nick });
    }
    Ok(IssueLog { of, copies })
}

/// The profile goes on cards, so it obeys exactly the card rules.
fn check_profile(p: &Profile) -> Result<(), Error> {
    let mut c = Card::new(1, 1);
    c.author.clone_from(&p.nick);
    c.handle.clone_from(&p.handle);
    c.link.clone_from(&p.link);
    pips_format::encode(&c).map(drop).map_err(|_| Error::Invalid("the profile breaks the card rules"))
}
