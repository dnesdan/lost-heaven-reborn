"""Runner tests use authored PNGs and fake renders, NOT a GPU or game capture."""
from pathlib import Path
import json
import os
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, call, patch
import zlib

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import capture_lab as lab


def chunk(kind, payload):
    return (struct.pack('>I', len(payload)) + kind + payload
            + struct.pack('>I', zlib.crc32(kind + payload) & 0xffffffff))


def png():
    return (lab.PNG_SIGNATURE
            + chunk(b'IHDR', struct.pack('>IIBBBBB', 1, 1, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(b'\0\x10\x20\x30'))
            + chunk(b'IEND', b''))


class CaptureTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / 'repo'
        self.repo.mkdir()
        self.out = self.root / 'capture'
        self.git('init', '-q')
        self.git('config', 'user.name', 'Synthetic Test')
        self.git('config', 'user.email', 'test@example.invalid')
        self.git('config', 'commit.gpgsign', 'false')
        (self.repo / 'scripts').mkdir()
        (self.repo / 'scripts/run.sh').write_text('# synthetic test placeholder\n')
        self.git('add', '.')
        self.git('commit', '-qm', 'synthetic source')
        self.calls = []

    def git(self, *args):
        return subprocess.run(['git', '-C', str(self.repo), *args], check=True,
                              capture_output=True).stdout

    def render(self, command, repo, log, timeout):
        self.calls.append(command)
        self.assertEqual(repo, self.repo.resolve())
        self.assertGreater(timeout, 0)
        Path(command[-1]).write_bytes(png())
        log.write_text('synthetic runner log; no renderer executed\n')

    def capture(self, scenes=None):
        return lab.capture(self.repo, self.out, scenes or ['courtyard'], 30)

    def test_all_scenes_and_receipt_without_private_paths(self):
        with patch.object(lab, 'run_capture', self.render):
            result = self.capture(list(lab.SCENES))
        stored = json.loads((self.out / 'receipt.json').read_text())
        self.assertEqual(result, stored)
        self.assertEqual([row['scene'] for row in result['images']], list(lab.SCENES))
        self.assertEqual(result['source_commit'], self.git('rev-parse', 'HEAD').decode().strip())
        self.assertFalse(result['original_game_parity'])
        self.assertIn('pixels NOT decoded or reviewed', result['validation'])
        self.assertNotIn(str(self.root), json.dumps(stored))
        for scene, command, row in zip(lab.SCENES, self.calls, result['images']):
            self.assertEqual(command[2:-1], [
                '--synthetic', '--lab-scene', scene, '--headless', '--capture',
            ])
            self.assertEqual((row['width'], row['height']), (1, 1))
            self.assertEqual(len(row['sha256']), 64)
            self.assertEqual(command[-1], str(self.out / f'{scene}.png'))

    def test_duplicate_scenes_are_captured_once(self):
        with patch.object(lab, 'run_capture', self.render):
            result = self.capture(['lights', 'lights'])
        self.assertEqual(len(result['images']), 1)
        self.assertEqual(len(self.calls), 1)

    def test_existing_output_is_not_overwritten(self):
        self.out.mkdir()
        marker = self.out / 'courtyard.png'
        marker.write_bytes(b'keep existing capture')
        with patch.object(lab, 'run_capture', self.render):
            with self.assertRaises(FileExistsError):
                self.capture()
        self.assertEqual(marker.read_bytes(), b'keep existing capture')
        self.assertFalse(self.calls)

    def test_output_inside_repository_is_refused(self):
        with self.assertRaises(lab.CaptureError):
            lab.capture(self.repo, self.repo / 'docs/screenshots/new', ['lights'], 30)
        self.assertFalse((self.repo / 'docs').exists())

    def test_dirty_source_is_refused_before_output_creation(self):
        for staged in (False, True):
            with self.subTest(staged=staged):
                (self.repo / 'scripts/run.sh').write_text('# modified\n')
                if staged:
                    self.git('add', '.')
                with self.assertRaises(lab.CaptureError):
                    self.capture()
                self.assertFalse(self.out.exists())

    def test_untracked_source_is_refused(self):
        (self.repo / 'new.rs').write_text('fn main() {}\n')
        with self.assertRaises(lab.CaptureError):
            self.capture()
        self.assertFalse(self.out.exists())

    def test_assume_unchanged_and_skip_worktree_are_refused(self):
        for flag, undo in [('--assume-unchanged', '--no-assume-unchanged'),
                           ('--skip-worktree', '--no-skip-worktree')]:
            with self.subTest(flag=flag):
                self.git('update-index', flag, 'scripts/run.sh')
                with self.assertRaises(lab.CaptureError):
                    self.capture()
                self.git('update-index', undo, 'scripts/run.sh')
        self.assertFalse(self.out.exists())

    def test_changed_source_during_capture_gets_no_receipt(self):
        def changed(*args):
            self.render(*args)
            self.git('commit', '--allow-empty', '-qm', 'changed source')
        with patch.object(lab, 'run_capture', changed):
            with self.assertRaises(lab.CaptureError):
                self.capture()
        self.assertFalse((self.out / 'receipt.json').exists())
        self.assertTrue((self.out / 'courtyard.png').exists())

    def test_success_exit_without_a_png_is_not_a_capture(self):
        with patch.object(lab, 'run_capture', return_value=None):
            with self.assertRaises(lab.CaptureError):
                self.capture()
        self.assertFalse((self.out / 'receipt.json').exists())

    def test_failed_render_preserves_local_output_without_success_receipt(self):
        def failed(*args):
            self.render(*args)
            raise lab.CaptureError('synthetic engine failure')
        with patch.object(lab, 'run_capture', failed):
            with self.assertRaises(lab.CaptureError):
                self.capture()
        self.assertTrue((self.out / 'courtyard.png').exists())
        self.assertFalse((self.out / 'receipt.json').exists())

    def test_unknown_scene_empty_suite_and_invalid_timeout_are_refused(self):
        for scenes, timeout in [(['../private'], 30), ([], 30), (['lights'], 0),
                                (['lights'], float('inf')), (['lights'], float('nan'))]:
            with self.subTest(scenes=scenes, timeout=timeout):
                with self.assertRaises(lab.CaptureError):
                    lab.capture(self.repo, self.out, scenes, timeout)
                self.assertFalse(self.out.exists())

    def test_png_validation_rejects_truncation_checksum_and_trailing_data(self):
        image = self.root / 'test.png'
        good = png()
        corrupt = bytearray(good)
        corrupt[29] ^= 1
        for payload in [b'', b'not PNG', good[:-1], good[:-12], good + b'x', bytes(corrupt)]:
            with self.subTest(length=len(payload)):
                image.write_bytes(payload)
                with self.assertRaises(lab.CaptureError):
                    lab.png_info(image)
        image.write_bytes(good)
        self.assertEqual(lab.png_info(image)['width'], 1)

    @unittest.skipUnless(os.name == 'posix', 'symlink check')
    def test_png_symlink_is_refused(self):
        real = self.root / 'real.png'
        real.write_bytes(png())
        link = self.root / 'link.png'
        link.symlink_to(real)
        with self.assertRaises(lab.CaptureError):
            lab.png_info(link)

    @unittest.skipUnless(os.name == 'posix', 'POSIX process group helper')
    def test_nonzero_process_status_is_not_success(self):
        with self.assertRaises(lab.CaptureError):
            lab.run_capture([sys.executable, '-c', 'raise SystemExit(9)'],
                            self.repo, self.root / 'exit.log', 10)

    @unittest.skipUnless(os.name == 'posix', 'POSIX process group helper')
    def test_timeout_terminates_owned_capture_process(self):
        with self.assertRaises(lab.CaptureError):
            lab.run_capture([sys.executable, '-c', 'import time; time.sleep(30)'],
                            self.repo, self.root / 'timeout.log', 0.05)

    @unittest.skipUnless(os.name == 'posix', 'POSIX process group helper')
    def test_termination_waits_are_bounded(self):
        process = Mock(pid=123456)
        process.wait.side_effect = subprocess.TimeoutExpired('synthetic', 3)
        with patch.object(lab.os, 'killpg') as kill:
            with self.assertRaises(lab.CaptureError):
                lab.stop_owned_group(process)
        self.assertEqual(kill.call_args_list, [
            call(process.pid, lab.signal.SIGTERM), call(process.pid, lab.signal.SIGKILL),
        ])
        self.assertEqual(process.wait.call_args_list, [call(timeout=3), call(timeout=3)])


if __name__ == '__main__':
    unittest.main()
