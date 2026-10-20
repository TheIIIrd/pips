//! The card format: decoding never panics, and whatever decodes re-encodes
//! to a card that decodes to the same thing (one spelling per card).
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for card in [pips_format::decode(data), pips_format::decode_file(data)].into_iter().flatten() {
        let again = pips_format::encode(&card).expect("a decoded card encodes");
        assert_eq!(pips_format::decode(&again).expect("and decodes again"), card);
        let _ = pips_format::digest(&card);
        let _ = pips_sign::verify_card(&card);
    }
});
