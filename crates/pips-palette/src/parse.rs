//! Reading and writing palette files.
//!
//! Two formats, chosen because nearly every pixel-art tool reads or writes
//! one of them:
//!
//! - **`.hex`** (Lospec): one `rrggbb` per line, `#` optional, any case.
//! - **`.gpl`** (GIMP, Aseprite, Krita): a `GIMP Palette` header, optional
//!   `Name:`, `Columns:` and `Channels: RGB` lines, `#` comments, then one
//!   `R G B [name]` line per colour in decimal.
//!
//! Anything else is refused with the line it failed on. Input is capped at
//! [`MAX_INPUT_BYTES`] before it is looked at, and parsing stops at the
//! thirty-third colour, so a hostile file costs nothing to reject.

use alloc::string::String;
use core::fmt::{self, Write as _};

use crate::{Palette, Rgb, MAX_COLORS, MIN_COLORS};

/// Largest file [`parse`] reads. A full 32-colour `.gpl` with long colour
/// names is under 2 KiB; the rest is headroom for comments.
pub const MAX_INPUT_BYTES: usize = 64 * 1024;

/// Longest palette name kept, in bytes.
const MAX_NAME_BYTES: usize = 64;

/// A palette read from a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
    /// The `Name:` of a `.gpl` file; `.hex` files have none.
    pub name: Option<String>,
    /// The colours.
    pub palette: Palette,
}

/// Why a palette file was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line the problem is on, or 0 when it concerns the whole file.
    pub line: usize,
    /// What is wrong.
    pub kind: ErrorKind,
}

/// The kinds of [`ParseError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// Larger than [`MAX_INPUT_BYTES`].
    TooLarge,
    /// Not UTF-8 text.
    NotUtf8,
    /// A line that should hold a colour does not.
    BadColor,
    /// A malformed `.gpl` header line.
    BadHeader,
    /// `.gpl` `Channels:` other than `RGB`; transparency belongs to index 0,
    /// not to a channel.
    UnsupportedChannels,
    /// Fewer than [`MIN_COLORS`] colours.
    TooFewColors,
    /// More than [`MAX_COLORS`] colours.
    TooManyColors,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line > 0 {
            write!(f, "line {}: ", self.line)?;
        }
        f.write_str(match self.kind {
            ErrorKind::TooLarge => "file is larger than 64 KiB",
            ErrorKind::NotUtf8 => "not UTF-8 text",
            ErrorKind::BadColor => "not a colour",
            ErrorKind::BadHeader => "malformed header line",
            ErrorKind::UnsupportedChannels => "only RGB palettes are supported",
            ErrorKind::TooFewColors => "a palette needs at least 2 colours",
            ErrorKind::TooManyColors => "a palette may have at most 32 colours",
        })
    }
}

impl core::error::Error for ParseError {}

const fn err(line: usize, kind: ErrorKind) -> ParseError {
    ParseError { line, kind }
}

/// Reads a `.hex` or `.gpl` palette; the format is recognised from the
/// content, not the file name.
///
/// # Errors
///
/// A [`ParseError`] naming the first problem found.
pub fn parse(bytes: &[u8]) -> Result<Imported, ParseError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(err(0, ErrorKind::TooLarge));
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = core::str::from_utf8(bytes).map_err(|_| err(0, ErrorKind::NotUtf8))?;
    let mut lines = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).enumerate();

    let is_gpl = text.lines().next().is_some_and(|l| l.trim_end() == "GIMP Palette");
    if is_gpl {
        lines.next();
        parse_gpl(lines)
    } else {
        parse_hex(lines)
    }
}

/// Colours collected so far; refuses the 33rd as soon as it appears.
struct Collector {
    colors: [Rgb; MAX_COLORS],
    len: usize,
}

impl Collector {
    const fn new() -> Self {
        Self { colors: [Rgb::new(0, 0, 0); MAX_COLORS], len: 0 }
    }

    fn push(&mut self, line: usize, c: Rgb) -> Result<(), ParseError> {
        let slot = self.colors.get_mut(self.len).ok_or(err(line, ErrorKind::TooManyColors))?;
        *slot = c;
        self.len += 1;
        Ok(())
    }

