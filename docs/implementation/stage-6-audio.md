# Stage 6: sound playback lifecycle

## Evidence and scope

This implements source-supported I3D sound contracts from the pinned
`mafia-re` revision `29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`, using
`ls3df/I3D/frames/I3d_sound.cpp` (`Open`, `IsPlaying`, `SetLoop`,
`SetCurrTime`, `SetOn`, `m_Pause`, `m_UpdateSound`) and the sound declarations
in `ls3df/I3D/I3D.h`. The upstream STOP/PLAY/PAUSE/RESUME/NOCAMERA labels
are descriptive guesses; their numeric states are retained. This is static
source correspondence and Rust implementation evidence. It adds no binary
function coverage or original-runtime validation credit.

## Connected behavior

- `RuntimeAudio::sound_virtual` distinguishes logical `IsPlaying(false)` from
  the physical PCM voice queried by `IsPlaying(true)`. One-argument `SetOn`
  requests playback at the update boundary; its two-argument form can update
  immediately. Explicit stop stops the voice; a spatial pause freezes its
  cursor while the logical lifetime continues.
- Range, sector and silent outer-cone gates pause a voice. Re-entry resumes
  from elapsed time; a non-looping sound expires while paused. Loop changes
  reapply `SetOn` when the frame is enabled, and paused seeks update the
  logical pause position without moving the stopped voice cursor.
- An absent camera is distinct from a camera without a sector. The live
  chase-camera listener uses the same `ModelArena::scene_get_sector` query as
  the canonical camera update, at the drawn camera's PC-coordinate position,
  retaining its previous sector hint. No vehicle-parent membership is assumed.
- Both canonical sound-frame synchronization paths copy the graph's existing
  `world_scale` after its world matrix has been resolved. Range attenuation
  and range gates use scaled distances. `ForceUpdate` also refreshes that
  frame input. Scaled ranges retain constructor/cache values until an eligible
  update refreshes dirty range/scale fields; half-cone cosine reads follow the
  current angles.
- `Open` preserves an existing voice only for an exact filename match. A
  different filename releases the previous source and voice before lookup or
  file decoding, including when replacement fails. Shared-source lookup is
  case-insensitive; a cache match still creates a fresh host voice. The frame
  adapter releases unused source/name projections on failure as well as success.

## Host boundaries

The PCM render duration drives the logical millisecond clock so captures and
CPAL output share deterministic state. This does not establish parity with
`timeGetTime`, device scheduling, or DirectSound cursor and block-alignment
behavior. Host seeks address PCM time directly; DirectSound's previous-block
seek rounding is not implemented here. The mixer is not a DirectSound backend.

Area/sector fades, reverb, driver master-volume propagation, hardware channel
priority/stealing, DirectSound spatialization and Doppler remain outside this
change. Camera absence is exposed by `clear_listener`; removal of an already
playing camera is not claimed to mute its existing voice, matching the source's
update guard. The live viewer still uses a host chase camera, not the original
game camera controller.

## Verification

Synthetic PCM tests cover replacement failure and retry, exact versus
case-insensitive opens, deferred/immediate play, real/logical queries, stop,
loop restart, paused seek and expiry, range pause/resume, absent camera, missing
and matching sectors, scaled range cache/update guards, zero and very small
cone vectors, cone-derived fields, and vehicle audio callers. Bridge tests cover inherited scale after a real parent update,
camera-position sector selection, primary fallback, and preservation of the
last sector hint on invalid listener input.

Targeted verification: `cargo test -p lhr-engine --lib audio` passes 19 tests
with a dedicated Stage 6 Cargo target directory. Full workspace verification
and two sequential independent reviews are separate integration gates. No
rendered, interactive, audible-device, or original-runtime comparison is
claimed by these logic tests.
