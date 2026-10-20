//! Palettes for Pips cards.
//!
//! A [`Palette`] is 2 to 32 colours. Index 0 is the background slot in every
//! palette: a card renders it transparent unless it asks to be opaque.
//!
//! Palettes come from three places:
//!
//! - **Built-ins** ([`builtin`], [`BUILTINS`]), referenced from a card by a
//!   one-byte id. Ids are part of the file format; see [`Builtin`].
//! - **Imports** of Lospec-style `.hex` and GIMP/Aseprite `.gpl` files
//!   ([`parse`]). Parsing is strict and bounded: palette files are shared
//!   between strangers just like cards are.
//! - **Exports** to the same two formats ([`to_hex`], [`to_gpl`]).
//!
//! ```
//! let p = pips_palette::builtin_by_slug("sweetie-16").unwrap().palette();
//! let back = pips_palette::parse(pips_palette::to_hex(&p).as_bytes()).unwrap();
//! assert_eq!(back.palette, p);
//! ```

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod builtin;
mod parse;

use core::fmt;

pub use builtin::BUILTINS;
pub use parse::{parse, to_gpl, to_hex, ErrorKind, Imported, ParseError, MAX_INPUT_BYTES};

/// Fewest colours a palette may have: a background and one colour to draw with.
pub const MIN_COLORS: usize = 2;
/// Most colours a palette may have; pixel indices are at most five bits.
pub const MAX_COLORS: usize = 32;

/// An 8-bit sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb {
    /// A colour from its components.
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// A colour from `0xRRGGBB`; bits above 24 are ignored.
    #[must_use]
    pub const fn from_u32(hex: u32) -> Self {
        let [_, r, g, b] = hex.to_be_bytes();
        Self { r, g, b }
    }

    /// The colour as `0xRRGGBB`.
    #[must_use]
    pub const fn to_u32(self) -> u32 {
        u32::from_be_bytes([0, self.r, self.g, self.b])
    }
}

/// Lowercase `rrggbb`, the form `.hex` files use.
impl fmt::LowerHex for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// `#RRGGBB`.
impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

/// An ordered set of 2 to 32 colours. Copyable and allocation-free.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Palette {
    len: u8,
    // Slots past `len` are always zero, so derived equality and hashing
    // compare only the colours that exist.
    colors: [Rgb; MAX_COLORS],
}

/// A palette with too few or too many colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidLength(pub usize);

impl fmt::Display for InvalidLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "a palette needs {MIN_COLORS} to {MAX_COLORS} colours, got {}", self.0)
    }
}

impl core::error::Error for InvalidLength {}

impl Palette {
    /// A palette of exactly these colours.
    ///
    /// # Errors
    ///
    /// [`InvalidLength`] unless there are [`MIN_COLORS`] to [`MAX_COLORS`].
    pub const fn new(colors: &[Rgb]) -> Result<Self, InvalidLength> {
        if colors.len() < MIN_COLORS || colors.len() > MAX_COLORS {
            return Err(InvalidLength(colors.len()));
        }
        let mut out = [Rgb::new(0, 0, 0); MAX_COLORS];
        let mut i = 0;
        while i < colors.len() {
            out[i] = colors[i];
            i += 1;
        }
        // Checked above: `colors.len() <= 32`.
        #[allow(clippy::cast_possible_truncation)]
        let len = colors.len() as u8;
        Ok(Self { len, colors: out })
    }

    /// The colours, in index order.
    #[must_use]
    pub fn colors(&self) -> &[Rgb] {
        &self.colors[..usize::from(self.len)]
    }

    /// Number of colours, 2 to 32.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    /// Always `false`; present because clippy expects it beside `len`.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// The colour at `index`, if the palette has one.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<Rgb> {
        self.colors().get(index).copied()
    }
}

impl fmt::Debug for Palette {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[")?;
        for (i, c) in self.colors().iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{c}")?;
        }
        f.write_str("]")
    }
}

/// A palette shipped with the format and referenced by a one-byte id.
///
/// Until version 1 of the format is frozen these may still change. After
/// that, an id never changes meaning and a palette never changes its colours
/// — new palettes only ever get new ids. That is what makes a one-byte
/// reference safe without any registry.
#[derive(Debug)]
pub struct Builtin {
    /// The id a card carries.
    pub id: u8,
    /// Stable lowercase name for files and command lines, e.g. `pico-8`.
    pub slug: &'static str,
    /// Display name.
    pub name: &'static str,
    /// One or two sentences on what the palette is for.
    pub summary: &'static str,
    /// Original author, for palettes not designed for Pips.
    pub credit: Option<&'static str>,
    colors: &'static [Rgb],
    /// Names for the colours, in index order, where the palette has them.
    pub color_names: Option<&'static [&'static str]>,
}

impl Builtin {
    /// The colours as a [`Palette`].
    ///
    /// # Panics
    ///
    /// Never: every built-in has a valid length, which the tests check.
    #[must_use]
    pub fn palette(&self) -> Palette {
        Palette::new(self.colors).expect("built-in palettes have valid lengths")
    }
}

/// The built-in palette with this id.
#[must_use]
pub fn builtin(id: u8) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.id == id)
}

/// The built-in palette with this slug, e.g. `"pips-16"`.
#[must_use]
pub fn builtin_by_slug(slug: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.slug == slug)
}
