"""Offline progress validation and non-mutating check behavior."""
import copy
import importlib.util
import json
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
        self.snapshot = json.loads((ROOT / 'docs/progress.json').read_text())['function_progress']

    def test_reviewed_baseline_is_not_game_completion(self):
        progress.validate(self.snapshot)
        text = progress.render(self.snapshot)
        self.assertIn('55 / 4,914 (1.12%)', text)
        self.assertIn('48 / 1,683 (2.85%)', text)
        self.assertIn('6 / N/A', text)
        self.assertIn('**0/12**', text)
        self.assertIn('not game-completion percentages', text)

    def test_svg_reports_distinct_metrics_and_unknown_scoped_denominator(self):
        svg = progress.render_svg(self.snapshot)
        self.assertIn('55 / 4,914 · 1.12%', svg)
        self.assertIn('48 / 1,683 · 2.85%', svg)
        self.assertIn('6 entries · 5 contracts', svg)
        self.assertIn('eligible denominator unknown', svg)
        self.assertIn('0 / 12', svg)
        self.assertIn('not a game-completion percentage', svg)
        self.assertIn('aria-labelledby="dashboard-title dashboard-desc"', svg)

    def test_svg_is_deterministic_and_history_uses_frozen_inventory_counts(self):
        svg = progress.render_svg(self.snapshot)
        self.assertEqual(svg, progress.render_svg(copy.deepcopy(self.snapshot)))
        self.assertIn('Game 9/4,914; LS3DF 45/1,683', svg)
        self.assertIn('Game 55/4,914; LS3DF 48/1,683', svg)
        self.assertIn('viewBox="0 0 840 580"', svg)

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
        self.assertIn('y="588" class="detail"', svg)
        self.assertIn('viewBox="0 0 840 602"', svg)
        self.assertGreater(svg.index('y="588" class="detail"'), svg.index('y="540" class="history"'))

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
        self.assertIn('Gameplay integration is pending', text)
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
        row = self.snapshot['libraries'][-1]
        row['accepted_functions'] = [{
            'entry_rva': '0x720f0', 'inventory_evidence': ['synthetic-inventory-proof'],
            'contract_evidence': ['synthetic-whole-contract-proof'],
            'rust_commit': 'a' * 40, 'test_evidence': ['synthetic-clock-tests'],
            'review_ids': ['independent-review-1', 'independent-review-2'],
            'runtime_evidence': []}]
        row['semantic_contracts'], row['rust_ported'] = 1, 1
        self.snapshot['history'][-1]['libraries'][-1]['rust_ported'] = 1
        progress.validate(self.snapshot)
        rendered = progress.render(self.snapshot)
        self.assertIn('| LS3DF.dll | 48 / 1,683 (2.85%) | 1 / N/A | 1 / N/A | N/A |', rendered)
        self.assertNotIn('1 / 1,683', rendered)
        self.assertIn('until eligible original functions are audited', rendered)
        self.assertNotIn('Full semantic-contract counts are unmeasured', rendered)
        self.assertIn('Verified code-byte coverage is unmeasured', rendered)
        row['accepted_functions'][0]['review_ids'] = ['same-review', 'same-review']
        with self.assertRaisesRegex(ValueError, 'two reviews'):
            progress.validate(self.snapshot)


if __name__ == '__main__':
    unittest.main()
