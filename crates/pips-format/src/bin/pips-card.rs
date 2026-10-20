//! `pips-card` — inspect a card: a `.pips` file or a bare payload.
//!
//! ```sh
//! pips-card inspect card.pips
//! pips-b45 -d < scanned.txt | pips-card inspect
//! ```

use std::fmt::Write as _;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use pips_format::{decode, decode_file, digest, limits::MAX_PAYLOAD, PaletteRef, MAGIC};

const USAGE: &str = "\
Usage: pips-card inspect [FILE]
Check a Pips card and describe it. FILE may be a .pips file or a bare
payload (as carried by a QR code); standard input if omitted.

  -h, --help     show this help
  -V, --version  show version

Exit status: 0 valid card, 1 invalid or unreadable, 2 usage error.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("-h" | "--help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some("-V" | "--version") => {
            println!("pips-card {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some("inspect") if args.len() <= 2 => {}
        Some("inspect") => return usage_error("too many arguments"),
        Some(other) => return usage_error(&format!("unknown command {other}")),
        None => return usage_error("no command given"),
    }

    // One byte over the largest valid input is enough to refuse it.
    let limit = (MAX_PAYLOAD + MAGIC.len() + 1) as u64;
    let mut bytes = Vec::new();
    let read = match args.get(1).map(String::as_str) {
        None | Some("-") => io::stdin().lock().take(limit).read_to_end(&mut bytes),
        Some(p) => std::fs::File::open(p).and_then(|f| f.take(limit).read_to_end(&mut bytes)),
    };
    if let Err(e) = read {
        return fail(&e.to_string());
    }

    let is_file = bytes.starts_with(&MAGIC);
    let card = match if is_file { decode_file(&bytes) } else { decode(&bytes) } {
        Ok(c) => c,
        Err(e) => return fail(&format!("invalid card: {e}")),
    };

    let mut out = String::new();
    let _ = writeln!(out, "kind      {}", if is_file { ".pips file" } else { "bare payload" });
    let _ = writeln!(out, "size      {} bytes", bytes.len());
    let _ = writeln!(out, "canvas    {}x{}, {} frame(s)", card.width, card.height, card.frames());
    if let Some(a) = card.animation {
        let _ = writeln!(out, "delay     {} ms", a.delay_ms);
    }
    let palette = match card.palette {
        PaletteRef::Builtin(id) => {
            let b = pips_palette::builtin(id).map_or("?", |b| b.slug);
            format!("built-in {id} ({b})")
        }
        PaletteRef::Inline(p) => format!("inline, {} colours", p.len()),
    };
    let _ = writeln!(out, "palette   {palette}{}", if card.opaque { ", opaque" } else { "" });
    let _ = writeln!(out, "frame     {}", card.frame);
    let _ = writeln!(out, "title     {:?}", card.title);
    let _ = writeln!(out, "author    {:?}", card.author);
    if let Some(h) = &card.handle {
        let _ = writeln!(out, "handle    {:?} {:?}", h.network, h.text);
    }
    if !card.link.is_empty() {
        // Printed as text, never as a clickable URL.
        let _ = writeln!(out, "link      {:?}", card.link);
    }
    for e in &card.extensions {
        let _ =
            writeln!(out, "extension {} ({} bytes{})", e.tag, e.data.len(), if e.critical { ", critical" } else { "" });
    }
    let signed = if card.signature.is_some() { "yes (not verified here)" } else { "no" };
    let _ = writeln!(out, "signed    {signed}");
    match digest(&card) {
        Ok(d) => {
            out += "digest    ";
            for b in d {
                let _ = write!(out, "{b:02x}");
            }
            out.push('\n');
        }
        Err(e) => return fail(&e.to_string()),
    }

    let mut stdout = io::stdout().lock();
    match stdout.write_all(out.as_bytes()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => fail(&e.to_string()),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("pips-card: {message}");
    ExitCode::FAILURE
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("pips-card: {message}\nTry 'pips-card --help'.");
    ExitCode::from(2)
}
