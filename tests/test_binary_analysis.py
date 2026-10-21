"""Synthetic checks of analysis privacy boundaries; these do not validate Ghidra."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest


ROOT = Path(__file__).resolve().parents[1]
WRAPPER = ROOT / "scripts/run_binary_analysis.sh"


class BinaryAnalysisBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="lhr-analysis-test-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.tools = self.base / "tools"
        (self.tools / "rea/scripts").mkdir(parents=True)
        (self.tools / "rea/scripts/rea.mjs").write_text("// synthetic entry marker\n")
        self.binary = self.base / "synthetic input"
        self.binary.write_bytes(b"synthetic data; never an original executable")
        commands = self.base / "commands"
        commands.mkdir()
        node = commands / "node"
        node.write_text("#!/usr/bin/env python3\nimport json, os, subprocess, sys, time\npayload = {'synthetic_arguments': sys.argv[1:]}\nif os.environ.get('SYNTHETIC_CHILD'):\n child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'], start_new_session=True)\n payload['child_pid'] = child.pid\nprint(json.dumps(payload), flush=True)\nif os.environ.get('SYNTHETIC_SLEEP'): time.sleep(60)\nsys.exit(int(os.environ.get('SYNTHETIC_EXIT', '0')))\n")
        node.chmod(0o700)
        self.env = dict(os.environ, PATH=str(commands) + os.pathsep + os.environ["PATH"], LHR_TOOLS_DIR=str(self.tools))

    def run_wrapper(self, *args):
        return subprocess.run([str(WRAPPER), *map(str, args)], env=self.env, capture_output=True, text=True, timeout=10)

    def test_private_output_literal_selector_and_permissions(self):
        output = self.base / "results/decompile.json"
        selector = "literal;$(touch SHOULD_NOT_EXIST)"
        result = self.run_wrapper("decompile", self.binary, selector, "--output", output)
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(output.read_text())
        self.assertIn("--procedure=" + selector, payload["synthetic_arguments"])
        self.assertEqual(output.stat().st_mode & 0o777, 0o600)
        record = json.loads(Path(str(output) + ".ownership.json").read_text())
        self.assertEqual(record["exit_code"], 0)
        self.assertTrue(record["source_unchanged"])
        self.assertFalse(record["runtime_verified"])
        self.assertFalse((ROOT / "SHOULD_NOT_EXIST").exists())

    def test_input_symlink_into_checkout_rejected(self):
        alias = self.base / "input-alias"
        alias.symlink_to(ROOT / "Cargo.toml")
        result = self.run_wrapper("analyze", alias, "--output", self.base / "output.json")
        self.assertEqual(result.returncode, 2)
        self.assertFalse((self.base / "output.json").exists())

    def test_output_parent_symlink_into_checkout_rejected(self):
        alias = self.base / "output-alias"
        alias.symlink_to(ROOT, target_is_directory=True)
        result = self.run_wrapper("analyze", self.binary, "--output", alias / "SHOULD_NOT_EXIST.json")
        self.assertEqual(result.returncode, 2)
        self.assertFalse((ROOT / "SHOULD_NOT_EXIST.json").exists())

    def test_output_file_symlink_into_checkout_rejected(self):
        alias = self.base / "output.json"
        alias.symlink_to(ROOT / "Cargo.toml")
        before = (ROOT / "Cargo.toml").read_bytes()
        result = self.run_wrapper("analyze", self.binary, "--output", alias)
        self.assertEqual(result.returncode, 2)
        self.assertEqual((ROOT / "Cargo.toml").read_bytes(), before)

    def test_existing_private_file_preserved(self):
        output = self.base / "output.json"
        output.write_text("preserve me")
        result = self.run_wrapper("analyze", self.binary, "--output", output)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(output.read_text(), "preserve me")

    def test_child_failure_status_preserved(self):
        self.env["SYNTHETIC_EXIT"] = "7"
        output = self.base / "failure.json"
        result = self.run_wrapper("analyze", self.binary, "--output", output)
        self.assertEqual(result.returncode, 7)
        self.assertEqual(json.loads(Path(str(output) + ".ownership.json").read_text())["exit_code"], 7)

    def test_setup_rejects_nested_tools_symlink_into_checkout(self):
        (self.tools / "ghidra").symlink_to(ROOT, target_is_directory=True)
        result = subprocess.run([str(ROOT / "scripts/setup_binary_analysis.sh")], env=self.env, capture_output=True, text=True, timeout=10)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("including through symlinks", result.stderr)

    def test_live_identity_persisted_and_sigterm_cleans_new_session_child(self):
        self.env.update(SYNTHETIC_CHILD="1", SYNTHETIC_SLEEP="1")
        output = self.base / "interrupted.json"
        process = subprocess.Popen([str(WRAPPER), "analyze", str(self.binary), "--output", str(output)], env=self.env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.addCleanup(lambda: process.kill() if process.poll() is None else None)
        ownership = Path(str(output) + ".ownership.json")
        deadline = time.monotonic() + 5
        record = {}
        while time.monotonic() < deadline:
            try:
                record = json.loads(ownership.read_text())
            except (FileNotFoundError, json.JSONDecodeError):
                pass
            if record.get("status") == "running" and len(record.get("children", [])) >= 2:
                break
            time.sleep(0.02)
        self.assertEqual(record.get("status"), "running")
        self.assertTrue(record.get("process_start_ticks"))
        self.assertGreaterEqual(len(record["children"]), 2)
        process.terminate()
        process.communicate(timeout=10)
        self.assertEqual(process.returncode, 130)
        final = json.loads(ownership.read_text())
        self.assertTrue(final["cleanup_complete"])
        self.assertEqual(final["status"], "cancelled")
        self.assertTrue(all(not Path('/proc/' + str(child['pid'])).exists() for child in final['children']))

    def test_orphan_child_cleaned_after_cli_parent_exits(self):
        self.env["SYNTHETIC_CHILD"] = "1"
        output = self.base / "orphan.json"
        result = self.run_wrapper("analyze", self.binary, "--output", output)
        self.assertEqual(result.returncode, 0, result.stderr)
        child_pid = json.loads(output.read_text())["child_pid"]
        self.assertFalse(Path('/proc/' + str(child_pid)).exists())
        self.assertTrue(json.loads(Path(str(output) + ".ownership.json").read_text())["cleanup_complete"])


if __name__ == "__main__":
    unittest.main()
