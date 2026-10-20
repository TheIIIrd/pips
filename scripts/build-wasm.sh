#!/usr/bin/env sh
# Builds the WebAssembly core and its JavaScript glue into web/pkg/.
# Needs: rustup target add wasm32-unknown-unknown
#        cargo install wasm-bindgen-cli --version <the wasm-bindgen version in Cargo.lock>
set -eu
cd "$(dirname "$0")/.."

want=$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/version = |"/, ""); print; exit }' Cargo.lock)
have=$(wasm-bindgen --version 2>/dev/null | awk '{ print $2 }' || true)
if [ "$have" != "$want" ]; then
    echo "wasm-bindgen-cli $want is needed (found: ${have:-none}):" >&2
    echo "  cargo install wasm-bindgen-cli --version $want" >&2
    exit 1
fi

# The core, and the QR reader the app loads only where the browser has no
# BarcodeDetector.
cargo build -p pips-wasm -p pips-scan --release --target wasm32-unknown-unknown
for m in pips_wasm pips_scan; do
    wasm-bindgen --target web --out-dir web/pkg "target/wasm32-unknown-unknown/release/$m.wasm"
done
ls -l web/pkg/*.wasm
