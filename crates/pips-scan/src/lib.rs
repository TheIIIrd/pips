//! QR reading for browsers without `BarcodeDetector`, as its own small
//! WebAssembly module (see `Cargo.toml` for why it is separate).
#![forbid(unsafe_code)]

use wasm_bindgen::prelude::*;

/// Texts of the QR symbols in an RGBA frame (`ImageData.data`). The app
/// classifies each one like any other scan; nothing here is trusted.
#[wasm_bindgen(js_name = scanQr)]
#[must_use]
pub fn scan_qr(rgba: &[u8], width: u32, height: u32) -> Vec<String> {
    pips_qr::scan::read_rgba(rgba, width as usize, height as usize)
}
