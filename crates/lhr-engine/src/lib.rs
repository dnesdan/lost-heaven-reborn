//! Reusable engine components for the Rust rewrite.

pub mod actor_animation;
pub mod actor_registry;
pub mod actor_tick;
pub mod animation_binding;
pub mod animation_controller;
pub mod animation_index;
pub mod buffer_managers;
pub mod c_str;
pub mod clock;
pub mod driver_state;
pub mod frame_delta;
pub mod frame_dispatch;
pub mod frame_lookup;
pub mod frame_transform;
pub mod i3d_object;
pub mod interpolator;
pub mod itexture;
pub mod joint_state;
pub mod managed_visual_resources;
pub mod mesh_bounds;
pub mod mission_effects;
pub mod model_loading;
pub mod model_tick;
pub mod named_script;
pub mod native_matrix;
pub mod native_transform;
pub mod original_config;
pub mod player_control;
pub mod police_state;
pub mod replay_state;
pub mod rotation_animation;
pub mod runtime;
pub mod scene_tick;
pub mod script_lookup;
pub mod script_scheduler;
pub mod selected_loop;
pub mod singlemesh_binding;
pub mod singlemesh_bounds;
pub mod singlemesh_deformation;
pub mod singlemesh_pose;
pub mod singlemorph_state;

pub mod skeletal_animation;
pub mod skeletal_playback;
pub mod target_direction;
pub mod target_state;
pub mod texture_manager;

pub mod visual_resources;

/// Original engine-library namespace, without moving existing modules or
/// claiming an additional DLL/Cargo package or a complete LS3DF reconstruction.
pub mod ls3df {
    pub use crate::{c_str, frame_lookup, i3d_object, itexture, texture_manager};
}

/// Original game executable boundary; replay helper names remain descriptive.
pub mod game {
    pub use crate::{replay_state, script_lookup, script_scheduler};
}
