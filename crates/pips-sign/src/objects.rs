//! Card signatures, edition certificates, recipient codes and envelopes.

use alloc::string::String;
use alloc::vec::Vec;

use pips_format::{canonical_bytes, decode, digest, encode, Card, Signature};

use crate::{Error, PublicKey, SecretKey};

const CARD_DOMAIN: &[u8] = b"pips-v1:card";
const EDITION_DOMAIN: &[u8] = b"pips-v1:edition";
const ENVELOPE_DOMAIN: &[u8] = b"pips-v1:envelope";

/// First byte of a recipient code.
const KIND_RECIPIENT: u8 = b'R';
/// First byte of an envelope.
const KIND_ENVELOPE: u8 = b'E';
const VERSION: u8 = 1;
const MAX_NICK: usize = 24;

fn concat(parts: &[&[u8]]) -> Vec<u8> {
    let mut v = Vec::with_capacity(parts.iter().map(|p| p.len()).sum());
    for p in parts {
        v.extend_from_slice(p);
    }
    v
}

fn check_nick(nick: &str) -> Result<(), Error> {
    let ok = nick.len() <= MAX_NICK && !nick.chars().any(char::is_control) && nick.trim() == nick;
    if ok {
        Ok(())
    } else {
        Err(Error::BadNick)
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if n > self.0.len() {
            return Err(Error::Truncated);
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let mut a = [0; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }
    fn nick(&mut self) -> Result<String, Error> {
        let n = usize::from(self.u8()?);
        if n > MAX_NICK {
            return Err(Error::BadNick);
        }
        let s = core::str::from_utf8(self.take(n)?).map_err(|_| Error::BadNick)?;
        check_nick(s)?;
        Ok(String::from(s))
    }
    fn end(&self) -> Result<(), Error> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(Error::TrailingBytes)
        }
    }
}

fn push_nick(out: &mut Vec<u8>, nick: &str) {
    #[allow(clippy::cast_possible_truncation)] // checked: at most 24
    out.push(nick.len() as u8);
    out.extend_from_slice(nick.as_bytes());
}

// ------------------------------------------------------------------ cards --

/// Signs a card as its author, replacing any previous signature.
///
/// # Errors
///
/// [`Error::Format`] if the card itself is invalid.
pub fn sign_card(card: &mut Card, key: &SecretKey) -> Result<(), Error> {
    let message = concat(&[CARD_DOMAIN, &canonical_bytes(card)?]);
    card.signature = Some(Signature { public_key: key.public_key().to_bytes(), bytes: key.sign(&message) });
    Ok(())
}

/// Checks a card's signature and returns its author.
///
/// # Errors
///
/// [`Error::Unsigned`], [`Error::BadKey`], [`Error::BadSignature`], or
/// [`Error::Format`].
pub fn verify_card(card: &Card) -> Result<PublicKey, Error> {
    let sig = card.signature.ok_or(Error::Unsigned)?;
    let author = PublicKey::from_bytes(&sig.public_key)?;
    let message = concat(&[CARD_DOMAIN, &canonical_bytes(card)?]);
    author.verify(&message, &sig.bytes)?;
    Ok(author)
}

// ----------------------------------------------------------- certificates --

/// Bytes in an encoded [`Certificate`].
pub const CERTIFICATE_LEN: usize = 105;
const CERT_BODY_LEN: usize = CERTIFICATE_LEN - 64;

/// "Copy `edition` of `of` of this card, issued to `recipient`", signed by
/// the card's author.
///
/// The card's digest is not stored: the certificate always travels with its
/// card, and the verifier computes the digest itself, which the signature
/// covers. Nothing stops an author from issuing the same number twice; that
/// is the author's honesty, which no offline system can enforce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Certificate {
    /// 1-based copy number.
    pub edition: u16,
    /// Run size, or 0 for an open run with no upper bound.
    pub of: u16,
    /// Who holds this copy.
    pub recipient: PublicKey,
    /// Day of issue, counted from 1970-01-01 UTC. A day, not a time, so the
    /// certificate does not record when two people met.
    pub issued_day: u32,
    signature: [u8; 64],
}

impl Certificate {
    /// Issues a certificate for `card`, which `author` must have signed.
    ///
    /// # Errors
    ///
    /// [`Error::NotAuthor`] if the card is not signed by `author`,
    /// [`Error::BadEdition`] for edition 0 or above `of`, or anything
    /// [`verify_card`] reports.
    pub fn issue(
        author: &SecretKey,
        card: &Card,
        edition: u16,
        of: u16,
        recipient: PublicKey,
        issued_day: u32,
    ) -> Result<Self, Error> {
        if verify_card(card)? != author.public_key() {
            return Err(Error::NotAuthor);
        }
        check_edition(edition, of)?;
        let mut cert = Self { edition, of, recipient, issued_day, signature: [0; 64] };
        cert.signature = author.sign(&cert.message(card)?);
        Ok(cert)
    }

