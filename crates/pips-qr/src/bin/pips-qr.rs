//! `pips-qr` — show bytes as QR codes, cut them into carousel frames, join frames back.
//!
//! ```sh
//! pips-sign seal -k me.key card.pips | pips-qr show
//! pips-qr frames big.bin | shuf | head -n 20 | pips-qr join > back.bin
//! ```

use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::process::ExitCode;

use pips_qr::carousel::{Decoder, Encoder, Progress};
use pips_qr::matrix::Matrix;
use pips_qr::{DEFAULT_FRAGMENT, SINGLE_MAX};

const USAGE: &str = "\
Usage: pips-qr show [--svg] [FILE]
       pips-qr frames [-f BYTES] [-c COUNT] [-s START] [FILE]
       pips-qr join

  show     draw bytes (up to 600) as one QR code on the terminal, or as SVG
  frames   cut bytes into carousel frames; print each as a Base45 line
           (-f fragment size, default 200; -c how many, default twice the
           minimum; -s first frame number, default 0)
  join     read Base45 frame lines from standard input until the message
           is complete, then write it to standard output

Input defaults to standard input. Binary output is refused on a terminal.
Exit status: 0 success, 1 bad input or incomplete message, 2 usage error.";

const MAX_INPUT: u64 = 70 * 1024;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, m)) => {
            eprintln!("pips-qr: {m}");
            if code == 2 {
                eprintln!("Try 'pips-qr --help'.");
            }
            ExitCode::from(code)
        }
    }
}

type Res<T> = Result<T, (u8, String)>;

fn usage<T>(m: &str) -> Res<T> {
    Err((2, m.to_owned()))
}

fn fail<E: std::fmt::Display>(e: E) -> (u8, String) {
    (1, e.to_string())
}

fn run() -> Res<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else { return usage("no command given") };
    let mut svg = false;
    let (mut frag, mut count, mut start, mut input) = (DEFAULT_FRAGMENT, None, 0u32, None);
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        let mut num = |name: &str| -> Res<usize> {
            it.next().and_then(|v| v.parse().ok()).map_or_else(|| usage(&format!("{name} needs a number")), Ok)
        };
        match a.as_str() {
            "--svg" => svg = true,
            "-f" => frag = num("-f")?,
            "-c" => count = Some(num("-c")?),
            "-s" => start = u32::try_from(num("-s")?).or_else(|_| usage("-s is too large"))?,
            s if s.starts_with('-') && s != "-" => return usage(&format!("unknown option {s}")),
            _ if input.is_some() => return usage("too many arguments"),
            _ => input = Some(a.clone()),
        }
    }
    match cmd.as_str() {
        "-h" | "--help" => {
            println!("{USAGE}");
            Ok(())
        }
        "-V" | "--version" => {
            println!("pips-qr {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "show" => {
            let bytes = read_input(input.as_deref())?;
            if bytes.is_empty() {
                return Err(fail("no input"));
            }
            if bytes.len() > SINGLE_MAX {
                return Err(fail(format!(
                    "{} bytes is more than one code holds ({SINGLE_MAX}); use 'pips-qr frames'",
                    bytes.len()
                )));
            }
            let m = Matrix::encode(&pips_base45::encode(&bytes)).map_err(fail)?;
            let out = if svg { m.to_svg(4) } else { m.to_terminal(2) };
            emit(out.as_bytes())
        }
        "frames" => {
            let bytes = read_input(input.as_deref())?;
            let enc = Encoder::new(&bytes, frag).map_err(fail)?;
            let count = count.unwrap_or(enc.fragments() * 2);
            let mut out = String::new();
            for i in 0..count {
                let seq =
                    start.checked_add(u32::try_from(i).map_err(fail)?).ok_or_else(|| fail("frame number overflow"))?;
                out += &pips_base45::encode(&enc.frame(seq));
                out.push('\n');
            }
            emit(out.as_bytes())
        }
        "join" => {
            let mut d = Decoder::new();
            for (n, line) in io::stdin().lock().lines().enumerate() {
                let line = line.map_err(fail)?;
                if line.is_empty() {
                    continue;
                }
                let frame = pips_base45::decode(&line).map_err(|e| fail(format!("line {}: {e}", n + 1)))?;
                match d.push(&frame).map_err(|e| fail(format!("line {}: {e}", n + 1)))? {
                    Progress::Done(msg) => {
                        if io::stdout().is_terminal() {
                            return Err(fail("refusing to write binary to a terminal"));
                        }
                        return emit(&msg);
                    }
                    Progress::Partial { have, need } => eprintln!("pips-qr: {have}/{need}"),
                }
            }
            Err(fail("input ended before the message was complete"))
        }
        other => usage(&format!("unknown command {other}")),
    }
}

fn read_input(path: Option<&str>) -> Res<Vec<u8>> {
    let mut buf = Vec::new();
    match path {
        None | Some("-") => io::stdin().lock().take(MAX_INPUT + 1).read_to_end(&mut buf),
        Some(p) => std::fs::File::open(p).and_then(|f| f.take(MAX_INPUT + 1).read_to_end(&mut buf)),
    }
    .map_err(fail)?;
    if buf.len() as u64 > MAX_INPUT {
        return Err(fail("input too large"));
    }
    Ok(buf)
}

fn emit(bytes: &[u8]) -> Res<()> {
    let mut out = io::stdout().lock();
    match out.write_all(bytes).and_then(|()| out.flush()) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(fail(e)),
        _ => Ok(()),
    }
}
