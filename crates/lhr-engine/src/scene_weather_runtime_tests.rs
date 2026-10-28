//! Actual SceneResources/arena ownership; only reset failures are injected.
use super::*;
use crate::itexture::{
    CpuTextureStorage, TextureImageError, TextureImageSource, TextureResources,
};
use crate::native_precision::Pc24PrecisionState;
use crate::scene_resources::SceneScreen;
use std::cell::Cell;
use std::rc::Rc;

struct Screen;
impl SceneScreen for Screen {
    type Error = ();
    fn screen_sy(&mut self) -> Result<i32, ()> {
        Ok(600)
    }
    fn screen_sx(&mut self) -> Result<i32, ()> {
        Ok(800)
    }
}
struct Images(Rc<Cell<usize>>);
impl TextureImageSource for Images {
    fn read_image(&mut self, name: &[u8]) -> Result<Vec<u8>, TextureImageError> {
        assert_eq!(name, b"Maps\\Snow.tga");
        self.0.set(self.0.get() + 1);
        let mut bytes = vec![0u8; 18];
        bytes[2] = 2;
        bytes[12] = 2;
        bytes[14] = 2;
        bytes[16] = 32;
        bytes[17] = 0x28;
        bytes.extend_from_slice(&[20, 40, 60, 128].repeat(4));
        Ok(bytes)
    }
}
struct Fixture {
    arena: ModelArena,
    resources: SceneResources,
    textures: TextureResources,
    storage: CpuTextureStorage,
    scene: SceneKey,
    image_reads: Rc<Cell<usize>>,
}
impl Fixture {
    fn new() -> Self {
        Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let image_reads = Rc::new(Cell::new(0));
            resources
                .continue_constructor(
                    &arena,
                    scene,
                    &mut Screen,
                    &mut textures,
                    &mut Images(image_reads.clone()),
                    &mut storage,
                )
                .unwrap();
            Self {
                arena,
                resources,
                textures,
                storage,
                scene,
                image_reads,
            }
        })
    }
    fn words(&self) -> [u32; SCENE_WORDS] {
        self.resources.construction(self.scene).unwrap().words
    }
    fn entry(&mut self) -> &mut super::super::SceneConstruction {
        self.resources
            .constructions
            .iter_mut()
            .find(|entry| entry.scene == self.scene)
            .unwrap()
    }
    fn prepare(&mut self) -> Result<(), SceneResourceError> {
        Pc24PrecisionState::default()
            .run(|| self.resources.prepare_mission_weather(&self.arena, self.scene))
    }
    fn dispose(&mut self) {
        self.resources
            .release_scene(
                &mut self.arena,
                self.scene,
                &mut self.textures,
                &mut self.storage,
            )
            .unwrap();
        assert_eq!(self.resources.ws_ref_count(), 0);
        assert!(self.resources.snow_texture().is_none());
        assert_eq!(self.arena.live_frame_count(), 0);
        assert_eq!(self.storage.live_graphics(), 0);
        assert_eq!(self.storage.live_pixels(), 0);
        assert_eq!(self.storage.live_strings(), 0);
    }
}

#[test]
fn weather_setup_resets_live_fields_then_disables_without_recreating_resources() {
    for flags in [0, 1, 0x40, 0x41, 0xffff_ffbe, u32::MAX] {
        let mut fixture = Fixture::new();
        let snow = fixture.resources.snow_texture().unwrap();
        fixture.entry().words[0x298 / 4] = flags;
        fixture.entry().words[0x2ac / 4..=0x328 / 4].fill(0x1234_5678);
        let before = fixture.words();
        let frames = fixture.arena.live_frame_count();
        let references = fixture.textures.texture(snow).unwrap().references;
        fixture.prepare().unwrap();
        let after = fixture.words();
        assert_eq!(after[0x298 / 4], (flags & 0x40) | 6);
        assert_eq!(after[0x2a4 / 4], 0);
        let expected = if flags & 0x40 == 0 {
            [0x3ccccccd, 0x3fc00000, 0x3cf5c28f]
        } else {
            [0x3b03126f, 0x3e99999a, 0x3e19999a]
        };
        assert_eq!(after[0x2ac / 4..0x2b8 / 4], expected);
        assert_eq!(after[0x2b8 / 4], 25f32.to_bits());
        assert_eq!(after[0x2bc / 4], 12f32.to_bits());
        assert_eq!(after[0x2c0 / 4..0x2c8 / 4], [1000, 3000]);
        assert_eq!(after[0x2c8 / 4..0x2d4 / 4], after[0x2d4 / 4..0x2e0 / 4]);
        for index in 0..SCENE_WORDS {
            let offset = index * 4;
            if offset != 0x298 && offset != 0x2a4 && !(0x2ac..=0x328).contains(&offset) {
                assert_eq!(after[index], before[index], "unrelated {offset:#x}");
            }
        }
        assert_eq!(fixture.arena.live_frame_count(), frames);
        assert_eq!(fixture.resources.ws_ref_count(), 1);
        assert_eq!(fixture.resources.snow_texture(), Some(snow));
        assert_eq!(fixture.textures.texture(snow).unwrap().references, references);
        assert_eq!(fixture.image_reads.get(), 1);
        assert!(!fixture.entry().weather_setup_interrupted);
        fixture.dispose();
    }
}

