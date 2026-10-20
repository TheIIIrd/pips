//! From RGBA to palette indices.
//!
//! A card pixel is an index; when the card is not opaque, index 0 means
//! "transparent". So a picture with transparent pixels can use only indices
//! 1 and up for its colours.

use alloc::vec::Vec;

use pips_format::{limits, PaletteRef};
use pips_palette::{Palette, Rgb, BUILTINS, MAX_COLORS};

use crate::{animation, Error, Picture, Raster};

/// Which colours [`to_palette`] uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The smallest built-in palette containing every colour exactly, or
    /// else the picture's own colours carried in the card.
    Auto,
    /// This palette; each colour becomes the nearest one in it.
    Nearest(PaletteRef),
}

/// Colour → palette index, as a sorted array: at most 257 entries, and far
/// less code in WebAssembly than a map.
struct ColorMap(Vec<(u32, u8)>);

impl ColorMap {
    fn new(mut pairs: Vec<(u32, u8)>) -> Self {
        crate::small_sort_by_key(&mut pairs, |&(k, _)| k);
        Self(pairs)
    }
    /// The index for a colour. Only used where every colour of the picture
    /// was entered; 0 for a missing one would be a bug, caught by tests.
    fn get(&self, c: u32) -> u8 {
        let found = self.0.binary_search_by_key(&c, |&(k, _)| k);
        debug_assert!(found.is_ok(), "colour {c:06x} missing from the map");
        found.map_or(0, |i| self.0[i].1)
    }
}

/// Most distinct colours counted before giving up.
const COUNT_CAP: usize = 257;

/// Converts a raster to a card picture.
///
/// Returns the picture, the number of distinct opaque colours in the source
/// (capped at 257) and how many pixels changed colour.
///
/// # Errors
///
/// [`Error::TooBig`], [`Error::PartialAlpha`], [`Error::TooManyColors`] for
/// [`Target::Auto`], the animation errors, or [`Error::Unsupported`] for an
/// unknown built-in palette id.
pub fn to_palette(r: &Raster, target: &Target) -> Result<(Picture, usize, usize), Error> {
    let too_big = Error::TooBig { width: r.width, height: r.height };
    let width = u8::try_from(r.width).ok().filter(|&w| w <= limits::MAX_EDGE).ok_or(too_big)?;
    let height = u8::try_from(r.height).ok().filter(|&h| h <= limits::MAX_EDGE).ok_or(too_big)?;
    let animation = animation(&r.frames)?;

    let (colors, transparent) = survey(r)?;
    let opaque = !transparent;
    let first = usize::from(transparent); // first index a colour may take

    let pixels_in = || r.frames.iter().flat_map(|f| f.pixels.iter());
    let (palette, pixels, changed) = match *target {
        Target::Auto => {
            if colors.len() > MAX_COLORS - first {
                return Err(Error::TooManyColors(colors.len()));
            }
            // At most 32 colours here, all of them in `colors`, so every
            // lookup below finds its colour.
            let (palette, map) = if let Some((id, map)) = exact_builtin(&colors, first) {
                (PaletteRef::Builtin(id), map)
            } else {
                let (own, map) = own_palette(&colors, transparent);
                (PaletteRef::Inline(own), map)
            };
            let pixels = pixels_in().map(|p| if p[3] == 0 { 0 } else { map.get(key(*p)) }).collect();
            (palette, pixels, 0)
        }
        Target::Nearest(p) => {
            // Per pixel, not per surveyed colour: the survey stops counting
            // at 257 and a picture may hold far more. Pixel art repeats
            // colours in runs, so the last answer is remembered.
            let pal = p.resolve().ok_or(Error::Unsupported("unknown built-in palette"))?;
            let mut last: Option<(u32, u8)> = None;
            let mut changed = 0;
            let pixels = pixels_in()
                .map(|px| {
                    if px[3] == 0 {
                        return 0;
                    }
                    let color = key(*px);
                    let index = match last {
                        Some((seen, index)) if seen == color => index,
                        _ => nearest(Rgb::from_u32(color), &pal, first),
                    };
                    last = Some((color, index));
                    changed += usize::from(pal.colors()[usize::from(index)].to_u32() != color);
                    index
                })
                .collect();
            (p, pixels, changed)
        }
    };
    Ok((Picture { width, height, opaque, palette, animation, pixels }, colors.len(), changed))
}

