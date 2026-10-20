//! `pips-b45` — Base45 encode or decode standard input or a file.
//!
//! Works like `base64`: raw bytes in, text out with a trailing newline;
//! `-d` reverses it. Because the space character belongs to the Base45
//! alphabet, decoding strips nothing but one final line ending.

use std::io::{self, Read, Write};
use std::process::ExitCode;

const USAGE: &str = "\
Usage: pips-b45 [-d] [FILE]
Base45 (RFC 9285) encode or decode FILE, or standard input, to standard output.

  -d, --decode   decode instead of encode
  -h, --help     show this help
  -V, --version  show version

When decoding, a single trailing line ending is ignored; any other
character outside the Base45 alphabet is an error.";

fn main() -> ExitCode {
    let mut decode = false;
    let mut path = None;
    for arg in std::env::args_os().skip(1) {
        match arg.to_str() {
            Some("-d" | "--decode") => decode = true,
            Some("-h" | "--help") => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            Some("-V" | "--version") => {
                println!("pips-b45 {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            Some(s) if s.starts_with('-') && s != "-" => return usage_error(&format!("unknown option {s}")),
            _ if path.is_some() => return usage_error("more than one file given"),
            _ => path = Some(arg),
        }
    }

    let input = match read_input(path.as_deref()) {
        Ok(bytes) => bytes,
        Err(e) => return fail(&e.to_string()),
    };

    let output = if decode {
        match pips_base45::decode(strip_line_ending(&input)) {
            Ok(bytes) => bytes,
            Err(e) => return fail(&e.to_string()),
        }
    } else {
        let mut text = pips_base45::encode(&input);
        text.push('\n');
        text.into_bytes()
    };

    let mut stdout = io::stdout().lock();
    match stdout.write_all(&output).and_then(|()| stdout.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        // A closed pipe (`pips-b45 big | head`) is the reader's choice.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => fail(&e.to_string()),
    }
}

fn read_input(path: Option<&std::ffi::OsStr>) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    match path {
        None => io::stdin().lock().read_to_end(&mut buf)?,
        Some(p) if p == "-" => io::stdin().lock().read_to_end(&mut buf)?,
        Some(p) => std::fs::File::open(p)?.read_to_end(&mut buf)?,
    };
    Ok(buf)
}

fn strip_line_ending(bytes: &[u8]) -> &[u8] {
    bytes.strip_suffix(b"\r\n").or_else(|| bytes.strip_suffix(b"\n")).unwrap_or(bytes)
}

fn fail(message: &str) -> ExitCode {
    eprintln!("pips-b45: {message}");
    ExitCode::FAILURE
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("pips-b45: {message}\nTry 'pips-b45 --help'.");
    ExitCode::from(2)
}
