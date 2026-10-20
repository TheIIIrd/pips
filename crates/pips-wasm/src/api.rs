//! Everything the app asks of the core, in plain data.
//!
//! This layer is ordinary Rust and is tested natively; [`crate::bindings`]
//! only converts its types to and from JavaScript. Types here are what the
//! app sees: palettes as `[r, g, b]` triples, keys as hex, pixels and
//! payloads as byte arrays.
//!
//! The app keeps a received card as the **payload bytes it arrived as**
//! ([`Opened::payload`]), not as [`CardData`]. Passing those bytes on
//! preserves extensions and flags this version does not understand; the
//! [`CardData`] view is for display and for the editor.

use pips_format::{Animation, Card, Handle, Network, PaletteRef};
use pips_palette::{Palette, Rgb};
use pips_qr::carousel::{Decoder, Encoder, Progress};
use pips_sign::{recovery, Certificate, RecipientCode, SecretKey};
use serde::{Deserialize, Serialize};

use crate::b64;
use pips_raster::Target;

// ------------------------------------------------------------------ backup --

/// One stored card, in the shape the app keeps in `IndexedDB`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupCard {
    /// The card as received.
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
    /// Made by the person.
    pub mine: bool,
    /// `collection`, `archive` or `trash`.
    pub state: String,
    /// ms since 1970.
    pub received: u64,
    /// ms since 1970, in the bin only.
    pub trashed_at: Option<u64>,
    /// The edition certificate.
    #[serde(with = "serde_bytes")]
    pub cert: Option<Vec<u8>>,
    /// Who passed it on.
    pub from: Option<FromData>,
    /// The person's note.
    pub note: String,
    /// Filled in by [`Identity::open_backup`] from the certificate; ignored
    /// when sealing.
    #[serde(default, skip_deserializing)]
    pub edition: Option<EditionData>,
    /// Own cards: the copies issued so far.
    #[serde(default)]
    pub editions: Option<IssueLogData>,
}

/// The author's log of copies issued, as the app stores it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssueLogData {
    /// Run size, 0 for open.
    pub of: u16,
    /// In order, numbered from 1.
    pub issued: Vec<IssuedData>,
}

/// One copy issued.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssuedData {
    /// Copy number.
    pub edition: u16,
    /// Holder's nick.
    pub nick: String,
    /// Holder's key, hex.
    pub holder: String,
    /// Days since 1970.
    pub day: u32,
}

/// Who passed a card on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FromData {
    /// Their nick.
    pub nick: String,
    /// Their key, hex.
    pub key: String,
}

/// Copy number, as the app stores it beside the certificate.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EditionData {
    /// Copy number.
    pub edition: u16,
    /// Run size, 0 for open.
    pub of: u16,
    /// Holder key, hex.
    pub holder: String,
    /// Days since 1970.
    pub day: u32,
}

/// The profile.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProfileData {
    /// Author name.
    pub nick: String,
    /// One handle.
    pub handle: Option<HandleData>,
    /// Link without the scheme.
    pub link: String,
}

/// A whole backup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BackupData {
    /// ms since 1970.
    pub created: u64,
    /// The profile.
    pub profile: ProfileData,
    /// Every card.
    pub cards: Vec<BackupCard>,
}

fn store_err(e: pips_store::Error) -> ApiError {
    let mut x = err(e.code(), e);
    if let pips_store::Error::OtherOwner(k) = e {
        if let Ok(k) = pips_sign::PublicKey::from_bytes(&k) {
            // The short form, so the app can say whose backup it is.
            x.message = key_data(&k).short;
        }
    }
    x
}

fn from_hex32(s: &str) -> Option<[u8; 32]> {
    let b = s.as_bytes();
    if b.len() != 64 {
        return None;
    }
    let mut out = [0; 32];
    for (o, p) in out.iter_mut().zip(b.chunks_exact(2)) {
        *o = u8::from_str_radix(core::str::from_utf8(p).ok()?, 16).ok()?;
    }
    Some(out)
}

