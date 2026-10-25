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
pub mod break_plain_glass;
pub mod break_plain_glass_lifecycle;
pub mod break_plain_glass_merge;
pub mod break_plain_glass_update;
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
pub mod collision_swept;
pub mod collision_swept_aabox;
pub mod collision_swept_box;
pub mod collision_swept_volumes;
pub mod collision_swept_world;
pub mod collision_volumes;
pub mod collision_world;
pub mod collision_world_driving;
pub mod collision_world_driving_contacts;
pub mod crash_actor;
pub mod crash_actors;
pub mod debris;
pub mod driver_state;
pub mod driving_vehicle_tables;
pub mod driving_world_assets;
pub mod dynamic_collision_manager;
pub mod dynamic_collision_queries;
pub mod frame_delta;
pub mod frame_dispatch;
pub mod frame_fast_lookup;
pub mod frame_hierarchy_bounds;
pub mod frame_light;
pub mod frame_lookup;
pub mod frame_projector;
pub mod frame_transform;
pub mod game_crash_registry;
pub mod game_spatial_sound;
pub mod glass_subdivision;
pub mod i3d_object;
pub mod indexed_sound_manager;
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
pub mod native_precision;
pub mod native_transform;
pub mod original_config;
pub mod original_program;
pub mod ph_thing_ai;
pub mod ph_thing_response;
pub mod ph_thing_update;
pub mod player_control;
pub mod police_state;
pub mod program_instructions;
pub mod program_parser;
pub mod replay_state;
pub mod rotation_animation;
pub mod runtime;
pub mod runtime_audio;
pub mod scene_lightness;
pub mod scene_ownership;
pub mod scene_tick;
pub mod scene_weather;
pub mod script_lookup;
pub mod script_scheduler;
pub mod selected_loop;
pub mod singlemesh_binding;
pub mod singlemesh_bounds;
pub mod singlemesh_deformation;
pub mod singlemesh_pose;
pub mod singlemorph_state;
#[path = "car_runtime_simulation_effects.rs"]
pub mod vehicle_runtime_effects;

pub mod skeletal_animation;
pub mod skeletal_playback;
pub mod target_direction;
pub mod target_state;
pub mod texture_manager;
pub mod vehicle_bodycol;
pub mod vehicle_bodycol_records;
pub mod vehicle_car_body_dynamic;
pub mod vehicle_car_contacts;
pub mod vehicle_component_records;
pub mod vehicle_contacts;
pub mod vehicle_controls;
pub mod vehicle_damage;
pub mod vehicle_damage_glass;
pub mod vehicle_drivetrain;
pub mod vehicle_dynamics;
pub mod vehicle_dynamics_body;
pub mod vehicle_dynamics_body_collision;
pub mod vehicle_dynamics_body_response;
pub mod vehicle_dynamics_forces;
pub mod vehicle_dynamics_move;
pub mod vehicle_dynamics_prepare;
pub mod vehicle_dynamics_tail;
pub mod vehicle_dynamics_tick;
pub mod vehicle_dynamics_update;
pub mod vehicle_dynamics_wheel_forces;
pub mod vehicle_engine_state;
pub mod vehicle_frame_backend;
pub mod vehicle_init;
pub mod vehicle_owner;
pub mod vehicle_sound;
pub mod vehicle_wheel_pose;

pub mod visual_resources;

/// Original engine-library namespace, without moving existing modules or
/// claiming an additional DLL/Cargo package or a complete LS3DF reconstruction.
pub mod ls3df {
    pub use crate::{
        c_str, frame_fast_lookup, frame_hierarchy_bounds, frame_light, frame_lookup,
        frame_projector, i3d_object, itexture, loaded_visual_resources, scene_ownership,
        scene_weather, texture_manager,
    };
}

/// Original game executable boundary; replay helper names remain descriptive.
pub mod game {
    pub use crate::{
        actor_world, car_effects, car_runtime, collision_dispatch, collision_line, collision_links,
        collision_prepare, collision_query, collision_swept, collision_swept_aabox,
        collision_swept_box, collision_swept_volumes, collision_swept_world, collision_volumes,
        collision_world, collision_world_driving, collision_world_driving_contacts,
        original_program, program_instructions, program_parser, replay_state, script_lookup,
        script_scheduler, vehicle_bodycol, vehicle_bodycol_records, vehicle_component_records,
        vehicle_contacts, vehicle_controls, vehicle_drivetrain, vehicle_dynamics,
        vehicle_dynamics_body, vehicle_dynamics_body_collision, vehicle_dynamics_body_response,
        vehicle_dynamics_forces, vehicle_dynamics_move, vehicle_dynamics_prepare,
        vehicle_dynamics_tail, vehicle_dynamics_tick, vehicle_dynamics_update,
        vehicle_dynamics_wheel_forces, vehicle_frame_backend, vehicle_init, vehicle_owner,
        vehicle_sound, vehicle_wheel_pose,
    };
}
