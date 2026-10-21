// Generated synthetic model/animation bytes only; no original game data.
const u16 = v => { const b = Buffer.alloc(2); b.writeUInt16LE(v); return b; };
const u32 = v => { const b = Buffer.alloc(4); b.writeUInt32LE(v >>> 0); return b; };
const f32 = v => { const b = Buffer.alloc(v.length * 4); v.forEach((n, i) => b.writeFloatLE(n, i * 4)); return b; };
const text = s => Buffer.concat([Buffer.from([s.length]), Buffer.from(s)]);
const ident = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
const frame = (type, parent, name, p, s = [1, 1, 1], q = [1, 0, 0, 0]) => Buffer.concat([
  Buffer.from(type), u16(parent), f32([...p, ...s, ...q]), Buffer.from([1]), text(name), text('')]);
export function animatedFixture({ badBind = false, badParent = false, overlap = false,
  duplicateNames = false, twoLods = false, skinOnly = false } = {}) {
  const header = Buffer.alloc(14); header.write('4DS\0'); header.writeUInt16LE(29, 4);
  const positions = [[1, 0, 0], [1, 1, 0], [0, 1, 0], [0, 0, 0]];
  const geometry = Buffer.concat([f32([0]), u16(4), f32(positions.flatMap((p, i) => [...p, 0, 0, 1, i % 2, Math.floor(i / 2)])),
    Buffer.from([1]), u16(2), ...[0, 1, 2, 0, 2, 3].map(u16), u16(1)]);
  const inverseA = [...ident]; inverseA[0] = inverseA[5] = inverseA[10] = .5; inverseA[12] = badBind ? 4 : -.5;
  const inverseB = [0, .5, 0, 0, -.5, 0, 0, 0, 0, 0, .5, 0, 1, -.5, 0, 1];
  const skin = Buffer.concat([Buffer.from([2]), u32(1), Buffer.alloc(24),
    f32(inverseA), u32(1), u32(1), u32(badParent ? 3 : 0), Buffer.alloc(24), f32([.25]),
    f32(inverseB), u32(1), u32(0), u32(1), Buffer.alloc(24)]);
  const regions = [[0, 1], [overlap ? 1 : 2]];
  const morph = Buffer.concat([Buffer.from([2, 2, 1]), ...regions.flatMap((indices, region) => [
    u16(indices.length), f32(indices.flatMap(index => [0, 1].flatMap(target => [
      positions[index][0] + target * .25 * (region + 1), positions[index][1], positions[index][2], 0, 0, 1]))),
    Buffer.from([1]), ...indices.map(u16)]), Buffer.alloc(40)]);
  const owner = Buffer.concat([frame([1, skinOnly ? 2 : 3, 0, 0], 0, 'owner', [2, 3, 4], [1, 1, 1], [Math.sqrt(.75), 0, .5, 0]),
    u16(0), Buffer.from([twoLods ? 2 : 1]), geometry, ...(twoLods ? [geometry] : []),
    skin, ...(twoLods ? [skin] : []), ...(skinOnly ? [] : [morph])]);
  const opaque = [...ident]; opaque[0] = 99;
  return Buffer.concat([header, u16(1), u32(0x40000), f32([0, 0, 0, .5, .5, .5, 0, 0, 0, 1]), text(''),
    u16(3), owner,
    frame([10], 1, 'jointA', [1, 0, 0], [2, 2, 2]), f32(opaque), u32(0),
    frame([10], 2, duplicateNames ? 'jointA' : 'jointB', [0, 1, 0], [1, 1, 1], [Math.SQRT1_2, 0, 0, Math.SQRT1_2]), f32(ident), u32(1),
    Buffer.from([1])]);
}

export function clipFixture({ name = 'owner', noteFlags = 16, duplicateTimes = false,
  empty = false, degenerate = false, zeroDuration = false } = {}) {
  const nodes = [{ name, flags: 6, tracks: [
    { times: empty ? [] : [0, duplicateTimes || zeroDuration ? 0 : 4], values: degenerate ? [[0, 0, 0, 0], [0, 0, 0, 0]] : [[1, 0, 0, 0], [Math.SQRT1_2, 0, Math.SQRT1_2, 0]] },
    { times: [0, zeroDuration ? 0 : 4], values: [[2, 3, 4], [3, 4, 5]] }] },
  { name:'jointA', flags:4, tracks:[{ times:[0], values:[[1, 0, 0, 0]] }] },
  { name:'jointB', flags:noteFlags, tracks:[{ times:[0, zeroDuration ? 0 : 3], values:[7, 9], note:true }] }];
  const header = Buffer.alloc(22); header.write('5DS\0'); header.writeUInt16LE(20, 4);
  header.writeUInt16LE(nodes.length, 18); header.writeUInt16LE(zeroDuration ? 0 : 4, 20);
  const table = Buffer.alloc(nodes.length * 8), chunks = [header, table]; let offset = 22 + table.length;
  nodes.forEach((node, i) => {
    const nameBytes = Buffer.from(node.name + '\0'); table.writeUInt32LE(offset - 18, i * 8);
    chunks.push(nameBytes); offset += nameBytes.length; table.writeUInt32LE(offset - 18, i * 8 + 4);
    const data = Buffer.concat([u32(node.flags), ...node.tracks.flatMap(track => [
      u16(track.times.length), ...track.times.map(u16),
      ...(!track.note && track.times.length % 2 === 0 ? [Buffer.alloc(2)] : []),
      ...(track.note ? track.values.map(u16) : [f32(track.times.length ? track.values.flat() : [])]),
      ...(track.note ? [Buffer.alloc(2)] : [])])]);
    chunks.push(data); offset += data.length;
  });
  const bytes = Buffer.concat(chunks); bytes.writeUInt32LE(bytes.length - 18, 14); return bytes;
}

export function glbParts(bytes) {
  const length = bytes.readUInt32LE(12);
  return { json: JSON.parse(bytes.toString('utf8', 20, 20 + length)), binary: bytes.subarray(28 + length) };
}
export function accessor(parts, index) {
  const a = parts.json.accessors[index], v = parts.json.bufferViews[a.bufferView];
  const size = {SCALAR:1,VEC2:2,VEC3:3,VEC4:4,MAT4:16}[a.type], width = a.componentType === 5123 ? 2 : 4;
  return Array.from({length:a.count * size}, (_,i) => parts.binary[a.componentType === 5123 ? 'readUInt16LE' : 'readFloatLE']((v.byteOffset ?? 0) + (a.byteOffset ?? 0) + i * width));
}
