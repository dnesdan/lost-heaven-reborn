//! The mission's `C_effects` (`act_mission->effects`, `C_mission` +0x6c) as
//! one session owner, and `C_effects::LoadStaticParticles` over it.
//!
//! Reference: Game.exe SHA-256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27, mafia-re
//! `game/mission/C_effects.{h,cpp}` and `game/C_chunk.h` at pin fe7e746.
//! `C_mission::Init` (GAME 0x53f230, `Mission.cpp:103-104`) constructs the
//! owner once (`new C_effects`, whose constructor GAME 0x449080 loads
//! `tables\effects.tbl`); `C_game::Init` then runs `LoadStaticParticles`
//! (GAME 0x44ad30, called at 0x5a0d3e) and `Mission_Start` (GAME 0x449180,
//! called at 0x5a0d4b), and `C_game::Done` runs `Mission_Done` (GAME
//! 0x4491d0, `game.cpp:1190`).
//!
//! Known divergence: natively the owner lives from `C_mission::Init` to
//! `C_mission::UnInit` across missions; here each session constructs it with
//! its installation. The loaded table survives disposal (it is not a mission
//! allocation); `Mission_Done` empties the mission's emitter lists and pools.
//!
//! The emitter, particle-type and particle owners behind `CreateSParticle`
//! (GAME 0x44a3a0: the effect's I3D model through `model_cache`, the scene's
//! `AddFrame`, `C_ParticleEmitter`/`C_ParticleType`/`S_particle_subpart`
//! `Init`) are unported: a reached call that passes the effect checks stops
//! with a named boundary, and no emitter is invented.
#![forbid(unsafe_code)]

use crate::effects_table::{Effect, EffectsTable};
use crate::frame_transform::FrameKey;
use crate::mission_effects::{
    effects_create, effects_mission_done, effects_mission_start, EffectsFreePools, EffectsState,
    EffectsTableBackend,
};

/// `C_effects::Load`'s table (STRING GAME 0x632590 `tables\effects.tbl`), as
/// an installation-relative path.
pub const EFFECTS_TABLE_PATH: &str = "tables/effects.tbl";

/// `CT_EFFECTS_BIN` (0x64): the mission file's root chunk (mafia-re
/// `C_effects.cpp:24`, names mafia-re's).
const CT_EFFECTS_BIN: u16 = 0x64;
/// `CT_EFFECTS_BIN_EFFECT` (0x3e8): one static effect at a frame.
const CT_EFFECTS_BIN_EFFECT: u16 = 0x3e8;
/// `char buf[128]` that `sprintf("missions\\%s\\effects.bin")` writes.
const PATH_BUFFER: usize = 128;
/// `ReadString`'s `char buf[1000]`, read at most 999 bytes.
const READ_STRING_LIMIT: usize = 999;

/// `C_effects::Load` (GAME 0x449380) over the installation: `Clear`, then
/// `ROpen("tables\\effects.tbl")`; a missing table is the `ROpen`-failed
/// return with the effects left empty. The chunk tree is parsed by the
/// existing [`EffectsTable`] reader; its malformed-input admission errors
/// are host boundaries (the original reads unchecked).
pub struct InstallationEffectsTable<'a> {
    installation: &'a lhr_assets::Installation,
}

impl<'a> InstallationEffectsTable<'a> {
    pub fn new(installation: &'a lhr_assets::Installation) -> Self {
        Self { installation }
    }
}

impl EffectsTableBackend<Effect> for InstallationEffectsTable<'_> {
    type Error = String;

    fn load_effects_table(&mut self, effects: &mut Vec<Effect>) -> Result<(), String> {
        let mut table = EffectsTable {
            effects: std::mem::take(effects),
        };
        table.clear();
        let result = match self.installation.read_optional(EFFECTS_TABLE_PATH) {
            Ok(None) => Ok(()),
            Ok(Some(bytes)) => table
                .load(&bytes)
                .map_err(|error| format!("C_effects::Load (GAME 0x449380): {error:?}")),
            Err(error) => Err(format!(
                "C_effects::Load (GAME 0x449380) source {EFFECTS_TABLE_PATH}: {error}"
            )),
        };
        *effects = table.effects;
        result
    }
}

