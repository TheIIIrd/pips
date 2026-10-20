# Pips

Pixel-art swap cards that travel by QR code. Draw or import a small
picture, put your handle on the back, sign it, and hand it over: the other
phone scans it, checks the signature, and the card joins their collection
with its history — who drew it, who passed it on, which copy of how many.

No server, no accounts, no tracking. Everything lives on the device; the
only way a card moves is a QR code (or a link carrying one).

> **Status: early.** The core is complete and tested; the app works end to
> end but its design is still a first draft. Formats may change before 1.0.

## What works

- Identity: an Ed25519 key, restored from 24 words or a paper QR code.
- Cards up to 128×128 px, 32 colours, animation; 33 built-in palettes.
- New cards from PNG and Aseprite files (layers, frames, upscaled exports).
- Giving and receiving by QR — one code, or a fountain-coded carousel for
  large cards; every card and envelope signature is checked. QR codes are
  read by the browser where it can, by our own reader everywhere else.
- Numbered copies: the artist scans the recipient's code and issues
  "№3 of 50", signed and bound to that person.
- Collection by weeks and months, archive, bin, personal notes.
- Encrypted backups that open only with the same 24 words.
- Russian and English; installable, works offline.

## Layout

| Path | What |
|---|---|
| `crates/pips-base45` | Base45 (RFC 9285), the text inside QR codes |
| `crates/pips-palette` | built-in palettes, `.hex` / `.gpl` import |
| `crates/pips-format` | the card format: strict codec, canonical form, digest |
| `crates/pips-sign` | keys, signatures, edition certificates, envelopes, recovery words |
| `crates/pips-qr` | single QR codes and the carousel |
| `crates/pips-raster` | PNG / Aseprite import |
| `crates/pips-store` | backups |
| `crates/pips-wasm` | one facade over all of the above for the web app |
| `crates/pips-scan` | QR reading for browsers without `BarcodeDetector`, loaded on demand |
| `web/` | the app: plain ES modules, no build step, strict CSP |

Each crate does one thing, is `no_std` where it can be, and comes with a
small command-line tool (`pips-card`, `pips-sign`, `pips-qr`, `pips-raster`,
`pips-backup`…) that reads standard input and writes standard output.
Every parser is strict: it refuses rather than repairs, and checks sizes
before allocating. Design decisions and their reasons are in
[DECISIONS.md](DECISIONS.md) (Russian).

## Build and run

Needs Rust (stable, with the `wasm32-unknown-unknown` target), the matching
`wasm-bindgen-cli`, and Python 3 for the local server. On NixOS, `nix-shell`
provides all of it.

```sh
cargo test --workspace      # the core
scripts/build-wasm.sh       # the core for the browser, into web/pkg/
scripts/serve.sh            # http://localhost:8000
```

Browser tests drive the app in Chromium through its real journeys —
import, exchange by QR, refusals, numbered copies, backups:

```sh
cd tests/e2e && npm ci && npx playwright install chromium && npm test
```

Every parser of outside input has a fuzz target (`fuzz/`, nightly Rust):

```sh
cargo install cargo-fuzz
cd fuzz && cargo +nightly fuzz run card corpus/card seeds/card
```

`scripts/build-site.sh` assembles what GitHub Pages publishes. CI runs
formatting, clippy, tests, the minimum Rust version, a type check of the
app and the browser tests, and deploys `main`; a second workflow fuzzes
every target on each push and for longer once a week.

## Licence

Code: [Apache-2.0](LICENSE). Fonts in `web/fonts`: SIL Open Font Licence
(see the files there). Test pictures in `crates/pips-raster/tests/aseprite`:
MIT, from the asefile project. `web/dev/qrcode.js` (development only, not
published): MIT, Kazuhiko Arase.

---

**По-русски.** Pips — пиксельные карточки для обмена через QR-код, без
сервера и аккаунтов. Проект на ранней стадии: ядро готово и покрыто
тестами, приложение работает целиком, дизайн — первый черновик. Решения и их
причины — в [DECISIONS.md](DECISIONS.md).
