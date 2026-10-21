// Original MIT project code. Experimental asset conversion, not engine behavior.
import { createHash } from 'node:crypto';
import { parseFiveDs } from './vendor/fiveds.mjs';

const identity = () => [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
const canonical = name => name.normalize('NFC').toLowerCase();
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
export const animationLimitations = [
  'Experimental asset playback; original engine and runtime fidelity are unverified.',
  'secondsPerFrame is an explicit caller assumption; raw source frame keys are retained.',
  'Local channel replacement and glTF LINEAR/rotation slerp interpolation are provisional.',
  'Final transform values are held through the declared source final frame to preserve chosen clip duration.',
  'Frame names match uniquely using NFC and lowercase; original matching rules are unverified.',
  'Note tracks are preserved as raw metadata; event behavior is not executed.',
  'Morph regions are independent glTF shapes; original mixing and speech behavior are unimplemented.',
  'Godot 4.6.3 imports skin weights with reduced precision, even with mesh compression disabled.',
  'Helper frames and opaque type10 matrix values are retained without implementing their engine behavior.'
];

// Column-major matrices act on column vectors. Reflection is C*M*C, C=diag(1,1,-1,1).
export function multiply(a, b) {
  return Array.from({ length: 16 }, (_, i) => {
    const row = i % 4, column = Math.floor(i / 4);
    return a[row] * b[column * 4] + a[4 + row] * b[column * 4 + 1]
      + a[8 + row] * b[column * 4 + 2] + a[12 + row] * b[column * 4 + 3];
  });
}
function inverse(matrix) {
  const rows = Array.from({ length: 4 }, (_, r) => [
    ...Array.from({ length: 4 }, (_, c) => matrix[c * 4 + r]),
    ...Array.from({ length: 4 }, (_, c) => Number(c === r))]);
  for (let c = 0; c < 4; c++) {
    let pivot = c;
    for (let r = c + 1; r < 4; r++) if (Math.abs(rows[r][c]) > Math.abs(rows[pivot][c])) pivot = r;
    if (Math.abs(rows[pivot][c]) < 1e-12) throw new Error('Singular bind transform.');
    [rows[c], rows[pivot]] = [rows[pivot], rows[c]];
    const divisor = rows[c][c]; rows[c] = rows[c].map(value => value / divisor);
    for (let r = 0; r < 4; r++) if (r !== c) {
      const factor = rows[r][c]; rows[r] = rows[r].map((value, i) => value - factor * rows[c][i]);
    }
  }
  return Array.from({ length: 16 }, (_, i) => rows[i % 4][4 + Math.floor(i / 4)]);
}
export function sourceQuaternion(q) {
  const norm = Math.hypot(...q);
  if (!Number.isFinite(norm) || norm < 1e-8) throw new Error('Degenerate source rotation.');
  // Conjugate stored WXYZ, then reflect Z: XYZW=(x,y,-z,w)/norm.
  return [q[1] / norm, q[2] / norm, -q[3] / norm, q[0] / norm];
}
export function sourceVector(values) {
  return values.map((value, i) => i % 3 === 2 ? -value : value);
}
export function localMatrix(frame) {
  const [x, y, z, w] = sourceQuaternion(frame.rotation), [sx, sy, sz] = frame.scale;
  return [(1 - 2 * (y*y + z*z)) * sx, 2 * (x*y + z*w) * sx, 2 * (x*z - y*w) * sx, 0,
    2 * (x*y - z*w) * sy, (1 - 2 * (x*x + z*z)) * sy, 2 * (y*z + x*w) * sy, 0,
    2 * (x*z + y*w) * sz, 2 * (y*z - x*w) * sz, (1 - 2 * (x*x + y*y)) * sz, 0,
    ...sourceVector(frame.position), 1];
}
function reflectMatrix(values) {
  return values.map((value, i) => ((i % 4 === 2) !== (Math.floor(i / 4) === 2)) ? -value : value);
}

export function prepareAnimation(model, { animations = [], secondsPerFrame } = {}) {
  if (typeof secondsPerFrame !== 'number' || !Number.isFinite(secondsPerFrame)
      || secondsPerFrame <= 0 || secondsPerFrame > 3600) {
    throw new Error('Experimental animation requires explicit secondsPerFrame in (0, 3600].');
  }
  if (!Array.isArray(animations) || animations.length > 32) throw new Error('At most 32 animation clips are supported.');
  if (model.meshes.some(f => ![1, 6, 7, 10].includes(f.type)
      || f.type === 1 && (![0, 1, 2, 3, 5].includes(f.visual) || f.instance))) {
    throw new Error('Experimental conversion supports standalone standard/skin/morph visuals, joints and helper frames only.');
  }
  if (model.meshes.some(f => f.lods && f.lods.length !== 1)) throw new Error('Experimental conversion requires exactly one LOD; no lower LODs are omitted.');
  const world = new Map([[0, identity()]]), visiting = new Set();
  function transform(frame) {
    if (world.has(frame.id)) return world.get(frame.id);
    if (visiting.has(frame.id)) throw new Error('Cyclic frame hierarchy.');
    visiting.add(frame.id);
    if (frame.scale.some(v => v <= 0)) throw new Error('Nonpositive frame scales are unsupported.');
    const parent = frame.parent ? transform(model.meshes[frame.parent - 1]) : identity();
    const value = multiply(parent, localMatrix(frame)); world.set(frame.id, value); visiting.delete(frame.id); return value;
  }
  for (const frame of model.meshes) transform(frame);
  const skins = new Map(), morphs = new Map();
  let morphValues = 0;
  for (const owner of model.meshes) {
    if (owner.skin?.length) {
      const skin = owner.skin[0], bones = [owner];
      if (!skin.valid) throw new Error('Invalid source skin runs or weights.');
      for (const group of skin.bones) {
        const matches = model.meshes.filter(f => {
          if (f.joint !== group.id) return false;
          let parent = f.parent;
          while (parent && parent !== owner.id) parent = model.meshes[parent - 1].parent;
          return parent === owner.id;
        });
        if (matches.length !== 1) throw new Error('Each skin joint must bind uniquely below its owner.');
        bones.push(matches[0]);
      }
      const matrices = [identity(), ...skin.bones.map(b => reflectMatrix(b.inverse))];
      let maxBindError = 0;
      for (let i = 0; i < matrices.length; i++) {
        if (matrices[i].some(v => !Number.isFinite(v))) throw new Error('Nonfinite source skin matrix.');
        const expected = multiply(inverse(world.get(bones[i].id)), world.get(owner.id));
        maxBindError = Math.max(maxBindError, ...expected.map((v, j) => Math.abs(v - matrices[i][j])));
      }
      if (maxBindError > 1e-4) throw new Error('Source skin matrices disagree with the rest hierarchy.');
      const indices = [], weights = [];
      for (const group of skin.bones) {
        for (let i = 0; i < group.rigid; i++) { indices.push(group.id, 0, 0, 0); weights.push(1, 0, 0, 0); }
        for (const weight of group.weights) { indices.push(group.id, group.parent, 0, 0); weights.push(weight, 1 - weight, 0, 0); }
      }
      for (let i = 0; i < skin.rootVertices; i++) { indices.push(0, 0, 0, 0); weights.push(1, 0, 0, 0); }
      if (indices.length !== owner.lods[0].vertices * 4) throw new Error('Skin runs do not cover every vertex.');
      skins.set(owner.id, { bones, matrices, indices, weights, maxBindError,
        source: { rootVertices: skin.rootVertices, bones: skin.bones } });
    }
    if (owner.morph?.targets) {
      const morph = owner.morph, lod = owner.lods[0], regions = morph.lods[0];
      if (morph.lods.length !== 1 || regions.length !== morph.channels || morph.targets * morph.channels > 64) {
        throw new Error('Unsupported morph LOD/region/target count.');
      }
      morphValues += morph.targets * morph.channels * lod.vertices * 6;
      if (morphValues > 16 * 1024 * 1024) throw new Error('Morph expansion exceeds the 64 MiB float budget.');
      const used = new Set();
      for (const region of regions) {
        if (region.valid === false || region.indices.length && region.targets.length !== morph.targets) throw new Error('Invalid morph region.');
        for (const index of region.indices) {
          if (used.has(index)) throw new Error('Overlapping morph regions need original overwrite/mixing semantics.');
          used.add(index);
        }
      }
      morphs.set(owner.id, { targets: morph.targets, regions });
    }
  }
  const names = new Map();
  for (const frame of model.meshes) {
    const name = canonical(frame.name);
    names.set(name, names.has(name) ? null : frame);
  }
  let clipBytes = 0;
  const seen = new Set(), clips = animations.map(({ name, source } = {}) => {
    if (typeof name !== 'string' || !/^[A-Za-z0-9_-]{1,64}$/.test(name) || seen.has(canonical(name))) throw new Error('Clip names must be unique plain identifiers.');
    seen.add(canonical(name));
    if (!(source instanceof Uint8Array) || source.length > 64 * 1024 * 1024) throw new Error('Invalid or oversized 5DS source.');
    clipBytes += source.length;
    if (clipBytes > 64 * 1024 * 1024) throw new Error('Combined animation sources exceed 64 MiB.');
    const parsed = parseFiveDs(source, true), destinations = new Set();
    for (const node of parsed.nodes) {
      if (node.unknownFlags) throw new Error('Unknown 5DS track flags cannot be omitted.');
      const match = names.get(canonical(node.name));
      if (!match || destinations.has(match.id)) throw new Error('Every animation node must match one distinct model frame.');
      destinations.add(match.id); node.destination = match.id - 1;
      for (const [channel, track] of Object.entries(node.tracks)) {
        if (!track.times.length || track.times.some((t, i) => i && t <= track.times[i - 1])) throw new Error('Empty or duplicate animation keys are unsupported.');
        if (channel === 'rotation') track.values.forEach(sourceQuaternion);
        const timesForOutput = channel !== 'note' && track.times.at(-1) < parsed.frames
          ? [...track.times, parsed.frames] : track.times;
        const outputTimes = timesForOutput.map(t => Math.fround(t * secondsPerFrame));
        if (outputTimes.some((t, i) => !Number.isFinite(t) || i && t <= outputTimes[i - 1])) {
          throw new Error('Chosen timing collapses distinct frame keys in float32.');
        }
      }
    }
    return { name, sourceSha256: sha256(source), sourceVersion: parsed.version,
      sourceFinalFrame: parsed.frames, duration: parsed.frames * secondsPerFrame, nodes: parsed.nodes };
  });
  return { skins, morphs, clips, secondsPerFrame, limitations: [...animationLimitations] };
}

export function addAnimatedGeometry(frame, lod, state, gltf, attribute, attributes) {
  const skin = state.skins.get(frame.id), morph = state.morphs.get(frame.id);
  if (skin) {
    attributes.JOINTS_0 = attribute(skin.indices, 4, false, 5123);
    attributes.WEIGHTS_0 = attribute(skin.weights, 4);
  }
  const targets = [], targetNames = [];
  if (morph) for (const [regionIndex, region] of morph.regions.entries()) {
    for (let target = 0; target < morph.targets; target++) {
      const positions = Array(lod.vertices * 3).fill(0), normals = [...positions];
      region.indices.forEach((vertex, i) => {
        for (let axis = 0; axis < 3; axis++) {
          positions[vertex * 3 + axis] = region.targets[target].positions[i * 3 + axis] - lod.positions[vertex * 3 + axis];
          normals[vertex * 3 + axis] = region.targets[target].normals[i * 3 + axis] - lod.normals[vertex * 3 + axis];
        }
      });
      targets.push({ POSITION: attribute(sourceVector(positions), 3, true), NORMAL: attribute(sourceVector(normals), 3) });
      targetNames.push(`region_${regionIndex}_target_${target}`);
    }
  }
  return { targets, targetNames };
}

export function finishAnimations(state, gltf, attribute) {
  gltf.skins = [];
  for (const [frameId, skin] of state.skins) {
    const node = gltf.nodes[frameId - 1];
    if (node.mesh === undefined) throw new Error('A skin owner must contain a supported nonempty surface.');
    const index = gltf.skins.push({ name: `${node.name}_skin`, skeleton: frameId - 1,
      joints: skin.bones.map(b => b.id - 1),
      inverseBindMatrices: attribute(skin.matrices.flat(), 16, false, 5126, null),
      extras: { sourceSkin: skin.source, maxBindMatrixError: skin.maxBindError } }) - 1;
    // Keep vertices in original owner-local space. jointWorld*sourceInverse maps
    // them into model space; an identity surface sibling avoids applying owner twice.
    const surface = gltf.nodes.push({ name: `${node.name}_skinned_surface`, mesh: node.mesh, skin: index }) - 1;
    delete node.mesh; gltf.scenes[0].nodes.push(surface);
  }
  if (!gltf.skins.length) delete gltf.skins;
  gltf.animations = state.clips.map(clip => {
    const result = { name: clip.name, channels: [], samplers: [], extras: {
      sourceSha256: clip.sourceSha256, sourceFinalFrame: clip.sourceFinalFrame,
      assumedSecondsPerFrame: state.secondsPerFrame, engineFidelity: false,
      endpointPolicy: 'Hold final transform values to sourceFinalFrame; raw keys below remain unchanged.',
      rawSourceNodes: clip.nodes.map(({ destination, layout, ...node }) => node) } };
    for (const node of clip.nodes) for (const [channel, track] of Object.entries(node.tracks)) {
      if (channel === 'note') continue; // Retained verbatim above; never executed.
      const times = [...track.times], sourceValues = [...track.values];
      if (times.at(-1) < clip.sourceFinalFrame) {
        times.push(clip.sourceFinalFrame); sourceValues.push(sourceValues.at(-1));
      }
      const values = sourceValues.flatMap(value => channel === 'rotation' ? sourceQuaternion(value) : channel === 'position' ? sourceVector(value) : value);
      const input = attribute(times.map(t => t * state.secondsPerFrame), 1, true, 5126, null);
      const output = attribute(values, channel === 'rotation' ? 4 : 3, false, 5126, null);
      result.channels.push({ sampler: result.samplers.length, target: {
        node: node.destination, path: { rotation: 'rotation', position: 'translation', scale: 'scale' }[channel] } });
      result.samplers.push({ input, output, interpolation: 'LINEAR' });
    }
    if (!result.channels.length) throw new Error('Clips without transform channels cannot produce glTF animations.');
    return result;
  });
  if (!gltf.animations.length) delete gltf.animations;
}
