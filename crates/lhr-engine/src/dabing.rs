//! Dubbing (speech) channel owner: `S_dabing_chanel[16]` and `G_DabingMorphActive`.
//!
//! Game.exe SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! Marker-only contract scope: typed channel/morph state plus the `DabingTick`
//! weight state machine. Device mixing and file I/O are explicitly deferred
//! (see the deferred list at the bottom of this header).
//!
//! Original layout (mafia-re `game/ui/g_dabing.h:46`):
//! `S_dabing_chanel` is 0x44 bytes; field offsets are kept as constants below
//! (`OFF_*`). `.bss`: `dabingChanel` at Game 0x6bdb50, `G_DabingMorphActive`
//! at Game 0x6bdb40 (mafia-re `game/ui/g_dabing.cpp:18-21`).
//!
//! Entry points (all Game RVAs):
//! `DabingInit` 0x5f4bc0, `DabingDone` 0x5f4c10, `DabingGetFreeChanel` 0x5f4ca0,
//! `DabingSetHuman` 0x5f4cd0, `DabingReset` 0x5f5060, `DabingSpeak` 0x5f5570,
//! `DabingIsSpeaking` 0x5f5c40, `DabingTick` 0x5f5dd0.
//! `DabingReset` is called by `C_recorder::Close` (Game 0x59c6a0,
//! mafia-re `game/mission/C_recorder.cpp:585`).
//!
//! Host-owned slot discipline (per AGENTS.md typed-owner boundaries):
//! - Speech audio attaches only as [`SoundId`] incarnations owned by
//!   [`RuntimeAudio`]; this owner never fabricates numeric sound handles and
//!   never calls the PCM mixer (`mix_stereo`: deferred to the device owner).
//! - Decoding `tables\dat\<speech>.dat` / `sounds\<speech>.wav` into
//!   [`PcmSource`] is deferred; [`DabingOwner::speak_marker`] records only the
//!   intake paths. The deferred decoder is [`PcmSource::from_wav`].
//! - `game_spatial_sound.rs` is touched only through host-owned slots: the
//!   kind-5 scene-sound case expects an already-owned [`SoundId`] from the
//!   scene-sound host (the original `"14_Coucou"` frame,
//!   mafia-re `game/ui/g_dabing.h:26-29`); no `GameSpatialSoundBackend` calls.
//! - Faces are host-owned [`DabingFaceId`] slots driven through
//!   [`DabingFaceSink::set_face_weight`] (the original `I3D_visual::SetWeight`).
//!   `human_*.rs` is never touched: [`DabingOwner::set_human_marker`] records
//!   only the opaque speaker slot.
//!
//! Explicitly deferred (not implemented here):
//! - File I/O for `tables\dat\*.dat` and `sounds\*.wav`, and subtitle-text
//!   intake (character count for the 30 ms/char + 1 s subtitle fade,
//!   mafia-re `game/ui/g_dabing.h:23-25`).
//! - Device playback/mixing (`RuntimeAudio::mix_stereo`) and voice
//!   prioritization.
//! - Script call sites of `DabingSpeak`/`DabingIsSpeaking`/`DabingSetHuman`
//!   and the `C_game::Tick` call site of `DabingTick`
//!   (mafia-re `game/ui/g_dabing.cpp:3`).
#![forbid(unsafe_code)]

use crate::runtime_audio::{PlaybackQuery, RuntimeAudio};

pub use crate::runtime_audio::SoundId;

/// Number of speaking channels (mafia-re `game/ui/g_dabing.h:16`).
pub const NUM_DABING_CHANELS: usize = 16;
/// Speech frames are 40 ms apart (mafia-re `game/ui/g_dabing.h:32`).
pub const DABING_FRAME_TIME_MS: u32 = 40;
/// The mouth closes over 200 ms after a speech (mafia-re `game/ui/g_dabing.h:33`).
pub const DABING_FADE_TIME_MS: u32 = 200;
/// Morph region levels cleared per tick (mafia-re `game/ui/g_dabing.h:64`).
pub const DABING_MORPH_LEVELS: u32 = 8;
/// Blend rate: `blend += elapsed_ms * 0.008`, i.e. full blend over 125 ms
/// (mafia-re `game/ui/g_dabing.cpp:195`).
pub const DABING_BLEND_PER_MS: f32 = 0.008;
/// Subtitle-only fade: 1 s plus 30 ms per character
/// (mafia-re `game/ui/g_dabing.h:23-25`).
pub const DABING_SUBTITLE_BASE_MS: u32 = 1000;
/// Per-character part of the subtitle-only fade (see above).
pub const DABING_SUBTITLE_PER_CHAR_MS: u32 = 30;
/// Scene-sound speech name for kind 5 without its own sound
/// (mafia-re `game/ui/g_dabing.h:26-29`).
pub const DABING_SCENE_SOUND_NAME: &str = "14_Coucou";

/// `S_dabing_chanel` field offsets within its 0x44 bytes
/// (mafia-re `game/ui/g_dabing.h:46-61`).
pub const CHANEL_SIZE: usize = 0x44;
/// `data` at 0x0 (`tables\dat\<speech>.dat` frames).
pub const CHANEL_OFF_DATA: usize = 0x0;
/// `time` at 0x4 (speech clock).
pub const CHANEL_OFF_TIME: usize = 0x4;
/// `fade_time` at 0x8 (`DABING_STATE_FADING`/`DABING_STATE_SUBTITLE` remainder).
pub const CHANEL_OFF_FADE_TIME: usize = 0x8;
/// `state` at 0xc (`DABING_STATE_*`).
pub const CHANEL_OFF_STATE: usize = 0xc;
/// `sound` at 0x14 (`sounds\<speech>.wav` voice).
pub const CHANEL_OFF_SOUND: usize = 0x14;
/// `speaker_frame` at 0x18 (speaker frame sound links to).
pub const CHANEL_OFF_SPEAKER_FRAME: usize = 0x18;
/// `face` at 0x1c (speaker face receiving weights).
pub const CHANEL_OFF_FACE: usize = 0x1c;
/// `weight[6]` at 0x28 (last face weights; next speech blends from them).
pub const CHANEL_OFF_WEIGHT: usize = 0x28;
/// `blend` at 0x40 (0..1 progress away from `weight[]`).
pub const CHANEL_OFF_BLEND: usize = 0x40;

