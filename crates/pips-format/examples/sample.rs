//! Writes `sample.pips`, the first sample card, signed with a dummy key,
//! for trying the command-line tools.

fn main() {
    let line = include_str!("../tests/data/samples.txt").lines().nth(1).unwrap();
    let px = line.rsplit(' ').next().unwrap();
    let mut card = pips_format::Card::new(32, 32);
    card.opaque = true;
    card.pixels = px.bytes().map(|b| u8::try_from((b as char).to_digit(16).unwrap()).unwrap()).collect();
    card.title = "Лис у костра".into();
    card.author = "kitsu".into();
    card.handle = Some(pips_format::Handle { network: pips_format::Network::Telegram, text: "@kitsu".into() });
    card.link = "kitsu.art".into();
    card.signature = Some(pips_format::Signature { public_key: [0; 32], bytes: [0; 64] });
    std::fs::write("sample.pips", pips_format::encode_file(&card).unwrap()).unwrap();
}
