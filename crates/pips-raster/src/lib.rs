//! Turns PNG and Aseprite files into the picture part of a Pips card.
//!
//! ```text
//! bytes ─► decode ─► Raster (RGBA frames) ─► unscale ─► palette ─► Picture
//! ```
//!
//! Each step is its own module and its own public function, so the editor
//! can show "before / after" between them and the command-line tool can
//! stop at any point.
//!
//! # What is refused, and why
//!
//! Pixel art has hard edges, so a picture is taken as it is or refused with
//! the reason — never silently "fixed":
//!
//! - semi-transparent pixels ([`Error::PartialAlpha`]): a card pixel is either
//!   a colour or transparent;
//! - more colours than a card holds, unless a target palette was chosen
//!   ([`Error::TooManyColors`]);
//! - Aseprite features whose result would differ from what the artist sees:
//!   blend modes other than normal, tilemaps, translucent groups.
//!
//! # Hostile input
//!
//! Sizes are checked against [`MAX_FILE`], [`MAX_EDGE`] and [`MAX_PIXELS`]
//! before anything is allocated from them, and inflation is bounded by the
//! size the header promised.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod aseprite;
mod palette;
mod png_in;
mod scale;

use alloc::vec::Vec;
use core::fmt;

use pips_format::{limits, Animation, Card, PaletteRef};

pub use palette::{nearest, to_palette, Target};
pub use scale::{detect_scale, unscale};

/// Largest file accepted, in bytes.
pub const MAX_FILE: usize = 16 * 1024 * 1024;
/// Largest source width or height. 16 × the card limit of 128.
pub const MAX_EDGE: u32 = 2048;
/// Largest source width × height × frames: 32 MiB of RGBA.
pub const MAX_PIXELS: usize = 8 * 1024 * 1024;

/// One RGBA pixel, straight (not premultiplied) alpha.
pub type Rgba = [u8; 4];

/// A decoded picture: one or more same-sized frames of RGBA.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// At least one frame.
    pub frames: Vec<Frame>,
}

/// One frame of a [`Raster`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// `width * height` pixels, row by row.
    pub pixels: Vec<Rgba>,
    /// How long the frame is shown; 0 for a still picture.
    pub delay_ms: u16,
}

/// The picture part of a card, ready for [`Picture::apply_to`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// 1..=128.
    pub width: u8,
    /// 1..=128.
    pub height: u8,
    /// No pixel is transparent, so index 0 is an ordinary colour.
    pub opaque: bool,
    /// Where the colours come from.
    pub palette: PaletteRef,
    /// Present for more than one frame.
    pub animation: Option<Animation>,
    /// One palette index per pixel, frame after frame.
    pub pixels: Vec<u8>,
}

impl Picture {
    /// Puts the picture into a card, leaving its text, frame shade and
    /// extensions alone. Any signature is dropped: it covered the old picture.
    pub fn apply_to(self, card: &mut Card) {
        card.width = self.width;
        card.height = self.height;
        card.opaque = self.opaque;
        card.palette = self.palette;
        card.animation = self.animation;
        card.pixels = self.pixels;
        card.signature = None;
    }
}

/// How [`import`] should treat the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Undo integer upscaling (a 4× export becomes the original pixels).
    pub unscale: bool,
    /// Keep only the first frame of an animation.
    pub first_frame_only: bool,
    /// Which colours to use.
    pub target: Target,
}

impl Default for Options {
    fn default() -> Self {
        Self { unscale: true, first_frame_only: false, target: Target::Auto }
    }
}

/// What [`import`] did, for the "before / after" view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    /// Source size.
    pub source_width: u32,
    /// Source size.
    pub source_height: u32,
    /// Frames in the source.
    pub source_frames: usize,
    /// The upscale factor removed; 1 if none.
    pub scale: u32,
    /// Distinct opaque colours in the source, capped at 257.
    pub colors: usize,
    /// Pixels whose colour changed to reach the palette; 0 for an exact match.
    pub changed_pixels: usize,
}

/// Decodes a PNG or Aseprite file.
///
/// # Errors
///
/// [`Error::UnknownFormat`], [`Error::TooLarge`], [`Error::Malformed`] or
/// [`Error::Unsupported`].
pub fn decode(bytes: &[u8]) -> Result<Raster, Error> {
    if bytes.len() > MAX_FILE {
        return Err(Error::TooLarge);
    }
    if bytes.starts_with(png_in::MAGIC) {
        png_in::decode(bytes)
    } else if aseprite::sniff(bytes) {
        aseprite::decode(bytes)
    } else {
        Err(Error::UnknownFormat)
    }
}

/// Decodes a file and turns it into a card picture.
///
/// # Errors
///
/// Any [`Error`]: see [`decode`] and [`to_palette`].
pub fn import(bytes: &[u8], options: &Options) -> Result<(Picture, Report), Error> {
    let mut raster = decode(bytes)?;
    let (source_width, source_height, source_frames) = (raster.width, raster.height, raster.frames.len());
    if options.first_frame_only {
        raster.frames.truncate(1);
        raster.frames[0].delay_ms = 0;
    }
    let scale = if options.unscale { detect_scale(&raster) } else { 1 };
    let raster = unscale(&raster, scale);
    let (picture, colors, changed_pixels) = to_palette(&raster, &options.target)?;
    Ok((picture, Report { source_width, source_height, source_frames, scale, colors, changed_pixels }))
}

