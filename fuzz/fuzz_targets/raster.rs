//! PNG and Aseprite files, decoded and then converted the way the app does:
//! exact colours, and the fitted palette offered for many-coloured pictures.
#![no_main]
use libfuzzer_sys::fuzz_target;
use pips_raster::{import, Options, Target};

fuzz_target!(|data: &[u8]| {
    if pips_raster::decode(data).is_ok() {
        for target in [Target::Auto, Target::Reduce(16)] {
            if let Ok((picture, _)) = import(data, &Options { target, ..Options::default() }) {
                let mut card = pips_format::Card::new(1, 1);
                picture.apply_to(&mut card);
                pips_format::encode(&card).expect("an imported picture is a valid card");
            }
        }
    }
});
