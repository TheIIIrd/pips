//! Round trips, identity, sizes, and hostile input.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use pips_format::{
    canonical_bytes, decode, decode_file, digest, encode, encode_file, Animation, Card, Error, Extension, Field,
    Handle, Network, PaletteRef, Signature,
};
use pips_palette::{Palette, Rgb};

fn samples() -> Vec<Card> {
    include_str!("data/samples.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|l| {
            let mut parts = l.split(' ');
            let opaque = parts.next() == Some("o");
            let pal: u8 = parts.next().unwrap().parse().unwrap();
            let px = parts.next().unwrap();
            let mut c = Card::new(32, 32);
            c.opaque = opaque;
            c.palette = PaletteRef::Builtin(pal);
            c.pixels = px.bytes().map(|b| (b as char).to_digit(16).unwrap() as u8).collect();
            c
        })
        .collect()
}

/// Every field in use.
fn full() -> Card {
    let colors: Vec<Rgb> = (0..32u8).map(|i| Rgb::new(i * 8, 255 - i * 8, i)).collect();
    let mut c = Card::new(16, 16);
    c.opaque = true;
    c.palette = PaletteRef::Inline(Palette::new(&colors).unwrap());
    c.frame = 7;
    c.animation = Some(Animation { frames: 3, delay_ms: 120 });
    c.title = "Лис у костра".into();
    c.author = "kitsu".into();
    c.handle = Some(Handle { network: Network::Mastodon, text: "@kitsu@pixel.town".into() });
    c.link = "kitsu.art/shop?ref=pips".into();
    c.extensions = vec![
        Extension { critical: false, tag: "acme.foil".into(), data: vec![1, 2, 3] },
        Extension { critical: false, tag: "zz.note".into(), data: vec![] },
    ];
    c.unknown_flags = 0x50;
    c.pixels = (0..16 * 16 * 3).map(|i| (i % 32) as u8).collect();
    c.signature = Some(Signature { public_key: [7; 32], bytes: [9; 64] });
    c
}

#[test]
fn samples_round_trip_and_fit_one_code() {
    let mut sizes = Vec::new();
    for c in samples() {
        let bytes = encode(&c).unwrap();
        assert_eq!(decode(&bytes).unwrap(), c);
        sizes.push(bytes.len());
    }
    sizes.sort_unstable();
    let median = sizes[sizes.len() / 2];
    // Measured: median ~225 B unsigned. With a 96-byte signature every
    // sample still fits the 600-byte budget of one QR code.
    assert!(median < 260, "median {median}");
    assert!(sizes.iter().all(|&s| s + 96 <= 600), "{sizes:?}");
}

#[test]
fn every_field_round_trips_byte_for_byte() {
    let c = full();
    let bytes = encode(&c).unwrap();
    let back = decode(&bytes).unwrap();
    assert_eq!(back, c);
    assert_eq!(encode(&back).unwrap(), bytes);
    assert_eq!(decode_file(&encode_file(&c).unwrap()).unwrap(), c);
    assert_eq!(decode_file(&bytes), Err(Error::BadMagic));
}

#[test]
fn digest_ignores_signature_and_compressor() {
    let c = full();
    let mut unsigned = c.clone();
    unsigned.signature = None;
    assert_eq!(digest(&c).unwrap(), digest(&unsigned).unwrap());
    assert_eq!(canonical_bytes(&c).unwrap(), canonical_bytes(&unsigned).unwrap());

    // Re-encode the pixels with a different compression level: different
    // bytes, same card, same digest.
    let a = encode(&unsigned).unwrap();
    let packed = miniz_oxide::deflate::compress_to_vec(&unsigned.pixels, 1);
    let header_end = a.len() - 2 - miniz_oxide::deflate::compress_to_vec(&unsigned.pixels, 10).len();
    let mut b = a[..header_end].to_vec();
    b.extend_from_slice(&(packed.len() as u16).to_le_bytes());
    b.extend_from_slice(&packed);
    assert_ne!(a, b);
    assert_eq!(digest(&decode(&b).unwrap()).unwrap(), digest(&unsigned).unwrap());

    let mut other = unsigned.clone();
    other.pixels[0] = 1;
    assert_ne!(digest(&other).unwrap(), digest(&unsigned).unwrap());
}

