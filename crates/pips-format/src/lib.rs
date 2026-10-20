//! The Pips card format.
//!
//! A card is a small pixel-art picture with a caption and a business-card
//! back, small enough for one QR code. This crate turns a [`Card`] into bytes
//! and back, and computes its identity. It does not sign (that is
//! `pips-sign`), transport (`pips-qr`) or draw.
//!
//! # Layout
//!
//! All integers little-endian. *(cond)* fields appear only when stated.
//!
//! ```text
//! version     u8      1
//! flags       u8      bit 0 opaque, 1 animated, 2 signed; bits 0-3 critical
//! width       u8      1..=128
//! height      u8      1..=128
//! palette     u8      built-in id (see pips-palette), or 0xFF for inline:
//!   count     u8        (cond) 2..=32
//!   colours   3*count   (cond) RGB
//! frame       u8      frame shade, 0..=15
//! frames      u8      (cond: animated) 2..=64
//! delay_ms    u16     (cond: animated) 20..=5000
//! title       u8+B    0..=32 bytes UTF-8
//! author      u8+B    0..=24 bytes UTF-8
//! handle      u8      network, 0 = none:
//!   text      u8+B      (cond) 1..=48 bytes
//! link        u8+B    0..=64 bytes: https URL without "https://"
//! ext_count   u8      0..=16, sorted by tag, tags unique
//!   tag_head  u8        bit 7 critical, bits 0-6 tag length 1..=24
//!   tag       B         [a-z0-9.-]
//!   data      u16+B     0..=4096 bytes
//! pixels      u16+B   raw deflate of width*height*frames bytes, one per
//!                     pixel, each a valid palette index
//! public_key  32      (cond: signed)
//! signature   64      (cond: signed)
//! ```
//!
//! One byte per pixel, not packed bits: deflate matches whole bytes, and on
//! sample artwork the unpacked form compressed smaller than four-bit
//! packing in 21 cases of 22, and a third smaller than five-bit packing.
//!
//! A `.pips` file is the payload after the magic [`MAGIC`].
//!
//! # Hostile input
//!
//! [`decode`] treats every length as a claim. Sizes are checked before any
//! allocation derived from them, inflation is bounded while it runs, and
//! everything with more than one possible spelling is refused rather than
//! normalised: lowercase-only link hosts, sorted extensions, no stray
//! bytes inside or after the pixel stream.
//!
//! # Identity
//!
//! Deflate is many-to-one, so payload bytes are not an identity. The
//! [canonical form](canonical_bytes) — the same fields with raw pixels and
//! without the signature — is what signatures cover and what
//! [`digest`] hashes.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod codec;
mod validate;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

pub use codec::{canonical_bytes, decode, decode_file, digest, encode, encode_file};
pub use pips_palette::Palette;

/// Format version this crate reads and writes.
pub const VERSION: u8 = 1;
/// Prefix of a `.pips` file. A QR code carries the bare payload.
pub const MAGIC: [u8; 4] = [0x50, 0x50, 0x1A, 0x0A];

/// Normative limits. A conforming decoder enforces every one of them.
pub mod limits {
    /// Largest canvas edge.
    pub const MAX_EDGE: u8 = 128;
    /// Most animation frames.
    pub const MAX_FRAMES: u8 = 64;
    /// Animation delay bounds, in milliseconds. The lower bound keeps
    /// cards from strobing.
    pub const DELAY_MS: core::ops::RangeInclusive<u16> = 20..=5000;
    /// Largest frame-shade id.
    pub const MAX_FRAME_SHADE: u8 = 15;
    /// Most raw pixel bytes (`width * height * frames`).
    pub const MAX_PIXELS: usize = 256 * 1024;
    /// Largest whole payload.
    pub const MAX_PAYLOAD: usize = 64 * 1024;
    /// Text limits, in bytes.
    pub const MAX_TITLE: usize = 32;
    /// See [`MAX_TITLE`].
    pub const MAX_AUTHOR: usize = 24;
    /// See [`MAX_TITLE`].
    pub const MAX_HANDLE: usize = 48;
    /// See [`MAX_TITLE`].
    pub const MAX_LINK: usize = 64;
    /// Extension limits.
    pub const MAX_EXTENSIONS: usize = 16;
    /// See [`MAX_EXTENSIONS`].
    pub const MAX_EXT_TAG: usize = 24;
    /// See [`MAX_EXTENSIONS`].
    pub const MAX_EXT_DATA: usize = 4096;
}

