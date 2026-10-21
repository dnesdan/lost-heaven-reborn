// All model/bitmap bytes below are generated synthetic fixtures, never game data.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inflateSync } from 'node:zlib';
import { convertStaticModel, safeTextureName } from '../importers/fourds/convert.mjs';
import { bmpToPng } from '../importers/fourds/png.mjs';

const repository = fileURLToPath(new URL('..', import.meta.url));
const u16 = value => { const b = Buffer.alloc(2); b.writeUInt16LE(value); return b; };
const u32 = value => { const b = Buffer.alloc(4); b.writeUInt32LE(value); return b; };
const floats = values => { const b = Buffer.alloc(values.length * 4); values.forEach((v, i) => b.writeFloatLE(v, i * 4)); return b; };
const text = value => Buffer.concat([Buffer.from([value.length]), Buffer.from(value, 'latin1')]);
const identity = [1, 0, 0, 0];
function frameHeader(type, parent, name, position, rotation, scale = [1, 1, 1], culling = 1) {
  return Buffer.concat([Buffer.from(type), u16(parent), floats([...position, ...scale, ...rotation]), Buffer.from([culling]), text(name), text('')]);
}
function fixture({ texture = '', flags = 0x40000, visual = 0, parent = 1,
  rotation = identity, scale = [1, 1, 1], material = 1, invalid = false, animated = 0,
  version = 29, instanceCycle = false } = {}) {
  const header = Buffer.alloc(14); header.write('4DS\0'); header.writeUInt16LE(version, 4);
  const sourceMaterial = Buffer.concat([u32(flags), floats([0, 0, 0, .3, .5, .7, 0, 0, 0, 1]),
    ...(flags & 0x80000 ? [floats([.5]), text('')] : []), text(texture),
    ...(flags & 0x04000000 ? [Buffer.alloc(18)] : [])]);
  const root = Buffer.concat([frameHeader([6], 0, 'pivot', [2, 3, 4], [Math.SQRT1_2, 0, Math.SQRT1_2, 0]), Buffer.alloc(24)]);
  const vertices = [0, 0, 1, 0, 0, 1, 0, 0, 2, 0, 1, 0, 0, 1, 1, 0, 0, 3, 1, 0, 0, invalid ? NaN : 1, 0, 1];
  const shape = Buffer.concat([frameHeader([1, visual, 0, 0], parent, 'triangle', [1, 2, 3], rotation, scale),
    ...(instanceCycle ? [u16(2)] : [u16(0), Buffer.from([1]), floats([0]), u16(3), floats(vertices), Buffer.from([1]), u16(1), u16(0), u16(1), u16(2), u16(material)]),
    ...(visual === 2 ? [Buffer.from([0]), u32(3), Buffer.alloc(24)] : [])]);
  return Buffer.concat([header, u16(1), sourceMaterial, u16(2), root, shape, Buffer.from([animated])]);
}
function glbParts(bytes) {
  assert.equal(bytes.toString('ascii', 0, 4), 'glTF'); assert.equal(bytes.readUInt32LE(4), 2);
  assert.equal(bytes.readUInt32LE(8), bytes.length);
  const length = bytes.readUInt32LE(12), json = JSON.parse(bytes.toString('utf8', 20, 20 + length));
  const start = 28 + length;
  return { json, binary: bytes.subarray(start) };
}
function readAccessor(parts, index) {
  const a = parts.json.accessors[index], view = parts.json.bufferViews[a.bufferView];
  const count = a.count * ({ VEC2: 2, VEC3: 3, SCALAR: 1 }[a.type]);
  const offset = view.byteOffset + (a.byteOffset ?? 0);
  return Array.from({ length: count }, (_, i) => a.componentType === 5126
    ? parts.binary.readFloatLE(offset + i * 4) : parts.binary.readUInt16LE(offset + i * 2));
}
function bitmap(bits = 24, topDown = false) {
  const width = 2, height = 2, stride = Math.ceil(width * bits / 32) * 4;
  const paletteBytes = bits === 8 ? 8 : 0, offset = 54 + paletteBytes;
  const bytes = Buffer.alloc(offset + stride * height);
  bytes.write('BM'); bytes.writeUInt32LE(bytes.length, 2); bytes.writeUInt32LE(offset, 10);
  bytes.writeUInt32LE(40, 14); bytes.writeInt32LE(width, 18); bytes.writeInt32LE(topDown ? -height : height, 22);
  bytes.writeUInt16LE(1, 26); bytes.writeUInt16LE(bits, 28);
  if (bits === 8) { bytes.writeUInt32LE(2, 46); bytes.set([0, 0, 255, 0, 0, 255, 0, 0], 54); }
  const colors = [[255, 0, 0], [0, 255, 0], [0, 255, 0], [255, 0, 0]];
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const at = offset + (topDown ? y : height - 1 - y) * stride + x * bits / 8;
    if (bits === 8) bytes[at] = colors[y * width + x][0] ? 0 : 1;
    else { const [r, g, b] = colors[y * width + x]; bytes.set([b, g, r], at); if (bits === 32) bytes[at + 3] = 0; }
  }
  return bytes;
}
function pngScanlines(png) {
  let cursor = 8; const payload = [];
  while (cursor < png.length) {
    const length = png.readUInt32BE(cursor), type = png.toString('ascii', cursor + 4, cursor + 8);
    if (type === 'IDAT') payload.push(png.subarray(cursor + 8, cursor + 8 + length));
    cursor += length + 12;
  }
  return inflateSync(Buffer.concat(payload));
}
function temporary(t) {
  const root = fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()), 'lhr-synthetic-model-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true })); return root;
}
function cli(args) {
  return spawnSync(process.execPath, [path.join(repository, 'importers/fourds/cli.mjs'), ...args], { encoding: 'utf8', timeout: 15000 });
}

