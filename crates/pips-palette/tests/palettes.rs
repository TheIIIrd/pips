//! Built-in table invariants, import strictness, and export round trips.
#![allow(clippy::unreadable_literal, clippy::format_push_string, clippy::format_collect)]

use std::collections::HashSet;

use pips_palette::{
    builtin, builtin_by_slug, parse, to_gpl, to_hex, ErrorKind, Palette, ParseError, Rgb, BUILTINS, MAX_INPUT_BYTES,
};

#[test]
fn builtin_table_is_well_formed() {
    let mut ids = HashSet::new();
    let mut slugs = HashSet::new();
    for b in BUILTINS {
        assert!(ids.insert(b.id), "duplicate id {}", b.id);
        assert!(slugs.insert(b.slug), "duplicate slug {}", b.slug);
        assert!(b.id <= 63, "{}: ids above 63 are reserved", b.slug);
        assert!(b.slug.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'));
        let p = b.palette();
        assert!(matches!(p.len(), 2..=32), "{}", b.slug);
        let distinct: HashSet<_> = p.colors().iter().collect();
        assert_eq!(distinct.len(), p.len(), "{} repeats a colour", b.slug);
        if let Some(names) = b.color_names {
            assert_eq!(names.len(), p.len(), "{} names", b.slug);
        }
        assert_eq!(builtin(b.id).unwrap().slug, b.slug);
        assert_eq!(builtin_by_slug(b.slug).unwrap().id, b.id);
    }
    // Table order is id order, which `list` relies on.
    assert!(BUILTINS.windows(2).all(|w| w[0].id < w[1].id));
}

#[test]
fn pips_32_extends_pips_16() {
    let p16 = builtin(0).unwrap().palette();
    let p32 = builtin(1).unwrap().palette();
    assert_eq!(builtin(0).unwrap().slug, "pips-16");
    assert_eq!(p32.len(), 32);
    assert_eq!(&p32.colors()[..16], p16.colors());
}

#[test]
fn every_builtin_round_trips_through_both_formats() {
    for b in BUILTINS {
        let p = b.palette();
        let hex = parse(to_hex(&p).as_bytes()).unwrap();
        assert_eq!(hex.palette, p, "{} via hex", b.slug);
        assert_eq!(hex.name, None);
        let gpl = parse(to_gpl(&p, b.name).as_bytes()).unwrap();
        assert_eq!(gpl.palette, p, "{} via gpl", b.slug);
        assert_eq!(gpl.name.as_deref(), Some(b.name));
    }
}

#[test]
fn hex_accepts_common_spellings() {
    let p = parse(b"\xEF\xBB\xBF#1A1C2C\r\n\n  5d275d  \r\nB13E53").unwrap().palette;
    assert_eq!(p.colors(), [Rgb::from_u32(0x1a1c2c), Rgb::from_u32(0x5d275d), Rgb::from_u32(0xb13e53)]);
}

#[test]
fn hex_refuses_anything_else() {
    for (input, line) in [
        (&b"1a1c2c\n1a1c2"[..], 2),       // short
        (b"1a1c2c\n1a1c2c0", 2),          // long
        (b"1a1c2c\n+a1c2c", 2),           // sign
        (b"1a1c2c\nff1a1c2c", 2),         // paint.net ARGB
        (b"1a1c2c ; comment\n1a1c2c", 1), // trailing junk
        (b"1a1c2c\n1g1c2c", 2),           // not hex
    ] {
        assert_eq!(parse(input), Err(ParseError { line, kind: ErrorKind::BadColor }), "{input:?}");
    }
}

#[test]
fn gpl_as_aseprite_and_gimp_write_it() {
    let file = "GIMP Palette\r\nName: Sweetie 16\r\nColumns: 8\r\nChannels: RGB\r\n#\r\n# comment\r\n\
                 26  28  44\tUntitled\r\n 93  39  93\r\n177 62 83 Rose red\r\n";
    let p = parse(file.as_bytes()).unwrap();
    assert_eq!(p.name.as_deref(), Some("Sweetie 16"));
    assert_eq!(p.palette.colors()[2], Rgb::new(177, 62, 83));
    assert_eq!(p.palette.len(), 3);
}

#[test]
fn gpl_refuses_anything_else() {
    let head = "GIMP Palette\n";
    for (body, line, kind) in [
        ("1 2\n3 4 5\n", 2, ErrorKind::BadColor),
        ("1 2 256\n3 4 5\n", 2, ErrorKind::BadColor),
        ("1 2 -3\n3 4 5\n", 2, ErrorKind::BadColor),
        ("1 2 0003\n3 4 5\n", 2, ErrorKind::BadColor),
        ("Channels: RGBA\n1 2 3 255\n", 2, ErrorKind::UnsupportedChannels),
        ("Columns: many\n", 2, ErrorKind::BadHeader),
        ("Name: a\u{7}b\n", 2, ErrorKind::BadHeader),
        ("1 2 3\n4 5 6\nName: late\n", 4, ErrorKind::BadColor),
    ] {
        let input = format!("{head}{body}");
        assert_eq!(parse(input.as_bytes()), Err(ParseError { line, kind }), "{body:?}");
    }
}

#[test]
fn colour_count_limits() {
    assert_eq!(parse(b"000000\n"), Err(ParseError { line: 0, kind: ErrorKind::TooFewColors }));
    assert_eq!(parse(b""), Err(ParseError { line: 0, kind: ErrorKind::TooFewColors }));
    let mut many = String::new();
    for i in 0..33 {
        many += &format!("{i:06x}\n");
    }
    assert_eq!(parse(many.as_bytes()), Err(ParseError { line: 33, kind: ErrorKind::TooManyColors }));
    let exactly: String = many.lines().take(32).map(|l| format!("{l}\n")).collect();
    assert_eq!(parse(exactly.as_bytes()).unwrap().palette.len(), 32);
}

#[test]
fn size_and_encoding_limits() {
    let big = vec![b'\n'; MAX_INPUT_BYTES + 1];
    assert_eq!(parse(&big), Err(ParseError { line: 0, kind: ErrorKind::TooLarge }));
    assert_eq!(parse(b"\xFF\xFE0\x000\x00"), Err(ParseError { line: 0, kind: ErrorKind::NotUtf8 }));
}

#[test]
fn gpl_export_sanitises_names() {
    let p = builtin(0).unwrap().palette();
    let out = to_gpl(&p, "evil\nName: x\u{0}");
    assert_eq!(parse(out.as_bytes()).unwrap().name.as_deref(), Some("evil Name: x"));
    let long = "ж".repeat(100);
    let name = parse(to_gpl(&p, &long).as_bytes()).unwrap().name.unwrap();
    assert_eq!(name.len(), 64);
}

#[test]
fn palette_construction_limits() {
    assert!(Palette::new(&[Rgb::default()]).is_err());
    assert!(Palette::new(&[Rgb::default(); 33]).is_err());
    let p = Palette::new(&[Rgb::new(1, 2, 3), Rgb::new(4, 5, 6)]).unwrap();
    assert_eq!(p.get(1), Some(Rgb::new(4, 5, 6)));
    assert_eq!(p.get(2), None);
    assert_eq!(format!("{p:?}"), "[#010203, #040506]");
}

/// Third-party built-ins against the `.hex` files their authors publish on
/// Lospec, kept in `tests/lospec/` as the reference.
#[test]
fn third_party_palettes_match_lospec() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/lospec");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let slug = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let published = parse(&std::fs::read(&path).unwrap()).unwrap().palette;
        let ours = builtin_by_slug(&slug).unwrap_or_else(|| panic!("{slug}.hex has no built-in"));
        assert_eq!(ours.palette(), published, "{slug}");
        checked += 1;
    }
    assert_eq!(checked, 19);
}
