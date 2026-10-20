//! Backups: round trip, ownership, damage, strictness.

use pips_format::{encode, Card, Handle, Network};
use pips_sign::{sign_card, Certificate, SecretKey};
use pips_store::{
    decode, encode as encode_body, open, owner, seal, Backup, Entry, Error, IssueLog, Issued, Place, Profile, Sender,
};

fn key(n: u8) -> SecretKey {
    SecretKey::from_seed(&[n; 32])
}

fn card(author: &SecretKey, title: &str) -> Card {
    let mut c = Card::new(4, 4);
    c.title = title.into();
    c.pixels = (0..16).map(|i| i % 3).collect();
    sign_card(&mut c, author).unwrap();
    c
}

fn entry(payload: Vec<u8>) -> Entry {
    Entry {
        payload,
        mine: false,
        place: Place::Collection,
        received_ms: 1_760_000_000_000,
        binned_ms: None,
        certificate: None,
        sender: None,
        note: String::new(),
        issued: None,
        tags: Vec::new(),
    }
}

/// A backup with one of everything.
fn sample() -> Backup {
    let me = key(1);
    let artist = key(2);
    let friend = key(3);
    let own = card(&me, "Моя");
    let theirs = card(&artist, "Сова");
    let cert = Certificate::issue(&artist, &theirs, 3, 10, me.public_key(), 20_380).unwrap();
    Backup {
        created_ms: 1_760_000_000_000,
        profile: Profile {
            nick: "кицунэ".into(),
            handle: Some(Handle { network: Network::Telegram, text: "@kitsu".into() }),
            link: "kitsu.art/shop".into(),
        },
        cards: vec![
            Entry {
                mine: true,
                issued: Some(IssueLog {
                    of: 10,
                    copies: vec![
                        Issued {
                            edition: 1,
                            day: 20_380,
                            holder: friend.public_key().to_bytes(),
                            nick: "mossfern".into(),
                        },
                        Issued {
                            edition: 2,
                            day: 20_381,
                            holder: artist.public_key().to_bytes(),
                            nick: "dapple".into(),
                        },
                    ],
                }),
                ..entry(encode(&own).unwrap())
            },
            Entry {
                certificate: Some(cert.to_bytes()),
                note: "с фестиваля, стол 12".into(),
                tags: vec!["фестиваль".into(), "совы".into()],
                ..entry(encode(&theirs).unwrap())
            },
            Entry {
                place: Place::Bin,
                binned_ms: Some(1_760_100_000_000),
                sender: Some(Sender { key: friend.public_key().to_bytes(), nick: "mossfern".into() }),
                ..entry(encode(&card(&artist, "Лиса")).unwrap())
            },
            Entry { place: Place::Archive, ..entry(encode(&card(&friend, "Дом")).unwrap()) },
        ],
    }
}

const NONCE: [u8; 24] = [7; 24];

#[test]
fn round_trip() {
    let b = sample();
    let file = seal(&b, &key(1), NONCE).unwrap();
    assert_eq!(open(&file, &key(1)).unwrap(), b);
    assert_eq!(owner(&file).unwrap(), key(1).public_key().to_bytes());
}

#[test]
fn the_same_words_open_it_on_another_device() {
    let file = seal(&sample(), &key(1), NONCE).unwrap();
    let words = pips_sign::recovery::to_words(&key(1));
    let restored = pips_sign::recovery::from_words(&words).unwrap();
    assert_eq!(open(&file, &restored).unwrap(), sample());
}

#[test]
fn another_key_is_told_whose_backup_it_is() {
    let file = seal(&sample(), &key(1), NONCE).unwrap();
    assert_eq!(open(&file, &key(9)), Err(Error::OtherOwner(key(1).public_key().to_bytes())));
}

#[test]
fn nothing_is_readable_without_the_key() {
    let b = sample();
    let file = seal(&b, &key(1), NONCE).unwrap();
    let text = String::from_utf8_lossy(&file);
    for needle in ["кицунэ", "kitsu", "фестиваля", "mossfern"] {
        assert!(!text.contains(needle), "{needle} visible in the file");
    }
    // A fresh nonce gives an unrelated file.
    let other = seal(&b, &key(1), [8; 24]).unwrap();
    assert_ne!(file[61..], other[61..]);
}

#[test]
fn every_flipped_bit_is_caught() {
    let mut b = sample();
    b.cards.truncate(1); // keeps the test quick: every bit of the file
    let file = seal(&b, &key(1), NONCE).unwrap();
    for i in 0..file.len() * 8 {
        let mut f = file.clone();
        f[i / 8] ^= 1 << (i % 8);
        let r = open(&f, &key(1));
        assert!(r.is_err(), "bit {i} flipped and the backup still opened");
        if i / 8 >= 37 {
            assert_eq!(r, Err(Error::Damaged), "bit {i}"); // nonce and body
        }
    }
}

#[test]
fn every_truncation_is_refused() {
    let file = seal(&sample(), &key(1), NONCE).unwrap();
    for n in 0..file.len() {
        assert!(open(&file[..n], &key(1)).is_err(), "cut at {n}");
    }
    let mut longer = file.clone();
    longer.push(0);
    assert_eq!(open(&longer, &key(1)), Err(Error::Damaged));
}

#[test]
fn header_errors_are_told_apart() {
    let file = seal(&sample(), &key(1), NONCE).unwrap();
    assert_eq!(owner(b"PK\x03\x04 this is a zip"), Err(Error::NotABackup));
    let mut newer = file.clone();
    newer[4] = 3;
    assert_eq!(owner(&newer), Err(Error::NewerVersion(3)));
}

// ------------------------------------------------------------ strictness --

