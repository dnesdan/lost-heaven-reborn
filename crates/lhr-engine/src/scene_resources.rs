//! Constructor-owned shared weather domain of I3D_scene (PC 0x48ab0).
//!
//! One owner corresponds to the original driver globals, rather than one snow
//! texture per scene. Driver CreateFrame and the derived destructor dependencies
//! are deliberately separate from this constructor continuation.
use crate::itexture::{
    CpuTextureStorage, TextureFactoryError, TextureImageSource, TextureKey, TextureResources,
};
use crate::model_loading::{ModelArena, ModelError};
use crate::scene_ownership::SceneKey;
use crate::scene_weather::{self, SCENE_WORDS, SECTOR_SOUND_PROPERTIES_WORDS};

/// Screen dimensions are queried in the native order, sy then sx. This is not
/// the viewport override performed later by the driver's CreateFrame caller.
pub trait SceneScreen {
    type Error;
    fn screen_sy(&mut self) -> Result<i32, Self::Error>;
    fn screen_sx(&mut self) -> Result<i32, Self::Error>;
}
#[derive(Debug)]
pub enum SceneConstructionError<E> {
    Model(ModelError),
    Screen(E),
    AlreadyStarted(SceneKey),
    Weather(scene_weather::WeatherResetUnavailable),
    TextureBoundary(TextureFactoryError),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneConstructionPhase {
    Viewport,
    Counted,
    WeatherReset,
    Complete,
}
#[derive(Debug)]
pub struct SceneConstruction {
    pub scene: SceneKey,
    arena_identity: crate::script_scheduler::FrameReferenceId,
    sectors: [crate::scene_ownership::SectorKey; 4],
    pub viewport: [i32; 4],
    pub viewport_projection: Option<SceneViewport>,
    driver_owner: Option<u64>,
    driver_registry_removed: bool,
    /// Host scratch for represented writes. Constructor-unwritten words are
    /// unavailable; use weather_word instead of treating scratch zero as native.
    pub words: [u32; SCENE_WORDS],
    pub phase: SceneConstructionPhase,
    close_complete: bool,
    sector_cursor: usize,
    count_released: bool,
    destroyed: bool,
    cpu_close_report: Option<SceneCpuCloseReport>,
}
impl SceneConstruction {
    /// Only original WeatherSystemReset stores are available in this view.
    pub fn weather_word(&self, byte_offset: usize) -> Option<u32> {
        if byte_offset.is_multiple_of(4)
            && (byte_offset == 0x298
                || byte_offset == 0x2a4
                || (0x2ac..=0x328).contains(&byte_offset))
        {
            Some(self.words[byte_offset / 4])
        } else {
            None
        }
    }
}
/// Original SetViewport (PC 0x4a9c0) outputs. The four integer arguments
/// are stored directly as rectangle boundaries; malformed rectangles are accepted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneViewport {
    pub rectangle: [i32; 4],
    pub half_extent: [f32; 2],
    pub center: [f32; 2],
}

/// Shared globals WSRefCnt, pSnowTexture and InitialSoundProp. Snow's factory
/// reference is transferred to this owner; scene construction acquires no
/// additional reference while pSnowTexture is already nonnull.
pub struct SceneResources {
    identity: u64,
    driver_domain: Option<u64>,
    ws_ref_count: i32,
    snow: Option<TextureKey>,
    initial_sound_properties: [u32; SECTOR_SOUND_PROPERTIES_WORDS],
    constructions: Vec<SceneConstruction>,
    frame_domain: Option<crate::script_scheduler::FrameReferenceId>,
    texture_domain: Option<u64>,
    storage_domain: Option<u64>,
}
impl Default for SceneResources {
    fn default() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_DOMAIN: AtomicU64 = AtomicU64::new(1);
        Self {
            identity: NEXT_DOMAIN
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .expect("scene resource identity exhausted"),
            driver_domain: None,
            ws_ref_count: 0,
            snow: None,
            initial_sound_properties: [0; SECTOR_SOUND_PROPERTIES_WORDS],
            constructions: Vec::new(),
            frame_domain: None,
            texture_domain: None,
            storage_domain: None,
        }
    }
}
impl SceneResources {
    pub fn identity(&self) -> u64 {
        self.identity
    }
    pub fn ws_ref_count(&self) -> i32 {
        self.ws_ref_count
    }
    pub fn snow_texture(&self) -> Option<TextureKey> {
        self.snow
    }
    pub fn initial_sound_properties(&self) -> &[u32; SECTOR_SOUND_PROPERTIES_WORDS] {
        &self.initial_sound_properties
    }
    pub fn cpu_close_report(&self, scene: SceneKey) -> Option<SceneCpuCloseReport> {
        self.construction(scene)
            .and_then(|entry| entry.cpu_close_report)
    }
    pub fn construction(&self, scene: SceneKey) -> Option<&SceneConstruction> {
        self.constructions.iter().find(|entry| entry.scene == scene)
    }

