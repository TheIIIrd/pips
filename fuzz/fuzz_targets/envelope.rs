//! Everything a scanned code can be: envelope, certificate, recipient code.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pips_sign::open(data);
    let _ = pips_sign::Certificate::from_bytes(data);
    let _ = pips_sign::RecipientCode::from_bytes(data);
    if let Ok(text) = core::str::from_utf8(data) {
        let _ = pips_sign::recovery::from_words(text);
        let _ = pips_sign::recovery::from_paper(text);
    }
});
