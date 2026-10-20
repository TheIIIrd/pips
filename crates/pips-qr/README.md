# pips-qr

Pips over QR codes: one code for up to 600 bytes, a fountain-coded carousel
for anything bigger. Screen text is always Base45, which is exactly the QR
alphanumeric set. The carousel is `no_std`; drawing symbols (feature
`matrix`, on by default) uses `qrcodegen`.

## Carousel

The message is cut into `n` fragments (≤ 256, 16–1024 bytes each). Frame
`seq < n` carries fragment `seq`; later frames carry the XOR of a
pseudo-random half of them (SplitMix64 seeded by the message CRC and
`seq`). Any `n` independent frames rebuild the message, so a camera can
start anywhere and miss frames freely; on average about 1.6 frames more
than `n` are needed. The decoder is incremental Gaussian elimination over
GF(2), bounded by `n` stored fragments, and checks a CRC-32 at the end.

Frame: `'F'`, version, message length u16, fragment length u16, CRC-32,
seq u32, data. Any malformed, foreign or corrupted frame is refused.

## Command line

```sh
pips-sign seal -k me.key card.pips | pips-qr show          # one code, on the terminal
pips-qr show --svg env.bin > code.svg
pips-qr frames -s 1000 big.bin | shuf | head -n 30 | pips-qr join > back.bin
```

## Testing

Every symbol size we draw is read back by `rqrr`, an independent QR
decoder: single codes at the full 600-byte budget, and carousel frames
joined from the middle of the loop. Carousel recovery is checked under 30%
frame loss over 300 runs, along with truncation, lying headers, frames from
another message and corrupted data.
