# Stage 7c: rw_data write lifecycle and explicit host output

Reference: local `Marvisak/mafia-re` pinned to
`29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`, `rw_data/rw_data.h`
and `rw_data/rw_data.cpp`: dtaOpenWrite (1313–1450), dtaWrite (1456–1510),
dtaSeek (1166–1261) and dtaClose (565–636). This is authorized source-only work;
no reconstruction, function, progress or original-runtime credit is added.

## Source contracts

`Handles::dta_open_write` uses the same handle table and existing dtaSeek/
dtaClose implementation as read-mode native lifecycle. It implements
CREATE_ALWAYS versus OPEN_ALWAYS, normal/hidden attribute selection, buffered
`.tmp` copy or delete, optional readonly-attribute retry, temporary-name and
32768-byte block allocation, and first-free-slot reuse. Native filesystem and
allocation failures preserve their executed prefix. No invented rollback is
performed after a successful create/truncate followed by allocation failure.

`Handles::dta_write` performs one backend write attempt. Successful writes
advance the OS cursor, but do not update the record's logical file_pointer or
size: dtaOpenWrite initializes size to zero, and dtaWrite does not change it.
A null buffer, failed write or short write sets the buffered discard bit.
Later successful writes do not clear that bit. The existing write-mode seek
path retains BEGIN-to-CURRENT-to-END fallthrough and OS-result commit order.
Host backend interruptions keep completed output visible and prevent replay.

The existing native dtaClose closes the handle first. A failed buffered write
causes temporary-file deletion; otherwise it attempts destination deletion,
readonly clearing/retry where needed, then a move. As in the source, deletion
and move boolean failures do not change a successful close return. Close is
therefore not an atomic-publication guarantee. Allocation frees and record
invalidation retain the existing native order and failure-prefix rules.

## Host adapter and boundaries

`rw_data::write_io::NativeWriteFiles` explicitly opts into actual output writes
under a caller-selected, existing output directory. It is separate from
Installation and `rw_data::io`, which stay read-only. It owns actual write-only
files, name allocations and block allocations, and shares core write/seek/close
logic. Stream drop attempts the same close path; failed-open resource prefixes
remain owned until adapter disposal, without pretending the original file was
restored. Host disposal releases OS handles and memory but does not roll back
filesystem effects or promote an unadmitted failed-open temporary file.

The adapter retains the existing ASCII/canonical relative-path, case-collision,
symlink, containment and snapshot policies. Only the final output component may
be newly created; parent directories must already exist. Concurrent opens of
the same output through this adapter are rejected. Own writes refresh the file
snapshot; external replacement or mutation prevents subsequent writes.
Input admission rejects nonregular and over-quota destinations before buffered
copy so a host exclusion cannot turn into an ignored copy failure and empty
replacement. Tests write synthetic temporary fixtures only.

The host adapter admits create, buffer and change-readonly flags. Win32 hidden
attributes (`DTA_TEMPORARY`) and unknown flags are rejected before mutation;
the generic native effect interface can represent the hidden flag. Readonly
mapping preserves Unix permission bits and adds owner-write only when clearing
a genuinely readonly file. Win32 cross-process share modes, hidden attributes,
crash durability and filesystem-race parity are not claimed. The adapter's
per-file extent limit is at most 64 MiB; its allocation budget charges owned
name/block byte lengths, not total process RSS or allocator capacity. It does
not add archive mutation, game-save serialization or native byte-name support.

## Synthetic checks

Tests exercise real OPEN_ALWAYS overwrite versus truncate, OS/logical cursor
divergence, seek fallthrough, slot reuse, buffered copy/create/promotion/discard,
null and quota failures, scope/case/symlink guards, external mutation, readonly
retry, allocation-failure filesystem prefixes and actual last-close cleanup.
Effect-level fault tests cover short and failed writes with output prefixes,
ignored close results and ordering, and a backend interruption that must never
be replayed. Build, rendered, interactive and original-runtime verification
remain separate claims; no private game files are included.