#[test]
fn truncation_at_every_offset_is_refused() {
    let bytes = encode(&full()).unwrap();
    for n in 0..bytes.len() {
        assert!(decode(&bytes[..n]).is_err(), "accepted a prefix of {n} bytes");
    }
    let mut long = bytes.clone();
    long.push(0);
    assert_eq!(decode(&long), Err(Error::TrailingBytes));
}

/// Random corruption must never panic, and anything accepted must be a
/// card in its own right: it re-encodes and decodes to the same thing.
#[test]
fn mutations_never_panic_and_accepted_cards_are_stable() {
    let seeds = [encode(&full()).unwrap(), encode(&samples()[3]).unwrap()];
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut rand = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut accepted = 0;
    for i in 0..60_000 {
        let mut b = seeds[i % 2].clone();
        for _ in 0..=(rand() % 4) {
            let at = (rand() as usize) % b.len();
            match rand() % 3 {
                0 => b[at] = rand() as u8,
                1 => b[at] ^= 1 << (rand() % 8),
                _ => {
                    b.truncate(at.max(1));
                }
            }
        }
        if let Ok(c) = decode(&b) {
            accepted += 1;
            assert_eq!(decode(&encode(&c).unwrap()).unwrap(), c);
        }
    }
    // Text and colour bytes can change without breaking anything.
    assert!(accepted > 0);
}

/// Header of an unsigned, still, untitled 2×2 card on Pips 16, up to and
/// including the extension count.
const HEAD: [u8; 11] = [1, 0, 2, 2, 0, 0, 0, 0, 0, 0, 0];

fn with_pixels(head: &[u8], raw: &[u8]) -> Vec<u8> {
    let packed = miniz_oxide::deflate::compress_to_vec(raw, 6);
    let mut b = head.to_vec();
    b.extend_from_slice(&(packed.len() as u16).to_le_bytes());
    b.extend_from_slice(&packed);
    b
}

#[test]
fn minimal_card_layout() {
    let c = decode(&with_pixels(&HEAD, &[0, 1, 2, 3])).unwrap();
    assert_eq!((c.width, c.height, c.pixels.as_slice()), (2, 2, &[0, 1, 2, 3][..]));
}

#[test]
fn decompression_bomb_is_bounded() {
    // 1 MiB of zeros compresses to about a kilobyte; the card expects four bytes.
    assert_eq!(decode(&with_pixels(&HEAD, &vec![0; 1 << 20])), Err(Error::CorruptPixels));
    assert_eq!(decode(&with_pixels(&HEAD, &[0, 0, 0])), Err(Error::CorruptPixels));
    // Bytes inside the pixel block after the end of the stream.
    let mut b = with_pixels(&HEAD, &[0; 4]);
    let n = u16::from_le_bytes([b[11], b[12]]) + 1;
    b[11..13].copy_from_slice(&n.to_le_bytes());
    b.push(0);
    assert_eq!(decode(&b), Err(Error::CorruptPixels));
}

#[test]
fn oversized_canvas_is_refused_before_inflating() {
    let mut h = HEAD;
    h[1] = 2; // animated
    h[2] = 128;
    h[3] = 128;
    let mut b = h[..6].to_vec();
    b.extend_from_slice(&[64, 100, 0]); // 64 frames, 100 ms
    b.extend_from_slice(&h[6..]);
    b.extend_from_slice(&[0, 0]);
    assert_eq!(decode(&b), Err(Error::TooManyPixels));
}

#[test]
fn header_rules() {
    let patch = |at: usize, v: u8| {
        let mut h = HEAD;
        h[at] = v;
        decode(&with_pixels(&h, &[0; 4]))
    };
    assert_eq!(patch(0, 2), Err(Error::UnsupportedVersion(2)));
    assert_eq!(patch(1, 0x08), Err(Error::UnknownCriticalFlag(0x08)));
    assert_eq!(patch(1, 0x80).unwrap().unknown_flags, 0x80);
    assert_eq!(patch(2, 0), Err(Error::BadDimensions));
    assert_eq!(patch(3, 129), Err(Error::BadDimensions));
    assert_eq!(patch(4, 12), Err(Error::UnknownPalette(12)));
    assert_eq!(patch(5, 16), Err(Error::BadFrame));
    assert_eq!(patch(8, 9), Err(Error::BadHandle));
}