/// Which of `LoadStaticParticles`' ends was reached (the `dbgPrintf`
/// messages are the original's only report). A host observation, not native
/// state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaticParticlesOutcome {
    /// `ROpen` failed (STRING GAME 0x6325f4 `Effects.bin - Error: could not
    /// open "%s"`).
    NotOpened,
    /// The root chunk is not `CT_EFFECTS_BIN` (STRING GAME 0x6325cc
    /// `different versions`); the file was closed.
    DifferentVersions,
    /// An inner chunk is not `CT_EFFECTS_BIN_EFFECT` (STRING GAME 0x6325a4
    /// `error in chunks..`) after `effects` `CreateSParticle` calls; the file
    /// was closed without the final `Descend`.
    ChunkError { effects: usize },
    /// The root chunk was read to its end: `effects` `CreateSParticle` calls
    /// returned, then `Descend` and `Close`.
    Read { effects: usize },
}

/// The session's `C_effects`: the loaded table (`effects` +0x10), the
/// emitter lists (`in_use` +0x0, `static_emitters` +0x20), `last_guid`
/// (+0x30) and `started` (+0x34) in [`EffectsState`], with the file-scope
/// free pools (`particle_pool` GAME 0x63d718, `particle_parts` 0x63d708,
/// `particle_subparts` 0x63d6f8, `particle_emitters` 0x63d6e8) beside it.
#[derive(Debug)]
pub struct SessionEffects {
    state: EffectsState<Effect>,
    pools: EffectsFreePools,
    static_particles: Option<StaticParticlesOutcome>,
    table_load_error: Option<String>,
}

impl SessionEffects {
    /// `new C_effects` in `C_mission::Init` (GAME 0x53f230): the constructor
    /// (GAME 0x449080) loads the table and marks `last_guid` -1.
    pub fn construct(installation: &lhr_assets::Installation) -> Result<Self, String> {
        Self::construct_with(&mut InstallationEffectsTable::new(installation))
    }

    /// The constructor over any table source.
    pub fn construct_with<B: EffectsTableBackend<Effect, Error = String>>(
        backend: &mut B,
    ) -> Result<Self, String> {
        Ok(Self {
            state: effects_create(backend)?,
            pools: EffectsFreePools::default(),
            static_particles: None,
            table_load_error: None,
        })
    }

    /// The loaded `tables\effects.tbl` effects, in file order.
    pub fn effects(&self) -> &[Effect] {
        &self.state.effects
    }

    pub fn static_emitter_count(&self) -> usize {
        self.state.static_emitters.len()
    }

    pub fn last_guid(&self) -> i32 {
        self.state.last_guid
    }

    pub fn started(&self) -> bool {
        self.state.started
    }

    /// The last `LoadStaticParticles` end reached, if any.
    pub fn static_particles(&self) -> Option<StaticParticlesOutcome> {
        self.static_particles
    }

    /// Mission-scoped emitter storage `Mission_Done` releases: emitters in
    /// use, static emitters and every free pool entry. The loaded table is
    /// process-lifetime natively and is not counted.
    pub fn live_mission_entries(&self) -> usize {
        self.state.in_use.len()
            + self.state.static_emitters.len()
            + self.state.free_emitters.len()
            + self.pools.particles.len()
            + self.pools.parts.len()
            + self.pools.subparts.len()
    }

    /// `C_effects::Mission_Start` (GAME 0x449180) over the installation.
    pub fn mission_start(
        &mut self,
        installation: &lhr_assets::Installation,
    ) -> Result<bool, String> {
        self.mission_start_with(&mut InstallationEffectsTable::new(installation))
    }

    pub fn mission_start_with<B: EffectsTableBackend<Effect, Error = String>>(
        &mut self,
        backend: &mut B,
    ) -> Result<bool, String> {
        if let Some(error) = &self.table_load_error {
            return Err(error.clone());
        }
        let result = effects_mission_start(&mut self.state, backend);
        if let Err(error) = &result {
            self.table_load_error = Some(error.clone());
        }
        result
    }

    /// `C_effects::Mission_Done` (GAME 0x4491d0): nothing without a started
    /// mission; otherwise the emitter lists and pools are emptied.
    pub fn mission_done(&mut self) -> bool {
        effects_mission_done(&mut self.state, &mut self.pools)
    }

