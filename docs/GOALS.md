# Reconstruction goals

## Current priority: original Freeride

The user requires the actual original Freeride mode, not a renamed MISE01 or
standalone driving sandbox. Acceptance requires the original map and startup
order (including FreerideSetup before Init and FreerideSetupAfter afterward),
Tommy/car coupling, the game update loop, city dynamics, observed interactive
control and repeatable cleanup. No Freeride acceptance gate is closed.
The fresh-session Freeride startup now has its own one-shot driver. Synthetic
tests cover actual PLAYER/CAR construction, model naming and retained-creator
cleanup. Actual car/player index tables now have one session-owned startup owner.
The menu view is borrowing-ready over the S_freeride type used by Setup but is
constructed only in tests: production menu/profile/session borrowing and an
application-level state owner remain uncomposed. No persistent car or
location selection is inferred from the profile: native GM_FreeRide is still
missing. Without explicit diagnostic StoreA/StoreB input, Setup stops before
GetCarModel. With that input, synthetic tests reach the real car cache Open
and stop before the unavailable Scene named-frame lookup; original content can
stop earlier. This input does not reconstruct the menu or close an acceptance gate.
A headless original-data startup run completes source Open and
collision loading. Without selection it stops at SelectionUnavailable before
GetCarModel; with the explicit diagnostic selection it reaches
SceneNamedLookupUnavailable after car cache Open. Both runs dispose all
represented mission owners. Neither is a full Setup, native Close or playable
startup, and this continuation receives no Setup or lifecycle function credit.
Index intake is a one-shot host adapter,
not the native AllDefault/repeated database Init or its short-read behavior.
Separate original-map driving probes stop at quaternion arithmetic or
canonical-cache LinkTo numeric-domain boundaries; they use a different intake
path, not the successful source Open above. The common mission Init
also remains incomplete. Do not clamp source transforms, skip dependencies or
silently load another map to conceal these boundaries. The standalone driving
harness remains a development tool, not Freeride completion.

## Campaign target

Build a standalone Rust engine that runs the original PC campaign from the
user's own assets. The immediate acceptance target is the playable first
mission, from the original menu through completion. **All six gates remain
open.** [Coverage](../COVERAGE.md) records the unchanged accepted-function
ledger and system gates. The campaign is not playable.

| First-mission gate | Required evidence |
| --- | --- |
| Original startup | Menu starts MISE01 with its actors, resources, scripts and update order. |
| Playable control | Original player/taxi movement, camera, collision and damage respond correctly. |
| Mission progression | Objectives, pursuit, events and replay transitions reach completion. |
| Presentation needed for play | Models, animation, materials, text/HUD and required audio support play. |
| Repeatability | Restart and repeated runs match representative original-runtime and interactive sequences. |
| Standalone execution | User assets suffice; original executables and REA remain development inputs. |

## Current foundation and next work

The MISE01 probe loads 3,183 scene objects, 293 actors, 39 scripts, the
13,857-node navigation graph and the 229-node road network, and disposes
all tracked owners to zero. This remains source inspection, not completed
MissionOpen or authored play. Global programs, VM dispatch, player/taxi
coupling, GameInit and mission scheduling remain separate work; recorded
playback still needs its difference pipeline.

1. Reconstruct actual MissionOpen and active Close in native order, including
   navigation from `check.bin`, road loading, cache and sound dependencies.
   Connect the owned navigation graph without skipping earlier unavailable work.
2. Continue reached actor factories and LoadData branches, then original program
   compilation, VM dispatch, initialization and Game scheduling over the retained
   live owners. Preserve executed prefixes, reached errors and cleanup order.
3. Connect the player, taxi, original camera, active HUD/profile, collision and
   damage. Existing controls, 64-bit key mask, camera, definition tables and
   load/save helpers are dependencies, not completed gameplay.
4. Complete play-critical model animation, controller timing/blending/events,
   rotation/scale and animated lit visuals; verify original menu animation and
   original-content material/audio behavior. General GPU fog and environment
   effects remain open.
5. Connect navigation and AI to objectives, pursuit, cutscenes, checkpoints and
   mission completion. Run repeatable menu-to-completion and restart sequences
   against suitable original-runtime evidence, then validate target platforms
   and hardware audio.

PC REA instruction receipts and PS2 symbols supply bounded evidence for names,
scalar behavior and control flow. They do not prove the full native call graph.
[coverage](../COVERAGE.md) · [progress](PROGRESS.md).
