// Adapted from mafia-workbench; see LICENSE and ../PROVENANCE.md.
// Read-only decoder; the upstream editing functions are omitted.
// Edit the actual uncompressed BMP variants used by this installation without
// re-encoding headers, row padding, palette reserved bytes or trailing data.
export function parseBmp(source) {
  const data = Buffer.from(source), view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  if (data.length < 54 || data.toString('ascii', 0, 2) !== 'BM') throw new Error('BMP: neplatná hlavička.');
  if (view.getUint32(14, true) !== 40) throw new Error('BMP: editace podporuje hlavičku BITMAPINFOHEADER (40 bajtů).');
  const width = view.getInt32(18, true), signedHeight = view.getInt32(22, true), height = Math.abs(signedHeight), bits = view.getUint16(28, true), offset = view.getUint32(10, true);
  if (view.getUint16(26, true) !== 1 || view.getUint32(30, true) !== 0 || ![8,24,32].includes(bits)) throw new Error('BMP: editace podporuje nekomprimované 8, 24 a 32bitové obrázky.');
  if (width <= 0 || height <= 0 || width * height > 16777216) throw new Error('BMP: neplatné nebo příliš velké rozměry.');
  const stride = Math.ceil(width * bits / 32) * 4, count = bits === 8 ? view.getUint32(46, true) || 256 : 0;
  if (count > 256 || offset < 54 + count * 4 || offset + stride * height > data.length) throw new Error('BMP: neúplná paleta nebo pixely.');
  const palette = Array.from({length:count}, (_,i) => [data[56+i*4], data[55+i*4], data[54+i*4]]);
  const channels = bits === 8 ? 1 : bits / 8, pixels = new Uint8Array(width * height * channels);
  for (let y=0;y<height;y++) for(let x=0;x<width;x++) {
    const at=offset+(signedHeight<0?y:height-1-y)*stride+x*channels, out=(y*width+x)*channels;
    if (bits===8) { if(data[at]>=count)throw new Error('BMP: pixel odkazuje mimo paletu.');pixels[out]=data[at]; }
    else { pixels[out]=data[at+2];pixels[out+1]=data[at+1];pixels[out+2]=data[at];if(bits===32)pixels[out+3]=data[at+3]; }
  }
  return {width,height,bits,palette,pixels,channels,offset,stride,topDown:signedHeight<0};
}
