#!/usr/bin/env bash
# Keep target identities, pseudocode, logs, snapshots and process records private.
set -euo pipefail
repo="$(cd -- "$(dirname -- "$0")/.." && pwd -P)"
exec python3 - "$repo" "$@" <<'PY'
import argparse
import ctypes
import datetime
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import uuid

repo = Path(sys.argv.pop(1)).resolve()
parser = argparse.ArgumentParser(description='Run pinned REA/Ghidra; all results remain outside this checkout.')
parser.add_argument('operation', choices=['analyze', 'decompile', 'function', 'instructions', 'search', 'xrefs'])
parser.add_argument('binary', type=Path)
parser.add_argument('selector', nargs='?')
parser.add_argument('--output', type=Path, help='New private JSON file; existing files are never replaced')
args = parser.parse_args()

def external(path):
    path = path.expanduser().resolve()
    if path == repo or repo in path.parents:
        parser.error('Original binaries, tools and analysis output must be outside the implementation checkout, including through symlinks.')
    return path

target = external(args.binary)
if not target.is_file():
    parser.error('The selected binary must be an existing regular file.')
tools = external(Path(os.environ.get('LHR_TOOLS_DIR', '/workspace/lhr-tools')))
entry = tools / 'rea/scripts/rea.mjs'
if not entry.is_file():
    parser.error('REA is missing; run scripts/setup_binary_analysis.sh first.')
if args.operation == 'analyze' and args.selector is not None:
    parser.error('analyze does not accept a selector.')
if args.operation != 'analyze' and args.selector is None:
    parser.error('This operation requires an exact selector or search text.')

output = external(args.output or Path(os.environ.get('LHR_ANALYSIS_DIR', '/workspace/lhr-private/analysis/cli')) / (args.operation + '-' + uuid.uuid4().hex + '.json'))
if output.exists() or output.is_symlink():
    parser.error('Output already exists; select a new private file.')

# Walk canonical components using directory descriptors; refuse symlink replacement
# while creating missing parents. Child output is written through these held FDs.
def open_directory(path):
    fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    try:
        for component in path.parts[1:]:
            try:
                os.mkdir(component, mode=0o700, dir_fd=fd)
            except FileExistsError:
                pass
            next_fd = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = next_fd
        return fd
    except BaseException:
        os.close(fd)
        raise

os.umask(0o077)
directory = open_directory(output.parent)
descriptors = []
def create(name):
    fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=directory)
    descriptors.append(fd)
    return fd

def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()

env = os.environ.copy()
env.update(JAVA_HOME=str(tools / 'ghidra/jdk-21.0.12.1+1'), GHIDRA_INSTALL_DIR=str(tools / 'ghidra/ghidra_12.1.4_PUBLIC'), GHIDRA_HEADLESS_MAXMEM='6G', REA_GHIDRA_STARTUP_TIMEOUT_MS='1800000')
temp_root = output.parent / '.rea-tmp'
temp_fd = open_directory(temp_root)
os.close(temp_fd)
env.update(TMPDIR=str(temp_root), TMP=str(temp_root), TEMP=str(temp_root))
env.pop('REA_GHIDRA_SEED_FILE', None)
env.pop('REA_GHIDRA_DISABLED_ANALYZERS', None)
env.pop('REA_GHIDRA_LANGUAGE_ID', None)
env.pop('REA_GHIDRA_COMPILER_SPEC_ID', None)
command = ['node', str(entry), args.operation, str(target), '--provider', 'ghidra', '--json']
if args.selector is not None:
    selector_flag = {'search': '--pattern', 'xrefs': '--address'}.get(args.operation, '--procedure')
    command.append(selector_flag + '=' + args.selector)
allowed_cpus = sorted(os.sched_getaffinity(0))
requested = os.environ.get('LHR_ANALYSIS_CPUS')
try:
    cpus = {int(value) for value in requested.split(',')} if requested else set(allowed_cpus[:2])
except ValueError:
    parser.error('LHR_ANALYSIS_CPUS must be a comma-separated list of allowed CPU numbers.')
if not cpus or not cpus.issubset(allowed_cpus):
    parser.error('LHR_ANALYSIS_CPUS must select allowed CPUs.')

started = datetime.datetime.now(datetime.timezone.utc).isoformat()
metadata = {'binary_sha256': digest(target), 'operation': args.operation, 'selector': args.selector, 'started_utc': started, 'cpu_affinity': sorted(cpus), 'heap_limit': '6G', 'runtime_verified': False}
process = None
metadata_fd = None
known_children = {}
def identity(pid):
    try:
        fields = Path('/proc/' + str(pid) + '/stat').read_text().rsplit(') ', 1)[1].split()
        return {'pid': pid, 'parent_pid': int(fields[1]), 'process_group': int(fields[2]), 'process_start_ticks': fields[19], 'state': fields[0]}
    except (FileNotFoundError, ProcessLookupError, PermissionError):
        return None

