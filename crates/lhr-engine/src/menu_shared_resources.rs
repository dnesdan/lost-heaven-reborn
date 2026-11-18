//! Explicit shared GM_Menu resource references and constructor/destructor count.
//! The operation host borrows the canonical texture, frame and audio owners.
//! This is not component creation, presentation, or a complete GM_Menu port.

#![forbid(unsafe_code)]

use crate::frame_sound::{SoundCreateFailure, SoundKey};
use crate::itexture::TextureKey;
use crate::menu_draw_resources::MenuComponentResources;
use crate::runtime_audio::SoundKind;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuSharedTexture {
    MenuTop,
    MenuBottom,
    Sponka,
    Progress,
}

impl MenuSharedTexture {
    pub const ALL: [Self; 4] = [
        Self::MenuTop,
        Self::MenuBottom,
        Self::Sponka,
        Self::Progress,
    ];

    pub fn filename(self) -> &'static [u8] {
        match self {
            Self::MenuTop => b"papir5a.tga",
            Self::MenuBottom => b"papir3.tga",
            Self::Sponka => b"sponka.tga",
            Self::Progress => b"kulicky.tga",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::MenuTop => 0,
            Self::MenuBottom => 1,
            Self::Sponka => 2,
            Self::Progress => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuSharedSound {
    Open,
    Close,
    Change,
}

impl MenuSharedSound {
    pub const ALL: [Self; 3] = [Self::Open, Self::Close, Self::Change];

    pub fn filename(self) -> &'static [u8] {
        match self {
            Self::Open => b"sounds\\menuopen.wav",
            Self::Close => b"sounds\\menuclose.wav",
            Self::Change => b"sounds\\menuchange.wav",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Open => 0,
            Self::Close => 1,
            Self::Change => 2,
        }
    }
}

/// Implement over the same arena/audio incarnation used by the scene factory.
/// None of these methods may manufacture a replacement identity from a frame
/// number. Release is the frame reference Release, not audio-only teardown.
pub trait MenuSharedSoundHost: MenuComponentResources {
    fn create_sound_frame(&mut self) -> Result<SoundKey, SoundCreateFailure>;
    fn set_sound_type(&mut self, sound: SoundKey, kind: SoundKind) -> Result<(), String>;
    /// Open(path, flags, NULL, NULL). Preserve native HRESULT as Ok(i32),
    /// including failure HRESULTs; Err is an interrupted host operation.
    fn open_sound(&mut self, sound: SoundKey, path: &[u8], flags: u32) -> Result<i32, String>;
    fn set_out_volume(&mut self, sound: SoundKey, volume: f32) -> Result<(), String>;
    fn set_volume(&mut self, sound: SoundKey, volume: f32) -> Result<(), String>;
    fn update_sound(&mut self, sound: SoundKey) -> Result<(), String>;
    fn set_on(&mut self, sound: SoundKey, on: bool, immediate: bool) -> Result<(), String>;
    /// Err can follow partial Release effects. Never replay that call.
    fn release_sound(&mut self, sound: SoundKey) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuSharedResourceStep {
    CreateTexture(MenuSharedTexture),
    CreateSound(MenuSharedSound),
    SetSoundType(MenuSharedSound),
    OpenSound(MenuSharedSound),
    SetOutVolume(MenuSharedSound),
    SetVolume(MenuSharedSound),
    UpdateSound(MenuSharedSound),
    SetOn {
        sound: MenuSharedSound,
        on: bool,
        immediate: bool,
    },
    ReleaseTexture(MenuSharedTexture),
    ReleaseSound(MenuSharedSound),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuSharedResourcesError {
    Host {
        step: MenuSharedResourceStep,
        error: String,
    },
    Interrupted(MenuSharedResourceStep),
    ForeignInstance,
    InstanceNotLive,
}

impl std::fmt::Display for MenuSharedResourcesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host { step, error } => write!(formatter, "menu {step:?}: {error}"),
            Self::Interrupted(step) => {
                write!(formatter, "menu {step:?} is interrupted; replay refused")
            }
            Self::ForeignInstance => formatter.write_str("menu instance belongs to another owner"),
            Self::InstanceNotLive => {
                formatter.write_str("menu instance destructor already started")
            }
        }
    }
}

