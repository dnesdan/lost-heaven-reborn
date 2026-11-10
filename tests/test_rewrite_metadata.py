"""Overlay admission tests using public synthetic metadata and source files."""
import contextlib
import copy
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "rewrite_metadata_subject", ROOT / "scripts" / "rewrite_progress.py"
)
progress = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(progress)


class RewriteMetadataTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for crate in ("lhr-engine", "lhr-assets"):
            source = self.root / "crates" / crate / "src"
            source.mkdir(parents=True)
            (source / "lib.rs").write_text("pub mod fixture;\n")
            (source / "fixture.rs").write_text("pub fn port() {}\n")
        self.libraries = {
            name: {
                "name": name,
                "binary_sha256": (
                    progress.RW_DATA_SHA256 if name == "rw_data.dll" else f"fixture-{index}"
                ),
                "inventory_total": 100,
                "retained_entries": ["0x1170"],
            }
            for index, name in enumerate(progress.BINARY_ORDER)
        }
        patches = mock.patch.multiple(
            progress,
            ROOT=self.root,
            LEDGER=self.root / "docs" / "progress.json",
            OVERLAY=self.root / "docs" / "rewrite-overlay.json",
            README=self.root / "README.md",
        )
        patches.start()
        self.addCleanup(patches.stop)

    def entry(self, binary="Game.exe", tier="contract"):
        crate = "lhr_assets" if binary == "rw_data.dll" else "lhr_engine"
        return {
            "binary": binary,
            "rva": "0x1170",
            "symbol": f"{crate}::fixture::port",
            "tier": tier,
            "evidence": "Synthetic fixture; not an original-runtime claim.",
        }

    def validate(self, entries):
        progress.validate_overlay({"functions": entries}, self.libraries, {})

    def write_inputs(self, entries):
        progress.LEDGER.parent.mkdir(parents=True, exist_ok=True)
        progress.LEDGER.write_text(json.dumps({
            "function_progress": {
                "libraries": list(self.libraries.values()),
                "scoped_ports": [],
            }
        }))
        progress.OVERLAY.write_text(json.dumps({
            "functions": entries,
            "vm": {"decoded_base": 0, "decoded_entity": 0, "executing_frontier": "fixture"},
            "gap_note": "Synthetic test input only.",
        }))
        progress.README.write_text(
            "before\n" + progress.START + "\nold\n" + progress.END + "\nafter\n"
        )

    def test_valid_records_preserve_input_and_counts(self):
        for binary in progress.BINARY_ORDER:
            for tier in ("contract", "differential"):
                with self.subTest(binary=binary, tier=tier):
                    entry = self.entry(binary, tier)
                    before = copy.deepcopy(entry)
                    self.validate([entry])
                    cards = progress.aggregate(self.libraries, [], {"functions": [entry]})
                    card = next(card for card in cards if card["name"] == binary)
                    self.assertEqual(card["ported"], 1)
                    self.assertEqual(card["verified"], int(tier == "differential"))
                    self.assertEqual(entry, before)

    def test_missing_blank_and_nonstring_evidence_are_rejected(self):
        for binary in progress.BINARY_ORDER:
            for tier in ("contract", "differential"):
                for evidence in (None, "", " \t\r\n", 0, False, [], {}):
                    with self.subTest(binary=binary, tier=tier, evidence=evidence):
                        entry = self.entry(binary, tier)
                        entry["evidence"] = evidence
                        with self.assertRaisesRegex(ValueError, "evidence must be a nonempty string"):
                            self.validate([entry])
                entry = self.entry(binary, tier)
                del entry["evidence"]
                with self.assertRaisesRegex(ValueError, "evidence must be a nonempty string"):
                    self.validate([entry])

    def test_required_fields_fail_with_value_error(self):
        for field in ("binary", "rva", "tier", "symbol"):
            for value in (None, "", " \t", 3, False, [], {}):
                with self.subTest(field=field, value=value):
                    entry = self.entry()
                    entry[field] = value
                    with self.assertRaisesRegex(ValueError, f"{field} must be a nonempty string"):
                        self.validate([entry])
            entry = self.entry()
            del entry[field]
            with self.assertRaisesRegex(ValueError, f"{field} must be a nonempty string"):
                self.validate([entry])

    def test_malformed_overlay_and_rows_are_rejected(self):
        for overlay in (None, [], {}, {"functions": None}, {"functions": {}}, {"functions": "x"}):
            with self.subTest(overlay=overlay):
                with self.assertRaisesRegex(ValueError, "functions must be a list"):
                    progress.validate_overlay(overlay, self.libraries, {})
        for entry in (None, [], "x", 1, False):
            with self.subTest(entry=entry):
                with self.assertRaisesRegex(ValueError, "entry 0 must be an object"):
                    self.validate([entry])
        self.validate([])

    def test_construction_covered_requires_a_boolean(self):
        for value in ("false", "true", "", 0, 1, None, [], {}):
            with self.subTest(value=value):
                entry = self.entry("rw_data.dll")
                entry["construction_covered"] = value
                with self.assertRaisesRegex(ValueError, "construction_covered must be a boolean"):
                    self.validate([entry])
        for value in (False, True):
            entry = self.entry("rw_data.dll")
            entry["construction_covered"] = value
            self.validate([entry])
            cards = progress.aggregate(self.libraries, [], {"functions": [entry]})
            card = next(card for card in cards if card["name"] == "rw_data.dll")
            self.assertEqual(card["covered"], int(value))
            self.assertEqual(card["ported"], int(not value))
        entry = self.entry("rw_data.dll", "differential")
        entry["construction_covered"] = True
        with self.assertRaisesRegex(ValueError, "construction-covered entry claims differential"):
            self.validate([entry])

    def test_symbol_path_rejects_traversal_and_empty_segments_before_io(self):
        for symbol in (
            "lhr_engine::../fixture::port", "lhr_engine::/fixture::port",
            "lhr_engine::fixture::..::port", "lhr_engine::fixture::::port",
            "lhr_engine::fixture::port\n", "lhr_engine::fixture::port ",
            "lhr_engine::fixture::port/other", "lhr_engine::fixture::9port",
            "lhr_engine::fixture", "lhr_engine::fixture::port\\other",
        ):
            with self.subTest(symbol=symbol):
                entry = self.entry()
                entry["symbol"] = symbol
                with mock.patch.object(Path, "read_text", side_effect=AssertionError("source read")):
                    with self.assertRaisesRegex(ValueError, "bad symbol path"):
                        self.validate([entry])

    def test_metadata_for_all_rows_is_checked_before_source_io(self):
        invalid = self.entry("LS3DF.dll")
        invalid["evidence"] = "\n"
        with mock.patch.object(Path, "read_text", side_effect=AssertionError("source read")):
            with self.assertRaisesRegex(ValueError, "entry 1: evidence"):
                self.validate([self.entry(), invalid])

    def test_rva_must_match_the_whole_value(self):
        for rva in ("0x1170\n", " 0x1170", "0x1170suffix", "0x", "1170"):
            with self.subTest(rva=rva):
                entry = self.entry()
                entry["rva"] = rva
                with self.assertRaisesRegex(ValueError, "bad RVA form"):
                    self.validate([entry])
        entry = self.entry()
        entry["rva"] = "0x1ABC"
        self.validate([entry])

    def test_existing_identity_and_symbol_guards_remain(self):
        with self.assertRaisesRegex(ValueError, "duplicate overlay"):
            self.validate([self.entry(), self.entry()])
        entry = self.entry()
        with self.assertRaisesRegex(ValueError, "duplicates ledger"):
            progress.validate_overlay(
                {"functions": [entry]}, self.libraries, {"fixture-0": {"0x1170": True}}
            )
        entry = self.entry()
        entry["symbol"] = "lhr_engine::fixture::missing"
        with self.assertRaises(ValueError):
            self.validate([entry])
        entry = self.entry()
        entry["tier"] = "verified"
        with self.assertRaisesRegex(ValueError, "bad tier"):
            self.validate([entry])
        entry = self.entry()
        entry["binary"] = "Unknown.exe"
        with self.assertRaisesRegex(ValueError, "unknown binary"):
            self.validate([entry])

    def test_blank_evidence_cannot_overwrite_outputs(self):
        self.write_inputs([self.entry()])
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(progress.main(["--write"]), 0)
        outputs = [progress.README, *sorted((self.root / "docs" / "rewrite").glob("*.svg"))]
        self.assertEqual(len(outputs), 5)
        before = {path: path.read_bytes() for path in outputs}
        overlay = json.loads(progress.OVERLAY.read_text())
        overlay["functions"][0]["evidence"] = ""
        progress.OVERLAY.write_text(json.dumps(overlay))
        for mode in ("--check", "--write"):
            with self.subTest(mode=mode):
                with self.assertRaisesRegex(ValueError, "evidence must be a nonempty string"):
                    progress.main([mode])
                self.assertEqual({path: path.read_bytes() for path in outputs}, before)

    def test_valid_end_to_end_write_and_check(self):
        self.write_inputs([self.entry()])
        ledger_before = progress.LEDGER.read_bytes()
        overlay_before = progress.OVERLAY.read_bytes()
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(progress.main(["--write"]), 0)
            self.assertEqual(progress.main(["--check"]), 0)
        self.assertEqual(progress.LEDGER.read_bytes(), ledger_before)
        self.assertEqual(progress.OVERLAY.read_bytes(), overlay_before)
        self.assertTrue(progress.README.read_text().startswith("before\n"))
        self.assertTrue(progress.README.read_text().endswith("\nafter\n"))


if __name__ == "__main__":
    unittest.main()
