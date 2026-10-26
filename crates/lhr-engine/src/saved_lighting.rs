//! Source-backed saved lighting on actual owned LMAP resources.
//!
//! Pinned mafia-re 29b65b0, I3d_lit_object.cpp Load (1159..1445), Destruct
//! (1127..1135), and the hardware vertex-color Create branch (2121..2142).
//! The vertex-only entry accepts modes 1/5; the explicit general entry also
//! accepts mode 2 with its truecolor512/HWTL atlas and private index owner.
//! Native short-read UB, replacement of occupied nonactive LODs, and Game
//! scene2 payload dispatch remain unavailable. Buffers are eager for inspection;
//! native Load only stores colors and Create fills the buffer when rendered.
use super::*;
use crate::i3d_object::{ObjectCleanupPhase, ObjectVisualType, PlainLodKey};
#[path = "saved_lightmap.rs"]
pub(super) mod lightmap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum SavedLightingAdmission {
    VertexOnly,
    TrueColorLightmap512,
}

#[derive(Clone, Copy)]
struct SavedLodTarget {
    object: ObjectResourceKey,
    lighting: usize,
    index: usize,
    mesh_level: MeshLevelKey,
}

#[derive(Debug)]
pub struct SavedLightingPendingCleanup {
    /// Detached host LOD still retained after interrupted installation. It was
    /// never published into the object's lighting, and must not be rendered.
    pub lod: PlainLodKey,
    /// Secondary cleanup failure, distinct from the load's primary failure.
    pub error: LoadedVisualError,
}

#[derive(Debug)]
pub struct SavedLightingLoadError {
    /// Bytes actually consumed, including any partial final read.
    pub cursor: usize,
    pub error: LoadedVisualError,
    pub pending_cleanup: Option<SavedLightingPendingCleanup>,
}
impl std::fmt::Display for SavedLightingLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "saved lighting at byte {}: {:?}",
            self.cursor, self.error
        )?;
        if let Some(pending) = &self.pending_cleanup {
            write!(
                f,
                "; pending LOD {:?}, cleanup blocked: {:?}",
                pending.lod, pending.error
            )?;
        }
        Ok(())
    }
}
impl std::error::Error for SavedLightingLoadError {}

struct Input<'a> {
    bytes: &'a [u8],
    cursor: usize,
}
impl<'a> Input<'a> {
    fn read(&mut self, count: usize) -> Result<&'a [u8]> {
        let start = self.cursor;
        let available = self.bytes.len() - start;
        self.cursor += count.min(available);
        if count > available {
            return Err(LoadedVisualError::Backend("short saved-lighting read"));
        }
        Ok(&self.bytes[start..self.cursor])
    }
    fn word(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.read(4)?.try_into().unwrap()))
    }
}

impl LoadedVisualResources {
    /// Load saved vertex colors into an existing genuine Lit object's owner.
    /// The returned cursor allows a caller to retain its surrounding stream;
    /// trailing bytes are never consumed or interpreted as another chunk.
    ///
    /// Once Destruct runs, any error retains its actual mutation prefix and
    /// marks this resource owner incomplete. Parse/validation failures permit
    /// explicit cleanup. A backend failure can poison the manager and block
    /// further teardown; pending_cleanup then exposes any detached LOD whose
    /// cleanup was blocked. Neither case permits rendering or retrying Load.
    pub fn load_saved_vertex_lighting(
        &mut self,
        object: ObjectResourceKey,
        bytes: &[u8],
    ) -> std::result::Result<usize, SavedLightingLoadError> {
        self.load_saved_lighting_with_admission(object, bytes, SavedLightingAdmission::VertexOnly)
    }

    /// Saved Load with mixed modes 1/5/2. Rectangle mode selects the explicit
    /// truecolor512, hardware-vertex profile. Presentation must independently
    /// establish its real sector lighting context; loading makes no lighting
    /// assumption.
    pub fn load_saved_lighting(
        &mut self,
        object: ObjectResourceKey,
        bytes: &[u8],
    ) -> std::result::Result<usize, SavedLightingLoadError> {
        self.load_saved_lighting_with_admission(
            object,
            bytes,
            SavedLightingAdmission::TrueColorLightmap512,
        )
    }

