# Owned mission navigation dependency

The Rust `MissionComponent` web-path resource now owns populated node and
connection arrays. `load_web_nodes` requires an explicit live `WebPathChunk`
and `WebPathAllocator`; it is not connected to the production `check.bin`
loader. This establishes an owned navigation dependency, without claiming
pathfinding, AI, `C_game::Init`, or a running first mission.

## Evidence and behavior

The input is PC `Game.exe`, SHA-256
`08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`.
REA/Ghidra instruction evidence covers `LoadNodes` RVA `0x14a8c0`, path resize
`0x14a220`, node resize `0x149fc0`, constructors `0x14d2c0` / `0x14d2b0`,
destructors `0x149f90` / `0x14a1e0`, and the array lifetime helpers. The four
indirect callback entries missing from default analysis were seeded from
observed call arguments in a separate recorded analysis profile. The input
remained unchanged; no original executable was run. Class names follow
pinned mafia-re `eb5a9216eea23210e96a3148e351e8211b27e240`'s attributed PS2
correspondence. They are not newly verified PS2 symbols.

The owner preserves the native ordered effects:

- Old node arrays and their connection arrays are destroyed before the
  signature read. Array destruction uses reverse cookie counts, including
  capacity nodes beyond the active count.
- Path capacity grows by its stored quantum. Node resize compares an unsigned
  16-bit request with byte capacity, then uses the request's low byte for the
  new capacity and allocation. Capacity changes precede allocation.
- Node construction initializes its packed 30 bytes. Connection construction
  initializes its two 16-bit words and preserves the distance's allocation
  seed. Resizing deep-copies active connections before disposing old storage.
- The chunk adapter reads the actual saved chunk-end cell after `dtaSeek`.
  Skipped reads preserve destinations; short reads preserve the actual written
  prefix. The loader does not eagerly validate later records.
- Serialized connection pointers are discarded before per-node allocation.
  Replacement allocations stay owned if a later copy or free is interrupted.
  Interrupted operations cannot be replayed automatically.

## Explicit limits

The native signature local has no initializer. A skipped or short signature
read leaves unknown bytes, so Rust stops at their first use with only the
initialized prefix. It does not invent a stack seed or a signature result.
The count local does have a native zero initializer; its partial reads retain
that seed. Null destinations and malformed extents stop when their actual
access is reached, after preceding effects. Zero growth, arithmetic-wrap
continuations, and the CRT constructor helper's signed-count edge remain
explicit unsupported boundaries. A reached cookie write and its allocation
stay owned when constructor admission stops. These unresolved branches receive
no original-function credit.

The dynamic allocator must supply real seeded storage, identities, and the
same allocator domain as the mission component. The chunk adapter must bind
the real stream and stack backing. Synthetic adapters exercise these
contracts; a production DTA/check.bin binding and original-runtime comparison
remain separate work.

Focused synthetic tests cover populated load and disposal, failed signature
reload, live chunk-bound mutation, skipped and partial reads, allocation
failure and null return, preserved storage after a callback error, unsigned
request truncation, deep-copy capacities, and interrupted reverse disposal.
