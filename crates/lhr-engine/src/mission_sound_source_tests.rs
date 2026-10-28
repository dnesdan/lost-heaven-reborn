//! Cache integration with synthetic files and retained scene sound identities.
use super::*;
use crate::driver_scene::DriverSceneOwner;
use crate::runtime_audio::RuntimeAudio;

struct Fixture {
    path: std::path::PathBuf,
    installation: lhr_assets::Installation,
    audio: RuntimeAudio,
    arena: ModelArena,
    caches: MissionCaches,
    memory: CpuMissionCacheMemory,
    _driver: DriverSceneOwner,
}
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lhr-cache-source-identity-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join("Sounds")).unwrap();
        let mut wav = b"RIFF".to_vec();
        wav.extend(52u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16u32.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(1u16.to_le_bytes());
        wav.extend(4u32.to_le_bytes());
        wav.extend(8u32.to_le_bytes());
        wav.extend(2u16.to_le_bytes());
        wav.extend(16u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(16u32.to_le_bytes());
        wav.extend([1000i16; 8].into_iter().flat_map(i16::to_le_bytes));
        std::fs::write(path.join("Sounds/a.wav"), wav).unwrap();
        let installation = lhr_assets::Installation::open(&path).unwrap();
        let audio = RuntimeAudio::default();
        let mut arena = ModelArena::default();
        let mut driver = DriverSceneOwner::default();
        driver.bind_host_sound_backend(&mut arena, &audio).unwrap();
        let mut caches = MissionCaches::empty().unwrap();
        let mut memory = CpuMissionCacheMemory::default();
        caches
            .add_dir(MissionCacheKind::Sound, b"Sounds", &mut memory)
            .unwrap();
        Self {
            path,
            installation,
            audio,
            arena,
            caches,
            memory,
            _driver: driver,
        }
    }

    fn open(
        &mut self,
        target: SoundKey,
    ) -> Result<i32, SoundCacheOpenError<<CpuMissionCacheMemory as CStrBackend>::Error>> {
        self.caches.open_static_sound(
            &mut self.arena,
            &self.installation,
            target,
            b"a.wav",
            &mut self.memory,
        )
    }

    fn replace(&self, sound: SoundKey) -> String {
        self.audio.sound_virtual(sound.frame().0, 0, &[]).unwrap();
        let replacement = self.audio.create_scene_sound(sound.frame()).unwrap();
        self.audio.set_sound_volume(&replacement, 0.75).unwrap();
        self.audio.set_sound_enabled(&replacement, true).unwrap();
        format!("{:?}", self.audio.sound(sound.frame().0).unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn directory_open_refuses_stale_target_and_retains_interrupted_prefix() {
    let mut f = Fixture::new();
    let sound = f.arena.create_owned_sound_frame().unwrap();
    let before = f.replace(sound);
    assert!(matches!(f.open(sound), Err(SoundCacheOpenError::Audio(_))));
    assert_eq!(
        format!("{:?}", f.audio.sound(sound.frame().0).unwrap()),
        before
    );
    assert!(f.caches.is_interrupted(MissionCacheKind::Sound));
    assert_eq!(
        f.caches.sound_open_progress().unwrap().phase,
        SoundCachePhase::SoundOpen
    );
    // An interrupted cache is not replayed after resolving a fresh numeric handle.
    assert!(matches!(
        f.open(sound),
        Err(SoundCacheOpenError::Cache(MissionCacheError::Interrupted))
    ));
    assert_eq!(
        format!("{:?}", f.audio.sound(sound.frame().0).unwrap()),
        before
    );
}

#[test]
fn cache_hit_refuses_replaced_source_before_touching_destination() {
    let mut f = Fixture::new();
    let first = f.arena.create_owned_sound_frame().unwrap();
    assert_eq!(f.open(first).unwrap(), 0);
    let cached = f.caches.sound_open_progress().unwrap().cached_source.unwrap();
    let source_before = f.replace(cached);
    let target = f.arena.create_owned_sound_frame().unwrap();
    let target_before = format!("{:?}", f.audio.sound(target.frame().0).unwrap());
    assert!(matches!(f.open(target), Err(SoundCacheOpenError::Audio(_))));
    assert_eq!(
        format!("{:?}", f.audio.sound(cached.frame().0).unwrap()),
        source_before
    );
    assert_eq!(
        format!("{:?}", f.audio.sound(target.frame().0).unwrap()),
        target_before
    );
    assert!(f.caches.is_interrupted(MissionCacheKind::Sound));
    assert_eq!(
        f.caches.sound_open_progress().unwrap().phase,
        SoundCachePhase::DuplicateHit
    );
}

#[test]
fn cache_hit_refuses_replaced_destination_without_attaching_the_cached_source() {
    let mut f = Fixture::new();
    let first = f.arena.create_owned_sound_frame().unwrap();
    assert_eq!(f.open(first).unwrap(), 0);
    let target = f.arena.create_owned_sound_frame().unwrap();
    let before = f.replace(target);
    assert!(matches!(f.open(target), Err(SoundCacheOpenError::Audio(_))));
    assert_eq!(
        format!("{:?}", f.audio.sound(target.frame().0).unwrap()),
        before
    );
    assert!(f.caches.is_interrupted(MissionCacheKind::Sound));
    assert_eq!(f.audio.live_source_count().unwrap(), 1);
}
