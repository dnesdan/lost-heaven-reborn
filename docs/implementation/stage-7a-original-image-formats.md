# Stage 7a: original image source formats

Reference: local `Marvisak/mafia-re` checkout pinned to
`29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`. This uses the authorized
source-only audit route. It changes no reconstruction or native validation
credit and makes no original-runtime parity claim.

## Source contracts and implementation

`ls3df/IGraph/ITexture.cpp::LoadImage` (lines 1434–1625) rejects nonzero BMP
compression, computes four-byte aligned BMP source rows, and reads indexed8
palettes as BGR quads. `ITexture::Create` (lines 500–658) converts source depths
8, 16, 24 and 32 to its pixel buffer. Its 16-bit branch uses fixed X1R5G5B5
masks and shifts: RGB channels expand to 0–248, without low-bit replication;
the high bit supplies alpha only when requested by the texture creation flags.
The 32-bit branch reads BGRA bytes and likewise applies source alpha only when
requested.

The shared `decode_bmp_original` now decodes BI_RGB 16-bit X1R5G5B5 and 32-bit
BGRA as well as its existing indexed8 and BGR24 sources. It retains the source
alpha bits in RGBA; the existing texture factory controls whether to use them.
The generic `decode_bmp` keeps its existing 8/24-bit policy. A shared private
1555 conversion also serves the existing 16-bit TGA decoder. BMP compression,
including BI_BITFIELDS/565, remains unsupported because the reference rejects
it. No externally inferred color masks or header variants were added.

The same `LoadImage` reads a TGA image ID before its palette and, for 8-bit
sources, always reads 256 BGR triples. It does not use the TGA color-map origin,
length or entry-size fields to change those reads. `decode_tga` therefore adds
only uncompressed indexed8 images whose declared palette agrees with that
contract: origin zero, 256 entries, 24-bit BGR. It retains the existing
true-color and optional Truevision-footer support. Short/sparse palettes,
16/32-bit palette entries, RLE, grayscale and interleaved images remain
explicitly unsupported.

`texture_source` admits these same source layouts and retains original BPP,
source row pitch and indexed palette-entry-zero RGB. The existing shared
`texture_factory` consumes this metadata for requested-versus-effective alpha
flags, palette color key, optional alpha-image brightness and retained pixel
storage; no image conversion is duplicated in engine consumers.

The reusable decoders return top-down RGBA. Their existing generic orientation
support is preserved, including negative-height BMP and right-origin TGA.
The original-source adapter still rejects those two layouts: the native TGA
loader diagnoses right-origin without reversing it, and the native BMP path
does not establish safe negative-height semantics. No native equivalence is
claimed for those reusable-decoder extensions.

## Verification boundaries

Synthetic fixtures cover 16/32-bit BMP row order, padding, source alpha and
extents; indexed TGA palette entries 0/255, image IDs, row origin, footer and
truncations; unsupported compression, masks and palette layouts; and the real
CPU texture factory's alpha flags, color key, optional alpha image, retained
ARGB bytes and last-release ownership. Existing image quotas and malformed
input guards remain in force. No private or original game assets are included.

Build and test results are recorded in the change review. Rendered, interactive
and original-runtime comparison are separate gates and are not asserted by
these source-format or CPU integration tests.
