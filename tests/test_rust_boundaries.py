"""Public synthetic tests; no original assets or Rust toolchain required."""
from contextlib import redirect_stderr, redirect_stdout
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
import rust_boundaries as audit


class LexerTests(unittest.TestCase):
    def kinds(self, source):
        return [(f.kind, f.token) for f in audit.scan('crates/test/src/lib.rs', source)]

    def test_classifies_actual_tokens_and_preserves_line_numbers(self):
        findings = audit.scan('x.rs', '// unsafe\nunsafe fn f(p: *const u8) {\ncore::arch::asm!("nop");\n}')
        self.assertEqual([(f.kind, f.line) for f in findings], [
            ('unsafe-keyword', 2), ('raw-pointer-type', 2), ('assembly-macro', 3),
        ])

    def test_comments_and_nested_comments_are_not_code(self):
        self.assertEqual(self.kinds('// asm! *mut\n/* outer /* unsafe */ transmute */'), [])

    def test_normal_byte_c_and_raw_strings_are_not_code(self):
        source = '''"unsafe \\\" asm!" b"*mut" c"eax"
        r###"unsafe \"## global_asm!"### br#"transmute"# cr"sound_virtual"
        '''
        self.assertEqual(self.kinds(source), [])

    def test_raw_identifiers_are_not_keywords_but_keep_symbol_identity(self):
        self.assertEqual(self.kinds('fn r#unsafe() {}'), [])
        self.assertEqual(self.kinds('core::arch::r#asm!("nop"); r#sound_virtual(1);'), [
            ('assembly-macro', 'asm'),
            ('compatibility-symbol', 'sound_virtual'),
        ])

    def test_characters_and_lifetimes_do_not_swallow_following_code(self):
        source = r'''let a = '\''; let b = '\u{1f43c}'; let c = b'\xff';
        fn x<'a>(x: &'a str) { unsafe {} }'''
        self.assertEqual(self.kinds(source), [('unsafe-keyword', 'unsafe')])

    def test_safe_references_ownership_flags_and_rvas_are_not_debt(self):
        source = 'fn f(x: &mut Arc<Vec<u8>>, y: Box<u32>) { let mask = 0x624df0; }'
        self.assertEqual(self.kinds(source), [])

    def test_exact_compatibility_and_memory_symbols(self):
        self.assertEqual(self.kinds('sound_virtual(1); ModelVTableId(0); native_this; esp; std::mem::transmute::<u32, f32>(1);'), [
            ('compatibility-symbol', 'sound_virtual'),
            ('compatibility-symbol', 'ModelVTableId'),
            ('compatibility-symbol', 'native_this'),
            ('cpu-register-name', 'esp'),
            ('raw-memory-symbol', 'transmute'),
        ])
        self.assertEqual(self.kinds('notunsafe; asm_variable; native_this_count;'), [])

    def test_tests_and_inactive_configuration_remain_in_scope(self):
        self.assertEqual(self.kinds('#[cfg(test)] mod tests { unsafe {} }'), [('unsafe-keyword', 'unsafe')])

    def test_unterminated_literals_and_comments_fail_instead_of_hiding_code(self):
        for source in ['/* unsafe', 'r##"unsafe"#', '"unsafe\\']:
            with self.subTest(source=source), self.assertRaises(audit.AuditError):
                audit.scan('x.rs', source)

    def test_lines_can_move_without_new_findings_but_added_occurrences_are_reported(self):
        before = audit.scan('x.rs', 'unsafe {}')
        after = audit.scan('x.rs', '\n\nunsafe {}\nunsafe {}')
        self.assertEqual([f.line for f in audit.additions(after, before)], [4])
        self.assertEqual(audit.additions(before, after), [])

    def test_moving_a_boundary_to_another_file_requires_review(self):
        before = audit.scan('old.rs', 'unsafe {}')
        after = audit.scan('new.rs', 'unsafe {}')
        self.assertEqual(audit.additions(after, before), after)


class RepositoryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.git('init', '-q')
        self.write('crates/demo/src/lib.rs', 'pub fn safe() {}\n')
        self.git('add', '.')
        self.git('-c', 'user.name=Synthetic', '-c', 'user.email=synthetic@example.invalid',
                 'commit', '-qm', 'Public fixture')
        self.base = self.git('rev-parse', 'HEAD').strip()

    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.root), *args], text=True)

    def write(self, path, text):
        file = self.root / path
        file.parent.mkdir(parents=True, exist_ok=True)
        file.write_text(text, encoding='utf-8')
        return file

    def run_audit(self, *args):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            result = audit.main(['--root', str(self.root), *args])
        return result, json.loads(out.getvalue()) if out.getvalue() else None, err.getvalue()

    def test_exact_base_and_new_untracked_sources_are_scanned(self):
        self.write('crates/demo/src/new.rs', 'unsafe fn added() {}\n')
        result, report, error = self.run_audit('--base', self.base, '--check')
        self.assertEqual((result, error), (1, ''))
        self.assertEqual(report['base_commit'], self.base)
        self.assertEqual(report['scanned_files'], 2)
        self.assertEqual(report['added'][0]['path'], 'crates/demo/src/new.rs')

    def test_clean_comparison_returns_zero_not_a_safety_claim(self):
        result, report, error = self.run_audit('--base', self.base, '--check')
        self.assertEqual((result, error, report['added']), (0, '', []))
        self.assertIn('not a safety proof', report['scope'])
        self.assertIn('escape detection', report['comparison'])

    def test_tracked_deletion_is_allowed_if_other_sources_exist(self):
        self.write('crates/demo/src/extra.rs', 'pub fn extra() {}')
        (self.root / 'crates/demo/src/lib.rs').unlink()
        result, report, _ = self.run_audit('--base', self.base, '--check')
        self.assertEqual((result, report['scanned_files']), (0, 1))

    def test_empty_tree_cannot_report_success(self):
        (self.root / 'crates/demo/src/lib.rs').unlink()
        result, report, error = self.run_audit()
        self.assertEqual((result, report), (2, None))
        self.assertIn('refusing an empty audit', error)

    def test_invalid_and_option_refs_fail_without_execution(self):
        for ref in ['missing-ref', '-h', 'HEAD; touch escaped']:
            with self.subTest(ref=ref):
                result, report, _ = self.run_audit('--base=' + ref, '--check')
                self.assertEqual((result, report), (2, None))
        self.assertFalse((self.root / 'escaped').exists())

    def test_symlinks_are_refused_instead_of_reading_external_data(self):
        outside = self.write('outside.rs', 'unsafe {}')
        link = self.root / 'crates/demo/src/linked.rs'
        link.symlink_to(outside)
        result, report, error = self.run_audit()
        self.assertEqual((result, report), (2, None))
        self.assertIn('symlink', error)

    def test_ignored_untracked_sources_are_excluded(self):
        self.write('.gitignore', 'crates/demo/src/generated.rs\n')
        self.write('crates/demo/src/generated.rs', 'unsafe {}')
        result, report, _ = self.run_audit('--base', self.base, '--check')
        self.assertEqual((result, report['scanned_files'], report['findings']), (0, 1, []))

    def test_skip_worktree_cannot_hide_sources(self):
        self.git('update-index', '--skip-worktree', 'crates/demo/src/lib.rs')
        result, report, error = self.run_audit()
        self.assertEqual((result, report), (2, None))
        self.assertIn('skip-worktree', error)

    def test_non_utf8_source_fails_closed(self):
        (self.root / 'crates/demo/src/lib.rs').write_bytes(b'\xff')
        result, report, _ = self.run_audit()
        self.assertEqual((result, report), (2, None))

    def test_subdirectory_is_not_silently_treated_as_the_whole_repository(self):
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            self.assertEqual(audit.main(['--root', str(self.root / 'crates')]), 2)

    def test_check_requires_explicit_base(self):
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as error:
            audit.main(['--root', str(self.root), '--check'])
        self.assertEqual(error.exception.code, 2)


if __name__ == '__main__':
    unittest.main()
