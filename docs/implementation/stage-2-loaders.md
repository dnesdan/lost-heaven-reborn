# Stage 2 original-data loaders

Source reference: `mafia-re` commit
`29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`. These are source-only
implementations with synthetic fixtures. Native REA coverage and the playable
mission gate are unchanged; no original-runtime comparison is claimed.

## Effects

`game/mission/C_effects.cpp` constructor/Load/Clear and `C_effects.h` inline
nested loaders provide the tree/default/field-order evidence. `game/C_chunk.h`
and `C_chunk.cpp` provide the packed six-byte inclusive-size headers, bounded
field reads, 999-byte string limit, and open seek-end/rewind prefix.

`effects_table` owns effect/part/subpart trees, raw scalar bit patterns, byte
strings and flags. Unknown child types follow the original parameter branch.
Short field reads retain defaults and their cursor, as `C_chunk::Read` does.
Host admission rejects invalid chunk extents and strings without a terminator
in the bounded buffer; it does not reproduce undefined/infinite-loop malformed
input behavior.

`MissionAssetAllocator` binds a caller-owned allocation domain to actual
`StartupAssetBackend` streams. `MissionComponent::finish_effects_constructor`
consumes the opened table and retains its tree; clear and destruction release
that tree. Stream acquisition uses the provider's existing DTA/loose selection,
seeks to the end, rewinds, reads a bounded snapshot and closes. The parser works
on this snapshot; it does not claim byte-for-byte native seek traces or recreate
C++ vector allocation identities. Native raw pointer words remain separate
from host tree ownership. Dropping an adapter closes its retained unconsumed
stream only; consumed/missing streams cannot trigger a second close.
A production mission allocation owner remains a
caller dependency; the adapter invents none.

## Program patches

`game/mission/C_program.cpp` LoadPatchFile/ReadPatchString/UnloadPatchFiles
provide the first `0x64` chunk, four signed-length nullable byte strings, per-file
record index XOR key (`index % 13 + 0x85`), append order and reverse cleanup.
`ProgramPatchTable` retains complete records, including terminal bytes and null
versus nonnull distinctions. Invalid/negative lengths are explicit host
admission failures. `SystemEffect::LoadPatchFile` reads actual selected streams;
missing files do not add changes. `UnloadPatchFiles` releases records in reverse.
`StartupAssetBackend::program_replacements` supplies the live slice expected by
existing `CProgram`/mission actor loaders; their reverse-priority consumer is
unchanged. A real mission/actor source loader must pass that slice alongside its
actual owner/frame/mission context; context is never inferred from patch data.

## Animation catalog

`game/actors/C_actor.cpp` `_anim_info_` (PC global `0x62e488`) supplies all 576
filename/code records and the final null-resource/`0xffff` sentinel, in original
order and spelling. The resource word is a filename pointer, not an animation
object handle. `animation_catalog` publishes the source catalog and a producer
that requests actual retained filename-storage identities from their owner;
zero identities fail explicitly. Callback filename bytes omit the terminal NUL;
the owner supplies and retains NUL-terminated storage. `init_original_animation_index` connects the
producer to the existing sorted animation index, preserving the original
resource positions used by lookup results.

`StartupAssetBackend` still rejects `InitAnimIndex`/`DoneAnimIndex`: it owns files,
not the required filename-storage identity owner or animation-index allocator.
Their genuine implementations must be supplied before startup can cross that
boundary. No `handle = 1` or fabricated resource identity fills it.

## Validation scope

Synthetic tests exercise nested effects/defaults/overrides/clear/reload and
malformed admission, genuine loose effects streams through the mission
component constructor and destruction, XOR key wrapping/null strings/per-file
index reset/order/unload, startup patch effects with missing files, and the
complete animation catalog through the existing sorted-index consumer. These
are logic and file-owner integration checks; rendered and interactive gameplay
and original-runtime comparison remain unverified.
