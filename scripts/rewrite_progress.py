#!/usr/bin/env python3
"""Rewrite-progress dashboard for lost-heaven-reborn.

Reads the frozen docs/progress.json ledger (never writes it) plus the
hand-maintained docs/rewrite-overlay.json of post-snapshot ports, and
regenerates the Rewrite progress README fragment plus per-binary SVG cards
under docs/rewrite/.

Definitions mirror the mafia-re progress section honestly adapted to a Rust
behavioral rewrite: cards count each binary's own functions without the C
runtime. Implemented means reviewed Rust source with tests. Verified means
original-data or instruction-level differential evidence on top. There is no
reccmp score here; byte-exact C++ matching does not apply to Rust ports.

Usage:
  python3 scripts/rewrite_progress.py --write   # regenerate fragment + SVGs
  python3 scripts/rewrite_progress.py --check   # verify without writes
"""
import json
import re
import sys
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parent.parent
LEDGER = ROOT / "docs" / "progress.json"
OVERLAY = ROOT / "docs" / "rewrite-overlay.json"
README = ROOT / "README.md"
SVG_DIR = ROOT / "docs" / "rewrite"
START = "<!-- rewrite-progress:start -->"
END = "<!-- rewrite-progress:end -->"

BINARY_ORDER = ["Game.exe", "LS3DF.dll", "rw_data.dll", "Setup.exe (supplied configuration)"]
BINARY_ROLE = {
    "Game.exe": "Game and mission executable",
    "LS3DF.dll": "3D, animation, audio and rendering engine",
    "rw_data.dll": "Game archive and data access library",
    "Setup.exe (supplied configuration)": "Installed game configuration utility",
}
RVA_RE = re.compile(r"^0x[0-9a-f]+$")


def load_ledger():
    document = json.loads(LEDGER.read_text())
    libraries = {
        lib["name"]: lib for lib in document["function_progress"]["libraries"]
    }
    ports = document["function_progress"]["scoped_ports"]
    return libraries, ports


def load_overlay():
    return json.loads(OVERLAY.read_text())


def ledger_ports_by_binary(ports):
    """Map binary SHA-256 to {rva: differential} from the frozen ledger."""
    by_sha = {}
    for port in ports:
        differential = bool((port.get("comparison") or {}).get("id"))
        for function in port.get("functions", []):
            sha = function["binary_sha256"]
            rva = function["entry_rva"].lower()
            known = by_sha.setdefault(sha, {})
            known[rva] = known.get(rva, False) or differential
    return by_sha


def validate_overlay(overlay, libraries, ledger_ports):
    """Raise ValueError on any inconsistency a commit must not introduce."""
    sha_of = {lib["name"]: lib["binary_sha256"] for lib in libraries.values()}
    seen = set()
    for entry in overlay["functions"]:
        name = entry["binary"]
        if name not in sha_of:
            raise ValueError(f"unknown binary: {name}")
        rva = entry["rva"].lower()
        if not RVA_RE.match(rva):
            raise ValueError(f"bad RVA form: {rva}")
        if entry["tier"] not in ("differential", "contract"):
            raise ValueError(f"bad tier: {entry['tier']}")
        key = (sha_of[name], rva)
        if key in seen:
            raise ValueError(f"duplicate overlay entry: {name} {rva}")
        seen.add(key)
        if rva in ledger_ports.get(sha_of[name], {}):
            raise ValueError(f"overlay duplicates ledger port: {name} {rva}")
        # Rust symbol must exist in the tree (last path component in its module).
        parts = entry["symbol"].split("::")
        if parts[0] != "lhr_engine" or len(parts) < 3:
            raise ValueError(f"bad symbol path: {entry['symbol']}")
        module_file = ROOT / "crates" / "lhr-engine" / "src" / (parts[1] + ".rs")
        if not module_file.exists():
            raise ValueError(f"missing module file: {module_file}")
        body = module_file.read_text()
        if not re.search(r"\b" + re.escape(parts[-1]) + r"\b", body):
            raise ValueError(f"symbol not found: {entry['symbol']}")


def aggregate(libraries, ports, overlay):
    """Per-binary totals with ledger + overlay combined (overlay never edited)."""
    sha_of = {lib["name"]: lib["binary_sha256"] for lib in libraries.values()}
    ledger_ports = ledger_ports_by_binary(ports)
    cards = []
    for name in BINARY_ORDER:
        lib = libraries[name]
        sha = sha_of[name]
        inventory = set(e.lower() for e in lib.get("retained_entries", []))
        base = ledger_ports.get(sha, {})
        extra = [
            e for e in overlay["functions"]
            if e["binary"] == name
        ]
        ported = dict(base)
        for entry in extra:
            ported[entry["rva"].lower()] = entry["tier"] == "differential"
        inventoried = sum(1 for rva in ported if rva in inventory)
        cards.append({
            "name": name,
            "role": BINARY_ROLE[name],
            "total": lib.get("inventory_total", 0),
            "ported": len(ported),
            "verified": sum(1 for differential in ported.values() if differential),
            "inventoried": inventoried,
            "estimate": name == "Game.exe",
        })
    return cards