    /// SetViewport__9I3D_sceneFUiUiUiUi, PC 0x4a9c0, Game PC24/RN.
    /// Native returns zero for all 32-bit arguments: it does not normalize,
    /// clamp or validate the rectangle. Host availability errors are separate.
    pub fn set_viewport(
        &mut self,
        arena: &ModelArena,
        scene: SceneKey,
        rectangle: [i32; 4],
    ) -> Result<u32, SceneResourceError> {
        let entry = self
            .constructions
            .iter_mut()
            .find(|entry| entry.scene == scene)
            .ok_or(SceneResourceError::UnknownConstruction(scene))?;
        if entry.arena_identity != arena.lookup_reference(scene.root()) {
            return Err(SceneResourceError::UnsupportedDomain("foreign arena"));
        }
        arena.scene(scene)?;
        if entry.destroyed {
            return Err(SceneResourceError::AlreadyDestroyed(scene));
        }
        if entry.phase != SceneConstructionPhase::Complete {
            return Err(SceneResourceError::IncompleteConstruction(scene));
        }
        let [left, top, right, bottom] = rectangle;
        let rounded = |exact: f64| {
            let single = exact as f32;
            crate::native_precision::record_precision(f64::from(single) != exact);
            single
        };
        // FILD64 zero-extends the wrapping horizontal subtraction. FILD32
        // treats the wrapping vertical subtraction as signed. Integer loads
        // remain exact until each x87 arithmetic instruction rounds to PC24.
        let half_width = rounded(f64::from((right as u32).wrapping_sub(left as u32)) * 0.5);
        let half_height = rounded(f64::from(top.wrapping_sub(bottom)) * 0.5);
        let center_x = rounded(f64::from(left) + f64::from(half_width));
        let center_y = rounded(f64::from(top) - f64::from(half_height));
        entry.viewport = rectangle;
        entry.viewport_projection = Some(SceneViewport {
            rectangle,
            half_extent: [half_width, half_height],
            center: [center_x, center_y],
        });
        for (offset, value) in [
            (0x23c, left as u32),
            (0x240, top as u32),
            (0x244, right as u32),
            (0x248, bottom as u32),
            (0x208, half_width.to_bits()),
            (0x20c, half_height.to_bits()),
            (0x180, center_x.to_bits()),
            (0x184, center_y.to_bits()),
        ] {
            entry.words[offset / 4] = value;
        }
        Ok(0)
    }
    pub(crate) fn bind_driver_domain(&mut self, driver: u64) -> Result<(), SceneResourceError> {
        if self.driver_domain.is_some_and(|owner| owner != driver) {
            return Err(SceneResourceError::UnsupportedDomain(
                "shared resources already belong to another driver",
            ));
        }
        self.driver_domain = Some(driver);
        Ok(())
    }
    pub(crate) fn bind_driver_scene(
        &mut self,
        scene: SceneKey,
        driver: u64,
    ) -> Result<(), SceneResourceError> {
        if self.driver_domain != Some(driver) {
            return Err(SceneResourceError::UnsupportedDomain(
                "foreign driver resource domain",
            ));
        }
        let entry = self
            .constructions
            .iter_mut()
            .find(|entry| entry.scene == scene)
            .ok_or(SceneResourceError::UnknownConstruction(scene))?;
        if entry.destroyed || entry.driver_owner.is_some() {
            return Err(SceneResourceError::UnsupportedDomain(
                "scene already registered with a driver",
            ));
        }
        entry.driver_owner = Some(driver);
        Ok(())
    }
    /// Continue the existing actual arena scene from 0x48e29. A host failure
    /// preserves the executed prefix; it never rolls back scene or shared count.
    /// Reentry is rejected rather than replaying a native constructor prefix.
    pub fn continue_constructor<S: SceneScreen>(
        &mut self,
        arena: &ModelArena,
        scene: SceneKey,
        screen: &mut S,
        textures: &mut TextureResources,
        images: &mut impl TextureImageSource,
        storage: &mut CpuTextureStorage,
    ) -> Result<(), SceneConstructionError<S::Error>> {
        arena.scene(scene).map_err(SceneConstructionError::Model)?;
        let frame_domain = arena.lookup_reference(crate::frame_transform::FrameKey(0));
        if self
            .frame_domain
            .is_some_and(|domain| domain != frame_domain)
        {
            return Err(SceneConstructionError::Model(ModelError::ForeignArena));
        }
        self.frame_domain = Some(frame_domain);
        let texture_domain = textures.objects().managed().resources().identity();
        if self
            .texture_domain
            .is_some_and(|domain| domain != texture_domain)
        {
            return Err(SceneConstructionError::TextureBoundary(
                TextureFactoryError::Unsupported("foreign texture domain"),
            ));
        }
        if self
            .storage_domain
            .is_some_and(|domain| domain != storage.identity())
        {
            return Err(SceneConstructionError::TextureBoundary(
                TextureFactoryError::Unsupported("foreign CPU texture storage"),
            ));
        }
        self.texture_domain = Some(texture_domain);
        self.storage_domain = Some(storage.identity());
        if self.construction(scene).is_some() {
            return Err(SceneConstructionError::AlreadyStarted(scene));
        }
        let sy = screen.screen_sy().map_err(SceneConstructionError::Screen)?;
        let sx = screen.screen_sx().map_err(SceneConstructionError::Screen)?;
        self.constructions.push(SceneConstruction {
            scene,
            arena_identity: arena.lookup_reference(scene.root()),
            sectors: arena
                .scene(scene)
                .map_err(SceneConstructionError::Model)?
                .sectors,
            viewport: [0, 0, sx, sy],
            viewport_projection: None,
            driver_owner: None,
            driver_registry_removed: false,
            words: [0; SCENE_WORDS],
            phase: SceneConstructionPhase::Viewport,
            close_complete: false,
            sector_cursor: 0,
            count_released: false,
            destroyed: false,
            cpu_close_report: None,
        });
        let entry = self.constructions.last_mut().unwrap();
        self.ws_ref_count = self.ws_ref_count.wrapping_add(1);
        entry.phase = SceneConstructionPhase::Counted;
        entry.words[0x298 / 4] = 0;
        scene_weather::weather_system_reset(&mut entry.words)
            .map_err(SceneConstructionError::Weather)?;
        entry.phase = SceneConstructionPhase::WeatherReset;
        if self.snow.is_none() {
            match textures.create_texture_from_file(Some(b"Snow.tga"), None, 2, images, storage) {
                Ok(texture) => self.snow = Some(texture),
                // The native constructor ignores CreateITexture's result. A
                // missing/corrupt input leaves the global output null.
                Err(error) if error.is_native_creation_failure() => {}
                Err(error) => return Err(SceneConstructionError::TextureBoundary(error)),
            }
        }
        scene_weather::setup_sector_sound_properties_defaults(&mut self.initial_sound_properties);
        entry.phase = SceneConstructionPhase::Complete;
        Ok(())
    }
}

