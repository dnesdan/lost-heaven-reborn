#!/usr/bin/env python3
"""Capture public Engine Lab scenes with the real renderer and a source receipt.

Requires the pinned Rust toolchain, Bash and a working Vulkan/Metal adapter.
This does not draw substitute images. Outputs stay in a NEW directory outside
Git; review pixels before copying selected PNGs and receipt.json into a PR.
Raw build/runtime logs are local debugging files and must not be published.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import signal
import struct
import subprocess
import sys
import time
import zlib

SCENES = ('gallery', 'courtyard', 'primitives', 'lights')
PNG_SIGNATURE = b'\x89PNG\r\n\x1a\n'
MAX_PNG_BYTES = 64 * 1024 * 1024
TERMINATION_GRACE_SECONDS = 3
REAP_TIMEOUT_SECONDS = 3


class CaptureError(Exception):
    pass


def git(repo: Path, *args: str) -> bytes:
    result = subprocess.run(
        ['git', '-C', str(repo), *args], capture_output=True, check=False,
    )
    if result.returncode:
        raise CaptureError('Git source verification failed')
    return result.stdout


def source_revision(repo: Path) -> str:
    top = Path(os.fsdecode(git(repo, 'rev-parse', '--show-toplevel').rstrip(b'\n'))).resolve()
    if top != repo:
        raise CaptureError('capture must run from the repository root')
    # status alone can hide modified assume-unchanged or sparse/skip-worktree files.
    for entry in git(repo, 'ls-files', '-v', '-z').split(b'\0'):
        if entry and (entry[:1] == b'S' or entry[:1].islower()):
            raise CaptureError('sparse or assume-unchanged files prevent source verification')
    # A clean status only verifies a symlink's target *name*, not the code at
    # that target. Gitlinks likewise do not attest to nested working trees.
    # This runner deliberately supports ordinary tracked files only.
    for entry in git(repo, 'ls-files', '--stage', '-z').split(b'\0'):
        if not entry:
            continue
        metadata, separator, _name = entry.partition(b'\t')
        fields = metadata.split()
        if (not separator or len(fields) != 3
                or fields[0] not in {b'100644', b'100755'} or fields[2] != b'0'):
            raise CaptureError('symlink, submodule or unmerged sources prevent verification')
    if git(repo, 'status', '--porcelain=v1', '-z', '--untracked-files=all'):
        raise CaptureError('commit or set aside local changes before capturing')
    revision = git(repo, 'rev-parse', '--verify', 'HEAD^{commit}').decode('ascii').strip()
    if len(revision) not in (40, 64) or any(c not in '0123456789abcdef' for c in revision):
        raise CaptureError('invalid source commit')
    return revision


def png_info(path: Path) -> dict:
    """Check bounded PNG structure and CRCs, not decompressed pixels or appearance."""
    if path.is_symlink() or not path.is_file():
        raise CaptureError('renderer did not create a regular PNG')
    with path.open('rb') as image:
        data = image.read(MAX_PNG_BYTES + 1)
    if len(data) > MAX_PNG_BYTES or not data.startswith(PNG_SIGNATURE):
        raise CaptureError('invalid or oversized PNG')
    offset, size, has_data, ended = 8, None, False, False
    while offset < len(data):
        if len(data) - offset < 12:
            raise CaptureError('truncated PNG chunk')
        length = struct.unpack_from('>I', data, offset)[0]
        kind = data[offset + 4:offset + 8]
        end = offset + 12 + length
        if end > len(data):
            raise CaptureError('truncated PNG payload')
        payload = data[offset + 8:end - 4]
        checksum = struct.unpack_from('>I', data, end - 4)[0]
        if zlib.crc32(kind + payload) & 0xffffffff != checksum:
            raise CaptureError('PNG checksum mismatch')
        if offset == 8 and kind != b'IHDR':
            raise CaptureError('missing PNG header')
        if kind == b'IHDR':
            if offset != 8 or length != 13:
                raise CaptureError('invalid PNG header')
            size = struct.unpack_from('>II', payload)
            if not all(size):
                raise CaptureError('empty PNG dimensions')
        elif kind == b'IDAT':
            has_data = has_data or length > 0
        elif kind == b'IEND':
            if length or end != len(data):
                raise CaptureError('invalid PNG end')
            ended = True
        offset = end
    if size is None or not has_data or not ended:
        raise CaptureError('incomplete PNG')
    return {
        'file': path.name, 'width': size[0], 'height': size[1],
        'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(),
    }


def stop_owned_group(process: subprocess.Popen) -> None:
    # Requires our exclusive waiter and the default SIGCHLD disposition (checked
    # before spawning). Never poll/wait between the two group signals: reaping
    # the session leader permits its numeric PID/PGID to be reused.
    if process.returncode is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        # Give descendants the same bounded grace even if the leader exits
        # immediately. Leave that child unreaped until signalling is complete.
        time.sleep(TERMINATION_GRACE_SECONDS)
    finally:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=REAP_TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired as error:
            raise CaptureError('capture process did not stop after termination') from error


def run_capture(command: list[str], repo: Path, log: Path, timeout: float) -> None:
    if os.name != 'posix':
        raise CaptureError('this Bash capture runner supports Linux and macOS')
    if signal.getsignal(signal.SIGCHLD) != signal.SIG_DFL:
        raise CaptureError('capture requires default SIGCHLD handling and an exclusive waiter')
    with log.open('xb') as output:
        process = subprocess.Popen(
            command, cwd=repo, stdout=output, stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        try:
            code = process.wait(timeout=timeout)
        except subprocess.TimeoutExpired as error:
            stop_owned_group(process)
            raise CaptureError('capture timed out; inspect the local log') from error
        except BaseException:
            stop_owned_group(process)
            raise
    if code:
        raise CaptureError(f'renderer/build exited with status {code}; inspect the local log')


def capture(repo: Path, output: Path, scenes: list[str], timeout: float) -> dict:
    repo, output = repo.resolve(strict=True), output.resolve()
    scenes = list(dict.fromkeys(scenes))
    if not scenes or any(scene not in SCENES for scene in scenes):
        raise CaptureError('select public Lab scenes only')
    if not math.isfinite(timeout) or timeout <= 0:
        raise CaptureError('timeout must be finite and positive')
    if output == repo or repo in output.parents:
        raise CaptureError('capture output must be outside the repository')
    revision = source_revision(repo)
    output.mkdir(parents=True, exist_ok=False)
    images = []
    for scene in scenes:
        filename = output / f'{scene}.png'
        arguments = ['--synthetic', '--lab-scene', scene, '--headless', '--capture']
        run_capture(
            ['bash', str(repo / 'scripts/run.sh'), *arguments, str(filename)],
            repo, output / f'{scene}.log', timeout,
        )
        if source_revision(repo) != revision:
            raise CaptureError('source changed during capture; no receipt was issued')
        images.append({'scene': scene, **png_info(filename)})
    receipt = {
        'schema': 1,
        'source_commit': revision,
        'created_utc': datetime.now(timezone.utc).isoformat(),
        'platform': {'system': platform.system(), 'machine': platform.machine()},
        'runner': 'scripts/run.sh; default features; locked Cargo dependencies',
        'mode': 'public synthetic Lab scenes; headless; no UI panel; time zero',
        'validation': 'successful process; PNG framing/CRCs; pixels NOT decoded or reviewed',
        'original_game_parity': False,
        'images': images,
    }
    with (output / 'receipt.json').open('x', encoding='utf-8') as target:
        json.dump(receipt, target, indent=2)
        target.write('\n')
    return receipt


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True, help='new directory outside Git')
    parser.add_argument('--scene', action='append', choices=SCENES, help='repeat; default: all four')
    parser.add_argument('--timeout', type=float, default=300, help='seconds per build/capture')
    args = parser.parse_args(argv)
    try:
        receipt = capture(Path(__file__).resolve().parents[1], args.output,
                          args.scene or list(SCENES), args.timeout)
    except (CaptureError, OSError, ValueError) as error:
        print(f'Lab capture failed: {error}', file=sys.stderr)
        return 1
    print(f"Captured {len(receipt['images'])} synthetic scenes; inspect pixels before publishing.")
    return 0


if __name__ == '__main__':
    sys.exit(main())
