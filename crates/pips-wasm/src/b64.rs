//! Base64url without padding (RFC 4648 §5), strict: one spelling per byte
//! string, so a link cannot be altered in its unused bits.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &b)| acc | u32::from(b) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize]));
        }
    }
    out
}

pub fn decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.as_bytes().chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            let v = ALPHABET.iter().position(|&a| a == c)?;
            n |= u32::try_from(v).ok()? << (18 - 6 * i);
        }
        let bytes = chunk.len() - 1;
        let [_, a, b, c] = n.to_be_bytes();
        // Bits below the last whole byte must be zero, or two texts would
        // give the same bytes.
        let unused = n & ((1 << (8 * (3 - bytes))) - 1);
        if unused != 0 {
            return None;
        }
        out.extend_from_slice(&[a, b, c][..bytes]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn rfc4648_vectors_and_strictness() {
        for (raw, text) in [
            ("", ""),
            ("f", "Zg"),
            ("fo", "Zm8"),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg"),
            ("fooba", "Zm9vYmE"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(super::encode(raw.as_bytes()), text);
            assert_eq!(super::decode(text).unwrap(), raw.as_bytes());
        }
        assert_eq!(super::encode(&[0xFB, 0xFF]), "-_8");
        assert!(super::decode("Zh").is_none(), "non-zero unused bits");
        assert!(super::decode("Zm9v=").is_none());
        assert!(super::decode("Z").is_none());
        assert!(super::decode("Zm+v").is_none());
    }
}
