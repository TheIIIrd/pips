//! Base45 is strict: anything it accepts encodes back to the same text.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(bytes) = pips_base45::decode(data) {
        assert_eq!(pips_base45::encode(&bytes).as_bytes(), data);
    }
});