test('GLB reflects vertices/normals/winding and preserves parent hierarchy, UVs, transforms and hash', () => {
  const source = fixture({ rotation: [Math.SQRT1_2, Math.SQRT1_2, 0, 0] });
  const original = Buffer.from(source), { glb, metadata } = convertStaticModel(source), parts = glbParts(glb);
  assert.deepEqual(source, original); assert.equal(parts.json.nodes.length, 2);
  assert.deepEqual(parts.json.scenes[0].nodes, [0]); assert.deepEqual(parts.json.nodes[0].children, [1]);
  assert.deepEqual(parts.json.nodes[1].translation, [1, 2, -3]);
  const q = parts.json.nodes[1].rotation; assert.ok(Math.abs(q[0] - Math.SQRT1_2) < 1e-7); assert.ok(Math.abs(q[3] - Math.SQRT1_2) < 1e-7);
  assert.ok(Math.abs(parts.json.nodes[0].rotation[1] - Math.SQRT1_2) < 1e-7);
  const primitive = parts.json.meshes[0].primitives[0];
  assert.deepEqual(readAccessor(parts, primitive.attributes.POSITION), [0, 0, -1, 2, 0, -1, 0, 3, -1]);
  assert.deepEqual(readAccessor(parts, primitive.attributes.NORMAL), [0, 0, -1, 0, 0, -1, 0, 0, -1]);
  assert.deepEqual(readAccessor(parts, primitive.attributes.TEXCOORD_0), [0, 0, 1, 0, 0, 1]);
  assert.deepEqual(readAccessor(parts, primitive.indices), [0, 2, 1]);
  assert.equal(metadata.triangleCount, 1); assert.equal(metadata.unitScale, 1);
  assert.match(metadata.unitStatus, /unverified/); assert.match(metadata.scope, /no animation, skeleton/);
  assert.equal(metadata.sourceSha256, crypto.createHash('sha256').update(source).digest('hex'));
  assert.equal(metadata.glbSha256, crypto.createHash('sha256').update(glb).digest('hex'));
});

test('opaque source BMP variants preserve top-left pixel orientation, row padding and palette RGB', () => {
  const expected = [0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 255, 0, 0];
  for (const bits of [8, 24, 32]) for (const topDown of [false, true]) {
    const source = bitmap(bits, topDown), copy = Buffer.from(source);
    assert.deepEqual([...pngScanlines(bmpToPng(source))], expected);
    assert.deepEqual(source, copy);
  }
  assert.throws(() => bmpToPng(bitmap().subarray(0, -1)), /BMP/);
});

