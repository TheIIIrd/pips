//! `pips-sign` — keys, card signatures, edition certificates and envelopes.

use std::fmt::Write as _;
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use pips_format::{decode_file, encode_file};
use pips_sign::recovery::{from_paper, from_words, to_paper, to_words, PAPER_PREFIX};
use pips_sign::{open, seal, sign_card, verify_card, Certificate, RecipientCode, SecretKey};

const USAGE: &str = "\
Usage: pips-sign keygen -o KEYFILE
       pips-sign pubkey -k KEYFILE
       pips-sign paper -k KEYFILE
       pips-sign card -k KEYFILE [-o OUT] [CARD.pips]
       pips-sign verify [CARD.pips]
       pips-sign recipient -k KEYFILE --nick NICK
       pips-sign seal -k KEYFILE [--edition N [--of M] --to RECIPIENT] [--from NICK] [CARD.pips]
       pips-sign open [ENVELOPE]

  keygen     new key, saved as 24 recovery words (file mode 0600, never overwrites)
  pubkey     print the public key and its short form
  paper      print the paper-key text; feed it to a QR generator and keep it safe
  card       sign a .pips card as its author
  verify     check a card's signature and print its author
  recipient  the code a person shows to receive a certified copy (binary)
  seal       wrap a card for transfer (binary): with --edition, issue copy N
             of M (0 or omitted: open run) to the RECIPIENT code file; with
             --from, sign as the person passing it on
  open       verify an envelope and describe it

A KEYFILE holds either the 24 words or the paper-key text. Card, envelope
and recipient inputs default to standard input. Binary output goes to
standard output and is refused on a terminal: pipe it into pips-b45.

Exit status: 0 success, 1 invalid input, failed check or I/O error, 2 usage error.";

const MAX_INPUT: u64 = 128 * 1024;

struct Args {
    key: Option<String>,
    out: Option<String>,
    nick: Option<String>,
    from: Option<String>,
    to: Option<String>,
    edition: Option<u16>,
    of: u16,
    input: Option<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(m)) => {
            eprintln!("pips-sign: {m}\nTry 'pips-sign --help'.");
            ExitCode::from(2)
        }
        Err(Failure::Failed(m)) => {
            eprintln!("pips-sign: {m}");
            ExitCode::FAILURE
        }
    }
}

enum Failure {
    Usage(String),
    Failed(String),
}

fn usage<T>(m: impl Into<String>) -> Result<T, Failure> {
    Err(Failure::Usage(m.into()))
}

fn failed<E: std::fmt::Display>(e: E) -> Failure {
    Failure::Failed(e.to_string())
}