/// Channel state: free; `DabingGetFreeChanel` takes it
/// (mafia-re `game/ui/g_dabing.h:19`).
pub const DABING_STATE_FREE: i32 = 0;
/// Channel state: speaking; face follows the speech frames
/// (mafia-re `game/ui/g_dabing.h:20`).
pub const DABING_STATE_SPEAKING: i32 = 2;
/// Channel state: speech ended; mouth closes over `fade_time`
/// (mafia-re `game/ui/g_dabing.h:21-22`).
pub const DABING_STATE_FADING: i32 = 3;
/// Channel state: subtitle-only speech, no sound or data
/// (mafia-re `game/ui/g_dabing.h:23-25`).
pub const DABING_STATE_SUBTITLE: i32 = 4;
/// Channel state: kind-5 speech borrowing the scene sound; time comes from the
/// sound position, never ends by itself
/// (mafia-re `game/ui/g_dabing.h:26-29`).
pub const DABING_STATE_SCENE_SOUND: i32 = 5;

/// Valid `DabingSpeak` kinds, 1..=6
/// (mafia-re `game/ui/g_dabing.h:87`: "kind, 1 to 6, picks the sound's type
/// and range").
pub const DABING_KIND_MIN: u32 = 1;
/// See [`DABING_KIND_MIN`].
pub const DABING_KIND_MAX: u32 = 6;
/// Kind borrowing the scene sound when it has none of its own
/// (mafia-re `game/ui/g_dabing.h:26-29`).
pub const DABING_SCENE_SOUND_KIND: u32 = 5;

/// One decoded speech-data frame: face-level weights 0..255 in file order,
/// plus 2 opaque bytes (mafia-re `game/ui/g_dabing.h:38-43`).
/// The data block starts with a header the size of a frame whose first dword
/// counts the frames including the header.
pub type DabingFrameBytes = [u8; 8];

/// Host-owned face slot receiving `SetWeight(region, level, weight)`.
/// The host (scene/morph owner) resolves this to its `I3D_visual`; dabing
/// never touches raw visuals or `human_*.rs` actors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DabingFaceId(pub u32);

/// Opaque host-owned speaker slot recorded by `DabingSetHuman` (Game 0x5f4cd0).
/// Never resolved here; `human_*.rs` stays untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DabingSpeakerId(pub u32);

/// Host face-weight sink (the original `I3D_visual::SetWeight`).
pub trait DabingFaceSink {
    fn set_face_weight(&mut self, face: DabingFaceId, region: u32, level: u32, weight: f32);
}

/// Audio queries `DabingTick` needs, without device mixing.
/// `IsPlaying(false)` is the logical query and `GetCurrTime`/`GetPlayTime` are
/// position/duration (mafia-re `game/ui/g_dabing.cpp:130-136`).
pub trait DabingAudioProbe {
    fn dabing_logical_playing(&self, sound: &SoundId) -> bool;
    fn dabing_position_ms(&self, sound: &SoundId) -> u32;
    fn dabing_duration_ms(&self, sound: &SoundId) -> u32;
    /// Detach the voice; stale incarnations are ignored.
    fn dabing_release(&self, sound: &SoundId);
}

impl DabingAudioProbe for RuntimeAudio {
    fn dabing_logical_playing(&self, sound: &SoundId) -> bool {
        self.sound_is_playing(sound, PlaybackQuery::Logical)
            .unwrap_or(false)
    }
    fn dabing_position_ms(&self, sound: &SoundId) -> u32 {
        self.sound_position_ms(sound).unwrap_or(0)
    }
    fn dabing_duration_ms(&self, sound: &SoundId) -> u32 {
        self.sound_duration_ms(sound).unwrap_or(0)
    }
    fn dabing_release(&self, sound: &SoundId) {
        let _ = self.release_sound(sound);
    }
}

/// Pending file intake recorded by the `DabingSpeak` marker (Game 0x5f5570).
/// Paths only; reading/decoding is deferred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DabingSpeechIntake {
    pub speech: u32,
    pub subtitle: bool,
    pub kind: u32,
    /// `tables\dat\<speech>.dat` (frame weights intake, deferred read).
    pub dat_path: String,
    /// `sounds\<speech>.wav` ([`PcmSource`] intake, deferred read/decode).
    pub wav_path: String,
    /// Kind 5 without its own sound borrows the scene sound frame
    /// `"14_Coucou"` (mafia-re `game/ui/g_dabing.h:26-29`).
    pub expects_scene_sound: bool,
}

/// Build the deferred intake paths for a speech id.
pub fn dabing_intake_paths(speech: u32) -> (String, String) {
    (
        format!("tables\\dat\\{speech}.dat"),
        format!("sounds\\{speech}.wav"),
    )
}

/// Subtitle-only fade for a subtitle of `chars` characters:
/// one second plus 30 ms per character
/// (mafia-re `game/ui/g_dabing.h:23-25`).
pub fn dabing_subtitle_fade_ms(chars: u32) -> u32 {
    DABING_SUBTITLE_BASE_MS.saturating_add(chars.saturating_mul(DABING_SUBTITLE_PER_CHAR_MS))
}

