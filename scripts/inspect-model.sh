#!/usr/bin/env bash
# Reuse the bounded capture/process-ownership wrapper for a private model render.
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
require_godot
if [[ $# -lt 1 ]]; then
    echo 'Usage: inspect-model.sh /absolute/private/model.glb [--output /private/new-capture-directory]' >&2
    exit 1
fi
inspection_model="$1"
shift
exec python3 "$ROOT/scripts/capture.py" --godot "$GODOT" --model "$inspection_model" "$@"
