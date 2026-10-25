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
from host tree ownership. A production mission allocation owner remains a
caller dependency; the adapter invents none.