impl std::error::Error for MenuSharedResourcesError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuSharedInstanceState {
    Live,
    DestructorStarted,
    Destroyed,
}

/// Non-cloneable receipt for one reached constructor count increment.
/// Dropping it does not run native destruction without an operation host.
#[derive(Debug)]
pub struct MenuSharedInstance {
    owner: Arc<()>,
    state: MenuSharedInstanceState,
}

impl MenuSharedInstance {
    pub fn is_live(&self) -> bool {
        self.state == MenuSharedInstanceState::Live
    }
}

/// One owner for the seven shared references, not another resource backend.
/// Interrupted host calls retain the reached prefix. They permanently block
/// automatic continuation, including after a caught panic during Release.
#[derive(Debug)]
pub struct MenuSharedResources {
    identity: Arc<()>,
    menu_count: i32,
    textures: [Option<TextureKey>; 4],
    sounds: [Option<SoundKey>; 3],
    interrupted: Option<MenuSharedResourceStep>,
}

impl Default for MenuSharedResources {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuSharedResources {
    pub fn new() -> Self {
        Self {
            identity: Arc::new(()),
            menu_count: 0,
            textures: [None; 4],
            sounds: [None; 3],
            interrupted: None,
        }
    }

    pub fn instance_count(&self) -> i32 {
        self.menu_count
    }

    pub fn interrupted_step(&self) -> Option<MenuSharedResourceStep> {
        self.interrupted
    }

    pub fn texture(
        &self,
        kind: MenuSharedTexture,
    ) -> Result<Option<TextureKey>, MenuSharedResourcesError> {
        self.available()?;
        Ok(self.textures[kind.index()])
    }

    pub fn sound(
        &self,
        kind: MenuSharedSound,
    ) -> Result<Option<SoundKey>, MenuSharedResourcesError> {
        self.available()?;
        Ok(self.sounds[kind.index()])
    }

    /// Diagnostic ownership record only: not drawable, playable or retryable
    /// when the owner is interrupted, even if the backend still knows the key.
    pub fn retained_texture(&self, kind: MenuSharedTexture) -> Option<TextureKey> {
        self.textures[kind.index()]
    }

    /// A failed factory can retain its real constructed frame here. A failed
    /// Release can also retain a key whose backend object is already gone.
    pub fn retained_sound(&self, kind: MenuSharedSound) -> Option<SoundKey> {
        self.sounds[kind.index()]
    }

    /// Call after the per-menu base-field constructor prefix, before Create.
    /// The first instance loads; only a returned load prefix increments count.
    pub fn construct<Host: MenuSharedSoundHost + ?Sized>(
        &mut self,
        host: &mut Host,
    ) -> Result<MenuSharedInstance, MenuSharedResourcesError> {
        self.available()?;
        if self.menu_count == 0 {
            self.load_resources(host)?;
        }
        self.menu_count = self.menu_count.wrapping_add(1);
        Ok(MenuSharedInstance {
            owner: self.identity.clone(),
            state: MenuSharedInstanceState::Live,
        })
    }

    /// Count decrements before last-instance unloading; zero is assigned only
    /// after UnloadResources returns. A started destructor is never replayed.
    pub fn destroy<Host: MenuSharedSoundHost + ?Sized>(
        &mut self,
        instance: &mut MenuSharedInstance,
        host: &mut Host,
    ) -> Result<(), MenuSharedResourcesError> {
        self.check_instance(instance)?;
        self.available()?;
        instance.state = MenuSharedInstanceState::DestructorStarted;
        self.menu_count = self.menu_count.wrapping_sub(1);
        if self.menu_count <= 0 {
            self.unload_resources(host)?;
            self.menu_count = 0;
        }
        instance.state = MenuSharedInstanceState::Destroyed;
        Ok(())
    }

    pub(crate) fn check_instance(
        &self,
        instance: &MenuSharedInstance,
    ) -> Result<(), MenuSharedResourcesError> {
        if !Arc::ptr_eq(&self.identity, &instance.owner) {
            return Err(MenuSharedResourcesError::ForeignInstance);
        }
        if !instance.is_live() {
            return Err(MenuSharedResourcesError::InstanceNotLive);
        }
        Ok(())
    }

