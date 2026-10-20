//! `pips-raster` — turn a PNG or Aseprite file into an unsigned `.pips` card.
//!
//! ```sh
//! pips-raster art.aseprite -o art.pips
//! pips-raster -p pico-8 photo-ish.png -o out.pips && pips-card inspect out.pips
//! ```

use std::io::{self, Read, Write};
use std::process::ExitCode;

use pips_format::{encode_file, Card, PaletteRef};
use pips_raster::{import, Options, Target, MAX_FILE};

const USAGE: &str = "\
Usage: pips-raster [OPTIONS] FILE [-o OUT]
Convert a PNG or Aseprite picture into an unsigned Pips card. Without -o
only reports what the conversion would do. FILE '-' is standard input.

  -p, --palette SLUG  map every colour to the nearest in this built-in
                      palette (see pips-palette list); default: an exact
                      built-in match, or the picture's own colours
  -1, --first-frame   keep only the first frame of an animation
      --keep-scale    do not undo integer upscaling
  -t, --title TEXT    card title
  -o, --output OUT    write the .pips file here ('-' for standard output)
  -h, --help          show this help
  -V, --version       show version

Exit status: 0 done, 1 cannot convert or write, 2 usage error.";

fn main() -> ExitCode {
    let mut opts = Options::default();
    let (mut input, mut output, mut title) = (None, None, String::new());
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        let r = match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("pips-raster {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "-1" | "--first-frame" => {
                opts.first_frame_only = true;
                Ok(())
            }
            "--keep-scale" => {
                opts.unscale = false;
                Ok(())
            }
            "-p" | "--palette" => value(&a).and_then(|s| match pips_palette::builtin_by_slug(&s) {
                Some(b) => {
                    opts.target = Target::Nearest(PaletteRef::Builtin(b.id));
                    Ok(())
                }
                None => Err(format!("unknown palette {s}")),
            }),
            "-t" | "--title" => value(&a).map(|s| title = s),
            "-o" | "--output" => value(&a).map(|s| output = Some(s)),
            s if s.starts_with('-') && s != "-" => Err(format!("unknown option {s}")),
            _ if input.is_some() => Err("more than one input file".into()),
            _ => {
                input = Some(a);
                Ok(())
            }
        };
        if let Err(e) = r {
            return usage_error(&e);
        }
    }
    let Some(input) = input else { return usage_error("no input file") };

    let limit = (MAX_FILE + 1) as u64;
    let mut bytes = Vec::new();
    let read = if input == "-" {
        io::stdin().lock().take(limit).read_to_end(&mut bytes)
    } else {
        std::fs::File::open(&input).and_then(|f| f.take(limit).read_to_end(&mut bytes))
    };
    if let Err(e) = read {
        return fail(&format!("{input}: {e}"));
    }

    let (picture, report) = match import(&bytes, &opts) {
        Ok(x) => x,
        Err(e) => return fail(&format!("{input}: {e}")),
    };
    eprintln!(
        "{}×{}{} → {}×{}, {} frame(s), {} colour(s), palette {}{}",
        report.source_width,
        report.source_height,
        if report.scale > 1 { format!(" at {}×", report.scale) } else { String::new() },
        picture.width,
        picture.height,
        picture.animation.map_or(1, |a| a.frames),
        report.colors,
        match picture.palette {
            PaletteRef::Builtin(id) => pips_palette::builtin(id).map_or("?", |b| b.slug).to_string(),
            PaletteRef::Inline(p) => format!("own ({} colours)", p.len()),
        },
        if report.changed_pixels > 0 {
            format!(", {} pixel(s) recoloured", report.changed_pixels)
        } else {
            String::new()
        },
    );

    let Some(output) = output else { return ExitCode::SUCCESS };
    let mut card = Card::new(1, 1);
    picture.apply_to(&mut card);
    card.title = title;
    let bytes = match encode_file(&card) {
        Ok(b) => b,
        Err(e) => return fail(&format!("cannot encode: {e}")),
    };
    let written = if output == "-" { io::stdout().lock().write_all(&bytes) } else { std::fs::write(&output, &bytes) };
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(&format!("{output}: {e}")),
    }
}

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("pips-raster: {msg}\nTry 'pips-raster --help'.");
    ExitCode::from(2)
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("pips-raster: {msg}");
    ExitCode::FAILURE
}
