#!/usr/bin/env bash
set -euo pipefail
converter_root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
if ! command -v node >/dev/null 2>&1; then
    echo 'Node.js 20.11 or newer is required.' >&2
    exit 1
fi
node -e 'const [major, minor] = process.versions.node.split(".").map(Number); if (major < 20 || (major === 20 && minor < 11)) process.exit(1)' || {
    echo 'Node.js 20.11 or newer is required.' >&2
    exit 1
}
exec node "$converter_root/importers/fourds/cli.mjs" "$@"
