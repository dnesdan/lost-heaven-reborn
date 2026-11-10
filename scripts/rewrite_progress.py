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

# rw_data.dll own-function evidence (CRT excluded).
# Denominator source: mafia-re tools/progress.py Target("rw_data.dll",
# "RWDATA", 62): functions from the start of .text up to the first import
# thunk or CRT function; first CRT = RVA 0x5cc0 (operator delete
# ??3@YAXPAX@Z, library_msvc.h:7). The own set is 58 retained-pseudocode
# RVAs plus 4 XCU thunks: 12 retained init/synthetic (0x1010-0x1160),
# 26 named FUNCTION (rw_data.cpp + rw_data.h:26), 4
# scalar-deleting dtors / S_HANDLE assign (rw_data.h:38,
# rw_data_inter.h:149,152,220), 16 STL TEMPLATE instantiations
# (rw_data_inter.h:260-315), plus 4 XCU startup thunks (0x1000, 0x1060,
# 0x10f0, 0x1130) proven by the live .CRT$XCU slots at 0x1000c004-0x1000c010
# (NULL __xc_a at 0x1000c000, NULL terminator at 0x1000c014; bounds call passes
# 0x1000c000-0x1000c014) holding [0x10001000, 0x10001060, 0x100010f0,
# 0x10001130], and
# raw-body decode (each: CALL ctor + JMP $E-register; e.g. 0x1000 calls
# 0x1010, jumps 0x1030). Those 4 carry no retained pseudocode (Ghidra folds
# the boundaries) but are address-evidenced from the shipped bytes, so the
# Binja-only gap is zero and the own set is fully closed at 62.
RW_DATA_SHA256 = "67f72c30cc7fe941f5526b8602a0bd2785baa0f22617bbcfd4a71bacd79b8d49"
RW_DATA_FIRST_CRT_RVA = 0x5CC0
RW_DATA_OWN_RVAS = frozenset({
    # init/synthetic (12)
    "0x1010", "0x1030", "0x1040", "0x1070", "0x1090", "0x10a0",
    "0x1100", "0x1110", "0x1120", "0x1140", "0x1150", "0x1160",
    # named own functions (26)
    "0x1170", "0x11e0", "0x1330", "0x13b0", "0x1640", "0x1820",
    "0x1850", "0x18a0", "0x1b90", "0x23e0", "0x25a0", "0x25c0",
    "0x3730", "0x3900", "0x39f0", "0x3f50", "0x4040", "0x4120",
    "0x4130", "0x4140", "0x4150", "0x4160", "0x4250", "0x4690",
    "0x4720", "0x5740",
    # scalar-deleting dtors / S_HANDLE assignment (4)
    "0x1340", "0x1360", "0x1380", "0x5750",
    # XCU startup thunks (4): CALL ctor + JMP $E, pointed to by the live
    # .CRT$XCU table; raw-byte evidenced, no retained pseudocode.
    "0x1000", "0x1060", "0x10f0", "0x1130",
    # STL templates (16)
    "0x4730", "0x4760", "0x4970", "0x4c90", "0x4ce0", "0x4ef0",
    "0x4f00", "0x4f30", "0x5230", "0x55c0", "0x5600", "0x5630",
    "0x5650", "0x59f0", "0x5a30", "0x5a50",
})
RW_DATA_BINJA_ONLY_GAP = 0
RW_DATA_OWN_TOTAL = len(RW_DATA_OWN_RVAS) + RW_DATA_BINJA_ONLY_GAP  # 62
# Per-binary own-function denominators (without CRT). Only rw_data is
# evidenced; every other binary keeps its frozen inventory_total.
OWN_TOTAL_BY_SHA = {RW_DATA_SHA256: RW_DATA_OWN_TOTAL}
# Rust crate root per symbol prefix: rw_data ports live in lhr-assets, not
# lhr-engine.
CRATE_OF_SYMBOL_ROOT = {"lhr_engine": "lhr-engine", "lhr_assets": "lhr-assets"}


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


