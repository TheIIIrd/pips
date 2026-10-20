//! PNG input and the whole pipeline, on pictures built here.

use pips_format::{decode_file, encode_file, Card, PaletteRef};
use pips_palette::{builtin_by_slug, Rgb};
use pips_raster::{decode, import, nearest, Error, Options, Target};

/// Encodes RGBA pixels as a PNG.
fn png(w: u32, h: u32, px: &[[u8; 4]]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut e = png::Encoder::new(&mut out, w, h);
    e.set_color(png::ColorType::Rgba);
    e.set_depth(png::BitDepth::Eight);
    e.write_header().unwrap().write_image_data(&px.concat()).unwrap();
    out
}

fn rgba(c: u32) -> [u8; 4] {
    let [_, r, g, b] = c.to_be_bytes();
    [r, g, b, 255]
}

const CLEAR: [u8; 4] = [0; 4];

/// A w×h picture drawn by `f`, then blown up `k` times.
fn art(w: u32, h: u32, k: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let px: Vec<_> = (0..h * k).flat_map(|y| (0..w * k).map(move |x| (x, y))).map(|(x, y)| f(x / k, y / k)).collect();
    png(w * k, h * k, &px)
}

fn pico(i: usize) -> u32 {
    builtin_by_slug("pico-8").unwrap().palette().colors()[i].to_u32()
}

#[test]
fn an_upscaled_export_comes_back_at_its_own_size() {
    let file = art(16, 12, 4, |x, y| rgba(pico(((x + y) % 5 + 1) as usize)));
    let (p, report) = import(&file, &Options::default()).unwrap();
    assert_eq!((p.width, p.height, report.scale), (16, 12, 4));
    assert_eq!((report.source_width, report.source_height), (64, 48));
    assert!(p.opaque);
}

#[test]
fn keep_scale_keeps_it() {
    let file = art(16, 12, 4, |x, _| rgba(pico((x % 2) as usize)));
    let opts = Options { unscale: false, ..Options::default() };
    let (p, report) = import(&file, &opts).unwrap();
    assert_eq!((p.width, p.height, report.scale), (64, 48, 1));
}

#[test]
fn exact_colours_pick_the_smallest_builtin_holding_them() {
    // PICO-8 colours 1..=5 also sit in larger palettes; the smallest wins,
    // and every pixel keeps its exact colour.
    let file = art(8, 8, 1, |x, y| rgba(pico(((x * y) % 5 + 1) as usize)));
    let (p, report) = import(&file, &Options::default()).unwrap();
    let PaletteRef::Builtin(id) = p.palette else { panic!("expected a built-in, got {:?}", p.palette) };
    let pal = pips_palette::builtin(id).unwrap().palette();
    assert!(pal.len() <= 16);
    for (i, &k) in p.pixels.iter().enumerate() {
        let (x, y) = (u32::try_from(i % 8).unwrap(), u32::try_from(i / 8).unwrap());
        assert_eq!(pal.colors()[usize::from(k)].to_u32(), pico(((x * y) % 5 + 1) as usize));
    }
    assert_eq!(report.changed_pixels, 0);
}

#[test]
fn own_colours_travel_with_the_card() {
    let own = [0x12_34_56, 0xAB_CD_EF, 0x01_02_03];
    let file = art(6, 1, 1, |x, _| if x == 5 { CLEAR } else { rgba(own[x as usize % 3]) });
    let (p, report) = import(&file, &Options::default()).unwrap();
    let PaletteRef::Inline(pal) = p.palette else { panic!("expected own colours") };
    assert!(!p.opaque);
    assert_eq!(report.colors, 3);
    // Index 0 is transparent and holds a colour the picture does not use.
    assert!(!own.contains(&pal.colors()[0].to_u32()));
    assert_eq!(&pal.colors()[1..].iter().map(|c| c.to_u32()).collect::<Vec<_>>(), &own);
    assert_eq!(p.pixels, [1, 2, 3, 1, 2, 0]);
}

#[test]
fn transparency_keeps_colours_off_index_zero() {
    // Black is PICO-8's index 0; with transparency it must not land there.
    let file = art(2, 1, 1, |x, _| if x == 0 { CLEAR } else { rgba(pico(0)) });
    let (p, _) = import(&file, &Options::default()).unwrap();
    assert_eq!(p.pixels[0], 0);
    assert_ne!(p.pixels[1], 0);
}

#[test]
fn one_colour_still_makes_a_valid_palette() {
    let file = art(3, 3, 1, |x, y| if (x + y) % 2 == 0 { rgba(0x12_34_56) } else { CLEAR });
    let (p, _) = import(&file, &Options::default()).unwrap();
    let mut card = Card::new(1, 1);
    p.apply_to(&mut card);
    decode_file(&encode_file(&card).unwrap()).unwrap();
}