    /// Only the shared-sound part of Create: call after OnCreate, before the
    /// parent's OnCreateChild and final OnChange. Read closing at that point.
    pub fn play_open_sound<Host: MenuSharedSoundHost + ?Sized>(
        &mut self,
        host: &mut Host,
        closing: bool,
    ) -> Result<(), MenuSharedResourcesError> {
        self.available()?;
        if !closing {
            if let Some(sound) = self.sounds[MenuSharedSound::Open.index()] {
                for on in [false, true] {
                    self.call_host(
                        MenuSharedResourceStep::SetOn {
                            sound: MenuSharedSound::Open,
                            on,
                            immediate: true,
                        },
                        || host.set_on(sound, on, true),
                    )?;
                }
            }
        }
        Ok(())
    }

    fn available(&self) -> Result<(), MenuSharedResourcesError> {
        match self.interrupted {
            Some(step) => Err(MenuSharedResourcesError::Interrupted(step)),
            None => Ok(()),
        }
    }

    fn call_host<Output>(
        &mut self,
        step: MenuSharedResourceStep,
        operation: impl FnOnce() -> Result<Output, String>,
    ) -> Result<Output, MenuSharedResourcesError> {
        self.interrupted = Some(step);
        let output = operation().map_err(|error| MenuSharedResourcesError::Host { step, error })?;
        self.interrupted = None;
        Ok(output)
    }

    fn load_resources<Host: MenuSharedSoundHost + ?Sized>(
        &mut self,
        host: &mut Host,
    ) -> Result<(), MenuSharedResourcesError> {
        for kind in MenuSharedTexture::ALL {
            if self.textures[kind.index()].is_none() {
                self.textures[kind.index()] = self
                    .call_host(MenuSharedResourceStep::CreateTexture(kind), || {
                        host.create_alpha_texture(kind.filename())
                    })?;
            }
        }
        for kind in MenuSharedSound::ALL {
            if self.sounds[kind.index()].is_some() {
                continue;
            }
            let step = MenuSharedResourceStep::CreateSound(kind);
            self.interrupted = Some(step);
            let sound = match host.create_sound_frame() {
                Ok(sound) => sound,
                Err(failure) => {
                    self.sounds[kind.index()] = failure.frame;
                    return Err(MenuSharedResourcesError::Host {
                        step,
                        error: failure.error.to_string(),
                    });
                }
            };
            self.sounds[kind.index()] = Some(sound);
            self.interrupted = None;
            self.call_host(MenuSharedResourceStep::SetSoundType(kind), || {
                host.set_sound_type(sound, SoundKind::Ambient)
            })?;
            self.call_host(MenuSharedResourceStep::OpenSound(kind), || {
                host.open_sound(sound, kind.filename(), 0)
            })?;
            self.call_host(MenuSharedResourceStep::SetOutVolume(kind), || {
                host.set_out_volume(sound, 1.0_f32)
            })?;
            self.call_host(MenuSharedResourceStep::SetVolume(kind), || {
                host.set_volume(sound, 1.0_f32)
            })?;
            self.call_host(MenuSharedResourceStep::UpdateSound(kind), || {
                host.update_sound(sound)
            })?;
            self.call_host(
                MenuSharedResourceStep::SetOn {
                    sound: kind,
                    on: false,
                    immediate: false,
                },
                || host.set_on(sound, false, false),
            )?;
        }
        Ok(())
    }