/// Insertion sort. The lists sorted here are short (colours, cels), and
/// std's sorts cost ~7 KB of WebAssembly per element type; this costs a
/// few hundred bytes.
pub(crate) fn small_sort_by_key<T, K: Ord>(v: &mut [T], key: impl Fn(&T) -> K) {
    for i in 1..v.len() {
        let mut j = i;
        while j > 0 && key(&v[j - 1]) > key(&v[j]) {
            v.swap(j - 1, j);
            j -= 1;
        }
    }
}

/// Checks the size a header claims before anything is allocated from it.
pub(crate) fn check_size(width: u32, height: u32, frames: usize) -> Result<(), Error> {
    if width == 0 || height == 0 || frames == 0 {
        return Err(Error::Malformed("empty picture"));
    }
    if width > MAX_EDGE || height > MAX_EDGE {
        return Err(Error::TooLarge);
    }
    let pixels = (width as usize) * (height as usize);
    if pixels.checked_mul(frames).map_or(true, |n| n > MAX_PIXELS) {
        return Err(Error::TooLarge);
    }
    Ok(())
}

/// Turns frame delays into card animation parameters.
pub(crate) fn animation(frames: &[Frame]) -> Result<Option<Animation>, Error> {
    if frames.len() < 2 {
        return Ok(None);
    }
    let n = u8::try_from(frames.len()).ok().filter(|&n| n <= limits::MAX_FRAMES).ok_or(Error::TooManyFrames)?;
    let delay = frames[0].delay_ms;
    if frames.iter().any(|f| f.delay_ms != delay) {
        return Err(Error::UnequalDelays);
    }
    if !limits::DELAY_MS.contains(&delay) {
        return Err(Error::DelayOutOfRange(delay));
    }
    Ok(Some(Animation { frames: n, delay_ms: delay }))
}

/// Why a file cannot become a card picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Neither PNG nor Aseprite.
    UnknownFormat,
    /// The file or the picture exceeds the import limits.
    TooLarge,
    /// The file is damaged or does not follow its format.
    Malformed(&'static str),
    /// A valid file using a feature that would not look the same on a card.
    Unsupported(&'static str),
    /// A pixel that is neither fully opaque nor fully transparent.
    PartialAlpha {
        /// Column.
        x: u32,
        /// Row.
        y: u32,
        /// Frame, from 0.
        frame: usize,
    },
    /// Wider or taller than a card after removing any upscaling.
    TooBig {
        /// Width after unscaling.
        width: u32,
        /// Height after unscaling.
        height: u32,
    },
    /// More colours than a card holds (capped at 257) and no target palette.
    TooManyColors(usize),
    /// More frames than a card holds.
    TooManyFrames,
    /// Frames shown for different times; a card has one delay.
    UnequalDelays,
    /// A frame delay outside what a card allows.
    DelayOutOfRange(u16),
}

impl Error {
    /// A stable machine-readable code, for the app to translate.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownFormat => "unknown_format",
            Self::TooLarge => "too_large",
            Self::Malformed(_) => "malformed",
            Self::Unsupported(_) => "unsupported",
            Self::PartialAlpha { .. } => "partial_alpha",
            Self::TooBig { .. } => "too_big",
            Self::TooManyColors(_) => "too_many_colors",
            Self::TooManyFrames => "too_many_frames",
            Self::UnequalDelays => "unequal_delays",
            Self::DelayOutOfRange(_) => "delay_out_of_range",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFormat => f.write_str("not a PNG or Aseprite file"),
            Self::TooLarge => write!(f, "too large to import (limits: {MAX_EDGE} px a side, {MAX_FILE} bytes)"),
            Self::Malformed(what) => write!(f, "damaged file: {what}"),
            Self::Unsupported(what) => write!(f, "not supported: {what}"),
            Self::PartialAlpha { x, y, frame } => {
                write!(f, "semi-transparent pixel at {x},{y} (frame {}); cards have none", frame + 1)
            }
            Self::TooBig { width, height } => {
                write!(f, "{width}×{height} is larger than a card ({0}×{0} at most)", limits::MAX_EDGE)
            }
            Self::TooManyColors(n) if *n > 256 => f.write_str("more than 256 colours; choose a palette to map them to"),
            Self::TooManyColors(n) => write!(f, "{n} colours; a card holds 32, choose a palette to map them to"),
            Self::TooManyFrames => write!(f, "more than {} frames", limits::MAX_FRAMES),
            Self::UnequalDelays => f.write_str("frames have different durations; a card has one"),
            Self::DelayOutOfRange(ms) => {
                write!(f, "frame duration {ms} ms is outside {:?} ms", limits::DELAY_MS)
            }
        }
    }
}

impl core::error::Error for Error {}
