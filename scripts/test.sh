#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
require_godot
cd "$ROOT"
command -v node >/dev/null || { echo 'Node.js 20.11+ is required for converter tests.' >&2; exit 1; }
node -e 'const [major, minor] = process.versions.node.split(".").map(Number); if (major < 20 || (major === 20 && minor < 11)) { console.error("Node.js 20.11+ is required for converter tests."); process.exit(1); }'
python3 -m unittest discover -s tests -p 'test_*.py' -v
GODOT="$GODOT" node --test tests/test_fourds.mjs
"$GODOT" --headless --path "$ROOT" --editor --import --quit
"$GODOT" --headless --path "$ROOT" --script res://tests/test_game.gd
