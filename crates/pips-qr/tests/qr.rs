//! Carousel recovery under loss, hostile frames, and symbols that really scan.
#![allow(clippy::cast_possible_truncation)]

use pips_qr::carousel::{crc32, Decoder, Encoder, FrameError, Progress};
use pips_qr::{plan, Plan, SINGLE_MAX};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn message(len: usize, seed: u64) -> Vec<u8> {
    let mut r = Rng(seed);
    (0..len).map(|_| r.next() as u8).collect()
}

fn feed(d: &mut Decoder, frame: &[u8]) -> Option<Vec<u8>> {
    match d.push(frame).unwrap() {
        Progress::Done(m) => Some(m),
        Progress::Partial { .. } => None,
    }
}

#[test]
fn crc32_reference() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

#[test]
fn in_order_needs_exactly_n_frames() {
    for len in [1, 199, 200, 201, 1000, 5000, 65_535] {
        let msg = message(len, len as u64);
        let enc = Encoder::new(&msg, 256).unwrap();
        let mut d = Decoder::new();
        for seq in 0..enc.fragments() as u32 {
            let got = feed(&mut d, &enc.frame(seq));
            if seq as usize + 1 == enc.fragments() {
                assert_eq!(got.as_deref(), Some(&msg[..]), "len {len}");
            } else {
                assert!(got.is_none());
            }
        }
    }
}

/// A camera that starts late and misses frames still gets the message, and
/// pays little for it.
#[test]
fn recovers_under_loss_with_small_overhead() {
    let msg = message(2400, 7); // 12 fragments of 200
    let enc = Encoder::new(&msg, 200).unwrap();
    let n = enc.fragments();
    let mut extra_total = 0u32;
    let trials = 300;
    let mut r = Rng(99);
    for _ in 0..trials {
        let mut d = Decoder::new();
        let start = (r.next() % 1000) as u32;
        let mut seen = 0;
        let mut seq = start;
        let got = loop {
            seq += 1;
            if r.next() % 10 < 3 {
                continue; // 30% of frames missed
            }
            seen += 1;
            if let Some(m) = feed(&mut d, &enc.frame(seq)) {
                break m;
            }
            assert!(seen < n * 4, "not converging");
        };
        assert_eq!(got, msg);
        extra_total += u32::try_from(seen - n).unwrap();
    }
    let mean = f64::from(extra_total) / f64::from(trials);
    // Random linear codes over GF(2) need about 1.6 extra frames on average.
    assert!(mean < 2.5, "mean overhead {mean:.2} frames");
}

#[test]
fn duplicates_are_harmless() {
    let msg = message(900, 3);
    let enc = Encoder::new(&msg, 300).unwrap();
    let mut d = Decoder::new();
    for _ in 0..5 {
        assert_eq!(d.push(&enc.frame(0)).unwrap(), Progress::Partial { have: 1, need: 3 });
    }
    assert!(feed(&mut d, &enc.frame(1)).is_none());
    assert_eq!(feed(&mut d, &enc.frame(2)).unwrap(), msg);
}

#[test]
fn hostile_frames() {
    let msg = message(900, 3);
    let enc = Encoder::new(&msg, 300).unwrap();
    let f = enc.frame(5);
    let mut d = Decoder::new();

    for n in 0..f.len() {
        assert!(d.push(&f[..n]).is_err(), "accepted {n}-byte prefix");
    }
    let mut long = f.clone();
    long.push(0);
    assert_eq!(d.push(&long), Err(FrameError::BadHeader));
    let mut v = f.clone();
    v[1] = 2;
    assert_eq!(d.push(&v), Err(FrameError::UnsupportedVersion(2)));
    assert_eq!(d.push(b"E\x01"), Err(FrameError::WrongKind));

    // Header lies: zero length, tiny fragments, too many fragments.
    let mut h = f.clone();
    h[2..4].copy_from_slice(&0u16.to_le_bytes());
    assert_eq!(d.push(&h), Err(FrameError::BadHeader));
    let mut h = f[..14].to_vec();
    h[2..4].copy_from_slice(&u16::MAX.to_le_bytes());
    h[4..6].copy_from_slice(&16u16.to_le_bytes());
    h.extend_from_slice(&[0; 16]);
    assert_eq!(d.push(&h), Err(FrameError::BadHeader));

    // A frame from another message mid-stream.
    assert!(d.push(&enc.frame(0)).is_ok());
    let other = Encoder::new(&message(900, 4), 300).unwrap();
    assert_eq!(d.push(&other.frame(1)), Err(FrameError::Mismatch));

    // A corrupted payload byte is caught by the checksum, and the decoder resets.
    let mut d = Decoder::new();
    let mut bad = enc.frame(1);
    bad[20] ^= 1;
    d.push(&enc.frame(0)).unwrap();
    d.push(&bad).unwrap();
    assert_eq!(d.push(&enc.frame(2)), Err(FrameError::Checksum));
    assert!(matches!(d.push(&enc.frame(0)), Ok(Progress::Partial { have: 1, .. })));
}

