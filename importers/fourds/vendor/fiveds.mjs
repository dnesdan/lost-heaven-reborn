// Adapted read-only from MIT mafia-workbench 9f1edbf; see LICENSE.
const sourceTextDecoder = new TextDecoder('windows-1250');
const HEADER = 18;
export function parseFiveDs(source, layout = false) {
  const bytes = Buffer.from(source), entries = [], nodes = [];
  function bounds(offset, size) { if (!Number.isSafeInteger(offset) || offset < 0 || size < 0 || offset + size > bytes.length) throw new Error('5DS: data překračují soubor.'); }
  bounds(0, 22);
  if (bytes.toString('ascii', 0, 4) !== '5DS\0' || bytes.readUInt16LE(4) !== 20) throw new Error('5DS: nepodporovaná hlavička nebo verze.');
  if (bytes.readUInt32LE(14) !== bytes.length - HEADER) throw new Error('5DS: neplatná deklarovaná délka.');
  const count = bytes.readUInt16LE(18), frames = bytes.readUInt16LE(20), tableEnd = 22 + count * 8;
  bounds(22, count * 8);
  for (let i = 0; i < count; i++) {
    const nameOffset = bytes.readUInt32LE(22 + i * 8) + HEADER, dataOffset = bytes.readUInt32LE(26 + i * 8) + HEADER;
    if (nameOffset < tableEnd || dataOffset < tableEnd) throw new Error('5DS: offset zasahuje hlavičku.');
    bounds(nameOffset, 1); bounds(dataOffset, 4);
    const end = bytes.indexOf(0, nameOffset); if (end < 0) throw new Error('5DS: chybí konec názvu.');
    const name = sourceTextDecoder.decode(bytes.subarray(nameOffset, end)), flags = bytes.readUInt32LE(dataOffset);
    const node = { id: i, name, flags, tracks: {}, unknownFlags: (flags & ~0x1e) >>> 0 }; let cursor = dataOffset + 4;
    entries.push({ id: `nodes/${i}/name`, label: 'Název uzlu', kind: 'string', value: name, context: name, offset: nameOffset, length: end - nameOffset + 1, pointer: 22 + i * 8 });
    for (const [channel, bit, components] of [['rotation', 4, 4], ['position', 2, 3], ['scale', 8, 3], ['note', 16, 1]]) {
      if (!(flags & bit)) continue;
      const start=cursor;bounds(cursor, 2); const length = bytes.readUInt16LE(cursor); cursor += 2; bounds(cursor, length * 2);const timeOffset=cursor;
      const times = Array.from({ length }, (_, k) => bytes.readUInt16LE(cursor + k * 2)); cursor += length * 2;
      // The header stores the final frame number, not the number of samples (0..frames inclusive).
      if (times.some((time, k) => time > frames || (k && time < times[k - 1]))) throw new Error('5DS: neplatné pořadí snímků.');
      if (channel !== 'note' && length % 2 === 0) { bounds(cursor, 2); cursor += 2; }
      const stride = channel === 'note' ? 2 : components * 4; bounds(cursor, length * stride);const valueOffset=cursor;
      const values = [];
      for (let k = 0; k < length; k++) {
        const offset = cursor + k * stride, value = channel === 'note' ? bytes.readUInt16LE(offset) : Array.from({ length: components }, (_, c) => bytes.readFloatLE(offset + c * 4));
        if (Array.isArray(value) && value.some(v => !Number.isFinite(v))) throw new Error('5DS: neplatné číslo v klíči.');
        values.push(value); entries.push({ id: `nodes/${i}/${channel}/${k}`, label: `${channel} · snímek ${times[k]}`, kind: channel === 'note' ? 'uint' : 'vector', value, context: name, offset, length: stride });
      }
      cursor += length * stride;
      if (channel === 'note') { bounds(cursor, 2); cursor += 2; }
      node.tracks[channel] = { times, values, ...(layout?{layout:{start,end:cursor,timeOffset,valueOffset,stride}}:{}) };
    }
    if(layout)node.layout={start:dataOffset,end:cursor,pointer:26+i*8};nodes.push(node);
  }
  return { version: 20, frames, nodes, entries };
}
