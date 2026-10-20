//! A strict PNG decoder, just large enough for pixel art.
//!
//! Why not the `png` crate: with its deflate back ends it added ~85 KB to
//! the WebAssembly core, against ~10 KB for this module, which reuses the
//! inflater `pips-format` already carries. `png` stays as a test-only
//! reference: `tests/png_reference.rs` decodes thousands of random images
//! with both and requires identical pixels.
//!
//! Every colour type, bit depth and Adam7 interlacing are supported.
//! Colour values are taken as stored: gamma, ICC and sRGB chunks are
//! ignored, because a palette match must be exact and pixel artists pick
//! colours by number. 16-bit samples are cut to their high byte. Only the
//! default image is read; APNG frames are ignored.
//!
//! Strictness: CRCs are checked, chunks must come in the order the
//! standard requires, an unknown critical chunk is refused, and nothing
//! may follow IEND.

use alloc::vec;
use alloc::vec::Vec;

use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

use crate::{check_size, Error, Frame, Raster, Rgba};

pub const MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

const fn bad(what: &'static str) -> Error {
    Error::Malformed(what)
}

struct Header {
    width: u32,
    height: u32,
    depth: u8,
    color: u8,
    interlaced: bool,
}

impl Header {
    /// Samples per pixel.
    const fn channels(&self) -> usize {
        match self.color {
            2 => 3,
            4 => 2,
            6 => 4,
            _ => 1, // 0 grey, 3 palette
        }
    }
    fn bits_per_pixel(&self) -> usize {
        self.channels() * usize::from(self.depth)
    }
    /// Bytes in one filtered row of `w` pixels, without the filter byte.
    fn row_bytes(&self, w: usize) -> usize {
        (w * self.bits_per_pixel()).div_ceil(8)
    }
}

/// Adam7 passes: x0, y0, dx, dy.
const ADAM7: [(usize, usize, usize, usize); 7] =
    [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)];

