#!/usr/bin/env bash
# Install official, pinned analysis tools outside the implementation checkout.
set -euo pipefail
umask 077
repo="$(cd -- "$(dirname -- "$0")/.." && pwd -P)"
[[ $# == 0 ]] || { echo 'Usage: scripts/setup_binary_analysis.sh' >&2; exit 2; }
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || { echo 'This installer supports Linux x86_64.' >&2; exit 1; }
for command in python3 git curl tar unzip node npm; do
    command -v "$command" >/dev/null || { echo "Missing prerequisite: $command" >&2; exit 1; }
done
tools="$(python3 - "$repo" "${LHR_TOOLS_DIR:-/workspace/lhr-tools}" <<'PY'
import pathlib, sys
repo, path = map(lambda s: pathlib.Path(s).resolve(), sys.argv[1:])
for candidate in [path, path / 'ghidra', path / 'rea', path / '.cache/npm-rea',
                  path / 'ghidra/ghidra_12.1.4_PUBLIC', path / 'ghidra/jdk-21.0.12.1+1']:
    resolved = candidate.resolve()
    if resolved == repo or repo in resolved.parents or any(ord(c) < 32 for c in str(resolved)):
        raise SystemExit('Analysis tools must be outside the implementation checkout, including through symlinks.')
print(path)
PY
)"
mkdir -p -- "$tools/ghidra" "$tools/.cache/npm-rea"
owned_partials=()
cleanup_partials() { local path; for path in "${owned_partials[@]}"; do rm -f -- "$path"; done; }
trap cleanup_partials EXIT
trap 'exit 130' INT TERM
download() {
    local url=$1 file=$2 expected=$3
    if [[ ! -f "$file" ]] || ! python3 - "$file" "$expected" <<'PY'
import hashlib, sys
with open(sys.argv[1], 'rb') as f:
    digest = hashlib.file_digest(f, 'sha256').hexdigest()
raise SystemExit(digest != sys.argv[2])
PY
    then
        local partial
        partial="$(mktemp "$tools/ghidra/.download.XXXXXXXX")"
        owned_partials+=("$partial")
        if ! curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' --retry 2 "$url" --output "$partial"; then
            rm -f -- "$partial"; return 1
        fi
        if ! python3 - "$partial" "$expected" <<'PY'
import hashlib, sys
with open(sys.argv[1], 'rb') as f:
    digest = hashlib.file_digest(f, 'sha256').hexdigest()
if digest != sys.argv[2]:
    raise SystemExit('Official artifact SHA-256 mismatch; artifact was not installed.')
PY
        then
            rm -f -- "$partial"; return 1
        fi
        mv -- "$partial" "$file"
    fi
}
ghidra_archive="$tools/ghidra/ghidra_12.1.4_PUBLIC_20260921.zip"
jdk_archive="$tools/ghidra/jdk21.tar.gz"
download 'https://github.com/NationalSecurityAgency/ghidra/releases/download/Ghidra_12.1.4_build/ghidra_12.1.4_PUBLIC_20260921.zip' "$ghidra_archive" ddac49f903da9d5bac833e5cc79395098b9c33cfd3279be5f31bd00387d2d4db
download 'https://github.com/adoptium/temurin21-binaries/releases/download/jdk-21.0.12.1%2B1/OpenJDK21U-jdk_x64_linux_hotspot_21.0.12.1_1.tar.gz' "$jdk_archive" ce79869e1307ed8ee1e2baa86a412b1eb5b75d10a01006d788a6f968bcfaee94
export GHIDRA_INSTALL_DIR="$tools/ghidra/ghidra_12.1.4_PUBLIC"
export JAVA_HOME="$tools/ghidra/jdk-21.0.12.1+1"
if [[ ! -d "$GHIDRA_INSTALL_DIR" ]]; then
    stage="$(mktemp -d "$tools/ghidra/.extract.XXXXXXXX")"
    unzip -q "$ghidra_archive" -d "$stage"
    mv -- "$stage/ghidra_12.1.4_PUBLIC" "$GHIDRA_INSTALL_DIR"
    rmdir -- "$stage"
fi
if [[ ! -d "$JAVA_HOME" ]]; then
    stage="$(mktemp -d "$tools/ghidra/.extract.XXXXXXXX")"
    tar --no-same-owner -xzf "$jdk_archive" -C "$stage"
    mv -- "$stage/jdk-21.0.12.1+1" "$JAVA_HOME"
    rmdir -- "$stage"
fi
revision=6d356b84344659085f2b912857b5e27fe0160c9b
if [[ ! -e "$tools/rea" ]]; then
    git clone -- https://github.com/morluto/rea.git "$tools/rea"
    git -C "$tools/rea" checkout --detach "$revision"
fi
[[ $(git -C "$tools/rea" rev-parse HEAD) == "$revision" ]] || { echo 'Existing REA revision differs; preserve it and select another external LHR_TOOLS_DIR.' >&2; exit 1; }
[[ -z $(git -C "$tools/rea" status --porcelain --untracked-files=no) ]] || { echo 'Existing REA tracked files are modified; preserving them.' >&2; exit 1; }
node -e 'const [a,b]=process.versions.node.split(".").map(Number); if(!((a===22&&b>=19)||(a===24&&b>=11)||a>=26)) process.exit(1)' || { echo 'REA requires Node 22.19+, 24.11+, or 26+ within those supported major lines.' >&2; exit 1; }
if [[ ! -f "$tools/rea/dist/cli-runtime/cli.js" || ! -d "$tools/rea/node_modules" ]] || ! (cd -- "$tools/rea" && npm run deps:check >/dev/null); then
    (cd -- "$tools/rea" && npm ci --cache "$tools/.cache/npm-rea" --no-audit --no-fund && npm run build:cached)
fi
export GHIDRA_HEADLESS_MAXMEM=6G
node "$tools/rea/scripts/rea.mjs" doctor --provider ghidra --json > "$tools/ghidra/doctor.json"
python3 - "$tools/ghidra/doctor.json" <<'PY'
import json, sys
if json.load(open(sys.argv[1])).get('healthy') is not True:
    raise SystemExit('Ghidra readiness failed; inspect the external doctor.json.')
PY
echo 'Pinned REA, Ghidra and full JDK are installed; provider health passed. Run a private binary analysis to verify the workflow.'
