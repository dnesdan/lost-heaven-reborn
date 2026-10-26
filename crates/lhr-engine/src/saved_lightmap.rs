//! Source mode-2 Load/postProcess and its real HWTL vertex/index/atlas owners.
//! Pinned mafia-re 29b65b0, I3d_lit_object.cpp:1226–1405,2178–2188,2200–2298.
//! Lighting-sector evaluation belongs to presentation, not this saved payload.
use super::*;
use crate::{
    i3d_object::LitVertexInfo, lightmap_atlas::RectangleInput,
    managed_visual_resources::BindingState,
};

const RGB_INPUT_BUDGET: usize = 256 * 1024 * 1024;

fn vector<T>(count: usize) -> Result<Vec<T>> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| LoadedVisualError::Backend("saved lightmap allocation unavailable"))?;
    Ok(result)
}
fn rgb_extent(width: u32, height: u32) -> Result<usize> {
    if width == 0 || height == 0 || width > i16::MAX as u32 || height > i16::MAX as u32 {
        return Err(LoadedVisualError::Backend(
            "saved lightmap dimensions outside admitted signed range",
        ));
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(3))
        .filter(|n| *n <= RGB_INPUT_BUDGET)
        .ok_or(LoadedVisualError::Backend(
            "saved lightmap RGB extent unavailable",
        ))
}

impl LoadedVisualResources {
    pub(super) fn load_saved_lightmap_lod(
        &mut self,
        target: SavedLodTarget,
        input: &mut Input<'_>,
        pending_cleanup: &mut Option<SavedLightingPendingCleanup>,
    ) -> Result<()> {
        let lod = self.objects.create_pending_saved_lightmap_lod()?;
        let result = (|| {
            self.read_saved_lightmap_payload(lod, target.mesh_level, input)?;
            self.postprocess_saved_lightmap(lod, target.mesh_level)?;
            self.objects.lightmap_lod_mut(lod)?.ready = true;
            self.objects.publish_loaded_lit_lod(
                target.object,
                target.lighting,
                target.index,
                lod,
                None,
            )?;
            Ok(())
        })();
        if let Err(error) = result {
            return Err(self.cleanup_failed_saved_lod(lod, error, pending_cleanup));
        }
        Ok(())
    }