test('texture embeds within BIN, names stay local and unsupported effects fail explicitly', () => {
  const { glb, metadata } = convertStaticModel(fixture({ texture: 'checker.bmp', flags: 0x40000 | 0x10000 }), { resolveTexture: name => { assert.equal(name, 'checker.bmp'); return bitmap(); } });
  const { json, binary } = glbParts(glb), image = json.images[0], view = json.bufferViews[image.bufferView];
  assert.equal(image.mimeType, 'image/png'); assert.equal(image.uri, undefined);
  assert.equal(binary.subarray(view.byteOffset, view.byteOffset + 8).toString('hex'), '89504e470d0a1a0a');
  assert.equal(json.samplers[0].wrapS, 33071); assert.equal(json.samplers[0].wrapT, 10497);
  assert.equal(metadata.textures[0].width, 2); assert.equal(metadata.textures[0].status, 'embedded');
  for (const name of ['../private.bmp', '/private.bmp', 'sub\\private.bmp', 'C:private.bmp', 'private.bmp ', 'remote.png']) assert.throws(() => safeTextureName(name));
  for (const flags of [0x8000, 0x80000, 0x04000000, 0x20000000, 0x80000000]) assert.throws(() => convertStaticModel(fixture({ flags })), /unsupported/);
  assert.throws(() => convertStaticModel(fixture({ texture: 'checker.bmp' })), /missing/);
  const missing = convertStaticModel(fixture({ texture: 'checker.bmp' }), { allowMissingTextures: true });
  assert.match(missing.metadata.warnings[0], /changes appearance/);
});

test('truncated data, invalid indices/parents/normals, old versions, skins and instance cycles are refused', () => {
  assert.throws(() => convertStaticModel(fixture().subarray(0, -1)));
  assert.throws(() => convertStaticModel(fixture({ parent: 2 })), /hierarchie/);
  assert.throws(() => convertStaticModel(fixture({ version: 27 })), /verze/);
  assert.throws(() => convertStaticModel(fixture({ material: 2 })), /missing material/);
  assert.throws(() => convertStaticModel(fixture({ invalid: true })), /normals|UV/);
  assert.throws(() => convertStaticModel(fixture({ visual: 2 })), /skinned/);
  assert.throws(() => convertStaticModel(fixture({ animated: 1 })), /Animated/);
  assert.throws(() => convertStaticModel(fixture({ instanceCycle: true })), /Cyclic/);
  assert.throws(() => convertStaticModel(fixture({ scale: [-1, 1, 1] })), /nonpositive/);
});

test('CLI preserves read-only source and refuses overwrite, repository output and symlink escapes', t => {
  const temp = temporary(t), source = path.join(temp, 'synthetic.4ds'), output = path.join(temp, 'converted');
  fs.writeFileSync(source, fixture(), { mode: 0o444 }); fs.chmodSync(source, 0o444); const before = fs.readFileSync(source);
  const result = cli(['--source', source, '--output', output]); assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(fs.readFileSync(source), before); assert.equal(fs.statSync(source).mode & 0o777, 0o444);
  assert.ok(fs.existsSync(path.join(output, 'model.glb'))); assert.ok(fs.existsSync(path.join(output, 'metadata.json')));
  assert.equal(cli(['--source', source, '--output', output]).status, 1);
  assert.equal(cli(['--source', source, '--output', path.join(repository, 'forbidden-output')]).status, 1);
  const link = path.join(temp, 'repository-link'); fs.symlinkSync(repository, link);
  assert.equal(cli(['--source', source, '--output', path.join(link, 'forbidden-output')]).status, 1);
  const textures = path.join(temp, 'textures'); fs.mkdirSync(textures);
  fs.writeFileSync(path.join(temp, 'outside.bmp'), bitmap()); fs.symlinkSync(path.join(temp, 'outside.bmp'), path.join(textures, 'checker.bmp'));
  const textured = path.join(temp, 'textured.4ds'); fs.writeFileSync(textured, fixture({ texture: 'checker.bmp' }));
  const failure = cli(['--source', textured, '--output', path.join(temp, 'blocked'), '--textures', textures]);
  assert.equal(failure.status, 1); assert.match(failure.stderr, /symlink escapes/); assert.equal(fs.existsSync(path.join(temp, 'blocked')), false);
  assert.equal(fs.existsSync(path.join(repository, 'forbidden-output')), false);
});

