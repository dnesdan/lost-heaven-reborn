#!/usr/bin/env bash
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
cd "$ROOT"
exec python3 -m importers.cli "$@"