fn to_store(d: &BackupData) -> Result<pips_store::Backup> {
    use pips_store::{Entry, Place, Profile, Sender};
    let handle = match &d.profile.handle {
        None => None,
        Some(h) => Some(Handle {
            network: NETWORKS
                .iter()
                .find(|(n, _)| *n == h.network)
                .map(|(_, n)| *n)
                .ok_or_else(|| err("invalid_backup", format!("unknown network {:?}", h.network)))?,
            text: h.text.clone(),
        }),
    };
    let cards = d
        .cards
        .iter()
        .map(|c| {
            let place = match c.state.as_str() {
                "collection" => Place::Collection,
                "archive" => Place::Archive,
                "trash" => Place::Bin,
                _ => return Err(err("invalid_backup", "unknown state")),
            };
            let certificate = match &c.cert {
                None => None,
                Some(b) => Some(b.as_slice().try_into().map_err(|_| err("invalid_backup", "certificate length"))?),
            };
            let sender = match &c.from {
                None => None,
                Some(f) => Some(Sender {
                    key: from_hex32(&f.key).ok_or_else(|| err("invalid_backup", "sender key"))?,
                    nick: f.nick.clone(),
                }),
            };
            Ok(Entry {
                payload: c.payload.clone(),
                mine: c.mine,
                place,
                received_ms: c.received,
                // The app keeps a bin time only in the bin; anything else is stale.
                binned_ms: if place == Place::Bin { Some(c.trashed_at.unwrap_or(c.received)) } else { None },
                certificate,
                sender,
                note: c.note.clone(),
                // An empty log is no log: the app may store one before any copy.
                issued: match &c.editions {
                    Some(l) if !l.issued.is_empty() => Some(pips_store::IssueLog {
                        of: l.of,
                        copies: l
                            .issued
                            .iter()
                            .map(|i| {
                                Ok(pips_store::Issued {
                                    edition: i.edition,
                                    day: i.day,
                                    holder: from_hex32(&i.holder).ok_or_else(|| err("invalid_backup", "holder key"))?,
                                    nick: i.nick.clone(),
                                })
                            })
                            .collect::<Result<Vec<_>>>()?,
                    }),
                    _ => None,
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(pips_store::Backup {
        created_ms: d.created,
        profile: Profile { nick: d.profile.nick.clone(), handle, link: d.profile.link.clone() },
        cards,
    })
}

fn from_store(b: &pips_store::Backup) -> BackupData {
    use pips_store::Place;
    let hex = |k: &[u8; 32]| {
        k.iter().fold(String::with_capacity(64), |mut s, x| {
            use core::fmt::Write as _;
            let _ = write!(s, "{x:02x}");
            s
        })
    };
    BackupData {
        created: b.created_ms,
        profile: ProfileData {
            nick: b.profile.nick.clone(),
            handle: b.profile.handle.as_ref().map(|h| HandleData {
                network: NETWORKS.iter().find(|(_, n)| *n == h.network).map_or("other", |(s, _)| s).into(),
                text: h.text.clone(),
            }),
            link: b.profile.link.clone(),
        },
        cards: b
            .cards
            .iter()
            .map(|e| BackupCard {
                payload: e.payload.clone(),
                mine: e.mine,
                state: match e.place {
                    Place::Collection => "collection",
                    Place::Archive => "archive",
                    Place::Bin => "trash",
                }
                .into(),
                received: e.received_ms,
                trashed_at: e.binned_ms,
                cert: e.certificate.map(|c| c.to_vec()),
                from: e.sender.as_ref().map(|s| FromData { nick: s.nick.clone(), key: hex(&s.key) }),
                note: e.note.clone(),
                // open() has verified every certificate against its card.
                edition: e.certificate.and_then(|c| Certificate::from_bytes(&c).ok()).map(|c| EditionData {
                    edition: c.edition,
                    of: c.of,
                    holder: hex(&c.recipient.to_bytes()),
                    day: c.issued_day,
                }),
                editions: e.issued.as_ref().map(|l| IssueLogData {
                    of: l.of,
                    issued: l
                        .copies
                        .iter()
                        .map(|c| IssuedData {
                            edition: c.edition,
                            nick: c.nick.clone(),
                            holder: hex(&c.holder),
                            day: c.day,
                        })
                        .collect(),
                }),
            })
            .collect(),
    }
}

/// Whose backup a file is, `{ hex, short }`, without the key.
///
/// # Errors
///
/// `not_a_backup`, `newer_version`, `too_large`.
pub fn backup_owner(bytes: &[u8]) -> Result<KeyData> {
    let k = pips_store::owner(bytes).map_err(store_err)?;
    let k = pips_sign::PublicKey::from_bytes(&k).map_err(|_| err("not_a_backup", "owner is not a key"))?;
    Ok(key_data(&k))
}

// ------------------------------------------------------------------ import --

/// How [`import_picture`] treats a file.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ImportOptions {
    /// Keep integer upscaling instead of undoing it.
    pub keep_scale: bool,
    /// Keep only the first frame.
    pub first_frame_only: bool,
    /// Map colours to the nearest in this built-in palette.
    pub palette: Option<u8>,
}

/// A picture ready to become a card, and what was done to get it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Imported {
    /// A card with the picture and empty text.
    pub card: CardData,
    /// Source width.
    pub source_width: u32,
    /// Source height.
    pub source_height: u32,
    /// Frames in the source.
    pub source_frames: usize,
    /// Upscale factor removed, 1 if none.
    pub scale: u32,
    /// Distinct colours in the source, capped at 257.
    pub colors: usize,
    /// Pixels that changed colour to fit the palette.
    pub changed_pixels: usize,
}

/// Turns a PNG or Aseprite file into a card picture.
///
/// # Errors
///
/// `unknown_format`, `too_large`, `malformed`, `unsupported`,
/// `partial_alpha` (args: x, y, frame from 1), `too_big` (args: width,
/// height), `too_many_colors` (args: count, 257 meaning "more than 256"),
/// `too_many_frames`, `unequal_delays`, `delay_out_of_range` (args: ms).
pub fn import_picture(bytes: &[u8], o: ImportOptions) -> Result<Imported> {
    let options = pips_raster::Options {
        unscale: !o.keep_scale,
        first_frame_only: o.first_frame_only,
        target: o.palette.map_or(Target::Auto, |id| Target::Nearest(PaletteRef::Builtin(id))),
    };
    let (picture, r) = pips_raster::import(bytes, &options).map_err(|e| {
        use pips_raster::Error as E;
        let to32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let args = match e {
            E::PartialAlpha { x, y, frame } => vec![x, y, to32(frame + 1)],
            E::TooBig { width, height } => vec![width, height],
            E::TooManyColors(n) => vec![to32(n)],
            E::DelayOutOfRange(ms) => vec![u32::from(ms)],
            _ => Vec::new(),
        };
        ApiError { args, ..err(e.code(), e) }
    })?;
    let mut card = Card::new(1, 1);
    picture.apply_to(&mut card);
    Ok(Imported {
        card: from_card(&card),
        source_width: r.source_width,
        source_height: r.source_height,
        source_frames: r.source_frames,
        scale: r.scale,
        colors: r.colors,
        changed_pixels: r.changed_pixels,
    })
}

// ------------------------------------------------------------------ errors --

/// An error with a stable machine code (for the app's logic and
/// translations) and an English message (for logs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiError {
    /// Stable identifier, e.g. `"bad_signature"`.
    pub code: &'static str,
    /// Human-readable detail.
    pub message: String,
    /// Numbers a translated message needs, e.g. the place of a bad pixel.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<u32>,
}

/// Result of every fallible call.
pub type Result<T> = core::result::Result<T, ApiError>;

#[allow(clippy::needless_pass_by_value)] // takes errors by value at call sites
fn err(code: &'static str, message: impl ToString) -> ApiError {
    ApiError { code, message: message.to_string(), args: Vec::new() }
}

impl ApiError {
    /// An error with a code and an English message.
    #[must_use]
    pub fn new(code: &'static str, message: impl ToString) -> Self {
        err(code, message)
    }
}

impl From<pips_format::Error> for ApiError {
    fn from(e: pips_format::Error) -> Self {
        err("invalid_card", e)
    }
}

impl From<pips_sign::Error> for ApiError {
    fn from(e: pips_sign::Error) -> Self {
        use pips_sign::Error as E;
        let code = match e {
            E::Format(f) => return f.into(),
            E::Unsigned => "unsigned",
            E::BadSignature => "bad_signature",
            E::BadKey => "bad_key",
            E::NotAuthor => "not_author",
            E::BadEdition => "bad_edition",
            E::BadNick => "bad_nick",
            _ => "invalid_code",
        };
        err(code, e)
    }
}

// ---------------------------------------------------------------- palettes --

/// A built-in palette.
#[derive(Debug, Clone, Serialize)]
pub struct PaletteInfo {
    /// Id stored in cards.
    pub id: u8,
    /// Stable name, e.g. `"pico-8"`.
    pub slug: &'static str,
    /// Display name.
    pub name: &'static str,
    /// What it is for.
    pub summary: &'static str,
    /// Original author, if not designed for Pips.
    pub credit: Option<&'static str>,
    /// The colours.
    pub colors: Vec<[u8; 3]>,
}

/// Every built-in palette, in id order.
#[must_use]
pub fn palettes() -> Vec<PaletteInfo> {
    pips_palette::BUILTINS
        .iter()
        .map(|b| PaletteInfo {
            id: b.id,
            slug: b.slug,
            name: b.name,
            summary: b.summary,
            credit: b.credit,
            colors: rgb_list(&b.palette()),
        })
        .collect()
}

/// A palette read from a `.hex` or `.gpl` file.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportedPalette {
    /// The file's `Name:`, if it had one.
    pub name: Option<String>,
    /// The colours.
    pub colors: Vec<[u8; 3]>,
}

/// Reads a palette file.
///
/// # Errors
///
/// `bad_palette`, with the line in the message.
pub fn parse_palette(bytes: &[u8]) -> Result<ImportedPalette> {
    let p = pips_palette::parse(bytes).map_err(|e| err("bad_palette", e))?;
    Ok(ImportedPalette { name: p.name, colors: rgb_list(&p.palette) })
}

fn rgb_list(p: &Palette) -> Vec<[u8; 3]> {
    p.colors().iter().map(|c| [c.r, c.g, c.b]).collect()
}

// ------------------------------------------------------------------- cards --

/// Where a card's colours come from: `{"builtin": 0}` or `{"inline": [[r,g,b], ...]}`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaletteData {
    /// A built-in palette id.
    Builtin(u8),
    /// 2 to 32 colours carried in the card.
    Inline(Vec<[u8; 3]>),
}

