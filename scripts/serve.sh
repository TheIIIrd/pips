#!/bin/sh
# Serves web/ on http://localhost:8000 for development.
#
# Opening web/index.html as a file does not work: browsers refuse ES module
# imports and WebAssembly from file:// (the page has no origin), and the
# page's CSP allows only its own origin. Nothing is reported on the page —
# the scripts simply never run.
#
# Bound to 127.0.0.1 only: not reachable from the network.
set -eu
cd "$(dirname "$0")/../web"
[ -f pkg/pips_wasm.js ] || echo "note: web/pkg is missing — run scripts/build-wasm.sh first" >&2
port="${1:-8000}"
echo "http://localhost:$port/   (Ctrl+C to stop)"
exec python3 -m http.server "$port" --bind 127.0.0.1
