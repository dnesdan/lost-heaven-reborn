//! Reusable engine components for the Rust rewrite.

pub mod actor_animation;
pub mod actor_frames;
pub mod actor_registry;
pub mod actor_shot;
pub mod actor_tick;
pub mod actor_world;
pub mod ai_driver_tick;
pub mod animation_binding;
pub mod animation_catalog;
pub mod animation_controller;
pub mod animation_index;
pub mod animation_pose;
pub mod animation_set_keys;
mod arena_frame_names;
pub mod billboard_state;
pub mod box_new_actor;
pub mod break_plain_glass;
pub mod break_plain_glass_lifecycle;
pub mod break_plain_glass_merge;
pub mod break_plain_glass_update;
pub mod bridge_actor;
pub mod buffer_managers;
pub mod c_str;
pub mod canonical_city_cache;
mod canonical_scene2_models;
pub mod car_actor;
pub(crate) mod car_actor_tables;
pub mod car_ai_position;
pub mod car_effects;
pub mod car_ex_actor;
pub mod car_inspection;
pub mod car_runtime;
pub mod car_state_source;
pub mod city_base_cache;
pub mod clock;
pub mod clock_actor;
pub mod collision_dispatch;
pub mod collision_line;
pub mod collision_links;
pub mod collision_prepare;
pub mod collision_query;
pub mod collision_scene;
pub mod collision_swept;
pub mod collision_swept_aabox;
pub mod collision_swept_box;
pub mod collision_swept_capsule;
pub mod collision_swept_volumes;
pub mod collision_swept_world;
pub mod collision_volumes;
pub mod collision_world;
pub mod collision_world_driving;
pub mod collision_world_driving_contacts;
pub mod cpu_info;
pub mod crash_actor;
pub mod crash_actors;
pub mod cutscene_owner;
pub mod cutscene_records;
pub mod cutscene_source;
pub mod dabing;
pub mod debris;
pub mod door_actor;
pub mod driver_lifecycle;
pub mod driver_scene;
pub mod driver_state;
pub mod driver_strings;
pub mod driver_visual_factory;
pub mod driving_vehicle_tables;
pub mod driving_world_assets;
pub mod dynamic_collision_manager;
pub mod dynamic_collision_queries;
pub mod effects_table;
pub mod entity_actor;
pub mod frame_area;
pub mod frame_callback;
pub mod frame_camera;
pub mod frame_delta;
pub mod frame_dispatch;
pub mod frame_dummy;
pub mod frame_emitor;
pub mod frame_fast_lookup;
pub mod frame_hierarchy_bounds;
pub mod frame_light;
pub mod frame_lookup;
pub mod frame_mirror;
pub mod frame_occluder;
pub mod frame_portal;
pub mod frame_projector;
pub mod frame_sector;
pub mod frame_shadow;
pub mod frame_sound;
pub mod frame_transform;
pub mod frame_user;
pub mod frame_volume;
pub mod game_camera_begin;
pub mod game_camera_projection;
pub mod game_camera_reset;
pub mod game_camera_tick_prologue;
pub mod game_controls;
pub mod game_crash_registry;
pub mod game_definition_tables;
pub mod game_init_order;
pub mod game_loadsave_controls;
pub mod game_loadsave_lifecycle;
pub mod game_spatial_sound;
pub mod glass_subdivision;
pub mod host_scene_adapters;
pub mod human_actor;
pub mod human_move_coll;
pub mod i3d_object;
pub mod indexed_sound_manager;
pub mod indicators;
pub mod input_keyboard;
pub mod inspection_lod;
pub mod interpolator;
pub mod ishow_renderer;
pub mod itexture;
pub mod joint_state;
pub mod landscape_roam;
pub mod loaded_visual_resources;
pub mod main_menu;
pub mod managed_visual_resources;
pub mod maps_dirs;
pub mod menu;
pub mod mesh_bounds;
pub mod mission_actor_stream;
pub mod mission_components;
pub mod mission_content_loading;
pub mod mission_effects;
pub mod mission_lifecycle;
pub mod mission_load_session;
pub mod mission_navigation_input;
pub mod mission_open_order;
pub mod mission_resources;
pub mod mission_scene_loading;
pub mod mission_scene_stream;
pub mod mission_session_navigation;
pub mod mission_session_roads;
pub mod mission_world_completeness;
pub mod model_animation_runtime;
pub mod model_loading;
pub mod model_tick;
pub mod named_script;
pub mod native_matrix;
pub mod native_precision;
pub mod native_transform;
pub mod null_material_lighting;
mod occluder_visibility;
pub mod original_config;
mod original_data_chunk;
pub mod original_language;
pub mod original_logo;
pub mod original_program;
pub mod original_system;
pub mod original_windows;
pub mod particle_emitor_elements;
pub mod ph_thing_ai;
pub mod ph_thing_response;
pub mod ph_thing_update;
pub mod player_actor;
pub mod player_control;
pub mod police_state;
mod portal_visibility;
mod presentation_lighting;
pub mod presentation_material;
pub mod program_compiler;
pub mod program_entity_parameters;
pub mod program_instructions;
pub mod program_parameters;
pub mod program_parser;
pub mod program_patch;
pub mod program_zero_parameters;
pub mod projector_pass;
pub mod pumper_actor;
pub mod racecam;
pub mod rail_generator;
pub mod railway_actor;
pub mod render_clip;
pub mod render_core;
pub mod render_ib;
pub mod render_lod;
pub mod render_state;
pub mod render_vb;
pub mod replay_state;
pub mod road_network;
pub mod rotation_animation;
pub mod runtime;
pub mod runtime_audio;
pub mod scene_animation_runtime;
pub mod scene_file_load;
pub mod scene_lightness;
pub mod scene_ownership;
pub mod scene_projection;
pub mod scene_render;
pub mod scene_resources;
pub mod scene_tick;
pub mod scene_visibility;
pub mod scene_weather;
pub mod scene_weather_dummy;
pub mod script_lookup;
pub mod script_scheduler;
pub mod sector_sound_data;
pub mod selected_loop;
pub mod shadow_pass;
pub mod singlemesh_binding;
pub mod singlemesh_bounds;
pub mod singlemesh_deformation;
pub mod singlemesh_lifecycle;
pub mod singlemesh_pose;
pub mod singlemorph_state;
pub mod singlemorph_visual;
pub mod startup_asset_backend;
pub mod traffic_car_actor;
pub mod traffic_generator;
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

