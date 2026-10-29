//! Mission weather preparation on the actual constructor-owned Scene state.
//! Reference: mafia-re dc867c3238488de27d094bb79b6550e392566e3c,
//! C_mission::Open (GAME 0x5409d0) and I3D_scene::SetWeatherSystemParam
//! (LS3DF 0x1004bce0, ON branch only). The existing reset owns PC24 numerics.
//! This is host composition, not a complete MissionOpen or weather renderer.
#![forbid(unsafe_code)]

use super::{SceneConstructionPhase, SceneResourceError, SceneResources};
use crate::model_loading::ModelArena;
use crate::scene_ownership::SceneKey;
use crate::scene_weather::{self, WeatherResetUnavailable, SCENE_WORDS};

impl SceneResources {
    /// Reset the existing weather record, then execute SetWeatherSystemParam(ON, 0).
    /// No constructor, shared texture creation, reference change or particle
    /// allocation is repeated. The caller supplies the normal PC24 scope.
    /// A reset failure retains its written prefix, does not disable weather and
    /// latches this operation unavailable; normal resource disposal stays possible.
    pub(crate) fn prepare_mission_weather(
        &mut self,
        arena: &ModelArena,
        scene: SceneKey,
    ) -> Result<(), SceneResourceError> {
        self.prepare_mission_weather_with(arena, scene, scene_weather::weather_system_reset)
    }

    // The production entry always supplies the existing reset. A private injected
    // failure lets tests inspect its reached prefix without altering numeric code.
    fn prepare_mission_weather_with(
        &mut self,
        arena: &ModelArena,
        scene: SceneKey,
        reset: impl FnOnce(&mut [u32; SCENE_WORDS]) -> Result<(), WeatherResetUnavailable>,
    ) -> Result<(), SceneResourceError> {
        let entry = self
            .constructions
            .iter_mut()
            .find(|entry| entry.scene == scene)
            .ok_or(SceneResourceError::UnknownConstruction(scene))?;
        if entry.arena_identity != arena.lookup_reference(scene.root()) {
            return Err(SceneResourceError::UnsupportedDomain("foreign arena"));
        }
        if entry.destroyed {
            return Err(SceneResourceError::AlreadyDestroyed(scene));
        }
        if entry.phase != SceneConstructionPhase::Complete {
            return Err(SceneResourceError::IncompleteConstruction(scene));
        }
        if entry.close_complete
            || entry.sector_cursor != 0
            || entry.listener_released
            || entry.count_released
            || entry.driver_registry_removed
        {
            return Err(SceneResourceError::UnsupportedDomain(
                "scene destruction already entered",
            ));
        }
        if entry.weather_setup_interrupted {
            return Err(SceneResourceError::UnsupportedDomain(
                "mission weather reset interrupted",
            ));
        }
        arena.check_owner_available(scene.into())?;
        if arena.scene(scene)?.cpu_world_intake_active {
            return Err(SceneResourceError::UnsupportedDomain(
                "world intake is incomplete",
            ));
        }
        // Mark before the call so neither a returned failure nor an unwound host
        // callback can later masquerade as successfully initialized weather.
        entry.weather_setup_interrupted = true;
        reset(&mut entry.words).map_err(|_| {
            SceneResourceError::UnsupportedDomain("mission weather numeric reset unavailable")
        })?;
        scene_weather::set_weather_enabled(&mut entry.words, false);
        entry.weather_setup_interrupted = false;
        Ok(())
    }
}

#[cfg(test)]
#[path = "scene_weather_runtime_tests.rs"]
mod tests;
