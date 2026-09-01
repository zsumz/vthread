#!/usr/bin/env sh
set -eu

LC_ALL=C
LANG=C
export LC_ALL LANG

ZRAIL_VERSION=0.0.3-rc.5
ZCHECK_VERSION=0.0.2
BIN_DIR=${BIN_DIR:-"$HOME/.local/bin"}
TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT INT TERM

case "$(uname -s):$(uname -m)" in
  Linux:x86_64)
    TARGET=x86_64-unknown-linux-gnu
    ZRAIL_SHA256=4debf765fa82ef08e812fa0cc1a640355d25f92dfed8ea222af2733774ec73e6
    ZCHECK_SHA256=a9e3ca964b6f5e86c693edeff4aeaddb46ee9d62f8713d3ded0bb6533b758e53
    ;;
  Linux:aarch64|Linux:arm64)
    TARGET=aarch64-unknown-linux-gnu
    ZRAIL_SHA256=c379bf428cae7b26bf4fd40fe460978edb26aa91555129d77e6c10fc296190be
    ZCHECK_SHA256=f5b296fa2c316f017bc8a189ac186c658ea1dc72b3311d7e0d3639d52d2608c7
    ;;
  Darwin:x86_64)
    TARGET=x86_64-apple-darwin
    ZRAIL_SHA256=b89fdcbb414b10e91c3d59b3f15f6e9c20260526e780f33aaed46b8499ffc5da
    ZCHECK_SHA256=37c382c741999d8749318a52851695a55724371da5ad2f964daa318bf4fe3163
    ;;
  Darwin:arm64|Darwin:aarch64)
    TARGET=aarch64-apple-darwin
    ZRAIL_SHA256=7fb292d904b4d2afa303a48d4cfdf48ecdf006b74dc173ea662322a8710efe23
    ZCHECK_SHA256=973c5ad9a83590a664c69f62aac2e38b07bbcc8c1d887389507f80cf77347883
    ;;
  *)
    echo "unsupported host: $(uname -s) $(uname -m)" >&2
    exit 1
    ;;
esac

checksum() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

install_tool() {
  project=$1
  version=$2
  digest=$3
  archive="$project-$version-$TARGET.tar.gz"
  url="https://github.com/zsumz/$project/releases/download/v$version/$archive"

  curl --fail --location --silent --show-error "$url" --output "$TEMP_DIR/$archive"
  actual=$(checksum "$TEMP_DIR/$archive")
  if [ "$actual" != "$digest" ]; then
    echo "$project checksum mismatch: $actual" >&2
    exit 1
  fi
  tar -xzf "$TEMP_DIR/$archive" -C "$TEMP_DIR"
  install -m 0755 "$TEMP_DIR/$project" "$BIN_DIR/$project"
}

mkdir -p "$BIN_DIR"
install_tool zrail "$ZRAIL_VERSION" "$ZRAIL_SHA256"
install_tool zcheck "$ZCHECK_VERSION" "$ZCHECK_SHA256"

"$BIN_DIR/zrail" --version
"$BIN_DIR/zcheck" --version
