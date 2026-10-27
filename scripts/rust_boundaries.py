#!/usr/bin/env python3
"""Lexical Rust-boundary triage. This is not a Rust parser or a safety proof.

Inventory the working tree, or --check --base COMMIT to reject newly observed
boundary tokens relative to an immutable commit. Comments and literals are
excluded; tests, inactive cfg branches and macro bodies are intentionally included.
No baseline is written and no command is executed from repository text.
"""
from __future__ import annotations

import argparse
from bisect import bisect_right
from collections import Counter
from dataclasses import asdict, dataclass
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys


@dataclass(frozen=True)
class Finding:
    path: str
    line: int
    kind: str
    token: str


class AuditError(Exception):
    pass


# Character literals, not lifetime annotations such as 'a or 'static.
CHAR = re.compile(r"'(?:[^'\\\n]|\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|[^\n]))'")
RAW = re.compile(r'(?:br|cr|r)(\#*)"')
IDENT = re.compile(r'(?:[^\W\d]|_)\w*', re.UNICODE)
NUMBER = re.compile(r'[0-9][a-zA-Z0-9_]*')
REGISTERS = {'eax', 'ebx', 'ecx', 'edx', 'esi', 'edi', 'esp', 'ebp', 'eip'}
BOUNDARIES = {'sound_virtual', 'native_this', 'game_init_stack', 'FRAME_BASE', 'NAME_BASE'}
RAW_CALLS = {
    'transmute', 'transmute_copy', 'from_raw', 'from_raw_parts',
    'from_raw_parts_mut', 'into_raw', 'read_volatile', 'write_volatile',
}