#[test]
fn encoder_limits() {
    assert!(Encoder::new(b"", 200).is_err());
    assert!(Encoder::new(&[0; 100], 15).is_err());
    assert!(Encoder::new(&[0; 100], 1025).is_err());
    assert!(Encoder::new(&vec![0; 65_536], 1024).is_err());
    assert!(Encoder::new(&[0; 5000], 16).is_err()); // 313 fragments
}

#[test]
fn plan_switches_at_the_single_code_budget() {
    assert!(matches!(plan(&[1; SINGLE_MAX]), Ok(Plan::Single(_))));
    assert!(matches!(plan(&[1; SINGLE_MAX + 1]), Ok(Plan::Carousel(_))));
    assert!(plan(&[]).is_err());
}

// ----------------------------------------------------------------- symbols --

/// Draws the matrix as a greyscale image and reads it back with `rqrr`,
/// an independent decoder.
fn scan(m: &pips_qr::matrix::Matrix) -> String {
    let (scale, border) = (4, 4);
    let side = (m.size() + 2 * border) * scale;
    let mut img = rqrr::PreparedImage::prepare_from_greyscale(side, side, |x, y| {
        let (mx, my) = (x / scale, y / scale);
        let dark = mx >= border && my >= border && m.get(mx - border, my - border);
        if dark {
            0
        } else {
            255
        }
    });
    let grids = img.detect_grids();
    assert_eq!(grids.len(), 1);
    grids[0].decode().unwrap().1
}

#[test]
fn single_codes_scan_back_at_full_budget() {
    for len in [40, 330, SINGLE_MAX] {
        let msg = message(len, 11);
        let Plan::Single(text) = plan(&msg).unwrap() else { panic!() };
        let m = pips_qr::matrix::Matrix::encode(&text).unwrap();
        let read = scan(&m);
        assert_eq!(pips_base45::decode(&read).unwrap(), msg, "{len} bytes");
        if len == SINGLE_MAX {
            assert!(m.size() <= 101, "600 bytes needs a {}-module symbol", m.size());
        }
    }
}

#[test]
fn carousel_frames_scan_back() {
    let msg = message(3000, 12);
    let Plan::Carousel(enc) = plan(&msg).unwrap() else { panic!() };
    let mut d = Decoder::new();
    let mut seq = 40; // start mid-loop
    loop {
        let text = pips_base45::encode(&enc.frame(seq));
        let m = pips_qr::matrix::Matrix::encode(&text).unwrap();
        assert!(m.size() <= 69, "frame symbol {} modules", m.size());
        if let Some(out) = feed(&mut d, &pips_base45::decode(scan(&m)).unwrap()) {
            assert_eq!(out, msg);
            break;
        }
        seq += 1;
    }
}

#[test]
fn svg_and_terminal_output() {
    let m = pips_qr::matrix::Matrix::encode("HELLO PIPS").unwrap();
    let svg = m.to_svg(4);
    assert!(svg.starts_with("<svg") && !svg.contains("<script"));
    let t = m.to_terminal(2);
    assert_eq!(t.lines().count(), (m.size() + 4).div_ceil(2));
}

/// What `matrix` draws, `scan` reads back — through an RGBA frame with the
/// symbol scaled up and offset, as a camera would see a screen.
#[cfg(all(feature = "matrix", feature = "scan"))]
#[test]
fn scan_reads_what_matrix_draws() {
    let text = "EPIPS TEST 123 $%*+-./:";
    let m = pips_qr::matrix::Matrix::encode(text).unwrap();
    let (k, pad) = (5, 37);
    let side = m.size() * k + 2 * pad;
    let mut rgba = vec![255u8; side * side * 4];
    for y in 0..m.size() {
        for x in 0..m.size() {
            if m.get(x, y) {
                for dy in 0..k {
                    for dx in 0..k {
                        let o = ((pad + y * k + dy) * side + pad + x * k + dx) * 4;
                        rgba[o..o + 3].fill(20);
                    }
                }
            }
        }
    }
    assert_eq!(pips_qr::scan::read_rgba(&rgba, side, side), vec![text.to_string()]);
    assert!(pips_qr::scan::read_rgba(&rgba[4..], side, side).is_empty(), "inconsistent size");
    assert!(pips_qr::scan::read_rgba(&[], 0, 0).is_empty());
}
