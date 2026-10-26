# Stage 5a: persistent presentation resources

Reference: `Marvisak/mafia-re` at
`29b65b0e5bcf77f1f36c4fceeaa2b64eb644087c`,
`ls3df/IGraph/IGraph.cpp:578-592` (persistent texture sharing).

The presentation renderer previously deduplicated textures only within one
upload call and recreated the depth attachment on every render. It now shares
GPU texture/bind-group allocations across draw replacements while the immutable
CPU texture identity remains alive. Weak source ownership prevents both pointer
reuse collisions and retention of an otherwise dead CPU texture. Unchanged
geometry outside the replaced range keeps its buffers.

The cache is local to one device, sampler policy and renderer. A pixel update
invalidates the immutable-source cache entry, while existing shared draws retain
their updated GPU allocation. A future upload of the old CPU source receives its
original pixels. Rejected replacements are validated before cache mutation.
Depth attachments are reused until the render extent changes. Replacing draws
does not synchronously wait for the device; wgpu retains in-flight resources.

## Validation

- `cargo test -p lhr-render --locked -- --include-ignored`: six tests passed on
  Linux software Vulkan (Mesa llvmpipe), including the explicit GPU test.
- The GPU regression checks cross-call sharing, invalid replacement atomicity,
  mutable-upload invalidation, depth reuse/resize and expired-source eviction.
- Run `scripts/test.sh` for the complete workspace gate before publication.

This is host presentation resource management informed by the reference. It
does not claim original-runtime function parity, material/lighting parity,
measured FPS gains, a rendered screenshot comparison or interactive validation.
Original texture-manager keys, mesh dirty-range updates, mipmaps and the wider
scene-to-render integration remain separate work.
