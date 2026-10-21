//! Read-only scene2.bin chunks. Layout follows pinned MIT mafia-workbench scene.js;
//! scripts, actor state, sound and light payloads remain unexecuted metadata.
use super::{Error, MAX_BYTES, Reader, Result, err};

#[derive(Debug, Clone, Default)]
pub struct SceneObject {
    pub name: String,
    pub frame_type: Option<u32>,
    pub model: Option<String>,
    pub parent: Option<String>,
    pub position: Option<[f32; 3]>,
    pub world_position: Option<[f32; 3]>,
    pub direction: Option<[f32; 3]>,
    pub rotation_wxyz: Option<[f32; 4]>,
    pub scale: Option<[f32; 3]>,
    pub uniform_scale: Option<f32>,
}
#[derive(Debug, Clone, Default)]
pub struct SceneActor {
    pub name: String,
    pub kind: Option<u32>,
}
#[derive(Debug, Clone)]
pub struct SceneScript {
    pub name: String,
    pub code: String,
}
#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub objects: Vec<SceneObject>,
    pub actors: Vec<SceneActor>,
    pub scripts: Vec<SceneScript>,
    /// Unsupported chunks or actor payloads; never executed.
    pub unknown_chunks: usize,
}
#[derive(Clone, Copy)]
struct Chunk<'a> {
    kind: u16,
    payload: &'a [u8],
}
fn chunks<'a>(bytes: &'a [u8], count: &mut usize) -> Result<Vec<Chunk<'a>>> {
    let mut r = Reader { b: bytes, p: 0 };
    let mut result = Vec::new();
    while r.p < bytes.len() {
        *count += 1;
        if *count > 200_000 {
            return err("scene chunk quota exceeded");
        }
        let kind = r.u16()?;
        let size = r.u32()? as usize;
        if size < 6 {
            return err("scene chunk smaller than header");
        }
        result.push(Chunk {
            kind,
            payload: r.take(size - 6)?,
        });
    }
    Ok(result)
}
fn string(bytes: &[u8]) -> Result<String> {
    let end = bytes
        .iter()
        .position(|b| *b == 0)
        .ok_or_else(|| Error("scene string lacks NUL terminator".into()))?;
    if end > 4096 {
        return err("scene string quota exceeded");
    }
    super::decode_windows1250(&bytes[..end])
}
fn numeric<const N: usize>(bytes: &[u8]) -> Result<[f32; N]> {
    Reader { b: bytes, p: 0 }.vec()
}
fn uint(bytes: &[u8]) -> Result<u32> {
    Reader { b: bytes, p: 0 }.u32()
}
fn set<T>(destination: &mut Option<T>, value: T) -> Result<()> {
    if destination.replace(value).is_some() {
        return err("duplicate scene object field");
    }
    Ok(())
}
fn object(bytes: &[u8], count: &mut usize, unknown: &mut usize) -> Result<SceneObject> {
    let mut result = SceneObject::default();
    let mut name = None;
    for c in chunks(bytes, count)? {
        match c.kind {
            0x10 => set(&mut name, string(c.payload)?)?,
            0x4011 => set(&mut result.frame_type, uint(c.payload)?)?,
            0x2012 => set(&mut result.model, string(c.payload)?)?,
            0x20 => set(&mut result.position, numeric(c.payload)?)?,
            0x21 => set(&mut result.direction, numeric(c.payload)?)?,
            0x22 => {
                let q: [f32; 4] = numeric(c.payload)?;
                if q.iter().map(|n| (*n as f64).powi(2)).sum::<f64>() < 1e-20 {
                    return err("zero scene quaternion");
                }
                set(&mut result.rotation_wxyz, q)?;
            }
            0x2c => set(&mut result.world_position, numeric(c.payload)?)?,
            0x2d => set(&mut result.scale, numeric(c.payload)?)?,
            0x2a => set(&mut result.uniform_scale, numeric::<1>(c.payload)?[0])?,
            0x4020 => {
                for parent in chunks(c.payload, count)? {
                    if parent.kind == 0x10 {
                        set(&mut result.parent, string(parent.payload)?)?;
                    } else {
                        *unknown += 1;
                    }
                }
            }
            _ => *unknown += 1,
        }
    }
    result.name = name.ok_or_else(|| Error("scene object lacks name".into()))?;
    Ok(result)
}
fn visit(bytes: &[u8], depth: usize, count: &mut usize, scene: &mut Scene) -> Result<()> {
    if depth > 32 {
        return err("scene nesting quota exceeded");
    }
    for chunk in chunks(bytes, count)? {
        match chunk.kind {
            0x4010 => {
                if scene.objects.len() >= 16_384 {
                    return err("scene object quota exceeded");
                }
                scene
                    .objects
                    .push(object(chunk.payload, count, &mut scene.unknown_chunks)?);
            }
            0xae21 => {
                if scene.actors.len() >= 16_384 {
                    return err("scene actor quota exceeded");
                }
                let mut name = None;
                let mut kind = None;
                let mut state = None;
                for c in chunks(chunk.payload, count)? {
                    match c.kind {
                        0xae23 => set(&mut name, string(c.payload)?)?,
                        0xae22 => set(&mut kind, uint(c.payload)?)?,
                        0xae24 => {
                            set(&mut state, c.payload)?;
                            scene.unknown_chunks += 1;
                        }
                        _ => scene.unknown_chunks += 1,
                    }
                }
                let name = name.unwrap_or_default();
                if let (Some(kind), Some(state)) = (kind, state)
                    && let Some(code) = crate::script::parse_actor_script(kind, state)?
                {
                    if scene.scripts.len() >= 4096 {
                        return err("scene script quota exceeded");
                    }
                    scene.scripts.push(SceneScript {
                        name: name.clone(),
                        code,
                    });
                }
                scene.actors.push(SceneActor { name, kind });
            }
            0x4000 | 0xae20 | 0xae50 => visit(chunk.payload, depth + 1, count, scene)?,
            0xae51 => {
                if scene.scripts.len() >= 4096 {
                    return err("scene script quota exceeded");
                }
                let mut r = Reader {
                    b: chunk.payload,
                    p: 0,
                };
                r.u8()?;
                let name_len = r.u32()? as usize;
                if name_len > 4096 {
                    return err("scene script name quota exceeded");
                }
                let name = super::decode_windows1250(r.take(name_len)?)?;
                let code_len = r.u32()? as usize;
                if code_len > 1_000_000 {
                    return err("scene script code quota exceeded");
                }
                let code = super::decode_windows1250(r.take(code_len)?)?;
                scene.scripts.push(SceneScript { name, code });
            }
            _ => scene.unknown_chunks += 1,
        }
    }
    Ok(())
}
/// Decode bounded object placement metadata without executing proprietary scripts.
/// Unknown chunks are counted and skipped using their declared extent. Model names
/// remain source references (often `.i3d`); resolve them through trusted asset APIs.
pub fn parse_scene2(bytes: &[u8]) -> Result<Scene> {
    if bytes.len() > MAX_BYTES {
        return err("scene exceeds 64 MiB quota");
    }
    let mut count = 0;
    let roots = chunks(bytes, &mut count)?;
    if roots.len() != 1 || roots[0].kind != 0x4c53 {
        return err("expected single LS scene root");
    }
    let mut scene = Scene::default();
    visit(roots[0].payload, 0, &mut count, &mut scene)?;
    Ok(scene)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn chunk(kind: u16, bytes: &[u8]) -> Vec<u8> {
        [
            kind.to_le_bytes().as_slice(),
            &((bytes.len() + 6) as u32).to_le_bytes(),
            bytes,
        ]
        .concat()
    }
    fn scene(fields: &[u8]) -> Vec<u8> {
        chunk(0x4c53, &chunk(0x4000, &chunk(0x4010, fields)))
    }
    fn fixture() -> Vec<u8> {
        let fields = [
            chunk(0x10, b"synthetic\0"),
            chunk(0x2012, b"prop.i3d\0"),
            chunk(
                0x20,
                &[1f32.to_le_bytes(), 2f32.to_le_bytes(), 3f32.to_le_bytes()].concat(),
            ),
            chunk(0x4020, &chunk(0x10, b"Primary sector\0")),
            chunk(0xffff, &[1, 2, 3]),
        ]
        .concat();
        scene(&fields)
    }
    #[test]
    fn placement_unknown_and_every_truncation() {
        let bytes = fixture();
        let s = parse_scene2(&bytes).unwrap();
        assert_eq!(s.objects.len(), 1);
        assert_eq!(s.objects[0].position, Some([1., 2., 3.]));
        assert_eq!(s.objects[0].parent.as_deref(), Some("Primary sector"));
        assert_eq!(s.unknown_chunks, 1);
        for n in 0..bytes.len() {
            assert!(parse_scene2(&bytes[..n]).is_err(), "{n}");
        }
    }
    #[test]
    fn rejects_ambiguous_fields_invalid_floats_and_missing_terminator() {
        assert!(
            parse_scene2(&scene(&[chunk(0x10, b"x\0"), chunk(0x10, b"y\0")].concat())).is_err()
        );
        assert!(parse_scene2(&scene(&chunk(0x10, b"x"))).is_err());
        assert!(
            parse_scene2(&scene(
                &[chunk(0x10, b"x\0"), chunk(0x20, &[0xff; 12])].concat()
            ))
            .is_err()
        );
        let mut nested = chunk(0x4010, &chunk(0x10, b"x\0"));
        for _ in 0..34 {
            nested = chunk(0x4000, &nested);
        }
        assert!(parse_scene2(&chunk(0x4c53, &nested)).is_err());
    }
    #[test]
    fn script_records_and_known_actor_program_are_intake_only() {
        let mut program = vec![0];
        program.extend(4u32.to_le_bytes());
        program.extend(b"init");
        program.extend(4u32.to_le_bytes());
        program.extend(b"end\n");
        let mut actor = vec![1, 0, 0, 0, 0, 1];
        actor.extend(0u32.to_le_bytes());
        actor.extend(4u32.to_le_bytes());
        actor.extend(b"end\n");
        let bytes = chunk(
            0x4c53,
            &[
                chunk(0xae50, &chunk(0xae51, &program)),
                chunk(
                    0xae20,
                    &chunk(
                        0xae21,
                        &[
                            chunk(0xae23, b"trigger\0"),
                            chunk(0xae22, &5u32.to_le_bytes()),
                            chunk(0xae24, &actor),
                        ]
                        .concat(),
                    ),
                ),
            ]
            .concat(),
        );
        let result = parse_scene2(&bytes).unwrap();
        assert_eq!(result.scripts.len(), 2);
        assert_eq!(result.scripts[0].name, "init");
        assert_eq!(result.scripts[1].name, "trigger");
        assert_eq!(result.scripts[1].code, "end\n");
        for n in 0..bytes.len() {
            assert!(parse_scene2(&bytes[..n]).is_err(), "{n}");
        }
        program[1..5].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_scene2(&chunk(0x4c53, &chunk(0xae50, &chunk(0xae51, &program)))).is_err());
    }
}
