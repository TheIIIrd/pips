# Pips — web app

A PWA in plain ES modules: no build step, no dependencies, no third-party
requests. Everything it loads comes from its own origin, which the
Content-Security-Policy in `index.html` enforces — the identity key lives in
this origin's IndexedDB, and that policy is what protects it.

## Run

```sh
scripts/build-wasm.sh       # the core, into web/pkg/
scripts/serve.sh            # http://localhost:8000 — opening index.html as a file does not work
```

Open http://localhost:8000. Without `web/pkg/`, on localhost only, the app
falls back to `src/stub-core.js`: same interface, **no real cryptography**, a
"DEV STUB" banner, and a separate database. Elsewhere it refuses to start
without the real core. Add `?dev` to the URL (or use the stub) to get the
"Add sample cards" button on the Me tab — on localhost only.

The published site is assembled by `scripts/build-site.sh` into `_site/`:
without `dev/` and the stub, and with the service worker's cache version and
file list filled in from what is actually published.

## Layout

```
index.html, manifest.webmanifest, sw.js   shell, install, offline cache
css/app.css                               tokens, base controls, the tab bar
css/kit/*.css                             one file per component of src/kit/
css/collection.css                        the collection screen
src/core.js                               loads the WASM core (or the dev stub)
src/qrscan.js                             QR reading: BarcodeDetector or pkg/pips_scan, on demand
src/store.js                              IndexedDB: meta + cards
src/state.js                              the one place that changes data
src/i18n.js                               ru / en strings
src/dom.js                                DOM without innerHTML, the icon set
src/kit/                                  the bricks: motion, gestures, buttons, fields, sheets
src/collection/                           the collection screen, built from the bricks
src/ui/                                   shell, card frame, qr, scan, the other tabs,
                                          import, edition, onboarding
fonts/                                    Literata, IBM Plex Sans (OFL), self-hosted
```

## How the interface is built

Like the Rust crates, the interface is made of small parts that each do one
thing, with one way of fitting together:

- **A component is a function**: `make(options) → { el, …methods }`. It takes
  plain data and callbacks, returns its element and a few methods to change
  it (`update`, `set`, `show`), and never reaches into `state.js` or another
  screen. What a press *means* is the caller's business.
- **Layers depend only downwards**:
  1. `css/app.css` tokens — colours, type, radii; `kit/motion.js` springs and
     easings. A component never invents its own value for these.
  2. `kit/motion.js`, `kit/gestures.js` — springs on one shared frame loop, and
     the touch arbiter: one owner per touch, everything else waits; a second
     finger anywhere makes a pinch. Plain logic, no app knowledge.
  3. `collection/layout.js`, `collection/query.js` — pure functions (cards in,
     rectangles or lists out), tested under node in `tests/unit/`.
  4. `kit/*` controls — icon button, segmented switch, chips, search field,
     top sheet, strip, action bar, identicon.
  5. Screen parts — `collection/stage.js` (the zoom axis), `turn.js` (the card
     that turns over), `back.js`, `share.js`.
  6. Screens — `collection/view.js` only puts parts together and decides
     what gestures mean.
- **Every gesture has a button.** Pull up / down in the feed is the action bar's
  Share / Archive; zooming works with the back arrow, Escape and ctrl+wheel.
  The browser tests use only the buttons.
- **Extract on the second use**, not before: a part used once stays in its
  screen until another screen needs it.

Received cards are stored as the payload bytes they arrived as; all text from
cards reaches the page as text nodes, never as markup; card links open only
after a confirmation that shows the host.
