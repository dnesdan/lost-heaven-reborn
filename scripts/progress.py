#!/usr/bin/env python3
"""Validate one public snapshot and render progress offline; --check never writes."""
import argparse
from datetime import date
from html import escape
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
START = '<!-- generated-function-progress:start -->'
END = '<!-- generated-function-progress:end -->'
README_DASHBOARD = '<p align="center"><img src="docs/progress.svg" alt="Progress dashboard: static retained pseudocode, reviewed scoped Rust entries, and strict system verification" width="100%"></p>'
SHA = re.compile(r'[0-9a-f]{64}\Z')
RVA = re.compile(r'0x[0-9a-f]+\Z')


def scoped_summary(ports):
    summary = []
    for row in ports:
        item = {'contract_id': row['contract']['id'], 'source_commit': row['source_commit'],
                'entries': len(row['functions']), 'cases': row['cases']}
        # Older frozen history rows remain unchanged; new snapshots retain runtime state.
        if 'integration_receipt' in row:
            item['integration'] = row['integration']
            item['integration_receipt_sha256'] = (
                row['integration_receipt']['sha256'] if row['integration_receipt'] else None)
        summary.append(item)
    return summary


def validate_scoped_ports(ports, libraries):
    """Scoped replacements do not confer whole-original-function acceptance."""
    if not isinstance(ports, list):
        raise ValueError('Scoped ports must be a list')
    inventories = {row['binary_sha256']: row['inventory_membership_sha256'] for row in libraries}
    entries, contract_ids = set(), set()
    for port in ports:
        required = {'name', 'source_path', 'source_commit', 'source_sha256', 'functions', 'contract',
                    'comparison', 'reviews', 'cases', 'scope', 'integration'}
        allowed = required | {'integration_receipt'}
        if not required <= set(port) or not set(port) <= allowed:
            raise ValueError('Unexpected scoped-port fields')
        if not re.fullmatch(r'crates/[a-z0-9-]+/src/[a-z0-9_]+\.rs', port['source_path']):
            raise ValueError('Invalid scoped-port public source path')
        if not re.fullmatch(r'[0-9a-f]{40}', port['source_commit']) or not SHA.fullmatch(port['source_sha256']):
            raise ValueError('Invalid scoped-port source identity')
        for field in ['name', 'scope']:
            if not isinstance(port[field], str) or not port[field].strip() or any(c in port[field] for c in '\r\n|[]<>'):
                raise ValueError('Invalid scoped-port text')
        if type(port['cases']) is not int or port['cases'] <= 0 or port['integration'] not in {'not_integrated', 'static_inspection'}:
            raise ValueError('Scoped-port cases/integration require explicit bounded evidence')
        receipt = port.get('integration_receipt')
        if port['integration'] == 'static_inspection':
            if not isinstance(receipt, dict) or set(receipt) != {'id', 'sha256'}:
                raise ValueError('Static inspection requires a hashed integration receipt')
            if not re.fullmatch(r'[A-Za-z0-9_-]+', receipt['id']) or not SHA.fullmatch(receipt['sha256']):
                raise ValueError('Invalid static integration receipt identity')
        elif receipt is not None:
            raise ValueError('Unintegrated scoped ports cannot claim a static integration receipt')
        if not isinstance(port['reviews'], list) or not isinstance(port['functions'], list):
            raise ValueError('Scoped-port functions/reviews must be lists')
        for evidence in [port['contract'], port['comparison'], *port['reviews']]:
            if set(evidence) != {'id', 'sha256'} or not re.fullmatch(r'[A-Za-z0-9_-]+', evidence['id']) or not SHA.fullmatch(evidence['sha256']):
                raise ValueError('Invalid scoped-port evidence identity')
        if len(port['reviews']) != 2 or len({r['id'] for r in port['reviews']}) != 2 or len({r['sha256'] for r in port['reviews']}) != 2:
            raise ValueError('Scoped ports require two distinct reviews')
        if port['contract']['id'] in contract_ids or not port['functions']:
            raise ValueError('Duplicate or empty scoped contract')
        contract_ids.add(port['contract']['id'])
        for function in port['functions']:
            if set(function) != {'binary_sha256', 'entry_rva', 'rust_symbol', 'inventory_membership_sha256'}:
                raise ValueError('Unexpected scoped function fields')
            if not RVA.fullmatch(function['entry_rva']):
                raise ValueError('Invalid scoped function RVA')
            key = (function['binary_sha256'], int(function['entry_rva'], 16))
            membership = function['inventory_membership_sha256']
            if key in entries or not SHA.fullmatch(membership) or membership != inventories.get(key[0]):
                raise ValueError('Duplicate or unproven scoped function entry')
            if not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)+', function['rust_symbol']):
                raise ValueError('Invalid Rust symbol')
            entries.add(key)


