#!/usr/bin/env node
// Original project code, MIT. All model data and output stay outside the checkout.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { convertStaticModel, convertExperimentalAnimatedModel, safeTextureName } from './convert.mjs';

const repository = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
function within(candidate, root) {
  const relative = path.relative(root, candidate);
  return relative === '' || !relative.startsWith(`..${path.sep}`) && relative !== '..' && !path.isAbsolute(relative);
}
function existingAncestor(candidate) {
  let current = path.resolve(candidate), suffix = [];
  while (!fs.existsSync(current)) {
    const parent = path.dirname(current);
    if (parent === current) throw new Error('Cannot resolve output destination.');
    suffix.unshift(path.basename(current)); current = parent;
  }
  return path.join(fs.realpathSync(current), ...suffix);
}
function privatePath(candidate) {
  const absolute = existingAncestor(candidate);
  if (within(absolute, repository)) throw new Error('Input, textures and output must remain outside the repository.');
  return absolute;
}
function readRegular(file, limit) {
  const info = fs.statSync(file);
  if (!info.isFile() || info.size > limit) throw new Error('Input must be a regular file within the supported size limit.');
  return fs.readFileSync(file);
}
function main() {
  const args = process.argv.slice(2), options = {}, clipArguments = [];
  for (let index = 0; index < args.length; index++) {
    const argument = args[index];
    if (argument === '--experimental-animation') {
      if (options.experimentalAnimation) throw new Error('Duplicate experimental option.');
      options.experimentalAnimation = true; continue;
    }
    if (argument === '--clip') {
      if (!args[index + 1] || args[index + 1].startsWith('--')) throw new Error('Expected --clip NAME=PRIVATE_FILE.5ds.');
      clipArguments.push(args[++index]); continue;
    }
    if (argument === '--allow-missing-textures') { options.allowMissingTextures = true; continue; }
    if (!['--source', '--output', '--textures', '--seconds-per-frame'].includes(argument) || !args[index + 1]
        || args[index + 1].startsWith('--') || options[argument]) throw new Error('Invalid or duplicate argument.');
    options[argument] = args[++index];
  }
  if (!options['--source'] || !options['--output']) {
    throw new Error('Usage: convert-model.sh --source FILE.4ds --output NEW_PRIVATE_DIRECTORY [--textures BMP_DIRECTORY] [--allow-missing-textures]');
  }
  if (!options.experimentalAnimation && (clipArguments.length || options['--seconds-per-frame'])) {
    throw new Error('Clip/timing options require --experimental-animation.');
  }
  if (options.experimentalAnimation && !options['--seconds-per-frame']) {
    throw new Error('Experimental conversion requires --seconds-per-frame with an explicit timing assumption.');
  }
  const source = privatePath(options['--source']), output = privatePath(options['--output']);
  if (fs.existsSync(output)) throw new Error('Output directory already exists; refusing to overwrite it.');
  let resolveTexture;
  if (options['--textures']) {
    const root = privatePath(options['--textures']);
    if (!fs.statSync(root).isDirectory()) throw new Error('Texture root must be a directory.');
    const entries = fs.readdirSync(root), names = new Map();
    for (const name of entries) {
      const canonical = name.toLowerCase();
      if (names.has(canonical)) throw new Error('Texture directory has ambiguous case-insensitive names.');
      names.set(canonical, name);
    }
    resolveTexture = name => {
      safeTextureName(name);
      const actual = names.get(name.toLowerCase());
      if (!actual) return undefined;
      const file = fs.realpathSync(path.join(root, actual));
      if (!within(file, root)) throw new Error('Texture symlink escapes the texture directory.');
      return readRegular(file, 64 * 1024 * 1024);
    };
  }
  const bytes = readRegular(source, 64 * 1024 * 1024);
  if (clipArguments.length > 32) throw new Error('At most 32 animation clips are supported.');
  let clipBytes = 0;
  const animations = clipArguments.map(value => {
    const split = value.indexOf('=');
    if (split < 1 || split === value.length - 1) throw new Error('Expected --clip NAME=PRIVATE_FILE.5ds.');
    const source = readRegular(privatePath(value.slice(split + 1)), 64 * 1024 * 1024);
    clipBytes += source.length;
    if (clipBytes > 64 * 1024 * 1024) throw new Error('Combined animation sources exceed 64 MiB.');
    return { name: value.slice(0, split), source };
  });
  const conversion = { resolveTexture, allowMissingTextures: options.allowMissingTextures,
    animations, secondsPerFrame: Number(options['--seconds-per-frame']) };
  const result = options.experimentalAnimation
    ? convertExperimentalAnimatedModel(bytes, conversion) : convertStaticModel(bytes, conversion);
  // Conversion completes before creating output. Fresh directories and exclusive
  // files prevent replacement of existing evidence or a source file.
  fs.mkdirSync(output, { mode: 0o700, recursive: false });
  try {
    fs.writeFileSync(path.join(output, 'model.glb'), result.glb, { mode: 0o600, flag: 'wx' });
    fs.writeFileSync(path.join(output, 'metadata.json'), JSON.stringify(result.metadata, null, 2) + '\n', { mode: 0o600, flag: 'wx' });
  } catch (error) {
    // Preserve partially written evidence; never recursively remove user paths.
    throw error;
  }
  console.log(JSON.stringify({ sourceSha256: result.metadata.sourceSha256,
    glbSha256: result.metadata.glbSha256, frames: result.metadata.frameCount,
    meshes: result.metadata.meshCount, triangles: result.metadata.triangleCount,
    ...(options.experimentalAnimation ? { experimentalAnimation: true, engineFidelity: false,
      chosenSecondsPerFrame: result.metadata.chosenSecondsPerFrame,
      sourceTimeUnitsPerFrame: result.metadata.sourceTimeUnitsPerFrame } : {}),
    warnings: result.metadata.warnings }));
}
try { main(); }
catch (error) {
  // OS errors can embed private filenames; detailed paths never enter normal logs.
  console.error(error.code ? 'Conversion failed: a private file could not be read or written.' : `Conversion failed: ${error.message}`);
  process.exitCode = 1;
}
