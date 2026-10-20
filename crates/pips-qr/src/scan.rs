//! Reading QR symbols from a camera frame (feature `scan`), through `rqrr`.
//!
//! For browsers without `BarcodeDetector`. Only the text is returned: what it
//! means is for `pips-wasm`'s classifier to decide, like any scanned text.

use alloc::string::String;
use alloc::vec::Vec;

/// Largest frame accepted, in pixels: a 1920×1080 camera frame and more.
pub const MAX_PIXELS: usize = 2560 * 1440;

/// Every QR symbol found in an RGBA frame, as text. Symbols that fail their
/// error correction are skipped; an oversized or inconsistent frame yields
/// nothing.
#[must_use]
pub fn read_rgba(rgba: &[u8], width: usize, height: usize) -> Vec<String> {
    let Some(n) = width.checked_mul(height).filter(|&n| n > 0 && n <= MAX_PIXELS) else { return Vec::new() };
    if rgba.len() != n * 4 {
        return Vec::new();
    }
    // Rec. 601 luma in integers: (77 R + 150 G + 29 B) / 256.
    let luma: Vec<u8> = rgba
        .chunks_exact(4)
        .map(|p| {
            let y = (77 * u32::from(p[0]) + 150 * u32::from(p[1]) + 29 * u32::from(p[2])) >> 8;
            u8::try_from(y).unwrap_or(255)
        })
        .collect();
    let mut img = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| luma[y * width + x]);
    img.detect_grids().into_iter().filter_map(|g| g.decode().ok().map(|(_, text)| text)).collect()
}
