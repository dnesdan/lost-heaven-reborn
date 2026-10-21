"""Capture lifecycle regression tests use only short-lived, task-owned processes."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from scripts import capture


@unittest.skipUnless(sys.platform == "linux" and hasattr(os, "pidfd_open"),
                     "capture recovery requires Linux pidfds")
class CaptureOwnershipTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="lhr-capture-test-", dir="/tmp")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.registry = self.root / "registry"
        self.registry.mkdir()
        self.child = self.process()

    def process(self):
        process = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])

        def clean():
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=2)
        self.addCleanup(clean)
        return process

    def write_record(self, controller, child=None, owner="scripts/capture.py"):
        output = self.root / "custom-output-outside-capture-default"
        output.mkdir(exist_ok=True)
        self.record = self.registry / "synthetic-run.json"
        self.record.write_text(json.dumps({
            "owner": owner, "controller": controller,
            "processes": [child or capture.identity(self.child.pid)],
            "output": str(output),
        }))

    def stale_controller(self):
        process = self.process()
        saved = capture.identity(process.pid)
        process.terminate()
        process.wait(timeout=2)
        return saved

    def test_live_controller_preserves_other_capture_child_and_record(self):
        self.write_record(capture.identity(os.getpid()))
        capture.recover(self.registry)
        self.assertIsNone(self.child.poll(), "another active capture must retain its child")
        self.assertTrue(self.record.exists(), "active capture ownership must remain registered")

    def test_stale_custom_output_capture_is_recovered_from_registry(self):
        self.write_record(self.stale_controller())
        capture.recover(self.registry)
        self.child.wait(timeout=2)
        self.assertEqual(self.child.returncode, -15)
        self.assertFalse(self.record.exists())

    def test_recycled_pid_identity_is_preserved(self):
        wrong = dict(capture.identity(self.child.pid))
        wrong["start_ticks"] = str(int(wrong["start_ticks"]) + 1)
        self.write_record(self.stale_controller(), child=wrong)
        capture.recover(self.registry)
        self.assertIsNone(self.child.poll(), "PID reuse must not authorize termination")

    def test_unknown_record_owner_is_preserved(self):
        self.write_record(self.stale_controller(), owner="unrelated-tool")
        capture.recover(self.registry)
        self.assertIsNone(self.child.poll())
        self.assertTrue(self.record.exists())


if __name__ == "__main__":
    unittest.main()