/// One speaking channel (mafia-re `game/ui/g_dabing.h:46-61`).
/// `data_words` holds the decoded `.dat` block: element 0 is the frame count
/// header dword, elements 1.. are frames (`S_dabing_frame`, 8 bytes each).
/// The host/test injects decoded frames; file I/O stays deferred.
#[derive(Clone, Debug, Default)]
pub struct DabingChannel {
    pub data_words: Option<Vec<u32>>,
    pub time_ms: u32,
    pub fade_ms: u32,
    pub state: i32,
    pub sound: Option<SoundId>,
    pub speaker: Option<DabingSpeakerId>,
    pub face: Option<DabingFaceId>,
    /// Last face weights the next speech blends from
    /// (offset 0x28, mafia-re `game/ui/g_dabing.h:59`).
    pub weight: [f32; 6],
    /// Blend progress 0..1 away from [`DabingChannel::weight`]
    /// (offset 0x40, mafia-re `game/ui/g_dabing.h:60`).
    pub blend: f32,
}

impl DabingChannel {
    fn clear(&mut self) {
        *self = Self::default();
    }

    /// Frame count header (first dword counts frames including the header).
    fn frame_count(&self) -> Option<u32> {
        self.data_words
            .as_ref()
            .and_then(|words| words.first().copied())
    }

    /// Raw frame bytes for 0-based frame `index` (past the header).
    fn frame_bytes(&self, index: u32) -> Option<DabingFrameBytes> {
        let words = self.data_words.as_ref()?;
        let base = 1 + (index as usize) * 2;
        let lo = *words.get(base)?;
        let hi = *words.get(base + 1)?;
        let mut out = [0u8; 8];
        out[0..4].copy_from_slice(&lo.to_le_bytes());
        out[4..8].copy_from_slice(&hi.to_le_bytes());
        Some(out)
    }
}

/// One active face morph (mafia-re `game/ui/g_dabing.h:68-74`, 0x10 bytes):
/// regions 0 and 1 move from one level to another over `duration_ms`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DabingMorph {
    pub face: DabingFaceId,
    pub duration_ms: u32,
    pub time_ms: u32,
    /// Per region (0, 1): [from-level, to-level].
    pub level: [[u8; 2]; 2],
}

/// Marker + tick errors. Allocation failure mirrors `DabingGetFreeChanel`
/// falling back to the last channel; it is never fatal here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DabingError {
    BadChannel { channel: usize },
    BadKind { kind: u32 },
}

/// Typed owner of `dabingChanel[16]` (Game 0x6bdb50) and
/// `G_DabingMorphActive` (Game 0x6bdb40).
#[derive(Clone, Debug, Default)]
pub struct DabingOwner {
    channels: [DabingChannel; NUM_DABING_CHANELS],
    morphs: Vec<DabingMorph>,
}

impl DabingOwner {
    pub fn new() -> Self {
        Self::default()
    }

    /// `DabingInit` (Game 0x5f4bc0): clear channels and morphs
    /// (mafia-re `game/ui/g_dabing.cpp:24-28`).
    pub fn dabing_init(&mut self) {
        for channel in &mut self.channels {
            channel.clear();
        }
        self.morphs.clear();
    }

    /// `DabingDone` (Game 0x5f4c10): release sounds (except scene-sound
    /// channels, whose sound belongs to the scene), drop data, clear all
    /// (mafia-re `game/ui/g_dabing.cpp:32-44`).
    pub fn dabing_done(&mut self, audio: &impl DabingAudioProbe) {
        for channel in &mut self.channels {
            if channel.sound.is_some() && channel.state != DABING_STATE_SCENE_SOUND {
                if let Some(sound) = channel.sound.clone() {
                    audio.dabing_release(&sound);
                }
            }
            channel.sound = None;
            channel.data_words = None;
        }
        for channel in &mut self.channels {
            channel.clear();
        }
        self.morphs.clear();
    }

    /// `DabingReset` (Game 0x5f5060, PC only): each face gets its weights
    /// back (region 0..2: level 0 is 1, the rest 0), then data is dropped and
    /// sounds released (scene-sound voices belong to the scene and are kept),
    /// then channels and morphs are cleared
    /// (mafia-re `game/ui/g_dabing.cpp:62-92`).
    /// Called by `C_recorder::Close` (Game 0x59c6a0,
    /// mafia-re `game/mission/C_recorder.cpp:585`); the recorder-side wiring
    /// itself is deferred, this is the callee contract.
    pub fn dabing_reset(&mut self, faces: &mut impl DabingFaceSink, audio: &impl DabingAudioProbe) {
        for channel in &mut self.channels {
            if let Some(face) = channel.face {
                dabing_reset_face_weights(faces, face);
            }
            // Original order: data first, then the sound
            // (mafia-re `game/ui/g_dabing.cpp:84-87`).
            channel.data_words = None;
            if channel.sound.is_some() && channel.state != DABING_STATE_SCENE_SOUND {
                if let Some(sound) = channel.sound.clone() {
                    audio.dabing_release(&sound);
                }
            }
            channel.sound = None;
        }
        for channel in &mut self.channels {
            channel.clear();
        }
        self.morphs.clear();
    }

    /// `C_recorder::Close` hook marker (Game 0x59c6a0): the Close-side call of
    /// `DabingReset` (mafia-re `game/mission/C_recorder.cpp:585`).
    /// The actual `C_recorder` wiring is deferred; recorder owners call this.
    pub fn recorder_close_reset(
        &mut self,
        faces: &mut impl DabingFaceSink,
        audio: &impl DabingAudioProbe,
    ) {
        self.dabing_reset(faces, audio);
    }

