# Third-party references

Original project code is MIT licensed. The read-only model/bitmap parsers below
reuse MIT-licensed mafia-workbench code; no proprietary assets or decompiled
output are included. Git inspection on 2026-10-21 covered each root README, root
license when present, and repository tree at the exact revision shown.

| Reference | Inspected revision | Observed root license |
| --- | --- | --- |
| [mafia-workbench](https://github.com/b4rtt/mafia-workbench) | `9f1edbfbc20ab2ac57e66d4f8a444127e92d602e` | MIT |
| [MafiaUnity](https://github.com/MafiaHub/MafiaUnity) | `0d7a12f03ce4385173377c3e9f457dcc2e891f24` | Apache-2.0 |
| [Max4dsTools](https://github.com/pudingus/Max4dsTools) | `15aaca9bd161f36272167880f28e8e129e2292c7` | MIT |
| [MafiaBlenderToolkit](https://github.com/Richard01CZ/MafiaBlenderToolkit) | `24fffb771607fc9dab0ebca4432e4bf010c28cf6` | GPL-3.0 text; README states GPL-3.0-or-later |
| [mafia-re](https://github.com/Marvisak/mafia-re) | `9902b9a7cb5737eab15ebc1858069847e3d72b8f` | No root license found; research only |
| [PC analysis](https://github.com/st0rm94/mafia-reverse-engineering-export) | `65a0ca5c14138fdaeb7564304adf1509d79f09d4` | No root license found; research only |
| [PS2 decompiled](https://github.com/st0rm94/mafia-ps2-decompiled) | `5bc2f9c361597c8e723f97847e8733070064d533` | No root license found; research only |
| [LS3D-SceneKit](https://github.com/alex-alex/LS3D-SceneKit) | `8fadd2201f52936d998c66c47b0a0d37476c2ce5` | No root license found; research only |
| [bully-remake](https://github.com/oh-ashen-one/bully-remake) | `a7411f35bae4164707760250f0642e12d81db8c7` | MIT for original code; game assets excluded |
| [awesome-mafia](https://github.com/MafiaHub/awesome-mafia) | `4f6eda8703c0d73f1c3019a42e9cb376eaf49b8e` | Apache-2.0 |

Additional parser inspection: Workbench `research/formats.md`,
`src/core/{fourds,fiveds,dta}.js`; MafiaUnity
`Assets/Scripts/API/MafiaFormats/{4DSLoader,5DSLoader,Scene2BINLoader}.cs`;
Max4dsTools `scripts/max4ds/4ds_parser.ms`; MafiaBlenderToolkit
`io_mafia_toolkit/4ds/codec/document.py`. These distinguish archive/model/animation
and scene metadata and expose hierarchy/skinning/timing validation needs.
Sources other than the explicitly vendored Workbench parsers remain research
references; their inspection does not validate runtime fidelity.

## Reused mafia-workbench code

`importers/fourds/vendor/fourds.mjs` adapts `src/core/fourds.js` and
`importers/fourds/vendor/bmp.mjs` adapts `src/core/bmp.js`, and
`importers/fourds/vendor/fiveds.mjs` adapts `src/core/fiveds.js`, from
[b4rtt/mafia-workbench](https://github.com/b4rtt/mafia-workbench), revision
`9f1edbfbc20ab2ac57e66d4f8a444127e92d602e`.
Copyright (c) 2026 Roman Šimr; MIT License. The **complete upstream license** is
preserved in [importers/fourds/vendor/LICENSE](importers/fourds/vendor/LICENSE).
The adaptations retain read-only parsing, omit the editing functions and their
material-settings dependency, and replace iconv-lite string decoding with Node's
built-in Windows-1250 decoder. No other parser behavior is intentionally changed.

The original converter follows the basis/rotation interpretation documented in
upstream `src/ui/transforms.js` and `src/ui/viewport.js`; the source revision and
adaptations are recorded in [importers/fourds/PROVENANCE.md](importers/fourds/PROVENANCE.md).
The experimental matrix/skin/morph and animation conversion module is original
MIT project code; it copies no decompiled engine implementation. Its provisional
clock/interpolation conventions and preserved metadata are documented in the
same provenance file.
New GLB/PNG serialization, CLI validation and synthetic fixtures are original MIT
project code. Node.js built-in libraries provide hashing, compression and text
decoding; no npm dependency or upstream game data is distributed.

Godot 4.6.3 is a separately installed MIT-licensed runtime and export templates
from [official godot-builds releases](https://github.com/godotengine/godot-builds/releases/tag/4.6.3-stable).
Godot's third-party dependency notices are available from its About dialog and
[official license information](https://godotengine.org/license/). No proprietary
Mafia/PS2 assets, BIOS, reference captures, or analysis databases are distributed.
The export command writes `GODOT_NOTICES.txt` using the installed engine's embedded
license/copyright data and copies the original-code `LICENSE.txt` beside the game;
keep both files with distributed packages.