/// A social handle: network is one of `telegram`, `instagram`, `x`,
/// `bluesky`, `mastodon`, `vk`, `tiktok`, `other`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HandleData {
    /// The network.
    pub network: String,
    /// The handle, e.g. `@kitsu`.
    pub text: String,
}

/// A card as the editor and the viewer see it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CardData {
    /// 1..=128.
    pub width: u8,
    /// 1..=128.
    pub height: u8,
    /// Draw index 0 as a colour.
    pub opaque: bool,
    /// The colours.
    pub palette: PaletteData,
    /// Frame shade, 0..=15.
    pub frame: u8,
    /// 1 for a still card, 2..=64 for animation.
    pub frames: u8,
    /// Per-frame time for animation; ignored for still cards.
    pub delay_ms: u16,
    /// Up to 32 bytes.
    pub title: String,
    /// Up to 24 bytes.
    pub author: String,
    /// Optional handle.
    pub handle: Option<HandleData>,
    /// https address without the scheme, or empty.
    pub link: String,
    /// One palette index per pixel, row by row, frame after frame.
    #[serde(with = "serde_bytes")]
    pub pixels: Vec<u8>,
}

const NETWORKS: [(&str, Network); 8] = [
    ("telegram", Network::Telegram),
    ("instagram", Network::Instagram),
    ("x", Network::X),
    ("bluesky", Network::Bluesky),
    ("mastodon", Network::Mastodon),
    ("vk", Network::Vk),
    ("tiktok", Network::TikTok),
    ("other", Network::Other),
];

