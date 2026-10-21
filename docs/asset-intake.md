# Original asset intake boundary

`lhr-assets` reads original bytes directly, without a Godot or GLB dependency.
The current scope is static standard visual meshes and dummy hierarchy in 4DS
version 29, all stored LOD records, and uncompressed 8-bit palette or 24-bit BGR
Windows BITMAPINFOHEADER BMP textures. The renderer chooses LOD0 explicitly;
original distance selection is not implemented. Input vertices, indices, float32
values, material references and frame order are preserved. Material references
and parents become zero-based Rust indices. Normals/UVs must be finite: damaged
or unused uninitialised values are rejected rather than replaced.

The static path rejects animated models, skins, morphs, other visual/frame types,
zero rotations, nonpositive scale, alpha/reflection/additive/animated/emissive
materials, malformed hierarchy and invalid references. Unknown name encoding is
rejected (ASCII names only). Texture addressing flags remain on each material.
The diffuse renderer does not reconstruct original lighting or material behavior.

Coordinate convention is stored left-handed Y-up. Stored quaternion components
are WXYZ; the existing format interpretation conjugates these for column vectors,
composes T*R*S, and reflects world Z and triangle winding for a right-handed
renderer. This convention is a data-format interpretation, not runtime fidelity.

`Installation` reads confined, bounded loose files from a user-selected directory.
It rejects traversal and symlink escapes and never launches an extractor. Reads
are case-sensitive. Read-only DTA archive lookup is **not implemented**: an
installation containing only archives is therefore not supported yet. No original
files or converted geometry belong in the repository.

Each input is limited to 64 MiB; models additionally limit frames, LOD count,
vertices and indices. Textures limit dimensions and decoded pixels. All reads use
checked input bounds. Synthetic tests exercise complete-file truncations,
headers, malformed paths, BMP padding/color, invalid dimensions and compression.
The read-only `inspect` example reports decoded counts without emitting source
names or geometry; private validation paths/hashes are stored outside Git.

## Provenance

The Rust reader is a manual adaptation of the read-only layout/algorithm in
`b4rtt/mafia-workbench` `src/core/fourds.js`, revision
`9f1edbfbc20ab2ac57e66d4f8a444127e92d602e` (MIT), with stricter quotas and
unsupported-feature rejection; editing, animation and sector functionality was
not carried over. BMP decoding is implemented from the documented Windows
BITMAPINFOHEADER layout. The original MIT copyright/license is retained at
`crates/lhr-assets/UPSTREAM_LICENSE`. Static format evidence and original-binary
boot evidence are separate; this parser establishes no gameplay behavior.

## Material tests and attribution notice

`Material.flags` is public. Before reading optional unsupported payloads, the
parser rejects `0x8000`, `0x80000`, `0x01000000`, `0x04000000`, `0x20000000`,
`0x40000000`, `0x80000000`, non-unit opacity and nonzero emission. Synthetic
cases assert each flag's explicit unsupported-material error. Clamp flags
`0x10000` (U) and `0x20000` (V) are preserved and accepted; absence means repeat.
These flags follow the pinned format implementation; original engine lighting,
filtering, alpha handling and texture selection are not asserted equivalent.

Third-party notice: portions of `crates/lhr-assets/src/lib.rs` adapt the MIT
read-only 4DS layout algorithm by Roman Šimr (2026), from the exact workbench
revision above. Copyright and full MIT permission text are preserved in
`crates/lhr-assets/UPSTREAM_LICENSE`. The adaptation adds bounded resource reads,
Rust structures, strict feature rejection and synthetic tests; it omits editing,
skin/morph, sector and original runtime behavior. No proprietary assets, original
binaries or decompiled source are included.
