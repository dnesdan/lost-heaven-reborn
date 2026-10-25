# Stage 7b: archive WAV general-compression fallback

Reference: `Marvisak/mafia-re` pinned to
`29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`,
`rw_data/rw_data.cpp::WavDecompression` (lines 1678–1819) and
`Decompression` immediately following it. This is source-only audit work,
without new reconstruction, function, progress or native-validation credit.

After optionally copying the 44-byte WAV header, WavDecompression tests integer
`block_align / channel_count`. A quotient of two selects its existing PCM16
predictor path. Every other quotient calls the ordinary Decompression on the
remaining bytes. The inner selector is raw only when zero; every nonzero inner
selector selects the existing LZ decoder. This differs from outer archive block
dispatch, where only selector one selects Decompression and other non-WAV tags
are raw. The two dispatch contracts are now explicit and share one bounded
Decompression implementation. No additional audio codec was introduced.

WAV header matching still compares only the three saved identifiers, preserving
all header bytes and requiring no guessed PCM format-tag or sample-rate gate.
Zero-channel division and truncated native scratch reads remain excluded.
PCM16 predictor support remains bounded to mono/stereo. Output remains bounded
to the existing 32768-byte decoded block domain. On malformed LZ or insufficient
output capacity, copied header bytes and completed decoder output remain
observable; numeric stream cursor/cache state retains its existing commit rules.

Synthetic tests cover nested raw and LZ streams, non-one/nonzero inner tags,
continuation blocks without a duplicate header, copied-header and partial-output
error prefixes, output limits, and real plain/encrypted archive files. The real
stream tests cover seek/read, admitted catalog selection, and a corrupted decoded
size alongside an existing loose file: selected archive corruption stays an
error and never falls back to another provider. All file writes in the tests
create synthetic temporary fixtures.

This change does not add write-mode handles, a new source-search policy, native
byte-name support, or original-runtime comparisons. The checked Installation
path remains read-only. Write-mode rw_data lifecycle is a separate change.
