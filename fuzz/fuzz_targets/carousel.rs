//! The carousel decoder fed a stream of frames: the input is split into
//! frames by length bytes, as a camera would deliver them one by one.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut d = pips_qr::carousel::Decoder::new();
    let mut rest = data;
    while let Some((&n, tail)) = rest.split_first() {
        let n = usize::from(n).min(tail.len());
        let (frame, next) = tail.split_at(n);
        let _ = d.push(frame);
        rest = next;
    }
});
