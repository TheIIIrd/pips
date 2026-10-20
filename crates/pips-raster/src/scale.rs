//! Undoing integer upscaling.
//!
//! Artists often export at 4× or 8× so the picture is visible on a screen.
//! Such a file is made of k×k blocks of one colour; the original is one
//! pixel per block.

use crate::{Frame, Raster};

/// The largest k such that every frame is made of k×k single-colour blocks;
/// 1 if there is none.
///
/// One pass: a picture is made of k×k blocks exactly when every colour
/// change, along rows and along columns, happens at a multiple of k. So k is
/// the gcd of the sides and of all positions where the colour changes.
#[must_use]
pub fn detect_scale(r: &Raster) -> u32 {
    let width = r.width as usize;
    let mut k = gcd(r.width, r.height);
    let mut changed = false;
    for f in &r.frames {
        for (i, &p) in f.pixels.iter().enumerate() {
            if k == 1 {
                return 1;
            }
            let (x, y) = (i % width, i / width);
            // Both fit in u32: they are below the sides, which are u32.
            #[allow(clippy::cast_possible_truncation)]
            if x > 0 && p != f.pixels[i - 1] {
                k = gcd(k, x as u32);
                changed = true;
            }
            #[allow(clippy::cast_possible_truncation)]
            if y > 0 && p != f.pixels[i - width] {
                k = gcd(k, y as u32);
                changed = true;
            }
        }
    }
    // No change inside any frame: every frame is one colour, and any k
    // would "fit". Collapsing to one pixel is never what the artist meant.
    if changed {
        k
    } else {
        1
    }
}

/// Keeps the top-left pixel of every k×k block.
///
/// # Panics
///
/// If `k` is 0 or does not divide both sides.
#[must_use]
pub fn unscale(r: &Raster, k: u32) -> Raster {
    assert!(k > 0 && r.width % k == 0 && r.height % k == 0, "scale must divide the size");
    if k == 1 {
        return r.clone();
    }
    let (w, h) = (r.width / k, r.height / k);
    let src_w = r.width as usize;
    let frames = r
        .frames
        .iter()
        .map(|f| Frame {
            pixels: (0..h as usize)
                .flat_map(|y| (0..w as usize).map(move |x| (y, x)))
                .map(|(y, x)| f.pixels[y * k as usize * src_w + x * k as usize])
                .collect(),
            delay_ms: f.delay_ms,
        })
        .collect();
    Raster { width: w, height: h, frames }
}

const fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn raster(w: u32, h: u32, f: impl Fn(u32, u32) -> u8) -> Raster {
        let pixels = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| [f(x, y), 0, 0, 255]).collect();
        Raster { width: w, height: h, frames: vec![Frame { pixels, delay_ms: 0 }] }
    }

    #[test]
    fn finds_the_largest_factor() {
        let art = |x: u32, y: u32| u8::from((x + y) % 3 == 0);
        let up = raster(12, 18, |x, y| art(x / 3, y / 3));
        assert_eq!(detect_scale(&up), 3);
        assert_eq!(unscale(&up, 3), raster(4, 6, art));
    }

    #[test]
    fn one_odd_pixel_stops_it() {
        let mut up = raster(8, 8, |x, y| u8::from((x / 2 + y / 2) % 2 == 0));
        up.frames[0].pixels[9][0] ^= 1;
        assert_eq!(detect_scale(&up), 1);
    }

    #[test]
    fn every_frame_must_agree() {
        let mut up = raster(4, 4, |x, _| u8::from(x / 2 == 0));
        let mut second = up.frames[0].clone();
        second.pixels[1][0] ^= 1; // breaks 2×2 in frame 2 only
        up.frames.push(second);
        assert_eq!(detect_scale(&up), 1);
    }

    #[test]
    fn flat_pictures_stay() {
        assert_eq!(detect_scale(&raster(16, 16, |_, _| 7)), 1);
    }
}