def descendants():
    observed = {}
    for path in Path('/proc').iterdir():
        if path.name.isdigit():
            item = identity(int(path.name))
            if item:
                observed[item['pid']] = item
    owned = {os.getpid()}
    changed = True
    while changed:
        changed = False
        for item in observed.values():
            if item['parent_pid'] in owned and item['pid'] not in owned:
                owned.add(item['pid'])
                changed = True
    live = []
    for pid in owned - {os.getpid()}:
        item = observed[pid]
        known_children[(pid, item['process_start_ticks'])] = item
        if item['state'] != 'Z':
            live.append(item)
    metadata['children'] = list(known_children.values())
    return live

def persist():
    data = memoryview((json.dumps(metadata, indent=2) + '\n').encode())
    temporary = '.ownership-' + uuid.uuid4().hex + '.tmp'
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=directory)
    try:
        while data:
            data = data[os.write(fd, data):]
        os.fsync(fd)
        os.replace(temporary, output.name + '.ownership.json', src_dir_fd=directory, dst_dir_fd=directory)
        os.fsync(directory)
    finally:
        os.close(fd)
        try:
            os.unlink(temporary, dir_fd=directory)
        except FileNotFoundError:
            pass

def cleanup():
    # Subreaping keeps double-forked/orphaned provider processes attributable.
    # Every signal is checked against an observed process birth, never a name.
    for sig, allowance in [(None, 2), (signal.SIGINT, 30), (signal.SIGTERM, 10), (signal.SIGKILL, 5)]:
        deadline = time.monotonic() + allowance
        signalled = set()
        while True:
            live = descendants()
            if not live:
                return True
            if sig is not None:
                for child in live:
                    key = (child['pid'], child['process_start_ticks'])
                    current = identity(child['pid'])
                    if key not in signalled and current and current['process_start_ticks'] == child['process_start_ticks']:
                        try:
                            os.kill(child['pid'], sig)
                        except ProcessLookupError:
                            pass
                        signalled.add(key)
            metadata['cleanup_signal'] = sig
            persist()
            if time.monotonic() >= deadline:
                break
            time.sleep(0.1)
    return not descendants()

def terminate(signum, frame):
    raise KeyboardInterrupt
signal.signal(signal.SIGTERM, terminate)
try:
    stdout_fd = create(output.name)
    stderr_fd = create(output.name + '.stderr.log')
    metadata_fd = create(output.name + '.ownership.json')
    metadata.update(wrapper_pid=os.getpid(), wrapper_start_ticks=identity(os.getpid())['process_start_ticks'], status='starting')
    persist()
    # Linux PR_SET_CHILD_SUBREAPER: orphaned provider descendants reparent here.
    if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0) != 0:
        raise OSError(ctypes.get_errno(), 'Cannot establish owned process subreaper')
    # Affinity is inherited; no unsupported JVM injection flags are needed.
    os.sched_setaffinity(0, cpus)
    process = subprocess.Popen(command, env=env, stdout=stdout_fd, stderr=stderr_fd, start_new_session=True)
    metadata.update(pid=process.pid, process_group=process.pid, process_start_ticks=identity(process.pid)['process_start_ticks'], status='running')
    descendants()
    persist()
    try:
        deadline = time.monotonic() + 1920
        while process.poll() is None:
            count = len(known_children)
            descendants()
            if len(known_children) != count:
                persist()
            if time.monotonic() >= deadline:
                raise subprocess.TimeoutExpired(command, 1920)
            time.sleep(0.1)
        status = process.returncode
    except subprocess.TimeoutExpired:
        status = 124
        metadata['status'] = 'timed_out'
    except KeyboardInterrupt:
        status = 130
        metadata['status'] = 'cancelled'
    signal.signal(signal.SIGINT, signal.SIG_IGN)
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    metadata['cleanup_complete'] = cleanup()
    if not metadata['cleanup_complete']:
        status = 125
    if process.poll() is None:
        process.wait(timeout=1)
    while True:
        try:
            reaped, _ = os.waitpid(-1, os.WNOHANG)
            if reaped == 0:
                break
        except ChildProcessError:
            break
    metadata.update(exit_code=status, finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(), source_unchanged=digest(target) == metadata['binary_sha256'])
    metadata['status'] = 'completed' if status == 0 else (metadata['status'] if metadata.get('status') in ['cancelled', 'timed_out'] else 'failed')
    persist()
    for fd in descriptors:
        os.fsync(fd)
    if not metadata['source_unchanged']:
        raise SystemExit('The selected input changed during analysis; do not treat the result as a stable baseline.')
    print(output)
    raise SystemExit(status if status >= 0 else 128 - status)
except BaseException as error:
    if metadata_fd is not None and 'finished_utc' not in metadata:
        signal.signal(signal.SIGINT, signal.SIG_IGN)
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        metadata.update(status='exception', error_type=type(error).__name__)
        try:
            metadata['cleanup_complete'] = cleanup()
            if process is not None and process.poll() is None:
                process.wait(timeout=1)
        except Exception as cleanup_error:
            metadata.update(cleanup_complete=False, cleanup_error_type=type(cleanup_error).__name__)
        metadata['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        persist()
    raise
finally:
    for fd in descriptors:
        os.close(fd)
    os.close(directory)
PY
