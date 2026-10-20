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
css/app.css                               tokens and components
src/core.js                               loads the WASM core (or the dev stub)
src/qrscan.js                             QR reading: BarcodeDetector or pkg/pips_scan, on demand
src/store.js                              IndexedDB: meta + cards
src/state.js                              the one place that changes data
src/i18n.js                               ru / en strings
src/dom.js                                DOM without innerHTML
src/ui/                                   shell, card, qr, scan, viewer, the four tabs,
                                          import, edition, onboarding
fonts/                                    Literata, IBM Plex Sans (OFL), self-hosted
```

Received cards are stored as the payload bytes they arrived as; all text from
cards reaches the page as text nodes, never as markup; card links open only
after a confirmation that shows the host.