#[test]
fn preparing_one_scene_keeps_another_scene_and_the_shared_snow_reference() {
    let mut fixture = Fixture::new();
    let second = fixture.arena.construct_scene_owner_prefix().unwrap();
    Pc24PrecisionState::default().run(|| {
        fixture.resources
            .continue_constructor(
                &fixture.arena,
                second,
                &mut Screen,
                &mut fixture.textures,
                &mut Images(fixture.image_reads.clone()),
                &mut fixture.storage,
            )
            .unwrap();
    });
    let other = fixture.resources.construction(second).unwrap().words;
    let snow = fixture.resources.snow_texture();
    fixture.entry().words[0x298 / 4] = 0x41;
    fixture.prepare().unwrap();
    assert_eq!(fixture.words()[0x298 / 4], 0x46);
    assert_eq!(fixture.resources.construction(second).unwrap().words, other);
    assert_eq!(fixture.resources.ws_ref_count(), 2);
    assert_eq!(fixture.resources.snow_texture(), snow);
    assert_eq!(fixture.image_reads.get(), 1);
    fixture.resources
        .release_scene(
            &mut fixture.arena,
            second,
            &mut fixture.textures,
            &mut fixture.storage,
        )
        .unwrap();
    fixture.dispose();
}

#[test]
fn foreign_arena_with_equal_numeric_scene_key_is_rejected_before_reset() {
    let mut fixture = Fixture::new();
    let mut foreign = ModelArena::default();
    let other = foreign.construct_scene_owner_prefix().unwrap();
    assert_eq!(other, fixture.scene);
    let before = fixture.words();
    assert!(matches!(
        fixture.resources.prepare_mission_weather_with(&foreign, other, |_| panic!("unreached")),
        Err(SceneResourceError::UnsupportedDomain("foreign arena"))
    ));
    assert_eq!(fixture.words(), before);
    assert!(!fixture.entry().weather_setup_interrupted);
    fixture.dispose();
}

#[test]
fn missing_construction_is_not_an_empty_successful_weather_record() {
    let mut arena = ModelArena::default();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let mut resources = SceneResources::default();
    assert!(matches!(
        resources.prepare_mission_weather_with(&arena, scene, |_| panic!("unreached")),
        Err(SceneResourceError::UnknownConstruction(key)) if key == scene
    ));
    assert_eq!(resources.ws_ref_count(), 0);
    assert!(resources.constructions.is_empty());
}

#[test]
fn incomplete_construction_never_enters_weather_reset() {
    for phase in [
        SceneConstructionPhase::Viewport,
        SceneConstructionPhase::Counted,
        SceneConstructionPhase::WeatherReset,
    ] {
        let mut fixture = Fixture::new();
        // Seed a host constructor-boundary fixture; do not restore it for disposal.
        fixture.entry().phase = phase;
        let before = fixture.words();
        assert!(matches!(
            fixture.resources.prepare_mission_weather_with(
                &fixture.arena, fixture.scene, |_| panic!("unreached")
            ),
            Err(SceneResourceError::IncompleteConstruction(_))
        ));
        assert_eq!(fixture.words(), before);
        assert!(!fixture.entry().weather_setup_interrupted);
    }
}

#[test]
fn destruction_prefix_is_rejected_even_when_the_scene_frame_is_still_alive() {
    for stage in 0..5 {
        let mut fixture = Fixture::new();
        // Each stored terminal marker independently rules out weather mutation.
        let entry = fixture.entry();
        match stage {
            0 => entry.close_complete = true,
            1 => entry.sector_cursor = 1,
            2 => entry.listener_released = true,
            3 => entry.count_released = true,
            _ => entry.driver_registry_removed = true,
        }
        let before = fixture.words();
        assert!(matches!(
            fixture.resources.prepare_mission_weather_with(
                &fixture.arena, fixture.scene, |_| panic!("unreached")
            ),
            Err(SceneResourceError::UnsupportedDomain("scene destruction already entered"))
        ));
        assert_eq!(fixture.words(), before);
        assert!(!fixture.entry().weather_setup_interrupted);
    }
}