    /// `C_effects::LoadStaticParticles` (GAME 0x44ad30) over the mission's
    /// `missions\<name>\effects.bin` (`None` is the `ROpen`-failed branch)
    /// and the live scene's `FindFrame(name, ENUMF_ALL)`.
    ///
    /// Native order (Ghidra body 0x44ad30..0x44b161): QPF/QPC (unobserved),
    /// `sprintf` of the path from `act_mission+0x68`, `ROpen` (0x411be0);
    /// the inlined root `RAscend` (`dtaRead` of the 6-byte header) and the
    /// `cmp word, 0x64`; then while `Size()`: the inlined `RAscend`, `cmp
    /// word, 0x3e8`, `Read(&m, 0x40)` and `Read(&effect, 4)` (0x404d80), and
    /// when the effect chunk has bytes left the out-of-line `RAscend`
    /// (0x44b4d0), `ReadString` (0x44b6d0, which descends), `Descend`
    /// (0x404ce0) and the scene's `FindFrame` (vtable +0x58, flags 0xffff);
    /// `Descend`, then `CreateSParticle` (0x44a3a0). The end runs `Descend`
    /// and `Close` (0x411d40).
    ///
    /// An effect read with a frame name descends once more than its chunks
    /// need (mafia-re `C_effects.cpp:285-286`): the second `Descend` leaves
    /// the root chunk, and the next `Size()` reads `back()` of an empty
    /// stack, which is undefined; that read is an explicit boundary. Chunk
    /// extents outside their parent or the file, short headers (where the
    /// original keeps the previous type) and unterminated names are host
    /// admission boundaries, not invented behavior.
    pub fn load_static_particles<F>(
        &mut self,
        file: Option<&[u8]>,
        mut find_frame: F,
    ) -> Result<StaticParticlesOutcome, String>
    where
        F: FnMut(&[u8]) -> Result<Option<FrameKey>, String>,
    {
        let Some(bytes) = file else {
            self.static_particles = Some(StaticParticlesOutcome::NotOpened);
            return Ok(StaticParticlesOutcome::NotOpened);
        };
        let mut chunk = NativeReadChunk::new(bytes);
        // `ushort type = 0`: a failed root header read keeps it.
        let root = chunk.r_ascend()?.unwrap_or(0);
        if root != CT_EFFECTS_BIN {
            self.static_particles = Some(StaticParticlesOutcome::DifferentVersions);
            return Ok(StaticParticlesOutcome::DifferentVersions);
        }
        // `S_matrix m; int effect;` are uninitialized locals: a failed
        // `Read` keeps the previous iteration's value or none.
        let mut matrix: Option<[u8; 0x40]> = None;
        let mut effect: Option<i32> = None;
        let mut created = 0usize;
        while chunk.size()? != 0 {
            let kind = chunk
                .r_ascend()?
                .ok_or("LoadStaticParticles: short effect chunk header (the original keeps the previous type)")?;
            if kind != CT_EFFECTS_BIN_EFFECT {
                let outcome = StaticParticlesOutcome::ChunkError { effects: created };
                self.static_particles = Some(outcome);
                return Ok(outcome);
            }
            if let Some(bytes) = chunk.read(0x40) {
                matrix = Some(bytes.try_into().expect("0x40-byte read"));
            }
            if let Some(bytes) = chunk.read(4) {
                effect = Some(i32::from_le_bytes(bytes.try_into().expect("4-byte read")));
            }
            let frame = if chunk.size()? != 0 {
                chunk
                    .r_ascend()?
                    .ok_or("LoadStaticParticles: short frame-name chunk header")?;
                let name = chunk.read_string()?;
                chunk.descend()?;
                if name.is_empty() {
                    return Err("LoadStaticParticles: FindFrame of an empty C_str (its pointer is unmodeled)".into());
                }
                find_frame(&name)?
            } else {
                None
            };
            chunk.descend()?;
            let effect = effect.ok_or(
                "LoadStaticParticles: CreateSParticle reached with the effect index never read (uninitialized local)",
            )?;
            self.create_s_particle(effect, matrix.as_ref(), frame)?;
            created += 1;
        }
        chunk.descend()?;
        let outcome = StaticParticlesOutcome::Read { effects: created };
        self.static_particles = Some(outcome);
        Ok(outcome)
    }

    /// The reached prefix of `C_effects::CreateSParticle` (GAME 0x44a3a0,
    /// mafia-re `C_effects.cpp:197-205`): an effect index outside the table
    /// (unsigned compare), an effect without parts or without a model name
    /// returns with nothing created. Anything else needs the model and
    /// emitter owners and stops. Not credited as a function.
    fn create_s_particle(
        &mut self,
        effect: i32,
        _matrix: Option<&[u8; 0x40]>,
        _frame: Option<FrameKey>,
    ) -> Result<(), String> {
        let Some(table_effect) = self.state.effects.get(effect as u32 as usize) else {
            return Ok(());
        };
        if table_effect.parts.is_empty() || table_effect.model.is_empty() {
            return Ok(());
        }
        Err(format!(
            "C_effects::CreateSParticle (GAME 0x44a3a0) for effect {effect}: the effect model's I3D frame and model_cache Open, the scene AddFrame and the C_ParticleEmitter/C_ParticleType/S_particle_subpart owners are unported"
        ))
    }
}