    fn load_saved_lighting_with_admission(
        &mut self,
        object: ObjectResourceKey,
        bytes: &[u8],
        admission: SavedLightingAdmission,
    ) -> std::result::Result<usize, SavedLightingLoadError> {
        let mut input = Input { bytes, cursor: 0 };
        let initial = (|| {
            if self.incomplete {
                return Err(LoadedVisualError::IncompleteAdmission);
            }
            let record = self.objects.object(object)?;
            if record.visual_type() != ObjectVisualType::Lit
                || record.phase != ObjectCleanupPhase::Alive
            {
                return Err(LoadedVisualError::Backend(
                    "saved lighting requires a live Lit owner",
                ));
            }
            Ok(())
        })();
        if let Err(error) = initial {
            return Err(SavedLightingLoadError {
                cursor: 0,
                error,
                pending_cleanup: None,
            });
        }
        let mut pending_cleanup = None;
        let result = self.load_saved_vertex_lighting_inner(
            object,
            &mut input,
            &mut pending_cleanup,
            admission,
        );
        match result {
            Ok(()) => Ok(input.cursor),
            Err(error) => {
                self.incomplete = true;
                Err(SavedLightingLoadError {
                    cursor: input.cursor,
                    error,
                    pending_cleanup,
                })
            }
        }
    }

