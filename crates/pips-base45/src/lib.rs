//! Base45 as defined by [RFC 9285], strict on input.
//!
//! Base45 exists for QR codes: every character it emits is in the QR
//! alphanumeric set, which packs 11 bits per two characters. Three
//! characters carry two bytes, so a payload grows by half — against
//! roughly a third for Base64 — yet the QR symbol ends up smaller, because
//! Base64 output would force the far less dense byte mode.
//!
//! # Strictness
//!
//! Text handed to [`decode`] comes off a stranger's screen, so it is refused
//! rather than repaired. The RFC permits exactly one string per byte
//! sequence, and this decoder accepts only that string:
//!
//! - every character must be in the alphabet — no lowercase, no whitespace
//!   other than the space that *is* an alphabet character;
//! - a three-character group must not exceed 65535, and a trailing
//!   two-character group must not exceed 255;
//! - a lone trailing character is an error, not padding.
//!
//! Consequently `encode(&decode(s)?) == s` for every `s` that decodes.
//!
//! # Resource use
//!
//! Decoding allocates at most two bytes for every three input bytes and
//! does so once, after the length has been validated.
//!
//! ```
//! let text = pips_base45::encode(b"Hello!!");
//! assert_eq!(text, "%69 VD92EX0");
//! assert_eq!(pips_base45::decode(&text).unwrap(), b"Hello!!");
//! ```
//!
//! [RFC 9285]: https://www.rfc-editor.org/rfc/rfc9285

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// The 45 characters, in value order.
pub const ALPHABET: &[u8; 45] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

const INVALID: u8 = 0xFF;

/// Byte value to digit, or [`INVALID`].
const DIGITS: [u8; 256] = {
    let mut table = [INVALID; 256];
    let mut i = 0;
    while i < ALPHABET.len() {
        // `i < 45`, so the cast is lossless.
        #[allow(clippy::cast_possible_truncation)]
        let digit = i as u8;
        table[ALPHABET[i] as usize] = digit;
        i += 1;
    }
    table
};

/// Number of characters [`encode`] produces for `len` input bytes.
#[must_use]
pub const fn encoded_len(len: usize) -> usize {
    // `len` is a slice length, so at most `isize::MAX`; `len / 2 * 3 + 2`
    // therefore stays below `usize::MAX`.
    len / 2 * 3 + (len % 2) * 2
}

/// Number of bytes `len` characters decode to, or `None` if no valid
/// Base45 string has that length.
#[must_use]
pub const fn decoded_len(len: usize) -> Option<usize> {
    match len % 3 {
        0 => Some(len / 3 * 2),
        2 => Some(len / 3 * 2 + 1),
        _ => None,
    }
}

/// Encodes `data` as Base45.
#[must_use]
pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(encoded_len(data.len()));
    encode_into(data, &mut out);
    out
}

/// Appends the Base45 form of `data` to `out`.
pub fn encode_into(data: &[u8], out: &mut String) {
    out.reserve(encoded_len(data.len()));
    let mut pairs = data.chunks_exact(2);
    for pair in &mut pairs {
        let n = u32::from(pair[0]) << 8 | u32::from(pair[1]);
        push_digits(out, n, 3);
    }
    if let [last] = pairs.remainder() {
        push_digits(out, u32::from(*last), 2);
    }
}

fn push_digits(out: &mut String, mut n: u32, count: usize) {
    for _ in 0..count {
        // `n % 45 < 45`: the index is in bounds and the cast lossless.
        out.push(char::from(ALPHABET[(n % 45) as usize]));
        n /= 45;
    }
}

/// Why a string is not valid Base45.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// The length leaves a single character after the last full group.
    InvalidLength {
        /// Length of the input, in bytes.
        len: usize,
    },
    /// A byte that is not one of the 45 alphabet characters.
    InvalidCharacter {
        /// Offset of the byte in the input.
        position: usize,
        /// The offending byte. For non-ASCII text this is the first byte
        /// of its UTF-8 encoding.
        byte: u8,
    },
    /// A group whose value does not fit the bytes it stands for — above
    /// 65535 for three characters, above 255 for two. Such a group is not
    /// produced by any encoder.
    GroupOutOfRange {
        /// Offset of the group's first character in the input.
        position: usize,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::InvalidLength { len } => {
                write!(f, "length {len} leaves a single trailing character")
            }
            Self::InvalidCharacter { position, byte } => {
                if byte.is_ascii_graphic() || byte == b' ' {
                    write!(f, "invalid character {:?} at position {position}", char::from(byte))
                } else {
                    write!(f, "invalid byte 0x{byte:02x} at position {position}")
                }
            }
            Self::GroupOutOfRange { position } => {
                write!(f, "group at position {position} is out of range")
            }
        }
    }
}

impl core::error::Error for DecodeError {}

/// Decodes Base45 text.
///
/// Accepts `&str`, `&[u8]` and anything else that is `AsRef<[u8]>`, since a
/// QR scanner may hand back either.
///
/// # Errors
///
/// Returns a [`DecodeError`] for anything other than the one canonical
/// encoding of some byte sequence; see the [crate docs](crate#strictness).
pub fn decode(text: impl AsRef<[u8]>) -> Result<Vec<u8>, DecodeError> {
    let text = text.as_ref();
    let len = decoded_len(text.len()).ok_or(DecodeError::InvalidLength { len: text.len() })?;
    let mut out = Vec::with_capacity(len);

    let mut groups = text.chunks_exact(3);
    let mut at = 0;
    for group in &mut groups {
        let n = group_value(group, at)?;
        let [_, _, hi, lo] = n.to_be_bytes();
        if n > 0xFFFF {
            return Err(DecodeError::GroupOutOfRange { position: at });
        }
        out.extend_from_slice(&[hi, lo]);
        at += 3;
    }
    let tail = groups.remainder();
    if !tail.is_empty() {
        // `decoded_len` already ruled out a single character.
        let n = group_value(tail, at)?;
        let [_, _, _, lo] = n.to_be_bytes();
        if n > 0xFF {
            return Err(DecodeError::GroupOutOfRange { position: at });
        }
        out.push(lo);
    }
    Ok(out)
}

/// Value of a group of two or three characters, least significant first.
fn group_value(group: &[u8], start: usize) -> Result<u32, DecodeError> {
    let mut n = 0u32;
    let mut weight = 1u32;
    for (i, &byte) in group.iter().enumerate() {
        let digit = DIGITS[usize::from(byte)];
        if digit == INVALID {
            return Err(DecodeError::InvalidCharacter { position: start + i, byte });
        }
        n += u32::from(digit) * weight;
        weight *= 45;
    }
    Ok(n)
}