def validate(snapshot):
    if set(snapshot) != {'schema_version', 'snapshot_id', 'as_of', 'source_baseline', 'recount_sha256', 'libraries', 'systems', 'history', 'scoped_ports'}:
        raise ValueError('Unexpected public snapshot fields')
    if snapshot['schema_version'] != 2:
        raise ValueError('Unsupported progress schema')
    if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', snapshot['as_of']):
        raise ValueError('Invalid as-of date')
    date.fromisoformat(snapshot['as_of'])
    if not re.fullmatch(r'[0-9a-f]{40}', snapshot['source_baseline']) or not SHA.fullmatch(snapshot['recount_sha256']):
        raise ValueError('Invalid source/evidence identity')
    seen = set()
    for library in snapshot['libraries']:
        if set(library) != {'name', 'binary_sha256', 'inventory_version', 'inventory_membership_sha256', 'inventory_total', 'retained_entries', 'retained_pseudocode', 'semantic_contracts', 'rust_ported', 'differential_verified', 'verified_code_bytes', 'scope', 'accepted_functions'}:
            raise ValueError('Unexpected public library fields')
        binary = library['binary_sha256']
        if not SHA.fullmatch(binary) or binary in seen:
            raise ValueError('Invalid or duplicate binary identity')
        seen.add(binary)
        total = library['inventory_total']
        entries = library['retained_entries']
        if any(not RVA.fullmatch(entry) for entry in entries) or len(entries) != len(set(int(x, 16) for x in entries)):
            raise ValueError('Invalid or duplicate canonical entries')
        if type(library['retained_pseudocode']) is not int or library['retained_pseudocode'] != len(entries):
            raise ValueError('Retained count disagrees with canonical entries')
        accepted = library['accepted_functions']
        if library['verified_code_bytes'] is not None:
            raise ValueError('Code-byte credits require an audited byte ledger; use null')
        accepted_keys = set()
        counts = {'semantic_contracts': 0, 'rust_ported': 0, 'differential_verified': 0}
        for function in accepted:
            if set(function) != {'entry_rva', 'inventory_evidence', 'contract_evidence', 'rust_commit', 'test_evidence', 'review_ids', 'runtime_evidence'}:
                raise ValueError('Unexpected accepted-function fields')
            entry = function['entry_rva']
            if not RVA.fullmatch(entry) or int(entry, 16) in accepted_keys:
                raise ValueError('Duplicate or invalid accepted function')
            accepted_keys.add(int(entry, 16))
            if not function['inventory_evidence'] or not function['contract_evidence']:
                raise ValueError('Accepted function requires inventory and whole contract evidence')
            counts['semantic_contracts'] += 1
            if function['rust_commit'] is not None:
                if not re.fullmatch(r'[0-9a-f]{40}', function['rust_commit']) or not function['test_evidence'] or len(set(function['review_ids'])) < 2:
                    raise ValueError('Port acceptance requires implementation, tests and two reviews')
                counts['rust_ported'] += 1
            if function['runtime_evidence']:
                if function['rust_commit'] is None:
                    raise ValueError('Runtime acceptance requires a Rust port')
                counts['differential_verified'] += 1
        for field, count in counts.items():
            if count and library[field] is None:
                raise ValueError('Accepted function credit cannot be hidden by an unknown count')
            if library[field] is not None and library[field] != count:
                raise ValueError('Whole-function count disagrees with accepted ledger')
        if total is not None:
            if type(total) is not int or total <= 0 or len(entries) > total:
                raise ValueError('Invalid inventory denominator')
            if not SHA.fullmatch(library['inventory_membership_sha256'] or ''):
                raise ValueError('Missing inventory fingerprint')
        for field in ['semantic_contracts', 'rust_ported', 'differential_verified']:
            value = library[field]
            if value is not None and (type(value) is not int or value < 0 or total is None or value > total):
                raise ValueError('Invalid full-function numerator')
        ported, verified = library['rust_ported'], library['differential_verified']
        if verified is not None and (ported is None or verified > ported):
            raise ValueError('Runtime acceptance requires accepted Rust ports')
    systems = snapshot['systems']
    if systems['total'] != 12 or type(systems['verified']) is not int or not 0 <= systems['verified'] <= 12:
        raise ValueError('Invalid full-system registry')
    history = snapshot['history']
    if len({row['snapshot_id'] for row in history}) != len(history):
        raise ValueError('Duplicate history snapshot')
    for row in history:
        date.fromisoformat(row['as_of'])
        for port in row.get('scoped_ports', []):
            old_fields = {'contract_id', 'source_commit', 'entries', 'cases'}
            new_fields = old_fields | {'integration', 'integration_receipt_sha256'}
            if (set(port) not in (old_fields, new_fields) or
                    not re.fullmatch(r'[A-Za-z0-9_-]+', port['contract_id']) or
                    not re.fullmatch(r'[0-9a-f]{40}', port['source_commit'])):
                raise ValueError('Invalid historical scoped-port identity')
            if any(type(port[k]) is not int or port[k] <= 0 for k in ['entries', 'cases']):
                raise ValueError('Invalid historical scoped-port counts')
            if set(port) == new_fields:
                if port['integration'] not in {'not_integrated', 'static_inspection'}:
                    raise ValueError('Invalid historical scoped integration state')
                receipt_hash = port['integration_receipt_sha256']
                if port['integration'] == 'static_inspection':
                    if not SHA.fullmatch(receipt_hash or ''):
                        raise ValueError('Historical static inspection requires a hashed integration receipt')
                elif receipt_hash is not None:
                    raise ValueError('Historical unintegrated port cannot include an integration receipt')
        for library in row['libraries']:
            total, retained = library['inventory_total'], library['retained_pseudocode']
            if type(retained) is not int or retained < 0 or (total is not None and (type(total) is not int or total <= 0 or retained > total)):
                raise ValueError('Invalid historical static counts')
    latest = history[-1]
    expected = [{k: row[k] for k in ['name', 'inventory_total', 'retained_pseudocode', 'rust_ported', 'differential_verified']} for row in snapshot['libraries']]
    if latest['snapshot_id'] != snapshot['snapshot_id'] or latest['as_of'] != snapshot['as_of'] or latest['libraries'] != expected or latest['systems'] != systems:
        raise ValueError('Current snapshot and history disagree')
    validate_scoped_ports(snapshot['scoped_ports'], snapshot['libraries'])
    if latest.get('scoped_ports', []) != scoped_summary(snapshot['scoped_ports']):
        raise ValueError('Scoped-port snapshot and history disagree')
    # Public projection is strictly metadata. Reject accidental private locators.
    encoded = json.dumps(snapshot)
    if any(token in encoded.lower() for token in ['http://', 'https://', 'file://', 'pseudocode_text', 'decompiler_output']):
        raise ValueError('Private locator or source text in public projection')
    if re.search(r'(?:^|[\s"\'=(:])(?:/[a-zA-Z0-9_.~-]|[a-zA-Z]:[\\/])', encoded):
        raise ValueError('Private locator in public projection')
    if re.search(r'(?i)(?:password|api[_-]?key|access[_-]?token|secret)\s*[:=]|\bBearer\s+\S+', encoded):
        raise ValueError('Credential-like payload in public projection')


