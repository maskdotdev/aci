#!/bin/sh
set -eu

repo="maskdotdev/aci"
bin_name="aci"
install_dir="${ACI_INSTALL_DIR:-$HOME/.local/bin}"

os="$(uname -s)"
arch="$(uname -m)"

case "$os:$arch" in
  Darwin:arm64)
    target="aarch64-apple-darwin"
    ;;
  Darwin:x86_64)
    target="x86_64-apple-darwin"
    ;;
  Linux:x86_64)
    target="x86_64-unknown-linux-gnu"
    ;;
  *)
    echo "aci: unsupported platform: $os $arch" >&2
    echo "aci: supported platforms: macOS arm64, macOS x86_64, Linux x86_64" >&2
    exit 1
    ;;
esac

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT INT TERM

archive="aci-${target}.tar.gz"
url="https://github.com/${repo}/releases/latest/download/${archive}"

echo "downloading ${url}"
if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$url" -o "$tmp_dir/$archive"
elif command -v wget >/dev/null 2>&1; then
  wget -q "$url" -O "$tmp_dir/$archive"
else
  echo "aci: install requires curl or wget" >&2
  exit 1
fi

tar -xzf "$tmp_dir/$archive" -C "$tmp_dir"
mkdir -p "$install_dir"
install -m 0755 "$tmp_dir/$bin_name" "$install_dir/$bin_name"

echo "installed aci to $install_dir/$bin_name"
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *)
    echo "note: add $install_dir to PATH to run aci from any directory" >&2
    ;;
esac
