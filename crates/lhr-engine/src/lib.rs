//! Reusable engine components for the Rust rewrite.

pub mod actor_animation;
pub mod actor_frames;
pub mod actor_registry;
pub mod actor_tick;
pub mod actor_world;
pub mod animation_binding;
pub mod animation_controller;
pub mod animation_index;
mod arena_frame_names;
pub mod buffer_managers;
pub mod c_str;
pub mod car_effects;
pub mod car_inspection;
pub mod car_runtime;
pub mod clock;
pub mod collision_dispatch;
pub mod collision_line;
pub mod collision_links;
pub mod collision_prepare;
pub mod collision_query;
pub mod collision_volumes;
pub mod collision_world;
pub mod driver_state;
pub mod frame_delta;
pub mod frame_dispatch;
pub mod frame_fast_lookup;
pub mod frame_hierarchy_bounds;
pub mod frame_light;
pub mod frame_lookup;
pub mod frame_projector;
pub mod frame_transform;
pub mod i3d_object;
pub mod interpolator;
pub mod itexture;
pub mod joint_state;
pub mod loaded_visual_resources;
pub mod managed_visual_resources;
pub mod mesh_bounds;
pub mod mission_effects;
pub mod model_loading;
pub mod model_tick;
pub mod named_script;
pub mod native_matrix;
pub mod native_transform;
pub mod original_config;
pub mod original_program;
pub mod player_control;
pub mod police_state;
pub mod program_instructions;
pub mod program_parser;
pub mod replay_state;
pub mod rotation_animation;
pub mod runtime;
pub mod scene_ownership;
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
pub mod vehicle_bodycol;
pub mod vehicle_bodycol_records;
pub mod vehicle_component_records;
pub mod vehicle_controls;
pub mod vehicle_drivetrain;
pub mod vehicle_frame_backend;
pub mod vehicle_init;
pub mod vehicle_owner;
pub mod vehicle_sound;

pub mod visual_resources;

/// Original engine-library namespace, without moving existing modules or
/// claiming an additional DLL/Cargo package or a complete LS3DF reconstruction.
pub mod ls3df {
    pub use crate::{
        c_str, frame_fast_lookup, frame_hierarchy_bounds, frame_light, frame_lookup,
        frame_projector, i3d_object, itexture, loaded_visual_resources, scene_ownership,
        texture_manager,
    };
}

/// Original game executable boundary; replay helper names remain descriptive.
pub mod game {
    pub use crate::{
        actor_world, car_effects, car_runtime, collision_dispatch, collision_line, collision_links,
        collision_prepare, collision_query, collision_volumes, collision_world, original_program,
        program_instructions, program_parser, replay_state, script_lookup, script_scheduler,
        vehicle_bodycol, vehicle_bodycol_records, vehicle_component_records, vehicle_controls,
        vehicle_drivetrain, vehicle_frame_backend, vehicle_init, vehicle_owner, vehicle_sound,
    };
}
