"""Offline progress validation and non-mutating check behavior."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('progress', ROOT / 'scripts/progress.py')
progress = importlib.util.module_from_spec(spec)
spec.loader.exec_module(progress)


class ProgressTests(unittest.TestCase):
    def setUp(self):
        snapshot_path = Path(os.environ.get('LHR_PROGRESS_TEST_SNAPSHOT', ROOT / 'tests/fixtures/progress-v2-legacy.json'))
        document = json.loads(snapshot_path.read_text())
        self.snapshot = document.get('function_progress', document)

    def test_current_public_snapshot_validates(self):
        public_path = ROOT / 'docs/progress.json'
        document = json.loads(public_path.read_text())
        snapshot = document.get('function_progress', document)
        progress.validate(snapshot)

    def schema3_snapshot(self):
        """Small valid migration fixture; historical schema-2 rows stay untouched."""
        snapshot = copy.deepcopy(self.snapshot)
        snapshot['schema_version'] = 3
        snapshot['snapshot_id'] = 'SYNTHETIC-SETUP-INVENTORY-03'
        snapshot['recount_sha256'] = 'c' * 64
        for row in snapshot['libraries']:
            row['inventory_basis'] = {'Game.exe': 'raw_profile_inventory',
                                      'LS3DF.dll': 'raw_profile_union',
                                      'rw_data.dll': 'unknown'}[row['name']]
            row['inventory_profiles'] = {
                'Game.exe': ['7810ea9437da59780820ec6fedb47ce47390841a95d527ddb6c3b3700fe180cb'],
                'LS3DF.dll': ['0c1148c7491bbcdf5b3a0f9ea8ba9aad2523b95135b48b9e8385e31b815e1c1a',
                              '20f79d3fb10ba69c5463530c8267a6e2a8f5d0d7232ea59fe159849342ec5d7e',
                              '46301268618f6f9f27a2856e15858155a42599f674def681aa6a56df9fe13c64',
                              '5fb8141f282b82b7a78df6b2d631f00eb93729605b53b333297ebfb4ff03fe9f'],
                'rw_data.dll': []}[row['name']]
        setup = copy.deepcopy(snapshot['libraries'][0])
        setup.update({'name': 'Setup.exe (supplied configuration)',
                      'binary_sha256': 'c6abff923ecedafc26be3f1abdb8fe286f3a0be1e787ea7b659b6c01a077d25d',
                      'inventory_version': 'REA profile inventory',
                      'inventory_membership_sha256': 'e0ccf8c5121747ba0608317068c72542b155fd2c758c784b518fd30f983e23df',
                      'inventory_total': 172,
                      'retained_entries': ['0x1410', '0x10a0', '0x14f0', '0x1210', '0x2c70',
                                           '0x2cf0', '0x2d00', '0x2e00', '0x2ea0', '0x35f0'],
                      'retained_pseudocode': 10,
                      'semantic_contracts': None, 'rust_ported': None, 'differential_verified': None,
                      'verified_code_bytes': None, 'scope': 'Static retained pseudocode only.',
                      'accepted_functions': [], 'inventory_basis': 'in_image_non_thunk_procedures',
                      'inventory_profiles': ['7810ea9437da59780820ec6fedb47ce47390841a95d527ddb6c3b3700fe180cb']})
        snapshot['libraries'].append(setup)
        snapshot['other_targets'] = [{'name': 'Setup.exe (archive installer bootstrap)',
                                      'binary_sha256': 'e25cadc80ea5163e0f1c6fd703479469d9f23b47ade244fac5f22bcdb01b39bd',
                                      'category': 'distribution_installer',
                                      'status': 'static_inventory_recounted',
                                      'inventory_profile': '7810ea9437da59780820ec6fedb47ce47390841a95d527ddb6c3b3700fe180cb',
                                      'inventory_basis': 'in_image_non_thunk_procedures',
                                      'inventory_membership_sha256': '27c2767e45bc4a202839bebda1c96224e0e77b36bf640ac8771d3c963d10759b',
                                      'inventory_total': 376, 'retained_pseudocode': 1,
                                      'retained_entries': ['0xb1cc']}]
        latest = copy.deepcopy(snapshot['history'][-1])
        latest['snapshot_id'] = snapshot['snapshot_id']
        latest['as_of'] = snapshot['as_of']
        latest['libraries'] = [{k: row[k] for k in ['name', 'binary_sha256', 'inventory_total', 'retained_pseudocode',
                                                     'rust_ported', 'differential_verified', 'inventory_basis',
                                                     'inventory_profiles', 'inventory_membership_sha256']}
                               for row in snapshot['libraries']]
        latest['history_prefix_sha256'] = progress.hash_history_prefix(snapshot['history'])
        snapshot['history'].append(latest)
        return snapshot

    def test_reviewed_baseline_is_not_game_completion(self):
        progress.validate(self.snapshot)
        text = progress.render(self.snapshot)
        for library in self.snapshot['libraries']:
            expected = progress.ratio(library['retained_pseudocode'], library['inventory_total'])
            self.assertIn(f"| {library['name']} | {expected} |", text)
        self.assertIn('**0/12**', text)
        self.assertIn('not game-completion percentages', text)

    def test_svg_reports_distinct_metrics_without_inventing_runtime_state(self):
        svg = progress.render_svg(self.snapshot)
        for library in self.snapshot['libraries']:
            if library['name'] not in {'Game.exe', 'LS3DF.dll'}:
                continue
            if library['inventory_total'] is None:
                label = f"{library['retained_pseudocode']} / unknown"
            else:
                label = f"{library['retained_pseudocode']:,} / {library['inventory_total']:,} · {100 * library['retained_pseudocode'] / library['inventory_total']:.2f}%"
            self.assertIn(label, svg)
        self.assertIn('Eligible scoped-function denominator: unknown', svg)
        self.assertIn('0 / 12', svg)
        self.assertIn('not a game-completion percentage', svg)
        self.assertIn('aria-labelledby="dashboard-title dashboard-desc"', svg)

    def test_svg_is_deterministic_and_history_uses_frozen_inventory_counts(self):
        svg = progress.render_svg(self.snapshot)
        self.assertEqual(svg, progress.render_svg(copy.deepcopy(self.snapshot)))
        current_totals = {row['name']: row['inventory_total'] for row in self.snapshot['libraries']}
        comparable = [row for row in self.snapshot['history']
                      if {lib['name']: lib['inventory_total'] for lib in row['libraries']}.get('Game.exe') == current_totals.get('Game.exe')
                      and {lib['name']: lib['inventory_total'] for lib in row['libraries']}.get('LS3DF.dll') == current_totals.get('LS3DF.dll')]
        if comparable:
            latest = comparable[-1]
            counts = {lib['name']: lib['retained_pseudocode'] for lib in latest['libraries']}
            self.assertIn(f"Game {counts['Game.exe']:,}/{current_totals['Game.exe']:,}; LS3DF {counts['LS3DF.dll']:,}/{current_totals['LS3DF.dll']:,}", svg)
        self.assertRegex(svg, r'viewBox="0 0 840 \d+"')

    def test_svg_omits_history_with_different_inventory_denominators(self):
        snapshot = copy.deepcopy(self.snapshot)
        snapshot['history'][0]['libraries'][1]['inventory_total'] = 1700
        svg = progress.render_svg(snapshot)
        self.assertNotIn('Game 9/4,914; LS3DF 45/1,683', svg)
        self.assertIn('Game 55/4,914; LS3DF 48/1,683', svg)

    def test_svg_canvas_grows_for_additional_comparable_history_rows(self):
        snapshot = copy.deepcopy(self.snapshot)
        third = copy.deepcopy(snapshot['history'][-1])
        third['snapshot_id'] = 'synthetic-comparable-history'
        third['libraries'][0]['retained_pseudocode'] = 56
        third['libraries'][1]['retained_pseudocode'] = 49
        snapshot['history'].append(third)
        svg = progress.render_svg(snapshot)
        self.assertIn('y="540" class="history"', svg)
        history_rows = svg.count('class="history"')
        footer_y = 496 + max(1, history_rows) * 22 + 8
        self.assertIn(f'y="{footer_y+18}" class="detail"', svg)
        self.assertRegex(svg, r'viewBox="0 0 840 \d+"')
        self.assertGreater(svg.index(f'y="{footer_y+18}" class="detail"'), svg.index('class="history"'))

    def test_svg_fallback_history_row_does_not_overlap_footer(self):
        snapshot = copy.deepcopy(self.snapshot)
        for row in snapshot['history']:
            for library in row['libraries']:
                library['inventory_total'] = None
        svg = progress.render_svg(snapshot)
        self.assertIn('No frozen snapshots match', svg)
        self.assertIn('y="496" class="history"', svg)
        self.assertIn('y="526" class="detail"', svg)

    def test_svg_zero_static_percentage_has_no_nonzero_fill(self):
        snapshot = copy.deepcopy(self.snapshot)
        snapshot['libraries'][0]['retained_pseudocode'] = 0
        snapshot['libraries'][0]['retained_entries'] = []
        svg = progress.render_svg(snapshot)
        self.assertIn('0 / 4,914 · 0.00%', svg)
        self.assertIn('x="60" y="235" width="320" height="10"', svg)
        self.assertNotIn('x="60" y="235" width="2.00"', svg)

    def test_duplicate_canonical_entry_is_rejected(self):
        row = self.snapshot['libraries'][0]
        row['retained_entries'].append(row['retained_entries'][0])
        row['retained_pseudocode'] += 1
        with self.assertRaisesRegex(ValueError, 'duplicate canonical'):
            progress.validate(self.snapshot)

    def test_count_cannot_exceed_denominator(self):
        self.snapshot['libraries'][0]['inventory_total'] = 1
        with self.assertRaisesRegex(ValueError, 'denominator'):
            progress.validate(self.snapshot)

    def test_private_locator_is_rejected(self):
        self.snapshot['libraries'][0]['scope'] = '/workspace/private/evidence.json'
        with self.assertRaisesRegex(ValueError, 'Private locator'):
            progress.validate(self.snapshot)

    def test_unsubstantiated_port_is_rejected(self):
        self.snapshot['libraries'][0]['rust_ported'] = 1
        with self.assertRaisesRegex(ValueError, 'accepted ledger'):
            progress.validate(self.snapshot)

    def test_runtime_requires_accepted_port(self):
        self.snapshot['libraries'][0]['differential_verified'] = 1
        with self.assertRaises(ValueError):
            progress.validate(self.snapshot)

    def test_history_disagreement_is_rejected(self):
        self.snapshot['history'][-1]['libraries'][0]['retained_pseudocode'] = 54
        with self.assertRaisesRegex(ValueError, 'history disagree'):
            progress.validate(self.snapshot)

    def test_missing_or_duplicate_markers_are_rejected(self):
        for text in ['', progress.START + progress.START + progress.END]:
            with self.assertRaises(ValueError):
                progress.replace_fragment(text, 'new', 'example.md')

    def test_check_is_read_only_even_when_stale(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'docs').mkdir()
            (root / 'docs/progress.json').write_text(json.dumps({'function_progress': self.snapshot}))
            for name in ['README.md', 'docs/PROGRESS.md']:
                (root / name).write_text('prefix\n' + progress.START + '\nstale\n' + progress.END + '\nsuffix')
            before = {str(p): (p.read_bytes(), p.stat().st_mtime_ns) for p in root.rglob('*') if p.is_file()}
            with patch.object(progress, 'ROOT', root):
                self.assertEqual(progress.main(['--check']), 1)
            after = {str(p): (p.read_bytes(), p.stat().st_mtime_ns) for p in root.rglob('*') if p.is_file()}
            self.assertEqual(before, after)
            with patch.object(progress, 'ROOT', root):
                self.assertEqual(progress.main(['--write']), 0)
                self.assertEqual(progress.main(['--check']), 0)
                expected_svg = progress.render_svg(self.snapshot)
                (root / 'docs/progress.svg').write_text('stale svg')
                self.assertEqual(progress.main(['--check']), 1)
                self.assertEqual((root / 'docs/progress.svg').read_text(), 'stale svg')
                self.assertEqual(progress.main(['--write']), 0)
                self.assertEqual((root / 'docs/progress.svg').read_text(), expected_svg)
                self.assertEqual(progress.main(['--check']), 0)
            self.assertTrue((root / 'README.md').read_text().endswith('\nsuffix'))

    def test_render_is_deterministic(self):
        self.assertEqual(progress.render(self.snapshot), progress.render(copy.deepcopy(self.snapshot)))

    def test_scoped_ports_remain_separate_from_whole_function_credit(self):
        snapshot = copy.deepcopy(self.snapshot)
        snapshot['scoped_ports'] = snapshot['scoped_ports'][:2]
        text = progress.render(snapshot)
        self.assertIn('3 original entries across 2 reviewed contracts', text)
        self.assertIn('20 cases', text)
        self.assertIn('13 cases', text)
        if any('integration_receipt' in port for port in snapshot['scoped_ports']):
            static_count = sum(port['integration'] == 'static_inspection' for port in snapshot['scoped_ports'])
            self.assertIn(f'Recorded runtime use: {static_count} contracts used in static inspection;', text)
            self.assertIn('Runtime use', text)
        else:
            self.assertIn('Runtime integration status was not recorded in this snapshot', text)
            self.assertNotIn('Runtime use', text)
        for row in snapshot['libraries']:
            self.assertIsNone(row['rust_ported'])
            self.assertEqual(row['accepted_functions'], [])

    def test_scoped_evidence_does_not_require_retained_pseudocode(self):
        # NullTimer is an exact inventory member with instruction/oracle evidence;
        # the frozen retained-pseudocode subset does not include it.
        clock = self.snapshot['scoped_ports'][0]
        self.assertEqual(clock['functions'][0]['entry_rva'], '0x720f0')
        progress.validate(self.snapshot)
        clock['functions'][0]['inventory_membership_sha256'] = '0' * 64
        with self.assertRaisesRegex(ValueError, 'unproven scoped'):
            progress.validate(self.snapshot)

    def test_scoped_duplicate_entry_normalizes_rva_spelling(self):
        entries = self.snapshot['scoped_ports'][0]['functions']
        duplicate = copy.deepcopy(entries[0])
        duplicate['entry_rva'] = '0x0720f0'
        entries.append(duplicate)
        with self.assertRaisesRegex(ValueError, 'Duplicate'):
            progress.validate(self.snapshot)

    def test_scoped_acceptance_requires_two_distinct_review_artifacts(self):
        reviews = self.snapshot['scoped_ports'][0]['reviews']
        reviews[1]['sha256'] = reviews[0]['sha256']
        with self.assertRaisesRegex(ValueError, 'two distinct reviews'):
            progress.validate(self.snapshot)

    def test_scoped_history_must_match_current_acceptance(self):
        self.snapshot['history'][-1]['scoped_ports'][0]['cases'] += 1
        with self.assertRaisesRegex(ValueError, 'Scoped-port snapshot and history'):
            progress.validate(self.snapshot)

    def test_scoped_scope_and_comparison_are_required(self):
        for field, value in [('scope', ''), ('cases', 0), ('integration', 'complete')]:
            snapshot = copy.deepcopy(self.snapshot)
            snapshot['scoped_ports'][0][field] = value
            with self.assertRaises(ValueError):
                progress.validate(snapshot)
        self.snapshot['scoped_ports'][0]['comparison']['sha256'] = 'missing'
        with self.assertRaisesRegex(ValueError, 'evidence identity'):
            progress.validate(self.snapshot)

    def test_static_inspection_requires_hashed_integration_receipt(self):
        snapshot = copy.deepcopy(self.snapshot)
        port = snapshot['scoped_ports'][0]
        port['integration'] = 'static_inspection'
        port.pop('integration_receipt', None)
        with self.assertRaisesRegex(ValueError, 'requires a hashed integration receipt'):
            progress.validate(snapshot)

        port['integration_receipt'] = {'id': 'RUST-STATIC-REVIEW', 'sha256': 'not-a-hash'}
        with self.assertRaisesRegex(ValueError, 'Invalid static integration receipt'):
            progress.validate(snapshot)

        port['integration_receipt'] = {'id': 'RUST-STATIC-REVIEW', 'sha256': 'a' * 64}
        if len(snapshot['scoped_ports']) > 1:
            snapshot['scoped_ports'][1]['integration'] = 'not_integrated'
            snapshot['scoped_ports'][1]['integration_receipt'] = None
        snapshot['history'][-1]['scoped_ports'] = progress.scoped_summary(snapshot['scoped_ports'])
        progress.validate(snapshot)
        rendered = progress.render(snapshot)
        self.assertIn('| Static inspection |', rendered)
        static_count = sum(p['integration'] == 'static_inspection' for p in snapshot['scoped_ports'])
        unrecorded_count = len(snapshot['scoped_ports']) - static_count
        self.assertIn(f'Recorded runtime use: {static_count} contracts used in static inspection; {unrecorded_count} contracts with no integration recorded', rendered)
        self.assertNotIn('active IPol', rendered)

    def test_static_integration_history_retains_receipt_hash(self):
        port = copy.deepcopy(self.snapshot['scoped_ports'][0])
        port['integration'] = 'static_inspection'
        port['integration_receipt'] = {'id': 'RUST-STATIC-REVIEW', 'sha256': 'b' * 64}
        history = progress.scoped_summary([port])
        self.assertEqual(history[0]['integration'], 'static_inspection')
        self.assertEqual(history[0]['integration_receipt_sha256'], 'b' * 64)

    def test_source_links_are_relative_to_each_generated_document(self):
        self.assertIn('](crates/lhr-engine/src/clock.rs)', progress.render(self.snapshot))
        self.assertIn('](../crates/lhr-engine/src/clock.rs)', progress.render(self.snapshot, include_history=True))

    def test_previous_history_is_validated(self):
        self.snapshot['history'][0]['libraries'][0]['retained_pseudocode'] = 10000
        with self.assertRaisesRegex(ValueError, 'historical static counts'):
            progress.validate(self.snapshot)

    def test_unexpected_public_payload_is_rejected(self):
        self.snapshot['libraries'][0]['raw_code'] = 'unexpected payload'
        with self.assertRaisesRegex(ValueError, 'Unexpected public library'):
            progress.validate(self.snapshot)

    def test_code_byte_credits_require_a_byte_ledger(self):
        for value in [-500, 0, 100]:
            self.snapshot['libraries'][0]['verified_code_bytes'] = value
            with self.assertRaisesRegex(ValueError, 'byte ledger'):
                progress.validate(self.snapshot)

    def test_generic_private_paths_and_credentials_are_rejected(self):
        for payload in ['/home/alice/private/secret.c', 'file:///private/secret.c',
                        r'C:\Users\alice\secret.c', 'password=example', 'Bearer example-token']:
            self.snapshot['libraries'][0]['scope'] = payload
            with self.assertRaises(ValueError, msg=payload):
                progress.validate(self.snapshot)

    def test_accepted_contract_cannot_hide_behind_unknown_count(self):
        self.snapshot['libraries'][0]['accepted_functions'] = [{
            'entry_rva': '0x720f0', 'inventory_evidence': ['synthetic'],
            'contract_evidence': ['synthetic'], 'rust_commit': None,
            'test_evidence': [], 'review_ids': [], 'runtime_evidence': []}]
        with self.assertRaisesRegex(ValueError, 'hidden'):
            progress.validate(self.snapshot)

    def test_future_clock_acceptance_requires_whole_function_evidence(self):
        row = next(library for library in self.snapshot['libraries'] if library['name'] == 'LS3DF.dll')
        row['accepted_functions'] = [{
            'entry_rva': '0x720f0', 'inventory_evidence': ['synthetic-inventory-proof'],
            'contract_evidence': ['synthetic-whole-contract-proof'],
            'rust_commit': 'a' * 40, 'test_evidence': ['synthetic-clock-tests'],
            'review_ids': ['independent-review-1', 'independent-review-2'],
            'runtime_evidence': []}]
        row['semantic_contracts'], row['rust_ported'] = 1, 1
        history_library = next(library for library in self.snapshot['history'][-1]['libraries'] if library['name'] == 'LS3DF.dll')
        history_library['rust_ported'] = 1
        progress.validate(self.snapshot)
        rendered = progress.render(self.snapshot)
        expected_ratio = progress.ratio(row['retained_pseudocode'], row['inventory_total'])
        self.assertIn(f'| LS3DF.dll | {expected_ratio} | 1 / N/A | 1 / N/A | N/A |', rendered)
        self.assertNotIn('1 / 1,683', rendered)
        self.assertIn('until eligible original functions are audited', rendered)
        self.assertNotIn('Full semantic-contract counts are unmeasured', rendered)
        self.assertIn('Verified code-byte coverage is unmeasured', rendered)
        row['accepted_functions'][0]['review_ids'] = ['same-review', 'same-review']
        with self.assertRaisesRegex(ValueError, 'two reviews'):
            progress.validate(self.snapshot)

    def test_schema3_setup_basis_and_separate_installer_render_without_global_percent(self):
        snapshot = self.schema3_snapshot()
        progress.validate(snapshot)
        markdown = progress.render(snapshot)
        svg = progress.render_svg(snapshot)
        self.assertIn('10 / 172 (5.81%)', markdown)
        self.assertIn('In-image non-thunk procedures', markdown)
        self.assertIn('archive installer bootstrap static inventory: **1 / 376 (0.27%)**', markdown)
        self.assertIn('10 / 172 · 5.81%', svg)
        self.assertIn('1 / 376 · 0.27%', svg)
        self.assertIn('viewBox="0 0 840', svg)
        self.assertNotIn('combined game percentage', markdown.lower())

    def test_schema3_rejects_mixed_setup_denominator_or_false_unknown_percent(self):
        snapshot = self.schema3_snapshot()
        setup = snapshot['libraries'][-1]
        setup['inventory_total'] = 236
        snapshot['history'][-1]['libraries'][-1]['inventory_total'] = 236
        with self.assertRaisesRegex(ValueError, '172-entry basis'):
            progress.validate(snapshot)

        snapshot = self.schema3_snapshot()
        unknown = next(row for row in snapshot['libraries'] if row['name'] == 'rw_data.dll')
        unknown['inventory_total'] = 12
        snapshot['history'][-1]['libraries'][2]['inventory_total'] = 12
        with self.assertRaisesRegex(ValueError, 'Unknown denominator'):
            progress.validate(snapshot)

    def test_schema3_installer_cannot_be_promoted_to_function_denominator(self):
        snapshot = self.schema3_snapshot()
        snapshot['other_targets'][0]['inventory_total'] = 1
        with self.assertRaisesRegex(ValueError, '376-procedure'):
            progress.validate(snapshot)

    def test_schema3_latest_history_tracks_basis_but_old_history_shape_is_preserved(self):
        snapshot = self.schema3_snapshot()
        before = copy.deepcopy(snapshot['history'][:-1])
        progress.validate(snapshot)
        self.assertEqual(snapshot['history'][:-1], before)
        snapshot['history'][0]['libraries'][0]['retained_pseudocode'] += 1
        with self.assertRaisesRegex(ValueError, 'history is frozen'):
            progress.validate(snapshot)

        snapshot = self.schema3_snapshot()
        snapshot['history'][-1]['libraries'][-1]['inventory_profiles'] = ['e' * 64]
        with self.assertRaisesRegex(ValueError, 'history and current inventory bases disagree'):
            progress.validate(snapshot)

    def test_schema3_supports_chained_second_snapshot_and_rejects_prior_v3_tampering(self):
        snapshot = self.schema3_snapshot()
        second = copy.deepcopy(snapshot['history'][-1])
        second['snapshot_id'] = 'SYNTHETIC-SETUP-INVENTORY-04'
        second['as_of'] = '2026-10-22'
        second['history_prefix_sha256'] = progress.hash_history_prefix(snapshot['history'])
        snapshot['snapshot_id'] = second['snapshot_id']
        snapshot['as_of'] = second['as_of']
        snapshot['history'].append(second)
        progress.validate(snapshot)
        snapshot['history'][-2]['as_of'] = '2026-10-20'
        with self.assertRaisesRegex(ValueError, 'append chain was modified'):
            progress.validate(snapshot)

    def test_schema3_second_append_revalidates_prior_identity_and_private_payload(self):
        cases = [
            ('inventory_profiles', ['/home/alice/private/profile.json'], 'known schema-3 historical inventory identity'),
            ('binary_sha256', 'Bearer private-token', 'historical binary identity'),
            ('inventory_basis', 'made_up_basis', 'historical denominator basis'),
            ('rust_ported', 1_000_000, 'historical acceptance count'),
            ('differential_verified', 1, 'Historical runtime count'),
        ]
        for field, value, message in cases:
            with self.subTest(field=field):
                snapshot = self.schema3_snapshot()
                first_v3 = copy.deepcopy(snapshot['history'][-1])
                second = copy.deepcopy(first_v3)
                first_v3['snapshot_id'] = 'SYNTHETIC-OLDER-V3'
                second['snapshot_id'] = 'SYNTHETIC-SETUP-INVENTORY-04'
                second['as_of'] = '2026-10-22'
                snapshot['history'][-1] = first_v3
                snapshot['history'].append(second)
                snapshot['snapshot_id'] = second['snapshot_id']
                snapshot['as_of'] = second['as_of']
                snapshot['history'][-2]['libraries'][0][field] = value
                if field == 'differential_verified':
                    snapshot['history'][-2]['libraries'][0]['rust_ported'] = None
                for index in range(6, len(snapshot['history'])):
                    snapshot['history'][index]['history_prefix_sha256'] = progress.hash_history_prefix(snapshot['history'][:index])
                with self.assertRaisesRegex(ValueError, message):
                    progress.validate(snapshot)

        snapshot = self.schema3_snapshot()
        first_v3 = copy.deepcopy(snapshot['history'][-1])
        second = copy.deepcopy(first_v3)
        first_v3['snapshot_id'] = 'SYNTHETIC-OLDER-V3'
        second['snapshot_id'] = 'SYNTHETIC-SETUP-INVENTORY-04'
        second['as_of'] = '2026-10-22'
        snapshot['history'][-1] = first_v3
        snapshot['history'].append(second)
        snapshot['snapshot_id'] = second['snapshot_id']
        snapshot['as_of'] = second['as_of']
        snapshot['history'][-2]['scoped_ports'][0]['contract_id'] = '/private/secret.c'
        for index in range(6, len(snapshot['history'])):
            snapshot['history'][index]['history_prefix_sha256'] = progress.hash_history_prefix(snapshot['history'][:index])
        with self.assertRaisesRegex(ValueError, 'historical scoped-port identity'):
            progress.validate(snapshot)

    def test_schema3_history_requires_profile_and_membership_identity_match(self):
        snapshot = self.schema3_snapshot()
        previous = copy.deepcopy(snapshot['history'][-1])
        previous['snapshot_id'] = 'SYNTHETIC-OLDER-V3'
        previous['libraries'][0]['inventory_profiles'] = ['e' * 64]
        previous['libraries'][0]['inventory_membership_sha256'] = 'f' * 64
        previous['history_prefix_sha256'] = progress.hash_history_prefix(snapshot['history'][:-1])
        current = copy.deepcopy(snapshot['history'][-1])
        current['history_prefix_sha256'] = progress.hash_history_prefix(snapshot['history'][:-1] + [previous])
        snapshot['history'][-1] = previous
        snapshot['history'].append(current)
        snapshot['history'][-1]['snapshot_id'] = snapshot['snapshot_id']
        progress.validate(snapshot)
        svg = progress.render_svg(snapshot)
        self.assertEqual(svg.count('Game 72/4,914; LS3DF 55/1,683'), 1)

    def test_schema2_history_identity_bridge_is_receipt_and_snapshot_specific(self):
        old = copy.deepcopy(self.snapshot['history'][0])
        self.assertIsNotNone(progress.schema2_history_bridge(old, 'Game.exe'))
        self.assertIsNotNone(progress.schema2_history_bridge(old, 'LS3DF.dll'))
        changed_id = copy.deepcopy(old)
        changed_id['snapshot_id'] = 'unknown-old-snapshot-with-same-denominators'
        self.assertIsNone(progress.schema2_history_bridge(changed_id, 'Game.exe'))
        changed_count = copy.deepcopy(old)
        changed_count['libraries'][0]['retained_pseudocode'] = 10
        self.assertIsNone(progress.schema2_history_bridge(changed_count, 'Game.exe'))

    def test_schema3_2x3_dashboard_and_history_are_deterministic(self):
        snapshot = self.schema3_snapshot()
        first = progress.render_svg(snapshot)
        self.assertEqual(first, progress.render_svg(copy.deepcopy(snapshot)))
        self.assertIn('x="36" y="444"', first)
        self.assertIn('x="432" y="444"', first)
        self.assertIn('FROZEN STATIC HISTORY', first)
        self.assertIn('Eligible scoped-function denominator: unknown', first)

    def test_schema3_docs_history_keeps_regeneration_and_contract_detail(self):
        snapshot = self.schema3_snapshot()
        overview = progress.render(snapshot)
        history = progress.render(snapshot, include_history=True)
        for text in [overview, history]:
            self.assertIn('python3 scripts/progress.py --write', text)
            self.assertIn('python3 scripts/progress.py --check', text)
        self.assertIn('[docs/PROGRESS.md](docs/PROGRESS.md)', overview)
        self.assertIn('| Rust contract | Source | Original entries | Native-derived cases | Runtime use | Verified scope |', history)
        port = snapshot['scoped_ports'][0]
        self.assertIn(port['name'], history)
        self.assertIn(f"[{port['source_path']}](../{port['source_path']})", history)
        self.assertIn(f"| {port['cases']} |", history)
        self.assertIn(port['scope'], history)

class BinaryCatalogTests(unittest.TestCase):
    def setUp(self):
        self.document = json.loads((ROOT / 'docs/progress.json').read_text())
        self.snapshot = self.document['function_progress']
        self.catalog = self.document['binary_catalog']

    def test_every_binary_renders_in_both_artifacts(self):
        progress.validate_catalog(self.catalog, self.snapshot)
        svg = progress.render_svg(self.snapshot, catalog=self.catalog)
        for include_history in [False, True]:
            markdown = progress.render(self.snapshot, include_history, self.catalog)
            for row in self.catalog['binaries']:
                for field in ['name', 'platform', 'variant', 'role']:
                    self.assertIn(row[field], markdown)
                    self.assertIn(row[field], svg)
        entries = sum(len(port['functions']) for port in self.snapshot['scoped_ports'])
        self.assertIn(f"{entries} entries · {len(self.snapshot['scoped_ports'])} contracts", svg)
        systems = self.snapshot['systems']
        self.assertIn(f"{systems['verified']} / {systems['total']}", svg)

    def test_unknown_static_stays_na_and_counts_join_by_hash(self):
        svg = progress.render_svg(self.snapshot, self.catalog)
        self.assertIn('class="catalog-static-fill"', svg)
        zero = copy.deepcopy(self.snapshot)
        for row in zero['libraries'] + zero['other_targets']:
            row['retained_pseudocode'] = 0
        zero_svg = progress.render_svg(zero, self.catalog)
        self.assertNotIn('class="catalog-static-fill"', zero_svg)
        known = sum(row['inventory_total'] is not None for row in zero['libraries'] + zero['other_targets'])
        self.assertEqual(zero_svg.count('class="catalog-static-track bar-track"'), known)
        rows = list(progress.catalog_rows(self.catalog, self.snapshot))
        measured = {r['binary_sha256'] for r in self.snapshot['libraries'] + self.snapshot['other_targets']}
        counts = {}
        for port in self.snapshot['scoped_ports']:
            for function in port['functions']:
                key = function['binary_sha256']
                counts[key] = counts.get(key, 0) + 1
        for binary, static, count in rows:
            self.assertEqual(count, counts.get(binary['binary_sha256'], 0))
            if binary['binary_sha256'] not in measured:
                self.assertEqual(static, 'N/A')
                self.assertEqual(count, 0)
        ps2 = next(row for row in rows if row[0]['platform'] == 'PS2')
        self.assertEqual(ps2[1:], ('N/A', 0))
        setup = [row for row in rows if row[0]['name'].startswith('Setup.exe')]
        self.assertEqual(len(setup), 2)
        self.assertEqual(len({row[0]['variant'] for row in setup}), 2)

    def test_catalog_rejects_missing_each_tracked_identity(self):
        for tracked in self.snapshot['libraries'] + self.snapshot['other_targets']:
            catalog = copy.deepcopy(self.catalog)
            catalog['binaries'] = [r for r in catalog['binaries'] if r['binary_sha256'] != tracked['binary_sha256']]
            with self.assertRaises(ValueError):
                progress.validate_catalog(catalog, self.snapshot)

    def test_catalog_rejects_invalid_public_fields_and_identities(self):
        mutations = [
            lambda c: c.update(schema_version=True),
            lambda c: c.update(schema_version=-1),
            lambda c: c.update(private_path='/tmp/private'),
            lambda c: c['binaries'][0].update(binary_sha256='A' * 64),
            lambda c: c['binaries'][0].update(binary_sha256=12),
            lambda c: c['binaries'][0].update(platform='unknown'),
            lambda c: c['binaries'][0].update(role='/workspace/private'),
            lambda c: c['binaries'][0].update(name='https://private.invalid'),
            lambda c: c['binaries'][0].update(inventory_total=-1),
            lambda c: c['binaries'][0].update(scoped_entries=100),
            lambda c: c['binaries'].append(copy.deepcopy(c['binaries'][0])),
            lambda c: c['binaries'][1].update(name=c['binaries'][0]['name'], variant=c['binaries'][0]['variant']),
        ]
        for mutate in mutations:
            catalog = copy.deepcopy(self.catalog)
            mutate(catalog)
            with self.assertRaises(ValueError):
                progress.validate_catalog(catalog, self.snapshot)

    def test_render_and_validation_preserve_history(self):
        before = copy.deepcopy(self.snapshot)
        progress.validate(self.snapshot)
        progress.validate_catalog(self.catalog, self.snapshot)
        progress.render(self.snapshot, True, self.catalog)
        progress.render_svg(self.snapshot, self.catalog)
        self.assertEqual(self.snapshot, before)
        self.assertEqual(progress.hash_history_prefix(self.snapshot['history'][:6]), progress.SCHEMA2_HISTORY_SHA256)


if __name__ == '__main__':
    unittest.main()
