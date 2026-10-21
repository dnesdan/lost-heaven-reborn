#!/usr/bin/env bash
# Official rustup with its published checksum; rustup verifies toolchain artifacts.
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
[[ $# == 0 || ( $# == 1 && $1 == --graphics ) ]] || { echo 'Usage: scripts/setup.sh [--graphics]' >&2; exit 2; }
for tool in python3 curl; do
    command -v "$tool" >/dev/null || { echo "Missing prerequisite: $tool" >&2; exit 1; }
done
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) host=x86_64-unknown-linux-gnu ;;
    Linux-aarch64|Linux-arm64) host=aarch64-unknown-linux-gnu ;;
    Darwin-arm64) host=aarch64-apple-darwin ;;
    Darwin-x86_64) host=x86_64-apple-darwin ;;
    *) echo 'Unsupported host; install official Rust 1.99.1 manually.' >&2; exit 1 ;;
esac
mkdir -p "$LHR_TOOLS_DIR/rust/downloads" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
if [[ ! -x "$CARGO_HOME/bin/rustup" ]]; then
    download="$LHR_TOOLS_DIR/rust/downloads"
    for artifact in rustup-init rustup-init.sha256; do
        curl --fail --location --proto '=https' --proto-redir '=https' --retry 3 \
            "https://static.rust-lang.org/rustup/dist/$host/$artifact" --output "$download/$artifact"
    done
    python3 - "$download" <<'PY'
import hashlib, pathlib, re, sys
root = pathlib.Path(sys.argv[1])
expected = (root / 'rustup-init.sha256').read_text().split()[0]
if not re.fullmatch('[0-9a-f]{64}', expected):
    raise SystemExit('Invalid official rustup checksum')
digest = hashlib.sha256()
with (root / 'rustup-init').open('rb') as artifact:
    for chunk in iter(lambda: artifact.read(1024 * 1024), b''):
        digest.update(chunk)
if digest.hexdigest() != expected:
    raise SystemExit('rustup checksum mismatch')
PY
    chmod +x "$download/rustup-init"
    "$download/rustup-init" -y --no-modify-path --profile minimal --default-toolchain 1.99.1 --component rustfmt --component clippy
else
    rustup toolchain install 1.99.1 --profile minimal --component rustfmt --component clippy
fi
require_rust
if [[ "${1:-}" == --graphics ]]; then
    "$ROOT/scripts/setup-graphics.sh"
fi
cd "$ROOT"
cargo fetch --locked
rustc --version
cargo --version