    fn load_saved_vertex_lighting_inner(
        &mut self,
        object: ObjectResourceKey,
        input: &mut Input<'_>,
        pending_cleanup: &mut Option<SavedLightingPendingCleanup>,
        admission: SavedLightingAdmission,
    ) -> Result<()> {
        self.objects
            .destruct_active_lighting(object, &mut self.backend)?;
        let mesh = self
            .objects
            .object(object)?
            .mesh
            .ok_or(LoadedVisualError::Backend("saved lighting requires a mesh"))?;
        let mesh = self
            .objects
            .managed()
            .resources()
            .mesh_object(mesh)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        let mesh_lod_count = mesh.num_lods;
        let mesh_levels = mesh.levels;
        let mesh_offsets = [mesh.u_ofs, mesh.v_ofs];
        if mesh_lod_count as usize > mesh_levels.len() {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let mask = input.read(1)?[0];
        for _ in 0..mask.count_ones() {
            let header = input.read(15)?;
            let version = header[0];
            let mode = u32::from(header[1]);
            let num_lods = u32::from_le_bytes(header[2..6].try_into().unwrap());
            let resolution = f32::from_le_bytes(header[6..10].try_into().unwrap());
            let angle = f32::from_le_bytes(header[10..14].try_into().unwrap());
            let number = if header[14] > 6 { 1 } else { header[14] };
            // Source zero indexes before its six-record array. It is outside
            // the admitted safe domain, not normalized into lighting one.
            if number == 0 {
                return Err(LoadedVisualError::Backend(
                    "saved lighting level zero unavailable",
                ));
            }
            let lighting = usize::from(number - 1);
            let level = self.objects.lit_level_mut(object, lighting)?;
            level.mode = mode;
            level.resolution = resolution;
            level.angle = angle;
            if version != 0x21 {
                return Err(LoadedVisualError::Backend(
                    "unsupported saved-lighting version",
                ));
            }
            if num_lods != mesh_lod_count {
                return Err(LoadedVisualError::Backend(
                    "saved lighting LOD count differs from mesh",
                ));
            }
            if mode != 1
                && mode != 5
                && !(mode == 2 && admission == SavedLightingAdmission::TrueColorLightmap512)
            {
                return Err(LoadedVisualError::Backend(
                    "saved lightmap/unknown mode requires its own constructed resource owner",
                ));
            }
            for (index, &level_key) in mesh_levels[..mesh_lod_count as usize].iter().enumerate() {
                // The original reads and retains no use of these flags.
                let _lod_flags = input.read(2)?;
                let level = self.objects.lit_level_mut(object, lighting)?;
                if level.lods[index].is_some() {
                    return Err(LoadedVisualError::Backend(
                        "overwriting a nonactive saved lighting LOD is unavailable",
                    ));
                }
                let level_key = level_key.ok_or(LoadedVisualError::InvalidGeometry)?;
                if mode == 2 {
                    if mesh_offsets != [0.; 2] {
                        return Err(LoadedVisualError::Backend(
                            "saved lightmap nonzero mesh UV offsets unavailable",
                        ));
                    }
                    self.load_saved_lightmap_lod(
                        SavedLodTarget {
                            object,
                            lighting,
                            index,
                            mesh_level: level_key,
                        },
                        input,
                        pending_cleanup,
                    )?;
                    continue;
                }
                let mesh_level = self
                    .objects
                    .managed()
                    .resources()
                    .mesh_level(level_key)
                    .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
                let count = mesh_level.num_vertices;
                if mode & 4 != 0 && input.word()? != count {
                    return Err(LoadedVisualError::Backend(
                        "saved lighting vertex count differs from mesh",
                    ));
                }
                let size = (count as usize)
                    .checked_mul(4)
                    .ok_or(LoadedVisualError::InvalidGeometry)?;
                let raw = input.read(size)?;
                let mut colors = Vec::new();
                colors.try_reserve_exact(count as usize).map_err(|_| {
                    LoadedVisualError::Backend("saved color allocation unavailable")
                })?;
                colors.extend(
                    raw.as_chunks::<4>()
                        .0
                        .iter()
                        .map(|word| u32::from_le_bytes(*word)),
                );
                self.install_lit_lod_geometry(
                    object,
                    lighting,
                    index,
                    level_key,
                    Some(colors.into_boxed_slice()),
                    pending_cleanup,
                )?;
            }
            self.objects.lit_level_mut(object, lighting)?.constructed = true;
        }
        // Native rendering lazily constructs these buffers. Keep the existing
        // host eager-inspection policy when Load leaves active lighting empty,
        // including mask zero and loads that only populate a nonactive level.
        let active = self
            .objects
            .object(object)?
            .lit_object()
            .unwrap()
            .active_level;
        if !self.objects.lit_level_mut(object, active)?.constructed {
            for (index, &level) in mesh_levels[..mesh_lod_count as usize].iter().enumerate() {
                if self.objects.lit_level_mut(object, active)?.lods[index].is_none() {
                    self.install_lit_lod_geometry(
                        object,
                        active,
                        index,
                        level.ok_or(LoadedVisualError::InvalidGeometry)?,
                        None,
                        pending_cleanup,
                    )?;
                }
            }
        }
        if admission == SavedLightingAdmission::TrueColorLightmap512 {
            self.objects.unlock_all_indices(&mut self.backend)?;
        }
        self.objects.unlock_all_vertices(&mut self.backend)?;
        Ok(())
    }

    fn install_lit_lod_geometry(
        &mut self,
        object: ObjectResourceKey,
        lighting: usize,
        index: usize,
        level: MeshLevelKey,
        colors: Option<Box<[u32]>>,
        pending_cleanup: &mut Option<SavedLightingPendingCleanup>,
    ) -> Result<()> {
        let source = self
            .objects
            .managed()
            .resources()
            .mesh_level(level)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        let count = source.num_vertices;
        if count == 0
            || colors
                .as_ref()
                .is_some_and(|colors| count as usize != colors.len())
        {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let descriptor = source.vertex_buffer;
        let source_owner = self
            .objects
            .managed()
            .vertex_bindings()
            .iter()
            .find(|binding| {
                binding.level == level
                    && binding.state == crate::managed_visual_resources::BindingState::Installed
            })
            .ok_or(LoadedVisualError::Backend(
                "saved lighting source vertex owner unavailable",
            ))?
            .owner;
        let layout = self
            .objects
            .managed()
            .vertex_storage_configuration(source_owner, descriptor, count)
            .map_err(|_| {
                LoadedVisualError::Backend("saved lighting source vertex layout unavailable")
            })?;
        if layout.fvf != 0x112 || layout.stride != Some(32) || layout.format != 6 {
            return Err(LoadedVisualError::Backend(
                "saved lighting requires original source vertices",
            ));
        }
        let source = self.inspection_graphics(
            GraphicsBuffer::Vertex(
                descriptor
                    .graphics
                    .ok_or(LoadedVisualError::InvalidGeometry)?,
            ),
            descriptor.base as usize * 32,
            count as usize * 32,
            Some(0x112),
        )?;
        let stride = if colors.is_some() { 24 } else { 32 };
        let mut vertices = bytes(count as usize * stride)?;
        for (index, (source, target)) in source
            .as_chunks::<32>()
            .0
            .iter()
            .zip(vertices.chunks_exact_mut(stride))
            .enumerate()
        {
            target[..12].copy_from_slice(&source[..12]);
            if let Some(colors) = &colors {
                target[12..16].copy_from_slice(&colors[index].to_le_bytes());
            }
            target[16..24].copy_from_slice(&source[24..32]);
        }
        let key = self.objects.create_pending_saved_lit_lod()?;
        let result = (|| {
            if !self.objects.allocate_plain_lod_vertices(
                key,
                VertexRequest {
                    fvf: if colors.is_some() { 0x142 } else { 0x242 },
                    usage: 0x8,
                    stride: stride as u32,
                    count,
                    format: if colors.is_some() { 0 } else { 1 },
                    exclusive: false,
                },
                &mut self.backend,
            )? {
                return Err(LoadedVisualError::Backend(
                    "saved lighting vertex allocation rejected",
                ));
            }
            let target = self.objects.lod(key)?.vertex_buffer;
            self.backend.upload_graphics(
                GraphicsBuffer::Vertex(target.graphics.ok_or(LoadedVisualError::InvalidGeometry)?),
                target.base as usize * stride,
                &vertices,
            )?;
            self.objects
                .publish_loaded_lit_lod(object, lighting, index, key, colors)?;
            Ok(())
        })();
        if let Err(error) = result {
            return Err(self.cleanup_failed_saved_lod(key, error, pending_cleanup));
        }
        Ok(())
    }

    fn cleanup_failed_saved_lod(
        &mut self,
        key: PlainLodKey,
        error: LoadedVisualError,
        pending_cleanup: &mut Option<SavedLightingPendingCleanup>,
    ) -> LoadedVisualError {
        // A poisoned manager cannot roll back an interrupted allocation. Keep
        // both its primary cause and the actual pending derived resource key.
        let cleanup_error = if self.objects.is_poisoned() {
            Some(LoadedVisualError::Object(ObjectError::Poisoned))
        } else {
            self.objects
                .destroy_pending_saved_lit_lod(key, &mut self.backend)
                .err()
                .map(Into::into)
        };
        if let Some(error) = cleanup_error {
            *pending_cleanup = Some(SavedLightingPendingCleanup { lod: key, error });
        }
        error
    }
}

impl crate::model_loading::ModelArena {
    /// Explicit standalone resource load. A future original Game loader must
    /// supply its evidenced payload; this API does not guess scene2 dispatch.
    pub fn load_frame_saved_vertex_lighting(
        &mut self,
        frame: crate::frame_transform::FrameKey,
        bytes: &[u8],
    ) -> std::result::Result<usize, SavedLightingLoadError> {
        self.load_frame_saved_lighting_with_admission(
            frame,
            bytes,
            SavedLightingAdmission::VertexOnly,
        )
    }

    /// Explicit source Load into an existing LMAP owner, including mode 2.
    /// This does not infer the original Game caller or its scene2 dispatch.
    pub fn load_frame_saved_lighting(
        &mut self,
        frame: crate::frame_transform::FrameKey,
        bytes: &[u8],
    ) -> std::result::Result<usize, SavedLightingLoadError> {
        self.load_frame_saved_lighting_with_admission(
            frame,
            bytes,
            SavedLightingAdmission::TrueColorLightmap512,
        )
    }

    fn load_frame_saved_lighting_with_admission(
        &mut self,
        frame: crate::frame_transform::FrameKey,
        bytes: &[u8],
        admission: SavedLightingAdmission,
    ) -> std::result::Result<usize, SavedLightingLoadError> {
        let object = self
            .frame_object_resources(frame)
            .map_err(|_| SavedLightingLoadError {
                cursor: 0,
                error: LoadedVisualError::Backend("saved lighting frame unavailable"),
                pending_cleanup: None,
            })?
            .ok_or(SavedLightingLoadError {
                cursor: 0,
                error: LoadedVisualError::Backend(
                    "saved lighting frame has no visual resource owner",
                ),
                pending_cleanup: None,
            })?;
        self.loaded_visual_resources_mut()
            .ok_or(SavedLightingLoadError {
                cursor: 0,
                error: LoadedVisualError::Backend("saved lighting resource owner unavailable"),
                pending_cleanup: None,
            })?
            .load_saved_lighting_with_admission(object, bytes, admission)
    }
}

#[cfg(test)]
#[path = "saved_lighting_tests.rs"]
mod tests;
