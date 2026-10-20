//! `pips-palette` — list built-in palettes, print them, convert palette files.

use std::fmt::Write as _;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use pips_palette::{builtin, builtin_by_slug, parse, to_gpl, to_hex, Palette, BUILTINS, MAX_INPUT_BYTES};

const USAGE: &str = "\
Usage: pips-palette list
       pips-palette show [-f hex|gpl] ID|SLUG
       pips-palette convert [-f hex|gpl] [FILE]

  list      built-in palettes: id, slug, number of colours, name, credit
  show      print a built-in palette (default format: hex)
  convert   read a .hex or .gpl palette from FILE or standard input,
            check it, and print it in the chosen format (default: hex)

  -f, --format   hex (Lospec) or gpl (GIMP, Aseprite)
  -h, --help     show this help
  -V, --version  show version";

#[derive(Clone, Copy)]
enum Format {
    Hex,
    Gpl,
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        return usage_error("no command given");
    };
    match command.as_str() {
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        "-V" | "--version" => {
            println!("pips-palette {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    let mut format = Format::Hex;
    let mut operand: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-f" | "--format" => match args.next().as_deref() {
                Some("hex") => format = Format::Hex,
                Some("gpl") => format = Format::Gpl,
                Some(other) => return usage_error(&format!("unknown format {other}")),
                None => return usage_error("-f needs a format"),
            },
            s if s.starts_with('-') && s != "-" => return usage_error(&format!("unknown option {s}")),
            _ if operand.is_some() => return usage_error("too many arguments"),
            _ => operand = Some(arg),
        }
    }

    match command.as_str() {
        "list" => {
            if operand.is_some() {
                return usage_error("list takes no arguments");
            }
            let mut out = String::new();
            for b in BUILTINS {
                let _ = writeln!(
                    out,
                    "{:>3}  {:<18} {:>2}  {}{}",
                    b.id,
                    b.slug,
                    b.palette().len(),
                    b.name,
                    b.credit.map(|c| format!(" (by {c})")).unwrap_or_default()
                );
            }
            emit(out.as_bytes())
        }
        "show" => {
            let Some(key) = operand else {
                return usage_error("show needs a palette id or slug");
            };
            let found = key.parse::<u8>().ok().and_then(builtin).or_else(|| builtin_by_slug(&key));
            match found {
                Some(b) => emit(render(&b.palette(), b.name, format).as_bytes()),
                None => fail(&format!("no built-in palette {key:?}; try 'pips-palette list'")),
            }
        }
        "convert" => {
            let bytes = match read_input(operand.as_deref()) {
                Ok(b) => b,
                Err(e) => return fail(&e),
            };
            match parse(&bytes) {
                Ok(p) => emit(render(&p.palette, p.name.as_deref().unwrap_or("Imported"), format).as_bytes()),
                Err(e) => fail(&e.to_string()),
            }
        }
        other => usage_error(&format!("unknown command {other}")),
    }
}

fn render(p: &Palette, name: &str, format: Format) -> String {
    match format {
        Format::Hex => to_hex(p),
        Format::Gpl => to_gpl(p, name),
    }
}

/// Reads at most one byte past the limit, so an endless stream is refused
/// without being buffered.
fn read_input(path: Option<&str>) -> Result<Vec<u8>, String> {
    let limit = u64::try_from(MAX_INPUT_BYTES).unwrap_or(u64::MAX) + 1;
    let mut buf = Vec::new();
    let result = match path {
        None | Some("-") => io::stdin().lock().take(limit).read_to_end(&mut buf),
        Some(p) => std::fs::File::open(p).and_then(|f| f.take(limit).read_to_end(&mut buf)),
    };
    result.map(|_| buf).map_err(|e| match path {
        Some(p) if p != "-" => format!("{p}: {e}"),
        _ => e.to_string(),
    })
}

fn emit(bytes: &[u8]) -> ExitCode {
    let mut out = io::stdout().lock();
    match out.write_all(bytes).and_then(|()| out.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => fail(&e.to_string()),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("pips-palette: {message}");
    ExitCode::FAILURE
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("pips-palette: {message}\nTry 'pips-palette --help'.");
    ExitCode::from(2)
}
