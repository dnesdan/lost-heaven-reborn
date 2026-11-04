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
