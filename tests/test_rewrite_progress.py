"""Unit tests for scripts/rewrite_progress.py (loads the script by path)."""
import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "scripts" / "rewrite_progress.py"


def load_script():
    spec = importlib.util.spec_from_file_location("rewrite_progress", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class RewriteProgressTests(unittest.TestCase):
    def test_live_overlay_validates_and_counts_combine(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        by_sha = script.ledger_ports_by_binary(ports)
        # Raises on any inconsistency a commit must not introduce.
        script.validate_overlay(overlay, libraries, by_sha)
        cards = script.aggregate(libraries, ports, overlay)
        game = next(card for card in cards if card["name"] == "Game.exe")
        self.assertEqual(game["total"], 4914)
        # Ledger base plus overlay entries, no overlap by construction.
        self.assertEqual(game["ported"], 149 + 23)
        self.assertEqual(game["verified"], 149 + 5)
        self.assertLessEqual(game["verified"], game["ported"])
        self.assertLessEqual(game["ported"], game["total"])

    def test_overlay_rejects_duplicates_and_unknown_symbols(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        by_sha = script.ledger_ports_by_binary(ports)
        bad = json.loads(json.dumps(overlay))
        bad["functions"].append(dict(bad["functions"][0]))
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)
        bad = json.loads(json.dumps(overlay))
        bad["functions"][0]["symbol"] = "lhr_engine::nope::missing_fn"
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)
        bad = json.loads(json.dumps(overlay))
        bad["functions"][0]["rva"] = "not-an-rva"
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)

    def test_rw_data_card_counts_own_functions_without_crt(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        cards = script.aggregate(libraries, ports, overlay)
        rw = next(card for card in cards if card["name"] == "rw_data.dll")
        self.assertEqual(rw["total"], 62)
        # 12 frozen ledger ports + 12 overlay behavior ports (2 write, 8
        # read/leak lane, DeCrypt, FirstForce) + 38 construction-covered
        # (4 XCU thunks, 12 further CRT startup, empty/synthetic dtors,
        # placement-delete no-op, 16 STL templates; contract tier only).
        self.assertEqual(rw["ported"], 24)
        self.assertEqual(rw.get("covered", 0), 38)
        self.assertEqual(rw["ported"] + rw.get("covered", 0), 62)
        self.assertEqual(rw["verified"], 17)
        self.assertLessEqual(rw["verified"], rw["ported"])
        self.assertLessEqual(rw["ported"], rw["total"])

    def test_rw_data_own_set_is_retained_below_first_crt_plus_xcu(self):
        # Evidence list pinned to the frozen ledger and the mafia-re total:
        # own = 58 retained entries below the first CRT address plus the 4
        # XCU startup thunks (0x1000/0x1060/0x10f0/0x1130) proven by the live
        # .CRT$XCU table at 0x1000c000 and raw CALL/JMP decode; they carry no
        # retained pseudocode (Ghidra folds the boundaries).
        script = load_script()
        libraries, _ = script.load_ledger()
        lib = libraries["rw_data.dll"]
        retained = {entry.lower() for entry in lib["retained_entries"]}
        self.assertEqual(len(retained), 176)
        below = {
            rva for rva in retained
            if int(rva, 16) < script.RW_DATA_FIRST_CRT_RVA
        }
        self.assertEqual(len(below), 58)
        xcu = {"0x1000", "0x1060", "0x10f0", "0x1130"}
        self.assertTrue(xcu.isdisjoint(below))
        self.assertEqual(set(script.RW_DATA_OWN_RVAS), below | xcu)
        self.assertEqual(len(script.RW_DATA_OWN_RVAS), 62)
        self.assertEqual(script.RW_DATA_BINJA_ONLY_GAP, 0)
        self.assertEqual(script.RW_DATA_OWN_TOTAL, 62)

    def test_rw_data_ledger_and_overlay_ports_are_all_own(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        sha = libraries["rw_data.dll"]["binary_sha256"]
        ledger = script.ledger_ports_by_binary(ports).get(sha, {})
        self.assertEqual(len(ledger), 12)
        for rva in ledger:
            self.assertIn(rva, script.RW_DATA_OWN_RVAS)
        for entry in overlay["functions"]:
            if entry["binary"] == "rw_data.dll":
                self.assertIn(
                    entry["rva"].lower(), script.RW_DATA_OWN_RVAS
                )
        # Construction-covered entries are all contract tier and exactly 38.
        covered = [
            entry for entry in overlay["functions"]
            if entry.get("construction_covered")
        ]
        self.assertEqual(len(covered), 38)
        for entry in covered:
            self.assertEqual(entry["binary"], "rw_data.dll")
            self.assertEqual(entry["tier"], "contract")

    def test_overlay_rejects_bare_type_behavior_for_rw_data(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        by_sha = script.ledger_ports_by_binary(ports)
        # A bare-type behavior symbol for rw_data must fail validation.
        # (Drop the live 0x1010 entry first so the duplicate tripwire, which
        # runs before the symbol check, does not mask the bare-type verdict.)
        bad = json.loads(json.dumps(overlay))
        bad["functions"] = [
            f for f in bad["functions"]
            if not (f["binary"] == "rw_data.dll" and f["rva"] == "0x1010")
        ]
        bad["functions"].append({
            "binary": "rw_data.dll",
            "rva": "0x1010",
            "va": "0x10001010",
            "name": "test only",
            "symbol": "lhr_assets::rw_data::Handles",
            "tier": "contract",
            "evidence": "test only",
        })
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)
        # ...unless flagged construction-covered (name presence suffices).
        good = json.loads(json.dumps(overlay))
        good["functions"] = [
            f for f in good["functions"]
            if not (f["binary"] == "rw_data.dll" and f["rva"] == "0x1010")
        ]
        good["functions"].append({
            "binary": "rw_data.dll",
            "rva": "0x1010",
            "va": "0x10001010",
            "name": "test only",
            "symbol": "lhr_assets::rw_data::Handles",
            "tier": "contract",
            "evidence": "test only",
            "construction_covered": True,
        })
        script.validate_overlay(good, libraries, by_sha)
        # Construction-covered entries must never claim differential.
        diff = json.loads(json.dumps(good))
        diff["functions"][-1]["tier"] = "differential"
        with self.assertRaises(ValueError):
            script.validate_overlay(diff, libraries, by_sha)

    def test_overlay_rejects_rw_data_crt_and_accepts_lhr_assets(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        by_sha = script.ledger_ports_by_binary(ports)
        # CRT entries must never count: _strcmp is retained but not own.
        bad = json.loads(json.dumps(overlay))
        bad["functions"].append({
            "binary": "rw_data.dll",
            "rva": "0x9510",
            "va": "0x10009510",
            "name": "strcmp",
            "symbol": "lhr_assets::rw_data::Handles",
            "tier": "contract",
            "evidence": "test only",
        })
        with self.assertRaises(ValueError):
            script.validate_overlay(bad, libraries, by_sha)
        # rw_data Rust lives in lhr-assets: an own RVA with a real symbol
        # there validates (de_crypt exists in
        # crates/lhr-assets/src/rw_data.rs). Re-add it on a copy with the
        # live entry removed to avoid the duplicate tripwire.
        good = json.loads(json.dumps(overlay))
        good["functions"] = [
            f for f in good["functions"]
            if not (f["binary"] == "rw_data.dll" and f["rva"] == "0x4690")
        ]
        good["functions"].append({
            "binary": "rw_data.dll",
            "rva": "0x4690",
            "va": "0x10004690",
            "name": "DeCrypt",
            "symbol": "lhr_assets::rw_data::de_crypt",
            "tier": "contract",
            "evidence": "test only",
        })
        script.validate_overlay(good, libraries, by_sha)

    def test_svg_render_is_deterministic(self):
        script = load_script()
        libraries, ports = script.load_ledger()
        overlay = script.load_overlay()
        first = script.aggregate(libraries, ports, overlay)
        second = script.aggregate(libraries, ports, overlay)
        self.assertEqual(
            [script.render_svg(card) for card in first],
            [script.render_svg(card) for card in second],
        )
        svg = script.render_svg(first[0])
        self.assertIn("Implemented", svg)
        self.assertIn("Verified", svg)

    def test_readme_check_passes_after_write(self):
        script = load_script()
        fragment, svgs, _ = script.build()
        self.assertIn("## Rewrite progress", fragment)
        self.assertIn("docs/rewrite/game.svg", fragment)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "README.md").write_text(
                "head\n" + script.START + "\n" + script.END + "\ntail\n"
            )
            text = (root / "README.md").read_text()
            before, rest = text.split(script.START, 1)
            _, after = rest.split(script.END, 1)
            (root / "README.md").write_text(
                before + script.START + "\n" + fragment + "\n" + script.END + after
            )
            reread = (root / "README.md").read_text()
            self.assertIn("## Rewrite progress", reread)
            self.assertTrue(reread.startswith("head\n"))
            self.assertTrue(reread.endswith("tail\n"))


if __name__ == "__main__":
    unittest.main()