pub fn decode(bytes: &[u8]) -> Result<Raster, Error> {
    let mut at = MAGIC.len();
    let mut header: Option<Header> = None;
    let mut palette: Vec<Rgba> = Vec::new();
    let mut trns: Option<&[u8]> = None;
    let mut idat: Vec<u8> = Vec::new();
    let mut idat_state = 0u8; // 0 none yet, 1 inside the run, 2 after it

    loop {
        let len = usize::try_from(read_u32(bytes, at)?).map_err(|_| bad("chunk length"))?;
        if len > 0x7FFF_FFFF {
            return Err(bad("chunk length"));
        }
        let kind: [u8; 4] =
            bytes.get(at + 4..at + 8).ok_or(bad("truncated"))?.try_into().map_err(|_| bad("truncated"))?;
        let end = at.checked_add(12 + len).filter(|&e| e <= bytes.len()).ok_or(bad("truncated"))?;
        let data = &bytes[at + 8..at + 8 + len];
        if read_u32(bytes, at + 8 + len)? != crc32(&bytes[at + 4..at + 8 + len]) {
            return Err(bad("checksum"));
        }
        at = end;

        if header.is_none() && &kind != b"IHDR" {
            return Err(bad("IHDR must come first"));
        }
        if idat_state == 1 && &kind != b"IDAT" {
            idat_state = 2;
        }
        match &kind {
            b"IHDR" if header.is_none() => header = Some(read_header(data)?),
            b"PLTE" => {
                if idat_state != 0 || !palette.is_empty() || len % 3 != 0 || len == 0 || len > 768 {
                    return Err(bad("PLTE"));
                }
                palette = data.chunks_exact(3).map(|c| [c[0], c[1], c[2], 255]).collect();
            }
            b"tRNS" => {
                if idat_state != 0 || trns.is_some() {
                    return Err(bad("tRNS"));
                }
                trns = Some(data);
            }
            b"IDAT" => {
                if idat_state == 2 {
                    return Err(bad("IDAT chunks must be consecutive"));
                }
                idat_state = 1;
                idat.extend_from_slice(data);
            }
            b"IEND" => {
                if at != bytes.len() {
                    return Err(bad("data after IEND"));
                }
                break;
            }
            // Ancillary chunks (lowercase first letter) may be skipped.
            k if k[0].is_ascii_lowercase() => {}
            _ => return Err(Error::Unsupported("unknown critical PNG chunk")),
        }
    }

    let h = header.ok_or(bad("no IHDR"))?;
    if idat_state == 0 {
        return Err(bad("no image data"));
    }
    if h.color == 3 && (palette.is_empty() || palette.len() > 1 << h.depth) {
        return Err(bad("PLTE"));
    }
    let key = transparency(&h, trns, &mut palette)?;

    // Exactly the bytes the header promises; inflation stops past them.
    let (w, ht) = (h.width as usize, h.height as usize);
    let passes: Vec<(usize, usize, usize, usize)> = if h.interlaced { ADAM7.to_vec() } else { vec![(0, 0, 1, 1)] };
    let expected: usize = passes
        .iter()
        .map(|&(x0, y0, dx, dy)| {
            let (pw, ph) = (w.saturating_sub(x0).div_ceil(dx), ht.saturating_sub(y0).div_ceil(dy));
            if pw == 0 || ph == 0 {
                0
            } else {
                ph * (1 + h.row_bytes(pw))
            }
        })
        .sum();
    let raw = decompress_to_vec_zlib_with_limit(&idat, expected).map_err(|_| bad("image data does not inflate"))?;
    if raw.len() != expected {
        return Err(bad("image data size"));
    }

    let mut pixels = vec![[0u8; 4]; w * ht];
    let mut off = 0;
    for (x0, y0, dx, dy) in passes {
        let (pw, ph) = (w.saturating_sub(x0).div_ceil(dx), ht.saturating_sub(y0).div_ceil(dy));
        if pw == 0 || ph == 0 {
            continue;
        }
        let rb = h.row_bytes(pw);
        let rows = unfilter(&raw[off..off + ph * (1 + rb)], rb, h.bits_per_pixel().div_ceil(8))?;
        off += ph * (1 + rb);
        for (py, row) in rows.chunks_exact(rb).enumerate() {
            for px in 0..pw {
                pixels[(y0 + py * dy) * w + x0 + px * dx] = pixel(&h, row, px, &palette, key)?;
            }
        }
    }
    Ok(Raster { width: h.width, height: h.height, frames: vec![Frame { pixels, delay_ms: 0 }] })
}