def validate_overlay_metadata(overlay):
    """Check metadata shape, not the truth or completeness of its evidence."""
    if not isinstance(overlay, dict) or not isinstance(overlay.get("functions"), list):
        raise ValueError("overlay functions must be a list")
    symbol_path = re.compile(r"[A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*){2,}")
    for index, entry in enumerate(overlay["functions"]):
        if not isinstance(entry, dict):
            raise ValueError(f"overlay entry {index} must be an object")
        for field in ("binary", "rva", "tier", "symbol", "evidence"):
            value = entry.get(field)
            if not isinstance(value, str) or not value.strip():
                raise ValueError(f"overlay entry {index}: {field} must be a nonempty string")
        if not RVA_RE.fullmatch(entry["rva"].lower()):
            raise ValueError(f"bad RVA form: {entry['rva']}")
        if entry["tier"] not in ("differential", "contract"):
            raise ValueError(f"bad tier: {entry['tier']}")
        if not symbol_path.fullmatch(entry["symbol"]):
            raise ValueError(f"bad symbol path: {entry['symbol']}")
        if "construction_covered" in entry and not isinstance(entry["construction_covered"], bool):
            raise ValueError(f"overlay entry {index}: construction_covered must be a boolean")


def validate_overlay(overlay, libraries, ledger_ports):
    """Raise ValueError on any inconsistency a commit must not introduce."""
    # Reject malformed/empty evidence before reading source or producing cards.
    # A nonempty description still needs independent provenance/runtime review.
    validate_overlay_metadata(overlay)
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
        if sha_of[name] == RW_DATA_SHA256 and rva not in RW_DATA_OWN_RVAS:
            raise ValueError(f"rw_data overlay entry outside own set: {rva}")
        # Rust symbol must exist in the tree (last path component in its module).
        # Crates map to their kebab-case directories; nested modules resolve to
        # subpaths (e.g. lhr_assets::rw_data::write::X -> lhr-assets/src/rw_data/write.rs).
        parts = entry["symbol"].split("::")
        crate_dirs = CRATE_OF_SYMBOL_ROOT
        if parts[0] not in crate_dirs or len(parts) < 3:
            raise ValueError(f"bad symbol path: {entry['symbol']}")
        crate_dir = crate_dirs[parts[0]]
        # Method symbols carry type segments (e.g. Handles::dta_open_write);
        # resolve to the deepest existing module file under the leading path.
        # #[path]-aware fallback: when the direct module file exists but does
        # not define the trailing name, only consult files referenced by its
        # #[path = "..."] attributes (sibling submodules such as
        # texture_factory.rs / texture_surface.rs / texture_load_image.rs
        # behind itexture.rs). Whole-crate substring search is forbidden: it
        # would mask wrong-module symbol paths.
        try:
            lib_body = (ROOT / "crates" / crate_dir / "src" / "lib.rs").read_text()
        except OSError:
            lib_body = ""
        found = None
        for i in range(len(parts) - 1, 1, -1):
            candidate = ROOT / "crates" / crate_dir / "src" / ("/".join(parts[1:i]) + ".rs")
            if candidate.exists():
                # Module membership: an on-disk file is not a module unless
                # the crate root declares it (texture_factory.rs etc. live
                # behind itexture.rs via #[path], never as crate modules).
                if not re.search(r"\bmod\s+" + re.escape(parts[1]) + r"\b", lib_body):
                    continue
                found = candidate
                break
        body = found.read_text() if found is not None else ""
        if found is None or not re.search(r"\b" + re.escape(parts[-1]) + r"\b", body):
            path_targets = []
            if found is not None:
                try:
                    for m in re.finditer(r'#\[path\s*=\s*"([^"]+)"\]', body):
                        target = (found.parent / m.group(1)).resolve()
                        try:
                            target.relative_to((ROOT / "crates" / crate_dir / "src").resolve())
                        except ValueError:
                            continue
                        if target.is_file():
                            path_targets.append(target)
                except OSError:
                    path_targets = []
            for candidate in sorted(path_targets):
                try:
                    text = candidate.read_text()
                except OSError:
                    continue
                if re.search(r"\b" + re.escape(parts[-1]) + r"\b", text):
                    # Penultimate-segment gate: the name before the symbol
                    # must be the defining file's stem (a real submodule
                    # like texture_surface), the re-exporting module itself
                    # (itexture, via `pub use factory::*`), or a type/trait
                    # defined in that module (TextureResources::method living
                    # in a #[path] sibling). Without this,
                    # itexture::bogus_mid::load_image would pass via
                    # texture_load_image.rs.
                    found_body = ""
                    if found is not None:
                        try:
                            found_body = found.read_text()
                        except OSError:
                            found_body = ""
                    is_type_in_found = bool(
                        re.search(
                            r"(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait|union)\s+"
                            + re.escape(parts[-2])
                            + r"\b",
                            found_body,
                        )
                    )
                    if parts[-2] != candidate.stem and (
                        found is None
                        or (parts[-2] != found.stem and not is_type_in_found)
                    ):
                        continue
                    found = candidate
                    body = text
                    break
            else:
                found = None
        if found is None:
            raise ValueError(
                f"missing module file: {ROOT / 'crates' / crate_dir / 'src' / (parts[1] + '.rs')}"
            )
        if entry.get("construction_covered"):
            # Covered-by-construction entries name a covering type/impl that
            # provides the semantics (Vec growth, construction, drop); they
            # must never claim differential tier (checked by test).
            if entry["tier"] == "differential":
                raise ValueError(f"construction-covered entry claims differential: {entry['symbol']}")
            if not re.search(r"\b" + re.escape(parts[-1]) + r"\b", body):
                raise ValueError(f"symbol not found: {entry['symbol']}")
        elif sha_of[name] == RW_DATA_SHA256 and re.search(
            r"(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|union|trait)\s+"
            + re.escape(parts[-1])
            + r"\b",
            body,
        ):
            raise ValueError(
                f"behavior symbol is a bare type, not behavior: {entry['symbol']}"
            )
        elif sha_of[name] == RW_DATA_SHA256 and not re.search(
            r"(?:pub(?:\([^)]*\))?\s+)?(?:fn|const|static)\s+"
            + re.escape(parts[-1])
            + r"\b",
            body,
        ):
            raise ValueError(f"behavior symbol is not defined code: {entry['symbol']}")
        elif not re.search(r"\b" + re.escape(parts[-1]) + r"\b", body):
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
        covered = set()
        for entry in extra:
            rva = entry["rva"].lower()
            ported[rva] = entry["tier"] == "differential"
            if entry.get("construction_covered"):
                covered.add(rva)
        if sha == RW_DATA_SHA256:
            # Never count CRT (or mistyped) entries toward an own-function card.
            ported = {
                rva: differential for rva, differential in ported.items()
                if rva in RW_DATA_OWN_RVAS
            }
            covered = {rva for rva in covered if rva in RW_DATA_OWN_RVAS}
        else:
            covered = set()
        behavior = {rva: diff for rva, diff in ported.items() if rva not in covered}
        inventoried = sum(1 for rva in ported if rva in inventory)
        cards.append({
            "name": name,
            "role": BINARY_ROLE[name],
            "total": OWN_TOTAL_BY_SHA.get(sha, lib.get("inventory_total", 0)),
            "ported": len(behavior),
            "covered": len(covered),
            "verified": sum(1 for differential in behavior.values() if differential),
            "inventoried": inventoried,
            "estimate": name == "Game.exe",
        })
    return cards


