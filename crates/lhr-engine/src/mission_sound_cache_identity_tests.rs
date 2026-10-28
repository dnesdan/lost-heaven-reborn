//! Retained-identity regressions through the real mission sound cache.
use super::*;
use crate::driver_scene::DriverSceneOwner;
use crate::runtime_audio::RuntimeAudio;

struct Fixture {
    path: std::path::PathBuf,
    installation: lhr_assets::Installation,
    audio: RuntimeAudio,
    _driver: DriverSceneOwner,
    arena: ModelArena,
    caches: MissionCaches,
    memory: CpuMissionCacheMemory,
}

impl Fixture {
    fn new(with_directory: bool) -> Self {
        let path = std::env::temp_dir().join(format!(
            "lhr-cache-identity-{}-{}",
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
        if with_directory {
            caches
                .add_dir(MissionCacheKind::Sound, b"Sounds", &mut memory)
                .unwrap();
        }
        Self {
            path,
            installation,
            audio,
            _driver: driver,
            arena,
            caches,
            memory,
        }
    }

    fn open(&mut self, target: SoundKey) -> Result<i32, String> {
        self.caches
            .open_static_sound(
                &mut self.arena,
                &self.installation,
                target,
                b"a.wav",
                &mut self.memory,
            )
            .map_err(|error| format!("{error:?}"))
    }

    fn prime(&mut self) -> SoundKey {
        let first = self.arena.create_owned_sound_frame().unwrap();
        assert_eq!(self.open(first), Ok(0));
        self.caches
            .sound_open_progress()
            .unwrap()
            .cached_source
            .unwrap()
    }

    fn replace(&self, key: SoundKey) -> String {
        self.audio.sound_virtual(key.frame().0, 0, &[]).unwrap();
        self.audio.create_sound_at(key.frame().0).unwrap();
        let replacement = self.audio.sound_id_from_handle(key.frame().0).unwrap();
        self.audio.set_sound_volume(&replacement, 0.75).unwrap();
        self.audio.set_sound_enabled(&replacement, true).unwrap();
        self.snapshot(key)
    }

    fn snapshot(&self, key: SoundKey) -> String {
        format!("{:?}", self.audio.sound(key.frame().0).unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // Faulted owners remain faulted; only this test's authored files go away.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[test]
fn cache_miss_refuses_replaced_destination_and_retains_open_prefix() {
    let mut fixture = Fixture::new(true);
    let target = fixture.arena.create_owned_sound_frame().unwrap();
    let before = fixture.replace(target);
    assert!(fixture
        .open(target)
        .unwrap_err()
        .contains("stale sound identity"));
    assert_eq!(fixture.snapshot(target), before);
    assert!(fixture.caches.is_interrupted(MissionCacheKind::Sound));
    let progress = fixture.caches.sound_open_progress().unwrap();
    assert_eq!(progress.phase, SoundCachePhase::SoundOpen);
    assert_eq!(progress.target, target);
    assert_eq!(progress.sound_open_result, None);
    assert_eq!(progress.cached_source, None);
    assert_eq!(fixture.audio.live_source_count().unwrap(), 0);
}

#[test]
fn cache_hit_refuses_replaced_cached_source_before_destination_changes() {
    let mut fixture = Fixture::new(true);
    let cached = fixture.prime();
    let target = fixture.arena.create_owned_sound_frame().unwrap();
    let before = fixture.snapshot(target);
    let replaced = fixture.replace(cached);
    assert!(fixture
        .open(target)
        .unwrap_err()
        .contains("stale sound identity"));
    assert_eq!(fixture.snapshot(target), before);
    assert_eq!(fixture.snapshot(cached), replaced);
    assert!(fixture.caches.is_interrupted(MissionCacheKind::Sound));
    let progress = fixture.caches.sound_open_progress().unwrap();
    assert_eq!(progress.phase, SoundCachePhase::DuplicateHit);
    assert_eq!(progress.cached_source, Some(cached));
    assert_eq!(progress.sound_open_result, None);
}

#[test]
fn cache_hit_refuses_replaced_destination_without_copying_cached_source() {
    let mut fixture = Fixture::new(true);
    let cached = fixture.prime();
    let target = fixture.arena.create_owned_sound_frame().unwrap();
    let before = fixture.replace(target);
    let cached_before = fixture.snapshot(cached);
    assert!(fixture
        .open(target)
        .unwrap_err()
        .contains("stale sound identity"));
    assert_eq!(fixture.snapshot(target), before);
    assert_eq!(fixture.snapshot(cached), cached_before);
    assert!(fixture.caches.is_interrupted(MissionCacheKind::Sound));
    assert_eq!(
        fixture.caches.sound_open_progress().unwrap().phase,
        SoundCachePhase::DuplicateHit
    );
}

#[test]
fn no_directory_path_does_not_eagerly_execute_an_unreached_audio_check() {
    let mut fixture = Fixture::new(false);
    let target = fixture.arena.create_owned_sound_frame().unwrap();
    let before = fixture.replace(target);
    assert_eq!(fixture.open(target), Ok(0x8000_0001u32 as i32));
    assert_eq!(fixture.snapshot(target), before);
    assert!(!fixture.caches.is_interrupted(MissionCacheKind::Sound));
    assert_eq!(
        fixture.caches.sound_open_progress().unwrap().phase,
        SoundCachePhase::Complete
    );
}
