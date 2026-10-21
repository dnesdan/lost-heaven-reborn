// Original project code, MIT. Lossless opaque BMP -> PNG transport for glTF.
import { deflateSync } from 'node:zlib';
import { parseBmp } from './vendor/bmp.mjs';

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const name = Buffer.from(type), result = Buffer.alloc(12 + data.length);
  result.writeUInt32BE(data.length); name.copy(result, 4); data.copy(result, 8);
  result.writeUInt32BE(crc32(Buffer.concat([name, data])), 8 + data.length);
  return result;
}
export function bmpToPng(source) {
  const bitmap = parseBmp(source);
  // The first milestone supports opaque materials only. Reserved BMP alpha bytes
  // must not turn an opaque source material into a transparent glTF material.
  const stride = bitmap.width * 3 + 1, scanlines = Buffer.alloc(stride * bitmap.height);
  for (let y = 0; y < bitmap.height; y++) {
    for (let x = 0; x < bitmap.width; x++) {
      const index = y * bitmap.width + x;
      const rgb = bitmap.bits === 8
        ? bitmap.palette[bitmap.pixels[index]]
        : bitmap.pixels.subarray(index * bitmap.channels, index * bitmap.channels + 3);
      scanlines.set(rgb, y * stride + 1 + x * 3);
    }
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(bitmap.width); header.writeUInt32BE(bitmap.height, 4);
  header[8] = 8; header[9] = 2;
  return Buffer.concat([
    Buffer.from('89504e470d0a1a0a', 'hex'), chunk('IHDR', header),
    chunk('IDAT', deflateSync(scanlines)), chunk('IEND', Buffer.alloc(0)),
  ]);
}
export function pngDimensions(bytes) {
  if (bytes.length < 33 || bytes.subarray(0, 8).toString('hex') !== '89504e470d0a1a0a'
      || bytes.readUInt32BE(8) !== 13 || bytes.toString('ascii', 12, 16) !== 'IHDR') {
    throw new Error('Texture must be a supported BMP or PNG image.');
  }
  const width = bytes.readUInt32BE(16), height = bytes.readUInt32BE(20);
  if (!width || !height || width * height > 16777216) throw new Error('Texture dimensions exceed the supported limit.');
  return { width, height };
}
