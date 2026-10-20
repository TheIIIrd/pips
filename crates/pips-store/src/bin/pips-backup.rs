//! `pips-backup` — look inside a Pips backup.
//!
//! ```sh
//! pips-backup owner backup.pipsbak           # whose backup, without the key
//! pips-backup show -k me.key backup.pipsbak  # decrypt, check, list
//! ```

use std::fmt::Write as _;
use std::io::Read;
use std::process::ExitCode;

use pips_sign::recovery::{from_paper, from_words, PAPER_PREFIX};
use pips_store::{open, owner, Place, MAX_FILE};

const USAGE: &str = "\
Usage: pips-backup owner FILE
       pips-backup show -k KEYFILE FILE

  owner  print the public key the backup belongs to (no key needed)
  show   decrypt with the key in KEYFILE (24 words or paper-key text),
         check every card and certificate, and list the contents

Exit status: 0 success, 1 invalid backup, wrong key or I/O error, 2 usage error.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, key, file) = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["-h" | "--help"] => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        ["-V" | "--version"] => {
            println!("pips-backup {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        ["owner", f] => ("owner", None, f),
        ["show", "-k", k, f] => ("show", Some(k), f),
        _ => {
            eprintln!("pips-backup: bad arguments\nTry 'pips-backup --help'.");
            return ExitCode::from(2);
        }
    };
    let mut bytes = Vec::new();
    if let Err(e) = std::fs::File::open(file).and_then(|f| f.take(MAX_FILE as u64 + 1).read_to_end(&mut bytes)) {
        return fail(&format!("{file}: {e}"));
    }
    let hex = |k: &[u8]| {
        k.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    };
    if cmd == "owner" {
        return match owner(&bytes) {
            Ok(k) => {
                println!("{}", hex(&k));
                ExitCode::SUCCESS
            }
            Err(e) => fail(&format!("{file}: {e}")),
        };
    }
    let key_path = key.unwrap_or_default();
    let text = match std::fs::read_to_string(key_path) {
        Ok(t) => zeroize::Zeroizing::new(t),
        Err(e) => return fail(&format!("{key_path}: {e}")),
    };
    let key = match if text.trim_start().starts_with(PAPER_PREFIX) { from_paper(&text) } else { from_words(&text) } {
        Ok(k) => k,
        Err(e) => return fail(&format!("{key_path}: {e}")),
    };
    let b = match open(&bytes, &key) {
        Ok(b) => b,
        Err(e) => return fail(&format!("{file}: {e}")),
    };
    println!("created   {} ms since 1970", b.created_ms);
    println!("nick      {}", b.profile.nick);
    let count = |p: Place| b.cards.iter().filter(|e| e.place == p).count();
    println!(
        "cards     {} ({} own; {} in the collection, {} archived, {} in the bin)",
        b.cards.len(),
        b.cards.iter().filter(|e| e.mine).count(),
        count(Place::Collection),
        count(Place::Archive),
        count(Place::Bin)
    );
    println!("editions  {}", b.cards.iter().filter(|e| e.certificate.is_some()).count());
    println!("notes     {}", b.cards.iter().filter(|e| !e.note.is_empty()).count());
    ExitCode::SUCCESS
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("pips-backup: {msg}");
    ExitCode::FAILURE
}