    /// `DabingGetFreeChanel` (Game 0x5f4ca0): the last channel that is not
    /// speaking, or the last channel if all are
    /// (mafia-re `game/ui/g_dabing.cpp:47-53`).
    pub fn dabing_get_free_chanel(&self) -> usize {
        for index in (0..NUM_DABING_CHANELS).rev() {
            if self.channels[index].state == DABING_STATE_FREE {
                return index;
            }
        }
        NUM_DABING_CHANELS - 1
    }

    /// `DabingSetHuman` marker (Game 0x5f4cd0, stub in
    /// mafia-re `game/ui/g_dabing.cpp:56-59`): records the speaker slot only.
    /// Face/sound linkage that the original derives from `C_human` is
    /// deferred to the host via [`DabingOwner::attach_face`] /
    /// [`DabingOwner::attach_sound`].
    pub fn set_human_marker(
        &mut self,
        channel: usize,
        speaker: DabingSpeakerId,
    ) -> Result<(), DabingError> {
        let slot = self
            .channels
            .get_mut(channel)
            .ok_or(DabingError::BadChannel { channel })?;
        slot.speaker = Some(speaker);
        Ok(())
    }

    /// `DabingSpeak` marker (Game 0x5f5570, stub in
    /// mafia-re `game/ui/g_dabing.cpp:95-98`): validates kind 1..=6, records
    /// the deferred file intake, resets the speech clock and blend source.
    /// Sound/data attachment is deferred to [`DabingOwner::attach_sound`] /
    /// [`DabingOwner::attach_data`]; the host attaches the kind-5 scene sound
    /// (`"14_Coucou"`) as an already-owned [`SoundId`].
    pub fn speak_marker(
        &mut self,
        channel: usize,
        speech: u32,
        subtitle: bool,
        kind: u32,
    ) -> Result<DabingSpeechIntake, DabingError> {
        if !(DABING_KIND_MIN..=DABING_KIND_MAX).contains(&kind) {
            return Err(DabingError::BadKind { kind });
        }
        let slot = self
            .channels
            .get_mut(channel)
            .ok_or(DabingError::BadChannel { channel })?;
        let (dat_path, wav_path) = dabing_intake_paths(speech);
        let expects_scene_sound = kind == DABING_SCENE_SOUND_KIND;
        // Keep `weight[]` as the blend-from source; the new speech blends
        // away from it over 125 ms once frames apply.
        slot.time_ms = 0;
        slot.fade_ms = 0;
        slot.blend = 0.0;
        slot.state = if expects_scene_sound {
            DABING_STATE_SCENE_SOUND
        } else {
            DABING_STATE_SPEAKING
        };
        Ok(DabingSpeechIntake {
            speech,
            subtitle,
            kind,
            dat_path,
            wav_path,
            expects_scene_sound,
        })
    }

    /// Host attaches the decoded `.dat` block (count header + frames).
    /// File I/O stays deferred; the host/test supplies decoded words.
    pub fn attach_data(&mut self, channel: usize, data_words: Vec<u32>) -> Result<(), DabingError> {
        let slot = self
            .channels
            .get_mut(channel)
            .ok_or(DabingError::BadChannel { channel })?;
        slot.data_words = Some(data_words);
        Ok(())
    }

    /// Host attaches an already-owned voice for the channel.
    pub fn attach_sound(&mut self, channel: usize, sound: SoundId) -> Result<(), DabingError> {
        let slot = self
            .channels
            .get_mut(channel)
            .ok_or(DabingError::BadChannel { channel })?;
        slot.sound = Some(sound);
        Ok(())
    }

    /// Host binds the speaker face (derived from `C_human` in the original).
    pub fn attach_face(&mut self, channel: usize, face: DabingFaceId) -> Result<(), DabingError> {
        let slot = self
            .channels
            .get_mut(channel)
            .ok_or(DabingError::BadChannel { channel })?;
        slot.face = Some(face);
        Ok(())
    }

    /// Subtitle-only speech setup: neither sound nor data, only the subtitle
    /// fade of 30 ms per character plus a second
    /// (mafia-re `game/ui/g_dabing.h:23-25`). The subtitle text/intake itself
    /// is deferred; the host supplies the character count.
    pub fn begin_subtitle_only(
        &mut self,
        channel: usize,
        subtitle_chars: u32,
    ) -> Result<(), DabingError> {
        let slot = self
            .channels
            .get_mut(channel)
            .ok_or(DabingError::BadChannel { channel })?;
        slot.state = DABING_STATE_SUBTITLE;
        slot.fade_ms = dabing_subtitle_fade_ms(subtitle_chars);
        Ok(())
    }

    /// `DabingIsSpeaking` (Game 0x5f5c40, stub in
    /// mafia-re `game/ui/g_dabing.cpp:101-105`): working contract is
    /// "any non-free state still speaks", matching `DabingGetFreeChanel`'s
    /// `!state` test (mafia-re `game/ui/g_dabing.cpp:50`). The exact native
    /// predicate is unverified; script call sites are deferred.
    pub fn dabing_is_speaking(&self, channel: usize) -> Result<bool, DabingError> {
        let slot = self
            .channels
            .get(channel)
            .ok_or(DabingError::BadChannel { channel })?;
        Ok(slot.state != DABING_STATE_FREE)
    }

    /// Queue a face morph moved by [`DabingOwner::dabing_tick`]
    /// (`G_DabingMorphActive`, Game 0x6bdb40,
    /// mafia-re `game/ui/g_dabing.h:77`).
    pub fn push_morph(&mut self, morph: DabingMorph) {
        self.morphs.push(morph);
    }

    pub fn morph_count(&self) -> usize {
        self.morphs.len()
    }

