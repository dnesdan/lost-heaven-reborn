#!/usr/bin/env bash
# Full verification. Rust checks run on the host toolchain (MSVC on Windows).
# The Python suites exercise POSIX behaviour (/proc, prctl, process groups,
# file modes, symlinks) and always run on Linux: natively on Linux hosts, and
# through an existing WSL distribution on Windows (Git Bash/MSYS) hosts. There
# is no native-Windows Python fallback; a missing WSL setup is a hard failure.
set -euo pipefail
source "$(dirname -- "$0")/common.sh"
cd "$ROOT"

usage() {
    echo 'usage: scripts/test.sh [--python-only | --wsl-snapshot]' >&2
    exit 2
}

run_python_suites() {
    python3 -m unittest discover -s tests -p 'test_binary_analysis.py' -v
    python3 -m unittest discover -s tests -p 'test_progress.py' -v
    python3 -m unittest discover -s tests -p 'test_rust_boundaries.py' -v
    python3 -m unittest discover -s tests -p 'test_capture_lab.py' -v
    python3 -m unittest discover -s tests -p 'test_rewrite_metadata.py' -v
    python3 -m unittest discover -s tests -p 'test_rewrite_qualifiers.py' -v
    python3 -m unittest discover -s tests -p 'test_rewrite_progress.py' -v
    python3 -m unittest discover -s tests -p 'test_test_runner.py' -v
    python3 scripts/progress.py --check
    python3 scripts/rewrite_progress.py --check
}

# Linux executes these files through their shebangs or sources them; a CR in
# the working copy breaks that. Report instead of rewriting the user's files.
require_lf_scripts() {
    local file status crlf=()
    command -v grep >/dev/null || { echo 'grep is required to check script line endings.' >&2; return 1; }
    for file in "$1"/scripts/*.sh; do
        # -U: Git Bash grep otherwise strips CR before matching.
        if grep -qU $'\r' "$file"; then
            crlf+=("scripts/${file##*/}")
        else
            status=$?
            if ((status != 1)); then
                echo "Cannot check script line endings: $file (grep exit $status)." >&2
                return "$status"
            fi
        fi
    done
    ((${#crlf[@]} == 0)) && return 0
    {
        echo "CRLF line endings in: ${crlf[*]}"
        echo '.gitattributes requires LF for *.sh. Re-checkout each unmodified file, e.g.'
        echo '  rm scripts/<name>.sh && git checkout -- scripts/<name>.sh'
        echo 'Save local edits first; use an LF-preserving editor for modified files.'
    } >&2
    return 1
}

is_windows_host() {
    case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) return 0 ;; esac
    return 1
}

# Windows side. The distribution is selected explicitly because the WSL default
# may be docker-desktop. All checks run before the (long) cargo steps.
WSL_DISTRO="${LHR_WSL_DISTRO:-Debian}"

wsl_run() {
    MSYS_NO_PATHCONV=1 wsl.exe -d "$WSL_DISTRO" --cd "$(cygpath -w "$ROOT")" --exec "$@"
}

wsl_preflight() {
    if ! command -v wsl.exe >/dev/null; then
        echo 'wsl.exe not found. The Python suites need Linux: install WSL with a distribution providing' >&2
        echo 'bash, python3, git, tar, grep, findutils and coreutils, then select it with LHR_WSL_DISTRO=<name> (default: Debian). See docs/RUNNING.md.' >&2
        return 1
    fi
    if ! MSYS_NO_PATHCONV=1 wsl.exe -d "$WSL_DISTRO" --exec /bin/true >/dev/null 2>&1; then
        echo "WSL distribution '$WSL_DISTRO' is not available (wsl.exe -l -v lists installed ones)." >&2
        echo 'Set LHR_WSL_DISTRO=<name> to a distribution providing bash, python3, git, tar, grep, findutils and coreutils. See docs/RUNNING.md.' >&2
        return 1
    fi
    require_lf_scripts "$ROOT"
    wsl_run bash scripts/test.sh --wsl-preflight
}

# Linux side of the WSL route.
require_linux_tools() {
    local missing=() tool
    for tool in bash dirname uname env python3 git tar mktemp stat grep rm mkdir find sort cp tr; do
        command -v "$tool" >/dev/null || missing+=("$tool")
    done
    ((${#missing[@]} == 0)) && return 0
    echo "Missing in this Linux environment: ${missing[*]}." >&2
    echo 'Install them in the selected WSL distribution (Debian/Ubuntu: sudo apt install bash python3 git tar grep findutils coreutils)' >&2
    echo 'or choose another distribution with LHR_WSL_DISTRO=<name>. See docs/RUNNING.md.' >&2
    return 1
}

# The checkout is reached through a 9p (drvfs) mount where every file reports
# mode 0777 and each path component costs a slow lstat (test_rewrite_progress
# did not finish in 10 minutes in place). Test the exact working tree (tracked
# and untracked, non-ignored files; bytes unchanged) from a private 0700 copy
# on the Linux filesystem instead.
run_from_wsl_snapshot() {
    local private fs list name
    require_linux_tools
    private="$(mktemp -d "${TMPDIR:-/tmp}/lhr-tests.XXXXXX")"
    LHR_TEST_SNAPSHOT_DIR="$private"
    trap 'rm -rf -- "$LHR_TEST_SNAPSHOT_DIR"' EXIT
    fs="$(stat -f -c %T "$private")"
    case "$fs" in v9fs | 9p | drvfs | fuseblk)
        echo "Private test directory $private is on $fs; set TMPDIR to a Linux filesystem." >&2
        return 1 ;;
    esac
    mkdir -m 700 "$private/tree" "$private/tmp"
    list="$private/files"
    # Read-only listing; never refresh the shared index from the Linux side.
    # Tracked files deleted in the working tree are left out of the snapshot.
    GIT_OPTIONAL_LOCKS=0 git -C "$ROOT" ls-files -z --cached --others --exclude-standard --deduplicate >"$list.all"
    while IFS= read -r -d '' name; do
        if [[ -e "$ROOT/$name" || -L "$ROOT/$name" ]]; then printf '%s\0' "$name"; fi
    done <"$list.all" >"$list"
    tar -C "$ROOT" --null -T "$list" -cf - | tar -C "$private/tree" -xf -
    require_lf_scripts "$private/tree"
    # Test temporaries stay private; temporary repositories must not inherit a
    # user-level autocrlf, so committed fixture bytes and hashes stay exact.
    TMPDIR="$private/tmp" GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.autocrlf GIT_CONFIG_VALUE_0=false \
        PYTHONDONTWRITEBYTECODE=1 bash "$private/tree/scripts/test.sh" --python-only
}

case "${1-}" in
    --python-only) (($# == 1)) || usage; run_python_suites; exit ;;
    --wsl-preflight) (($# == 1)) || usage; require_linux_tools; exit ;;
    --wsl-snapshot) (($# == 1)) || usage; run_from_wsl_snapshot; exit ;;
    '') (($# == 0)) || usage ;;
    *) usage ;;
esac

if is_windows_host; then wsl_preflight; fi
require_rust
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
if is_windows_host; then
    echo "Running Python suites in WSL distribution '$WSL_DISTRO'." >&2
    wsl_run bash scripts/test.sh --wsl-snapshot
else
    run_python_suites
fi
