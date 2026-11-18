//! Borrowed session adapter for the shared GM_Menu resource operations.
//! The arena's existing factory backend and the draw owner remain authoritative.
//! Count, interrupted-operation guards and receipts stay in MenuSharedResources;
//! dropping this view neither releases nor retries any native reference.

#![forbid(unsafe_code)]

use crate::frame_sound::{SoundCreateFailure, SoundError, SoundKey};
use crate::itexture::TextureKey;
use crate::menu_draw_resources::{MenuComponentResources, MenuDrawResources};
use crate::menu_shared_resources::MenuSharedSoundHost;
use crate::model_loading::{ModelArena, ModelError};
use crate::runtime_audio::{RuntimeAudio, SoundId, SoundKind};
use lhr_assets::Installation;
use std::sync::Arc;

pub struct MenuSessionResources<'a> {
    arena: &'a mut ModelArena,
    audio: &'a RuntimeAudio,
    draw: &'a mut MenuDrawResources,
    installation: &'a Installation,
}

impl<'a> MenuSessionResources<'a> {
    /// Borrow the canonical session owners without binding a driver or loading
    /// anything. Sound creation requires the arena's already-bound backend to
    /// use this audio owner; a missing or foreign backend is a reached error.
    pub fn new(
        arena: &'a mut ModelArena,
        audio: &'a RuntimeAudio,
        draw: &'a mut MenuDrawResources,
        installation: &'a Installation,
    ) -> Self {
        Self {
            arena,
            audio,
            draw,
            installation,
        }
    }

    fn check_sound_backend(&self) -> Result<(), ModelError> {
        if !self.arena.host_sound_backend()?.uses_audio(self.audio) {
            return Err(SoundError::ForeignDriver.into());
        }
        Ok(())
    }

    fn sound_id(&self, sound: SoundKey) -> Result<&SoundId, String> {
        self.arena
            .sound(sound)
            .map(|frame| frame.audio_id())
            .map_err(|error| format!("menu sound frame: {error}"))
    }
}

impl MenuComponentResources for MenuSessionResources<'_> {
    fn text(&mut self, text_id: u32) -> Result<Option<Arc<[u8]>>, String> {
        self.draw.text(text_id)
    }

    fn create_alpha_texture(&mut self, name: &[u8]) -> Result<Option<TextureKey>, String> {
        self.draw.create_alpha_texture(name)
    }

    fn release_texture(&mut self, key: TextureKey) -> Result<(), String> {
        self.draw.release_texture(key)
    }
}

