# Development shell: `nix-shell` in this directory.
#
# Uses rustup (patched for NixOS by nixpkgs) rather than nixpkgs' rustc,
# because building the web core needs the wasm32 standard library, which
# rustup installs and nixpkgs' rustc does not ship.
{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  packages = with pkgs; [ rustup pkg-config openssl gcc python3 ];

  shellHook = ''
    export PATH="$HOME/.cargo/bin:$PATH"
    rustup toolchain install stable --profile minimal --component clippy >/dev/null
    rustup default stable >/dev/null
    rustup target add wasm32-unknown-unknown >/dev/null
    echo "rust: $(rustc --version)   wasm32: $(rustup target list --installed | grep -c wasm32) target"
  '';
}
