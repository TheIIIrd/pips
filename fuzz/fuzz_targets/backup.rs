//! Backups: the decrypted body (where a real attacker cannot reach without
//! the key, but bugs can), and whole files against a fixed key.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = pips_store::decode(data);
    let key = pips_sign::SecretKey::from_seed(&[7; 32]);
    let _ = pips_store::open(data, &key);
    let _ = pips_store::owner(data);
});