    pub fn channel(&self, channel: usize) -> Result<&DabingChannel, DabingError> {
        self.channels
            .get(channel)
            .ok_or(DabingError::BadChannel { channel })
    }

    /// `DabingTick` (Game 0x5f5dd0): advance channel clocks and faces by
    /// `elapsed_ms`, then move queued morphs
    /// (mafia-re `game/ui/g_dabing.cpp:117-306`). No device mixing happens
    /// here; that stays deferred to the audio device owner.
    /// The `C_game::Tick` call site is deferred; tick owners call this.
    pub fn dabing_tick(
        &mut self,
        audio: &impl DabingAudioProbe,
        faces: &mut impl DabingFaceSink,
        elapsed_ms: u32,
    ) {
        for index in 0..NUM_DABING_CHANELS {
            // Borrow scope discipline: query audio first (immutable owner
            // view), then mutate the channel, mirroring the original's
            // reread of sound state per channel.
            let (has_sound, state) = {
                let channel = &self.channels[index];
                (channel.sound.is_some(), channel.state)
            };
            if state == DABING_STATE_FREE {
                continue;
            }
            if !has_sound {
                let channel = &mut self.channels[index];
                channel.time_ms = channel.time_ms.wrapping_add(elapsed_ms);
            }
            if state == DABING_STATE_SPEAKING {
                self.tick_speaking_clock(audio, index, elapsed_ms);
            }
            let state = self.channels[index].state;
            match state {
                DABING_STATE_SCENE_SOUND => {
                    // The sound's position, whether it plays or not
                    // (mafia-re `game/ui/g_dabing.cpp:171-176`).
                    let Some(sound) = self.channels[index].sound.clone() else {
                        continue;
                    };
                    let position = audio.dabing_position_ms(&sound);
                    let channel = &mut self.channels[index];
                    channel.time_ms = position;
                    let frame = position / DABING_FRAME_TIME_MS;
                    Self::apply_speaking_frame(faces, channel, frame, elapsed_ms);
                }
                DABING_STATE_SPEAKING => {
                    let frame = self.channels[index].time_ms / DABING_FRAME_TIME_MS;
                    let channel = &mut self.channels[index];
                    Self::apply_speaking_frame(faces, channel, frame, elapsed_ms);
                }
                DABING_STATE_FADING => {
                    Self::apply_fading_frame(faces, index, &mut self.channels, elapsed_ms);
                }
                DABING_STATE_SUBTITLE => {
                    let channel = &mut self.channels[index];
                    if channel.fade_ms > elapsed_ms {
                        channel.fade_ms -= elapsed_ms;
                    } else {
                        channel.fade_ms = 0;
                        channel.state = DABING_STATE_FREE;
                    }
                }
                _ => {}
            }
        }

        Self::tick_morphs(faces, &mut self.morphs, elapsed_ms);
    }

    /// Speaking clock for channels with/without a voice
    /// (mafia-re `game/ui/g_dabing.cpp:126-167`).
    fn tick_speaking_clock(
        &mut self,
        audio: &impl DabingAudioProbe,
        index: usize,
        elapsed_ms: u32,
    ) {
        let channel = &mut self.channels[index];
        let Some(sound) = channel.sound.clone() else {
            // Soundless: clock runs on; the speech ends a frame past the
            // data end, or immediately without data
            // (mafia-re `game/ui/g_dabing.cpp:151-166`).
            let frame = channel.time_ms / DABING_FRAME_TIME_MS;
            match channel.frame_count() {
                Some(count) => {
                    if frame >= count.saturating_sub(1) {
                        channel.fade_ms = DABING_FADE_TIME_MS;
                        channel.state = DABING_STATE_FADING;
                    }
                }
                None => {
                    channel.fade_ms = DABING_FADE_TIME_MS;
                    channel.state = DABING_STATE_FADING;
                }
            }
            return;
        };
        if audio.dabing_logical_playing(&sound) {
            let current = audio.dabing_position_ms(&sound);
            let play_time = audio.dabing_duration_ms(&sound);
            // Speech ends when the sound stopped or went back to its start
            // near its end (mafia-re `game/ui/g_dabing.cpp:133-143`).
            if channel.time_ms > current && channel.time_ms.wrapping_add(300) > play_time {
                channel.fade_ms = DABING_FADE_TIME_MS;
                channel.state = DABING_STATE_FADING;
            } else {
                channel.time_ms = current;
            }
        } else {
            channel.fade_ms = DABING_FADE_TIME_MS;
            channel.state = DABING_STATE_FADING;
        }
        let _ = elapsed_ms;
    }

    /// Speaking weights from the data frame at `frame`, blended in from the
    /// last weights over 125 ms (mafia-re `game/ui/g_dabing.cpp:177-221`).
    /// File byte order per channel: `w1,w0,w2,w4,w3,w5 = fr[0..5] / 255`.
    fn apply_speaking_frame(
        faces: &mut impl DabingFaceSink,
        channel: &mut DabingChannel,
        frame: u32,
        elapsed_ms: u32,
    ) {
        let Some(face) = channel.face else { return };
        let Some(count) = channel.frame_count() else {
            return;
        };
        if count == 0 {
            return;
        }
        let clamped = frame.min(count.saturating_sub(2));
        let Some(raw) = channel.frame_bytes(clamped) else {
            return;
        };
        let mut w = [0.0f32; 6];
        // Declaration order w0..w5 with frame overwrites, as in the original
        // (mafia-re `game/ui/g_dabing.cpp:179-192`).
        let w1 = f32::from(raw[0]) * (1.0 / 255.0);
        let w0 = f32::from(raw[1]) * (1.0 / 255.0);
        let w2 = f32::from(raw[2]) * (1.0 / 255.0);
        let w4 = f32::from(raw[3]) * (1.0 / 255.0);
        let w3 = f32::from(raw[4]) * (1.0 / 255.0);
        let w5 = f32::from(raw[5]) * (1.0 / 255.0);
        w[0] = w0;
        w[1] = w1;
        w[2] = w2;
        w[3] = w3;
        w[4] = w4;
        w[5] = w5;
        if channel.blend < 1.0 {
            channel.blend += elapsed_ms as f32 * DABING_BLEND_PER_MS;
            if channel.blend < 1.0 {
                let f = channel.blend;
                for (i, value) in w.iter_mut().enumerate() {
                    *value = *value * f + channel.weight[i] * (1.0 - f);
                }
            }
        }
        apply_face_weights(faces, face, &w);
    }