def ratio(value, total):
    if value is None:
        return 'N/A'
    if total is None:
        return f'{value} / N/A'
    return f'{value} / {total:,} ({100 * value / total:.2f}%)'


def render(snapshot, include_history=False):
    lines = [START, f"As of {snapshot['as_of']} · snapshot `{snapshot['snapshot_id']}`.", '']
    if not include_history:
        lines += [README_DASHBOARD, '']
    lines += [
             '| Original binary | Retained pseudocode | Full contracts | Full Rust ports | Differential runtime verified |',
             '| --- | ---: | ---: | ---: | ---: |']
    for row in snapshot['libraries']:
        lines.append(f"| {row['name']} | {ratio(row['retained_pseudocode'], row['inventory_total'])} | {ratio(row['semantic_contracts'], None)} | {ratio(row['rust_ported'], None)} | {ratio(row['differential_verified'], None)} |")
    gate = snapshot['systems']
    semantic_note = 'Full semantic-contract counts are unmeasured. ' if all(row['semantic_contracts'] is None for row in snapshot['libraries']) else ''
    lines += ['', f"Strict full-system gate: **{gate['verified']}/{gate['total']}**. These static activity ratios are not game-completion percentages. N/A means the whole-function acceptance audit is incomplete; rw_data has no frozen inventory denominator. LS3DF uses an explicit profile union. Accepted stages use absolute counts with an N/A denominator until eligible original functions are audited; raw analyzer inventories include externals and thunks. {semantic_note}Verified code-byte coverage is unmeasured. The campaign is not playable.", '',
              'Regenerate locally with `python3 scripts/progress.py --write`; verify without writes with `python3 scripts/progress.py --check`. Versioned snapshots are retained in `docs/progress.json`; milestone history is maintained in `docs/PROGRESS.md`.']
    ports = snapshot['scoped_ports']
    if ports:
        entry_count = sum(len(port['functions']) for port in ports)
        has_runtime_metadata = any('integration_receipt' in port for port in ports)
        static_count = sum(port['integration'] == 'static_inspection' for port in ports)
        unintegrated_count = len(ports) - static_count
        if has_runtime_metadata:
            lines += ['', f"Accepted scoped Rust ports: **{entry_count} original entries across {len(ports)} reviewed contracts**. Recorded runtime use: {static_count} contracts used in static inspection; {unintegrated_count} contracts with no integration recorded. These preserve the stated domains and do not increase the whole-function figures above.", '',
                      '| Rust contract | Original entries | Native-derived validation cases | Runtime use | Verified scope |',
                      '| --- | ---: | ---: | --- | --- |']
            for port in ports:
                path = ('../' if include_history else '') + port['source_path']
                runtime_use = 'Static inspection' if port['integration'] == 'static_inspection' else 'No integration recorded'
                lines.append(f"| [{port['name']}]({path}) | {len(port['functions'])} | {port['cases']} cases | {runtime_use} | {port['scope']} |")
            lines += ['', 'Runtime-use labels come from reviewed, hashed receipts. Scoped ports do not increase whole-function acceptance counts.']
        else:
            lines += ['', f"Accepted scoped Rust ports: **{entry_count} original entries across {len(ports)} reviewed contracts**. These preserve the stated domains and do not increase the whole-function figures above. Runtime integration status was not recorded in this snapshot.", '',
                      '| Rust contract | Original entries | Native-derived validation cases | Verified scope |',
                      '| --- | ---: | ---: | --- |']
            for port in ports:
                path = ('../' if include_history else '') + port['source_path']
                lines.append(f"| [{port['name']}]({path}) | {len(port['functions'])} | {port['cases']} cases | {port['scope']} |")
    if include_history:
        lines += ['', '| Frozen snapshot | As of | Retained static pseudocode | Scoped Rust entries | Complete systems |', '| --- | --- | --- | ---: | ---: |']
        for row in snapshot['history']:
            counts = '; '.join(f"{library['name']} {ratio(library['retained_pseudocode'], library['inventory_total'])}" for library in row['libraries'])
            scoped = sum(port['entries'] for port in row['scoped_ports']) if 'scoped_ports' in row else 'N/A'
            lines.append(f"| `{row['snapshot_id']}` | {row['as_of']} | {counts} | {scoped} | {row['systems']['verified']}/{row['systems']['total']} |")
    lines.append(END)
    return '\n'.join(lines)