fn read_u32(b: &[u8], at: usize) -> Result<u32, Error> {
    let s = b.get(at..at + 4).ok_or(bad("truncated"))?;
    Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn read_header(d: &[u8]) -> Result<Header, Error> {
    if d.len() != 13 {
        return Err(bad("IHDR"));
    }
    let h = Header {
        width: read_u32(d, 0)?,
        height: read_u32(d, 4)?,
        depth: d[8],
        color: d[9],
        interlaced: match d[12] {
            0 => false,
            1 => true,
            _ => return Err(bad("interlace method")),
        },
    };
    let ok = match h.color {
        0 => matches!(h.depth, 1 | 2 | 4 | 8 | 16),
        3 => matches!(h.depth, 1 | 2 | 4 | 8),
        2 | 4 | 6 => matches!(h.depth, 8 | 16),
        _ => false,
    };
    if !ok || d[10] != 0 || d[11] != 0 {
        return Err(bad("IHDR"));
    }
    check_size(h.width, h.height, 1)?;
    Ok(h)
}

/// Applies tRNS: alpha into the palette, or returns the colour key for
/// grey and RGB images (samples at the image's own depth).
fn transparency(h: &Header, trns: Option<&[u8]>, palette: &mut [Rgba]) -> Result<Option<[u16; 3]>, Error> {
    let Some(t) = trns else { return Ok(None) };
    let s = |i: usize| u16::from_be_bytes([t[i], t[i + 1]]);
    match h.color {
        3 if t.len() <= palette.len() => {
            for (p, &a) in palette.iter_mut().zip(t) {
                p[3] = a;
            }
            Ok(None)
        }
        0 if t.len() == 2 => Ok(Some([s(0); 3])),
        2 if t.len() == 6 => Ok(Some([s(0), s(2), s(4)])),
        _ => Err(bad("tRNS")),
    }
}

/// Reverses the five PNG row filters. `bpp` is bytes per complete pixel,
/// at least 1.
fn unfilter(data: &[u8], rb: usize, bpp: usize) -> Result<Vec<u8>, Error> {
    let mut out = vec![0u8; data.len() / (rb + 1) * rb];
    for (y, line) in data.chunks_exact(rb + 1).enumerate() {
        let (kind, src) = (line[0], &line[1..]);
        let (done, rest) = out.split_at_mut(y * rb);
        let prev = if y == 0 { None } else { Some(&done[(y - 1) * rb..]) };
        let cur = &mut rest[..rb];
        for i in 0..rb {
            let a = if i >= bpp { cur[i - bpp] } else { 0 };
            let b = prev.map_or(0, |p| p[i]);
            let c = if i >= bpp { prev.map_or(0, |p| p[i - bpp]) } else { 0 };
            cur[i] = src[i].wrapping_add(match kind {
                0 => 0,
                1 => a,
                2 => b,
                // Rounds down, as the PNG "Average" filter requires.
                3 => u8::midpoint(a, b),
                4 => paeth(a, b, c),
                _ => return Err(bad("row filter")),
            });
        }
    }
    Ok(out)
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = ((p - i16::from(a)).abs(), (p - i16::from(b)).abs(), (p - i16::from(c)).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Sample `i` of a row at the header's depth, at full precision.
fn sample(h: &Header, row: &[u8], i: usize) -> u16 {
    match h.depth {
        16 => u16::from_be_bytes([row[2 * i], row[2 * i + 1]]),
        8 => u16::from(row[i]),
        d => {
            let d = usize::from(d);
            let bit = i * d;
            u16::from((row[bit / 8] >> (8 - d - bit % 8)) & ((1 << d) - 1))
        }
    }
}

/// A sample scaled to 8 bits: high byte of 16, low depths stretched.
fn to8(h: &Header, v: u16) -> u8 {
    let b = match h.depth {
        16 => v >> 8,
        8 => v,
        d => v * 255 / ((1 << d) - 1),
    };
    u8::try_from(b).unwrap_or(255)
}

#[allow(clippy::many_single_char_names)] // h, x, n, s: header, column, channels, sample
fn pixel(h: &Header, row: &[u8], x: usize, palette: &[Rgba], key: Option<[u16; 3]>) -> Result<Rgba, Error> {
    let n = h.channels();
    let s = |c: usize| sample(h, row, x * n + c);
    Ok(match h.color {
        3 => *palette.get(usize::from(s(0))).ok_or(bad("colour index outside PLTE"))?,
        0 => {
            let v = to8(h, s(0));
            [v, v, v, if key == Some([s(0); 3]) { 0 } else { 255 }]
        }
        2 => {
            let rgb = [s(0), s(1), s(2)];
            [to8(h, rgb[0]), to8(h, rgb[1]), to8(h, rgb[2]), if key == Some(rgb) { 0 } else { 255 }]
        }
        4 => {
            let v = to8(h, s(0));
            [v, v, v, to8(h, s(1))]
        }
        _ => [to8(h, s(0)), to8(h, s(1)), to8(h, s(2)), to8(h, s(3))],
    })
}

/// CRC-32 (ISO-HDLC), the one PNG uses; table built at compile time.
fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut t = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            // i < 256: lossless.
            #[allow(clippy::cast_possible_truncation)]
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                k += 1;
            }
            t[i] = c;
            i += 1;
        }
        t
    };
    !data.iter().fold(!0u32, |c, &b| TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8))
}
