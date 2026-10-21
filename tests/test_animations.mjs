import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {convertExperimentalAnimatedModel, convertStaticModel} from '../importers/fourds/convert.mjs';
import {multiply, localMatrix, sourceQuaternion} from '../importers/fourds/animation.mjs';
import {parseFiveDs} from '../importers/fourds/vendor/fiveds.mjs';
import {animatedFixture, clipFixture, glbParts, accessor} from './animation_fixtures.mjs';

const repository = fileURLToPath(new URL('..', import.meta.url));
const options = (source = clipFixture()) => ({secondsPerFrame:.04, animations:[{name:'SYNTHETIC',source}]});
const close = (a,b,tolerance=1e-6) => { assert.equal(a.length,b.length);a.forEach((v,i)=>assert.ok(Math.abs(v-b[i])<tolerance, `${i}: ${v} != ${b[i]}`)); };
const point = (matrix,p) => Array.from({length:3},(_,r)=>matrix[r]*p[0]+matrix[4+r]*p[1]+matrix[8+r]*p[2]+matrix[12+r]);

test('experimental converter preserves complete skin/morph geometry, opaque joint data and raw clip events', () => {
  const source=animatedFixture(), original=Buffer.from(source), clip=clipFixture(), copy=Buffer.from(clip);
  const {glb,metadata}=convertExperimentalAnimatedModel(source,options(clip)),parts=glbParts(glb),g=parts.json;
  assert.deepEqual(source,original);assert.deepEqual(clip,copy);
  assert.equal(metadata.engineFidelity,false);assert.equal(metadata.chosenSecondsPerFrame,.04);assert.equal(metadata.sourceTimeUnitsPerFrame,40);
  assert.match(metadata.sourceTimeUnitStatus,/physical seconds unverified/);
  assert.equal(metadata.vertexCount,4);assert.equal(metadata.triangleCount,2);assert.deepEqual(metadata.omitted,[]);
  assert.equal(g.skins[0].joints.length,3);assert.equal(g.meshes[0].primitives[0].targets.length,4);
  assert.equal(g.nodes[0].mesh,undefined);assert.equal(g.nodes[3].mesh,0);assert.equal(g.nodes[3].skin,0);
  assert.deepEqual(g.scenes[0].nodes,[0,3]);assert.deepEqual(g.skins[0].joints,[0,1,2]);
  assert.equal(g.nodes[1].extras.opaqueType10Matrix[0],99);assert.match(g.nodes[1].extras.opaqueType10MatrixSemantics,/unknown/);
  assert.equal(metadata.hierarchy[1].opaqueType10Matrix[0],99);
  const primitive=g.meshes[0].primitives[0],indices=accessor(parts,primitive.attributes.JOINTS_0),weights=accessor(parts,primitive.attributes.WEIGHTS_0);
  assert.deepEqual(indices,[1,0,0,0,1,0,0,0,2,0,0,0,0,0,0,0]);
  assert.deepEqual(weights,[1,0,0,0,.25,.75,0,0,1,0,0,0,1,0,0,0]);
  assert.deepEqual(accessor(parts,primitive.indices),[0,2,1,0,3,2]);
  const targets=primitive.targets.map(t=>accessor(parts,t.POSITION));
  close(targets[0],Array(12).fill(0));close(targets[1],[.25,0,0,.25,0,0,0,0,0,0,0,0]);
  close(targets[3],[0,0,0,0,0,0,.5,0,0,0,0,0]);
  const animation=g.animations[0];assert.equal(animation.channels.length,3);
  const notes=animation.extras.rawSourceNodes.find(n=>n.name==='jointB').tracks.note;
  assert.deepEqual(notes.times,[0,3]);assert.deepEqual(notes.values,[7,9]);
  assert.deepEqual(metadata.animations[0].nodes[0].tracks.rotation.times,[0,4]);
  assert.deepEqual(accessor(parts,animation.samplers[0].input),[0,Math.fround(.16)]);
  assert.equal(g.bufferViews[g.accessors[g.skins[0].inverseBindMatrices].bufferView].target,undefined);
});

test('source inverse matrices reconstruct nontrivial rest hierarchy and preserve weighted/rest transforms', () => {
  const {glb,metadata}=convertExperimentalAnimatedModel(animatedFixture(),options()),parts=glbParts(glb);
  const h=metadata.hierarchy,world=[];
  for(const frame of h)world.push(frame.parent?multiply(world[frame.parent-1],localMatrix(frame)):localMatrix(frame));
  const matrices=accessor(parts,parts.json.skins[0].inverseBindMatrices);
  for(let joint=0;joint<3;joint++){
    const skinMatrix=multiply(world[joint],matrices.slice(joint*16,joint*16+16));
    close(point(skinMatrix,[.3,.7,.2]),point(world[0],[.3,.7,.2]));
  }
  close(sourceQuaternion([Math.SQRT1_2,Math.SQRT1_2,0,0]),[Math.SQRT1_2,0,0,Math.SQRT1_2]);
  const rotation=localMatrix({position:[0,0,0],scale:[1,1,1],rotation:[Math.SQRT1_2,0,0,Math.SQRT1_2]});
  close(point(rotation,[1,0,0]),[0,-1,0]);
  assert.ok(metadata.skins[0].maxBindMatrixError<1e-6);
});