/// The index of the colour in `palette[from..]` nearest to `c`; ties go to
/// the lower index.
///
/// Distance is "redmean", a weighted RGB distance that tracks perception far
/// better than plain RGB and needs no floating point, so every platform
/// picks the same colour.
///
/// # Panics
///
/// If `from` leaves no colour to choose from.
#[must_use]
pub fn nearest(c: Rgb, palette: &Palette, from: usize) -> u8 {
    let d = |p: Rgb| {
        let rm = (i32::from(c.r) + i32::from(p.r)) / 2;
        let (dr, dg, db) =
            (i32::from(c.r) - i32::from(p.r), i32::from(c.g) - i32::from(p.g), i32::from(c.b) - i32::from(p.b));
        (((512 + rm) * dr * dr) >> 8) + 4 * dg * dg + (((767 - rm) * db * db) >> 8)
    };
    let best = (from..palette.len()).min_by_key(|&i| (d(palette.colors()[i]), i)).expect("palette has a usable colour");
    // A palette has at most 32 colours.
    #[allow(clippy::cast_possible_truncation)]
    {
        best as u8
    }
}

const fn key(p: [u8; 4]) -> u32 {
    u32::from_be_bytes([0, p[0], p[1], p[2]])
}

/// Distinct opaque colours in order of first appearance, and whether any
/// pixel is transparent.
fn survey(r: &Raster) -> Result<(Vec<u32>, bool), Error> {
    let mut seen: Vec<u32> = Vec::new(); // sorted
    let mut order = Vec::new();
    let mut transparent = false;
    let w = r.width as usize;
    for (n, f) in r.frames.iter().enumerate() {
        for (i, p) in f.pixels.iter().enumerate() {
            match p[3] {
                0 => transparent = true,
                255 => {
                    if order.len() < COUNT_CAP {
                        if let Err(at) = seen.binary_search(&key(*p)) {
                            seen.insert(at, key(*p));
                            order.push(key(*p));
                        }
                    }
                }
                // Both below the sides, which are u32.
                #[allow(clippy::cast_possible_truncation)]
                _ => return Err(Error::PartialAlpha { x: (i % w) as u32, y: (i / w) as u32, frame: n }),
            }
        }
    }
    Ok((order, transparent))
}

/// The smallest built-in (then lowest id) holding every colour at an index
/// of at least `first`.
fn exact_builtin(colors: &[u32], first: usize) -> Option<(u8, ColorMap)> {
    BUILTINS
        .iter()
        .filter_map(|b| {
            let p = b.palette();
            let pairs: Option<Vec<(u32, u8)>> = colors
                .iter()
                .map(|&c| {
                    let i = p.colors().iter().skip(first).position(|x| x.to_u32() == c)? + first;
                    // Below 32.
                    #[allow(clippy::cast_possible_truncation)]
                    Some((c, i as u8))
                })
                .collect();
            Some((p.len(), b.id, pairs?))
        })
        .min_by_key(|&(len, id, _)| (len, id))
        .map(|(_, id, pairs)| (id, ColorMap::new(pairs)))
}

/// The picture's own colours. With transparency, index 0 holds a colour the
/// picture does not use, so the editor never shows two identical swatches.
fn own_palette(colors: &[u32], transparent: bool) -> (Palette, ColorMap) {
    let mut list: Vec<u32> = Vec::with_capacity(MAX_COLORS);
    if transparent {
        list.push(unused_after(colors));
    }
    list.extend_from_slice(colors);
    while list.len() < pips_palette::MIN_COLORS {
        list.push(unused_after(&list));
    }
    let rgb: Vec<Rgb> = list.iter().map(|&c| Rgb::from_u32(c)).collect();
    let palette = Palette::new(&rgb).expect("2..=32 colours, checked by the caller");
    let first = usize::from(transparent);
    // Below 32.
    #[allow(clippy::cast_possible_truncation)]
    let map = ColorMap::new(colors.iter().enumerate().map(|(i, &c)| (c, (i + first) as u8)).collect());
    (palette, map)
}

/// The darkest grey not in `list`: there are 256 greys and at most 32 colours.
fn unused_after(list: &[u32]) -> u32 {
    (0..=0xFF_FF_FF).step_by(0x01_01_01).find(|g| !list.contains(g)).expect("at most 31 colours used")
}