impl MenuSharedSoundHost for MenuSessionResources<'_> {
    fn create_sound_frame(&mut self) -> Result<SoundKey, SoundCreateFailure> {
        self.check_sound_backend()
            .map_err(|error| SoundCreateFailure { frame: None, error })?;
        self.arena.create_owned_sound_frame()
    }

    fn set_sound_type(&mut self, sound: SoundKey, kind: SoundKind) -> Result<(), String> {
        self.audio
            .set_sound_kind(self.sound_id(sound)?, kind)
            .map_err(|error| format!("menu SetSoundType: {error}"))
    }

    fn open_sound(&mut self, sound: SoundKey, path: &[u8], flags: u32) -> Result<i32, String> {
        let identity = self.sound_id(sound)?;
        let path = std::str::from_utf8(path)
            .map_err(|error| format!("menu Open filename encoding: {error}"))?;
        self.audio
            .open_sound_source(identity, path, flags, self.installation)
            .map_err(|error| format!("menu Open: {error}"))
    }

    fn set_out_volume(&mut self, sound: SoundKey, volume: f32) -> Result<(), String> {
        self.audio
            .set_sound_outside_volume(self.sound_id(sound)?, volume)
            .map_err(|error| format!("menu SetOutVol: {error}"))
    }

    fn set_volume(&mut self, sound: SoundKey, volume: f32) -> Result<(), String> {
        self.audio
            .set_sound_volume(self.sound_id(sound)?, volume)
            .map_err(|error| format!("menu SetVolume: {error}"))
    }

    fn update_sound(&mut self, sound: SoundKey) -> Result<(), String> {
        self.audio
            .update_sound(self.sound_id(sound)?)
            .map_err(|error| format!("menu Update sound: {error}"))
    }

    fn set_on(&mut self, sound: SoundKey, on: bool, immediate: bool) -> Result<(), String> {
        self.audio
            .set_sound_playback(self.sound_id(sound)?, on, immediate)
            .map_err(|error| format!("menu SetOn: {error}"))
    }

    fn release_sound(&mut self, sound: SoundKey) -> Result<(), String> {
        self.sound_id(sound)?;
        self.check_sound_backend()
            .map_err(|error| format!("menu sound Release backend: {error}"))?;
        self.arena
            .release(sound.frame())
            .map_err(|error| format!("menu sound frame Release: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver_scene::DriverSceneOwner;
    use crate::itexture::{
        CpuTextureStorage, CpuTextureSurface, TextureImageError, TextureImageSource,
        TextureResources,
    };
    use crate::menu_draw_resources::{CpuMenuTextureFactory, MenuTextDatabase, MenuTextureBackend};
    use crate::menu_shared_resources::{
        MenuSharedResourceStep, MenuSharedResources, MenuSharedResourcesError, MenuSharedSound,
        MenuSharedTexture,
    };
    use crate::runtime_audio::{SoundFrameOwner, SoundState};
    use std::cell::Cell;
    use std::path::PathBuf;
    use std::rc::Rc;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "lhr-menu-session-resources-{}-{}",
                std::process::id(),
                crate::frame_lookup::incarnation()
            ));
            std::fs::create_dir(&root).unwrap();
            std::fs::create_dir(root.join("sounds")).unwrap();
            Self(root)
        }

        fn write_sounds(&self) {
            for filename in ["menuopen.wav", "menuclose.wav", "menuchange.wav"] {
                std::fs::write(self.0.join("sounds").join(filename), wav()).unwrap();
            }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            for filename in ["menuopen.wav", "menuclose.wav", "menuchange.wav"] {
                let _ = std::fs::remove_file(self.0.join("sounds").join(filename));
            }
            let _ = std::fs::remove_dir(self.0.join("sounds"));
            let _ = std::fs::remove_dir(&self.0);
        }
    }

    struct Images(Result<Vec<u8>, TextureImageError>);

    impl TextureImageSource for Images {
        fn read_image(&mut self, _path: &[u8]) -> Result<Vec<u8>, TextureImageError> {
            self.0.clone()
        }
    }

    fn tga() -> Vec<u8> {
        let mut bytes = vec![0; 18];
        bytes[2] = 2;
        bytes[12] = 1;
        bytes[14] = 1;
        bytes[16] = 32;
        bytes[17] = 0x20;
        bytes.extend_from_slice(&[3, 2, 1, 255]);
        bytes
    }

    fn wav() -> Vec<u8> {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend(44u32.to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16u32.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(4u32.to_le_bytes());
        bytes.extend(8u32.to_le_bytes());
        bytes.extend(2u16.to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(8u32.to_le_bytes());
        bytes.extend(
            [8192i16, 16384, 0, -8192]
                .into_iter()
                .flat_map(i16::to_le_bytes),
        );
        bytes
    }

    fn text_image(text: &[u8]) -> Vec<u8> {
        let mut bytes = 3u32.to_le_bytes().to_vec();
        bytes.extend(17u32.to_le_bytes());
        for text_id in [1u32, 42, 100] {
            bytes.extend(text_id.to_le_bytes());
            bytes.extend(32u32.to_le_bytes());
        }
        bytes.extend_from_slice(text);
        bytes.push(0);
        bytes
    }

    fn texture_factory(images: Images) -> CpuMenuTextureFactory<Images> {
        CpuMenuTextureFactory::new(
            TextureResources::new().unwrap(),
            CpuTextureStorage::default(),
            images,
        )
        .unwrap()
    }

    struct Fixture {
        arena: ModelArena,
        audio: RuntimeAudio,
        driver: DriverSceneOwner,
        draw: MenuDrawResources,
        installation: Installation,
        scratch: Scratch,
    }

    impl Fixture {
        fn new() -> Self {
            let scratch = Scratch::new();
            let installation = Installation::open(&scratch.0).unwrap();
            let mut arena = ModelArena::default();
            let audio = RuntimeAudio::default();
            let mut driver = DriverSceneOwner::default();
            driver.bind_host_sound_backend(&mut arena, &audio).unwrap();
            Self {
                arena,
                audio,
                driver,
                draw: MenuDrawResources::new(
                    MenuTextDatabase::new(),
                    texture_factory(Images(Ok(tga()))),
                ),
                installation,
                scratch,
            }
        }

        fn host(&mut self) -> MenuSessionResources<'_> {
            MenuSessionResources::new(
                &mut self.arena,
                &self.audio,
                &mut self.draw,
                &self.installation,
            )
        }

        fn state(&self, sound: SoundKey) -> SoundState {
            self.audio.sound(sound.frame().0).unwrap()
        }
    }

    #[test]
    fn text_and_textures_borrow_the_canonical_draw_owner() {
        let mut fixture = Fixture::new();
        fixture
            .draw
            .text_database_mut()
            .load_from_bytes(&text_image(b"first"))
            .unwrap();
        let text = fixture.draw.text(42).unwrap().unwrap();
        let direct_texture = fixture
            .draw
            .create_alpha_texture(b"direct.tga")
            .unwrap()
            .unwrap();
        let borrowed_texture = {
            let mut host = fixture.host();
            assert!(host.text(0).unwrap().is_none());
            assert!(Arc::ptr_eq(&host.text(42).unwrap().unwrap(), &text));
            host.release_texture(direct_texture).unwrap();
            host.create_alpha_texture(b"borrowed.tga").unwrap().unwrap()
        };
        assert!(fixture.draw.texture_surface(direct_texture).is_err());
        assert_eq!(
            fixture.draw.texture_surface(borrowed_texture).unwrap().bgra,
            [3, 2, 1, 255]
        );
        fixture.draw.release_texture(borrowed_texture).unwrap();
        assert!(fixture.draw.texture_surface(borrowed_texture).is_err());
        fixture
            .draw
            .text_database_mut()
            .load_from_bytes(&text_image(b"second"))
            .unwrap();
        assert_eq!(
            fixture.host().text(42).unwrap().unwrap().as_ref(),
            b"second"
        );
        assert_eq!(text.as_ref(), b"first");
        assert_eq!(fixture.arena.live_frame_count(), 0);
        assert_eq!(fixture.audio.live_sound_count().unwrap(), 0);
    }

    #[test]
    fn unsupported_text_probe_reaches_the_canonical_error() {
        let mut fixture = Fixture::new();
        let mut image = text_image(b"single entry");
        image[..4].copy_from_slice(&1u32.to_le_bytes());
        fixture
            .draw
            .text_database_mut()
            .load_from_bytes(&image)
            .unwrap();
        assert_eq!(fixture.draw.text_database().len(), 1);
        let expected = fixture.draw.text(42).unwrap_err();
        assert_eq!(fixture.host().text(42), Err(expected));
    }

    #[test]
    fn texture_native_failure_and_host_error_are_not_coalesced() {
        let mut fixture = Fixture::new();
        fixture.draw = MenuDrawResources::new(
            MenuTextDatabase::new(),
            texture_factory(Images(Err(TextureImageError::Missing))),
        );
        assert!(fixture
            .host()
            .create_alpha_texture(b"missing.tga")
            .unwrap()
            .is_none());
        fixture.draw = MenuDrawResources::new(
            MenuTextDatabase::new(),
            texture_factory(Images(Err(TextureImageError::Backend(
                "synthetic I/O".into(),
            )))),
        );
        assert!(fixture
            .host()
            .create_alpha_texture(b"interrupted.tga")
            .is_err());
    }

    #[test]
    fn factory_and_release_use_the_original_frame_and_mixer_identity() {
        let mut fixture = Fixture::new();
        let sound = fixture.host().create_sound_frame().unwrap();
        let identity = fixture.arena.sound(sound).unwrap().audio_id().clone();
        assert_eq!(fixture.arena.frame(sound.frame()).unwrap().references, 1);
        assert_eq!(
            fixture
                .driver
                .host_sound_backend()
                .unwrap()
                .sounds()
                .unwrap(),
            [sound]
        );
        assert_eq!(
            fixture.audio.sound_frame_owner(sound.frame().0).unwrap(),
            SoundFrameOwner::Scene(sound.frame())
        );
        fixture.arena.retain(sound.frame()).unwrap();
        fixture.host().release_sound(sound).unwrap();
        assert_eq!(fixture.arena.frame(sound.frame()).unwrap().references, 1);
        fixture.audio.validate_sound_id(&identity).unwrap();
        assert_eq!(
            fixture
                .driver
                .host_sound_backend()
                .unwrap()
                .sounds()
                .unwrap(),
            [sound]
        );
        fixture.arena.release(sound.frame()).unwrap();
        assert!(fixture.arena.sound(sound).is_err());
        assert!(fixture.audio.validate_sound_id(&identity).is_err());
        assert_eq!(fixture.audio.live_sound_count().unwrap(), 0);
        assert!(fixture
            .driver
            .host_sound_backend()
            .unwrap()
            .sounds()
            .unwrap()
            .is_empty());
        assert!(fixture.host().release_sound(sound).is_err());
    }

    #[test]
    fn real_pcm_open_setters_and_immediate_playback_reach_the_same_owner() {
        let mut fixture = Fixture::new();
        fixture.scratch.write_sounds();
        fixture
            .audio
            .set_listener([0.0; 3], [1.0, 0.0, 0.0], None)
            .unwrap();
        let sound = fixture.host().create_sound_frame().unwrap();
        let identity = fixture.arena.sound(sound).unwrap().audio_id().clone();
        {
            let mut host = fixture.host();
            host.set_sound_type(sound, SoundKind::Ambient).unwrap();
            assert_eq!(
                host.open_sound(sound, b"sounds\\menuopen.wav", 8).unwrap(),
                0
            );
            host.set_out_volume(sound, f32::from_bits(0x3f00_0001))
                .unwrap();
            host.set_volume(sound, f32::from_bits(0x3e80_0001)).unwrap();
            host.set_on(sound, true, false).unwrap();
        }
        let state = fixture.state(sound);
        assert_eq!(state.sound_type, SoundKind::Ambient as u32);
        assert_eq!(state.open_flags, 0x218);
        assert_eq!(state.source.as_ref().unwrap().name, "sounds\\menuopen.wav");
        let source = state.source.as_ref().unwrap();
        assert_eq!(
            (source.sample_rate, source.channels, source.frames()),
            (4, 1, 4)
        );
        assert_eq!(state.out_volume.to_bits(), 0x3f00_0001);
        assert_eq!(state.volume.to_bits(), 0x3e80_0001);
        assert!(state.on);
        assert!(!state.voice_playing);
        fixture.host().update_sound(sound).unwrap();
        assert!(fixture.state(sound).voice_playing);
        let mut output = [0.0; 8];
        fixture.audio.mix_stereo(4, &mut output).unwrap();
        let expected = [0.25, 0.25, 0.5, 0.5, 0.0, 0.0, -0.25, -0.25]
            .map(|sample| sample * f32::from_bits(0x3e80_0001));
        assert_eq!(output.map(f32::to_bits), expected.map(f32::to_bits));
        fixture.host().set_on(sound, false, true).unwrap();
        assert!(!fixture.state(sound).on);
        assert!(!fixture.state(sound).voice_playing);
        fixture.host().set_on(sound, true, true).unwrap();
        assert!(fixture.state(sound).voice_playing);
        assert_eq!(fixture.arena.sound(sound).unwrap().audio_id(), &identity);
        fixture.host().release_sound(sound).unwrap();
    }

    #[test]
    fn open_preserves_missing_hresult_and_decode_error_prefix() {
        let mut fixture = Fixture::new();
        fixture.scratch.write_sounds();
        let sound = fixture.host().create_sound_frame().unwrap();
        assert_eq!(
            fixture
                .host()
                .open_sound(sound, b"sounds\\missing.wav", 0)
                .unwrap(),
            0x8000_0001u32 as i32
        );
        assert_eq!(
            fixture
                .host()
                .open_sound(sound, b"sounds\\menuopen.wav", 0)
                .unwrap(),
            0
        );
        assert!(fixture.state(sound).source.is_some());
        std::fs::write(
            fixture.scratch.0.join("sounds/menuclose.wav"),
            b"invalid WAV",
        )
        .unwrap();
        assert!(fixture
            .host()
            .open_sound(sound, b"sounds\\menuclose.wav", 0)
            .is_err());
        assert!(fixture.state(sound).source.is_none());
        assert_eq!(fixture.audio.live_sound_count().unwrap(), 1);
        assert!(fixture.audio.load_failures().unwrap().is_empty());
        fixture.host().release_sound(sound).unwrap();
    }

    #[test]
    fn unsupported_filename_encoding_is_a_host_error_without_lossy_open() {
        let mut fixture = Fixture::new();
        fixture.scratch.write_sounds();
        let sound = fixture.host().create_sound_frame().unwrap();
        fixture
            .host()
            .open_sound(sound, b"sounds\\menuopen.wav", 0)
            .unwrap();
        let before = fixture.state(sound).source.unwrap();
        for path in [&b"sounds\\\xff.wav"[..], "sounds\\\u{e9}.wav".as_bytes()] {
            assert!(fixture.host().open_sound(sound, path, 0).is_err());
            assert!(Arc::ptr_eq(
                fixture.state(sound).source.as_ref().unwrap(),
                &before
            ));
        }
        fixture.host().release_sound(sound).unwrap();
    }

    #[test]
    fn missing_or_foreign_factory_backend_stops_before_frame_creation() {
        let mut fixture = Fixture::new();
        let mut unbound = ModelArena::default();
        let mut host = MenuSessionResources::new(
            &mut unbound,
            &fixture.audio,
            &mut fixture.draw,
            &fixture.installation,
        );
        assert!(host.text(0).unwrap().is_none());
        let failure = host.create_sound_frame().unwrap_err();
        assert!(failure.frame.is_none());
        assert!(matches!(
            failure.error,
            ModelError::Sound(SoundError::BackendUnavailable)
        ));
        assert_eq!(unbound.live_frame_count(), 0);
        let foreign_audio = RuntimeAudio::default();
        let mut host = MenuSessionResources::new(
            &mut fixture.arena,
            &foreign_audio,
            &mut fixture.draw,
            &fixture.installation,
        );
        let failure = host.create_sound_frame().unwrap_err();
        assert!(failure.frame.is_none());
        assert!(matches!(
            failure.error,
            ModelError::Sound(SoundError::ForeignDriver)
        ));
        assert_eq!(fixture.arena.live_frame_count(), 0);
        assert_eq!(fixture.audio.live_sound_count().unwrap(), 0);
        assert_eq!(foreign_audio.live_sound_count().unwrap(), 0);
        assert!(fixture
            .driver
            .host_sound_backend()
            .unwrap()
            .sounds()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn partial_factory_failure_keeps_the_reached_creator_frame() {
        let mut fixture = Fixture::new();
        let mut probe = ModelArena::default();
        let first_frame = probe.create_sound_frame().unwrap();
        fixture.audio.create_sound_at(first_frame.0).unwrap();
        let retained = fixture.audio.sound_id_from_handle(first_frame.0).unwrap();
        let failure = fixture.host().create_sound_frame().unwrap_err();
        let partial = failure.frame.unwrap();
        assert_eq!(partial.frame(), first_frame);
        assert!(matches!(
            failure.error,
            ModelError::Sound(SoundError::Audio(_))
        ));
        assert_eq!(fixture.arena.live_frame_count(), 1);
        assert_eq!(fixture.arena.frame(partial.frame()).unwrap().references, 1);
        assert!(fixture.arena.sound(partial).is_err());
        assert_eq!(fixture.audio.live_sound_count().unwrap(), 1);
        fixture.audio.validate_sound_id(&retained).unwrap();
        assert!(fixture
            .driver
            .host_sound_backend()
            .unwrap()
            .sounds()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn foreign_frame_and_stale_audio_cannot_mutate_or_release_a_replacement() {
        let mut fixture = Fixture::new();
        let mut foreign = Fixture::new();
        let sound = fixture.host().create_sound_frame().unwrap();
        let foreign_sound = foreign.host().create_sound_frame().unwrap();
        assert_eq!(sound.frame(), foreign_sound.frame());
        assert_ne!(sound, foreign_sound);
        let before = format!("{:?}", fixture.state(sound));
        {
            let mut host = fixture.host();
            assert!(host
                .set_sound_type(foreign_sound, SoundKind::Ambient)
                .is_err());
            assert!(host
                .open_sound(foreign_sound, b"sounds\\menuopen.wav", 0)
                .is_err());
            assert!(host.set_out_volume(foreign_sound, 0.5).is_err());
            assert!(host.set_volume(foreign_sound, 0.5).is_err());
            assert!(host.update_sound(foreign_sound).is_err());
            assert!(host.set_on(foreign_sound, true, true).is_err());
            assert!(host.release_sound(foreign_sound).is_err());
        }
        assert_eq!(format!("{:?}", fixture.state(sound)), before);
        let identity = fixture.arena.sound(sound).unwrap().audio_id().clone();
        fixture.audio.release_sound(&identity).unwrap();
        fixture.audio.create_sound_at(sound.frame().0).unwrap();
        let replacement = fixture.audio.sound_id_from_handle(sound.frame().0).unwrap();
        fixture.audio.set_sound_volume(&replacement, 0.75).unwrap();
        let before = format!("{:?}", fixture.state(sound));
        {
            let mut host = fixture.host();
            assert!(host.set_sound_type(sound, SoundKind::Ambient).is_err());
            assert!(host.open_sound(sound, b"sounds\\menuopen.wav", 0).is_err());
            assert!(host.set_out_volume(sound, 0.5).is_err());
            assert!(host.set_volume(sound, 0.5).is_err());
            assert!(host.update_sound(sound).is_err());
            assert!(host.set_on(sound, true, true).is_err());
            assert!(host.release_sound(sound).is_err());
        }
        assert_eq!(format!("{:?}", fixture.state(sound)), before);
        assert_eq!(fixture.arena.sound(sound).unwrap().audio_id(), &identity);
        assert_eq!(fixture.arena.frame(sound.frame()).unwrap().references, 1);
        fixture.audio.validate_sound_id(&replacement).unwrap();
    }

    #[test]
    fn shared_lifecycle_loads_once_and_releases_the_real_resources() {
        let mut fixture = Fixture::new();
        fixture.scratch.write_sounds();
        fixture
            .audio
            .set_listener([0.0; 3], [1.0, 0.0, 0.0], None)
            .unwrap();
        let mut shared = MenuSharedResources::new();
        let mut first = shared.construct(&mut fixture.host()).unwrap();
        let sounds = MenuSharedSound::ALL.map(|kind| shared.sound(kind).unwrap().unwrap());
        let textures = MenuSharedTexture::ALL.map(|kind| shared.texture(kind).unwrap().unwrap());
        for (kind, sound) in MenuSharedSound::ALL.into_iter().zip(sounds) {
            let state = fixture.state(sound);
            assert_eq!(state.sound_type, SoundKind::Ambient as u32);
            assert_eq!(
                state.source.as_ref().unwrap().name.as_bytes(),
                kind.filename()
            );
            assert_eq!(state.open_flags, 0x200);
            assert_eq!(state.volume.to_bits(), 1.0f32.to_bits());
            assert_eq!(state.out_volume.to_bits(), 1.0f32.to_bits());
            assert!(!state.on);
            assert!(!state.voice_playing);
        }
        let mut second = shared.construct(&mut fixture.host()).unwrap();
        assert_eq!(shared.instance_count(), 2);
        assert_eq!(
            fixture
                .driver
                .host_sound_backend()
                .unwrap()
                .sounds()
                .unwrap(),
            sounds
        );
        shared.play_open_sound(&mut fixture.host(), true).unwrap();
        assert!(!fixture.state(sounds[0]).on);
        shared.play_open_sound(&mut fixture.host(), false).unwrap();
        assert!(fixture.state(sounds[0]).voice_playing);
        shared.destroy(&mut first, &mut fixture.host()).unwrap();
        assert_eq!(fixture.audio.live_sound_count().unwrap(), 3);
        shared.destroy(&mut second, &mut fixture.host()).unwrap();
        assert_eq!(shared.instance_count(), 0);
        assert_eq!(fixture.arena.live_frame_count(), 0);
        assert_eq!(fixture.audio.live_sound_count().unwrap(), 0);
        assert!(fixture
            .driver
            .host_sound_backend()
            .unwrap()
            .sounds()
            .unwrap()
            .is_empty());
        assert!(textures
            .into_iter()
            .all(|key| fixture.draw.texture_surface(key).is_err()));
    }

    #[test]
    fn native_open_failure_is_ignored_but_host_failure_stops_without_replay() {
        let mut fixture = Fixture::new();
        let mut shared = MenuSharedResources::new();
        let mut instance = shared.construct(&mut fixture.host()).unwrap();
        assert_eq!(shared.instance_count(), 1);
        assert_eq!(fixture.audio.live_sound_count().unwrap(), 3);
        for kind in MenuSharedSound::ALL {
            assert!(fixture
                .state(shared.sound(kind).unwrap().unwrap())
                .source
                .is_none());
        }
        shared.destroy(&mut instance, &mut fixture.host()).unwrap();
        std::fs::write(
            fixture.scratch.0.join("sounds/menuopen.wav"),
            b"invalid WAV",
        )
        .unwrap();
        let mut shared = MenuSharedResources::new();
        assert!(matches!(
            shared.construct(&mut fixture.host()),
            Err(MenuSharedResourcesError::Host {
                step: MenuSharedResourceStep::OpenSound(MenuSharedSound::Open),
                ..
            })
        ));
        assert_eq!(shared.instance_count(), 0);
        assert_eq!(fixture.audio.live_sound_count().unwrap(), 1);
        let sound = shared.retained_sound(MenuSharedSound::Open).unwrap();
        assert_eq!(fixture.state(sound).sound_type, SoundKind::Ambient as u32);
        assert!(shared.retained_sound(MenuSharedSound::Close).is_none());
        assert!(shared.retained_sound(MenuSharedSound::Change).is_none());
        assert!(matches!(
            shared.construct(&mut fixture.host()),
            Err(MenuSharedResourcesError::Interrupted(
                MenuSharedResourceStep::OpenSound(MenuSharedSound::Open)
            ))
        ));
        assert!(fixture.audio.load_failures().unwrap().is_empty());
        assert_eq!(fixture.audio.live_sound_count().unwrap(), 1);
    }

    struct InterruptedTextureRelease {
        factory: CpuMenuTextureFactory<Images>,
        calls: Rc<Cell<usize>>,
    }

    impl MenuTextureBackend for InterruptedTextureRelease {
        fn create_alpha_texture(&mut self, name: &[u8]) -> Result<Option<TextureKey>, String> {
            self.factory.create_alpha_texture(name)
        }

        fn release_texture(&mut self, key: TextureKey) -> Result<(), String> {
            self.calls.set(self.calls.get() + 1);
            self.factory.release_texture(key)?;
            Err("synthetic interruption after actual texture Release".into())
        }

        fn texture_surface(&self, key: TextureKey) -> Result<&CpuTextureSurface, String> {
            self.factory.texture_surface(key)
        }
    }

    #[test]
    fn interrupted_release_keeps_reached_effects_and_cannot_replay() {
        let mut fixture = Fixture::new();
        let calls = Rc::new(Cell::new(0));
        fixture.draw = MenuDrawResources::new(
            MenuTextDatabase::new(),
            InterruptedTextureRelease {
                factory: texture_factory(Images(Ok(tga()))),
                calls: Rc::clone(&calls),
            },
        );
        let mut shared = MenuSharedResources::new();
        let mut instance = shared.construct(&mut fixture.host()).unwrap();
        let texture = shared.texture(MenuSharedTexture::MenuTop).unwrap().unwrap();
        let step = MenuSharedResourceStep::ReleaseTexture(MenuSharedTexture::MenuTop);
        assert!(
            matches!(shared.destroy(&mut instance, &mut fixture.host()), Err(MenuSharedResourcesError::Host { step: reached, .. }) if reached == step)
        );
        assert_eq!(shared.instance_count(), 0);
        assert_eq!(
            shared.retained_texture(MenuSharedTexture::MenuTop),
            Some(texture)
        );
        assert!(fixture.draw.texture_surface(texture).is_err());
        assert_eq!(shared.interrupted_step(), Some(step));
        assert_eq!(
            shared.destroy(&mut instance, &mut fixture.host()),
            Err(MenuSharedResourcesError::InstanceNotLive)
        );
        assert!(
            matches!(shared.construct(&mut fixture.host()), Err(MenuSharedResourcesError::Interrupted(reached)) if reached == step)
        );
        assert_eq!(calls.get(), 1);
        assert_eq!(fixture.audio.live_sound_count().unwrap(), 3);
    }
}