#[test]
fn semi_transparent_pixels_are_refused_with_their_place() {
    let file = art(4, 4, 1, |x, y| if (x, y) == (2, 3) { [9, 9, 9, 128] } else { rgba(0) });
    assert_eq!(import(&file, &Options::default()).unwrap_err(), Error::PartialAlpha { x: 2, y: 3, frame: 0 });
}

#[test]
fn too_many_colours_ask_for_a_palette_and_nearest_maps_them() {
    let file = art(40, 1, 1, |x, _| rgba(x * 0x06_05_04));
    assert_eq!(import(&file, &Options::default()).unwrap_err(), Error::TooManyColors(40));

    let id = builtin_by_slug("pico-8").unwrap().id;
    let opts = Options { target: Target::Nearest(PaletteRef::Builtin(id)), ..Options::default() };
    let (p, report) = import(&file, &opts).unwrap();
    assert_eq!(p.palette, PaletteRef::Builtin(id));
    assert!(report.changed_pixels > 30, "{report:?}");
    assert!(p.pixels.iter().all(|&i| i < 16));
}

#[test]
fn nearest_is_perceptual_and_stable() {
    let pal = builtin_by_slug("pico-8").unwrap().palette();
    // Pure colours land on themselves.
    for (i, c) in pal.colors().iter().enumerate() {
        assert_eq!(usize::from(nearest(*c, &pal, 0)), i);
    }
    // `from` excludes lower indices.
    assert_ne!(nearest(pal.colors()[0], &pal, 1), 0);
    // A dark grey goes to a dark colour, not to white.
    let k = nearest(Rgb::new(20, 20, 20), &pal, 0);
    assert!(pal.colors()[usize::from(k)].to_u32() < 0x40_40_40);
}

#[test]
fn larger_than_a_card_is_refused() {
    let file = art(129, 2, 1, |x, _| rgba(x % 2 * 0xFF_FF_FF));
    assert_eq!(import(&file, &Options::default()).unwrap_err(), Error::TooBig { width: 129, height: 2 });
    // ...but an upscaled 128 is fine.
    let file = art(128, 2, 2, |x, _| rgba(x % 2 * 0xFF_FF_FF));
    assert_eq!(import(&file, &Options::default()).unwrap().0.width, 128);
}