def render_svg(card):
    total = max(card["total"], 1)
    ported_w = 560.0 * card["ported"] / total
    verified_w = 560.0 * card["verified"] / total
    estimate = " (estimate*)" if card["estimate"] else ""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="640" height="148" role="img">
<rect width="640" height="148" rx="8" fill="#f6f8fa"/>
<text x="24" y="34" font-family="sans-serif" font-size="20" font-weight="bold" fill="#1f2328">{escape(card["name"])}</text>
<text x="24" y="56" font-family="sans-serif" font-size="13" fill="#59636e">{escape(card["role"])}</text>
<text x="24" y="82" font-family="sans-serif" font-size="14" fill="#1f2328">Implemented {card["ported"]}/{card["total"]}{estimate} · Verified {card["verified"]}</text>
<rect x="24" y="94" width="560" height="18" rx="4" fill="#d1d9e0"/>
<rect x="24" y="94" width="{ported_w:.1f}" height="18" rx="4" fill="#9ecbff"/>
<rect x="24" y="94" width="{verified_w:.1f}" height="18" rx="4" fill="#0969da"/>
<text x="24" y="132" font-family="sans-serif" font-size="11" fill="#59636e">own functions, no CRT · light = ported · dark = differential evidence</text>
</svg>
"""


def render_fragment(cards, overlay):
    lines = [
        "## Rewrite progress",
        "",
        '<p align="center">',
    ]
    for card in cards:
        slug = (
            card["name"]
            .lower()
            .replace(".dll", "")
            .replace(".exe", "")
            .replace(" (supplied configuration)", "")
            .replace(" ", "-")
        )
        lines.append(f'<img src="docs/rewrite/{slug}.svg" width="49%">')
    lines += [
        "</p>",
        "",
        "The cards count each binary's own functions, without the C runtime linked "
        "into it. *Implemented* is how many of them have reviewed Rust source with "
        "tests. *Verified* is how many of those carry original-data or "
        "instruction-level differential evidence on top. There is no reccmp score "
        "here: byte-exact C++ matching does not apply to behavioral Rust ports. "
        "The function counts are exact but for `Game.exe`'s, an estimate until its "
        "code has been worked through. Static pseudocode retention, scoped ports, "
        "tests and original-runtime comparisons are independent measures and are "
        "never combined into one percentage.",
        "",
        "| Binary | Implemented | Verified |",
        "| --- | ---: | ---: |",
    ]
    for card in cards:
        estimate = "*" if card["estimate"] else ""
        lines.append(
            f"| `{card['name']}` | {card['ported']}/{card['total']}{estimate} "
            f"| {card['verified']} |"
        )
    lines += [
        "",
        f"*Estimate: frozen inventory union. PS2 `SLUS_206.71` supplies linker "
        f"naming evidence only, without function coverage. Installer/bootstrap "
        f"helpers (`DrvMgt.dll`, `INet.dll`, `IJoy.dll`, codecs) are unmeasured "
        f"boundaries, not zero ports.",
        "",
        f"Script VM: {overlay['vm']['decoded_base']} base + "
        f"{overlay['vm']['decoded_entity']} entity opcodes decoded; executing "
        f"frontier advances with the probe ({overlay['vm']['executing_frontier']}).",
        "",
        f"Gap: {overlay['gap_note']}",
        "",
    ]
    return "\n".join(lines)


def build():
    libraries, ports = load_ledger()
    overlay = load_overlay()
    validate_overlay(overlay, libraries, ledger_ports_by_binary(ports))
    cards = aggregate(libraries, ports, overlay)
    svgs = {}
    for card in cards:
        slug = (
            card["name"]
            .lower()
            .replace(".dll", "")
            .replace(".exe", "")
            .replace(" (supplied configuration)", "")
            .replace(" ", "-")
        )
        svgs[f"docs/rewrite/{slug}.svg"] = render_svg(card)
    return render_fragment(cards, overlay), svgs, cards


def splice_readme(fragment):
    text = README.read_text()
    if START not in text or END not in text:
        raise ValueError("README rewrite markers missing")
    before, rest = text.split(START, 1)
    _, after = rest.split(END, 1)
    return before + START + "\n" + fragment + "\n" + END + after


def main(argv):
    write = "--write" in argv
    check = "--check" in argv
    if write == check:
        print("usage: rewrite_progress.py --write | --check")
        return 2
    fragment, svgs, cards = build()
    if check:
        current = README.read_text()
        if splice_readme(fragment) != current:
            print("README rewrite fragment differs; run --write")
            return 1
        for path, content in svgs.items():
            existing = ROOT / path
            if not existing.exists() or existing.read_text() != content:
                print(f"SVG differs: {path}; run --write")
                return 1
        for card in cards:
            print(
                f"{card['name']}: {card['ported']}/{card['total']} "
                f"implemented, {card['verified']} verified"
            )
        print("rewrite progress check passed")
        return 0
    README.write_text(splice_readme(fragment))
    for path, content in svgs.items():
        target = ROOT / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)
    print("rewrite progress written")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
