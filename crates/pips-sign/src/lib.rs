//! Identity and trust for Pips.
//!
//! - **Keys** ([`SecretKey`], [`PublicKey`]): Ed25519. The secret is a
//!   32-byte seed; nothing else is ever stored.
//! - **Recovery** ([`recovery`]): the same 32 bytes as 24 English BIP39 words
//!   and as a paper QR text. Either brings the key back on a new phone.
//! - **Card signatures** ([`sign_card`], [`verify_card`]): over the card's
//!   canonical form, so a signature survives re-encoding by another app.
//! - **Edition certificates** ([`Certificate`]): "copy N of M of this card,
//!   issued to key X", signed by the card's author. Copies of the picture are
//!   free; the certificate is what makes one of them *the* copy N.
//! - **Recipient codes** ([`RecipientCode`]): what a person shows to receive
//!   a certified copy — their key and nickname.
//! - **Envelopes** ([`seal`], [`open`]): how a card travels — the card,
//!   optionally a certificate, and optionally a signed "from" when the
//!   sender is not the author.
//!
//! Every signature is domain-separated (`pips-v1:card`, `pips-v1:edition`,
//! `pips-v1:envelope`), so a signature made for one purpose can never be
//! replayed as another. Verification uses `verify_strict`, which refuses
//! small-order keys and malleable signatures.
//!
//! This crate never touches a random number generator: callers pass a seed
//! from the platform's CSPRNG. That keeps it `no_std` and makes every
//! operation deterministic and testable.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod objects;
pub mod recovery;

use core::fmt;

use ed25519_dalek::{Signer as _, SigningKey, VerifyingKey};
use zeroize::Zeroizing;

pub use objects::{open, seal, sign_card, verify_card, Certificate, Opened, RecipientCode, Sender, CERTIFICATE_LEN};

/// An Ed25519 signing key. Wiped from memory when dropped.
pub struct SecretKey(SigningKey);

impl SecretKey {
    /// The key for a 32-byte seed. The seed must come from a
    /// cryptographically secure random source, or from [`recovery`].
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self(SigningKey::from_bytes(seed))
    }

    /// The seed, for backup. Wiped when the returned value is dropped.
    #[must_use]
    pub fn seed(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.0.to_bytes())
    }

    /// The matching public key.
    #[must_use]
    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.verifying_key().to_bytes())
    }

    fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.0.sign(message).to_bytes()
    }
}

/// Never prints the secret.
impl fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretKey(public {})", self.public_key())
    }
}

/// An Ed25519 public key that has been checked to be a valid, non-weak point.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PublicKey([u8; 32]);

impl PublicKey {
    /// Checks and wraps 32 bytes.
    ///
    /// # Errors
    ///
    /// [`Error::BadKey`] for bytes that are not a point on the curve, or are
    /// a small-order point, under which signatures prove nothing.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, Error> {
        let key = VerifyingKey::from_bytes(bytes).map_err(|_| Error::BadKey)?;
        if key.is_weak() {
            return Err(Error::BadKey);
        }
        Ok(Self(*bytes))
    }

    /// The raw 32 bytes.
    #[must_use]
    pub const fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    /// The first four bytes as `4f·a1·c8·0e`, for showing people. Not unique;
    /// use the whole key to compare.
    #[must_use]
    pub fn short(&self) -> ShortKey {
        ShortKey([self.0[0], self.0[1], self.0[2], self.0[3]])
    }

    fn verify(&self, message: &[u8], signature: &[u8; 64]) -> Result<(), Error> {
        let key = VerifyingKey::from_bytes(&self.0).map_err(|_| Error::BadKey)?;
        let sig = ed25519_dalek::Signature::from_bytes(signature);
        key.verify_strict(message, &sig).map_err(|_| Error::BadSignature)
    }
}

/// Lowercase hex of all 32 bytes.
impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({self})")
    }
}

/// See [`PublicKey::short`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortKey([u8; 4]);

impl fmt::Display for ShortKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let k = self.0;
        write!(f, "{:02x}·{:02x}·{:02x}·{:02x}", k[0], k[1], k[2], k[3])
    }
}

/// Why something does not verify or parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The card itself is invalid.
    Format(pips_format::Error),
    /// The card carries no signature where one is required.
    Unsigned,
    /// A signature does not match.
    BadSignature,
    /// Bytes that are not a usable public key.
    BadKey,
    /// Only a card's author may issue editions of it.
    NotAuthor,
    /// Edition 0, or an edition above the run size.
    BadEdition,
    /// A nickname too long, or with control characters or surrounding spaces.
    BadNick,
    /// Not the kind of object expected (first byte).
    WrongKind,
    /// A version this crate does not read.
    UnsupportedVersion(u8),
    /// A flag this version does not know.
    UnknownFlag(u8),
    /// Input ended inside a field.
    Truncated,
    /// Bytes after the last field.
    TrailingBytes,
}

impl From<pips_format::Error> for Error {
    fn from(e: pips_format::Error) -> Self {
        Self::Format(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(e) => write!(f, "invalid card: {e}"),
            Self::Unsigned => f.write_str("card is not signed"),
            Self::BadSignature => f.write_str("signature does not match"),
            Self::BadKey => f.write_str("not a valid public key"),
            Self::NotAuthor => f.write_str("only the card's author can issue editions"),
            Self::BadEdition => f.write_str("edition number out of range"),
            Self::BadNick => f.write_str("bad nickname"),
            Self::WrongKind => f.write_str("not the expected kind of code"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported version {v}"),
            Self::UnknownFlag(b) => write!(f, "unknown flags 0x{b:02x}"),
            Self::Truncated => f.write_str("truncated"),
            Self::TrailingBytes => f.write_str("trailing bytes"),
        }
    }
}

impl core::error::Error for Error {}
