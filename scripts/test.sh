#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
require_godot
cd "$ROOT"
python3 -m unittest discover -s tests -p 'test_*.py' -v
"$GODOT" --headless --path "$ROOT" --editor --import --quit
"$GODOT" --headless --path "$ROOT" --script res://tests/test_game.gd