    /// Fading weights: the last frame scaled down to 0 with the time left;
    /// the scaled weights become the next speech's blend source
    /// (mafia-re `game/ui/g_dabing.cpp:222-266`).
    fn apply_fading_frame(
        faces: &mut impl DabingFaceSink,
        index: usize,
        channels: &mut [DabingChannel; NUM_DABING_CHANELS],
        elapsed_ms: u32,
    ) {
        let channel = &mut channels[index];
        if channel.fade_ms > elapsed_ms {
            channel.fade_ms -= elapsed_ms;
        } else {
            channel.fade_ms = 0;
            channel.state = DABING_STATE_FREE;
        }
        let (Some(face), Some(count)) = (channel.face, channel.frame_count()) else {
            return;
        };
        if count == 0 {
            return;
        }
        let Some(raw) = channel.frame_bytes(count.saturating_sub(2)) else {
            return;
        };
        let fade = (channel.fade_ms as f32) * (1.0 / DABING_FADE_TIME_MS as f32);
        let w1 = f32::from(raw[0]) * (1.0 / 255.0) * fade;
        let w0 = f32::from(raw[1]) * (1.0 / 255.0) * fade;
        let w2 = f32::from(raw[2]) * (1.0 / 255.0);
        let w4 = f32::from(raw[3]) * (1.0 / 255.0);
        let w3 = f32::from(raw[4]) * (1.0 / 255.0);
        let w5 = f32::from(raw[5]) * (1.0 / 255.0);
        let w = [w0, w1, w2, w3, w4, w5];
        channels[index].weight = w;
        apply_face_weights(faces, face, &w);
    }

    /// Queued morphs: clear regions 0..1, advance, drop at the end or lerp
    /// between levels (mafia-re `game/ui/g_dabing.cpp:279-305`).
    fn tick_morphs(
        faces: &mut impl DabingFaceSink,
        morphs: &mut Vec<DabingMorph>,
        elapsed_ms: u32,
    ) {
        let mut drop_at = Vec::new();
        for (position, morph) in morphs.iter_mut().enumerate() {
            for level in 0..DABING_MORPH_LEVELS {
                faces.set_face_weight(morph.face, 0, level, 0.0);
                faces.set_face_weight(morph.face, 1, level, 0.0);
            }
            morph.time_ms = morph.time_ms.saturating_add(elapsed_ms);
            if morph.time_ms > morph.duration_ms {
                faces.set_face_weight(morph.face, 0, u32::from(morph.level[0][1]), 1.0);
                faces.set_face_weight(morph.face, 1, u32::from(morph.level[1][1]), 1.0);
                drop_at.push(position);
            } else if morph.duration_ms != 0 {
                let f = (morph.time_ms as f32) / (morph.duration_ms as f32);
                faces.set_face_weight(morph.face, 0, u32::from(morph.level[0][0]), 1.0 - f);
                faces.set_face_weight(morph.face, 1, u32::from(morph.level[1][0]), 1.0 - f);
                faces.set_face_weight(morph.face, 0, u32::from(morph.level[0][1]), f);
                faces.set_face_weight(morph.face, 1, u32::from(morph.level[1][1]), f);
            }
        }
        for position in drop_at.into_iter().rev() {
            morphs.remove(position);
        }
    }
}

/// `DabingReset` face restore: regions 0..2 get level 0 = 1, rest 0
/// (mafia-re `game/ui/g_dabing.cpp:68-83`).
fn dabing_reset_face_weights(faces: &mut impl DabingFaceSink, face: DabingFaceId) {
    for region in 0..3 {
        faces.set_face_weight(face, region, 0, 1.0);
        for level in 1..5 {
            faces.set_face_weight(face, region, level, 0.0);
        }
    }
}