fn run() -> Result<(), Failure> {
    let mut argv = std::env::args().skip(1);
    let command = argv.next().unwrap_or_default();
    match command.as_str() {
        "" => return usage("no command given"),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            return Ok(());
        }
        "-V" | "--version" => {
            println!("pips-sign {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }
    let a = parse_args(argv)?;

    match command.as_str() {
        "keygen" => keygen(&a),
        "pubkey" => {
            let k = load_key(&a)?;
            println!("{}\n{}", k.public_key(), k.public_key().short());
            Ok(())
        }
        "paper" => {
            let k = load_key(&a)?;
            println!("{}", *to_paper(&k));
            Ok(())
        }
        "card" => {
            let k = load_key(&a)?;
            let mut card = decode_file(&read_input(a.input.as_deref())?).map_err(failed)?;
            sign_card(&mut card, &k).map_err(failed)?;
            let bytes = encode_file(&card).map_err(failed)?;
            match &a.out {
                Some(p) => std::fs::write(p, bytes).map_err(|e| failed(format!("{p}: {e}"))),
                None => write_binary(&bytes),
            }
        }
        "verify" => {
            let card = decode_file(&read_input(a.input.as_deref())?).map_err(failed)?;
            let author = verify_card(&card).map_err(failed)?;
            println!("valid signature by {} ({})", author, author.short());
            Ok(())
        }
        "recipient" => {
            let k = load_key(&a)?;
            let Some(nick) = a.nick.clone() else { return usage("recipient needs --nick") };
            let code = RecipientCode { key: k.public_key(), nick }.to_bytes().map_err(failed)?;
            write_binary(&code)
        }
        "seal" => seal_cmd(&a),
        "open" => open_cmd(&a),
        other => usage(format!("unknown command {other}")),
    }
}

fn parse_args(mut argv: impl Iterator<Item = String>) -> Result<Args, Failure> {
    let mut a = Args { key: None, out: None, nick: None, from: None, to: None, edition: None, of: 0, input: None };
    while let Some(arg) = argv.next() {
        let mut value = |name: &str| argv.next().map_or_else(|| usage(format!("{name} needs a value")), Ok);
        match arg.as_str() {
            "-k" | "--key" => a.key = Some(value("-k")?),
            "-o" | "--output" => a.out = Some(value("-o")?),
            "--nick" => a.nick = Some(value("--nick")?),
            "--from" => a.from = Some(value("--from")?),
            "--to" => a.to = Some(value("--to")?),
            "--edition" => {
                a.edition = Some(value("--edition")?.parse().or_else(|_| usage("--edition needs 1..=65535"))?);
            }
            "--of" => a.of = value("--of")?.parse().or_else(|_| usage("--of needs 0..=65535"))?,
            s if s.starts_with('-') && s != "-" => return usage(format!("unknown option {s}")),
            _ if a.input.is_some() => return usage("too many arguments"),
            _ => a.input = Some(arg),
        }
    }
    Ok(a)
}

fn keygen(a: &Args) -> Result<(), Failure> {
    let Some(path) = &a.out else { return usage("keygen needs -o KEYFILE") };
    let mut seed = zeroize::Zeroizing::new([0u8; 32]);
    getrandom::getrandom(seed.as_mut()).map_err(|e| failed(format!("no secure randomness: {e}")))?;
    let key = SecretKey::from_seed(&seed);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    let mut f = opts.open(path).map_err(|e| failed(format!("{path}: {e}")))?;
    f.write_all(format!("{}\n", *to_words(&key)).as_bytes()).map_err(failed)?;
    eprintln!("key saved to {path}; public key {}", key.public_key().short());
    eprintln!("these 24 words are your identity: anyone who reads them can sign as you");
    Ok(())
}

fn load_key(a: &Args) -> Result<SecretKey, Failure> {
    let Some(path) = &a.key else { return usage("this command needs -k KEYFILE") };
    #[cfg(unix)]
    if let Ok(meta) = std::fs::metadata(path) {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            eprintln!("pips-sign: warning: {path} is readable by other users");
        }
    }
    let text = zeroize::Zeroizing::new(std::fs::read_to_string(path).map_err(|e| failed(format!("{path}: {e}")))?);
    let key = if text.trim_start().starts_with(PAPER_PREFIX) { from_paper(&text) } else { from_words(&text) };
    key.map_err(|e| failed(format!("{path}: {e}")))
}

fn seal_cmd(a: &Args) -> Result<(), Failure> {
    let k = load_key(a)?;
    let card = decode_file(&read_input(a.input.as_deref())?).map_err(failed)?;
    let cert = match (a.edition, &a.to) {
        (None, None) => None,
        (Some(n), Some(to)) => {
            let code = RecipientCode::from_bytes(&read_file(to)?).map_err(|e| failed(format!("{to}: {e}")))?;
            let day = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| u32::try_from(d.as_secs() / 86_400).unwrap_or(u32::MAX));
            eprintln!("issuing copy {n}{} to {} ({})", of_text(a.of), code.nick, code.key.short());
            Some(Certificate::issue(&k, &card, n, a.of, code.key, day).map_err(failed)?)
        }
        _ => return usage("--edition and --to go together"),
    };
    let sender = a.from.as_deref().map(|nick| (&k, nick));
    write_binary(&seal(&card, cert.as_ref(), sender).map_err(failed)?)
}

fn open_cmd(a: &Args) -> Result<(), Failure> {
    let o = open(&read_input(a.input.as_deref())?).map_err(failed)?;
    let mut s = format!("card      {:?} by {:?}\n", o.card.title, o.card.author);
    match o.author {
        Some(k) => {
            let _ = writeln!(s, "author    {k} ({})", k.short());
        }
        None => s += "author    unsigned card: authorship unknown\n",
    }
    if let Some(c) = o.certificate {
        let _ = writeln!(s, "edition   copy {}{} held by {}", c.edition, of_text(c.of), c.recipient.short());
    }
    if let Some(snd) = &o.sender {
        let _ = writeln!(s, "from      {:?} ({})", snd.nick, snd.key.short());
    }
    s += "verified  all signatures check out\n";
    io::stdout().write_all(s.as_bytes()).map_err(failed)
}

fn of_text(of: u16) -> String {
    if of == 0 {
        String::from(" (open run)")
    } else {
        format!(" of {of}")
    }
}

fn read_file(path: &str) -> Result<Vec<u8>, Failure> {
    read_limited(std::fs::File::open(Path::new(path)).map_err(|e| failed(format!("{path}: {e}")))?)
}

fn read_input(path: Option<&str>) -> Result<Vec<u8>, Failure> {
    match path {
        None | Some("-") => read_limited(io::stdin().lock()),
        Some(p) => read_file(p),
    }
}

fn read_limited(r: impl Read) -> Result<Vec<u8>, Failure> {
    let mut buf = Vec::new();
    r.take(MAX_INPUT + 1).read_to_end(&mut buf).map_err(failed)?;
    if buf.len() as u64 > MAX_INPUT {
        return Err(Failure::Failed("input too large".into()));
    }
    Ok(buf)
}

fn write_binary(bytes: &[u8]) -> Result<(), Failure> {
    let mut out = io::stdout().lock();
    if out.is_terminal() {
        return Err(Failure::Failed("refusing to write binary to a terminal; pipe it into pips-b45 or a file".into()));
    }
    match out.write_all(bytes).and_then(|()| out.flush()) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(failed(e)),
        _ => Ok(()),
    }
}