#[test]
fn a_huge_header_is_refused_before_allocating() {
    // A valid 1×1 PNG with IHDR rewritten to 60000×60000 (CRC fixed).
    let mut b = png(1, 1, &[rgba(0)]);
    b[16..20].copy_from_slice(&60000u32.to_be_bytes());
    b[20..24].copy_from_slice(&60000u32.to_be_bytes());
    let crc = crc32(&b[12..29]);
    b[29..33].copy_from_slice(&crc.to_be_bytes());
    assert_eq!(decode(&b), Err(Error::TooLarge));
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

#[test]
fn indexed_and_grey_pngs_decode_to_their_colours() {
    let mut out = Vec::new();
    let mut e = png::Encoder::new(&mut out, 2, 1);
    e.set_color(png::ColorType::Indexed);
    e.set_depth(png::BitDepth::Four);
    e.set_palette(vec![10, 20, 30, 40, 50, 60]);
    e.set_trns(vec![0]);
    e.write_header().unwrap().write_image_data(&[0x01]).unwrap();
    assert_eq!(decode(&out).unwrap().frames[0].pixels, [[10, 20, 30, 0], [40, 50, 60, 255]]);

    let mut out = Vec::new();
    let mut e = png::Encoder::new(&mut out, 2, 1);
    e.set_color(png::ColorType::Grayscale);
    e.set_depth(png::BitDepth::Sixteen);
    e.write_header().unwrap().write_image_data(&[0x12, 0x34, 0xFF, 0xFF]).unwrap();
    assert_eq!(decode(&out).unwrap().frames[0].pixels, [[0x12, 0x12, 0x12, 255], [255, 255, 255, 255]]);
}

#[test]
fn junk_is_refused() {
    assert_eq!(decode(b"GIF89a"), Err(Error::UnknownFormat));
    let b = png(4, 4, &[rgba(1); 16]);
    for n in 0..b.len() {
        assert!(decode(&b[..n]).is_err(), "cut at {n}");
    }
    let mut extra = b.clone();
    extra.push(0);
    assert!(decode(&extra).is_err(), "data after IEND");
    let mut flipped = b.clone();
    flipped[40] ^= 1;
    assert_eq!(decode(&flipped), Err(Error::Malformed("checksum")));
}

#[test]
fn the_result_is_a_valid_card_that_keeps_its_text() {
    let file = art(10, 10, 3, |x, y| if x == y { CLEAR } else { rgba(pico((x % 4 + 1) as usize)) });
    let (p, _) = import(&file, &Options::default()).unwrap();
    let mut card = Card::new(32, 32);
    card.title = "Полдень".into();
    p.clone().apply_to(&mut card);
    let back = decode_file(&encode_file(&card).unwrap()).unwrap();
    assert_eq!((back.width, back.height, back.title.as_str()), (10, 10, "Полдень"));
    assert_eq!(back.pixels, p.pixels);
}

/// Regression: colours past the survey's count cap (257) were mapped to
/// index 0 in nearest mode.
#[test]
fn nearest_maps_every_colour_even_past_the_count_cap() {
    let channel = |v: u32| u8::try_from(v).unwrap();
    let file = art(64, 8, 1, |x, y| [channel(x * 4), channel(y * 30), 128, 255]); // 512 colours
    let pico8 = builtin_by_slug("pico-8").unwrap();
    let opts = Options { target: Target::Nearest(PaletteRef::Builtin(pico8.id)), ..Options::default() };
    let (p, report) = import(&file, &opts).unwrap();
    assert_eq!(report.colors, 257);
    let pal = pico8.palette();
    for (i, &k) in p.pixels.iter().enumerate() {
        let (x, y) = (u32::try_from(i % 64).unwrap(), u32::try_from(i / 64).unwrap());
        let want = nearest(Rgb::new(channel(x * 4), channel(y * 30), 128), &pal, 0);
        assert_eq!(k, want, "pixel {x},{y}");
    }
}

/// Mean redmean distance between the source pixels and the picture's.
fn error(file: &[u8], p: &pips_raster::Picture) -> f64 {
    let src = decode(file).unwrap();
    let pal = p.palette.resolve().unwrap();
    let total: f64 = src.frames[0]
        .pixels
        .iter()
        .zip(&p.pixels)
        .map(|(s, &i)| {
            let c = pal.colors()[usize::from(i)];
            let rm = (i32::from(s[0]) + i32::from(c.r)) / 2;
            let (dr, dg, db) =
                (i32::from(s[0]) - i32::from(c.r), i32::from(s[1]) - i32::from(c.g), i32::from(s[2]) - i32::from(c.b));
            f64::from((((512 + rm) * dr * dr) >> 8) + 4 * dg * dg + (((767 - rm) * db * db) >> 8)).sqrt()
        })
        .sum();
    total / f64::from(u32::try_from(p.pixels.len()).unwrap())
}

#[test]
fn fitting_keeps_a_many_coloured_picture_closer_than_any_fixed_palette() {
    let channel = |v: u32| u8::try_from(v).unwrap();
    let file = art(64, 8, 1, |x, y| [channel(x * 4), channel(y * 30), 128, 255]); // 512 colours
    let fit = Options { target: Target::Reduce(32), ..Options::default() };
    let (p, report) = import(&file, &fit).unwrap();
    let PaletteRef::Inline(pal) = p.palette else { panic!("expected own colours") };
    assert!(pal.len() <= 32 && p.opaque);
    assert!(report.changed_pixels > 0);
    // Deterministic: the same file fits the same palette.
    assert_eq!(import(&file, &fit).unwrap().0, p);
    let fitted = error(&file, &p);
    for b in pips_palette::BUILTINS {
        let fixed = Options { target: Target::Nearest(PaletteRef::Builtin(b.id)), ..Options::default() };
        let other = error(&file, &import(&file, &fixed).unwrap().0);
        assert!(fitted < other, "fitted {fitted:.1} vs {} {other:.1}", b.slug);
    }
}

#[test]
fn fitting_a_picture_that_already_fits_changes_nothing() {
    let file = art(10, 8, 6, |x, y| if (x + y) % 3 == 0 { CLEAR } else { rgba(pico(((x * y) % 5 + 1) as usize)) });
    let fit = Options { target: Target::Reduce(32), ..Options::default() };
    assert_eq!(import(&file, &fit).unwrap(), import(&file, &Options::default()).unwrap());
}

#[test]
fn fitting_keeps_small_accents_and_transparency() {
    // Mostly a soft grey ramp, one red dot, and a transparent corner.
    let file = art(32, 32, 1, |x, y| match (x, y) {
        (0..=3, 0..=3) => CLEAR,
        (20, 20) => [230, 20, 40, 255],
        _ => {
            let v = u8::try_from(60 + x * 3 + y).unwrap();
            [v, v, v.saturating_add(4), 255]
        }
    });
    let (p, _) = import(&file, &Options { target: Target::Reduce(16), ..Options::default() }).unwrap();
    let pal = p.palette.resolve().unwrap();
    assert!(!p.opaque && p.pixels[0] == 0, "transparent stays transparent");
    assert!(pal.len() <= 16 && p.pixels.iter().skip(4).filter(|&&i| i == 0).count() == 12);
    let red = pal.colors()[usize::from(p.pixels[20 * 32 + 20])];
    assert!(red.r > 200 && red.g < 60, "the accent keeps its own colour: {red:?}");
}