test('source final frame preserves clip length after sparse transforms and a later numeric event', () => {
  const clip=clipFixture();clip.writeUInt16LE(8,20);
  const numericNote=parseFiveDs(clip,true).nodes[2].tracks.note;
  clip.writeUInt16LE(8,numericNote.layout.timeOffset+2);
  const initial=glbParts(convertExperimentalAnimatedModel(animatedFixture(),options(clip)).glb);
  // All original transform keys end at4; the declared final frame is8.
  for(const sampler of initial.json.animations[0].samplers)
    assert.equal(accessor(initial,sampler.input).at(-1),Math.fround(.32));
  const raw=initial.json.animations[0].extras.rawSourceNodes;
  assert.deepEqual(raw[0].tracks.rotation.times,[0,4]);assert.deepEqual(raw[1].tracks.rotation.times,[0]);
  assert.deepEqual(raw[2].tracks.note.times,[0,8]);
  assert.match(initial.json.animations[0].extras.endpointPolicy,/Hold/);
  const track=initial.json.animations[0].samplers[0],values=accessor(initial,track.output);
  close(values.slice(-4),values.slice(-8,-4));
});

test('unsupported skin, morph, timing and unmatched/event layouts fail explicitly', () => {
  const model=animatedFixture();assert.throws(()=>convertStaticModel(model),/Animated/);
  for(const secondsPerFrame of [undefined,0,-1,NaN,Infinity,'0.04',Number.MIN_VALUE])
    assert.throws(()=>convertExperimentalAnimatedModel(model,{...options(),secondsPerFrame}),/secondsPerFrame|collapses/);
  for(const [fixture,pattern] of [[{badBind:true},/matrices disagree/],[{badParent:true},/skin/],[{overlap:true},/Overlapping/],[{twoLods:true},/one LOD/]])
    assert.throws(()=>convertExperimentalAnimatedModel(animatedFixture(fixture),options()),pattern);
  assert.throws(()=>convertExperimentalAnimatedModel(model,options(clipFixture({name:'missing'}))),/match/);
  assert.throws(()=>convertExperimentalAnimatedModel(animatedFixture({duplicateNames:true}),options()),/match/);
  assert.throws(()=>convertExperimentalAnimatedModel(model,options(clipFixture({noteFlags:32}))),/Unknown/);
  assert.throws(()=>convertExperimentalAnimatedModel(model,options(clipFixture({duplicateTimes:true}))),/duplicate/);
  assert.throws(()=>convertExperimentalAnimatedModel(model,options(clipFixture({empty:true}))),/Empty/);
  assert.throws(()=>convertExperimentalAnimatedModel(model,options(clipFixture({degenerate:true}))),/Degenerate/);
  assert.throws(()=>convertExperimentalAnimatedModel(model,options(clipFixture().subarray(0,-1))),/5DS/);
  assert.throws(()=>convertExperimentalAnimatedModel(model,{...options(),animations:[...options().animations,...options().animations]}),/unique/);
});

test('CLI requires explicit experimental/timing opt-in and keeps animation inputs outside repository', t => {
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'lhr-synthetic-animation-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const model=path.join(root,'model.4ds'),clip=path.join(root,'clip.5ds');fs.writeFileSync(model,animatedFixture());fs.writeFileSync(clip,clipFixture());
  const run=(extra,output='output')=>spawnSync(process.execPath,[path.join(repository,'importers/fourds/cli.mjs'),'--source',model,'--output',path.join(root,output),...extra],{encoding:'utf8',timeout:15000});
  assert.equal(run(['--clip',`TEST=${clip}`]).status,1);assert.equal(run(['--experimental-animation']).status,1);
  const result=run(['--experimental-animation','--seconds-per-frame','0.04','--clip',`TEST=${clip}`]);assert.equal(result.status,0,result.stderr);
  const metadata=JSON.parse(fs.readFileSync(path.join(root,'output','metadata.json')));assert.equal(metadata.engineFidelity,false);
  assert.equal(metadata.animations[0].name,'TEST');
  const bypass=run(['--experimental-animation','--seconds-per-frame','.04','--clip',`TEST=${path.join(repository,'README.md')}`],'blocked');
  assert.equal(bypass.status,1);assert.match(bypass.stderr,/outside the repository/);assert.equal(fs.existsSync(path.join(root,'blocked')),false);
});

test('Godot loads synthetic experimental skin and morph geometry plus all transform clips', {skip:!process.env.GODOT}, t => {
  const root=fs.mkdtempSync(path.join(fs.realpathSync(os.tmpdir()),'lhr-synthetic-animation-godot-'));t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const source=path.join(root,'model.glb'),report=path.join(root,'report.json');
  fs.writeFileSync(source,convertExperimentalAnimatedModel(animatedFixture(),options()).glb);
  const result=spawnSync(process.env.GODOT,['--headless','--path',repository,'--script','res://importers/fourds/model_viewer.gd','--',source,report,'--verify-only'],{encoding:'utf8',timeout:30000,env:{...process.env,XDG_DATA_HOME:path.join(root,'data'),XDG_CACHE_HOME:path.join(root,'cache'),XDG_CONFIG_HOME:path.join(root,'config')}});
  assert.equal(result.status,0,result.stdout+result.stderr);
  const details=JSON.parse(fs.readFileSync(report));assert.equal(details.skeleton_bones,3);assert.equal(details.blend_shapes,4);
  assert.deepEqual(details.animation_names,['SYNTHETIC']);assert.equal(details.surfaces,1);assert.equal(details.check_pose,'rest');
  assert.equal(details.mesh_compression,'disabled');assert.equal(details.gltf_import_flags,64);
});