fn body() -> Vec<u8> {
    encode_body(&sample()).unwrap()
}

/// Offset of the first entry's flags byte in `body()`.
fn first_flags() -> usize {
    let b = body();
    let profile = 8 + 1 + "кицунэ".len() + 1 + 1 + "@kitsu".len() + 1 + "kitsu.art/shop".len();
    let payload = usize::from(u16::from_le_bytes([b[profile + 4], b[profile + 5]]));
    profile + 4 + 2 + payload
}

#[test]
fn the_body_is_strict() {
    let good = body();
    assert_eq!(decode(&good).unwrap(), sample());

    let mut b = good.clone();
    b.push(0);
    assert_eq!(decode(&b), Err(Error::Invalid("data after the last card")));

    let f = first_flags();
    let mut b = good.clone();
    b[f] |= 0x80;
    assert_eq!(decode(&b), Err(Error::Invalid("reserved flag bits")));

    let mut b = good.clone();
    b[f] |= 0b110; // place 3
    assert_eq!(decode(&b), Err(Error::Invalid("unknown place")));

    let mut b = good.clone();
    b[f] |= 0b100; // the bin, but no bin time
    assert_eq!(decode(&b), Err(Error::Invalid("bin time without the bin, or the reverse")));

    // A tampered card inside (its signature, just before the flags).
    let mut b = good;
    b[f - 30] ^= 1;
    assert!(decode(&b).is_err());
}

#[test]
fn a_huge_count_is_refused_before_allocating() {
    let b = body();
    let profile = 8 + 1 + "кицунэ".len() + 1 + 1 + "@kitsu".len() + 1 + "kitsu.art/shop".len();
    for count in [100_000u32, 100_001, u32::MAX] {
        let mut x = b.clone();
        x[profile..profile + 4].copy_from_slice(&count.to_le_bytes());
        assert!(matches!(decode(&x), Err(Error::TooLarge | Error::Invalid(_))), "count {count}");
    }
}

#[test]
fn a_certificate_must_belong_to_its_card() {
    let mut b = sample();
    let other = card(&key(2), "Другая");
    let cert = Certificate::issue(&key(2), &other, 1, 1, key(1).public_key(), 1).unwrap();
    b.cards[1].certificate = Some(cert.to_bytes());
    assert_eq!(seal(&b, &key(1), NONCE), Err(Error::Invalid("a certificate does not match its card")));
}

#[test]
fn own_cards_must_be_signed_by_the_owner() {
    let mut b = sample();
    b.cards[3].mine = true; // signed by key(3)
    let file = seal(&b, &key(1), NONCE).unwrap();
    assert_eq!(open(&file, &key(1)), Err(Error::Invalid("an own card signed by another key")));
}

#[test]
fn what_seal_writes_always_opens() {
    let mut b = sample();
    b.cards[0].note = "x".repeat(4001);
    assert_eq!(seal(&b, &key(1), NONCE), Err(Error::Invalid("a note is too long")));
    let mut b = sample();
    b.profile.link = "Kitsu.art".into();
    assert_eq!(seal(&b, &key(1), NONCE), Err(Error::Invalid("the profile breaks the card rules")));
    let mut b = sample();
    b.profile.nick = "очень-очень-длинный-ник".into(); // 44 bytes
    assert!(seal(&b, &key(1), NONCE).is_err());
}

#[test]
fn an_empty_backup_is_fine() {
    let b = Backup { created_ms: 0, profile: Profile::default(), cards: vec![] };
    assert_eq!(open(&seal(&b, &key(1), NONCE).unwrap(), &key(1)).unwrap(), b);
}

#[test]
fn the_issue_log_is_checked() {
    let mut b = sample();
    b.cards[0].issued.as_mut().unwrap().copies[1].edition = 3;
    assert_eq!(seal(&b, &key(1), NONCE), Err(Error::Invalid("copies out of order")));
    let mut b = sample();
    b.cards[0].issued.as_mut().unwrap().of = 1;
    assert_eq!(seal(&b, &key(1), NONCE), Err(Error::Invalid("more copies than the run")));
    let mut b = sample();
    b.cards[1].issued = b.cards[0].issued.clone();
    assert_eq!(seal(&b, &key(1), NONCE), Err(Error::Invalid("an issue log on someone else's card")));
}

#[test]
fn a_version_1_body_opens_without_tags() {
    // Version 2 only adds a tag count (then the tags) after each entry, so a
    // one-card backup without tags is a version 1 body plus one zero byte.
    let mut b = sample();
    b.cards.truncate(1);
    b.cards[0].tags.clear();
    let v2 = encode_body(&b).unwrap();
    assert_eq!(*v2.last().unwrap(), 0);
    assert_eq!(pips_store::decode_version(&v2[..v2.len() - 1], 1).unwrap(), b);
    assert_eq!(decode(&v2).unwrap(), b);
}

#[test]
fn tags_follow_the_rules() {
    for (bad, why) in [
        ("", "empty"),
        ("Cats", "capital"),
        ("two words", "space"),
        ("#cats", "hash"),
        ("ааааааааааааааааа", "over 32 bytes"),
    ] {
        assert!(pips_store::check_tag(bad).is_err(), "{why}");
    }
    assert!(pips_store::check_tag("котики").is_ok());
    let mut b = sample();
    b.cards[1].tags = vec!["a".into(), "a".into()];
    let body = encode_body(&b).unwrap();
    assert_eq!(decode(&body), Err(Error::Invalid("a tag twice")));
    b.cards[1].tags = (0..6).map(|i| format!("t{i}")).collect();
    let body = encode_body(&b).unwrap();
    assert_eq!(decode(&body), Err(Error::Invalid("too many tags")));
}
