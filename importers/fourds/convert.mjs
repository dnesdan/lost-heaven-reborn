// Original project code, MIT. Static opaque LOD0 conversion; never executes input.
import { createHash } from 'node:crypto';
import { parseFourDs } from './vendor/fourds.mjs';
import { bmpToPng, pngDimensions } from './png.mjs';
import { prepareAnimation, addAnimatedGeometry, finishAnimations } from './animation.mjs';

const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const unsupportedMaterialFlags = (0x8000 | 0x80000 | 0x01000000 | 0x04000000
  | 0x20000000 | 0x40000000 | 0x80000000) >>> 0;
function vector(values, count, label) {
  if (values.length !== count || values.some(value => !Number.isFinite(value))) {
    throw new Error(`Invalid ${label}.`);
  }
}
export function safeTextureName(value) {
  if (typeof value !== 'string' || !value || value.length > 255
      || /[\\/\x00-\x1f\x7f:]/.test(value) || value === '.' || value === '..'
      || /[. ]$/.test(value) || !/\.bmp$/i.test(value)) {
    throw new Error('Only plain BMP texture filenames are supported.');
  }
  return value;
}

export function convertStaticModel(source, { resolveTexture, allowMissingTextures = false } = {}) {
  return convertModel(source, { resolveTexture, allowMissingTextures });
}

export function convertExperimentalAnimatedModel(source, options = {}) {
  return convertModel(source, options, options);
}