    fn unload_resources<Host: MenuSharedSoundHost + ?Sized>(
        &mut self,
        host: &mut Host,
    ) -> Result<(), MenuSharedResourcesError> {
        for kind in MenuSharedTexture::ALL {
            if let Some(texture) = self.textures[kind.index()] {
                self.call_host(MenuSharedResourceStep::ReleaseTexture(kind), || {
                    host.release_texture(texture)
                })?;
            }
            self.textures[kind.index()] = None;
        }
        for kind in MenuSharedSound::ALL {
            if let Some(sound) = self.sounds[kind.index()] {
                self.call_host(MenuSharedResourceStep::ReleaseSound(kind), || {
                    host.release_sound(sound)
                })?;
                self.sounds[kind.index()] = None;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_sound::{HostSoundBackend, SoundError};
    use crate::itexture::{CpuTextureStorage, TextureError, TextureResources, TextureState};
    use crate::model_loading::ModelArena;
    use crate::runtime_audio::{RuntimeAudio, SoundId};

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Call {
        CreateTexture(Vec<u8>),
        ReleaseTexture(TextureKey),
        CreateSound,
        SetSoundType(SoundKey, SoundKind),
        Open(SoundKey, Vec<u8>, u32),
        SetOutVolume(SoundKey, u32),
        SetVolume(SoundKey, u32),
        Update(SoundKey),
        SetOn(SoundKey, bool, bool),
        ReleaseSound(SoundKey),
    }

    struct Host {
        textures: TextureResources,
        storage: CpuTextureStorage,
        arena: ModelArena,
        audio: RuntimeAudio,
        calls: Vec<Call>,
        fail_at: Option<usize>,
        fail_after: bool,
        panic_after: bool,
        missing_texture: Option<MenuSharedTexture>,
        open_hresult: i32,
    }

    impl Host {
        fn new() -> Self {
            let audio = RuntimeAudio::default();
            let mut arena = ModelArena::default();
            arena
                .bind_host_sound_backend(HostSoundBackend::new(1, &audio))
                .unwrap();
            Self {
                textures: TextureResources::new().unwrap(),
                storage: CpuTextureStorage::default(),
                arena,
                audio,
                calls: Vec::new(),
                fail_at: None,
                fail_after: false,
                panic_after: false,
                missing_texture: None,
                open_hresult: 0,
            }
        }

        fn entered(&mut self, call: Call) -> Result<(), String> {
            self.calls.push(call);
            if self.fail_at == Some(self.calls.len()) && !self.fail_after {
                Err("synthetic host interruption before effect".into())
            } else {
                Ok(())
            }
        }

        fn completed(&self) -> Result<(), String> {
            if self.fail_at == Some(self.calls.len()) && self.fail_after {
                assert!(!self.panic_after, "synthetic host panic after effect");
                Err("synthetic host interruption after effect".into())
            } else {
                Ok(())
            }
        }

        fn audio_id(&self, sound: SoundKey) -> Result<SoundId, String> {
            self.arena
                .sound(sound)
                .map(|frame| frame.audio_id().clone())
                .map_err(|error| error.to_string())
        }
    }

    impl MenuComponentResources for Host {
        fn text(&mut self, _text_id: u32) -> Result<Option<Arc<[u8]>>, String> {
            panic!("shared menu lifecycle must not query the text database")
        }

        fn create_alpha_texture(&mut self, name: &[u8]) -> Result<Option<TextureKey>, String> {
            self.entered(Call::CreateTexture(name.to_vec()))?;
            let texture = if self
                .missing_texture
                .is_some_and(|kind| kind.filename() == name)
            {
                None
            } else {
                Some(
                    self.textures
                        .create_texture()
                        .map_err(|error| format!("{error:?}"))?,
                )
            };
            self.completed()?;
            Ok(texture)
        }

        fn release_texture(&mut self, texture: TextureKey) -> Result<(), String> {
            self.entered(Call::ReleaseTexture(texture))?;
            self.textures
                .release_texture(texture, &mut self.storage)
                .map_err(|error| format!("{error:?}"))?;
            self.completed()
        }
    }

    impl MenuSharedSoundHost for Host {
        fn create_sound_frame(&mut self) -> Result<SoundKey, SoundCreateFailure> {
            self.entered(Call::CreateSound)
                .map_err(|_| SoundCreateFailure {
                    frame: None,
                    error: SoundError::AllocationUnavailable.into(),
                })?;
            let sound = self.arena.create_owned_sound_frame()?;
            self.completed().map_err(|_| SoundCreateFailure {
                frame: Some(sound),
                error: SoundError::RegistryUnavailable.into(),
            })?;
            Ok(sound)
        }

        fn set_sound_type(&mut self, sound: SoundKey, kind: SoundKind) -> Result<(), String> {
            self.entered(Call::SetSoundType(sound, kind))?;
            self.audio
                .set_sound_kind(&self.audio_id(sound)?, kind)
                .map_err(|error| error.to_string())?;
            self.completed()
        }

        fn open_sound(&mut self, sound: SoundKey, path: &[u8], flags: u32) -> Result<i32, String> {
            self.entered(Call::Open(sound, path.to_vec(), flags))?;
            self.audio_id(sound)?;
            self.completed()?;
            Ok(self.open_hresult)
        }

        fn set_out_volume(&mut self, sound: SoundKey, volume: f32) -> Result<(), String> {
            self.entered(Call::SetOutVolume(sound, volume.to_bits()))?;
            self.audio
                .set_sound_outside_volume(&self.audio_id(sound)?, volume)
                .map_err(|error| error.to_string())?;
            self.completed()
        }

        fn set_volume(&mut self, sound: SoundKey, volume: f32) -> Result<(), String> {
            self.entered(Call::SetVolume(sound, volume.to_bits()))?;
            self.audio
                .set_sound_volume(&self.audio_id(sound)?, volume)
                .map_err(|error| error.to_string())?;
            self.completed()
        }

        fn update_sound(&mut self, sound: SoundKey) -> Result<(), String> {
            self.entered(Call::Update(sound))?;
            self.audio
                .update_sound(&self.audio_id(sound)?)
                .map_err(|error| error.to_string())?;
            self.completed()
        }

        fn set_on(&mut self, sound: SoundKey, on: bool, immediate: bool) -> Result<(), String> {
            self.entered(Call::SetOn(sound, on, immediate))?;
            self.audio
                .set_sound_playback(&self.audio_id(sound)?, on, immediate)
                .map_err(|error| error.to_string())?;
            self.completed()
        }

        fn release_sound(&mut self, sound: SoundKey) -> Result<(), String> {
            self.entered(Call::ReleaseSound(sound))?;
            self.arena.sound(sound).map_err(|error| error.to_string())?;
            self.arena
                .release(sound.frame())
                .map_err(|error| error.to_string())?;
            self.completed()
        }
    }

    fn sound_keys(resources: &MenuSharedResources) -> [SoundKey; 3] {
        MenuSharedSound::ALL.map(|kind| resources.sound(kind).unwrap().unwrap())
    }

    fn texture_keys(resources: &MenuSharedResources) -> [TextureKey; 4] {
        MenuSharedTexture::ALL.map(|kind| resources.texture(kind).unwrap().unwrap())
    }

    fn load_calls(sounds: [SoundKey; 3]) -> Vec<Call> {
        let mut calls: Vec<_> = MenuSharedTexture::ALL
            .map(|kind| Call::CreateTexture(kind.filename().to_vec()))
            .into();
        for (kind, sound) in MenuSharedSound::ALL.into_iter().zip(sounds) {
            calls.extend([
                Call::CreateSound,
                Call::SetSoundType(sound, SoundKind::Ambient),
                Call::Open(sound, kind.filename().to_vec(), 0),
                Call::SetOutVolume(sound, 1.0_f32.to_bits()),
                Call::SetVolume(sound, 1.0_f32.to_bits()),
                Call::Update(sound),
                Call::SetOn(sound, false, false),
            ]);
        }
        calls
    }

    fn unload_calls(textures: [TextureKey; 4], sounds: [SoundKey; 3]) -> Vec<Call> {
        textures
            .into_iter()
            .map(Call::ReleaseTexture)
            .chain(sounds.into_iter().map(Call::ReleaseSound))
            .collect()
    }

    #[test]
    fn first_constructor_loads_in_order_and_only_last_destructor_releases() {
        let mut resources = MenuSharedResources::new();
        let mut host = Host::new();
        let mut first = resources.construct(&mut host).unwrap();
        let sounds = sound_keys(&resources);
        let textures = texture_keys(&resources);
        let load = load_calls(sounds);
        assert_eq!(host.calls, load);
        assert_eq!(resources.instance_count(), 1);
        for sound in sounds {
            let state = host.audio.sound(sound.frame().0).unwrap();
            assert_eq!(state.sound_type, SoundKind::Ambient as u32);
            assert_eq!(state.volume.to_bits(), 1.0_f32.to_bits());
            assert_eq!(state.out_volume.to_bits(), 1.0_f32.to_bits());
            assert!(!state.on);
        }
        let mut second = resources.construct(&mut host).unwrap();
        assert_eq!(host.calls, load);
        assert_eq!(resources.instance_count(), 2);
        resources.destroy(&mut first, &mut host).unwrap();
        assert_eq!(resources.instance_count(), 1);
        assert_eq!(host.calls, load);
        for texture in textures {
            let native = host.textures.texture(texture).unwrap();
            assert_eq!(native.state, TextureState::Live);
            assert_eq!(native.references, 1);
            assert_eq!(native.external_references, 1);
        }
        resources.destroy(&mut second, &mut host).unwrap();
        assert_eq!(resources.instance_count(), 0);
        assert_eq!(host.calls[load.len()..], unload_calls(textures, sounds));
        assert_eq!(resources.textures, [None; 4]);
        assert_eq!(resources.sounds, [None; 3]);
        assert_eq!(host.audio.live_sound_count().unwrap(), 0);
        for sound in sounds {
            assert!(host.arena.sound(sound).is_err());
        }
        for texture in textures {
            assert_eq!(
                host.textures.texture(texture),
                Err(TextureError::UnknownTexture(texture))
            );
        }
        let mut next = resources.construct(&mut host).unwrap();
        assert_ne!(sound_keys(&resources), sounds);
        assert_ne!(texture_keys(&resources), textures);
        resources.destroy(&mut next, &mut host).unwrap();
    }

    #[test]
    fn native_null_texture_and_failure_hresult_do_not_become_host_errors() {
        let mut resources = MenuSharedResources::new();
        let mut host = Host::new();
        host.missing_texture = Some(MenuSharedTexture::Sponka);
        host.open_hresult = 0x8000_0001_u32 as i32;
        let mut first = resources.construct(&mut host).unwrap();
        assert_eq!(resources.instance_count(), 1);
        assert_eq!(resources.texture(MenuSharedTexture::Sponka), Ok(None));
        assert_eq!(host.calls, load_calls(sound_keys(&resources)));
        let count = host.calls.len();
        let mut second = resources.construct(&mut host).unwrap();
        assert_eq!(host.calls.len(), count);
        resources.destroy(&mut first, &mut host).unwrap();
        resources.destroy(&mut second, &mut host).unwrap();
        assert_eq!(resources.instance_count(), 0);
        assert_eq!(host.calls.len(), count + 6);
    }

    #[test]
    fn every_interrupted_load_call_retains_prefix_without_increment_or_replay() {
        for fail_at in 1..=25 {
            let mut resources = MenuSharedResources::new();
            let mut host = Host::new();
            host.fail_at = Some(fail_at);
            host.fail_after = true;
            assert!(matches!(
                resources.construct(&mut host),
                Err(MenuSharedResourcesError::Host { .. })
            ));
            assert_eq!(resources.instance_count(), 0);
            assert_eq!(host.calls.len(), fail_at);
            let step = resources.interrupted_step().unwrap();
            assert!(matches!(
                resources.construct(&mut host),
                Err(MenuSharedResourcesError::Interrupted(actual)) if actual == step
            ));
            assert_eq!(host.calls.len(), fail_at);
            assert!(host
                .calls
                .iter()
                .all(|call| !matches!(call, Call::ReleaseSound(_) | Call::ReleaseTexture(_))));
            if fail_at >= 5 {
                for kind in MenuSharedTexture::ALL {
                    assert!(resources.retained_texture(kind).is_some());
                }
            }
        }
    }

    #[test]
    fn failed_sound_factory_retains_only_the_real_reached_frame() {
        for after in [false, true] {
            let mut resources = MenuSharedResources::new();
            let mut host = Host::new();
            host.fail_at = Some(5);
            host.fail_after = after;
            assert!(resources.construct(&mut host).is_err());
            assert_eq!(resources.instance_count(), 0);
            assert_eq!(
                resources.interrupted_step(),
                Some(MenuSharedResourceStep::CreateSound(MenuSharedSound::Open))
            );
            let sound = resources.retained_sound(MenuSharedSound::Open);
            assert_eq!(sound.is_some(), after);
            if let Some(sound) = sound {
                host.arena.sound(sound).unwrap();
            }
            assert_eq!(host.audio.live_sound_count().unwrap(), usize::from(after));
            assert_eq!(host.calls.len(), 5);
        }
    }

    #[test]
    fn interrupted_release_keeps_failed_slot_and_never_repeats_count_or_release() {
        for fail_after in [false, true] {
            for release_index in 0..7 {
                let mut resources = MenuSharedResources::new();
                let mut host = Host::new();
                let mut instance = resources.construct(&mut host).unwrap();
                let textures = texture_keys(&resources);
                let sounds = sound_keys(&resources);
                let load_len = host.calls.len();
                host.fail_at = Some(load_len + release_index + 1);
                host.fail_after = fail_after;
                let step = if release_index < 4 {
                    MenuSharedResourceStep::ReleaseTexture(MenuSharedTexture::ALL[release_index])
                } else {
                    MenuSharedResourceStep::ReleaseSound(MenuSharedSound::ALL[release_index - 4])
                };
                assert!(matches!(
                    resources.destroy(&mut instance, &mut host),
                    Err(MenuSharedResourcesError::Host { step: actual, .. }) if actual == step
                ));
                assert!(!instance.is_live());
                assert_eq!(resources.instance_count(), 0);
                assert_eq!(resources.interrupted_step(), Some(step));
                let calls = host.calls.clone();
                assert_eq!(
                    host.calls[load_len..],
                    unload_calls(textures, sounds)[..=release_index]
                );
                for (index, kind) in MenuSharedTexture::ALL.into_iter().enumerate() {
                    let texture = textures[index];
                    assert_eq!(
                        resources.retained_texture(kind),
                        (index >= release_index).then_some(texture)
                    );
                    if index < release_index || (index == release_index && fail_after) {
                        assert_eq!(
                            host.textures.texture(texture),
                            Err(TextureError::UnknownTexture(texture))
                        );
                    } else {
                        let native = host.textures.texture(texture).unwrap();
                        assert_eq!(native.state, TextureState::Live);
                        assert_eq!(native.references, 1);
                        assert_eq!(native.external_references, 1);
                    }
                }
                for (index, kind) in MenuSharedSound::ALL.into_iter().enumerate() {
                    let sound = sounds[index];
                    assert_eq!(
                        resources.retained_sound(kind),
                        (index + 4 >= release_index).then_some(sound)
                    );
                    if index + 4 < release_index || (index + 4 == release_index && fail_after) {
                        assert!(host.arena.sound(sound).is_err());
                    } else {
                        host.arena.sound(sound).unwrap();
                    }
                }
                let released_sounds = (release_index + usize::from(fail_after)).saturating_sub(4);
                assert_eq!(host.audio.live_sound_count().unwrap(), 3 - released_sounds);
                assert_eq!(
                    resources.texture(MenuSharedTexture::MenuTop),
                    Err(MenuSharedResourcesError::Interrupted(step))
                );
                assert_eq!(
                    resources.sound(MenuSharedSound::Open),
                    Err(MenuSharedResourcesError::Interrupted(step))
                );
                assert_eq!(
                    resources.destroy(&mut instance, &mut host),
                    Err(MenuSharedResourcesError::InstanceNotLive)
                );
                assert!(matches!(
                    resources.construct(&mut host),
                    Err(MenuSharedResourcesError::Interrupted(actual)) if actual == step
                ));
                assert_eq!(
                    resources.play_open_sound(&mut host, false),
                    Err(MenuSharedResourcesError::Interrupted(step))
                );
                assert_eq!(host.calls, calls);
                assert_eq!(resources.instance_count(), 0);
            }
        }
    }

    #[test]
    fn caught_release_panic_also_blocks_replay_and_retains_ownership_record() {
        let mut resources = MenuSharedResources::new();
        let mut host = Host::new();
        let mut instance = resources.construct(&mut host).unwrap();
        let texture = resources.texture(MenuSharedTexture::MenuTop).unwrap();
        host.fail_at = Some(host.calls.len() + 1);
        host.fail_after = true;
        host.panic_after = true;
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            resources.destroy(&mut instance, &mut host)
        }))
        .is_err());
        assert_eq!(resources.instance_count(), 0);
        assert_eq!(
            resources.retained_texture(MenuSharedTexture::MenuTop),
            texture
        );
        assert_eq!(
            resources.interrupted_step(),
            Some(MenuSharedResourceStep::ReleaseTexture(
                MenuSharedTexture::MenuTop
            ))
        );
        let count = host.calls.len();
        assert!(resources.destroy(&mut instance, &mut host).is_err());
        assert!(resources.construct(&mut host).is_err());
        assert_eq!(host.calls.len(), count);
    }