fn to_card(d: &CardData) -> Result<Card> {
    let palette = match &d.palette {
        PaletteData::Builtin(id) => PaletteRef::Builtin(*id),
        PaletteData::Inline(cs) => {
            let rgb: Vec<Rgb> = cs.iter().map(|c| Rgb::new(c[0], c[1], c[2])).collect();
            PaletteRef::Inline(Palette::new(&rgb).map_err(|e| err("bad_palette", e))?)
        }
    };
    let handle = match &d.handle {
        None => None,
        Some(h) => {
            let network = NETWORKS
                .iter()
                .find(|(n, _)| *n == h.network)
                .map(|(_, n)| *n)
                .ok_or_else(|| err("invalid_card", format!("unknown network {:?}", h.network)))?;
            Some(Handle { network, text: h.text.clone() })
        }
    };
    let mut c = Card::new(d.width, d.height);
    c.opaque = d.opaque;
    c.palette = palette;
    c.frame = d.frame;
    c.animation = (d.frames > 1).then_some(Animation { frames: d.frames, delay_ms: d.delay_ms });
    c.title.clone_from(&d.title);
    c.author.clone_from(&d.author);
    c.handle = handle;
    c.link.clone_from(&d.link);
    c.pixels.clone_from(&d.pixels);
    Ok(c)
}

fn from_card(c: &Card) -> CardData {
    CardData {
        width: c.width,
        height: c.height,
        opaque: c.opaque,
        palette: match c.palette {
            PaletteRef::Builtin(id) => PaletteData::Builtin(id),
            PaletteRef::Inline(p) => PaletteData::Inline(rgb_list(&p)),
        },
        frame: c.frame,
        frames: c.frames(),
        delay_ms: c.animation.map_or(0, |a| a.delay_ms),
        title: c.title.clone(),
        author: c.author.clone(),
        handle: c.handle.as_ref().map(|h| HandleData {
            network: NETWORKS.iter().find(|(_, n)| *n == h.network).map_or("other", |(s, _)| s).into(),
            text: h.text.clone(),
        }),
        link: c.link.clone(),
        pixels: c.pixels.clone(),
    }
}