function convertModel(source, { resolveTexture, allowMissingTextures = false } = {}, experimental = null) {
  if (source.length > 64 * 1024 * 1024) throw new Error('Model exceeds the 64 MiB limit.');
  const model = parseFourDs(source, 'all');
  const animation = experimental ? prepareAnimation(model, experimental) : null;
  if (!animation && (model.animated || model.meshes.some(frame => frame.skin?.length || frame.morph?.targets || frame.type === 10))) {
    throw new Error('Animated, skinned, morph and joint models are outside this static converter.');
  }
  if (!animation && model.meshes.some(frame => ![1, 6].includes(frame.type)
      || frame.type === 1 && ![0, 1].includes(frame.visual))) {
    throw new Error('Only standard static visuals and dummy frames are supported.');
  }
  for (const material of model.materials) {
    if ((material.flags & unsupportedMaterialFlags) !== 0 || material.opacity !== 1) {
      throw new Error('Alpha, reflection, additive and animated materials are unsupported.');
    }
    for (const field of ['ambient', 'diffuse', 'emission']) {
      vector(material[field], 3, `material ${field}`);
      if (material[field].some(value => value < 0 || value > 1)) throw new Error('Material colors must be in 0..1.');
    }
    if (material.emission.some(value => value !== 0)) throw new Error('Emissive materials are unsupported.');
    if (material.texture) safeTextureName(material.texture);
  }
  const chunks = [], bufferViews = [], accessors = [];
  let byteLength = 0;
  function append(bytes, target) {
    const padding = (4 - byteLength % 4) % 4;
    if (padding) { chunks.push(Buffer.alloc(padding)); byteLength += padding; }
    const index = bufferViews.length;
    bufferViews.push({ buffer: 0, byteOffset: byteLength, byteLength: bytes.length, ...(target ? { target } : {}) });
    chunks.push(bytes); byteLength += bytes.length;
    return index;
  }
  function attribute(values, size, includeBounds = false, componentType = 5126, target = 34962) {
    if (values.length % size || values.some(value => !Number.isFinite(value))) throw new Error('Invalid vertex attribute.');
    const width = componentType === 5123 ? 2 : 4, bytes = Buffer.alloc(values.length * width);
    values.forEach((value, index) => {
      if (componentType === 5123) bytes.writeUInt16LE(value, index * width);
      else {
        if (!Number.isFinite(Math.fround(value))) throw new Error('Attribute exceeds float32 range.');
        bytes.writeFloatLE(value, index * width);
      }
    });
    const accessor = { bufferView: append(bytes, target), componentType,
      count: values.length / size, type: { 1: 'SCALAR', 2: 'VEC2', 3: 'VEC3', 4: 'VEC4', 16: 'MAT4' }[size] };
    if (includeBounds) {
      accessor.min = Array(size).fill(Infinity); accessor.max = Array(size).fill(-Infinity);
      values.forEach((value, index) => {
        if (componentType === 5126) value = Math.fround(value);
        accessor.min[index % size] = Math.min(accessor.min[index % size], value);
        accessor.max[index % size] = Math.max(accessor.max[index % size], value);
      });
    }
    return accessors.push(accessor) - 1;
  }
  function indexAccessor(values) {
    const bytes = Buffer.alloc(values.length * 2);
    values.forEach((value, index) => bytes.writeUInt16LE(value, index * 2));
    return accessors.push({ bufferView: append(bytes, 34963), componentType: 5123,
      count: values.length, type: 'SCALAR' }) - 1;
  }
  const gltf = { asset: { version: '2.0', generator: 'Lost Heaven Reborn static 4DS converter' },
    scene: 0, scenes: [{ nodes: [] }], nodes: [], meshes: [], materials: [],
    images: [], textures: [], samplers: [], bufferViews, accessors };
  const textureCache = new Map(), textureRecords = [], warnings = [];
  function texture(name, flags) {
    if (!name) return undefined;
    const wrapS = flags & 0x10000 ? 33071 : 10497, wrapT = flags & 0x20000 ? 33071 : 10497;
    const key = `${name.toLowerCase()}:${wrapS}:${wrapT}`;
    if (textureCache.has(key)) return textureCache.get(key);
    const original = resolveTexture?.(name);
    if (!original) {
      if (!allowMissingTextures) throw new Error('Referenced texture is missing; supply --textures or explicitly allow missing textures.');
      warnings.push('A referenced texture is missing; diffuse-color fallback changes appearance.');
      textureRecords.push({ name, status: 'missing' });
      textureCache.set(key, undefined); return undefined;
    }
    if (original.length > 64 * 1024 * 1024) throw new Error('Texture exceeds the 64 MiB limit.');
    const png = bmpToPng(original), dimensions = pngDimensions(png);
    const image = gltf.images.push({ bufferView: append(png), mimeType: 'image/png' }) - 1;
    // Filtering is a documented presentation choice, not a game-engine fidelity claim.
    const sampler = gltf.samplers.push({ magFilter: 9728, minFilter: 9728, wrapS, wrapT }) - 1;
    const index = gltf.textures.push({ source: image, sampler }) - 1;
    textureCache.set(key, index);
    textureRecords.push({ name, status: 'embedded', sourceSha256: sha256(original), pngSha256: sha256(png), ...dimensions });
    return index;
  }
  model.materials.forEach((material, index) => {
    const image = texture(material.texture, material.flags);
    const color = image !== undefined && !(material.flags & 0x08000000) ? [1, 1, 1] : material.diffuse;
    gltf.materials.push({ name: `material_${index + 1}`, alphaMode: 'OPAQUE',
      doubleSided: Boolean(material.flags & 0x10000000),
      pbrMetallicRoughness: { baseColorFactor: [...color, 1], metallicFactor: 0, roughnessFactor: 1,
        ...(image === undefined ? {} : { baseColorTexture: { index: image } }) },
      extras: { sourceFlags: material.flags, sourceTexture: material.texture,
        sourceAmbient: material.ambient, sourceDiffuse: material.diffuse, sourceEmission: material.emission } });
  });
  function resolve(frame, visited = new Set()) {
    if (visited.has(frame.id)) throw new Error('Cyclic geometry instance.');
    visited.add(frame.id);
    if (!frame.instance) return frame;
    const target = model.meshes[frame.instance - 1];
    if (!target || target.type !== 1) throw new Error('Invalid geometry instance target.');
    return resolve(target, visited);
  }
  const enabled = frame => Boolean(frame.culling & 1) && (!frame.parent || enabled(model.meshes[frame.parent - 1]));
  for (const frame of model.meshes) {
    const quaternion = frame.rotation, norm = Math.hypot(...quaternion);
    if (norm < 1e-8 || frame.scale.some(value => value <= 0)) throw new Error('Degenerate rotation or nonpositive scale.');
    gltf.nodes.push({ name: frame.name || `frame_${frame.id}`,
      translation: [frame.position[0], frame.position[1], -frame.position[2]],
      rotation: [quaternion[1] / norm, quaternion[2] / norm, -quaternion[3] / norm, quaternion[0] / norm],
      scale: frame.scale,
      extras: { sourceFrameId: frame.id, sourceParent: frame.parent, sourceType: frame.type,
        sourceVisual: frame.visual, sourceCulling: frame.culling, sourceRenderFlags: frame.renderFlags,
        sourceLogicFlags: frame.logicFlags, sourceProperties: frame.properties,
        ...(animation && frame.type === 10 ? { sourceJointId: frame.joint,
          opaqueType10Matrix: frame.inverseBind, opaqueType10MatrixSemantics: 'unknown; not the skin inverse matrix' } : {}) } });
  }
  let vertexCount = 0, triangleCount = 0;
  const omitted = [];
  for (const frame of model.meshes) {
    const node = gltf.nodes[frame.id - 1];
    if (frame.parent) (gltf.nodes[frame.parent - 1].children ??= []).push(frame.id - 1);
    else gltf.scenes[0].nodes.push(frame.id - 1);
    const geometry = resolve(frame), lod = geometry.lods?.[0];
    if (!lod?.positions.length) continue;
    if (!enabled(frame) || frame.renderFlags & 0x40 || /(?:^|\s)SECT(?:\s|$)/i.test(frame.properties ?? '')) {
      if (animation) throw new Error('Experimental conversion refuses hidden geometry instead of omitting polygons.');
      omitted.push({ frame: frame.id, reason: 'disabled or hidden source surface' }); continue;
    }
    if (lod.invalidNormals || lod.invalidUV) throw new Error('Invalid source normals or UVs cannot be converted silently.');
    for (let index = 0; index < lod.normals.length; index += 3) {
      if (Math.abs(Math.hypot(...lod.normals.slice(index, index + 3)) - 1) > 0.001) {
        throw new Error('Source normals must be normalized.');
      }
    }
    const positions = lod.positions.map((value, index) => index % 3 === 2 ? -value : value);
    const normals = lod.normals.map((value, index) => index % 3 === 2 ? -value : value);
    const attributes = { POSITION: attribute(positions, 3, true), NORMAL: attribute(normals, 3), TEXCOORD_0: attribute(lod.uv, 2) };
    const morph = animation ? addAnimatedGeometry(frame, lod, animation, gltf, attribute, attributes) : null;
    const primitives = [];
    for (const group of lod.faces) {
      if (!group.indices.length) continue;
      if (group.material < 1 || group.material > model.materials.length) throw new Error('Triangle group references a missing material.');
      const reversed = [];
      for (let index = 0; index < group.indices.length; index += 3) {
        reversed.push(group.indices[index], group.indices[index + 2], group.indices[index + 1]);
      }
      triangleCount += reversed.length / 3;
      primitives.push({ attributes, indices: indexAccessor(reversed), material: group.material - 1, mode: 4,
        ...(morph?.targets.length ? { targets: morph.targets } : {}) });
    }
    if (!primitives.length) continue;
    vertexCount += lod.vertices;
    node.mesh = gltf.meshes.push({ name: node.name, primitives,
      ...(morph?.targets.length ? { weights: Array(morph.targets.length).fill(0) } : {}),
      extras: { sourceLod: 0, sourceLodDistance: lod.distance, omittedLowerLods: geometry.lods.length - 1 } }) - 1;
    if (morph?.targets.length) gltf.meshes[node.mesh].extras.targetNames = morph.targetNames;
  }
  if (!triangleCount) throw new Error('Model contains no supported visible triangles.');
  if (animation) finishAnimations(animation, gltf, attribute);
  if (!gltf.images.length) { delete gltf.images; delete gltf.textures; delete gltf.samplers; }
  const metadata = { schema: 'lhr-static-model-v1', sourceFormat: '4DS', sourceVersion: model.version,
    sourceSha256: sha256(source), sourceBasis: 'left-handed Y-up', targetBasis: 'right-handed Y-up',
    conversion: 'Z reflection; conjugate stored WXYZ and reflect its basis; reverse triangle winding; preserve UVs',
    unitScale: 1, unitStatus: 'source numeric units preserved; physical metre interpretation unverified',
    scope: 'opaque static LOD0; positive scales; original hierarchy retained; no animation, skeleton, collision or gameplay conversion',
    shading: 'opaque diffuse PBR approximation; metallic=0, roughness=1, nearest texture filtering; game appearance unverified',
    frameCount: model.meshes.length, meshCount: gltf.meshes.length, vertexCount, triangleCount,
    sourceAnimatedFlag: model.animated, sourceMaterials: model.materials, textures: textureRecords, omitted, warnings,
    hierarchy: model.meshes.map(frame => ({ id: frame.id, name: frame.name, parent: frame.parent,
      type: frame.type, visual: frame.visual, position: frame.position, rotation: frame.rotation, scale: frame.scale })) };
  gltf.extras = { schema: metadata.schema, sourceSha256: metadata.sourceSha256,
    sourceBasis: metadata.sourceBasis, targetBasis: metadata.targetBasis,
    unitScale: 1, unitStatus: metadata.unitStatus, scope: metadata.scope };
  if (animation) {
    metadata.schema = 'lhr-experimental-animation-v1';
    metadata.scope = 'experimental single-LOD skin/morph/5DS asset playback; original engine behavior unverified';
    metadata.engineFidelity = false;
    metadata.chosenSecondsPerFrame = animation.secondsPerFrame;
    metadata.sourceTimeUnitsPerFrame = 40;
    metadata.sourceTimeUnitStatus = 'PC static analysis: 40 engine API units per file frame; physical seconds unverified';
    metadata.staticEvidence = 'PC-ANIMATION-API-DOSSIER; duration/key getters; no original runtime comparison';
    metadata.interpolation = 'provisional local replacement; glTF LINEAR vectors and slerp rotations';
    metadata.limitations = animation.limitations;
    metadata.warnings.push('Experimental animation: original engine fidelity is unverified; secondsPerFrame and interpolation are explicit playback assumptions.');
    metadata.skins = [...animation.skins].map(([frameId, skin]) => ({ frameId,
      jointFrameIds: skin.bones.map(b => b.id), maxBindMatrixError: skin.maxBindError,
      source: skin.source }));
    metadata.morphs = [...animation.morphs].map(([frameId, morph]) => ({ frameId,
      targetsPerRegion: morph.targets, regions: morph.regions.map(r => ({ indices: r.indices })),
      exportedRegionTargets: morph.targets * morph.regions.length }));
    metadata.animations = animation.clips;
    metadata.hierarchy.forEach((frame, index) => {
      if (model.meshes[index].type === 10) Object.assign(frame, {
        sourceJointId: model.meshes[index].joint,
        opaqueType10Matrix: model.meshes[index].inverseBind,
        opaqueType10MatrixSemantics: 'unknown; not the skin inverse matrix' });
    });
    gltf.extras = { ...gltf.extras, schema: metadata.schema, scope: metadata.scope,
      engineFidelity: false, chosenSecondsPerFrame: metadata.chosenSecondsPerFrame,
      sourceTimeUnitsPerFrame: 40, sourceTimeUnitStatus: metadata.sourceTimeUnitStatus,
      limitations: animation.limitations };
  }
  const binary = Buffer.concat(chunks); gltf.buffers = [{ byteLength: binary.length }];
  const json = Buffer.from(JSON.stringify(gltf)), jsonPadded = Buffer.concat([json, Buffer.alloc((4 - json.length % 4) % 4, 32)]);
  const binaryPadded = Buffer.concat([binary, Buffer.alloc((4 - binary.length % 4) % 4)]);
  const glb = Buffer.alloc(28 + jsonPadded.length + binaryPadded.length);
  glb.write('glTF'); glb.writeUInt32LE(2, 4); glb.writeUInt32LE(glb.length, 8);
  glb.writeUInt32LE(jsonPadded.length, 12); glb.writeUInt32LE(0x4e4f534a, 16); jsonPadded.copy(glb, 20);
  const offset = 20 + jsonPadded.length;
  glb.writeUInt32LE(binaryPadded.length, offset); glb.writeUInt32LE(0x004e4942, offset + 4); binaryPadded.copy(glb, offset + 8);
  metadata.glbSha256 = sha256(glb);
  return { glb, metadata };
}