/// Sealed proof minted only after both actual CPU resource domains have
/// rejected managed residency. It authorizes a host cursor projection, not a
/// native device effect.
pub(crate) struct CpuManagedResidencyAbsent;

/// Structural proof for the bounded CPU resource projection. The original
/// device virtual call is not executed and earns no native Close parity credit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneCpuCloseReport {
    pub legacy_immediate_textures_before_close: usize,
    pub canonical_immediate_textures_before_close: usize,
    pub native_managed_residency_absent: bool,
    pub native_device_virtual14_executed: bool,
    pub canonical_managed_unload_executed: bool,
    pub loaded_vertex_then_index_reclamation_executed: bool,
}

/// Explicit failures of the bounded constructor-owned lifecycle are host
/// boundaries, not original HRESULTs. Completed destructor stages are retained.
#[derive(Debug)]
pub enum SceneResourceError {
    Model(ModelError),
    Texture(TextureFactoryError),
    UnknownConstruction(SceneKey),
    IncompleteConstruction(SceneKey),
    UnsupportedDomain(&'static str),
    AlreadyDestroyed(SceneKey),
}
impl From<ModelError> for SceneResourceError {
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}
impl SceneResources {
    fn check_domain(
        &self,
        arena: &ModelArena,
        scene: SceneKey,
        textures: &TextureResources,
        storage: &CpuTextureStorage,
    ) -> Result<(), SceneResourceError> {
        let entry = self
            .construction(scene)
            .ok_or(SceneResourceError::UnknownConstruction(scene))?;
        if entry.arena_identity != arena.lookup_reference(scene.root())
            || self.storage_domain != Some(storage.identity())
            || self.texture_domain != Some(textures.objects().managed().resources().identity())
        {
            return Err(SceneResourceError::UnsupportedDomain(
                "foreign scene/texture domain",
            ));
        }
        Ok(())
    }
    /// Close admitted constructor-owned state. Arrays absent in the original
    /// constructor stay genuinely unallocated. Foreign driver/listener/camera,
    /// managed CPU textures and unavailable derived resources are not admitted.
    /// Loaded vertex/index resources use the arena's actual owner, in native order.
    pub fn close_constructed_scene(
        &mut self,
        arena: &mut ModelArena,
        scene: SceneKey,
        textures: &mut TextureResources,
        storage: &mut CpuTextureStorage,
    ) -> Result<(), SceneResourceError> {
        use crate::scene_ownership::{
            SceneCloseBackend, SceneCloseEffect as Effect, SceneOwnerError,
        };
        let entry_index = self
            .constructions
            .iter()
            .position(|entry| entry.scene == scene)
            .ok_or(SceneResourceError::UnknownConstruction(scene))?;
        self.check_domain(arena, scene, textures, storage)?;
        if self.constructions[entry_index].destroyed {
            return Err(SceneResourceError::AlreadyDestroyed(scene));
        }
        if self.constructions[entry_index].phase != SceneConstructionPhase::Complete {
            return Err(SceneResourceError::IncompleteConstruction(scene));
        }
        if arena
            .loaded_visual_resources()
            .is_some_and(|owner| owner.backend().managed_texture_count() != 0)
        {
            return Err(SceneResourceError::UnsupportedDomain(
                "loaded CPU managed textures",
            ));
        }
        textures
            .prove_cpu_immediate_domain(storage)
            .map_err(SceneResourceError::Texture)?;
        for frame in arena.scene(scene)?.frames() {
            if matches!(
                arena.state(*frame)?.kind,
                crate::frame_transform::FrameKind::Sound
                    | crate::frame_transform::FrameKind::Scene
                    | crate::frame_transform::FrameKind::Sector
            ) {
                return Err(SceneResourceError::UnsupportedDomain(
                    "foreign derived scene-frame resources",
                ));
            }
        }
        let legacy_immediate_textures_before_close = arena
            .loaded_visual_resources()
            .map_or(0, |owner| owner.backend().live_textures());
        let canonical_immediate_textures_before_close =
            textures.cached().len() + textures.uncached().len();
        let loaded_buffers = arena.loaded_visual_resources().is_some();
        let sectors = arena.scene(scene)?.sectors;
        for sector in sectors {
            for &light in &arena.sector(sector)?.lights {
                if arena.state(light)?.kind != crate::frame_transform::FrameKind::Light {
                    return Err(SceneResourceError::UnsupportedDomain(
                        "noncanonical sector light",
                    ));
                }
                let light = arena.graph.light_state(light).map_err(ModelError::from)?;
                if (5..8).contains(&light.light_type) {
                    return Err(SceneResourceError::UnsupportedDomain(
                        "sector fog light cleanup",
                    ));
                }
            }
        }
        struct Backend<'a> {
            words: &'a mut [u32; SCENE_WORDS],
            textures: &'a mut TextureResources,
            storage: &'a mut CpuTextureStorage,
            completed: Option<Effect>,
            failure: Option<TextureFactoryError>,
        }
        impl SceneCloseBackend for Backend<'_> {
            fn close_effect(
                &mut self,
                _: &ModelArena,
                scene: SceneKey,
                effect: Effect,
            ) -> Result<(), ModelError> {
                if self.completed == Some(effect) {
                    self.completed = None;
                    return Ok(());
                }
                match effect {
                    Effect::UnloadManagedTextures => {
                        if let Err(error) =
                            self.textures.unload_cpu_immediate_textures(self.storage)
                        {
                            self.failure = Some(error);
                            return Err(SceneOwnerError::BackendUnavailable(effect).into());
                        }
                    }
                    Effect::FreeActiveAreas
                    | Effect::DeletePrimarySounds
                    | Effect::ClearDriverSectors => {
                        // Global arrays and primary sound membership remain
                        // null in this constructor-only admitted domain.
                    }
                    Effect::ReleaseLensflares => self.words[0x288 / 4..=0x290 / 4].fill(0),
                    Effect::ClearTickList => self.words[0x148 / 4..=0x150 / 4].fill(0),
                    Effect::DeleteInterpolation => self.words[0x178 / 4] = 0,
                    Effect::ClearCameraAndSectorState => {}
                    Effect::ResetPrimarySound
                    | Effect::FreeUnusedVertexBuffers
                    | Effect::FreeUnusedIndexBuffers => {
                        return Err(SceneOwnerError::BackendUnavailable(effect).into());
                    }
                    Effect::DeviceVirtual14 => {
                        // Preserve the low-level missing-device failure. The
                        // outer CPU composition explicitly projects this stage.
                        return Err(SceneOwnerError::BackendUnavailable(effect).into());
                    }
                    Effect::ClearShadowBackPointers => self.words[0x160 / 4] = 0,
                }
                let _ = scene;
                Ok(())
            }
        }
        let mut backend = Backend {
            words: &mut self.constructions[entry_index].words,
            textures,
            storage,
            completed: None,
            failure: None,
        };
        loop {
            match arena.close_scene_with_backend(scene, &mut backend) {
                Ok(()) => break,
                Err(ModelError::Scene(SceneOwnerError::BackendUnavailable(effect))) => {
                    if let Some(error) = backend.failure.take() {
                        return Err(SceneResourceError::Texture(error));
                    }
                    match effect {
                        Effect::DeviceVirtual14 => {
                            arena.project_scene_cpu_device_boundary(
                                scene,
                                CpuManagedResidencyAbsent,
                            )?;
                            continue;
                        }
                        Effect::ResetPrimarySound => {
                            let primary = sectors[0];
                            let sector = arena
                                .sectors
                                .iter_mut()
                                .find(|sector| sector.key == primary)
                                .unwrap();
                            sector.sound_words_1e8_1ec = [0; 2];
                            scene_weather::setup_sector_sound_properties_defaults(
                                &mut sector.sound_properties,
                            );
                        }
                        Effect::FreeUnusedVertexBuffers => {
                            arena.reclaim_unused_visual_vertices()?
                        }
                        Effect::FreeUnusedIndexBuffers => arena.reclaim_unused_visual_indices()?,
                        _ => {
                            return Err(SceneResourceError::Model(
                                SceneOwnerError::BackendUnavailable(effect).into(),
                            ));
                        }
                    }
                    backend.completed = Some(effect);
                }
                Err(error) => return Err(SceneResourceError::Model(error)),
            }
        }
        self.constructions[entry_index].cpu_close_report = Some(SceneCpuCloseReport {
            legacy_immediate_textures_before_close,
            canonical_immediate_textures_before_close,
            native_managed_residency_absent: true,
            native_device_virtual14_executed: false,
            canonical_managed_unload_executed: true,
            loaded_vertex_then_index_reclamation_executed: loaded_buffers,
        });
        Ok(())
    }
}

