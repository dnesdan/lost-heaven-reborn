#!/usr/bin/env bash
# Official version-pinned engine and templates; downloaded artifacts stay outside Git.
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
if [[ "${1:-}" != '' && "${1:-}" != --graphics ]]; then
    echo 'Usage: scripts/setup.sh [--graphics]' >&2
    exit 2
fi
for tool in python3 curl unzip sha512sum; do
    command -v "$tool" >/dev/null || { echo "Missing setup prerequisite: $tool" >&2; exit 1; }
done
[[ "$(uname -s)" == Linux ]] || { echo 'This setup script prepares Linux. Install official Godot 4.6.3 on Apple hosts.' >&2; exit 1; }
case "$(uname -m)" in
    x86_64) arch=x86_64 ;;
    aarch64|arm64) arch=arm64 ;;
    *) echo 'Unsupported Linux architecture.' >&2; exit 1 ;;
esac
tools_dir="${LHR_TOOLS_DIR:-/workspace/lhr-tools}"
mkdir -p "$tools_dir" "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
version=4.6.3
release="https://github.com/godotengine/godot-builds/releases/download/${version}-stable"
engine_archive="Godot_v${version}-stable_linux.${arch}.zip"
template_archive="Godot_v${version}-stable_export_templates.tpz"
curl --fail --location --proto '=https' --proto-redir '=https' --retry 3 \
    "$release/SHA512-SUMS.txt" --output "$tools_dir/SHA512-SUMS.txt"
for archive in "$engine_archive" "$template_archive"; do
    expected="$(python3 - "$tools_dir/SHA512-SUMS.txt" "$archive" <<'PY'
import pathlib, re, sys
matches = [line.split()[0] for line in pathlib.Path(sys.argv[1]).read_text().splitlines()
           if len(line.split()) == 2 and line.split()[1].lstrip('*') == sys.argv[2]]
if len(matches) != 1 or not re.fullmatch('[a-fA-F0-9]{128}', matches[0]):
    raise SystemExit('Official checksum entry missing or ambiguous')
print(matches[0])
PY
)"
    if [[ ! -f "$tools_dir/$archive" ]] || ! printf '%s  %s\n' "$expected" "$tools_dir/$archive" | sha512sum --check --status; then
        curl --fail --location --proto '=https' --proto-redir '=https' --retry 3 --continue-at - \
            "$release/$archive" --output "$tools_dir/$archive.part"
        printf '%s  %s\n' "$expected" "$tools_dir/$archive.part" | sha512sum --check --status
        mv -- "$tools_dir/$archive.part" "$tools_dir/$archive"
    fi
done
unzip -oq "$tools_dir/$engine_archive" -d "$tools_dir/engine-$version"
engine="$tools_dir/engine-$version/Godot_v${version}-stable_linux.${arch}"
chmod +x "$engine"
[[ "$("$engine" --version)" == 4.6.3.stable* ]] || { echo 'Engine version mismatch.' >&2; exit 1; }
ln -sfn "$engine" "$tools_dir/godot"
unzip -oq "$tools_dir/$template_archive" -d "$tools_dir/template-stage"
[[ "$(cat "$tools_dir/template-stage/templates/version.txt")" == 4.6.3.stable ]] || { echo 'Template version mismatch.' >&2; exit 1; }
mkdir -p "$XDG_DATA_HOME/godot/export_templates/4.6.3.stable"
cp -a "$tools_dir/template-stage/templates/." "$XDG_DATA_HOME/godot/export_templates/4.6.3.stable/"
if [[ "${1:-}" == --graphics ]]; then
    "$ROOT/scripts/setup-graphics.sh"
fi
"$engine" --headless --path "$ROOT" --editor --import --quit
echo 'Godot 4.6.3 and matching export templates installed and checksum verified.'
