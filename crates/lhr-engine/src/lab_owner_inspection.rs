//! Read-only host observations of the actual retained resource owners.
//! UI values are presentation data, never inputs to original engine operations.
#![cfg_attr(not(feature = "engine-lab"), allow(dead_code))]

use lhr_engine::scene_render::CanonicalSceneState;
use lhr_engine::visual_resources::{I3dMaterial, MaterialKey, VisualResources};
use std::collections::HashSet;

pub const MAX_ANIMATED_ROWS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Channel {
    pub frame: usize,
    pub frames: usize,
    pub delay_ms: i32,
    pub loop_frame: i32,
    pub elapsed_ms: i32,
    pub last_time_ms: i32,
    pub texture: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimatedMaterial {
    pub arena: u64,
    pub slot: u32,
    pub references: u32,
    pub flags: u32,
    pub driver_time_ms: i32,
    pub color: Channel,
    pub environment: Channel,
}
impl AnimatedMaterial {
    fn read(key: MaterialKey, material: &I3dMaterial, driver_time_ms: i32) -> Option<Self> {
        let animation = material.animation.as_ref()?;
        Some(Self {
            arena: key.arena(),
            slot: key.slot(),
            references: material.references,
            flags: material.flags,
            driver_time_ms,
            color: Channel {
                frame: animation.frame,
                frames: animation.textures.len(),
                delay_ms: animation.frame_delay,
                loop_frame: animation.loop_frame,
                elapsed_ms: animation.elapsed,
                last_time_ms: animation.last_time,
                texture: material.texture.map(|key| key.0),
            },
            environment: Channel {
                frame: animation.env_frame,
                frames: animation.env_textures.len(),
                delay_ms: animation.env_frame_delay,
                loop_frame: animation.env_loop_frame,
                elapsed_ms: animation.env_elapsed,
                last_time_ms: animation.env_last_time,
                texture: material.env_texture.map(|key| key.0),
            },
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frontier {
    pub cursor: usize,
    pub entry: u32,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mission {
    pub frames: usize,
    pub objects: usize,
    pub map_entries: usize,
    pub actors: usize,
    pub standalone_programs: usize,
    pub content_allocations: usize,
    pub cursor: usize,
    pub frontier: Option<Frontier>,
}
impl Mission {
    fn read(owner: &CanonicalSceneState) -> Self {
        let mut result = Self {
            frames: owner.arena.live_frame_count(),
            frontier: owner.frontier.as_ref().map(|error| Frontier {
                cursor: error.cursor,
                entry: error.entry,
                detail: error.detail.clone(),
            }),
            ..Self::default()
        };
        if let Some(loader) = &owner.loader {
            result.objects = loader.finished_objects;
            result.map_entries = loader.load_map.publications.len();
            result.actors = loader.content.actors.world.owned_actor_references().count();
            result.standalone_programs = loader.content.live_program_count();
            result.content_allocations = loader.content.live_allocation_count();
            result.cursor = loader.cursor;
        }
        result
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedCleanup {
    pub slot: usize,
    pub frames: usize,
    pub actors: usize,
    pub standalone_programs: usize,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainedCarCleanup {
    pub slot: usize,
    pub frames: usize,
    pub reason: String,
}

#[derive(Default)]
pub struct Inspection {
    pub material_owners: usize,
    pub materials: usize,
    pub animated_materials: usize,
    pub animated: Vec<AnimatedMaterial>,
    pub mission: Option<Mission>,
    pub retained_cleanup: Vec<RetainedCleanup>,
    pub retained_car_cleanup: Vec<RetainedCarCleanup>,
    pub patch_replacements: usize,
    pub patch_phase: String,
    seen_material_arenas: HashSet<u64>,
}
impl Inspection {
    /// Each actual arena contributes once even if two views alias its owner.
    /// Only the displayed rows are bounded; aggregate counts include all slots.
    fn observe_materials(&mut self, resources: &VisualResources) {
        if !self.seen_material_arenas.insert(resources.identity()) {
            return;
        }
        self.material_owners += 1;
        for (key, material) in resources.materials() {
            self.materials += 1;
            if material.animation.is_some() {
                self.animated_materials += 1;
                if self.animated.len() < MAX_ANIMATED_ROWS {
                    self.animated.push(
                        AnimatedMaterial::read(key, material, resources.material_driver_time())
                            .expect("observed animated material"),
                    );
                }
            }
        }
    }

    pub fn read(app: &super::App) -> Self {
        let mut result = Self {
            patch_replacements: app.startup_patches.table().replacements().len(),
            patch_phase: format!("{:?}", app.startup_patches.phase()),
            ..Self::default()
        };
        if let Some(owner) = &app.canonical {
            result.mission = Some(Mission::read(owner));
            if let Some(resources) = owner.arena.loaded_visual_resources() {
                result.observe_materials(resources.objects().managed().resources());
            }
        }
        if let Some(owner) = &app.car {
            if let Some(resources) = owner.arena().loaded_visual_resources() {
                result.observe_materials(resources.objects().managed().resources());
            }
        }
        for model in &app.runtime.models {
            if let Some(resources) = model.material_resources() {
                result.observe_materials(resources);
            }
        }
        for slot in lhr_engine::scene_render::cleanup_failure_slots() {
            if let Some(failure) =
                lhr_engine::scene_render::inspect_cleanup_failure(slot, |owner| {
                    let mission = Mission::read(owner);
                    RetainedCleanup {
                        slot,
                        frames: mission.frames,
                        actors: mission.actors,
                        standalone_programs: mission.standalone_programs,
                        reason: owner.cleanup_failure().map(str::to_owned),
                    }
                })
            {
                result.retained_cleanup.push(failure);
            }
        }
        for slot in lhr_engine::car_runtime::failure_slots() {
            if let Some(failure) =
                lhr_engine::car_runtime::inspect_failed_car(slot, |owner, reason| {
                    RetainedCarCleanup {
                        slot,
                        frames: owner.arena().live_frame_count(),
                        reason: reason.to_owned(),
                    }
                })
            {
                result.retained_car_cleanup.push(failure);
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_rows_are_bounded_but_counts_and_owner_identity_remain_exact() {
        let mut resources = VisualResources::new().unwrap();
        let static_material = resources.create_material().unwrap();
        for _ in 0..MAX_ANIMATED_ROWS + 3 {
            resources.create_animated_material().unwrap();
        }
        resources.set_material_driver_time(600).unwrap();
        let mut result = Inspection::default();
        result.observe_materials(&resources);
        result.observe_materials(&resources);
        assert_eq!(result.material_owners, 1);
        assert_eq!(result.materials, MAX_ANIMATED_ROWS + 4);
        assert_eq!(result.animated_materials, MAX_ANIMATED_ROWS + 3);
        assert_eq!(result.animated.len(), MAX_ANIMATED_ROWS);
        assert!(result.animated.iter().all(|row| {
            row.driver_time_ms == 600
                && row.color.last_time_ms == 0
                && row.environment.last_time_ms == 0
                && row.color.texture.is_none()
                && row.environment.texture.is_none()
        }));
        assert!(resources
            .material(static_material)
            .unwrap()
            .animation
            .is_none());
        assert!(resources
            .materials()
            .filter_map(|(_, m)| m.animation.as_ref())
            .all(|m| {
                m.last_time == 0 && m.env_last_time == 0 && m.frame == 0 && m.env_frame == 0
            }));
    }
}
