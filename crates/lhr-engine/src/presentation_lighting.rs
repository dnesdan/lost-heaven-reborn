//! Read-only presentation images, scoped to the actual arena's source owners.
//! This cache owns no source lightmap, allocation lease or gameplay state.
#![forbid(unsafe_code)]
use crate::i3d_object::lightmap_atlas::{LightmapAtlas, LightmapAtlasOwner};
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock, Weak},
};

struct AtlasImage {
    source: Weak<RwLock<LightmapAtlas>>,
    revision: u64,
    texture: Arc<lhr_render::Texture>,
}
impl std::fmt::Debug for AtlasImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AtlasImage")
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default)]
pub(crate) struct PresentationLighting {
    atlases: BTreeMap<usize, AtlasImage>,
}
impl PresentationLighting {
    pub(crate) fn prune(&mut self) {
        self.atlases
            .retain(|_, image| image.source.strong_count() != 0);
    }

    /// The caller holds the source read lock through revision and pixel reads.
    /// Old snapshots keep their immutable image when the shared atlas changes.
    pub(crate) fn atlas_image(
        &mut self,
        source: &LightmapAtlasOwner,
        atlas: &LightmapAtlas,
    ) -> Result<Arc<lhr_render::Texture>, String> {
        // A Weak keeps the allocation identity reserved even after its last
        // strong source reference dies; unrelated atlases cannot reuse a key.
        let key = Arc::as_ptr(source) as usize;
        if let Some(image) = self.atlases.get(&key) {
            if image.source.ptr_eq(&Arc::downgrade(source)) && image.revision == atlas.revision {
                return Ok(Arc::clone(&image.texture));
            }
        }
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(atlas.rgba.len())
            .map_err(|_| "Saved lightmap atlas allocation unavailable")?;
        rgba.extend_from_slice(&atlas.rgba);
        let texture = Arc::new(lhr_render::Texture {
            width: atlas.width,
            height: atlas.height,
            rgba,
            repeat: [false; 2],
        });
        self.atlases.insert(
            key,
            AtlasImage {
                source: Arc::downgrade(source),
                revision: atlas.revision,
                texture: Arc::clone(&texture),
            },
        );
        Ok(texture)
    }
}