    /// Checks the certificate against its card: the card must be validly
    /// signed, and the certificate signed by the same author.
    ///
    /// # Errors
    ///
    /// [`Error::BadSignature`] and the errors of [`verify_card`].
    pub fn verify(&self, card: &Card) -> Result<(), Error> {
        let author = verify_card(card)?;
        check_edition(self.edition, self.of)?;
        author.verify(&self.message(card)?, &self.signature)
    }

    /// Whether the certificate names `key` as holder. Meaningful only after
    /// [`verify`](Self::verify).
    #[must_use]
    pub fn is_held_by(&self, key: &PublicKey) -> bool {
        self.recipient == *key
    }

    /// Encodes to [`CERTIFICATE_LEN`] bytes.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; CERTIFICATE_LEN] {
        let mut out = [0; CERTIFICATE_LEN];
        out[..CERT_BODY_LEN].copy_from_slice(&self.body());
        out[CERT_BODY_LEN..].copy_from_slice(&self.signature);
        out
    }

    /// Decodes the structure. Does not verify; call [`verify`](Self::verify).
    ///
    /// # Errors
    ///
    /// Malformed input.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader(bytes);
        let version = r.u8()?;
        if version != VERSION {
            return Err(Error::UnsupportedVersion(version));
        }
        let edition = u16::from_le_bytes(r.array()?);
        let of = u16::from_le_bytes(r.array()?);
        check_edition(edition, of)?;
        let recipient = PublicKey::from_bytes(&r.array()?)?;
        let issued_day = u32::from_le_bytes(r.array()?);
        let signature = r.array()?;
        r.end()?;
        Ok(Self { edition, of, recipient, issued_day, signature })
    }

    fn body(&self) -> [u8; CERT_BODY_LEN] {
        let mut b = [0; CERT_BODY_LEN];
        b[0] = VERSION;
        b[1..3].copy_from_slice(&self.edition.to_le_bytes());
        b[3..5].copy_from_slice(&self.of.to_le_bytes());
        b[5..37].copy_from_slice(&self.recipient.to_bytes());
        b[37..41].copy_from_slice(&self.issued_day.to_le_bytes());
        b
    }

    fn message(&self, card: &Card) -> Result<Vec<u8>, Error> {
        Ok(concat(&[EDITION_DOMAIN, &digest(card)?, &self.body()]))
    }
}

fn check_edition(edition: u16, of: u16) -> Result<(), Error> {
    if edition == 0 || (of != 0 && edition > of) {
        Err(Error::BadEdition)
    } else {
        Ok(())
    }
}

// -------------------------------------------------------- recipient codes --

/// What a person shows to receive a certified copy: their key and nickname.
///
/// It is not signed. Its only use is to name who a certificate is for, and
/// someone who shows a key they do not hold only gives their copy away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientCode {
    /// The key a certificate will name.
    pub key: PublicKey,
    /// Shown to the author when scanning; up to 24 bytes.
    pub nick: String,
}

impl RecipientCode {
    /// Encodes: `'R'`, version, key, nickname.
    ///
    /// # Errors
    ///
    /// [`Error::BadNick`].
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        check_nick(&self.nick)?;
        let mut out = alloc::vec![KIND_RECIPIENT, VERSION];
        out.extend_from_slice(&self.key.to_bytes());
        push_nick(&mut out, &self.nick);
        Ok(out)
    }

    /// Decodes strictly.
    ///
    /// # Errors
    ///
    /// [`Error::WrongKind`] for other codes, and malformed input.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader(bytes);
        if r.u8()? != KIND_RECIPIENT {
            return Err(Error::WrongKind);
        }
        let version = r.u8()?;
        if version != VERSION {
            return Err(Error::UnsupportedVersion(version));
        }
        let key = PublicKey::from_bytes(&r.array()?)?;
        let nick = r.nick()?;
        r.end()?;
        Ok(Self { key, nick })
    }
}

// -------------------------------------------------------------- envelopes --

const FLAG_CERTIFICATE: u8 = 1 << 0;
const FLAG_SENDER: u8 = 1 << 1;
/// Reserved for a code expiry. Not used in version 1, and refused, so that
/// adding it later cannot be silently ignored by old readers.
const KNOWN_FLAGS: u8 = FLAG_CERTIFICATE | FLAG_SENDER;

