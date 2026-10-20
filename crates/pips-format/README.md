# pips-format

The Pips card format: a strict, bounded encoder and decoder, the canonical
form, and the digest that identifies a card. `no_std` + `alloc`, no `unsafe`;
depends on `pips-palette`, `miniz_oxide` (deflate) and `sha2`.

The byte layout and the rules are documented at the top of `src/lib.rs`.
In short: a small header, title, author, one social handle, one https link
without its scheme, optional extensions, one byte per pixel deflated, and an
optional Ed25519 key and signature (checked by `pips-sign`, not here).

A typical signed 32×32 card with title, handle and link is about 330 bytes —
under 500 Base45 characters, one QR code.

```rust
let bytes = pips_format::encode(&card)?;     // what a QR code carries
let card = pips_format::decode(&bytes)?;     // from a stranger: strict, bounded
let id = pips_format::digest(&card)?;        // same across encoders and signatures
```

## Command line

```sh
pips-card inspect card.pips
pips-b45 -d < scanned.txt | pips-card inspect
```

## Testing

`cargo test` covers round trips of 22 sample artworks and of a card using
every field, digest stability across compressors, truncation at every
offset, 60 000 random mutations (no panic; anything accepted re-encodes to
itself), decompression bombs, oversized canvases refused before inflation,
and every field rule. `cargo run --example sample` writes a card to try the
tools on.
