# pips-base45

Base45 ([RFC 9285]) for QR payloads. `no_std`, no dependencies, no `unsafe`.

The decoder is strict: it accepts exactly one string per byte sequence and
refuses everything else — lowercase, stray whitespace, out-of-range groups, a
lone trailing character. Input from a camera is input from a stranger.

```rust
let text = pips_base45::encode(b"Hello!!");     // "%69 VD92EX0"
let bytes = pips_base45::decode(&text)?;         // Err(DecodeError) if not canonical
```

## Command line

```sh
$ printf 'Hello!!' | pips-b45
%69 VD92EX0
$ echo '%69 VD92EX0' | pips-b45 -d
Hello!!
```

Exit status: 0 on success, 1 on invalid input or I/O error, 2 on a usage
error. Decoding ignores one trailing line ending and nothing else, since the
space is itself a Base45 character.

## Testing

`cargo test` checks the RFC examples, every one- and two-byte input, and every
two- and three-character string — which together show the encoding is a
bijection and the decoder never silently alters input.

[RFC 9285]: https://www.rfc-editor.org/rfc/rfc9285
