//! Scene2 intake for the standalone driving caller's actual shared Scene.
//! Mission scripts, actors and original MissionOpen scheduling remain separate.
use super::*;
use crate::mission_resources::{CpuMissionCacheMemory, MissionCacheKind, MissionCaches};
use crate::mission_scene_loading::{MissionSceneLoadError, MissionSceneLoader};

pub(super) struct DrivingSceneMission {
    pub loader: MissionSceneLoader,
    pub frontier: Option<MissionSceneLoadError>,
    pub(super) caches: MissionCaches,
    pub(super) memory: CpuMissionCacheMemory,
}

impl CarRuntime {
    /// Explicit inspection selection over the existing world cache metadata.
    /// Selected models and subsequent visibility changes use actual Scene keys;
    /// no legacy render snapshot is appended to the canonical draw list.
    pub fn refresh_scene_city_cache(&mut self, position: [f32; 3]) -> Result<(), String> {
        Pc24PrecisionState::run_owner(
            self,
            |owner| &mut owner.precision,
            |owner| owner.refresh_scene_city_cache_inner(position),
        )
    }
    pub(super) fn refresh_scene_city_cache_inner(
        &mut self,
        position: [f32; 3],
    ) -> Result<(), String> {
        if !self.has_canonical_scene_world() {
            return Ok(());
        }
        if position.iter().any(|v| !v.is_finite() || v.abs() > 1.0e8) {
            return Err("City-cache inspection position is outside its supported range".into());
        }
        let bucket = [
            (position[0] / 25.).floor() as i32,
            (position[2] / 25.).floor() as i32,
        ];
        let scene = self.scene.ok_or("Canonical driving Scene absent")?;
        let session = self.driving.as_mut().ok_or("Driving cache source absent")?;
        if self.scene_cache_bucket != Some(bucket) {
            session
                .world
                .load_cache_render_near(&self.installation, position, 300.)?;
            let mission = self
                .scene_mission
                .as_mut()
                .ok_or("Driving model-cache owner absent")?;
            let adopted = self.scene_city_cache.adopt(
                &mut self.arena,
                scene,
                &mut mission.caches,
                &mut mission.memory,
                &self.installation,
                &session.world,
            )?;
            if adopted.adopted != 0 || adopted.unsupported != 0 {
                println!("Canonical city cache: {} placements adopted, {} explicitly unsupported; {} total actual Scene models",adopted.adopted,adopted.unsupported,self.scene_city_cache.placement_count());
            }
            self.scene_cache_bucket = Some(bucket);
        }
        self.scene_city_cache
            .sync_visibility(&mut self.arena, &session.world)?;
        self.arena
            .publish_runtime_buffers()
            .map_err(|e| format!("Canonical cache publication: {e:?}"))?;
        Ok(())
    }
    pub(super) fn load_driving_scene2(
        &mut self,
        mission: &str,
        options: CarRuntimeOptions,
    ) -> Result<(), String> {
        let scene = self.scene.ok_or("Driving Scene absent")?;
        self.scene_mission = Some(DrivingSceneMission {
            loader: MissionSceneLoader::new(&self.arena, scene).map_err(|e| e.detail)?,
            frontier: None,
            caches: MissionCaches::empty().map_err(|e| format!("Driving scene cache: {e:?}"))?,
            memory: Default::default(),
        });
        let state = self.scene_mission.as_mut().unwrap();
        // Game startup supplies Sounds before the model directory. The cache
        // open path applies the source's 0x10 Sound Open flag.
        state
            .caches
            .add_dir(MissionCacheKind::Sound, b"Sounds", &mut state.memory)
            .map_err(|e| format!("Driving scene sound directory: {e:?}"))?;
        state
            .caches
            .add_dir(MissionCacheKind::Model, b"Models", &mut state.memory)
            .map_err(|e| format!("Driving scene model directory: {e:?}"))?;
        let bytes = self
            .installation
            .read(&format!("missions/{mission}/scene2.bin"))
            .map_err(|e| format!("Driving scene2 input: {e}"))?;
        let [width, height] = options.scene_screen_size;
        // This host inspection caller supplies the ordinary screen mode.
        // A stopped prefix stays owned and inspectable; it is not MissionOpen.
        state.frontier = state
            .loader
            .load_prefix_with_resource_cache(
                &mut self.scene_driver,
                &mut self.arena,
                &self.scene_resources,
                &mut InspectionScreen {
                    size: options.scene_screen_size,
                    viewport: options.scene_viewport.unwrap_or([0, 0, width, height]),
                },
                crate::game_camera_projection::GameCameraProjection {
                    global_mode: 0,
                    screen_flag_46f4: false,
                },
                &bytes,
                &mut state.caches,
                &self.installation,
                &mut state.memory,
            )
            .err();
        self.arena
            .publish_runtime_buffers()
            .map_err(|e| format!("Driving world publication: {e:?}"))?;
        Ok(())
    }

    pub(super) fn close_driving_scene_caches(&mut self) -> Result<(), String> {
        if let Some(state) = &mut self.scene_mission {
            for kind in [
                MissionCacheKind::Model,
                MissionCacheKind::Sound,
                MissionCacheKind::Animation,
            ] {
                state
                    .caches
                    .destruct(kind, &mut self.arena, &mut state.memory)
                    .map_err(|e| format!("Driving scene cache cleanup: {e:?}"))?;
            }
            if state.memory.live_allocations() != 0 || state.memory.live_bytes() != 0 {
                return Err("Driving scene cache retained allocations".into());
            }
            // Scene Close reclaims before this caller drops cached model
            // sources. Reclaim their now-unused shared buffers afterwards.
            self.arena
                .reclaim_unused_visual_buffers()
                .map_err(|e| format!("Driving cached visual reclamation: {e:?}"))?;
        }
        self.scene_mission = None;
        Ok(())
    }
}