/// Bytes the card will take when shared by its author: signed payload in
/// an envelope. What the editor's meter shows against [`pips_qr::SINGLE_MAX`].
///
/// # Errors
///
/// `invalid_card` naming the first broken rule.
pub fn share_size(card: &CardData) -> Result<usize> {
    const SIGNATURE: usize = 96;
    const ENVELOPE: usize = 5;
    Ok(pips_format::encode(&to_card(card)?)?.len() + SIGNATURE + ENVELOPE)
}

/// One frame as RGBA, ready for `ImageData`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Rgba {
    /// Pixels per row.
    pub width: u8,
    /// Rows.
    pub height: u8,
    /// `width * height * 4` bytes; index 0 is transparent unless opaque.
    #[serde(with = "serde_bytes")]
    pub rgba: Vec<u8>,
}

/// Renders frame `frame` of a card.
///
/// # Errors
///
/// `invalid_card` for an invalid card or a frame past the last.
pub fn render(card: &CardData, frame: u8) -> Result<Rgba> {
    let c = to_card(card)?;
    pips_format::encode(&c)?; // all rules, including pixel indices
    if frame >= c.frames() {
        return Err(err("invalid_card", "no such frame"));
    }
    let palette = c.palette.resolve().ok_or_else(|| err("invalid_card", "unknown palette"))?;
    let n = usize::from(c.width) * usize::from(c.height);
    let start = n * usize::from(frame);
    let mut rgba = vec![0u8; n * 4];
    for (px, out) in c.pixels[start..start + n].iter().zip(rgba.chunks_exact_mut(4)) {
        if *px == 0 && !c.opaque {
            continue;
        }
        let col = palette.get(usize::from(*px)).unwrap_or_default();
        out.copy_from_slice(&[col.r, col.g, col.b, 255]);
    }
    Ok(Rgba { width: c.width, height: c.height, rgba })
}

// ---------------------------------------------------------------- identity --

/// A public key as the app shows it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KeyData {
    /// All 32 bytes in hex: the identity to compare and store.
    pub hex: String,
    /// `4f·a1·c8·0e`: for people, not for comparison.
    pub short: String,
}

fn key_data(k: &pips_sign::PublicKey) -> KeyData {
    KeyData { hex: k.to_string(), short: k.short().to_string() }
}

/// The user's own key. The app persists only [`Identity::seed`].
pub struct Identity(SecretKey);

impl Identity {
    /// From the stored seed, or from fresh `crypto.getRandomValues` output.
    ///
    /// # Errors
    ///
    /// `bad_seed` unless exactly 32 bytes.
    pub fn from_seed(seed: &[u8]) -> Result<Self> {
        let seed: &[u8; 32] = seed.try_into().map_err(|_| err("bad_seed", "a seed is 32 bytes"))?;
        Ok(Self(SecretKey::from_seed(seed)))
    }

    /// From the 24 recovery words.
    ///
    /// # Errors
    ///
    /// `bad_words`, saying which word or what is wrong.
    pub fn from_words(words: &str) -> Result<Self> {
        recovery::from_words(words).map(Self).map_err(|e| err("bad_words", e))
    }

    /// From a scanned paper key.
    ///
    /// # Errors
    ///
    /// `bad_paper_key`.
    pub fn from_paper(text: &str) -> Result<Self> {
        recovery::from_paper(text).map(Self).map_err(|e| err("bad_paper_key", e))
    }

    /// The seed to store.
    #[must_use]
    pub fn seed(&self) -> Vec<u8> {
        self.0.seed().to_vec()
    }

    /// The 24 recovery words.
    #[must_use]
    pub fn words(&self) -> String {
        (*recovery::to_words(&self.0)).clone()
    }

    /// The paper-key text, to draw as a QR code for printing.
    #[must_use]
    pub fn paper(&self) -> String {
        (*recovery::to_paper(&self.0)).clone()
    }

    /// The public key.
    #[must_use]
    pub fn public_key(&self) -> KeyData {
        key_data(&self.0.public_key())
    }