/// Flag bits this version understands.
pub mod flags {
    /// Index 0 is drawn as a colour, not as transparency.
    pub const OPAQUE: u8 = 1 << 0;
    /// More than one frame.
    pub const ANIMATED: u8 = 1 << 1;
    /// A public key and signature follow the pixels.
    pub const SIGNED: u8 = 1 << 2;
    /// Bits a decoder must understand or refuse the card.
    pub const CRITICAL: u8 = 0x0F;
}

/// A card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// Canvas width, 1..=128.
    pub width: u8,
    /// Canvas height, 1..=128.
    pub height: u8,
    /// Draw index 0 as a colour rather than transparent.
    pub opaque: bool,
    /// The colours.
    pub palette: PaletteRef,
    /// Frame shade, 0..=15; 0 is the default frame.
    pub frame: u8,
    /// Present for animated cards.
    pub animation: Option<Animation>,
    /// Up to 32 bytes.
    pub title: String,
    /// Up to 24 bytes.
    pub author: String,
    /// One social handle, optional.
    pub handle: Option<Handle>,
    /// An https address without the scheme, e.g. `kitsu.art/shop`; empty for none.
    pub link: String,
    /// Extension records, sorted by tag.
    pub extensions: Vec<Extension>,
    /// Ancillary flag bits (4-7) this version does not know, kept so a card
    /// passed on is passed on whole.
    pub unknown_flags: u8,
    /// One palette index per pixel, row by row, frame after frame.
    pub pixels: Vec<u8>,
    /// Ed25519 key and signature over the canonical form, if signed.
    pub signature: Option<Signature>,
}

/// Where a card's colours come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteRef {
    /// A built-in palette by id.
    Builtin(u8),
    /// Colours carried in the card.
    Inline(Palette),
}

impl PaletteRef {
    /// The colours, or `None` for an unknown built-in id.
    #[must_use]
    pub fn resolve(&self) -> Option<Palette> {
        match *self {
            Self::Builtin(id) => pips_palette::builtin(id).map(pips_palette::Builtin::palette),
            Self::Inline(p) => Some(p),
        }
    }
}

/// Animation parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Animation {
    /// 2..=64 frames.
    pub frames: u8,
    /// Time each frame is shown, 20..=5000 ms.
    pub delay_ms: u16,
}

/// A social handle on the back of the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handle {
    /// The network.
    pub network: Network,
    /// The handle as the person writes it, e.g. `@kitsu`; no whitespace.
    pub text: String,
}

/// Networks a handle can belong to. The byte values are part of the format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[allow(missing_docs)]
pub enum Network {
    Telegram = 1,
    Instagram = 2,
    X = 3,
    Bluesky = 4,
    Mastodon = 5,
    Vk = 6,
    TikTok = 7,
    Other = 8,
}

impl Network {
    /// The network for a format byte; 0 means "no handle" and is not a network.
    #[must_use]
    pub const fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            1 => Self::Telegram,
            2 => Self::Instagram,
            3 => Self::X,
            4 => Self::Bluesky,
            5 => Self::Mastodon,
            6 => Self::Vk,
            7 => Self::TikTok,
            8 => Self::Other,
            _ => return None,
        })
    }
}

/// An extension record. Tags are vendor-prefixed strings (`acme.foil`);
/// there is no registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extension {
    /// If set, a decoder that does not know the tag must refuse the card.
    pub critical: bool,
    /// 1..=24 bytes of `[a-z0-9.-]`.
    pub tag: String,
    /// Up to 4096 bytes.
    pub data: Vec<u8>,
}

/// An Ed25519 public key and signature. This crate only carries them;
/// `pips-sign` checks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    /// The author's key.
    pub public_key: [u8; 32],
    /// Signature over the canonical form.
    pub bytes: [u8; 64],
}