/// `missions\<name>\effects.bin` from `act_mission->GetMissionName()`
/// (STRING GAME 0x632620, `sprintf` into `char buf[128]`), as an
/// installation-relative path. A name that would overflow the buffer is a
/// boundary (the original writes past it).
pub fn static_particles_path(mission_name: &[u8]) -> Result<String, String> {
    let written = b"missions\\".len() + mission_name.len() + b"\\effects.bin".len() + 1;
    if written > PATH_BUFFER {
        return Err(format!(
            "LoadStaticParticles: sprintf of {written} bytes overflows the 128-byte path buffer"
        ));
    }
    let name = std::str::from_utf8(mission_name)
        .map_err(|_| "LoadStaticParticles: mission name is not a host path".to_string())?;
    Ok(format!("missions/{name}/effects.bin"))
}

/// `C_chunk` opened for reading (mafia-re `game/C_chunk.h`): the file
/// position and the stack of chunk ends `RAscend` pushes and `Descend` pops.
struct NativeReadChunk<'a> {
    bytes: &'a [u8],
    position: usize,
    stack: Vec<usize>,
}

impl<'a> NativeReadChunk<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            position: 0,
            stack: Vec::new(),
        }
    }

    /// `RAscend`: `None` when fewer than six header bytes remain. The pushed
    /// end must stay inside the enclosing chunk and the file.
    fn r_ascend(&mut self) -> Result<Option<u16>, String> {
        let Some(header) = self.bytes.get(self.position..self.position + 6) else {
            // The short `dtaRead` consumes what is left and fails.
            self.position = self.bytes.len();
            return Ok(None);
        };
        let kind = u16::from_le_bytes([header[0], header[1]]);
        let size = i32::from_le_bytes([header[2], header[3], header[4], header[5]]);
        let limit = self.stack.last().copied().unwrap_or(self.bytes.len());
        let end = usize::try_from(size)
            .ok()
            .filter(|&size| size >= 6)
            .and_then(|size| self.position.checked_add(size))
            .filter(|&end| end <= limit)
            .ok_or_else(|| {
                format!("C_chunk::RAscend: chunk {kind:#x} size {size} outside its parent")
            })?;
        self.position += 6;
        self.stack.push(end);
        Ok(Some(kind))
    }

    /// `Size()`: what is left of the current chunk.
    fn size(&self) -> Result<usize, String> {
        let end = self.stack.last().ok_or(
            "C_chunk::Size over an empty chunk stack (stack.back() past the vector): the frame-name over-descend",
        )?;
        Ok(end - self.position)
    }

    /// `Read`: false without moving when the current chunk has fewer bytes.
    fn read(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = *self.stack.last()?;
        if count > end - self.position {
            return None;
        }
        let start = self.position;
        self.position += count;
        Some(&self.bytes[start..self.position])
    }

    /// `ReadString`: at most 999 bytes of the current chunk up to its first
    /// NUL, then `Descend`.
    fn read_string(&mut self) -> Result<Vec<u8>, String> {
        let end = *self
            .stack
            .last()
            .ok_or("C_chunk::ReadString over an empty stack")?;
        let count = READ_STRING_LIMIT.min(end - self.position);
        let bytes = self.read(count).expect("bounded by the current chunk");
        let length = bytes
            .iter()
            .position(|&byte| byte == 0)
            .ok_or("C_chunk::ReadString: unterminated string (uninitialized buffer)")?;
        let name = bytes[..length].to_vec();
        self.descend()?;
        Ok(name)
    }

    /// `Descend`: seek to the current chunk's end and pop it.
    fn descend(&mut self) -> Result<(), String> {
        let end = self
            .stack
            .pop()
            .ok_or("C_chunk::Descend over an empty chunk stack")?;
        self.position = end;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects_table::EffectPart;

    struct Table(Vec<Effect>, usize);
    impl EffectsTableBackend<Effect> for Table {
        type Error = String;
        fn load_effects_table(&mut self, effects: &mut Vec<Effect>) -> Result<(), String> {
            self.1 += 1;
            effects.extend(self.0.iter().cloned());
            Ok(())
        }
    }

    fn chunk(kind: u16, body: &[u8]) -> Vec<u8> {
        let mut out = kind.to_le_bytes().to_vec();
        out.extend(((body.len() + 6) as i32).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    fn effect_chunk(effect: i32, frame: Option<&[u8]>) -> Vec<u8> {
        let mut body = vec![0u8; 0x40];
        body.extend(effect.to_le_bytes());
        if let Some(name) = frame {
            let mut name = name.to_vec();
            name.push(0);
            body.extend(chunk(0x10, &name));
        }
        chunk(CT_EFFECTS_BIN_EFFECT, &body)
    }

    fn owner(effects: Vec<Effect>) -> SessionEffects {
        SessionEffects::construct_with(&mut Table(effects, 0)).unwrap()
    }

    fn modelled() -> Effect {
        Effect {
            model: b"2bloodp.I3D".to_vec(),
            parts: vec![EffectPart::default()],
            ..Effect::default()
        }
    }

    #[test]
    fn constructor_loads_the_table_and_mission_start_keeps_it() {
        let mut table = Table(vec![Effect::default(), modelled()], 0);
        let mut owner = SessionEffects::construct_with(&mut table).unwrap();
        assert_eq!(
            (table.1, owner.effects().len(), owner.last_guid()),
            (1, 2, -1)
        );
        assert!(!owner.started());
        assert!(owner.mission_start_with(&mut table).unwrap());
        // A loaded table is not reloaded; the flag and the number are set.
        assert_eq!((table.1, owner.started(), owner.last_guid()), (1, true, -1));
        assert!(owner.mission_done());
        assert!(!owner.started());
        assert_eq!(owner.effects().len(), 2);
        assert!(!owner.mission_done());
    }

    #[test]
    fn mission_start_loads_an_empty_table() {
        let mut owner = owner(Vec::new());
        let mut table = Table(vec![modelled()], 0);
        assert!(owner.mission_start_with(&mut table).unwrap());
        assert_eq!((table.1, owner.effects().len()), (1, 1));
    }

    #[test]
    fn a_failed_table_load_refuses_retry_even_without_a_prefix() {
        struct Failing;
        impl EffectsTableBackend<Effect> for Failing {
            type Error = String;
            fn load_effects_table(&mut self, _: &mut Vec<Effect>) -> Result<(), String> {
                Err("table".into())
            }
        }
        let mut owner = owner(Vec::new());
        assert_eq!(owner.mission_start_with(&mut Failing), Err("table".into()));
        assert!(!owner.started());
        assert_eq!(owner.last_guid(), -1);
        let mut table = Table(vec![modelled()], 0);
        assert_eq!(owner.mission_start_with(&mut table), Err("table".into()));
        assert_eq!((table.1, owner.effects().len()), (0, 0));
        assert!(!owner.started());
        assert!(SessionEffects::construct_with(&mut Failing).is_err());
    }

    #[test]
    fn a_partial_table_load_retains_its_prefix_and_refuses_retry() {
        struct PartialFailure(usize);
        impl EffectsTableBackend<Effect> for PartialFailure {
            type Error = String;
            fn load_effects_table(&mut self, effects: &mut Vec<Effect>) -> Result<(), String> {
                self.0 += 1;
                effects.push(modelled());
                Err("malformed continuation".into())
            }
        }
        let mut owner = owner(Vec::new());
        let mut failing = PartialFailure(0);
        let expected = Err("malformed continuation".into());
        assert_eq!(owner.mission_start_with(&mut failing), expected);
        assert_eq!(owner.effects(), &[modelled()]);
        assert_eq!((owner.started(), owner.last_guid()), (false, -1));
        assert_eq!(owner.mission_start_with(&mut failing), expected);
        assert_eq!(failing.0, 1);
        assert!(!owner.mission_done());
        let mut replacement = Table(vec![Effect::default()], 0);
        assert_eq!(owner.mission_start_with(&mut replacement), expected);
        assert_eq!(replacement.1, 0);
        assert_eq!(owner.effects(), &[modelled()]);
        assert!(!owner.started());
    }

    #[test]
    fn empty_root_chunk_reads_to_its_end() {
        // MISE01's effects.bin is exactly this six-byte root chunk.
        let mut owner = owner(vec![modelled()]);
        let file = chunk(CT_EFFECTS_BIN, &[]);
        assert_eq!(file, [0x64, 0, 6, 0, 0, 0]);
        let outcome = owner
            .load_static_particles(Some(&file), |_| panic!("no frame"))
            .unwrap();
        assert_eq!(outcome, StaticParticlesOutcome::Read { effects: 0 });
        assert_eq!(owner.static_particles(), Some(outcome));
        assert_eq!(owner.live_mission_entries(), 0);
    }

    #[test]
    fn open_and_version_failures_return_without_effects() {
        let mut owner = owner(Vec::new());
        assert_eq!(
            owner.load_static_particles(None, |_| unreachable!()),
            Ok(StaticParticlesOutcome::NotOpened)
        );
        // A short root header keeps `type = 0`.
        for file in [chunk(0x65, &[]), Vec::new(), vec![0x64, 0, 6]] {
            assert_eq!(
                owner.load_static_particles(Some(&file), |_| unreachable!()),
                Ok(StaticParticlesOutcome::DifferentVersions)
            );
        }
    }

    #[test]
    fn effects_without_a_model_or_outside_the_table_return_from_create() {
        let mut owner = owner(vec![Effect::default(), modelled()]);
        let mut body = effect_chunk(0, None);
        body.extend(effect_chunk(7, None));
        body.extend(effect_chunk(-1, None));
        let file = chunk(CT_EFFECTS_BIN, &body);
        assert_eq!(
            owner.load_static_particles(Some(&file), |_| unreachable!()),
            Ok(StaticParticlesOutcome::Read { effects: 3 })
        );
        // A wrong inner chunk ends after the effects before it.
        let mut body = effect_chunk(0, None);
        body.extend(chunk(0x3e9, &[]));
        let file = chunk(CT_EFFECTS_BIN, &body);
        assert_eq!(
            owner.load_static_particles(Some(&file), |_| unreachable!()),
            Ok(StaticParticlesOutcome::ChunkError { effects: 1 })
        );
    }

    #[test]
    fn a_creatable_effect_stops_at_create_s_particle() {
        let mut owner = owner(vec![Effect::default(), modelled()]);
        let file = chunk(CT_EFFECTS_BIN, &effect_chunk(1, None));
        let error = owner
            .load_static_particles(Some(&file), |_| unreachable!())
            .unwrap_err();
        assert!(
            error.starts_with("C_effects::CreateSParticle (GAME 0x44a3a0)"),
            "{error}"
        );
        assert_eq!(owner.static_emitter_count(), 0);
    }

    #[test]
    fn a_frame_name_is_found_then_the_over_descend_stops_at_size() {
        let mut owner = owner(vec![Effect::default()]);
        let mut body = effect_chunk(0, Some(b"lampa"));
        body.extend(effect_chunk(0, None));
        let file = chunk(CT_EFFECTS_BIN, &body);
        let mut names = Vec::new();
        let error = owner
            .load_static_particles(Some(&file), |name| {
                names.push(name.to_vec());
                Ok(None)
            })
            .unwrap_err();
        // FindFrame and CreateSParticle ran for the first effect; the second
        // is never reached.
        assert_eq!(names, vec![b"lampa".to_vec()]);
        assert!(error.contains("empty chunk stack"), "{error}");
    }

    #[test]
    fn malformed_extents_and_names_are_boundaries() {
        let mut owner = owner(Vec::new());
        let mut oversized = chunk(CT_EFFECTS_BIN, &[]);
        oversized[2] = 7;
        assert!(owner
            .load_static_particles(Some(&oversized), |_| unreachable!())
            .is_err());
        let mut body = vec![0u8; 0x44];
        body.extend(chunk(0x10, b"no nul"));
        let file = chunk(CT_EFFECTS_BIN, &chunk(CT_EFFECTS_BIN_EFFECT, &body));
        assert!(owner
            .load_static_particles(Some(&file), |_| unreachable!())
            .unwrap_err()
            .contains("unterminated"));
        // A short effect chunk never read its index: CreateSParticle's
        // argument is an uninitialized local.
        let file = chunk(CT_EFFECTS_BIN, &chunk(CT_EFFECTS_BIN_EFFECT, &[]));
        assert!(owner
            .load_static_particles(Some(&file), |_| unreachable!())
            .unwrap_err()
            .contains("uninitialized"));
    }

    #[test]
    fn path_follows_the_sprintf_buffer() {
        assert_eq!(
            static_particles_path(b"mise01").unwrap(),
            "missions/mise01/effects.bin"
        );
        assert!(static_particles_path(&[b'a'; 106]).is_ok());
        assert!(static_particles_path(&[b'a'; 107]).is_err());
    }
}