    /// Signs a card from the editor; returns its payload bytes.
    ///
    /// # Errors
    ///
    /// `invalid_card`.
    pub fn sign_card(&self, card: &CardData) -> Result<Vec<u8>> {
        let mut c = to_card(card)?;
        pips_sign::sign_card(&mut c, &self.0)?;
        Ok(pips_format::encode(&c)?)
    }

    /// The Base45 text of this person's recipient code, for a QR code.
    ///
    /// # Errors
    ///
    /// `bad_nick`.
    pub fn recipient_code(&self, nick: &str) -> Result<String> {
        let code = RecipientCode { key: self.0.public_key(), nick: nick.into() }.to_bytes()?;
        Ok(pips_base45::encode(&code))
    }

    /// Issues copy `edition` of `of` (0: open run) of an own card to the
    /// person whose recipient code was scanned. `day` is days since
    /// 1970-01-01 UTC.
    ///
    /// # Errors
    ///
    /// `not_author`, `bad_edition`, `invalid_code`, `invalid_card`.
    pub fn issue(&self, payload: &[u8], edition: u16, of: u16, recipient: &[u8], day: u32) -> Result<Vec<u8>> {
        let card = pips_format::decode(payload)?;
        let to = RecipientCode::from_bytes(recipient)?;
        Ok(Certificate::issue(&self.0, &card, edition, of, to.key, day)?.to_bytes().to_vec())
    }

    /// An envelope for an own card, optionally with a certificate.
    ///
    /// # Errors
    ///
    /// `invalid_card`, or a certificate that does not match the card.
    pub fn seal_as_author(&self, payload: &[u8], certificate: Option<&[u8]>) -> Result<Vec<u8>> {
        let card = pips_format::decode(payload)?;
        let cert = certificate.map(Certificate::from_bytes).transpose()?;
        Ok(pips_sign::seal(&card, cert.as_ref(), None)?)
    }

    /// An envelope for passing on someone else's card, signed as `nick`. A
    /// certified copy is never passed on with its certificate: what goes on
    /// is a copy.
    ///
    /// # Errors
    ///
    /// `invalid_card`, `bad_nick`.
    pub fn seal_as_sender(&self, payload: &[u8], nick: &str) -> Result<Vec<u8>> {
        let card = pips_format::decode(payload)?;
        Ok(pips_sign::seal(&card, None, Some((&self.0, nick)))?)
    }

    /// Encrypts a backup to this identity. `nonce`: 24 bytes from
    /// `crypto.getRandomValues`.
    ///
    /// # Errors
    ///
    /// `invalid_backup` naming the broken rule, `too_large`, `bad_nonce`.
    pub fn seal_backup(&self, data: &BackupData, nonce: &[u8]) -> Result<Vec<u8>> {
        let nonce: [u8; 24] = nonce.try_into().map_err(|_| err("bad_nonce", "a nonce is 24 bytes"))?;
        pips_store::seal(&to_store(data)?, &self.0, nonce).map_err(store_err)
    }

    /// Decrypts and checks a backup of this identity.
    ///
    /// # Errors
    ///
    /// `not_a_backup`, `newer_version`, `other_owner` (message: the owner's
    /// short key), `damaged`, `invalid_backup`, `too_large`.
    pub fn open_backup(&self, bytes: &[u8]) -> Result<BackupData> {
        pips_store::open(bytes, &self.0).map(|b| from_store(&b)).map_err(store_err)
    }
}

// ---------------------------------------------------------------- receiving --

/// A verified envelope.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Opened {
    /// For display.
    pub card: CardData,
    /// The card exactly as received: what the app stores and passes on.
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
    /// Identity of the card, for de-duplication: hex SHA-256 of the
    /// canonical form.
    pub digest: String,
    /// The author, or `None` for an unsigned card (accept, but label it).
    pub author: Option<KeyData>,
    /// A verified certificate, if one came with it.
    pub certificate: Option<CertificateData>,
    /// Who passed it on, if not the author.
    pub sender: Option<SenderData>,
    /// Extension records this version does not understand (kept, not shown).
    pub unknown_extensions: usize,
}

/// A verified edition certificate.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CertificateData {
    /// Copy number.
    pub edition: u16,
    /// Run size, 0 for an open run.
    pub of: u16,
    /// Whose copy it is: compare `holder.hex` with your own key.
    pub holder: KeyData,
    /// Days since 1970-01-01 UTC.
    pub issued_day: u32,
    /// The certificate itself, to store beside the card.
    #[serde(with = "serde_bytes")]
    pub bytes: Vec<u8>,
}

/// A verified sender.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SenderData {
    /// As they presented themselves.
    pub nick: String,
    /// Their key.
    pub key: KeyData,
}

