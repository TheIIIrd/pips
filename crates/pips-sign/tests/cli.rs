//! The whole edition flow through the `pips-sign` tool, as a shell user would run it.
#![cfg(feature = "cli")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn tool(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pips-sign")).args(args).output().unwrap()
}

fn ok(args: &[&str]) -> Vec<u8> {
    let out = tool(args);
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    out.stdout
}

fn text(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

struct Dir(PathBuf);
impl Dir {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("pips-sign-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Self(d)
    }
    fn p(&self, f: &str) -> String {
        self.0.join(f).to_str().unwrap().to_owned()
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn edition_flow() {
    let d = Dir::new("flow");
    let (author, holder) = (d.p("author.key"), d.p("holder.key"));
    ok(&["keygen", "-o", &author]);
    ok(&["keygen", "-o", &holder]);
    assert_eq!(tool(&["keygen", "-o", &author]).status.code(), Some(1), "overwrote a key");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&author).unwrap().permissions().mode() & 0o777, 0o600);
    }

    // An unsigned card, signed by its author.
    let mut card = pips_format::Card::new(8, 8);
    card.title = "Кит".into();
    card.author = "kitsu".into();
    card.pixels = (0..64u8).map(|i| i % 16).collect();
    std::fs::write(d.p("card.pips"), pips_format::encode_file(&card).unwrap()).unwrap();
    assert_eq!(tool(&["verify", &d.p("card.pips")]).status.code(), Some(1));
    ok(&["card", "-k", &author, "-o", &d.p("signed.pips"), &d.p("card.pips")]);
    let author_pub = text(&ok(&["pubkey", "-k", &author]));
    let author_hex = author_pub.lines().next().unwrap();
    assert!(text(&ok(&["verify", &d.p("signed.pips")])).contains(author_hex));

    // The holder shows a recipient code; the author issues copy 3 of 50 to it.
    std::fs::write(d.p("holder.code"), ok(&["recipient", "-k", &holder, "--nick", "mossfern"])).unwrap();
    let env =
        ok(&["seal", "-k", &author, "--edition", "3", "--of", "50", "--to", &d.p("holder.code"), &d.p("signed.pips")]);
    std::fs::write(d.p("env.bin"), &env).unwrap();
    let opened = text(&ok(&["open", &d.p("env.bin")]));
    let holder_short = text(&ok(&["pubkey", "-k", &holder])).lines().nth(1).unwrap().to_owned();
    assert!(opened.contains(&format!("copy 3 of 50 held by {holder_short}")), "{opened}");

    // The holder passes a copy on, signing as sender.
    std::fs::write(d.p("fwd.bin"), ok(&["seal", "-k", &holder, "--from", "mossfern", &d.p("signed.pips")])).unwrap();
    assert!(text(&ok(&["open", &d.p("fwd.bin")])).contains("from      \"mossfern\""));

    // A non-author cannot issue editions.
    let out = tool(&["seal", "-k", &holder, "--edition", "1", "--to", &d.p("holder.code"), &d.p("signed.pips")]);
    assert_eq!(out.status.code(), Some(1));

    // Tampering with the envelope is caught.
    let mut bad = env.clone();
    let last = bad.len() - 1;
    bad[last] ^= 1;
    std::fs::write(d.p("bad.bin"), bad).unwrap();
    assert_eq!(tool(&["open", &d.p("bad.bin")]).status.code(), Some(1));
}

#[test]
fn paper_key_is_the_same_key() {
    let d = Dir::new("paper");
    let k = d.p("k");
    ok(&["keygen", "-o", &k]);
    let paper = ok(&["paper", "-k", &k]);
    assert!(text(&paper).starts_with("PIPS:KEY:1:"));
    std::fs::write(d.p("paper"), &paper).unwrap();
    assert_eq!(ok(&["pubkey", "-k", &d.p("paper")]), ok(&["pubkey", "-k", &k]));
    assert!(!Path::new(&d.p("missing")).exists());
    assert_eq!(tool(&["pubkey", "-k", &d.p("missing")]).status.code(), Some(1));
    assert_eq!(tool(&["pubkey"]).status.code(), Some(2));
}