impl Card {
    /// A blank, unsigned 32×32 card on Pips 16.
    #[must_use]
    pub fn new(width: u8, height: u8) -> Self {
        Self {
            width,
            height,
            opaque: false,
            palette: PaletteRef::Builtin(0),
            frame: 0,
            animation: None,
            title: String::new(),
            author: String::new(),
            handle: None,
            link: String::new(),
            extensions: Vec::new(),
            unknown_flags: 0,
            pixels: alloc::vec![0; usize::from(width) * usize::from(height)],
            signature: None,
        }
    }

    /// Number of frames, 1 for a still card.
    #[must_use]
    pub fn frames(&self) -> u8 {
        self.animation.map_or(1, |a| a.frames)
    }
}

/// Why bytes are not a card, or a card cannot be encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Input ended inside a field.
    Truncated,
    /// Bytes after the last field.
    TrailingBytes,
    /// A `.pips` file without the magic.
    BadMagic,
    /// A version this crate does not read.
    UnsupportedVersion(u8),
    /// A critical flag bit this version does not know.
    UnknownCriticalFlag(u8),
    /// Width or height outside 1..=128.
    BadDimensions,
    /// A built-in palette id that does not exist (here, yet).
    UnknownPalette(u8),
    /// An inline palette with too few or too many colours.
    BadPaletteSize,
    /// Frame shade above 15.
    BadFrame,
    /// Frame count or delay out of range, or animated with one frame.
    BadAnimation,
    /// A text field too long, not UTF-8, or with control characters or
    /// surrounding whitespace.
    BadText(Field),
    /// Unknown handle network, or a handle with whitespace.
    BadHandle,
    /// A link that is not a lowercase https host with an optional path.
    BadLink,
    /// Too many extensions, a malformed tag or oversized data.
    BadExtension,
    /// Extensions not in strictly increasing tag order.
    ExtensionOrder,
    /// A critical extension this version does not know.
    UnknownCriticalExtension,
    /// More raw pixels than [`limits::MAX_PIXELS`].
    TooManyPixels,
    /// Pixel count does not match the canvas.
    PixelCount,
    /// The pixel stream is not one complete raw deflate stream of exactly
    /// the expected size.
    CorruptPixels,
    /// A pixel refers to a colour the palette does not have.
    PixelOutOfPalette,
    /// Payload above [`limits::MAX_PAYLOAD`].
    PayloadTooLarge,
}

/// Text fields, for [`Error::BadText`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Field {
    Title,
    Author,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Truncated => f.write_str("truncated"),
            Self::TrailingBytes => f.write_str("trailing bytes after the card"),
            Self::BadMagic => f.write_str("not a .pips file"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported version {v}"),
            Self::UnknownCriticalFlag(b) => write!(f, "unknown critical flags 0x{b:02x}"),
            Self::BadDimensions => f.write_str("canvas size out of range"),
            Self::UnknownPalette(id) => write!(f, "unknown built-in palette {id}"),
            Self::BadPaletteSize => f.write_str("inline palette needs 2 to 32 colours"),
            Self::BadFrame => f.write_str("frame shade out of range"),
            Self::BadAnimation => f.write_str("bad animation parameters"),
            Self::BadText(Field::Title) => f.write_str("bad title"),
            Self::BadText(Field::Author) => f.write_str("bad author"),
            Self::BadHandle => f.write_str("bad handle"),
            Self::BadLink => f.write_str("bad link"),
            Self::BadExtension => f.write_str("bad extension"),
            Self::ExtensionOrder => f.write_str("extensions out of order"),
            Self::UnknownCriticalExtension => f.write_str("unknown critical extension"),
            Self::TooManyPixels => f.write_str("canvas too large"),
            Self::PixelCount => f.write_str("pixel count does not match canvas"),
            Self::CorruptPixels => f.write_str("corrupt pixel data"),
            Self::PixelOutOfPalette => f.write_str("pixel outside the palette"),
            Self::PayloadTooLarge => f.write_str("payload too large"),
        }
    }
}

impl core::error::Error for Error {}