/// Borrowed driver identity and its actual registry removal effect at the
/// scene destructor's post-weather boundary.
struct DriverSceneRemoval<'a> {
    driver: u64,
    unregister: &'a mut dyn FnMut(SceneKey) -> Result<(), SceneResourceError>,
}

impl SceneResources {
    /// Release a constructor-sector reference retained by an external caller.
    /// A surviving sector retains the native stale scene pointer until its own
    /// destructor, whose null ring links avoid dereferencing the retired scene.
    pub fn release_retained_sector(
        &mut self,
        arena: &mut ModelArena,
        sector: crate::scene_ownership::SectorKey,
    ) -> Result<(), SceneResourceError> {
        let entry = self
            .constructions
            .iter()
            .find(|entry| entry.sectors.contains(&sector))
            .ok_or(SceneResourceError::UnsupportedDomain("foreign sector"))?;
        if entry.arena_identity != arena.lookup_reference(entry.scene.root()) {
            return Err(SceneResourceError::UnsupportedDomain("foreign arena"));
        }
        let references = arena.frame(sector.frame())?.references;
        if references > 1 {
            arena.release(sector.frame())?;
            return Ok(());
        }
        for &light in &arena.sector(sector)?.lights {
            arena.graph.light_state(light).map_err(ModelError::from)?;
        }
        if arena.frame(sector.frame())?.owner.is_some() {
            return Err(SceneResourceError::UnsupportedDomain(
                "sector membership in another owner",
            ));
        }
        arena
            .graph
            .check_empty_scene_base_destruction(sector.frame())
            .map_err(ModelError::from)?;
        // 4ee80 entry and the actual nonnull-scene -> null transition in
        // 4f260(NULL), with constructor ring links and portal arrays null.
        arena.sector_version = arena.sector_version.wrapping_add(2);
        // Actual 4ee80 removes each raw sector pointer from the light's list
        // without invoking SectorDeleteLight or incrementing light-change state.
        if let Some(occluder) = arena.sector(sector)?.hull_occluder {
            arena.clear_occluder_sector_backpointer(occluder)?;
        }
        let lights = arena.sector(sector)?.lights.clone();
        for light in lights {
            let reciprocal = &mut arena
                .graph
                .light_state_mut(light)
                .map_err(ModelError::from)?
                .sectors;
            if let Some(index) = reciprocal.iter().position(|&value| value == sector.frame()) {
                reciprocal.swap_remove(index);
            }
        }
        let si = arena
            .sectors
            .iter()
            .position(|value| value.key == sector)
            .unwrap();
        arena.sectors[si].hull_occluder = None;
        arena.sectors[si].lights = Vec::new();

        arena.destroy_empty_scene_base(sector.frame())?;
        arena.sectors.retain(|entry| entry.key != sector);
        Ok(())
    }
    /// Last-reference Scene destructor for the constructor-only domain. Close
    /// retains Snow; only the signed last-scene condition releases its owned
    /// reference. External Snow and sector references remain independent.
    pub fn release_scene(
        &mut self,
        arena: &mut ModelArena,
        scene: SceneKey,
        textures: &mut TextureResources,
        storage: &mut CpuTextureStorage,
    ) -> Result<(), SceneResourceError> {
        self.release_scene_inner(arena, scene, textures, storage, None)
    }
    pub(crate) fn release_driver_scene(
        &mut self,
        arena: &mut ModelArena,
        scene: SceneKey,
        textures: &mut TextureResources,
        storage: &mut CpuTextureStorage,
        driver: u64,
        unregister: &mut dyn FnMut(SceneKey) -> Result<(), SceneResourceError>,
    ) -> Result<(), SceneResourceError> {
        self.release_scene_inner(
            arena,
            scene,
            textures,
            storage,
            Some(DriverSceneRemoval { driver, unregister }),
        )
    }
    fn release_scene_inner(
        &mut self,
        arena: &mut ModelArena,
        scene: SceneKey,
        textures: &mut TextureResources,
        storage: &mut CpuTextureStorage,
        mut driver: Option<DriverSceneRemoval<'_>>,
    ) -> Result<(), SceneResourceError> {
        let index = self
            .constructions
            .iter()
            .position(|entry| entry.scene == scene)
            .ok_or(SceneResourceError::UnknownConstruction(scene))?;
        self.check_domain(arena, scene, textures, storage)?;
        if self.constructions[index].destroyed {
            return Err(SceneResourceError::AlreadyDestroyed(scene));
        }
        if self.constructions[index].driver_owner != driver.as_ref().map(|entry| entry.driver) {
            return Err(SceneResourceError::UnsupportedDomain(
                "scene requires its owning driver registry",
            ));
        }
        if arena.frame(scene.root())?.owner.is_some() {
            return Err(SceneResourceError::UnsupportedDomain(
                "scene membership in another owner",
            ));
        }
        if arena.frame(scene.root())?.references > 1 {
            arena.release(scene.root())?;
            return Ok(());
        }
        if !self.constructions[index].close_complete {
            self.close_constructed_scene(arena, scene, textures, storage)?;
            self.constructions[index].close_complete = true;
        }
        // Scene destructor releases its independent active-camera reference
        // after Close and before its four retained sectors.
        arena.scene_set_active_camera(scene, None)?;
        // Camera and listener are null in the admitted absent-native-sound-driver
        // domain. Querying or creating a native listener remains an explicit
        // dependency boundary in DriverSceneOwner.
        let sectors = arena.scene(scene)?.sectors;
        while self.constructions[index].sector_cursor < sectors.len() {
            let sector = sectors[self.constructions[index].sector_cursor];
            self.release_retained_sector(arena, sector)?;
            self.constructions[index].sector_cursor += 1;
        }
        if !self.constructions[index].count_released {
            self.ws_ref_count = self.ws_ref_count.wrapping_sub(1);
            self.constructions[index].count_released = true;
        }
        if self.ws_ref_count <= 0 {
            if let Some(snow) = self.snow {
                textures
                    .release_texture(snow, storage)
                    .map_err(TextureFactoryError::from)
                    .map_err(SceneResourceError::Texture)?;
                self.snow = None;
            }
            // WSComplVBStr is constructor-global null in this admitted domain;
            // no weather rendering/allocation API can populate it here.
        }
        if !self.constructions[index].driver_registry_removed {
            if let Some(removal) = driver.as_mut() {
                (removal.unregister)(scene)?;
            }
            self.constructions[index].driver_registry_removed = true;
        }
        let entry = &mut self.constructions[index];
        for offset in [0x32c, 0x29c, 0x288, 0x22c, 0x168, 0x158, 0x148] {
            entry.words[offset / 4..offset / 4 + 3].fill(0);
        }
        arena.destroy_empty_scene_base(scene.root())?;
        arena.scenes.retain(|entry| entry.key != scene);
        entry.destroyed = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itexture::TextureImageError;
    struct Screen(Vec<&'static str>);
    impl SceneScreen for Screen {
        type Error = ();
        fn screen_sy(&mut self) -> Result<i32, ()> {
            self.0.push("sy");
            Ok(600)
        }
        fn screen_sx(&mut self) -> Result<i32, ()> {
            self.0.push("sx");
            Ok(800)
        }
    }
    struct Images {
        reads: usize,
        missing: bool,
    }
    impl TextureImageSource for Images {
        fn read_image(&mut self, path: &[u8]) -> Result<Vec<u8>, TextureImageError> {
            assert_eq!(path, b"Maps\\Snow.tga");
            self.reads += 1;
            if self.missing {
                return Err(TextureImageError::Missing);
            }
            let mut image = vec![0u8; 18];
            image[2] = 2;
            image[12] = 2;
            image[14] = 2;
            image[16] = 32;
            image[17] = 0x28;
            image.extend_from_slice(&[20, 40, 60, 128].repeat(4));
            Ok(image)
        }
    }
    #[test]
    fn two_scenes_share_the_factory_reference_and_only_last_destruction_releases_it() {
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        precision.run(|| {
            let mut arena = ModelArena::default();
            let mut shared = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut images = Images {
                reads: 0,
                missing: false,
            };
            for _ in 0..3 {
                let mut screen = Screen(Vec::new());
                let first = arena.construct_scene_owner_prefix().unwrap();
                shared
                    .continue_constructor(
                        &arena,
                        first,
                        &mut screen,
                        &mut textures,
                        &mut images,
                        &mut storage,
                    )
                    .unwrap();
                let snow = shared.snow_texture().unwrap();
                let second = arena.construct_scene_owner_prefix().unwrap();
                shared
                    .continue_constructor(
                        &arena,
                        second,
                        &mut screen,
                        &mut textures,
                        &mut images,
                        &mut storage,
                    )
                    .unwrap();
                assert_eq!(screen.0, ["sy", "sx", "sy", "sx"]);
                assert_eq!(
                    shared.construction(first).unwrap().viewport,
                    [0, 0, 800, 600]
                );
                assert_eq!(shared.ws_ref_count(), 2);
                assert_eq!(shared.snow_texture(), Some(snow));
                assert_eq!(textures.texture(snow).unwrap().references, 1);
                shared
                    .close_constructed_scene(&mut arena, first, &mut textures, &mut storage)
                    .unwrap();
                assert_eq!(shared.snow_texture(), Some(snow));
                assert_eq!(shared.ws_ref_count(), 2);
                shared
                    .release_scene(&mut arena, first, &mut textures, &mut storage)
                    .unwrap();
                assert_eq!(shared.ws_ref_count(), 1);
                assert!(arena.frame(first.root()).is_err());
                assert_eq!(textures.texture(snow).unwrap().references, 1);
                shared
                    .release_scene(&mut arena, second, &mut textures, &mut storage)
                    .unwrap();
                assert_eq!(shared.ws_ref_count(), 0);
                assert_eq!(shared.snow_texture(), None);
                assert!(textures.texture(snow).is_err());
                assert_eq!(
                    (
                        storage.live_graphics(),
                        storage.live_pixels(),
                        storage.live_strings()
                    ),
                    (0, 0, 0)
                );
            }
            assert_eq!(images.reads, 3);
            assert_eq!(arena.sector_version, 72);
        });
    }
    #[test]
    fn external_snow_reference_survives_the_last_scene() {
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        precision.run(|| {
            let mut arena = ModelArena::default();
            let mut shared = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            shared
                .continue_constructor(
                    &arena,
                    scene,
                    &mut Screen(Vec::new()),
                    &mut textures,
                    &mut Images {
                        reads: 0,
                        missing: false,
                    },
                    &mut storage,
                )
                .unwrap();
            let snow = shared.snow_texture().unwrap();
            textures.retain_texture(snow).unwrap();
            shared
                .release_scene(&mut arena, scene, &mut textures, &mut storage)
                .unwrap();
            assert_eq!(shared.snow_texture(), None);
            assert_eq!(textures.texture(snow).unwrap().references, 1);
            assert_eq!(storage.live_graphics(), 1);
            textures.release_texture(snow, &mut storage).unwrap();
            assert_eq!((storage.live_graphics(), storage.live_strings()), (0, 0));
        });
    }
    #[test]
    fn unchecked_missing_snow_still_completes_and_retries_on_next_scene() {
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        precision.run(|| {
            let mut arena = ModelArena::default();
            let mut shared = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut images = Images {
                reads: 0,
                missing: true,
            };
            for _ in 0..2 {
                let scene = arena.construct_scene_owner_prefix().unwrap();
                shared
                    .continue_constructor(
                        &arena,
                        scene,
                        &mut Screen(Vec::new()),
                        &mut textures,
                        &mut images,
                        &mut storage,
                    )
                    .unwrap();
                assert_eq!(
                    shared.construction(scene).unwrap().phase,
                    SceneConstructionPhase::Complete
                );
                assert_eq!(shared.initial_sound_properties()[0], 1f32.to_bits());
                shared
                    .release_scene(&mut arena, scene, &mut textures, &mut storage)
                    .unwrap();
            }
            assert_eq!(images.reads, 2);
            assert_eq!(shared.ws_ref_count(), 0);
            assert_eq!((storage.live_graphics(), storage.live_strings()), (0, 0));
        });
    }
    #[test]
    fn retained_sector_is_not_destroyed_by_scene_and_can_be_released_later() {
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        precision.run(|| {
            let mut arena = ModelArena::default();
            let mut shared = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let sector = arena.scene(scene).unwrap().sectors[0];
            shared
                .continue_constructor(
                    &arena,
                    scene,
                    &mut Screen(Vec::new()),
                    &mut textures,
                    &mut Images {
                        reads: 0,
                        missing: false,
                    },
                    &mut storage,
                )
                .unwrap();
            arena.retain(sector.frame()).unwrap();
            shared
                .release_scene(&mut arena, scene, &mut textures, &mut storage)
                .unwrap();
            assert_eq!(arena.frame(sector.frame()).unwrap().references, 1);
            assert_eq!(arena.sector(sector).unwrap().scene, scene);
            assert_eq!(arena.global_sector_version(), 10);
            shared.release_retained_sector(&mut arena, sector).unwrap();
            assert!(arena.frame(sector.frame()).is_err());
            assert_eq!(arena.global_sector_version(), 12);
        });
    }
    #[test]
    fn foreign_texture_domain_is_rejected_before_close_or_count_release() {
        let mut precision = crate::native_precision::Pc24PrecisionState::default();
        precision.run(|| {
            let mut arena = ModelArena::default();
            let mut shared = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            shared
                .continue_constructor(
                    &arena,
                    scene,
                    &mut Screen(Vec::new()),
                    &mut textures,
                    &mut Images {
                        reads: 0,
                        missing: false,
                    },
                    &mut storage,
                )
                .unwrap();
            let mut foreign = TextureResources::new().unwrap();
            assert!(matches!(
                shared.release_scene(&mut arena, scene, &mut foreign, &mut storage),
                Err(SceneResourceError::UnsupportedDomain(_))
            ));
            assert_eq!(shared.ws_ref_count(), 1);
            assert_eq!(
                arena.scene(scene).unwrap().close_phase(),
                crate::scene_ownership::ClosePhase::Idle
            );
            shared
                .release_scene(&mut arena, scene, &mut textures, &mut storage)
                .unwrap();
        });
    }
}
