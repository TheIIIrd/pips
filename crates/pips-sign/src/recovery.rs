//! Getting a key back: 24 words, or a paper QR.
//!
//! Both are the 32-byte seed itself, in two spellings. The words are BIP39
//! (English list): 256 bits of seed plus an 8-bit checksum. Unlike a
//! cryptocurrency wallet, the words are *not* stretched through PBKDF2 into
//! a different seed — they are the seed, so the words and the paper code are
//! interchangeable and either can be checked against the other.
//!
//! Neither is encrypted. Anyone who reads them can sign as you: keep them
//! like a passport.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use sha2::{Digest as _, Sha256};
use zeroize::Zeroizing;

use crate::SecretKey;

/// Prefix of a paper key. All characters are in the QR alphanumeric set.
pub const PAPER_PREFIX: &str = "PIPS:KEY:1:";

const PAPER_DOMAIN: &[u8] = b"pips-v1:paper";

/// Why words or a paper code do not give a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecoveryError {
    /// Not 24 words.
    WordCount(usize),
    /// The word at this 0-based position is not in the English BIP39 list.
    UnknownWord(usize),
    /// Every word is valid but the checksum fails: a word is wrong or two
    /// are swapped.
    Checksum,
    /// Not a Pips paper key, or damaged.
    BadPaperKey,
}

impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::WordCount(n) => write!(f, "expected 24 words, got {n}"),
            Self::UnknownWord(i) => write!(f, "word {} is not in the list", i + 1),
            Self::Checksum => f.write_str("the words do not add up: one is wrong or out of order"),
            Self::BadPaperKey => f.write_str("not a Pips paper key"),
        }
    }
}

impl core::error::Error for RecoveryError {}

/// The 24 words for a key, separated by single spaces.
///
/// # Panics
///
/// Never: 32 bytes is always a valid BIP39 entropy length.
#[must_use]
pub fn to_words(key: &SecretKey) -> Zeroizing<String> {
    let seed = key.seed();
    // 32 bytes is always a valid BIP39 entropy length.
    let m = bip39::Mnemonic::from_entropy(seed.as_ref()).expect("32-byte entropy is valid");
    let mut out = Zeroizing::new(String::with_capacity(24 * 9));
    for (i, w) in m.words().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(w);
    }
    out
}

/// The key for 24 words. Case and extra whitespace are forgiven, since
/// people type these by hand; nothing else is.
///
/// # Errors
///
/// A [`RecoveryError`] saying which word or what is wrong.
pub fn from_words(text: &str) -> Result<SecretKey, RecoveryError> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() != 24 {
        return Err(RecoveryError::WordCount(words.len()));
    }
    let mut normal = Zeroizing::new(String::with_capacity(text.len()));
    for (i, w) in words.iter().enumerate() {
        if !w.is_ascii() {
            return Err(RecoveryError::UnknownWord(i));
        }
        if i > 0 {
            normal.push(' ');
        }
        normal.push_str(&w.to_ascii_lowercase());
    }
    let m = bip39::Mnemonic::parse_in_normalized(bip39::Language::English, &normal).map_err(|e| match e {
        bip39::Error::UnknownWord(i) => RecoveryError::UnknownWord(i),
        bip39::Error::BadWordCount(n) => RecoveryError::WordCount(n),
        _ => RecoveryError::Checksum,
    })?;
    let (entropy, len) = m.to_entropy_array();
    let entropy = Zeroizing::new(entropy);
    let seed: &[u8; 32] = entropy[..len].try_into().map_err(|_| RecoveryError::WordCount(24))?;
    Ok(SecretKey::from_seed(seed))
}

/// The paper form: [`PAPER_PREFIX`], then Base45 of the seed and a two-byte
/// check. About 55 characters — a small, easily scanned QR code.
#[must_use]
pub fn to_paper(key: &SecretKey) -> Zeroizing<String> {
    let seed = key.seed();
    let mut body = Zeroizing::new([0u8; 34]);
    body[..32].copy_from_slice(seed.as_ref());
    body[32..].copy_from_slice(&paper_check(&seed));
    let mut out = Zeroizing::new(String::from(PAPER_PREFIX));
    pips_base45::encode_into(body.as_ref(), &mut out);
    out
}

/// The key from its paper form.
///
/// # Errors
///
/// [`RecoveryError::BadPaperKey`] for anything else, including a code whose
/// check bytes do not match.
pub fn from_paper(text: &str) -> Result<SecretKey, RecoveryError> {
    let b45 = text.trim().strip_prefix(PAPER_PREFIX).ok_or(RecoveryError::BadPaperKey)?;
    let body = Zeroizing::new(pips_base45::decode(b45).map_err(|_| RecoveryError::BadPaperKey)?);
    if body.len() != 34 {
        return Err(RecoveryError::BadPaperKey);
    }
    let seed: &[u8; 32] = body[..32].try_into().map_err(|_| RecoveryError::BadPaperKey)?;
    if body[32..] != paper_check(seed) {
        return Err(RecoveryError::BadPaperKey);
    }
    Ok(SecretKey::from_seed(seed))
}

fn paper_check(seed: &[u8; 32]) -> [u8; 2] {
    let h = Sha256::new().chain_update(PAPER_DOMAIN).chain_update(seed).finalize();
    [h[0], h[1]]
}
