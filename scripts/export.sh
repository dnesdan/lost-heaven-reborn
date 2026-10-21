#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
require_godot
target="${1:-Linux}"
case "$target" in
    Linux) default_path="$ROOT/build/linux/lost-heaven-reborn.x86_64" ;;
    macOS) default_path="$ROOT/build/macos/LostHeavenReborn.zip" ;;
    iOS) default_path="$ROOT/build/ios/LostHeavenReborn.zip" ;;
    *) echo 'Usage: scripts/export.sh [Linux|macOS|iOS] [output-path]' >&2; exit 2 ;;
esac
output="${2:-$default_path}"
output="$(python3 -c 'import pathlib,sys; print(pathlib.Path(sys.argv[1]).expanduser().resolve())' "$output")"
mkdir -p -- "$(dirname -- "$output")"
"$GODOT" --headless --path "$ROOT" --editor --import --quit
"$GODOT" --headless --path "$ROOT" --export-release "$target" "$output"
"$GODOT" --headless --path "$ROOT" --script res://scripts/write_engine_notices.gd -- "$(dirname -- "$output")/GODOT_NOTICES.txt"
cp -- "$ROOT/LICENSE" "$(dirname -- "$output")/LICENSE.txt"
if [[ "$target" == Linux ]]; then
    smoke_log="$(mktemp /tmp/lhr-export-smoke.XXXXXX)"
    trap 'rm -f -- "$smoke_log"' EXIT
    "$output" --headless --quit-after 30 >"$smoke_log" 2>&1 || {
        cat "$smoke_log" >&2
        exit 1
    }
    python3 - "$smoke_log" <<'PY'
import pathlib, sys
text = pathlib.Path(sys.argv[1]).read_text(errors='replace')
if 'Godot Engine v4.6.3' not in text or any(value in text for value in ('SCRIPT ERROR', 'ERROR:', 'Parse Error', 'Failed to load')):
    print(text, file=sys.stderr)
    raise SystemExit('Standalone smoke check failed')
print('Standalone headless startup passed; rendered behavior requires a separate check.')
PY
fi