def render_svg(snapshot):
    """Render an accessible, self-contained dashboard from the validated snapshot."""
    rows = {row['name']: row for row in snapshot['libraries']}
    game, ls3df = rows.get('Game.exe'), rows.get('LS3DF.dll')
    ports = snapshot['scoped_ports']
    entry_count = sum(len(port['functions']) for port in ports)
    has_runtime_metadata = any('integration_receipt' in port for port in ports)
    static_count = sum(port['integration'] == 'static_inspection' for port in ports)
    unintegrated_count = len(ports) - static_count
    systems = snapshot['systems']
    palette = {'ink': '#142b3b', 'muted': '#506577', 'paper': '#f3f7f8', 'card': '#ffffff',
               'line': '#d4e0e5', 'blue': '#176b87', 'green': '#33765a', 'amber': '#9a5a12',
               'track': '#e2ebee', 'softblue': '#e6f2f6', 'softgreen': '#e9f3ed', 'softamber': '#f8efe2'}

    def percentage(row):
        if row is None or row['inventory_total'] is None:
            return None
        return 100 * row['retained_pseudocode'] / row['inventory_total']

    def count_label(row):
        if row is None:
            return 'Unknown'
        total = row['inventory_total']
        return (f"{row['retained_pseudocode']:,} / {total:,} · {percentage(row):.2f}%"
                if total is not None else f"{row['retained_pseudocode']:,} / unknown")

    def card(x, y, label, value, detail, accent, percent=None):
        bar = ''
        if percent is not None:
            length = 320 * percent / 100
            if length > 0:
                bar = (f'<rect class="bar-track" x="{x+24}" y="{y+123}" width="320" height="10" rx="5" fill="{palette["track"]}"/>'
                       f'<rect x="{x+24}" y="{y+123}" width="{length:.2f}" height="10" rx="5" fill="{accent}"/>')
            else:
                bar = f'<rect class="bar-track" x="{x+24}" y="{y+123}" width="320" height="10" rx="5" fill="{palette["track"]}"/>'
        return (f'<g><rect class="card-bg" x="{x}" y="{y}" width="372" height="148" rx="16" fill="{palette["card"]}" stroke="{palette["line"]}"/>'
                f'<rect x="{x}" y="{y}" width="7" height="148" rx="3" fill="{accent}"/>'
                f'<text x="{x+24}" y="{y+39}" class="label">{escape(label)}</text>'
                f'<text x="{x+24}" y="{y+82}" class="value">{escape(value)}</text>'
                f'<text x="{x+24}" y="{y+105}" class="detail">{escape(detail)}</text>{bar}</g>')

    game_percent, ls_percent = percentage(game), percentage(ls3df)
    cards = ''.join([
        card(36, 112, 'GAME.EXE · STATIC RETAINED PSEUDOCODE', count_label(game), 'Default-profile inventory · not semantic coverage', palette['blue'], game_percent),
        card(432, 112, 'LS3DF.DLL · STATIC RETAINED PSEUDOCODE', count_label(ls3df), 'Explicit profile union · not semantic coverage', palette['green'], ls_percent),
        card(36, 278, 'REVIEWED SCOPED RUST PORTS', f'{entry_count} entries · {len(ports)} contracts',
             f'{static_count} used in static inspection · {unintegrated_count} unrecorded' if has_runtime_metadata else 'Absolute scoped counts · eligible denominator unknown', palette['amber']),
        card(432, 278, 'STRICT FULL-SYSTEM VERIFICATION', f"{systems['verified']} / {systems['total']}", 'Verified complete systems · strict evidence gate', palette['muted'])
    ])

    history = []
    previous_counts = None
    for item in snapshot['history']:
        libs = {row['name']: row for row in item['libraries']}
        g, l = libs.get('Game.exe'), libs.get('LS3DF.dll')
        if (g and g['inventory_total'] is not None and g['inventory_total'] > 0 and
                g['inventory_total'] == (game or {}).get('inventory_total') and
                l and l['inventory_total'] is not None and l['inventory_total'] > 0 and
                l['inventory_total'] == (ls3df or {}).get('inventory_total')):
            counts = (g['retained_pseudocode'], g['inventory_total'], l['retained_pseudocode'], l['inventory_total'])
            if counts != previous_counts:
                history.append(f"{item['as_of']}: Game {g['retained_pseudocode']:,}/{g['inventory_total']:,}; LS3DF {l['retained_pseudocode']:,}/{l['inventory_total']:,}")
                previous_counts = counts
    history_lines = ''.join(f'<text x="36" y="{496 + index*22}" class="history">{escape(line)}</text>' for index, line in enumerate(history))
    if not history_lines:
        history_lines = '<text x="36" y="496" class="history">No frozen snapshots match the current inventory denominators.</text>'
    footer_first = 496 + max(1, len(history)) * 22 + 8
    footer_second = footer_first + 18
    canvas_height = footer_second + 14
    date_label = escape(snapshot['as_of'])
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 840 {canvas_height}" role="img" aria-labelledby="dashboard-title dashboard-desc">
<title id="dashboard-title">Mafia Rust reconstruction progress evidence</title>
<desc id="dashboard-desc">As of {date_label}. Static retained pseudocode: {escape(count_label(game))} for Game.exe and {escape(count_label(ls3df))} for LS3DF.dll. {entry_count} reviewed scoped Rust entries across {len(ports)} contracts with an unknown eligible-function denominator{f'; {static_count} with recorded static inspection and {unintegrated_count} with no integration recorded' if has_runtime_metadata else ''}. Strict full-system verification: {systems['verified']} of {systems['total']}. These measures are separate and are not a game-completion percentage.</desc>
<style>
text{{font-family:Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;fill:{palette['ink']}}}
.kicker{{font-size:14px;font-weight:700;letter-spacing:1.1px;fill:{palette['muted']}}}
.title{{font-size:28px;font-weight:750;letter-spacing:-.35px}}
.label{{font-size:13px;font-weight:750;letter-spacing:.5px;fill:{palette['muted']}}}
.value{{font-size:25px;font-weight:750;letter-spacing:-.4px}}
.detail{{font-size:13px;fill:{palette['muted']}}}
.history{{font-size:13px;fill:{palette['muted']}}}
.dark-bg{{display:none}}
@media(prefers-color-scheme:dark){{.light-bg{{display:none}}.dark-bg{{display:inline}}text{{fill:#edf4f6}}.kicker,.label,.detail,.history{{fill:#b3c5ce}}.card-bg{{fill:#172b39;stroke:#3b5361}}.bar-track{{fill:#314957}}}}
</style>
<rect class="light-bg" width="840" height="{canvas_height}" rx="24" fill="{palette['paper']}"/>
<rect class="dark-bg" width="840" height="{canvas_height}" rx="24" fill="#0d1b25"/>
<text x="36" y="38" class="kicker">LOST HEAVEN REBORN · EVIDENCE SNAPSHOT · {date_label}</text>
<text x="36" y="75" class="title">Progress measures, separate denominators</text>
{cards}
<text x="36" y="469" class="kicker">FROZEN STATIC HISTORY · MATCHING INVENTORY DENOMINATORS</text>
{history_lines}
<text x="36" y="{footer_first}" class="detail">Eligible scoped-function denominator: unknown · scoped ports are not whole-function credits.</text>
<text x="36" y="{footer_second}" class="detail">Static analysis is not implementation coverage; full-system verification is not completion.</text>
</svg>'''


def replace_fragment(text, fragment, filename):
    if text.count(START) != 1 or text.count(END) != 1:
        raise ValueError(f'{filename}: expected exactly one generated progress block')
    before, rest = text.split(START)
    _, after = rest.split(END)
    return before + fragment + after


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--check', action='store_true')
    mode.add_argument('--write', action='store_true')
    args = parser.parse_args(argv)
    try:
        document = (ROOT / 'docs/progress.json').read_text()
        if '/workspace/' in document or '/tmp/' in document:
            raise ValueError('Private locator in progress document')
        snapshot = json.loads(document)['function_progress']
        validate(snapshot)
        changes = []
        for filename in ['README.md', 'docs/PROGRESS.md']:
            path = ROOT / filename
            old = path.read_text()
            fragment = render(snapshot, include_history=filename == 'docs/PROGRESS.md')
            new = replace_fragment(old, fragment, filename)
            if old != new:
                if args.write:
                    path.write_text(new)
                changes.append(filename)
        svg_path = ROOT / 'docs/progress.svg'
        svg = render_svg(snapshot)
        if not svg_path.exists() or svg_path.read_text() != svg:
            if args.write:
                svg_path.write_text(svg)
            changes.append('docs/progress.svg')
        if changes and args.check:
            print('Stale generated progress: ' + ', '.join(changes), file=sys.stderr)
            return 1
    except (ValueError, KeyError, TypeError, IndexError) as error:
        print(f'Invalid progress: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