    fn read_saved_lightmap_payload(
        &mut self,
        lod: PlainLodKey,
        level_key: MeshLevelKey,
        input: &mut Input<'_>,
    ) -> Result<()> {
        let resources = self.objects.managed().resources();
        let level = resources
            .mesh_level(level_key)
            .map_err(|e| ObjectError::Managed(e.into()))?;
        let counts = input.read(4)?;
        let num_rectangles = u16::from_le_bytes(counts[..2].try_into().unwrap()) as usize;
        let num_groups = u16::from_le_bytes(counts[2..].try_into().unwrap()) as usize;
        if num_groups != level.num_fgroups as usize {
            return Err(LoadedVisualError::Backend(
                "saved lightmap face-group count differs from mesh",
            ));
        }
        let mut groups = vector(num_groups)?;
        let mut next = level.fgroups;
        let mut expected_indices = 0u32;
        for _ in 0..num_groups {
            let key = next.ok_or(LoadedVisualError::InvalidGeometry)?;
            let group = resources
                .face_group(key)
                .map_err(|e| ObjectError::Managed(e.into()))?;
            expected_indices = group
                .num_faces
                .checked_mul(3)
                .and_then(|count| expected_indices.checked_add(count))
                .filter(|count| *count <= i32::MAX as u32)
                .ok_or(LoadedVisualError::InvalidGeometry)?;
            groups.push(group.num_faces);
            next = group.next;
        }
        if next.is_some() {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        self.objects.lightmap_lod_mut(lod)?.pending_rectangles = vector(num_rectangles)?;
        let mut retained_rgb_bytes = 0usize;
        for _ in 0..num_rectangles {
            let single = input.read(1)?[0] != 0;
            let mut primary = if single {
                Some(RectangleInput::SingleColor {
                    argb: input.word()?,
                })
            } else {
                None
            };
            let maps = input.word()?;
            for map in 0..maps {
                let width = input.word()?;
                let height = input.word()?;
                // Source single-color records consume dimensions only: there
                // is no RGB body even when num_maps is nonzero.
                if !single {
                    let raw = input.read(rgb_extent(width, height)?)?;
                    if map == 0 {
                        if width == 3 && height == 3 {
                            return Err(LoadedVisualError::Backend(
                                "saved nonsingle lightmap cannot be 3x3",
                            ));
                        }
                        if width < 4 || height < 4 {
                            return Err(LoadedVisualError::Backend(
                                "saved lightmap dimensions below Construct's safe domain",
                            ));
                        }
                        // The first maps remain owned together until atlas
                        // construction. Apply the cumulative host budget only
                        // after the source read, before making another copy.
                        retained_rgb_bytes = retained_rgb_bytes
                            .checked_add(raw.len())
                            .filter(|&size| size <= RGB_INPUT_BUDGET)
                            .ok_or(LoadedVisualError::Backend(
                                "saved lightmap retained RGB input budget exceeded",
                            ))?;
                        let mut pixels = bytes(raw.len())?;
                        pixels.copy_from_slice(raw);
                        primary = Some(RectangleInput::Rgb {
                            width: width as u16,
                            height: height as u16,
                            pixels: pixels.into_boxed_slice(),
                        });
                    }
                }
            }
            self.objects
                .lightmap_lod_mut(lod)?
                .pending_rectangles
                .push(primary.ok_or(LoadedVisualError::Backend(
                    "saved nonsingle lightmap has no map",
                ))?);
        }

        let count = input.word()?;
        if count == 0 || count > 65_536 {
            return Err(LoadedVisualError::Backend(
                "saved lightmap lit vertex count outside 16-bit index domain",
            ));
        }
        self.objects.lightmap_lod_mut(lod)?.vertex_count = count;
        let raw = input.read(count as usize * 8)?;
        let mut uv = vector(count as usize)?;
        uv.extend(raw.as_chunks::<8>().0.iter().map(|pair| {
            [
                f32::from_le_bytes(pair[..4].try_into().unwrap()),
                f32::from_le_bytes(pair[4..].try_into().unwrap()),
            ]
        }));
        self.objects.lightmap_lod_mut(lod)?.lightmap_uv = uv.into_boxed_slice();
        let raw = input.read(count as usize * 4)?;
        let mut info = vector(count as usize)?;
        info.extend(raw.as_chunks::<4>().0.iter().map(|pair| LitVertexInfo {
            vertex: u16::from_le_bytes(pair[..2].try_into().unwrap()),
            rectangle: u16::from_le_bytes(pair[2..].try_into().unwrap()),
        }));
        self.objects.lightmap_lod_mut(lod)?.vertex_info = info.into_boxed_slice();

        let indices = input.word()?;
        if indices != expected_indices {
            return Err(LoadedVisualError::Backend(
                "saved lightmap index count differs from mesh",
            ));
        }
        if !self
            .objects
            .allocate_lit_lod_indices(lod, indices, &mut self.backend)?
        {
            return Err(LoadedVisualError::Backend(
                "saved lightmap index allocation rejected",
            ));
        }
        let raw = input.read(indices as usize * 2)?;
        if raw
            .as_chunks::<2>()
            .0
            .iter()
            .any(|index| u32::from(u16::from_le_bytes(*index)) >= count)
        {
            return Err(LoadedVisualError::Backend(
                "saved lightmap index exceeds expanded vertices",
            ));
        }
        let descriptor = self.objects.lightmap_lod(lod)?.unwrap().index_buffer;
        self.backend.upload_graphics(
            GraphicsBuffer::Index(
                descriptor
                    .graphics
                    .ok_or(LoadedVisualError::InvalidGeometry)?,
            ),
            descriptor.base as usize * 2,
            raw,
        )?;
        let assignments = &mut self.objects.lightmap_lod_mut(lod)?.face_rectangles;
        assignments.try_reserve_exact(num_groups).map_err(|_| {
            LoadedVisualError::Backend("saved lightmap face allocation unavailable")
        })?;
        for expected in groups {
            let faces = input.word()?;
            if faces != expected {
                return Err(LoadedVisualError::Backend(
                    "saved lightmap face count differs from group",
                ));
            }
            let raw = input.read(faces as usize * 2)?;
            let mut rectangles = vector(faces as usize)?;
            rectangles.extend(
                raw.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|word| u16::from_le_bytes(*word)),
            );
            // Loaded postProcess uses VI and the saved IB, not these face
            // assignments. Preserve their raw values for the same owner.
            assignments.push(rectangles.into_boxed_slice());
        }
        self.objects.allocate_lit_lightmap(lod)?;
        Ok(())
    }