/// Who handed a card on, when it was not its author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sender {
    /// The sender's nickname, as they chose to present it.
    pub nick: String,
    /// The sender's key, which signed the envelope.
    pub key: PublicKey,
}

/// A verified envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The card.
    pub card: Card,
    /// Its author, if the card is signed. `None` marks an unsigned card,
    /// which the app accepts but labels.
    pub author: Option<PublicKey>,
    /// A verified edition certificate, if one came along. Compare
    /// [`Certificate::is_held_by`] with your own key to see whether the copy
    /// is yours.
    pub certificate: Option<Certificate>,
    /// Who passed it on, if not the author.
    pub sender: Option<Sender>,
}

/// Wraps a card for transfer.
///
/// `sender` is `Some((key, nick))` when the person handing the card on is not
/// its author; it signs the card's identity, the certificate if any, and the
/// nickname, so none of them can be swapped on the way.
///
/// # Errors
///
/// An invalid card, a certificate that does not verify against it, a bad
/// nickname, or a payload too large to carry.
pub fn seal(
    card: &Card,
    certificate: Option<&Certificate>,
    sender: Option<(&SecretKey, &str)>,
) -> Result<Vec<u8>, Error> {
    let payload = encode(card)?;
    let len = u16::try_from(payload.len()).map_err(|_| Error::Format(pips_format::Error::PayloadTooLarge))?;
    if let Some(c) = certificate {
        c.verify(card)?;
    }
    let mut flags = 0;
    if certificate.is_some() {
        flags |= FLAG_CERTIFICATE;
    }
    if sender.is_some() {
        flags |= FLAG_SENDER;
    }
    let mut out = alloc::vec![KIND_ENVELOPE, VERSION, flags];
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&payload);
    let cert_bytes = certificate.map(Certificate::to_bytes);
    if let Some(b) = &cert_bytes {
        out.extend_from_slice(b);
    }
    if let Some((key, nick)) = sender {
        check_nick(nick)?;
        let message = sender_message(card, cert_bytes.as_ref(), nick)?;
        push_nick(&mut out, nick);
        out.extend_from_slice(&key.public_key().to_bytes());
        out.extend_from_slice(&key.sign(&message));
    }
    Ok(out)
}

/// Decodes and verifies an envelope: the card, its author signature if
/// present, the certificate against the card, and the sender's signature.
///
/// # Errors
///
/// The first thing that does not check out. Nothing partially verified is
/// returned.
pub fn open(bytes: &[u8]) -> Result<Opened, Error> {
    let mut r = Reader(bytes);
    if r.u8()? != KIND_ENVELOPE {
        return Err(Error::WrongKind);
    }
    let version = r.u8()?;
    if version != VERSION {
        return Err(Error::UnsupportedVersion(version));
    }
    let flags = r.u8()?;
    if flags & !KNOWN_FLAGS != 0 {
        return Err(Error::UnknownFlag(flags & !KNOWN_FLAGS));
    }
    let len = usize::from(u16::from_le_bytes(r.array()?));
    let card = decode(r.take(len)?)?;

    let author = match verify_card(&card) {
        Ok(a) => Some(a),
        Err(Error::Unsigned) => None,
        Err(e) => return Err(e),
    };

    let mut cert_bytes = None;
    let certificate = if flags & FLAG_CERTIFICATE != 0 {
        let b: [u8; CERTIFICATE_LEN] = r.array()?;
        let c = Certificate::from_bytes(&b)?;
        c.verify(&card)?;
        cert_bytes = Some(b);
        Some(c)
    } else {
        None
    };

    let sender = if flags & FLAG_SENDER != 0 {
        let nick = r.nick()?;
        let key = PublicKey::from_bytes(&r.array()?)?;
        let sig: [u8; 64] = r.array()?;
        key.verify(&sender_message(&card, cert_bytes.as_ref(), &nick)?, &sig)?;
        Some(Sender { nick, key })
    } else {
        None
    };
    r.end()?;
    Ok(Opened { card, author, certificate, sender })
}

fn sender_message(card: &Card, cert: Option<&[u8; CERTIFICATE_LEN]>, nick: &str) -> Result<Vec<u8>, Error> {
    let mut m = concat(&[ENVELOPE_DOMAIN, &digest(card)?]);
    match cert {
        Some(b) => {
            m.push(1);
            m.extend_from_slice(b);
        }
        None => m.push(0),
    }
    push_nick(&mut m, nick);
    Ok(m)
}