#[test]
fn failed_world_intake_and_in_progress_close_cannot_reset_weather() {
    struct Stop;
    impl crate::scene_ownership::SceneCloseBackend for Stop {
        fn close_effect(
            &mut self,
            _: &ModelArena,
            _: SceneKey,
            effect: crate::scene_ownership::SceneCloseEffect,
        ) -> Result<(), crate::model_loading::ModelError> {
            Err(crate::scene_ownership::SceneOwnerError::BackendUnavailable(effect).into())
        }
    }
    for closing in [false, true] {
        let mut fixture = Fixture::new();
        if closing {
            assert!(fixture.arena.close_scene_with_backend(fixture.scene, &mut Stop).is_err());
        } else {
            let index = fixture.arena.scene_index(fixture.scene).unwrap();
            fixture.arena.scenes[index].cpu_world_intake_active = true;
        }
        let before = fixture.words();
        assert!(fixture.resources.prepare_mission_weather_with(
            &fixture.arena, fixture.scene, |_| panic!("unreached")
        ).is_err());
        assert_eq!(fixture.words(), before);
        assert!(!fixture.entry().weather_setup_interrupted);
    }
}

#[test]
fn retired_scene_keeps_its_receipt_but_cannot_restart_weather_or_shared_count() {
    let mut fixture = Fixture::new();
    fixture.dispose();
    let before = fixture.words();
    assert!(matches!(fixture.prepare(), Err(SceneResourceError::AlreadyDestroyed(_))));
    assert_eq!(fixture.words(), before);
    assert_eq!(fixture.resources.ws_ref_count(), 0);
    assert!(fixture.resources.snow_texture().is_none());
}

#[test]
fn reset_error_preserves_its_prefix_does_not_disable_and_blocks_replay() {
    let mut fixture = Fixture::new();
    fixture.entry().words[0x298 / 4] = 0x41;
    let error = fixture.resources.prepare_mission_weather_with(
        &fixture.arena,
        fixture.scene,
        |words| {
            assert_eq!(words[0x298 / 4], 0x41); // ON has not been cleared early.
            words[0x298 / 4] = 0x47;
            words[0x2b8 / 4] = 17f32.to_bits();
            Err(WeatherResetUnavailable)
        },
    );
    assert!(matches!(error, Err(SceneResourceError::UnsupportedDomain(
        "mission weather numeric reset unavailable"
    ))));
    assert_eq!(fixture.words()[0x298 / 4], 0x47);
    assert_eq!(fixture.words()[0x2b8 / 4], 17f32.to_bits());
    assert!(fixture.entry().weather_setup_interrupted);
    let before = fixture.words();
    assert!(matches!(fixture.prepare(), Err(SceneResourceError::UnsupportedDomain(
        "mission weather reset interrupted"
    ))));
    assert_eq!(fixture.words(), before);
    // Numeric setup did not destroy resources; their canonical disposal remains valid.
    fixture.dispose();
}

#[test]
fn unwound_reset_also_keeps_the_guard_before_a_later_call() {
    let mut fixture = Fixture::new();
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fixture.resources.prepare_mission_weather_with(
            &fixture.arena,
            fixture.scene,
            |words| {
                words[0x298 / 4] = 0x47;
                panic!("synthetic host reset panic");
            },
        )
    }));
    assert!(unwind.is_err());
    assert_eq!(fixture.words()[0x298 / 4], 0x47);
    assert!(fixture.entry().weather_setup_interrupted);
    assert!(fixture.prepare().is_err());
    fixture.dispose();
}

#[test]
fn successful_reset_can_repeat_without_reconstructing_shared_resources() {
    let mut fixture = Fixture::new();
    for _ in 0..3 {
        fixture.entry().words[0x298 / 4] = 0x41;
        fixture.prepare().unwrap();
        assert_eq!(fixture.words()[0x298 / 4], 0x46);
        assert!(!fixture.entry().weather_setup_interrupted);
        assert_eq!(fixture.resources.ws_ref_count(), 1);
        assert_eq!(fixture.image_reads.get(), 1);
    }
    fixture.dispose();
}

#[test]
fn on_parameter_changes_only_one_bit_and_never_clears_other_weather_storage() {
    for flags in [0, 1, 0x40, 0x41, 0x8000_0000, u32::MAX] {
        for enabled in [false, true] {
            let mut words = [0xa5a5_5a5a; SCENE_WORDS];
            words[0x298 / 4] = flags;
            let before = words;
            scene_weather::set_weather_enabled(&mut words, enabled);
            assert_eq!(words[0x298 / 4] & 1 != 0, enabled);
            assert_eq!(words[0x298 / 4] & !1, flags & !1);
            for index in 0..SCENE_WORDS {
                if index != 0x298 / 4 {
                    assert_eq!(words[index], before[index]);
                }
            }
        }
    }
}