    #[test]
    fn receipts_are_owner_scoped_and_destructible_only_once() {
        let mut resources = MenuSharedResources::new();
        let mut other = MenuSharedResources::new();
        let mut host = Host::new();
        let mut instance = resources.construct(&mut host).unwrap();
        let count = host.calls.len();
        assert_eq!(
            other.destroy(&mut instance, &mut host),
            Err(MenuSharedResourcesError::ForeignInstance)
        );
        assert!(instance.is_live());
        assert_eq!(other.instance_count(), 0);
        assert_eq!(host.calls.len(), count);
        resources.destroy(&mut instance, &mut host).unwrap();
        let count = host.calls.len();
        assert_eq!(
            resources.destroy(&mut instance, &mut host),
            Err(MenuSharedResourcesError::InstanceNotLive)
        );
        assert_eq!(resources.instance_count(), 0);
        assert_eq!(host.calls.len(), count);
    }

    #[test]
    fn count_wraps_as_signed_dword_and_clamps_only_after_unload_returns() {
        let mut resources = MenuSharedResources::new();
        let mut host = Host::new();
        resources.menu_count = i32::MAX;
        let mut instance = resources.construct(&mut host).unwrap();
        assert_eq!(resources.instance_count(), i32::MIN);
        resources.destroy(&mut instance, &mut host).unwrap();
        assert_eq!(resources.instance_count(), i32::MAX);
        assert!(host.calls.is_empty());
        let mut instance = resources.construct(&mut host).unwrap();
        resources.menu_count = -2;
        resources.destroy(&mut instance, &mut host).unwrap();
        assert_eq!(resources.instance_count(), 0);
        assert!(host.calls.is_empty());

        let mut instance = resources.construct(&mut host).unwrap();
        resources.menu_count = -2;
        host.fail_at = Some(host.calls.len() + 1);
        assert!(resources.destroy(&mut instance, &mut host).is_err());
        assert_eq!(resources.instance_count(), -3);
    }

