//! The `pips-palette` tool.
#![cfg(feature = "cli")]

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pips-palette"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn show_by_id_and_slug_agree() {
    let by_id = run(&["show", "17"], b"");
    assert!(by_id.status.success());
    assert_eq!(by_id.stdout, run(&["show", "sweetie-16"], b"").stdout);
    assert!(by_id.stdout.starts_with(b"1a1c2c\n"));
}

#[test]
fn convert_normalises_gpl_to_hex_and_back() {
    let gpl = run(&["show", "-f", "gpl", "pico-8"], b"").stdout;
    let hex = run(&["convert"], &gpl);
    assert!(hex.status.success());
    assert_eq!(hex.stdout, run(&["show", "pico-8"], b"").stdout);
    let again = run(&["convert", "-f", "gpl"], &hex.stdout);
    assert!(String::from_utf8(again.stdout).unwrap().starts_with("GIMP Palette\nName: Imported\n"));
}

#[test]
fn errors_and_exit_codes() {
    let bad = run(&["convert"], b"123456\nnope\n");
    assert_eq!(bad.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&bad.stderr), "pips-palette: line 2: not a colour\n");
    assert_eq!(run(&["show", "nope"], b"").status.code(), Some(1));
    assert_eq!(run(&["show"], b"").status.code(), Some(2));
    assert_eq!(run(&["frobnicate"], b"").status.code(), Some(2));
    assert_eq!(run(&["convert", "-f", "png"], b"").status.code(), Some(2));
}
