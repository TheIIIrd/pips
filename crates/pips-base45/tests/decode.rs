//! Conformance with RFC 9285 and the strictness promises in the crate docs.

use pips_base45::{decode, decoded_len, encode, encoded_len, DecodeError, ALPHABET};

#[test]
fn rfc_9285_examples() {
    // Section 4.3 and 4.4 of the RFC.
    for (bytes, text) in
        [(&b"AB"[..], "BB8"), (b"Hello!!", "%69 VD92EX0"), (b"base-45", "UJCLQE7W581"), (b"ietf!", "QED8WEX0")]
    {
        assert_eq!(encode(bytes), text);
        assert_eq!(decode(text).unwrap(), bytes);
    }
}

#[test]
fn empty() {
    assert_eq!(encode(b""), "");
    assert_eq!(decode("").unwrap(), b"");
}

#[test]
fn every_one_and_two_byte_input_round_trips() {
    for a in 0..=255u8 {
        assert_eq!(decode(encode(&[a])).unwrap(), [a]);
        for b in 0..=255u8 {
            assert_eq!(decode(encode(&[a, b])).unwrap(), [a, b]);
        }
    }
}

/// Every two- and three-character string over the alphabet either decodes
/// and re-encodes to itself, or is refused as out of range — never both,
/// never silently altered. Together with the per-group structure this makes
/// the encoding a bijection.
#[test]
fn every_short_string_is_canonical_or_refused() {
    for &c in ALPHABET {
        for &d in ALPHABET {
            let s = [c, d];
            let value = digit(c) + 45 * digit(d);
            match decode(s) {
                Ok(bytes) => {
                    assert!(value <= 0xFF);
                    assert_eq!(encode(&bytes).as_bytes(), s);
                }
                Err(e) => {
                    assert!(value > 0xFF);
                    assert_eq!(e, DecodeError::GroupOutOfRange { position: 0 });
                }
            }
            for &e in ALPHABET {
                let s = [c, d, e];
                let value = digit(c) + 45 * digit(d) + 2025 * digit(e);
                match decode(s) {
                    Ok(bytes) => {
                        assert!(value <= 0xFFFF);
                        assert_eq!(encode(&bytes).as_bytes(), s);
                    }
                    Err(_) => assert!(value > 0xFFFF),
                }
            }
        }
    }
}

#[test]
fn round_trips_at_every_length() {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    for len in 0..600 {
        let data: Vec<u8> = (0..len)
            .map(|_| {
                // xorshift64: enough variety, no dependency.
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state.to_le_bytes()[0]
            })
            .collect();
        let text = encode(&data);
        assert_eq!(text.len(), encoded_len(len));
        assert_eq!(decoded_len(text.len()), Some(len));
        assert_eq!(decode(&text).unwrap(), data);
    }
}

#[test]
fn single_trailing_character_is_refused() {
    assert_eq!(decode("A"), Err(DecodeError::InvalidLength { len: 1 }));
    assert_eq!(decode("BB8A"), Err(DecodeError::InvalidLength { len: 4 }));
}

#[test]
fn characters_outside_the_alphabet_are_refused() {
    assert_eq!(decode("bb8"), Err(DecodeError::InvalidCharacter { position: 0, byte: b'b' }));
    assert_eq!(decode("BB\n"), Err(DecodeError::InvalidCharacter { position: 2, byte: b'\n' }));
    // Non-ASCII: reported at its first UTF-8 byte.
    assert_eq!(decode("Bé"), Err(DecodeError::InvalidCharacter { position: 1, byte: 0xC3 }));
}

#[test]
fn out_of_range_groups_are_refused() {
    // ":::" is 44 + 44*45 + 44*2025 = 91124 > 65535.
    assert_eq!(decode(":::"), Err(DecodeError::GroupOutOfRange { position: 0 }));
    // "::" is 44 + 44*45 = 2024 > 255, as a trailing pair.
    assert_eq!(decode("BB8::"), Err(DecodeError::GroupOutOfRange { position: 3 }));
    // The largest legal values sit right below the limits.
    assert_eq!(decode(encode(&[0xFF, 0xFF])).unwrap(), [0xFF, 0xFF]);
    assert_eq!(decode(encode(&[0xFF])).unwrap(), [0xFF]);
}

#[test]
fn errors_display_usefully() {
    assert_eq!(decode("bb8").unwrap_err().to_string(), "invalid character 'b' at position 0");
    assert_eq!(decode("BB\n").unwrap_err().to_string(), "invalid byte 0x0a at position 2");
    assert_eq!(decode("A").unwrap_err().to_string(), "length 1 leaves a single trailing character");
}

fn digit(c: u8) -> u32 {
    u32::try_from(ALPHABET.iter().position(|&a| a == c).unwrap()).unwrap()
}
