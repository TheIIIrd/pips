//! Fitting a picture's own palette: median cut.
//!
//! For pictures with more colours than a card holds — scaled with
//! smoothing, recompressed, painted with soft brushes. A fixed palette
//! changes such a picture a lot; a palette cut from its own colours keeps
//! it close to the original (on sample art, 32 fitted colours were hard to
//! tell from the source, while the nearest built-in palette shifted hues).
//!
//! The colour cloud, weighted by how many pixels use each colour, is split
//! box by box, along the box's widest channel, at the weighted median. The
//! box split next is the one with the largest span × √pixels. Plain
//! span × pixels fits smooth areas a little better (about 5% less average
//! error on sample art) but lets a small accent — an eye, a highlight —
//! melt into its surroundings below 32 colours; the square root keeps it
//! from 16 up, which matters more in pixel art. Each final box gives its
//! pixel-weighted mean. Integers only: every platform fits the same palette.

use alloc::vec::Vec;

use pips_palette::Rgb;

use crate::{heap_sort_by_key, Raster};

/// Up to `n` colours (at least 1) that represent the opaque pixels of `r`,
/// darkest first. `n` is clamped to 1..=32 by the caller.
#[must_use]
pub fn median_cut(raster: &Raster, max: usize) -> Vec<Rgb> {
    // Histogram: every opaque pixel's colour, sorted, then counted.
    let mut px: Vec<u32> = raster
        .frames
        .iter()
        .flat_map(|f| f.pixels.iter())
        .filter(|p| p[3] == 255)
        .map(|p| u32::from_be_bytes([0, p[0], p[1], p[2]]))
        .collect();
    heap_sort_by_key(&mut px, |&c| c);
    let mut hist: Vec<(u32, u32)> = Vec::new();
    for c in px {
        match hist.last_mut() {
            Some((seen, count)) if *seen == c => *count += 1,
            _ => hist.push((c, 1)),
        }
    }
    if hist.is_empty() {
        return Vec::new();
    }

    let mut boxes: Vec<(usize, usize)> = alloc::vec![(0, hist.len())];
    while boxes.len() < max {
        // The box that most deserves a split; none left means every box is
        // a single colour already.
        let best = boxes
            .iter()
            .enumerate()
            .filter(|(_, &(lo, hi))| hi - lo > 1)
            .map(|(at, &(lo, hi))| {
                let (ch, range) = widest(&hist[lo..hi]);
                let pop: u64 = hist[lo..hi].iter().map(|&(_, count)| u64::from(count)).sum();
                (u64::from(range) * pop.isqrt(), at, ch)
            })
            .max_by_key(|&(score, at, _)| (score, core::cmp::Reverse(at)));
        let Some((_, at, ch)) = best else { break };
        let (lo, hi) = boxes[at];
        let part = &mut hist[lo..hi];
        heap_sort_by_key(part, |&(c, _)| (channel(c, ch), c));
        let total: u64 = part.iter().map(|&(_, count)| u64::from(count)).sum();
        // Weighted median, keeping at least one colour on each side.
        let mut acc = 0u64;
        let mut cut = 1;
        for (k, &(_, count)) in part.iter().enumerate() {
            acc += u64::from(count);
            if acc * 2 >= total {
                cut = k + 1;
                break;
            }
        }
        let cut = cut.clamp(1, part.len() - 1);
        boxes[at] = (lo, lo + cut);
        boxes.push((lo + cut, hi));
    }

    let mut out: Vec<Rgb> = boxes.iter().map(|&(lo, hi)| mean(&hist[lo..hi])).collect();
    heap_sort_by_key(&mut out, |c| (luma(*c), c.to_u32()));
    out.dedup();
    out
}

const fn channel(c: u32, ch: u32) -> u32 {
    (c >> (16 - 8 * ch)) & 0xFF
}

/// The channel (0 red, 1 green, 2 blue) with the widest span, and the span.
fn widest(colors: &[(u32, u32)]) -> (u32, u32) {
    (0..3)
        .map(|ch| {
            let (lo, hi) = colors.iter().fold((255, 0), |(lo, hi), &(c, _)| {
                let v = channel(c, ch);
                (lo.min(v), hi.max(v))
            });
            (ch, hi - lo)
        })
        .max_by_key(|&(ch, span)| (span, core::cmp::Reverse(ch)))
        .unwrap_or((0, 0))
}

fn mean(colors: &[(u32, u32)]) -> Rgb {
    let total: u64 = colors.iter().map(|&(_, n)| u64::from(n)).sum();
    let avg = |ch| {
        let sum: u64 = colors.iter().map(|&(c, n)| u64::from(channel(c, ch)) * u64::from(n)).sum();
        // A weighted mean of bytes is a byte.
        u8::try_from((sum + total / 2) / total).unwrap_or(255)
    };
    Rgb::new(avg(0), avg(1), avg(2))
}

fn luma(c: Rgb) -> u32 {
    77 * u32::from(c.r) + 150 * u32::from(c.g) + 29 * u32::from(c.b)
}
