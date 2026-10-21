#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
require_godot
exec python3 "$ROOT/scripts/capture.py" --godot "$GODOT" "$@"