#[test]
fn pixels_must_index_the_palette() {
    let mut h = HEAD;
    h[4] = 21; // Game Boy: four colours
    assert!(decode(&with_pixels(&h, &[0, 1, 2, 3])).is_ok());
    assert_eq!(decode(&with_pixels(&h, &[0, 1, 2, 4])), Err(Error::PixelOutOfPalette));
}

#[test]
fn inline_palette_sizes() {
    for (n, ok) in [(1u8, false), (2, true), (32, true), (33, false)] {
        let mut b = HEAD[..4].to_vec();
        b.extend_from_slice(&[0xFF, n]);
        b.extend(std::iter::repeat_n(0x80, usize::from(n) * 3));
        b.extend_from_slice(&HEAD[5..]);
        let r = decode(&with_pixels(&b, &[0, 1, 0, 1]));
        assert_eq!(r.is_ok(), ok, "{n} colours: {r:?}");
    }
}

#[test]
fn animation_rules() {
    let anim = |frames: u8, delay: u16| {
        let mut c = Card::new(2, 2);
        c.animation = Some(Animation { frames, delay_ms: delay });
        c.pixels = vec![0; 4 * usize::from(frames)];
        encode(&c)
    };
    assert!(anim(2, 20).is_ok());
    assert_eq!(anim(1, 100), Err(Error::BadAnimation));
    assert_eq!(anim(65, 100), Err(Error::BadAnimation));
    assert_eq!(anim(2, 19), Err(Error::BadAnimation));
    assert_eq!(anim(2, 5001), Err(Error::BadAnimation));
}

#[test]
fn text_rules() {
    let with = |f: &dyn Fn(&mut Card)| {
        let mut c = Card::new(2, 2);
        f(&mut c);
        encode(&c)
    };
    assert!(with(&|c| c.title = "ж".repeat(16)).is_ok());
    assert_eq!(with(&|c| c.title = "ж".repeat(17)), Err(Error::BadText(Field::Title)));
    assert_eq!(with(&|c| c.title = " lead".into()), Err(Error::BadText(Field::Title)));
    assert_eq!(with(&|c| c.author = "a\u{202E}b\n".into()), Err(Error::BadText(Field::Author)));
    assert_eq!(
        with(&|c| c.handle = Some(Handle { network: Network::X, text: "two words".into() })),
        Err(Error::BadHandle)
    );
    // Invalid UTF-8 on the wire.
    let mut h = HEAD.to_vec();
    h[6] = 1;
    h.insert(7, 0xFF);
    assert_eq!(decode(&with_pixels(&h, &[0; 4])), Err(Error::BadText(Field::Title)));
}

#[test]
fn link_rules() {
    let link = |s: &str| {
        let mut c = Card::new(2, 2);
        c.link = s.into();
        encode(&c).map(|_| ())
    };
    for ok in ["kitsu.art", "linktr.ee/kitsu", "a-b.example.co.uk/x?y=1#z", "xn--80ak6aa92e.com", ""] {
        assert_eq!(link(ok), Ok(()), "{ok}");
    }
    for bad in [
        "https://kitsu.art",
        "javascript:alert(1)",
        "Kitsu.art",
        "localhost",
        "192.168.0.1",
        "user@evil.com",
        "evil.com:8080",
        "-a.com",
        "a..com",
        "a.com/<script>",
        "a.com/ space",
        &format!("a.com/{}", "x".repeat(60)),
    ] {
        assert_eq!(link(bad), Err(Error::BadLink), "{bad}");
    }
}

#[test]
fn extension_rules() {
    let ext = |e: Vec<Extension>| {
        let mut c = Card::new(2, 2);
        c.extensions = e;
        encode(&c)
    };
    let e = |t: &str, crit: bool| Extension { critical: crit, tag: t.into(), data: vec![] };
    assert!(ext(vec![e("a.x", false), e("b.x", false)]).is_ok());
    assert_eq!(ext(vec![e("b.x", false), e("a.x", false)]), Err(Error::ExtensionOrder));
    assert_eq!(ext(vec![e("a.x", false), e("a.x", false)]), Err(Error::ExtensionOrder));
    assert_eq!(ext(vec![e("a.x", true)]), Err(Error::UnknownCriticalExtension));
    assert_eq!(ext(vec![e("A.x", false)]), Err(Error::BadExtension));
    assert_eq!(ext((0..17).map(|i| e(&format!("t{i:02}"), false)).collect()), Err(Error::BadExtension));
}