    fn finish(self) -> Result<Palette, ParseError> {
        if self.len < MIN_COLORS {
            return Err(err(0, ErrorKind::TooFewColors));
        }
        Palette::new(&self.colors[..self.len]).map_err(|_| err(0, ErrorKind::TooManyColors))
    }
}

fn parse_hex<'a>(lines: impl Iterator<Item = (usize, &'a str)>) -> Result<Imported, ParseError> {
    let mut out = Collector::new();
    for (i, line) in lines {
        let line_no = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let digits = trimmed.strip_prefix('#').unwrap_or(trimmed);
        let c = hex_color(digits).ok_or(err(line_no, ErrorKind::BadColor))?;
        out.push(line_no, c)?;
    }
    Ok(Imported { name: None, palette: out.finish()? })
}

fn hex_color(s: &str) -> Option<Rgb> {
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    // Six hex digits always fit; `from_str_radix` alone would also accept a sign.
    u32::from_str_radix(s, 16).ok().map(Rgb::from_u32)
}

fn parse_gpl<'a>(lines: impl Iterator<Item = (usize, &'a str)>) -> Result<Imported, ParseError> {
    let mut out = Collector::new();
    let mut name = None;
    for (i, line) in lines {
        let line_no = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if out.len == 0 {
            if let Some(v) = trimmed.strip_prefix("Name:") {
                let v = v.trim();
                if v.len() > MAX_NAME_BYTES || v.chars().any(char::is_control) {
                    return Err(err(line_no, ErrorKind::BadHeader));
                }
                name = Some(String::from(v));
                continue;
            }
            if let Some(v) = trimmed.strip_prefix("Columns:") {
                // Display hint only; checked so garbage is not waved through.
                match decimal(v.trim()) {
                    Some(c) if c <= 256 => continue,
                    _ => return Err(err(line_no, ErrorKind::BadHeader)),
                }
            }
            if let Some(v) = trimmed.strip_prefix("Channels:") {
                if v.trim() == "RGB" {
                    continue;
                }
                return Err(err(line_no, ErrorKind::UnsupportedChannels));
            }
        }
        let mut parts = trimmed.split_ascii_whitespace();
        let mut channel = || parts.next().and_then(decimal).and_then(|v| u8::try_from(v).ok());
        match (channel(), channel(), channel()) {
            // Whatever follows is the colour's name, which Pips does not keep.
            (Some(r), Some(g), Some(b)) => out.push(line_no, Rgb::new(r, g, b))?,
            _ => return Err(err(line_no, ErrorKind::BadColor)),
        }
    }
    Ok(Imported { name, palette: out.finish()? })
}

/// One to three ASCII digits. Rejects signs, spaces and anything longer,
/// which `str::parse` would not all do.
fn decimal(s: &str) -> Option<u16> {
    if s.is_empty() || s.len() > 3 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Writes a palette as `.hex`: lowercase `rrggbb`, one per line, as Lospec
/// publishes them.
#[must_use]
pub fn to_hex(palette: &Palette) -> String {
    let mut s = String::with_capacity(palette.len() * 7);
    for c in palette.colors() {
        // Writing to a String cannot fail.
        let _ = writeln!(s, "{c:x}");
    }
    s
}

/// Writes a palette as `.gpl`. Control characters in `name` become spaces
/// and it is cut to 64 bytes, so the result always parses back.
#[must_use]
pub fn to_gpl(palette: &Palette, name: &str) -> String {
    let mut clean = String::new();
    for ch in name.trim().chars() {
        let ch = if ch.is_control() { ' ' } else { ch };
        if clean.len() + ch.len_utf8() > MAX_NAME_BYTES {
            break;
        }
        clean.push(ch);
    }
    let mut s = String::with_capacity(48 + palette.len() * 20);
    let _ = write!(s, "GIMP Palette\nName: {}\nColumns: 8\n#\n", clean.trim());
    for c in palette.colors() {
        let _ = writeln!(s, "{:>3} {:>3} {:>3}\t{c}", c.r, c.g, c.b);
    }
    s
}
