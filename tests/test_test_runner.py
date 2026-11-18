"""Routing checks for scripts/test.sh with stub toolchains; no cargo or WSL runs."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
RUST_STEPS = [
    "cargo fmt --all -- --check",
    "cargo check --workspace --all-targets --locked",
    "cargo test --workspace --locked",
    "cargo clippy --workspace --all-targets --locked -- -D warnings",
]
WSL_CHECKS = [
    "wsl.exe -d {0} --exec /bin/true",
    "wsl.exe -d {0} --cd C:\\fake\\checkout --exec bash scripts/test.sh --wsl-preflight",
]
WSL_RUN = "wsl.exe -d {0} --cd C:\\fake\\checkout --exec bash scripts/test.sh --wsl-snapshot"
# Real host tools the runner needs; everything else is a stub, so host-installed
# wsl.exe cannot leak into the routing decisions under test.
HOST_TOOLS = ["bash", "dirname", "git", "tar", "mktemp", "stat", "grep", "rm", "mkdir", "find", "sort", "cp", "env", "tr"]
STUBS = {
    "uname": 'echo "${STUB_UNAME:-Linux}"\n',
    "rustc": "echo 'rustc 1.99.1 (stub)'\n",
    "cygpath": "echo 'C:\\fake\\checkout'\n",
    "cargo": 'echo "cargo $*" >>"$STUB_LOG"\n',
    "python3": (
        'echo "python3 $*" >>"$STUB_LOG"\n'
        'if [[ -n "${STUB_RECORD:-}" && ! -e "$STUB_RECORD" ]]; then\n'
        '  { echo "cwd=$PWD"; echo "tmp=$TMPDIR"; echo "autocrlf=${GIT_CONFIG_KEY_0:-}=${GIT_CONFIG_VALUE_0:-}";\n'
        '    find . -type f | LC_ALL=C sort; } >"$STUB_RECORD"\n'
        "fi\n"
        'if [[ -n "${STUB_COPY:-}" && -e tracked.txt ]]; then command -p cp tracked.txt "$STUB_COPY"; fi\n'
        'exit "${STUB_PY_EXIT:-0}"\n'
    ),
    "wsl.exe": (
        'echo "wsl.exe $*" >>"$STUB_LOG"\n'
        'case "$*" in\n'
        '  *"--exec /bin/true"*) [[ " $* " == *" -d ${STUB_WSL_MISSING:-<none>} "* ]] && exit 1 || exit 0 ;;\n'
        '  *--wsl-preflight*)\n'
        '    if [[ "${STUB_WSL_REAL_PREFLIGHT:-0}" == 1 ]]; then\n'
        '      STUB_UNAME=Linux exec bash "$STUB_RUNNER" --wsl-preflight\n'
        '    else exit "${STUB_WSL_PREFLIGHT_EXIT:-0}"; fi ;;\n'
        "esac\n"
        'exit "${STUB_WSL_EXIT:-0}"\n'
    ),
}


@unittest.skipUnless(os.name == "posix", "the runner's Linux half executes on POSIX hosts only")
class TestRunnerRoutingTests(unittest.TestCase):
    def setUp(self):
        self.bash = shutil.which("bash")
        self.assertIsNotNone(self.bash, "bash is required")
        temporary = tempfile.TemporaryDirectory(prefix="lhr-runner-test-")
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name)
        self.repo = self.base / "checkout"
        (self.repo / "scripts").mkdir(parents=True)
        for name in ("test.sh", "common.sh"):
            data = (ROOT / "scripts" / name).read_bytes()
            self.assertNotIn(b"\r", data, name + " must be checked out with LF")
            (self.repo / "scripts" / name).write_bytes(data)
        self.stubs = self.base / "stubs"
        self.stubs.mkdir()
        for tool in HOST_TOOLS:
            found = shutil.which(tool)
            self.assertIsNotNone(found, tool + " is required")
            (self.stubs / tool).symlink_to(found)
        for name, body in STUBS.items():
            path = self.stubs / name
            path.write_text("#!" + self.bash + "\n" + body)
            path.chmod(0o700)
        self.log = self.base / "calls.log"
        self.env = {
            "PATH": str(self.stubs),
            "HOME": str(self.base / "home"),
            "TMPDIR": str(self.base),
            "LHR_TOOLS_DIR": str(self.base / "tools"),
            "STUB_LOG": str(self.log),
        }

    def run_runner(self, *args, **env):
        return subprocess.run([self.bash, str(self.repo / "scripts/test.sh"), *args], env={**self.env, **env},
                              capture_output=True, text=True, timeout=60)

    def calls(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def assert_nothing_ran_after_failed_preflight(self):
        self.assertFalse(any(c.startswith(("cargo", "python3")) or "--wsl-snapshot" in c for c in self.calls()))

    def test_linux_host_runs_python_natively_without_wsl(self):
        result = self.run_runner()
        self.assertEqual(result.returncode, 0, result.stderr)
        calls = self.calls()
        self.assertEqual(calls[:4], RUST_STEPS)
        self.assertEqual(len(calls), 4 + 10)
        self.assertTrue(all(call.startswith("python3 ") for call in calls[4:]))
        self.assertIn("python3 -m unittest discover -s tests -p test_test_runner.py -v", calls)
        self.assertIn("python3 -m unittest discover -s tests -p test_rewrite_metadata.py -v", calls)
        self.assertIn("python3 -m unittest discover -s tests -p test_rewrite_qualifiers.py -v", calls)

    def test_python_failure_stops_linux_runner(self):
        result = self.run_runner(STUB_PY_EXIT="3")
        self.assertEqual(result.returncode, 3)
        self.assertEqual(sum(call.startswith("python3") for call in self.calls()), 1)

    def test_windows_host_checks_wsl_first_keeps_native_rust_then_routes_python(self):
        result = self.run_runner(STUB_UNAME="MINGW64_NT-10.0-26200")
        self.assertEqual(result.returncode, 0, result.stderr)
        expected = [c.format("Debian") for c in WSL_CHECKS] + RUST_STEPS + [WSL_RUN.format("Debian")]
        self.assertEqual(self.calls(), expected)

    def test_distro_override_is_used_for_every_wsl_call(self):
        result = self.run_runner(STUB_UNAME="MSYS_NT-10.0", LHR_WSL_DISTRO="Ubuntu-24.04")
        self.assertEqual(result.returncode, 0, result.stderr)
        expected = [c.format("Ubuntu-24.04") for c in WSL_CHECKS] + RUST_STEPS + [WSL_RUN.format("Ubuntu-24.04")]
        self.assertEqual(self.calls(), expected)

    def test_linux_side_exit_status_propagates(self):
        result = self.run_runner(STUB_UNAME="MINGW64_NT-10.0", STUB_WSL_EXIT="5")
        self.assertEqual(result.returncode, 5)
        self.assertEqual(self.calls()[-1], WSL_RUN.format("Debian"))

    def test_missing_distro_fails_before_cargo(self):
        result = self.run_runner(STUB_UNAME="MINGW64_NT-10.0", STUB_WSL_MISSING="Debian")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("'Debian' is not available", result.stderr)
        self.assertIn("LHR_WSL_DISTRO", result.stderr)
        self.assert_nothing_ran_after_failed_preflight()

    def test_missing_linux_tools_fail_before_cargo(self):
        result = self.run_runner(STUB_UNAME="MINGW64_NT-10.0", STUB_WSL_PREFLIGHT_EXIT="1")
        self.assertEqual(result.returncode, 1)
        self.assert_nothing_ran_after_failed_preflight()

    def test_missing_wsl_fails_without_native_python_fallback(self):
        (self.stubs / "wsl.exe").unlink()
        result = self.run_runner(STUB_UNAME="MINGW64_NT-10.0")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("wsl.exe not found", result.stderr)
        self.assert_nothing_ran_after_failed_preflight()

    def test_crlf_script_is_reported_before_cargo_and_left_untouched(self):
        (self.repo / "scripts/run.sh").write_bytes(b"#!/usr/bin/env bash\r\necho\r\n")
        result = self.run_runner(STUB_UNAME="MINGW64_NT-10.0")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("scripts/run.sh", result.stderr)
        self.assert_nothing_ran_after_failed_preflight()
        self.assertEqual((self.repo / "scripts/run.sh").read_bytes(), b"#!/usr/bin/env bash\r\necho\r\n")

    def test_missing_grep_fails_line_ending_check_before_cargo(self):
        path = self.repo / "scripts/test.sh"
        before = path.read_bytes()
        (self.stubs / "grep").unlink()
        result = self.run_runner(STUB_UNAME="MINGW64_NT-10.0")
        self.assertEqual(result.returncode, 1)
        self.assertIn("grep is required to check script line endings", result.stderr)
        self.assert_nothing_ran_after_failed_preflight()
        self.assertEqual(path.read_bytes(), before)

    def test_grep_failure_is_not_accepted_as_lf_before_cargo(self):
        path = self.repo / "scripts/test.sh"
        before = path.read_bytes()
        grep = self.stubs / "grep"
        grep.unlink()
        grep.write_text("#!" + self.bash + "\nexit 2\n")
        grep.chmod(0o700)
        result = self.run_runner(STUB_UNAME="MINGW64_NT-10.0")
        self.assertEqual(result.returncode, 2)
        self.assertIn("Cannot check script line endings", result.stderr)
        self.assertIn("grep exit 2", result.stderr)
        self.assert_nothing_ran_after_failed_preflight()
        self.assertEqual(path.read_bytes(), before)

    def test_each_missing_linux_suite_tool_fails_before_cargo(self):
        for tool in ("find", "sort", "cp", "env", "rm", "mkdir", "tr"):
            with self.subTest(tool=tool):
                path = self.stubs / tool
                target = path.readlink()
                path.unlink()
                try:
                    result = self.run_runner(STUB_UNAME="MINGW64_NT-10.0", STUB_WSL_REAL_PREFLIGHT="1",
                                             STUB_RUNNER=str(self.repo / "scripts/test.sh"))
                    self.assertEqual(result.returncode, 1, result.stderr)
                    self.assertIn("Missing in this Linux environment: " + tool + ".", result.stderr)
                    self.assert_nothing_ran_after_failed_preflight()
                finally:
                    path.symlink_to(target)

    def test_unknown_arguments_are_rejected(self):
        self.assertEqual(self.run_runner("--bogus").returncode, 2)
        self.assertEqual(self.run_runner("--python-only", "extra").returncode, 2)
        self.assertEqual(self.calls(), [])

    def test_linux_preflight_accepts_complete_and_names_missing_tools(self):
        self.assertEqual(self.run_runner("--wsl-preflight").returncode, 0)
        (self.stubs / "mktemp").unlink()
        (self.stubs / "stat").unlink()
        result = self.run_runner("--wsl-preflight")
        self.assertEqual(result.returncode, 1)
        self.assertIn("Missing in this Linux environment: mktemp stat.", result.stderr)
        self.assertIn("LHR_WSL_DISTRO", result.stderr)
        self.assertEqual(self.calls(), [])

    def test_linux_preflight_checks_grep_and_find(self):
        (self.stubs / "grep").unlink()
        (self.stubs / "find").unlink()
        result = self.run_runner("--wsl-preflight")
        self.assertEqual(result.returncode, 1)
        self.assertIn("Missing in this Linux environment: grep find.", result.stderr)
        self.assertIn("findutils", result.stderr)
        self.assertEqual(self.calls(), [])

    def git(self, *args):
        subprocess.run(["git", "-C", str(self.repo), "-c", "user.name=Synthetic",
                        "-c", "user.email=synthetic@example.invalid", *args],
                       check=True, capture_output=True, env={**self.env, "GIT_CONFIG_NOSYSTEM": "1"})

    def make_repository(self):
        (self.repo / ".gitignore").write_text("ignored.txt\n")
        (self.repo / "tracked.txt").write_bytes(b"crlf stays\r\n")
        (self.repo / "deleted.txt").write_text("gone\n")
        self.git("init", "-q")
        self.git("add", ".")
        self.git("commit", "-qm", "synthetic")
        (self.repo / "deleted.txt").unlink()
        (self.repo / "untracked.txt").write_text("new\n")
        (self.repo / "ignored.txt").write_text("private\n")

    def test_wsl_snapshot_runs_exact_working_tree_from_private_linux_copy(self):
        self.make_repository()
        record = self.base / "record.txt"
        result = self.run_runner("--wsl-snapshot", STUB_RECORD=str(record))
        self.assertEqual(result.returncode, 0, result.stderr)
        lines = record.read_text().splitlines()
        cwd = Path(lines[0].removeprefix("cwd="))
        tmp = Path(lines[1].removeprefix("tmp="))
        self.assertEqual(cwd.name, "tree")
        self.assertEqual(tmp, cwd.parent / "tmp")
        self.assertTrue(cwd.parent.name.startswith("lhr-tests."))
        self.assertEqual(lines[2], "autocrlf=core.autocrlf=false")
        self.assertEqual(lines[3:], ["./.gitignore", "./scripts/common.sh", "./scripts/test.sh",
                                     "./tracked.txt", "./untracked.txt"])
        self.assertFalse(cwd.parent.exists(), "private snapshot must be removed")
        self.assertEqual(sum(call.startswith("python3") for call in self.calls()), 10)
        self.assertFalse(any(call.startswith("cargo") for call in self.calls()))

    def test_wsl_snapshot_preserves_working_tree_bytes(self):
        self.make_repository()
        copied = self.base / "copied.txt"
        result = self.run_runner("--wsl-snapshot", STUB_COPY=str(copied))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(copied.read_bytes(), b"crlf stays\r\n")

    def test_wsl_snapshot_propagates_failure_and_cleans_up(self):
        self.make_repository()
        record = self.base / "record.txt"
        result = self.run_runner("--wsl-snapshot", STUB_RECORD=str(record), STUB_PY_EXIT="4")
        self.assertEqual(result.returncode, 4)
        private = Path(record.read_text().splitlines()[0].removeprefix("cwd=")).parent
        self.assertFalse(private.exists())

    def test_wsl_snapshot_rejects_crlf_scripts(self):
        self.make_repository()
        (self.repo / "scripts/run.sh").write_bytes(b"#!/usr/bin/env bash\r\n")
        result = self.run_runner("--wsl-snapshot")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("scripts/run.sh", result.stderr)
        self.assertFalse(any(call.startswith("python3") for call in self.calls()))


if __name__ == "__main__":
    unittest.main()
