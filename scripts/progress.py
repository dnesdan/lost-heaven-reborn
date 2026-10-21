#!/usr/bin/env python3
"""Validate one public snapshot and render progress offline; --check never writes."""
import argparse
from datetime import date
from html import escape
import hashlib
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
SCHEMA2_HISTORY_SHA256 = 'd4c2cb1a3b3d24c702ce63864f6bb3912f9c4bd443a3af89de37d17a9468bd80'
LEGACY_HISTORY_IDENTITIES = {
    # Each tuple is Game retained/total, LS3DF retained/total, then the
    # frozen source receipt that binds those counts to the profile identity below.
    'historical-retained-static-20261021': (9, 4914, 45, 1683, 'f2c7da62460776bbe723eda3d7bd6b2afa017dd2a96a3711382bd4dd1323b0ee'),
    'pc-function-r2-20261021': (55, 4914, 48, 1683, '9600f01bb93cd85ddfa537457a18387c292bccd913f9f3762754cc38c9a9c393'),
    'RUST-SCOPED-PORTS-20261021-01': (55, 4914, 48, 1683, '9600f01bb93cd85ddfa537457a18387c292bccd913f9f3762754cc38c9a9c393'),
    'RUST-SCOPED-PORTS-20261021-02': (55, 4914, 48, 1683, '9600f01bb93cd85ddfa537457a18387c292bccd913f9f3762754cc38c9a9c393'),
    'RUST-SCOPED-PORTS-20261021-03': (72, 4914, 55, 1683, 'c6dce332d276fefa8b2c655e4422ccabfa2626bd05892bec4a974b9d44a72415'),
    'RUST-SCOPED-PORTS-20261021-04': (72, 4914, 55, 1683, 'c6dce332d276fefa8b2c655e4422ccabfa2626bd05892bec4a974b9d44a72415'),
}
LEGACY_BRIDGE_BY_NAME = {
    'Game.exe': {'binary_sha256': '08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27',
                 'inventory_total': 4914, 'inventory_basis': 'raw_profile_inventory',
                 'inventory_profiles': ['7810ea9437da59780820ec6fedb47ce47390841a95d527ddb6c3b3700fe180cb'],
                 'inventory_membership_sha256': 'bd82dbdaecf1981548dd73c0793264b520dc08191fb7e8dc8cc8e3bc244e186b'},
    'LS3DF.dll': {'binary_sha256': 'e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6',
                  'inventory_total': 1683, 'inventory_basis': 'raw_profile_union',
                  'inventory_profiles': ['0c1148c7491bbcdf5b3a0f9ea8ba9aad2523b95135b48b9e8385e31b815e1c1a',
                                         '20f79d3fb10ba69c5463530c8267a6e2a8f5d0d7232ea59fe159849342ec5d7e',
                                         '46301268618f6f9f27a2856e15858155a42599f674def681aa6a56df9fe13c64',
                                         '5fb8141f282b82b7a78df6b2d631f00eb93729605b53b333297ebfb4ff03fe9f'],
                  'inventory_membership_sha256': '82e5a161d38e2dd48c71c4f2570a60f32a4328b226e23b6345713cc6ce601b60'},
}


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


