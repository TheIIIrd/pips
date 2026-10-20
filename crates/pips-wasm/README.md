# pips-wasm

The Pips core for the web app: one facade (`src/api.rs`, tested natively)
over the format, palette, signing and QR crates, exposed to JavaScript by
`wasm-bindgen` (`src/bindings.rs`, one-line conversions only).

Build with `scripts/build-wasm.sh`; it checks that `wasm-bindgen-cli`
matches the library version and writes `web/pkg/`.

## Using it

```js
import init, * as pips from "./pkg/pips_wasm.js";
await init();

// Identity: 32 random bytes once, then only the seed is stored.
const me = pips.Identity.fromSeed(crypto.getRandomValues(new Uint8Array(32)));
me.words();                       // 24 words to write down
me.paper();                       // text for the paper-key QR

// Editor → share
const card = { width: 32, height: 32, opaque: true, palette: { builtin: 1 },
               frame: 0, frames: 1, delay_ms: 0, title: "Сова", author: "kitsu",
               handle: { network: "telegram", text: "@kitsu" }, link: "kitsu.art",
               pixels: new Uint8Array(1024) };
pips.shareSize(card);             // live meter, compare with 600
const payload = me.signCard(card);
const envelope = me.sealAsAuthor(payload, undefined);
const plan = pips.qrPlan(envelope); // { kind: "single", text } | { kind: "carousel", fragments }

// Scan → receive
const s = pips.classify(scannedText);  // envelope | frame | recipient | paper_key | unknown
if (s.kind === "envelope") {
  const got = pips.openEnvelope(s.bytes); // throws { code, message } if anything fails
  // store got.payload (exact bytes) and got.certificate?.bytes; dedupe by got.digest
}
```

Errors are thrown as `{ code, message }`; `code` is stable (`bad_signature`,
`invalid_card`, `not_author`, `bad_words`, …) for the app to translate.

## Security notes for the app

- The seed is the identity. Keep it in IndexedDB on the app's own origin,
  load no third-party scripts, and set a strict Content-Security-Policy:
  any script running on the origin can read it.
- Store received cards as `payload` bytes, never re-encoded from the
  display data, so unknown extensions survive being passed on.
- A `paper_key` scan means someone showed their secret. Ask before using
  it, and say so.
