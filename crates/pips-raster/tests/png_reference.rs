//! Our PNG decoder against the `png` crate, on random images of every
//! colour type, bit depth, filter and interlacing.
//!
//! The images are built here: random filter bytes and random filtered data
//! form a valid PNG stream whatever their values, so every filter path and
//! every Adam7 pass gets exercised without an encoder.

// Random test data is cut to size on purpose.
#![allow(clippy::cast_possible_truncation)]

use miniz_oxide::deflate::compress_to_vec_zlib;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }
}

fn chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
    let start = out.len();
    out.extend_from_slice(&kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 == 1 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

const ADAM7: [(u32, u32, u32, u32); 7] =
    [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)];

/// A random PNG; returns the file.
fn random_png(rng: &mut Rng) -> Vec<u8> {
    let (color, depths): (u8, &[u8]) = match rng.below(5) {
        0 => (0, &[1, 2, 4, 8, 16]),
        1 => (2, &[8, 16]),
        2 => (3, &[1, 2, 4, 8]),
        3 => (4, &[8, 16]),
        _ => (6, &[8, 16]),
    };
    let depth = depths[rng.below(depths.len() as u64) as usize];
    let (w, h) = (1 + rng.below(33) as u32, 1 + rng.below(33) as u32);
    let interlaced = rng.below(2) == 1;
    let channels = match color {
        2 => 3,
        4 => 2,
        6 => 4,
        _ => 1,
    };
    let bits = channels * u32::from(depth);

    let mut file = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[depth, color, 0, 0, u8::from(interlaced)]);
    chunk(&mut file, *b"IHDR", &ihdr);
    if color == 3 {
        // Usually a full palette; sometimes a short one, so out-of-range
        // indices must be refused by both decoders.
        let full = 1usize << depth;
        let n = if rng.below(8) == 0 { 1 + rng.below(full as u64) as usize } else { full };
        chunk(&mut file, *b"PLTE", &rng.bytes(3 * n));
        if rng.below(2) == 1 {
            let k = rng.below(n as u64 + 1) as usize;
            chunk(&mut file, *b"tRNS", &rng.bytes(k));
        }
    } else if (color == 0 || color == 2) && rng.below(2) == 1 {
        // A colour key; small values so that it actually matches pixels.
        let max = if depth == 16 { 3 } else { (1u64 << depth.min(8)) - 1 }.min(3);
        let key: Vec<u8> =
            (0..if color == 0 { 1 } else { 3 }).flat_map(|_| (rng.below(max + 1) as u16).to_be_bytes()).collect();
        chunk(&mut file, *b"tRNS", &key);
    }
    chunk(&mut file, *b"tEXt", b"Comment\0ignored"); // ancillary: skipped

    let passes: Vec<(u32, u32, u32, u32)> = if interlaced { ADAM7.to_vec() } else { vec![(0, 0, 1, 1)] };
    let mut raw = Vec::new();
    for (x0, y0, dx, dy) in passes {
        let (pw, ph) = (w.saturating_sub(x0).div_ceil(dx), h.saturating_sub(y0).div_ceil(dy));
        if pw == 0 || ph == 0 {
            continue;
        }
        for _ in 0..ph {
            raw.push(rng.below(5) as u8);
            // Mostly small values, so colour keys and short palettes get hit.
            let rb = (pw * bits).div_ceil(8) as usize;
            raw.extend((0..rb).map(|_| if rng.below(3) == 0 { rng.next() as u8 } else { rng.below(2) as u8 }));
        }
    }
    let z = compress_to_vec_zlib(&raw, 6);
    // Split the data over several IDAT chunks.
    let cut = z.len() / 2;
    chunk(&mut file, *b"IDAT", &z[..cut]);
    chunk(&mut file, *b"IDAT", &z[cut..]);
    chunk(&mut file, *b"IEND", &[]);
    file
}

/// The `png` crate's view, as straight RGBA.
fn reference(file: &[u8]) -> Result<Vec<[u8; 4]>, ()> {
    let mut d = png::Decoder::new(std::io::Cursor::new(file));
    d.set_transformations(png::Transformations::normalize_to_color8());
    let mut r = d.read_info().map_err(|_| ())?;
    let mut buf = vec![0; r.output_buffer_size().ok_or(())?];
    let out = r.next_frame(&mut buf).map_err(|_| ())?;
    let data = &buf[..out.buffer_size()];
    Ok(match out.color_type {
        png::ColorType::Rgba => data.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]]).collect(),
        png::ColorType::Rgb => data.chunks_exact(3).map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => data.iter().map(|&v| [v, v, v, 255]).collect(),
        png::ColorType::Indexed => return Err(()),
    })
}

#[test]
fn agrees_with_the_png_crate() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let (mut ok, mut refused) = (0, 0);
    for n in 0..4000 {
        let file = random_png(&mut rng);
        let ours = pips_raster::decode(&file).map(|r| r.frames.into_iter().next().unwrap().pixels);
        match (ours, reference(&file)) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a, b, "image {n} decodes differently");
                ok += 1;
            }
            // Both refuse; or we alone, deliberately: the standard forbids
            // indices past the palette, `png` quietly draws them.
            (Err(_), Err(())) | (Err(pips_raster::Error::Malformed("colour index outside PLTE")), Ok(_)) => {
                refused += 1;
            }
            (a, b) => panic!("image {n}: we say {:?}, png says {:?}", a.map(|_| "ok"), b.map(|_| "ok")),
        }
    }
    // Both kinds of outcome were actually exercised.
    assert!(ok > 3000 && refused > 20, "ok {ok}, refused {refused}");
}