/// Shared speaking/fading weight application
/// (mafia-re `game/ui/g_dabing.cpp:207-219`).
fn apply_face_weights(faces: &mut impl DabingFaceSink, face: DabingFaceId, w: &[f32; 6]) {
    let (w0, w1, w2, w3, w4, w5) = (w[0], w[1], w[2], w[3], w[4], w[5]);
    faces.set_face_weight(face, 0, 0, 0.0);
    faces.set_face_weight(face, 0, 1, w1 * w0);
    faces.set_face_weight(face, 0, 2, (1.0 - w0) * w1);
    faces.set_face_weight(face, 0, 3, (1.0 - w2) * (1.0 - w1));
    faces.set_face_weight(face, 0, 4, (1.0 - w1) * w2);
    faces.set_face_weight(face, 1, 0, 0.0);
    faces.set_face_weight(face, 1, 1, w4 * w3);
    faces.set_face_weight(face, 1, 2, (1.0 - w4) * w3);
    faces.set_face_weight(face, 1, 3, 1.0 - w3);
    faces.set_face_weight(face, 2, 0, 0.0);
    faces.set_face_weight(face, 2, 1, w5 * w3);
    faces.set_face_weight(face, 2, 2, (1.0 - w5) * w3);
    faces.set_face_weight(face, 2, 3, 1.0 - w3);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FaceLog {
        writes: Vec<(DabingFaceId, u32, u32, u32)>,
    }

    impl DabingFaceSink for FaceLog {
        fn set_face_weight(&mut self, face: DabingFaceId, region: u32, level: u32, weight: f32) {
            self.writes.push((face, region, level, weight.to_bits()));
        }
    }

    fn weight_of(log: &FaceLog, region: u32, level: u32) -> Option<f32> {
        log.writes
            .iter()
            .rev()
            .find(|(_, r, l, _)| *r == region && *l == level)
            .map(|(_, _, _, bits)| f32::from_bits(*bits))
    }

    struct SilentAudio;

    impl DabingAudioProbe for SilentAudio {
        fn dabing_logical_playing(&self, _sound: &SoundId) -> bool {
            false
        }
        fn dabing_position_ms(&self, _sound: &SoundId) -> u32 {
            0
        }
        fn dabing_duration_ms(&self, _sound: &SoundId) -> u32 {
            0
        }
        fn dabing_release(&self, _sound: &SoundId) {}
    }

    /// Two frames (header count 3) with distinct bytes; frame 0 has
    /// w0 = w1 = 255 so region-0 level 1 reaches 1.0 after full blend.
    fn two_frame_data() -> Vec<u32> {
        let header = 3u32;
        let frame0 = [255u8, 255, 0, 0, 0, 0, 0, 0];
        let frame1 = [0u8, 0, 0, 0, 255, 0, 0, 0];
        let pack = |bytes: [u8; 8]| {
            [
                u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            ]
        };
        let [a0, a1] = pack(frame0);
        let [b0, b1] = pack(frame1);
        vec![header, a0, a1, b0, b1]
    }

    #[test]
    fn init_clears_channels_and_morphs() {
        let mut owner = DabingOwner::new();
        owner.channels[3].state = DABING_STATE_SPEAKING;
        owner.channels[3].time_ms = 120;
        owner.push_morph(DabingMorph {
            face: DabingFaceId(1),
            duration_ms: 100,
            time_ms: 10,
            level: [[0, 1], [0, 1]],
        });
        owner.dabing_init();
        assert!(owner.channels.iter().all(|c| c.state == DABING_STATE_FREE));
        assert_eq!(owner.morph_count(), 0);
    }

    #[test]
    fn get_free_chanel_prefers_last_free_and_last_when_full() {
        let owner = DabingOwner::new();
        assert_eq!(owner.dabing_get_free_chanel(), NUM_DABING_CHANELS - 1);
        let mut full = DabingOwner::new();
        for channel in &mut full.channels {
            channel.state = DABING_STATE_SPEAKING;
        }
        assert_eq!(full.dabing_get_free_chanel(), NUM_DABING_CHANELS - 1);
        full.channels[NUM_DABING_CHANELS - 1].state = DABING_STATE_FREE;
        full.channels[0].state = DABING_STATE_FREE;
        assert_eq!(full.dabing_get_free_chanel(), NUM_DABING_CHANELS - 1);
    }

    #[test]
    fn speak_marker_validates_kind_and_records_deferred_paths() {
        let mut owner = DabingOwner::new();
        assert_eq!(
            owner.speak_marker(0, 7, false, 0),
            Err(DabingError::BadKind { kind: 0 })
        );
        assert_eq!(
            owner.speak_marker(0, 7, false, 7),
            Err(DabingError::BadKind { kind: 7 })
        );
        assert_eq!(
            owner.speak_marker(99, 7, false, 1),
            Err(DabingError::BadChannel { channel: 99 })
        );
        let intake = owner.speak_marker(2, 7, true, 3).unwrap();
        assert_eq!(intake.dat_path, "tables\\dat\\7.dat");
        assert_eq!(intake.wav_path, "sounds\\7.wav");
        assert!(!intake.expects_scene_sound);
        assert_eq!(owner.channel(2).unwrap().state, DABING_STATE_SPEAKING);
        assert!(owner.dabing_is_speaking(2).unwrap());

        let kind5 = owner.speak_marker(4, 9, false, 5).unwrap();
        assert!(kind5.expects_scene_sound);
        assert_eq!(owner.channel(4).unwrap().state, DABING_STATE_SCENE_SOUND);
    }

    #[test]
    fn set_human_marker_records_opaque_speaker_only() {
        let mut owner = DabingOwner::new();
        owner.speak_marker(1, 3, false, 2).unwrap();
        owner.set_human_marker(1, DabingSpeakerId(0xbeef)).unwrap();
        assert_eq!(
            owner.channel(1).unwrap().speaker,
            Some(DabingSpeakerId(0xbeef))
        );
        assert_eq!(
            owner.set_human_marker(40, DabingSpeakerId(1)),
            Err(DabingError::BadChannel { channel: 40 })
        );
    }

    #[test]
    fn subtitle_only_counts_down_fade_then_frees() {
        let mut owner = DabingOwner::new();
        let mut faces = FaceLog::default();
        owner.speak_marker(0, 11, true, 2).unwrap();
        owner.begin_subtitle_only(0, 10).unwrap();
        assert_eq!(
            owner.channel(0).unwrap().fade_ms,
            1000 + 10 * 30,
            "30 ms per character plus a second"
        );
        owner.dabing_tick(&SilentAudio, &mut faces, 100);
        assert!(owner.dabing_is_speaking(0).unwrap());
        owner.dabing_tick(&SilentAudio, &mut faces, 1200);
        assert!(!owner.dabing_is_speaking(0).unwrap());
        assert_eq!(owner.channel(0).unwrap().state, DABING_STATE_FREE);
    }

    /// Uniform block: `frames` identical frames; header count includes itself.
    fn uniform_data(frames: u32, bytes: [u8; 8]) -> Vec<u32> {
        let mut words = vec![frames + 1];
        for _ in 0..frames {
            words.push(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
            words.push(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]));
        }
        words
    }

    #[test]
    fn speaking_blends_from_last_weights_over_125_ms() {
        let mut owner = DabingOwner::new();
        let mut faces = FaceLog::default();
        owner.speak_marker(0, 5, false, 1).unwrap();
        owner.attach_face(0, DabingFaceId(9)).unwrap();
        // w1 = w0 = 255 in every frame, so region-0 level 1 is blend^2.
        owner
            .attach_data(0, uniform_data(11, [255, 255, 0, 0, 0, 0, 0, 0]))
            .unwrap();
        for _ in 0..5 {
            owner.dabing_tick(&SilentAudio, &mut faces, 10);
        }
        // t = 50 ms, blend = 50 * 0.008 = 0.4, still on frame 1 < 11.
        assert_eq!(owner.channel(0).unwrap().state, DABING_STATE_SPEAKING);
        let partial = weight_of(&faces, 0, 1).unwrap();
        assert!(
            (partial - 0.16).abs() < 1e-5,
            "blend 0.4 must give level weight 0.16, got {partial}"
        );
        for _ in 0..8 {
            owner.dabing_tick(&SilentAudio, &mut faces, 10);
        }
        // t = 130 ms: blend saturated, frame 3 still within data.
        let full = weight_of(&faces, 0, 1).unwrap();
        assert!(
            (full - 1.0).abs() < 1e-5,
            "saturated blend must give level weight 1.0, got {full}"
        );
        assert_eq!(owner.channel(0).unwrap().state, DABING_STATE_SPEAKING);
    }

    #[test]
    fn soundless_data_speech_fades_past_data_end_and_frees() {
        let mut owner = DabingOwner::new();
        let mut faces = FaceLog::default();
        owner.speak_marker(0, 5, false, 1).unwrap();
        owner.attach_face(0, DabingFaceId(9)).unwrap();
        owner.attach_data(0, two_frame_data()).unwrap();
        // Header count 3: frames 0..1 valid, fading once frame >= 2.
        owner.dabing_tick(&SilentAudio, &mut faces, DABING_FRAME_TIME_MS);
        assert_eq!(owner.channel(0).unwrap().state, DABING_STATE_SPEAKING);
        owner.dabing_tick(&SilentAudio, &mut faces, DABING_FRAME_TIME_MS);
        assert_eq!(owner.channel(0).unwrap().state, DABING_STATE_FADING);
        owner.dabing_tick(&SilentAudio, &mut faces, DABING_FADE_TIME_MS);
        assert_eq!(owner.channel(0).unwrap().state, DABING_STATE_FREE);
        // Fading stores the scaled last frame as the next blend source:
        // last frame has fr[4] = 255, i.e. w3 = 1.
        assert!((owner.channel(0).unwrap().weight[3] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn dataless_soundless_speech_fades_immediately() {
        let mut owner = DabingOwner::new();
        let mut faces = FaceLog::default();
        owner.speak_marker(0, 5, false, 1).unwrap();
        owner.attach_face(0, DabingFaceId(9)).unwrap();
        owner.dabing_tick(&SilentAudio, &mut faces, 10);
        assert_eq!(owner.channel(0).unwrap().state, DABING_STATE_FADING);
    }

    #[test]
    fn reset_restores_faces_and_detaches_while_done_skips_faces() {
        let mut owner = DabingOwner::new();
        let mut faces = FaceLog::default();
        owner.speak_marker(0, 5, false, 1).unwrap();
        owner.attach_face(0, DabingFaceId(3)).unwrap();
        owner.attach_data(0, two_frame_data()).unwrap();
        owner.push_morph(DabingMorph {
            face: DabingFaceId(3),
            duration_ms: 50,
            time_ms: 0,
            level: [[0, 1], [2, 3]],
        });
        owner.recorder_close_reset(&mut faces, &SilentAudio);
        assert_eq!(owner.morph_count(), 0);
        assert!(!owner.dabing_is_speaking(0).unwrap());
        assert_eq!(weight_of(&faces, 0, 0), Some(1.0));
        assert_eq!(weight_of(&faces, 1, 0), Some(1.0));
        assert_eq!(weight_of(&faces, 2, 0), Some(1.0));
        assert_eq!(weight_of(&faces, 0, 4), Some(0.0));

        owner.speak_marker(1, 6, false, 2).unwrap();
        owner.dabing_done(&SilentAudio);
        assert!(owner.channels.iter().all(|c| c.state == DABING_STATE_FREE));
    }

    #[test]
    fn morph_moves_levels_over_duration_and_drops_at_end() {
        let mut owner = DabingOwner::new();
        let mut faces = FaceLog::default();
        owner.push_morph(DabingMorph {
            face: DabingFaceId(7),
            duration_ms: 100,
            time_ms: 0,
            level: [[0, 1], [2, 3]],
        });
        owner.dabing_tick(&SilentAudio, &mut faces, 50);
        assert_eq!(owner.morph_count(), 1);
        let half = weight_of(&faces, 0, 1).unwrap();
        assert!(
            (half - 0.5).abs() < 1e-5,
            "mid-duration morph must lerp to 0.5, got {half}"
        );
        owner.dabing_tick(&SilentAudio, &mut faces, 51);
        assert_eq!(owner.morph_count(), 0);
        assert_eq!(weight_of(&faces, 0, 1), Some(1.0));
        assert_eq!(weight_of(&faces, 1, 3), Some(1.0));
    }
}