pub mod visual_replacement;
pub mod visual_resources;

/// Original engine-library namespace, without moving existing modules or
/// claiming an additional DLL/Cargo package or a complete LS3DF reconstruction.
pub mod ls3df {
    pub use crate::{
        c_str, driver_lifecycle, driver_scene, driver_strings, driver_visual_factory, frame_camera,
        frame_fast_lookup, frame_hierarchy_bounds, frame_light, frame_lookup, frame_occluder,
        frame_portal, frame_projector, frame_sector, i3d_object, itexture, loaded_visual_resources,
        scene_animation_runtime, scene_ownership, scene_projection, scene_resources, scene_weather,
        scene_weather_dummy, sector_sound_data, texture_manager,
    };
}

/// Original game executable boundary; replay helper names remain descriptive.
pub mod game {
    pub use crate::{
        actor_world, ai_driver_tick, car_ai_position, car_effects, car_runtime, city_base_cache,
        collision_dispatch, collision_line, collision_links, collision_prepare, collision_query,
        collision_swept, collision_swept_aabox, collision_swept_box, collision_swept_capsule,
        collision_swept_volumes, collision_swept_world, collision_volumes, collision_world,
        collision_world_driving, collision_world_driving_contacts, game_camera_projection,
        game_init_order, mission_actor_stream, mission_components, mission_lifecycle,
        mission_resources, mission_scene_loading, mission_scene_stream, mission_session_navigation,
        mission_session_roads, mission_world_completeness, original_logo, original_program,
        original_system, original_windows, police_state, program_entity_parameters,
        program_instructions, program_parser, railway_actor, replay_state, road_network,
        script_lookup, script_scheduler, traffic_car_actor, vehicle_bodycol,
        vehicle_bodycol_records, vehicle_component_records, vehicle_contacts, vehicle_controls,
        vehicle_drivetrain, vehicle_dynamics, vehicle_dynamics_body,
        vehicle_dynamics_body_collision, vehicle_dynamics_body_response, vehicle_dynamics_forces,
        vehicle_dynamics_move, vehicle_dynamics_prepare, vehicle_dynamics_tail,
        vehicle_dynamics_tick, vehicle_dynamics_update, vehicle_dynamics_wheel_forces,
        vehicle_frame_backend, vehicle_init, vehicle_owner, vehicle_sound, vehicle_wheel_pose,
    };
}

#[cfg(test)]
mod live_lod_tests;

#[cfg(test)]
mod lit_object_tests;