/// Verifies and unpacks an envelope.
///
/// # Errors
///
/// `bad_signature`, `invalid_card`, `invalid_code` and the like; nothing
/// partly verified is ever returned.
pub fn open_envelope(bytes: &[u8]) -> Result<Opened> {
    let o = pips_sign::open(bytes)?;
    // Layout: 'E', version, flags, u16 length, payload. `open` has just
    // checked all of it.
    let len = usize::from(u16::from_le_bytes([bytes[3], bytes[4]]));
    let payload = bytes[5..5 + len].to_vec();
    let digest = hex_digest(&o.card)?;
    Ok(Opened {
        card: from_card(&o.card),
        unknown_extensions: o.card.extensions.len(),
        payload,
        digest,
        author: o.author.as_ref().map(key_data),
        certificate: o.certificate.map(|c| CertificateData {
            edition: c.edition,
            of: c.of,
            holder: key_data(&c.recipient),
            issued_day: c.issued_day,
            bytes: c.to_bytes().to_vec(),
        }),
        sender: o.sender.map(|s| SenderData { nick: s.nick, key: key_data(&s.key) }),
    })
}

/// A stored card, checked and unpacked.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Inspected {
    /// For display.
    pub card: CardData,
    /// Identity, hex SHA-256 of the canonical form.
    pub digest: String,
    /// The author, or `None` for an unsigned card.
    pub author: Option<KeyData>,
}

/// Checks and unpacks card payload bytes, as stored by the app: format
/// rules, and the signature if there is one.
///
/// # Errors
///
/// `invalid_card`, `bad_signature`, `bad_key`.
pub fn inspect_card(payload: &[u8]) -> Result<Inspected> {
    let card = pips_format::decode(payload)?;
    let author = match pips_sign::verify_card(&card) {
        Ok(k) => Some(key_data(&k)),
        Err(pips_sign::Error::Unsigned) => None,
        Err(e) => return Err(e.into()),
    };
    Ok(Inspected { card: from_card(&card), digest: hex_digest(&card)?, author })
}

fn hex_digest(card: &Card) -> Result<String> {
    Ok(pips_format::digest(card)?.iter().fold(String::with_capacity(64), |mut s, b| {
        use core::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    }))
}

/// What a scanned code is. Nothing is verified yet except its shape.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scanned {
    /// A card envelope: pass the bytes to [`open_envelope`].
    Envelope {
        /// The envelope.
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    /// A carousel frame: feed it to a [`Joiner`].
    Frame {
        /// The frame.
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    /// Someone's recipient code.
    Recipient {
        /// Their key.
        key: KeyData,
        /// Their nickname.
        nick: String,
        /// The code itself, for [`Identity::issue`].
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
    /// A secret paper key. The app must ask before using it — and should
    /// warn that whoever showed it has shown their identity.
    PaperKey,
    /// Not a Pips code.
    Unknown,
}

/// Recognises scanned text: Base45 codes, Pips links (from printed cards)
/// and paper keys.
#[must_use]
pub fn classify(text: &str) -> Scanned {
    let text = text.trim();
    if text.starts_with(recovery::PAPER_PREFIX) {
        return Scanned::PaperKey;
    }
    let bytes = if text.starts_with("https://") {
        match parse_link(text) {
            Ok(b) => b,
            Err(_) => return Scanned::Unknown,
        }
    } else {
        match pips_base45::decode(text) {
            Ok(b) => b,
            Err(_) => return Scanned::Unknown,
        }
    };
    match bytes.first() {
        Some(b'E') => Scanned::Envelope { bytes },
        Some(&pips_qr::carousel::KIND) => Scanned::Frame { bytes },
        Some(b'R') => match RecipientCode::from_bytes(&bytes) {
            Ok(r) => Scanned::Recipient { key: key_data(&r.key), nick: r.nick, bytes },
            Err(_) => Scanned::Unknown,
        },
        _ => Scanned::Unknown,
    }
}

// ------------------------------------------------------------------- links --

/// Longest link accepted, in characters.
pub const MAX_LINK: usize = 16 * 1024;

/// A link that opens the app on this envelope: `base#<base64url>`. The part
/// after `#` never reaches the server.
///
/// # Errors
///
/// `bad_link` unless `base` is an https URL without a fragment and the
/// result fits [`MAX_LINK`].
pub fn card_link(base: &str, envelope: &[u8]) -> Result<String> {
    if !base.starts_with("https://") || base.contains('#') || envelope.first() != Some(&b'E') {
        return Err(err("bad_link", "base must be https without a fragment; payload must be an envelope"));
    }
    let link = format!("{base}#{}", b64::encode(envelope));
    if link.len() > MAX_LINK {
        return Err(err("bad_link", "card too large for a link"));
    }
    Ok(link)
}

/// The envelope inside a card link. Verify it with [`open_envelope`].
///
/// # Errors
///
/// `bad_link`.
pub fn parse_link(url: &str) -> Result<Vec<u8>> {
    if url.len() > MAX_LINK {
        return Err(err("bad_link", "link too long"));
    }
    let fragment = url.split_once('#').map(|(_, f)| f).ok_or_else(|| err("bad_link", "no card in link"))?;
    let bytes = b64::decode(fragment).ok_or_else(|| err("bad_link", "damaged link"))?;
    if bytes.first() != Some(&b'E') {
        return Err(err("bad_link", "no card in link"));
    }
    Ok(bytes)
}

// ---------------------------------------------------------------------- QR --

/// How to show bytes.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QrPlan {
    /// One code with this text.
    Single {
        /// Base45 text.
        text: String,
    },
    /// Too big for one: make a [`Carousel`].
    Carousel {
        /// Fewest frames that carry it.
        fragments: usize,
    },
}

/// One code or a carousel?
///
/// # Errors
///
/// `too_large`.
pub fn qr_plan(bytes: &[u8]) -> Result<QrPlan> {
    match pips_qr::plan(bytes).map_err(|e| err("too_large", e))? {
        pips_qr::Plan::Single(text) => Ok(QrPlan::Single { text }),
        pips_qr::Plan::Carousel(e) => Ok(QrPlan::Carousel { fragments: e.fragments() }),
    }
}

/// A QR symbol: `modules[y * size + x]` is 1 for dark.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MatrixData {
    /// Modules per side.
    pub size: usize,
    /// One byte per module, 0 or 1.
    #[serde(with = "serde_bytes")]
    pub modules: Vec<u8>,
}