test('Godot headless viewer loads external synthetic GLB, checks hierarchy/bounds and refuses external URIs', { skip: !process.env.GODOT }, t => {
  const temp = temporary(t), source = path.join(temp, 'synthetic.glb'), report = path.join(temp, 'structure.json');
  fs.writeFileSync(source, convertStaticModel(fixture()).glb);
  const run = (model = source, destination = report) => spawnSync(process.env.GODOT, ['--headless', '--path', repository, '--script', 'res://importers/fourds/model_viewer.gd', '--', model, destination, '--verify-only'], { encoding: 'utf8', timeout: 30000, env: { ...process.env, XDG_DATA_HOME: path.join(temp, 'data'), XDG_CONFIG_HOME: path.join(temp, 'config'), XDG_CACHE_HOME: path.join(temp, 'cache') } });
  const result = run(); assert.equal(result.status, 0, result.stdout + result.stderr);
  const details = JSON.parse(fs.readFileSync(report)); assert.equal(details.check, 'structure');
  assert.equal(details.glb_nodes, 2); assert.equal(details.mesh_instances, 1); assert.equal(details.surfaces, 1);
  // Converted parent quarter-turn around Y: (x,y,z) -> (z,y,-x).
  // Godot expands a flat mesh AABB to a minimum thickness of about 1e-5.
  assert.ok(Math.abs(details.bounds_size[0]) < 2e-5, JSON.stringify(details)); assert.ok(Math.abs(details.bounds_size[1] - 3) < 1e-5);
  assert.ok(Math.abs(details.bounds_size[2] - 2) < 1e-5);
  details.bounds_position.forEach((value, i) => assert.ok(Math.abs(value - [-2, 5, -7][i]) < 1e-5));
  assert.equal(run().status, 1, 'existing report must not be overwritten');
  fs.unlinkSync(report);
  // Exercise the direct entrypoint, not the already-canonicalizing Python wrapper.
  const repositoryLink = path.join(temp, 'checkout-link');
  fs.symlinkSync(repository, repositoryLink, 'dir');
  const forbiddenName = `.synthetic-viewer-report-${crypto.randomUUID()}.json`;
  const forbiddenReport = path.join(repository, forbiddenName);
  t.after(() => fs.rmSync(forbiddenReport, { force: true }));
  const outputBypass = run(source, path.join(repositoryLink, forbiddenName));
  assert.equal(outputBypass.status, 1); assert.match(outputBypass.stdout + outputBypass.stderr, /without symlink components/);
  assert.equal(fs.existsSync(forbiddenReport), false, 'a fresh report must never reach the checkout');
  fs.symlinkSync(forbiddenReport, report);
  const danglingReport = run();
  assert.equal(danglingReport.status, 1); assert.match(danglingReport.stdout + danglingReport.stderr, /without symlink components/);
  assert.equal(fs.existsSync(forbiddenReport), false); fs.unlinkSync(report);
  const sourceBypass = run(path.join(repositoryLink, 'README.md'));
  assert.equal(sourceBypass.status, 1); assert.match(sourceBypass.stdout + sourceBypass.stderr, /without symlink components/);
  assert.equal(fs.existsSync(report), false);
  const sourceLink = path.join(temp, 'source-link.glb'); fs.symlinkSync(source, sourceLink);
  const unchanged = fs.readFileSync(source); fs.chmodSync(source, 0o444);
  const fileBypass = run(sourceLink);
  assert.equal(fileBypass.status, 1); assert.match(fileBypass.stdout + fileBypass.stderr, /without symlink components/);
  assert.deepEqual(fs.readFileSync(source), unchanged); assert.equal(fs.statSync(source).mode & 0o777, 0o444);
  const resourceBypass = run('res://README.md');
  assert.equal(resourceBypass.status, 1); assert.match(resourceBypass.stdout + resourceBypass.stderr, /without symlink components/);
  fs.chmodSync(source, 0o600);
  const parts = glbParts(fs.readFileSync(source)); parts.json.images = [{ uri: 'outside.png' }];
  const json = Buffer.from(JSON.stringify(parts.json)), jsonPadded = Buffer.concat([json, Buffer.alloc((4 - json.length % 4) % 4, 32)]);
  const altered = Buffer.alloc(28 + jsonPadded.length + parts.binary.length);
  altered.write('glTF'); altered.writeUInt32LE(2, 4); altered.writeUInt32LE(altered.length, 8); altered.writeUInt32LE(jsonPadded.length, 12); altered.writeUInt32LE(0x4e4f534a, 16); jsonPadded.copy(altered, 20);
  altered.writeUInt32LE(parts.binary.length, 20 + jsonPadded.length); altered.writeUInt32LE(0x004e4942, 24 + jsonPadded.length); parts.binary.copy(altered, 28 + jsonPadded.length);
  fs.writeFileSync(source, altered); const rejected = run(); assert.equal(rejected.status, 1); assert.match(rejected.stdout + rejected.stderr, /self-contained/);
});