def tokens(source: str):
    """Yield (text, line); omit literal contents, retaining raw-identifier spelling."""
    newlines = [-1] + [i for i, char in enumerate(source) if char == '\n']
    i = 0
    while i < len(source):
        char = source[i]
        if char.isspace():
            i += 1
            continue
        if source.startswith('//', i):
            end = source.find('\n', i + 2)
            i = len(source) if end < 0 else end
            continue
        if source.startswith('/*', i):
            depth = 1
            i += 2
            while depth and i < len(source):
                if source.startswith('/*', i):
                    depth += 1
                    i += 2
                elif source.startswith('*/', i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            if depth:
                raise AuditError('unterminated block comment')
            continue
        raw = RAW.match(source, i)
        if raw:
            end = source.find('"' + raw[1], raw.end())
            if end < 0:
                raise AuditError('unterminated raw string')
            i = end + 1 + len(raw[1])
            continue
        if char == '"':
            i += 1
            while i < len(source) and source[i] != '"':
                i += 2 if source[i] == '\\' else 1
            if i >= len(source):
                raise AuditError('unterminated string')
            i += 1
            continue
        literal = CHAR.match(source, i) if char == "'" else None
        if literal:
            i = literal.end()
            continue
        if source.startswith('r#', i):
            raw_identifier = IDENT.match(source, i + 2)
            if raw_identifier:
                yield source[i:raw_identifier.end()], bisect_right(newlines, i)
                i = raw_identifier.end()
                continue
        match = IDENT.match(source, i) or NUMBER.match(source, i)
        end = match.end() if match else i + 1
        yield source[i:end], bisect_right(newlines, i)
        i = end


def scan(path: str, source: str) -> list[Finding]:
    stream = list(tokens(source))
    found = []
    for index, (token, line) in enumerate(stream):
        after = stream[index + 1][0] if index + 1 < len(stream) else ''
        is_raw_identifier = token.startswith('r#')
        token = token[2:] if is_raw_identifier else token
        kind = None
        if token == 'unsafe' and not is_raw_identifier:
            kind = 'unsafe-keyword'
        elif token in {'asm', 'global_asm', 'naked_asm'} and after == '!':
            kind = 'assembly-macro'
        elif token == '*' and after in {'const', 'mut'}:
            kind, token = 'raw-pointer-type', '*' + after
        elif token in RAW_CALLS:
            kind = 'raw-memory-symbol'
        elif token in BOUNDARIES or token.endswith('VTableId'):
            kind = 'compatibility-symbol'
        elif token in REGISTERS:
            kind = 'cpu-register-name'
        if kind:
            found.append(Finding(path, line, kind, token))
    return found


def git(root: Path, *args: str) -> bytes:
    process = subprocess.run(
        ['git', '-C', str(root), *args], capture_output=True, check=False,
    )
    if process.returncode:
        raise AuditError(f'git {args[0]} failed: ' + process.stderr.decode('utf-8', 'replace').strip())
    return process.stdout


def paths(data: bytes) -> list[str]:
    result = []
    for raw in data.split(b'\0'):
        if not raw:
            continue
        name = raw.decode('utf-8')
        parts = PurePosixPath(name).parts
        if not parts or parts[0] != 'crates' or '..' in parts or name.startswith('/'):
            raise AuditError('source path is not repository-relative')
        if name.endswith('.rs'):
            result.append(name)
    return sorted(set(result))


def working_sources(root: Path) -> dict[str, str]:
    # A sparse checkout can otherwise look like many ordinary tracked deletions.
    for entry in git(root, 'ls-files', '-t', '-z', '--', 'crates').split(b'\0'):
        if entry.startswith(b'S ') and entry.endswith(b'.rs'):
            raise AuditError('skip-worktree Rust sources prevent a complete working-tree audit')
    names = paths(git(root, 'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', 'crates'))
    result = {}
    for name in names:
        file = root / name
        if any(parent.is_symlink() for parent in [file, *file.parents] if parent != root):
            raise AuditError(f'symlink source is not audited: {name}')
        if not file.exists():  # A tracked deletion is absent from the working tree.
            continue
        result[name] = file.read_text(encoding='utf-8')
    if not result:
        raise AuditError('no Rust sources found under crates; refusing an empty audit')
    return result


def baseline_sources(root: Path, ref: str) -> tuple[str, dict[str, str]]:
    if not ref or ref.startswith('-'):
        raise AuditError('invalid base ref')
    commit = git(root, 'rev-parse', '--verify', '--end-of-options', ref + '^{commit}').decode('ascii').strip()
    if not re.fullmatch(r'[0-9a-f]{40}|[0-9a-f]{64}', commit):
        raise AuditError('base did not resolve to one commit')
    entries = git(root, 'ls-tree', '-r', '-z', commit, '--', 'crates').split(b'\0')
    sources = {}
    for entry in entries:
        if not entry:
            continue
        metadata, name_bytes = entry.split(b'\t', 1)
        names = paths(name_bytes + b'\0')
        if not names:
            continue
        mode, kind, blob = metadata.split()
        if mode not in {b'100644', b'100755'} or kind != b'blob':
            raise AuditError(f'nonregular baseline source: {names[0]}')
        sources[names[0]] = git(root, 'cat-file', 'blob', blob.decode('ascii')).decode('utf-8')
    if not sources:
        raise AuditError('base has no Rust sources under crates')
    return commit, sources


def inventory(sources: dict[str, str]) -> list[Finding]:
    result = []
    for path, source in sorted(sources.items()):
        try:
            result.extend(scan(path, source))
        except AuditError as error:
            raise AuditError(f'{path}: {error}') from error
    return result


def additions(current: list[Finding], previous: list[Finding]) -> list[Finding]:
    """Per-file token multiset, not proof that preexisting uses are unchanged."""
    remaining = Counter((f.path, f.kind, f.token) for f in previous)
    added = []
    for finding in current:
        key = finding.path, finding.kind, finding.token
        if remaining[key]:
            remaining[key] -= 1
        else:
            added.append(finding)
    return added


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument('--base', help='existing commit/ref; resolved once before comparison')
    parser.add_argument('--check', action='store_true', help='exit 1 on token-count increases; requires --base')
    args = parser.parse_args(argv)
    if args.check and not args.base:
        parser.error('--check requires --base')
    try:
        root = args.root.resolve(strict=True)
        top = Path(git(root, 'rev-parse', '--show-toplevel').decode('utf-8').strip()).resolve()
        if top != root:
            raise AuditError('--root must be the repository root')
        sources = working_sources(root)
        current = inventory(sources)
        commit, added = None, []
        if args.base:
            commit, before = baseline_sources(root, args.base)
            added = additions(current, inventory(before))
        print(json.dumps({
            'scope': 'lexical triage; includes tests, macros and inactive cfg; not a safety proof',
            'comparison': 'per-file token counts; replacement of an existing use can escape detection',
            'base_commit': commit,
            'scanned_files': len(sources),
            'findings': [asdict(f) for f in current],
            'added': [asdict(f) for f in added],
        }, indent=2))
        return int(args.check and bool(added))
    except (AuditError, OSError, UnicodeError, ValueError) as error:
        print(f'rust boundary audit: {error}', file=sys.stderr)
        return 2


if __name__ == '__main__':
    sys.exit(main())
