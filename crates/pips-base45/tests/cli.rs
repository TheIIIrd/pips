//! The `pips-b45` tool: behaviour at the edges a shell pipeline hits.
#![cfg(feature = "cli")]

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], stdin: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pips-b45"))
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
fn encodes_with_trailing_newline() {
    let out = run(&[], b"Hello!!");
    assert!(out.status.success());
    assert_eq!(out.stdout, b"%69 VD92EX0\n");
}

#[test]
fn decodes_its_own_output_including_spaces() {
    for ending in ["", "\n", "\r\n"] {
        let out = run(&["-d"], format!("%69 VD92EX0{ending}").as_bytes());
        assert!(out.status.success(), "{ending:?}");
        assert_eq!(out.stdout, b"Hello!!");
    }
}

#[test]
fn round_trips_binary() {
    let data: Vec<u8> = (0..=255).collect();
    let text = run(&[], &data).stdout;
    assert_eq!(run(&["--decode"], &text).stdout, data);
}

#[test]
fn bad_input_fails_with_message_and_no_output() {
    let out = run(&["-d"], b"bb8");
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert_eq!(String::from_utf8_lossy(&out.stderr), "pips-b45: invalid character 'b' at position 0\n");
    // Only one line ending is forgiven.
    assert_eq!(run(&["-d"], b"BB8\n\n").status.code(), Some(1));
}

#[test]
fn usage_errors_exit_2() {
    assert_eq!(run(&["--frobnicate"], b"").status.code(), Some(2));
    assert_eq!(run(&["a", "b"], b"").status.code(), Some(2));
}
