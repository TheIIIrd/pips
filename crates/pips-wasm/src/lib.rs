//! The Pips core for the web app.
//!
//! [`api`] is a single facade over the format, palettes, signatures and QR
//! crates, in plain data types; it is ordinary Rust, tested natively.
//! [`bindings`] exposes it to JavaScript through `wasm-bindgen` and does
//! nothing else — every function there is a one-line conversion.
//!
//! Build (needs the `wasm32-unknown-unknown` target and a `wasm-bindgen-cli`
//! matching the `wasm-bindgen` version in `Cargo.lock`):
//!
//! ```sh
//! cargo build -p pips-wasm --release --target wasm32-unknown-unknown
//! wasm-bindgen --target web --out-dir web/pkg \
//!     target/wasm32-unknown-unknown/release/pips_wasm.wasm
//! ```

#![forbid(unsafe_code)]

pub mod api;
mod b64;
pub mod bindings;