def render_svg(card):
    total = max(card["total"], 1)
    covered = card.get("covered", 0)
    ported_w = 560.0 * card["ported"] / total
    covered_w = 560.0 * covered / total
    verified_w = 560.0 * card["verified"] / total
    estimate = " (estimate*)" if card["estimate"] else ""
    covered_text = f" + {covered} construction-covered" if covered else ""
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="640" height="148" role="img">
<rect width="640" height="148" rx="8" fill="#f6f8fa"/>
<text x="24" y="34" font-family="sans-serif" font-size="20" font-weight="bold" fill="#1f2328">{escape(card["name"])}</text>
<text x="24" y="56" font-family="sans-serif" font-size="13" fill="#59636e">{escape(card["role"])}</text>
<text x="24" y="82" font-family="sans-serif" font-size="14" fill="#1f2328">Implemented {card["ported"]}/{card["total"]}{estimate}{covered_text} · Verified {card["verified"]}</text>
<rect x="24" y="94" width="560" height="18" rx="4" fill="#d1d9e0"/>
<rect x="24" y="94" width="{ported_w + covered_w:.1f}" height="18" rx="4" fill="#9ecbff"/>
<rect x="24" y="94" width="{verified_w:.1f}" height="18" rx="4" fill="#0969da"/>
<text x="24" y="132" font-family="sans-serif" font-size="11" fill="#59636e">own functions, no CRT · light = ported+covered · dark = differential evidence</text>
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
        "code has been worked through. `rw_data.dll` additionally reports "
        "construction-covered CRT/startup/empty/synthetic helpers (no separate "
        "code; covering construction + transparent evidence, contract tier only) "
        "apart from behavior ports. Static pseudocode retention, scoped ports, "
        "tests and original-runtime comparisons are independent measures and are "
        "never combined into one percentage.",
        "",
        "| Binary | Implemented | Verified |",
        "| --- | ---: | ---: |",
    ]
    for card in cards:
        estimate = "*" if card["estimate"] else ""
        covered = card.get("covered", 0)
        extra = f" + {covered} covered" if covered else ""
        lines.append(
            f"| `{card['name']}` | {card['ported']}/{card['total']}{estimate}{extra} "
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