def hash_history_prefix(rows):
    """Stable hash used to chain append-only snapshot history rows."""
    return hashlib.sha256(json.dumps(rows, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def schema2_history_bridge(row, name):
    """Return the pinned denominator identity for an immutable schema-2 history row."""
    record = LEGACY_HISTORY_IDENTITIES.get(row['snapshot_id'])
    if record is None or row['as_of'] != '2026-10-21':
        return None
    retained_index, total_index = (0, 1) if name == 'Game.exe' else (2, 3)
    if name not in {'Game.exe', 'LS3DF.dll'}:
        return None
    expected_retained, expected_total = record[retained_index], record[total_index]
    library = next((lib for lib in row['libraries'] if lib['name'] == name), None)
    if (library is None or library['inventory_total'] != expected_total or
            library['retained_pseudocode'] != expected_retained or not SHA.fullmatch(record[4])):
        return None
    return LEGACY_BRIDGE_BY_NAME[name]


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


def validate_v2(snapshot):
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


def validate_v3(snapshot):
    """Validate the additive executable inventory schema without rewriting v2 history."""
    required = {'schema_version', 'snapshot_id', 'as_of', 'source_baseline', 'recount_sha256',
                'libraries', 'systems', 'history', 'scoped_ports', 'other_targets'}
    if set(snapshot) != required or snapshot['schema_version'] != 3:
        raise ValueError('Unexpected schema-3 public snapshot fields')
    if not isinstance(snapshot['libraries'], list) or not isinstance(snapshot['other_targets'], list):
        raise ValueError('Schema-3 inventories must be lists')
    if len(snapshot['history']) < 7:
        raise ValueError('Schema-3 must append to the frozen schema-2 history')
    frozen_prefix_hash = hashlib.sha256(
        json.dumps(snapshot['history'][:6], sort_keys=True, separators=(',', ':')).encode()).hexdigest()
    if frozen_prefix_hash != SCHEMA2_HISTORY_SHA256:
        raise ValueError('Schema-2 history is frozen and cannot be modified')
    for prior in snapshot['history'][:6]:
        if prior['as_of'] != '2026-10-21' or prior['snapshot_id'] not in LEGACY_HISTORY_IDENTITIES:
            raise ValueError('Frozen schema-2 history lacks a pinned inventory-identity bridge')
        for name in ['Game.exe', 'LS3DF.dll']:
            if schema2_history_bridge(prior, name) is None:
                raise ValueError('Frozen schema-2 denominator disagrees with its pinned identity bridge')
    bases = {'raw_profile_inventory', 'raw_profile_union', 'unknown', 'in_image_non_thunk_procedures'}
    setup_membership = 'e0ccf8c5121747ba0608317068c72542b155fd2c758c784b518fd30f983e23df'
    names = set()
    for row in snapshot['libraries']:
        if set(row) != {'name', 'binary_sha256', 'inventory_version', 'inventory_membership_sha256',
                        'inventory_total', 'retained_entries', 'retained_pseudocode', 'semantic_contracts',
                        'rust_ported', 'differential_verified', 'verified_code_bytes', 'scope',
                        'accepted_functions', 'inventory_basis', 'inventory_profiles'}:
            raise ValueError('Unexpected schema-3 library fields')
        if row['name'] in names:
            raise ValueError('Duplicate schema-3 library name')
        names.add(row['name'])
        if row['inventory_basis'] not in bases:
            raise ValueError('Invalid inventory denominator basis')
        if row['inventory_basis'] == 'unknown':
            if row['inventory_total'] is not None or row['inventory_profiles'] != [] or row['inventory_membership_sha256'] is not None:
                raise ValueError('Unknown denominator must not carry a profile or membership percentage basis')
        elif (not isinstance(row['inventory_profiles'], list) or not row['inventory_profiles'] or
              any(not isinstance(profile, str) or not SHA.fullmatch(profile) for profile in row['inventory_profiles']) or
              len(row['inventory_profiles']) != len(set(row['inventory_profiles']))):
            raise ValueError('Known denominator requires exact profile identities')
        if row['inventory_basis'] == 'raw_profile_inventory' and len(row['inventory_profiles']) != 1:
            raise ValueError('Single-profile inventory basis requires exactly one profile identity')
        if row['inventory_basis'] == 'in_image_non_thunk_procedures':
            if (row['binary_sha256'] != 'c6abff923ecedafc26be3f1abdb8fe286f3a0be1e787ea7b659b6c01a077d25d' or
                    row['inventory_profiles'] != ['7810ea9437da59780820ec6fedb47ce47390841a95d527ddb6c3b3700fe180cb'] or
                    row['inventory_total'] != 172 or row['inventory_membership_sha256'] != setup_membership):
                raise ValueError('Setup procedure denominator must match the frozen 172-entry basis')
        elif row['inventory_basis'] != 'unknown' and row['inventory_total'] is None:
            raise ValueError('Known inventory basis requires a denominator')

    def validate_v3_inventory_identity(row, *, historical=False):
        expected = {'name', 'binary_sha256', 'inventory_total', 'retained_pseudocode',
                    'rust_ported', 'differential_verified', 'inventory_basis',
                    'inventory_profiles', 'inventory_membership_sha256'}
        if set(row) != expected:
            raise ValueError('Invalid schema-3 historical inventory identity')
        if (not isinstance(row['name'], str) or not row['name'].strip() or
                any(c in row['name'] for c in '\r\n|[]<>') or
                not isinstance(row['binary_sha256'], str) or not SHA.fullmatch(row['binary_sha256'])):
            raise ValueError('Invalid schema-3 historical binary identity')
        basis = row['inventory_basis']
        profiles = row['inventory_profiles']
        total, retained, membership = (row['inventory_total'], row['retained_pseudocode'],
                                       row['inventory_membership_sha256'])
        if basis not in bases:
            raise ValueError('Invalid schema-3 historical denominator basis')
        if type(retained) is not int or retained < 0:
            raise ValueError('Invalid schema-3 historical retained count')
        if basis == 'unknown':
            if total is not None or profiles != [] or membership is not None:
                raise ValueError('Unknown historical denominator must not carry an identity')
        else:
            if (type(total) is not int or total <= 0 or retained > total or
                    not isinstance(profiles, list) or not profiles or
                    any(not isinstance(profile, str) or not SHA.fullmatch(profile) for profile in profiles) or
                    len(profiles) != len(set(profiles)) or
                    not isinstance(membership, str) or not SHA.fullmatch(membership)):
                raise ValueError('Invalid known schema-3 historical inventory identity')
            if basis == 'raw_profile_inventory' and len(profiles) != 1:
                raise ValueError('Historical single-profile basis requires one profile')
            if basis == 'in_image_non_thunk_procedures':
                if (row['binary_sha256'] != 'c6abff923ecedafc26be3f1abdb8fe286f3a0be1e787ea7b659b6c01a077d25d' or
                        profiles != ['7810ea9437da59780820ec6fedb47ce47390841a95d527ddb6c3b3700fe180cb'] or
                        total != 172 or membership != setup_membership):
                    raise ValueError('Historical Setup denominator must match the frozen 172-entry basis')
        if historical:
            for field in ['rust_ported', 'differential_verified']:
                value = row[field]
                if value is not None and (type(value) is not int or value < 0 or total is None or value > total):
                    raise ValueError('Invalid schema-3 historical acceptance count')
            ported, verified = row['rust_ported'], row['differential_verified']
            if verified is not None and (ported is None or verified > ported):
                raise ValueError('Historical runtime count requires accepted Rust ports')

    # Validate all existing v2 invariants through its unchanged implementation. The
    # compatibility projection omits schema-3-only rows/fields; their own invariants
    # are checked below, so old snapshots and history remain byte-for-byte stable.
    legacy = json.loads(json.dumps(snapshot))
    legacy['schema_version'] = 2
    legacy.pop('other_targets')
    for row in legacy['libraries']:
        row.pop('inventory_basis'); row.pop('inventory_profiles')
    # The latest history projection includes all v3 rows. Strip its v3-only row fields
    # and Setup row before invoking the v2 validator; verify exact v3 agreement below.
    for historical in legacy['history'][6:]:
        historical.pop('history_prefix_sha256', None)
        for row in historical['libraries']:
            row.pop('inventory_basis', None); row.pop('inventory_profiles', None)
            row.pop('inventory_membership_sha256', None); row.pop('binary_sha256', None)
    validate_v2(legacy)

    if not snapshot['history'] or snapshot['history'][-1]['snapshot_id'] != snapshot['snapshot_id']:
        raise ValueError('Schema-3 current snapshot must be the latest frozen history row')
    latest = snapshot['history'][-1]
    latest_projection = [{k: row[k] for k in ['name', 'binary_sha256', 'inventory_total', 'retained_pseudocode',
                                                'rust_ported', 'differential_verified', 'inventory_basis',
                                                'inventory_profiles', 'inventory_membership_sha256']} for row in snapshot['libraries']]
    if latest['libraries'] != latest_projection:
        raise ValueError('Schema-3 latest history and current inventory bases disagree')
    for index in range(6, len(snapshot['history'])):
        row = snapshot['history'][index]
        required_history = {'snapshot_id', 'as_of', 'libraries', 'systems', 'scoped_ports', 'history_prefix_sha256'}
        if (set(row) != required_history or not isinstance(row['snapshot_id'], str) or
                not re.fullmatch(r'[A-Za-z0-9_-]+', row['snapshot_id']) or
                not isinstance(row['as_of'], str) or not re.fullmatch(r'\d{4}-\d{2}-\d{2}', row['as_of']) or
                not SHA.fullmatch(row['history_prefix_sha256'])):
            raise ValueError('Schema-3 history row requires a chained append receipt')
        date.fromisoformat(row['as_of'])
        prefix_hash = hashlib.sha256(json.dumps(snapshot['history'][:index], sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        if row['history_prefix_sha256'] != prefix_hash:
            raise ValueError('Schema-3 history append chain was modified')
        if not isinstance(row['libraries'], list) or not row['libraries']:
            raise ValueError('Schema-3 historical inventory must be a nonempty list')
        seen_names = set()
        for library in row['libraries']:
            validate_v3_inventory_identity(library, historical=True)
            if library['name'] in seen_names:
                raise ValueError('Duplicate schema-3 historical inventory name')
            seen_names.add(library['name'])
        if (not isinstance(row['scoped_ports'], list) or
                not isinstance(row['systems'], dict) or
                set(row['systems']) != {'verified', 'total'} or row['systems']['total'] != 12 or
                type(row['systems']['verified']) is not int or not 0 <= row['systems']['verified'] <= 12):
            raise ValueError('Invalid schema-3 historical systems or scoped ports')
        for port in row['scoped_ports']:
            if (not isinstance(port, dict) or set(port) != {'contract_id', 'source_commit', 'entries', 'cases',
                                                            'integration', 'integration_receipt_sha256'} or
                    not re.fullmatch(r'[A-Za-z0-9_-]+', port['contract_id']) or
                    not re.fullmatch(r'[0-9a-f]{40}', port['source_commit']) or
                    type(port['entries']) is not int or port['entries'] <= 0 or
                    type(port['cases']) is not int or port['cases'] <= 0 or
                    port['integration'] not in {'not_integrated', 'static_inspection'}):
                raise ValueError('Invalid schema-3 historical scoped-port identity')
            receipt_hash = port['integration_receipt_sha256']
            if ((port['integration'] == 'static_inspection' and not SHA.fullmatch(receipt_hash or '')) or
                    (port['integration'] == 'not_integrated' and receipt_hash is not None)):
                raise ValueError('Invalid schema-3 historical scoped integration receipt')
    targets = snapshot['other_targets']
    if len(targets) != 1:
        raise ValueError('Schema-3 requires exactly one separately inventoried installer target')
    target = targets[0]
    if set(target) != {'name', 'binary_sha256', 'category', 'status', 'inventory_profile',
                       'inventory_basis', 'inventory_membership_sha256', 'inventory_total',
                       'retained_pseudocode', 'retained_entries'}:
        raise ValueError('Unexpected installer target fields')
    if (target['name'] != 'Setup.exe (archive installer bootstrap)' or
            target['binary_sha256'] != 'e25cadc80ea5163e0f1c6fd703479469d9f23b47ade244fac5f22bcdb01b39bd' or
            target['category'] != 'distribution_installer' or
            target['status'] != 'static_inventory_recounted' or
            target['inventory_profile'] != '7810ea9437da59780820ec6fedb47ce47390841a95d527ddb6c3b3700fe180cb' or
            target['inventory_basis'] != 'in_image_non_thunk_procedures' or
            target['inventory_membership_sha256'] != '27c2767e45bc4a202839bebda1c96224e0e77b36bf640ac8771d3c963d10759b' or
            target['inventory_total'] != 376 or type(target['retained_pseudocode']) is not int or
            not 0 <= target['retained_pseudocode'] <= target['inventory_total'] or
            not isinstance(target['retained_entries'], list) or
            target['retained_pseudocode'] != len(target['retained_entries']) or
            len(set(target['retained_entries'])) != len(target['retained_entries'])):
        raise ValueError('Installer target must retain its separate reviewed 376-procedure static inventory')
    if not all(RVA.fullmatch(entry) for entry in target['retained_entries']):
        raise ValueError('Invalid installer canonical retained entry')
    encoded = json.dumps(snapshot)
    if (any(token in encoded.lower() for token in ['http://', 'https://', 'file://', 'pseudocode_text', 'decompiler_output']) or
            re.search(r'(?:^|[\s"\'=(:])(?:/[a-zA-Z0-9_.~-]|[a-zA-Z]:[\\/])', encoded) or
            re.search(r'(?i)(?:password|api[_-]?key|access[_-]?token|secret)\s*[:=]|\bBearer\s+\S+', encoded)):
        raise ValueError('Private locator or source text in public projection')


def validate(snapshot):
    if snapshot.get('schema_version') == 2:
        return validate_v2(snapshot)
    if snapshot.get('schema_version') == 3:
        return validate_v3(snapshot)
    raise ValueError('Unsupported progress schema')


def ratio(value, total):
    if value is None:
        return 'N/A'
    if total is None:
        return f'{value} / N/A'
    return f'{value} / {total:,} ({100 * value / total:.2f}%)'


def render(snapshot, include_history=False):
    if snapshot['schema_version'] == 3:
        return render_v3(snapshot, include_history)
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


def render_v3(snapshot, include_history=False):
    """Render schema-3 procedure bases explicitly, without aggregating percentages."""
    lines = [START, f"As of {snapshot['as_of']} · snapshot `{snapshot['snapshot_id']}`.", '']
    if not include_history:
        lines += [README_DASHBOARD, '']
    lines += ['| Original binary | Static retained pseudocode | Denominator basis | Full contracts | Full Rust ports | Differential runtime verified |',
              '| --- | ---: | --- | ---: | ---: | ---: |']
    basis_labels = {'raw_profile_inventory': 'Raw profile inventory', 'raw_profile_union': 'Explicit profile union',
                    'unknown': 'Unknown inventory', 'in_image_non_thunk_procedures': 'In-image non-thunk procedures'}
    for row in snapshot['libraries']:
        lines.append(f"| {row['name']} | {ratio(row['retained_pseudocode'], row['inventory_total'])} | {basis_labels[row['inventory_basis']]} | {ratio(row['semantic_contracts'], None)} | {ratio(row['rust_ported'], None)} | {ratio(row['differential_verified'], None)} |")
    setup_rows = [r for r in snapshot['libraries'] if r['inventory_basis'] == 'in_image_non_thunk_procedures']
    installer = snapshot['other_targets'][0]
    installer_ratio = ratio(installer['retained_pseudocode'], installer['inventory_total'])
    lines += ['', f"Strict full-system gate: **{snapshot['systems']['verified']}/{snapshot['systems']['total']}**. Binary-level static counts use the named denominator basis and do not combine into a game-completion percentage. Unknown denominators remain N/A. {len(setup_rows)} supplied configuration Setup inventory uses 172 in-image non-thunk procedures. Separately, archive installer bootstrap static inventory: **{installer_ratio}**; this distribution target is not combined with engine-library counts. The campaign is not playable."]
    if snapshot['scoped_ports']:
        lines += ['', f"Accepted scoped Rust ports: **{sum(len(p['functions']) for p in snapshot['scoped_ports'])} entries across {len(snapshot['scoped_ports'])} reviewed contracts**. These remain separate from retained pseudocode and whole-function counts."]
        if include_history:
            lines += ['', '| Rust contract | Source | Original entries | Native-derived cases | Runtime use | Verified scope |',
                      '| --- | --- | ---: | ---: | --- | --- |']
            for port in snapshot['scoped_ports']:
                path = '../' + port['source_path']
                runtime_use = 'Static inspection' if port['integration'] == 'static_inspection' else 'No integration recorded'
                lines.append(f"| {port['name']} | [{port['source_path']}]({path}) | {len(port['functions'])} | {port['cases']} | {runtime_use} | {port['scope']} |")
            lines += ['', 'These rows describe only the reviewed bounded domains. Their native-derived cases do not establish whole-function completion.']
    if include_history:
        lines += ['', '| Frozen snapshot | As of | Retained static pseudocode | Scoped Rust entries | Complete systems |', '| --- | --- | --- | ---: | ---: |']
        for row in snapshot['history']:
            counts = '; '.join(f"{lib['name']} {ratio(lib['retained_pseudocode'], lib['inventory_total'])}" for lib in row['libraries'])
            scoped = sum(port['entries'] for port in row.get('scoped_ports', [])) if 'scoped_ports' in row else 'N/A'
            lines.append(f"| `{row['snapshot_id']}` | {row['as_of']} | {counts} | {scoped} | {row['systems']['verified']}/{row['systems']['total']} |")
    lines += ['', 'Regenerate locally with `python3 scripts/progress.py --write`; verify without writes with `python3 scripts/progress.py --check`. Versioned snapshots are retained in `docs/progress.json`.']
    if include_history:
        lines.append('See the [README overview](../README.md) for the compact dashboard.')
    else:
        lines.append('Milestone history and scoped implementation evidence are in [docs/PROGRESS.md](docs/PROGRESS.md).')
    lines.append(END)
    return '\n'.join(lines)


def render_svg(snapshot):
    """Render an accessible, self-contained dashboard from the validated snapshot."""
    if snapshot['schema_version'] == 3:
        return render_svg_v3(snapshot)
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


def render_svg_v3(snapshot):
    """Compact 2x3 dashboard for engine libraries and the separate installer target."""
    palette = {'ink': '#142b3b', 'muted': '#506577', 'paper': '#f3f7f8', 'card': '#ffffff',
               'line': '#d4e0e5', 'blue': '#176b87', 'green': '#33765a', 'amber': '#9a5a12',
               'track': '#e2ebee', 'softblue': '#e6f2f6', 'softgreen': '#e9f3ed', 'softamber': '#f8efe2'}
    by_name = {row['name']: row for row in snapshot['libraries']}
    game, ls3df = by_name.get('Game.exe'), by_name.get('LS3DF.dll')
    setup = next((r for r in snapshot['libraries'] if r['inventory_basis'] == 'in_image_non_thunk_procedures'), None)
    ports = snapshot['scoped_ports']; systems = snapshot['systems']
    entry_count = sum(len(p['functions']) for p in ports)
    static_count = sum(p['integration'] == 'static_inspection' for p in ports)
    unrecorded = len(ports) - static_count

    def pct(row):
        return None if row is None or row['inventory_total'] is None else 100 * row['retained_pseudocode'] / row['inventory_total']

    def count(row):
        if row is None: return 'Unknown'
        total = row['inventory_total']
        return f"{row['retained_pseudocode']:,} / {total:,} · {pct(row):.2f}%" if total is not None else f"{row['retained_pseudocode']:,} / unknown"

    def card(x, y, label, value, detail, accent, percent=None):
        bar = ''
        if percent is not None:
            bar = (f'<rect class="bar-track" x="{x+24}" y="{y+123}" width="320" height="10" rx="5" fill="{palette["track"]}"/>'
                   + (f'<rect x="{x+24}" y="{y+123}" width="{320*percent/100:.2f}" height="10" rx="5" fill="{accent}"/>' if percent > 0 else ''))
        return (f'<g><rect class="card-bg" x="{x}" y="{y}" width="372" height="148" rx="16" fill="{palette["card"]}" stroke="{palette["line"]}"/>'
                f'<rect x="{x}" y="{y}" width="7" height="148" rx="3" fill="{accent}"/>'
                f'<text x="{x+24}" y="{y+39}" class="label">{escape(label)}</text>'
                f'<text x="{x+24}" y="{y+82}" class="value">{escape(value)}</text>'
                f'<text x="{x+24}" y="{y+105}" class="detail">{escape(detail)}</text>{bar}</g>')

    installer = snapshot['other_targets'][0]
    cards = ''.join([
        card(36, 112, 'GAME.EXE · STATIC RETAINED PSEUDOCODE', count(game), 'Raw profile inventory · includes external/thunk rows', palette['blue'], pct(game)),
        card(432, 112, 'LS3DF.DLL · STATIC RETAINED PSEUDOCODE', count(ls3df), 'Explicit profile union · not semantic coverage', palette['green'], pct(ls3df)),
        card(36, 278, 'REVIEWED SCOPED RUST PORTS', f'{entry_count} entries · {len(ports)} contracts', f'{static_count} used in static inspection · {unrecorded} unrecorded', palette['amber']),
        card(432, 278, 'STRICT FULL-SYSTEM VERIFICATION', f"{systems['verified']} / {systems['total']}", 'Verified complete systems · strict evidence gate', palette['muted']),
        card(36, 444, 'SETUP.EXE · STATIC PSEUDOCODE', count(setup), '172 in-image non-thunk procedures', palette['green'], pct(setup)),
        card(432, 444, 'INSTALLER · STATIC PSEUDOCODE',
             f"{installer['retained_pseudocode']} / {installer['inventory_total']} · {100*installer['retained_pseudocode']/installer['inventory_total']:.2f}%",
             'In-image non-thunk procedures · distribution target', palette['muted'],
             100*installer['retained_pseudocode']/installer['inventory_total'])])

    history = []
    previous = None
    for item in snapshot['history']:
        libs = {r['name']: r for r in item['libraries']}
        g, l = libs.get('Game.exe'), libs.get('LS3DF.dll')
        def identity(old, name):
            if old is None:
                return None
            if 'inventory_basis' in old:
                return {k: old.get(k) for k in ['binary_sha256', 'inventory_total', 'inventory_basis',
                                                 'inventory_profiles', 'inventory_membership_sha256']}
            return schema2_history_bridge(item, name)
        def matching_inventory(old, current, name):
            return current is not None and old is not None and identity(old, name) == {
                k: current[k] for k in ['binary_sha256', 'inventory_total', 'inventory_basis',
                                         'inventory_profiles', 'inventory_membership_sha256']}
        if matching_inventory(g, game, 'Game.exe') and matching_inventory(l, ls3df, 'LS3DF.dll'):
            values = (g['retained_pseudocode'], g['inventory_total'], l['retained_pseudocode'], l['inventory_total'])
            if values != previous:
                history.append(f"{item['as_of']}: Game {g['retained_pseudocode']:,}/{g['inventory_total']:,}; LS3DF {l['retained_pseudocode']:,}/{l['inventory_total']:,}")
                previous = values
    history_y = 638
    history_svg = ''.join(f'<text x="36" y="{history_y+i*22}" class="history">{escape(line)}</text>' for i,line in enumerate(history))
    if not history_svg:
        history_svg = f'<text x="36" y="{history_y}" class="history">No frozen snapshots match current Game and LS3DF denominators.</text>'
    rows = max(1, len(history)); footer1 = history_y + rows*22 + 8; footer2 = footer1 + 18; height = footer2 + 14
    date_label = escape(snapshot['as_of'])
    installer = snapshot['other_targets'][0]
    installer_count = f"{installer['retained_pseudocode']:,} / {installer['inventory_total']:,} · {100*installer['retained_pseudocode']/installer['inventory_total']:.2f}%"
    desc = escape(f"As of {date_label}. Static retained pseudocode for Game.exe {count(game)}, LS3DF.dll {count(ls3df)}, supplied Setup.exe {count(setup)}, and separate archive installer bootstrap {installer_count}. {entry_count} reviewed scoped entries across {len(ports)} contracts. Strict full-system verification: {systems['verified']} of {systems['total']}; not a game-completion percentage.")
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 840 {height}" role="img" aria-labelledby="dashboard-title dashboard-desc">
<title id="dashboard-title">Mafia Rust reconstruction progress evidence</title><desc id="dashboard-desc">{desc}</desc>
<style>text{{font-family:Inter,ui-sans-serif,system-ui,-apple-system,"Segoe UI",sans-serif;fill:{palette['ink']}}}.kicker{{font-size:14px;font-weight:700;letter-spacing:1.1px;fill:{palette['muted']}}}.title{{font-size:28px;font-weight:750;letter-spacing:-.35px}}.label{{font-size:13px;font-weight:750;letter-spacing:.5px;fill:{palette['muted']}}}.value{{font-size:25px;font-weight:750;letter-spacing:-.4px}}.detail,.history{{font-size:13px;fill:{palette['muted']}}}.dark-bg{{display:none}}@media(prefers-color-scheme:dark){{.light-bg{{display:none}}.dark-bg{{display:inline}}text{{fill:#edf4f6}}.kicker,.label,.detail,.history{{fill:#b3c5ce}}.card-bg{{fill:#172b39;stroke:#3b5361}}.bar-track{{fill:#314957}}}}</style>
<rect class="light-bg" width="840" height="{height}" rx="24" fill="{palette['paper']}"/><rect class="dark-bg" width="840" height="{height}" rx="24" fill="#0d1b25"/>
<text x="36" y="38" class="kicker">LOST HEAVEN REBORN · EVIDENCE SNAPSHOT · {date_label}</text><text x="36" y="75" class="title">Progress measures, separate denominators</text>{cards}
<text x="36" y="611" class="kicker">FROZEN STATIC HISTORY · MATCHING GAME AND LS3DF DENOMINATORS</text>{history_svg}
<text x="36" y="{footer1}" class="detail">Eligible scoped-function denominator: unknown · scoped ports are not whole-function credits.</text><text x="36" y="{footer2}" class="detail">Static analysis is not implementation coverage; full-system verification is not completion.</text></svg>'''


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
