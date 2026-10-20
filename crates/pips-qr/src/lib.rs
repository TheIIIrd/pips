//! Pips over QR codes.
//!
//! Everything shown on a screen is Base45 text, because Base45 characters
//! are exactly the QR alphanumeric set, which packs 5.5 bits per character
//! against 8 for byte mode. A message up to [`SINGLE_MAX`] bytes is one code;
//! anything larger goes on a [`carousel`] of codes the camera joins in on.
//!
//! What the bytes *are* — an envelope (`'E'`), a recipient code (`'R'`), a
//! carousel frame (`'F'`) — is told by their first byte, so one scanner
//! handles all of them. This crate does not look inside; `pips-sign` does.
//!
//! The carousel is `no_std`. Drawing symbols ([`matrix`]) needs the `matrix`
//! feature, which uses `qrcodegen` and therefore `std` — available on every
//! target Pips runs on, including `wasm32-unknown-unknown`.

#![cfg_attr(not(any(feature = "matrix", feature = "scan")), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod carousel;
#[cfg(feature = "matrix")]
pub mod matrix;
#[cfg(feature = "scan")]
pub mod scan;

use alloc::string::String;

/// Largest message shown as a single code. About 900 Base45 characters:
/// still reliable screen to screen at error-correction level M.
pub const SINGLE_MAX: usize = 600;

/// Fragment size for carousels: each frame is then about 330 Base45
/// characters — a sparse code a phone reads at a glance.
pub const DEFAULT_FRAGMENT: usize = 200;

/// How a message should be shown.
#[derive(Debug, Clone)]
pub enum Plan {
    /// One code with this text.
    Single(String),
    /// A loop of codes.
    Carousel(carousel::Encoder),
}

/// Chooses between one code and a carousel.
///
/// # Errors
///
/// [`carousel::TooLarge`] for an empty message or one beyond the carousel's
/// limits.
pub fn plan(message: &[u8]) -> Result<Plan, carousel::TooLarge> {
    if message.is_empty() {
        return Err(carousel::TooLarge);
    }
    if message.len() <= SINGLE_MAX {
        return Ok(Plan::Single(pips_base45::encode(message)));
    }
    carousel::Encoder::new(message, DEFAULT_FRAGMENT).map(Plan::Carousel)
}