/// Draws text as a QR symbol, for a canvas.
///
/// # Errors
///
/// `too_large`.
pub fn qr_matrix(text: &str) -> Result<MatrixData> {
    let m = pips_qr::matrix::Matrix::encode(text).map_err(|e| err("too_large", e))?;
    let size = m.size();
    let modules = (0..size * size).map(|i| u8::from(m.get(i % size, i / size))).collect();
    Ok(MatrixData { size, modules })
}

/// Draws text as an SVG QR code, for printing.
///
/// # Errors
///
/// `too_large`.
pub fn qr_svg(text: &str) -> Result<String> {
    Ok(pips_qr::matrix::Matrix::encode(text).map_err(|e| err("too_large", e))?.to_svg(4))
}

/// Shows a large message as a loop of codes.
pub struct Carousel(Encoder);

impl Carousel {
    /// A carousel for `bytes`.
    ///
    /// # Errors
    ///
    /// `too_large`.
    pub fn new(bytes: &[u8]) -> Result<Self> {
        Encoder::new(bytes, pips_qr::DEFAULT_FRAGMENT).map(Self).map_err(|e| err("too_large", e))
    }

    /// Fewest frames that carry the message.
    #[must_use]
    pub fn fragments(&self) -> usize {
        self.0.fragments()
    }

    /// Base45 text of frame `seq`; loop `seq` upward forever.
    #[must_use]
    pub fn frame_text(&self, seq: u32) -> String {
        pips_base45::encode(&self.0.frame(seq))
    }
}

/// Where joining stands.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JoinProgress {
    /// Keep scanning.
    Partial {
        /// Useful frames so far.
        have: usize,
        /// Frames needed.
        need: usize,
    },
    /// The message; usually an envelope for [`open_envelope`].
    Done {
        /// The joined bytes.
        #[serde(with = "serde_bytes")]
        bytes: Vec<u8>,
    },
}

/// Joins carousel frames scanned in any order.
#[derive(Default)]
pub struct Joiner(Decoder);

impl Joiner {
    /// An empty joiner.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one frame (as returned in [`Scanned::Frame`]).
    ///
    /// # Errors
    ///
    /// `bad_frame`; for a frame from another carousel the app may start a
    /// new joiner.
    pub fn push(&mut self, frame: &[u8]) -> Result<JoinProgress> {
        match self.0.push(frame).map_err(|e| err("bad_frame", e))? {
            Progress::Partial { have, need } => Ok(JoinProgress::Partial { have, need }),
            Progress::Done(bytes) => Ok(JoinProgress::Done { bytes }),
        }
    }
}
