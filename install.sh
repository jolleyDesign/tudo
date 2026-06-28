#!/bin/sh
# tudo installer — downloads a prebuilt binary from GitHub releases and puts it
# on your PATH. No Rust toolchain required.
#
#   curl -fsSL https://raw.githubusercontent.com/jolleydesign/tudo/main/install.sh | sh
#
# Environment overrides:
#   TUDO_INSTALL_DIR   where to install (default: $HOME/.local/bin)
#   TUDO_VERSION       release tag to install, e.g. v0.1.0 (default: latest)

set -eu

REPO="jolleydesign/tudo"
BIN="tudo"
INSTALL_DIR="${TUDO_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${TUDO_VERSION:-latest}"

err()  { printf 'error: %s\n' "$1" >&2; exit 1; }
info() { printf '%s\n' "$1" >&2; }

# --- detect platform ---------------------------------------------------------
os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Darwin) os_part="apple-darwin" ;;
  Linux)  os_part="unknown-linux-musl" ;;
  *) err "unsupported OS '$os' — try: cargo install --git https://github.com/$REPO" ;;
esac

case "$arch" in
  arm64 | aarch64) arch_part="aarch64" ;;
  x86_64 | amd64)  arch_part="x86_64" ;;
  *) err "unsupported architecture '$arch' — try: cargo install --git https://github.com/$REPO" ;;
esac

target="${arch_part}-${os_part}"
asset="${BIN}-${target}.tar.gz"

if [ "$VERSION" = "latest" ]; then
  url="https://github.com/$REPO/releases/latest/download/$asset"
else
  url="https://github.com/$REPO/releases/download/$VERSION/$asset"
fi

# --- download ----------------------------------------------------------------
command -v curl >/dev/null 2>&1 || err "curl is required but was not found"
command -v tar  >/dev/null 2>&1 || err "tar is required but was not found"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

info "Downloading $asset ($VERSION)..."
if ! curl -fsSL "$url" -o "$tmp/$asset"; then
  err "download failed: $url
Is there a published release with that asset? See https://github.com/$REPO/releases"
fi

tar -xzf "$tmp/$asset" -C "$tmp" || err "failed to extract $asset"
[ -f "$tmp/$BIN" ] || err "archive did not contain a '$BIN' binary"

# --- install -----------------------------------------------------------------
mkdir -p "$INSTALL_DIR"
mv "$tmp/$BIN" "$INSTALL_DIR/$BIN"
chmod +x "$INSTALL_DIR/$BIN"

info ""
info "Installed $BIN to $INSTALL_DIR/$BIN"

# --- PATH check --------------------------------------------------------------
case ":$PATH:" in
  *":$INSTALL_DIR:"*)
    info "Run 'tudo' to get started."
    ;;
  *)
    info ""
    info "$INSTALL_DIR is not on your PATH. Add it, e.g.:"
    info "  echo 'export PATH=\"$INSTALL_DIR:\$PATH\"' >> ~/.profile"
    info "Then restart your shell and run 'tudo'."
    ;;
esac