    fn postprocess_saved_lightmap(
        &mut self,
        lod: PlainLodKey,
        level_key: MeshLevelKey,
    ) -> Result<()> {
        let level = self
            .objects
            .managed()
            .resources()
            .mesh_level(level_key)
            .map_err(|e| ObjectError::Managed(e.into()))?;
        let source_count = level.num_vertices;
        let descriptor = level.vertex_buffer;
        let owner = self
            .objects
            .managed()
            .vertex_bindings()
            .iter()
            .find(|binding| binding.level == level_key && binding.state == BindingState::Installed)
            .ok_or(LoadedVisualError::Backend(
                "saved lightmap source vertex owner unavailable",
            ))?
            .owner;
        let layout = self
            .objects
            .managed()
            .vertex_storage_configuration(owner, descriptor, source_count)
            .map_err(ObjectError::Managed)?;
        if layout.fvf != 0x112 || layout.stride != Some(32) || layout.format != 6 {
            return Err(LoadedVisualError::Backend(
                "saved lightmap requires original source vertices",
            ));
        }
        let source = self.inspection_graphics(
            GraphicsBuffer::Vertex(
                descriptor
                    .graphics
                    .ok_or(LoadedVisualError::InvalidGeometry)?,
            ),
            descriptor.base as usize * 32,
            source_count as usize * 32,
            Some(0x112),
        )?;
        let data = self.objects.lightmap_lod(lod)?.unwrap();
        let count = data.vertex_count;
        if data.vertex_info.len() != count as usize || data.lightmap_uv.len() != count as usize {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let atlas = self
            .objects
            .lightmap_atlases()
            .allocation(data.allocation.ok_or(LoadedVisualError::InvalidGeometry)?)
            .map_err(ObjectError::Lightmap)?;
        let su = 1.0f32 / atlas.texture.width as f32;
        let sv = 1.0f32 / atlas.texture.height as f32;
        let mut vertices = bytes(count as usize * 32)?;
        let mut base_uv = vector(count as usize)?;
        let mut lightmap_uv = vector(count as usize)?;
        for ((vertex, info), uv) in vertices
            .as_chunks_mut::<32>()
            .0
            .iter_mut()
            .zip(&data.vertex_info)
            .zip(&data.lightmap_uv)
        {
            // Source postProcess allocates its atlas before checking VI.rect.
            // Failed VI/UV validation must release that real allocation.
            let offset = usize::from(info.vertex) * 32;
            let source = source
                .get(offset..offset + 32)
                .ok_or(LoadedVisualError::Backend(
                    "saved lightmap source vertex outside mesh",
                ))?;
            let placement =
                atlas
                    .placements
                    .get(info.rectangle as usize)
                    .ok_or(LoadedVisualError::Backend(
                        "saved lightmap vertex rectangle outside atlas",
                    ))?;
            let origin = placement.source_uv_origin([atlas.texture.width, atlas.texture.height]);
            let [width, height] = placement.texture_size;
            let mapped = if !placement.rotated {
                [
                    (width - 1) as f32 * uv[0] * su + origin[0] + su * 0.5,
                    (height - 1) as f32 * uv[1] * sv + origin[1] + sv * 0.5,
                ]
            } else {
                [
                    (height - 1) as f32 * uv[1] * su + origin[0] - 2.0 + su * 0.5,
                    (width - 1) as f32 * uv[0] * sv + origin[1] + sv * 0.5,
                ]
            };
            let base = [
                f32::from_le_bytes(source[24..28].try_into().unwrap()),
                f32::from_le_bytes(source[28..32].try_into().unwrap()),
            ];
            if !mapped.into_iter().chain(base).all(f32::is_finite) {
                return Err(LoadedVisualError::Backend(
                    "saved lightmap UV outside finite presentation domain",
                ));
            }
            vertex[..12].copy_from_slice(&source[..12]);
            // Diffuse remains zero until an admitted presentation context
            // proves the zero-light/emission branch. Load does not invent a
            // sector or evaluate dynamic lighting from missing state.
            vertex[16..20].copy_from_slice(&mapped[0].to_le_bytes());
            vertex[20..24].copy_from_slice(&mapped[1].to_le_bytes());
            vertex[24..32].copy_from_slice(&source[24..32]);
            base_uv.push(base);
            lightmap_uv.push(mapped);
        }
        let data = self.objects.lightmap_lod_mut(lod)?;
        data.base_uv = base_uv.into_boxed_slice();
        data.lightmap_uv = lightmap_uv.into_boxed_slice();
        if !self.objects.allocate_plain_lod_vertices(
            lod,
            VertexRequest {
                fvf: 0x242,
                usage: 0x8,
                stride: 32,
                count,
                format: 1,
                exclusive: false,
            },
            &mut self.backend,
        )? {
            return Err(LoadedVisualError::Backend(
                "saved lightmap vertex allocation rejected",
            ));
        }
        let descriptor = self.objects.lod(lod)?.vertex_buffer;
        self.backend.upload_graphics(
            GraphicsBuffer::Vertex(
                descriptor
                    .graphics
                    .ok_or(LoadedVisualError::InvalidGeometry)?,
            ),
            descriptor.base as usize * 32,
            &vertices,
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "saved_lightmap_fixture.rs"]
pub(crate) mod fixture;
#[cfg(test)]
#[path = "saved_lightmap_tests.rs"]
pub(crate) mod tests;
