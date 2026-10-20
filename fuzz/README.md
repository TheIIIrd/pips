# Fuzzing

One target per parser of outside input, for cargo-fuzz (nightly Rust):

| Target | What it feeds | Checked beyond "no panic" |
|---|---|---|
| `card` | card payloads and `.pips` files | decode → encode → decode is the same card |
| `envelope` | envelopes, certificates, recipient codes, words, paper keys | |
| `base45` | QR text | accepted text encodes back to itself |
| `palette` | `.hex` / `.gpl` | |
| `raster` | PNG / Aseprite, then the app's import | an imported picture is a valid card |
| `backup` | backup bodies and whole files | |
| `carousel` | a stream of carousel frames | |

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run card corpus/card seeds/card -- -max_total_time=60
```

`seeds/` holds a few valid inputs per target so the fuzzer starts from real
structure. The key in `seeds/envelope/words` and `paper` is a fixed test key
(seed `[7; 32]`), public on purpose. The Aseprite files in `seeds/raster`
come from the asefile project (MIT), as in `crates/pips-raster/tests`.
`corpus/` and `artifacts/` are local and ignored; CI keeps its corpus in a
cache.
