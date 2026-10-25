# Texture and Scene CPU lifetime — historical Source-v5

These previous current summaries retain their source-bound scope at baseline
`3fbd671`. They are historical evidence, not the latest validation.

## README.md at baseline 3fbd671

The latest bounded CPU milestone adds immediate file-backed texture creation,
cache/reference lifetime, constructor-owned Scene cleanup and ordered actor-loader
interfaces. Legacy loaded-model textures retain their existing owner; the new
canonical texture owner supplies the separate weather domain. Supplied data has
no `Snow.tga`, so the original constructor's null-texture outcome is preserved.
The actor loader requires caller-supplied preceding LoadMap context; the API does
not execute or prove that phase. Complete Mission Open remains unavailable.

Accepted code: `c84bcff`; [source-bound validation](../../COVERAGE.md).


## COVERAGE.md at baseline 3fbd671

## Current bounded milestone

Immediate `IGraph::CreateITexture` supports the admitted file-backed CPU domain,
source-BPP/alpha behavior, real cache identity, references and last release.
Flags `2` and managed `0x40` remain distinct. Texture validation has 46 source
cases in both actual debug/release profiles; profiles do not multiply cases.
Native comparison uses uncompressed format 21, minimum 1 / maximum 1024 texture
caps and no hardware DXT support; tested image sizes are 2×2 and 4×1. General
resizing, mipmaps, compressed formats and padded-alpha rows remain outside it.

Constructor-owned Scene CPU lifetime covers three scenarios / 16 phase
projections, including two Scenes and external survivors. Close reaches the
existing ModelArena resource owner and its 19 legacy textures, admitted only
without managed `0x40`; those identities are not migrated to the separate
canonical weather texture domain. Shutdown leaves zero owned resources,
including the Scene; older five-prefix-frame receipts are historical.

All 14 supplied DTA archives / 36,311 entries contain no `Snow.tga`. Native code
ignores that factory failure and retains null; no replacement is invented.
Original `#kom03.bmp` checks exercise real factory/cache/last-release ownership;
synthetic Snow inputs test lifetime, not authentic weather rendering.

The actor-loader interface requires caller-supplied preceding LoadMap context,
ActorWorld and detector state. That API does not perform or prove preceding
construction. Eight controlled original caller traces and six separate Rust
units are not a direct differential comparison or complete Mission Open.
MISE01's first kind-8 `C_railway` chain is researched, not ported.

CPU Close records `native_device_virtual14_executed=false`; the strict low-level
interface rejects that unavailable device operation. Driver CreateFrame,
listener creation, viewport override, full native Close and GPU parity remain
open. The script probe still stops at FINDACTOR/UnsupportedWorldBackend,
PC 3 / source line 4.

Code commit `c84bcff` binds the final 270-file Source-v5 tree. The full gate
passes 1093 Rust tests (114 assets, 7 assets integration, 933 engine library,
26 application, 8 engine integration, 5 renderer), 9 REA-wrapper and 47 progress
tests, formatting, locked checking, Clippy and the application build. Both
sequential independent non-author reviews approve this bounded source.
Actual debug window input exercises W/A/D/S/Space, three R reloads and Escape;
four cleanups leave zero owned CPU resources including the Scene. App and Xvfb
exit successfully and are reaped. [Source-bound records](../progress.json).


## docs/PROGRESS.md at baseline 3fbd671

## Current milestone — 2026-10-25

Bounded immediate texture creation now composes with constructor-owned CPU Scene
lifetime. Real cache/reference cleanup and the live ModelArena owner are exercised;
legacy model textures and canonical weather textures remain distinct domains.
The supplied installation has no Snow texture, preserving the native null outcome.

Scene comparison covers three scenarios / 16 phase projections. Texture comparison
covers 46 source cases in both Cargo profiles. The ordered actor-loader interface
has eight controlled original caller traces and six separate Rust units, without
direct differential or full Mission Open credit. Caller-supplied LoadMap context is
required; its presence in the API does not prove preceding construction occurred.

CPU device-operation, Driver CreateFrame/listener/viewport override, authored actor classes,
player/camera/scripts and mission completion remain open. The first MISE01 kind-8
railway chain is researched but unported. The script probe remains at PC 3 / line 4,
FINDACTOR/UnsupportedWorldBackend.

Code commit `c84bcff` binds the final 270-file Source-v5 tree. The full gate
passes 1093 Rust tests (114 assets, 7 assets integration, 933 engine library,
26 application, 8 engine integration, 5 renderer), 9 REA-wrapper and 47 progress
tests, formatting, locked checking, Clippy and the application build. Both
sequential independent non-author reviews approve this bounded source.
Actual debug window input exercises W/A/D/S/Space, three R reloads and Escape;
four cleanups leave zero owned CPU resources including the Scene. App and Xvfb
exit successfully and are reaped. [Source-bound records](../progress.json).
