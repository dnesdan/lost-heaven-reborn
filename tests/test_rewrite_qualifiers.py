"""Bounded qualifier-name checks; not a Rust compiler or namespace resolver."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "rewrite_qualifier_subject", ROOT / "scripts" / "rewrite_progress.py"
)
progress = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(progress)


class RewriteQualifierTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.source = self.root / "crates" / "lhr-engine" / "src"
        self.source.mkdir(parents=True)
        (self.source / "lib.rs").write_text("pub mod fixture;\n")
        self.module = self.source / "fixture.rs"
        self.module.write_text("pub fn port() {}\n")
        patch = mock.patch.object(progress, "ROOT", self.root)
        patch.start()
        self.addCleanup(patch.stop)
        self.libraries = {"Game.exe": {"name": "Game.exe", "binary_sha256": "synthetic"}}

    def validate(self, symbol):
        progress.validate_overlay({"functions": [{
            "binary": "Game.exe",
            "rva": "0x1170",
            "symbol": symbol,
            "tier": "contract",
            "evidence": "Synthetic source fixture only.",
        }]}, self.libraries, {})

    def test_direct_function_cannot_hide_a_missing_type(self):
        self.validate("lhr_engine::fixture::port")
        with self.assertRaisesRegex(ValueError, "unresolved symbol qualifiers.*NonexistentType"):
            self.validate("lhr_engine::fixture::NonexistentType::port")

    def test_comments_and_literals_do_not_declare_qualifiers(self):
        for text in (
            "// struct Bogus;\n",
            "/* struct Bogus; /* enum Nested {} */ */\n",
            'const TEXT: &str = "struct Bogus;";\n',
            'const TEXT: &str = r###"type Bogus = u32;"###;\n',
            'const TEXT: &[u8] = br#"mod Bogus {}"#;\n',
        ):
            with self.subTest(text=text):
                self.module.write_text(text + "pub fn port() {}\n")
                with self.assertRaisesRegex(ValueError, "unresolved symbol qualifiers.*Bogus"):
                    self.validate("lhr_engine::fixture::Bogus::port")

    def test_declared_types_aliases_and_inline_modules_remain_admitted(self):
        for declaration in (
            "pub struct Known;",
            "pub enum Known { A }",
            "pub union Known { a: u32 }",
            "pub trait Known { fn port(); }",
            "pub type Known = u32;",
            "pub mod Known { pub fn port() {} }",
        ):
            with self.subTest(declaration=declaration):
                self.module.write_text(declaration + "\npub fn port() {}\n")
                self.validate("lhr_engine::fixture::Known::port")
        # This check establishes a qualifier spelling, not membership of the
        # method in its type; the compiler/source review owns that distinction.

    def test_explicit_import_spellings_remain_admitted(self):
        for declaration in (
            "use crate::other::Known;",
            "pub use crate::other::Original as Known;",
            "use crate::other::{Known, Another};",
        ):
            with self.subTest(declaration=declaration):
                self.module.write_text(declaration + "\npub fn port() {}\n")
                self.validate("lhr_engine::fixture::Known::port")

    def test_every_extra_segment_is_checked(self):
        self.module.write_text("pub struct Known;\npub fn port() {}\n")
        for symbol in (
            "lhr_engine::fixture::Missing::Known::port",
            "lhr_engine::fixture::Known::Missing::port",
        ):
            with self.subTest(symbol=symbol):
                with self.assertRaisesRegex(ValueError, "unresolved symbol qualifiers.*Missing"):
                    self.validate(symbol)

    def test_path_sibling_convention_remains_and_cannot_hide_missing_segments(self):
        self.module.write_text(
            'pub struct Known;\n#[path = "detail.rs"]\nmod implementation;\n'
        )
        (self.source / "detail.rs").write_text("pub fn port() {}\n")
        self.validate("lhr_engine::fixture::detail::port")
        self.validate("lhr_engine::fixture::Known::port")
        for symbol in (
            "lhr_engine::fixture::Missing::Known::port",
            "lhr_engine::fixture::Missing::detail::port",
            "lhr_engine::fixture::Missing::port",
        ):
            with self.subTest(symbol=symbol):
                with self.assertRaisesRegex(ValueError, "unresolved symbol qualifiers.*Missing"):
                    self.validate(symbol)

    def test_real_nested_file_modules_remain_admitted(self):
        self.module.write_text("pub mod nested;\n")
        nested = self.source / "fixture"
        nested.mkdir()
        (nested / "nested.rs").write_text("pub struct Known;\npub fn port() {}\n")
        self.validate("lhr_engine::fixture::nested::port")
        self.validate("lhr_engine::fixture::nested::Known::port")
        with self.assertRaisesRegex(ValueError, "unresolved symbol qualifiers.*Missing"):
            self.validate("lhr_engine::fixture::nested::Missing::port")

    def test_glob_imported_parent_types_remain_admitted(self):
        # Mirrors lhr_assets::rw_data::write::Handles::dta_open_write: the
        # type is declared in the parent module and pulled in by
        # `use super::*;`, so the qualifier is lexically available even
        # though no `struct Known;` sits in the child file itself.
        self.module.write_text("pub mod child;\npub struct Known;\n")
        child = self.source / "fixture"
        child.mkdir()
        (child / "child.rs").write_text("use super::*;\npub fn port() {}\n")
        self.validate("lhr_engine::fixture::child::Known::port")
        with self.assertRaisesRegex(ValueError, "unresolved symbol qualifiers.*Missing"):
            self.validate("lhr_engine::fixture::child::Missing::port")

    def test_lexer_errors_are_not_accepted_as_qualifier_evidence(self):
        for ending in ('/* unfinished', 'const S: &str = "unfinished'):
            with self.subTest(ending=ending):
                self.module.write_text("pub struct Known;\npub fn port() {}\n" + ending)
                with self.assertRaisesRegex(ValueError, "cannot inspect symbol qualifiers"):
                    self.validate("lhr_engine::fixture::Known::port")

    def test_lexer_loading_never_runs_git_or_source_commands(self):
        progress._rust_lexer.cache_clear()
        self.module.write_text("pub struct Known;\npub fn port() {}\n")
        with mock.patch.object(subprocess, "run", side_effect=AssertionError("unexpected command")):
            self.validate("lhr_engine::fixture::Known::port")

    def test_cache_is_per_validation_not_a_stale_source_authority(self):
        self.module.write_text("pub struct Known;\npub fn port() {}\n")
        self.validate("lhr_engine::fixture::Known::port")
        self.module.write_text("pub fn port() {}\n")
        with self.assertRaisesRegex(ValueError, "unresolved symbol qualifiers.*Known"):
            self.validate("lhr_engine::fixture::Known::port")


if __name__ == "__main__":
    unittest.main()
