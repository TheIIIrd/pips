#!/bin/sh
# Assembles the published site in _site/ from web/.
#
# - Development files stay out: web/dev/ (sample art), the core stub, notes,
#   TypeScript declarations.
# - The service worker gets a version from the commit and the list of
#   exactly the files published, so the offline cache can never miss a
#   module or keep an old one.
#
#   scripts/build-site.sh [VERSION]      (default: short commit hash)
set -eu
cd "$(dirname "$0")/.."
[ -f web/pkg/pips_wasm_bg.wasm ] || { echo "web/pkg is missing: run scripts/build-wasm.sh first" >&2; exit 1; }
version="pips-${1:-$(git rev-parse --short HEAD 2>/dev/null || date -u +%Y%m%d%H%M)}"

rm -rf _site
mkdir _site
list=$(cd web && find . -type f \
    ! -path './dev/*' ! -name 'stub-core.js' ! -name 'README.md' ! -name '*.d.ts' \
    ! -name 'jsconfig.json' ! -name '.gitignore' ! -name 'package.json' | sed 's|^\./||' | LC_ALL=C sort)
for f in $list; do
    mkdir -p "_site/$(dirname "$f")"
    cp "web/$f" "_site/$f"
done

# Every published file except the worker itself, as one JS array line.
files=$(printf '%s\n' $list | grep -v '^sw\.js$' | sed "s/.*/'&'/" | paste -sd, -)
sed -i \
    -e "s|^const VERSION = .*|const VERSION = '$version';|" \
    -e "s|^const FILES = .*|const FILES = ['./', $files];|" \
    _site/sw.js
grep -q "^const VERSION = '$version';" _site/sw.js || { echo "sw.js template changed: VERSION not set" >&2; exit 1; }

echo "_site: $(printf '%s\n' $list | wc -l) files, $(du -sh _site | cut -f1), cache $version"