    #[test]
    fn create_sound_fragment_preserves_closing_and_immediate_false_true_order() {
        let mut resources = MenuSharedResources::new();
        let mut host = Host::new();
        let mut instance = resources.construct(&mut host).unwrap();
        let sound = resources.sound(MenuSharedSound::Open).unwrap().unwrap();
        let count = host.calls.len();
        resources.play_open_sound(&mut host, true).unwrap();
        assert_eq!(host.calls.len(), count);
        resources.play_open_sound(&mut host, false).unwrap();
        assert_eq!(
            host.calls[count..],
            [
                Call::SetOn(sound, false, true),
                Call::SetOn(sound, true, true)
            ]
        );
        resources.destroy(&mut instance, &mut host).unwrap();
    }

    #[test]
    fn interrupted_open_toggle_does_not_replay_or_reach_second_set_on() {
        let mut resources = MenuSharedResources::new();
        let mut host = Host::new();
        let mut instance = resources.construct(&mut host).unwrap();
        let sound = resources.sound(MenuSharedSound::Open).unwrap().unwrap();
        let count = host.calls.len();
        host.fail_at = Some(count + 1);
        host.fail_after = true;
        assert!(resources.play_open_sound(&mut host, false).is_err());
        assert_eq!(host.calls[count..], [Call::SetOn(sound, false, true)]);
        assert!(resources.play_open_sound(&mut host, false).is_err());
        assert!(resources.destroy(&mut instance, &mut host).is_err());
        assert_eq!(resources.instance_count(), 1);
        assert!(instance.is_live());
        assert_eq!(host.calls.len(), count + 1);
    }
}
