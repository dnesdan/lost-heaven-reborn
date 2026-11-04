//! Bounded raw police-manager initialization and reset, with working names.
//!
//! Existing bytes and the constructor's opaque allocator marker are explicit
//! inputs. Native pointer words remain opaque words, never Rust pointers. This
//! module does not construct resources, own actors, or implement manager ON.
//!
//! PR3 parity (mafia-re `pr3` b0c02ef): `S_shvestky::RemoveAngryActor`
//! 0x49f780, `C_shvestky::RemoveAngryActor` 0x49f540 and
//! `ReleaseWantedHuman` 0x49f850. `SendPoliceBackup` 0x49eee0,
//! `SendPoliceToHuman` 0x49f980 and `FreeMyTempActor` 0x49e7e0 remain
//! unimplemented stubs; the previous vulgar working names are retired in
//! favor of the English descriptive names above.

/// Raw words of one retained native allocation; these are not host addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawManagerPool {
    pub begin_word: u32,
    pub end_word: u32,
    pub allocation_end_word: u32,
}

/// The recovered raw state extent. Unknown bytes retain their supplied values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoliceManagerRawState {
    bytes: [u8; 0xac],
}

impl PoliceManagerRawState {
    /// Supply existing state without assuming zeroed construction or a zero mode byte.
    pub const fn from_bytes(bytes: [u8; 0xac]) -> Self {
        Self { bytes }
    }
    pub const fn as_bytes(&self) -> &[u8; 0xac] {
        &self.bytes
    }
    pub const fn raw_mode(&self) -> u8 {
        self.bytes[0]
    }
    /// The native guard tests truth without normalizing an existing mode byte.
    pub const fn mode_is_nonzero(&self) -> bool {
        self.raw_mode() != 0
    }
    /// Raw first script argument; no actor-slot interpretation is imposed.
    pub fn script_argument_0_bits(&self) -> u32 {
        self.word(0x34)
    }
    /// Stored native byte; script dispatch normalizes its argument separately.
    pub const fn script_argument_1_raw(&self) -> u8 {
        self.bytes[0x39]
    }
    /// Seven retained allocations in original order. No pointer is dereferenced.
    pub fn pool(&self, index: usize) -> Option<RawManagerPool> {
        if index >= 7 {
            return None;
        }
        let offset = 0x40 + index * 0x10;
        Some(RawManagerPool {
            begin_word: self.word(offset),
            end_word: self.word(offset + 4),
            allocation_end_word: self.word(offset + 8),
        })
    }

    /// Apply only the evidenced constructor stores. The original copies the
    /// high byte of its object address into seven opaque allocator bytes; this
    /// portable projection requires that marker explicitly. Mode and script
    /// arguments are preserved, so construction does not imply a zero mode byte.
    pub fn apply_constructor_fields(&mut self, allocator_marker: u8) {
        for offset in [0x04, 0x14, 0x1c, 0x24] {
            self.set_word(offset, 0);
        }
        for offset in [0x08, 0x18, 0x20, 0x28] {
            self.set_word(offset, 200);
        }
        for index in 0..7 {
            let marker = 0x3c + index * 0x10;
            self.bytes[marker] = allocator_marker;
            for offset in [marker + 4, marker + 8, marker + 12] {
                self.set_word(offset, 0);
            }
        }
    }

    /// Reset the observed scalar words and truncate four logical pools. Every
    /// allocation word and all pointee storage remain owned by the caller.
    /// Other pool ends, allocator markers, script arguments and unknown bytes
    /// are preserved. No element destruction or resource creation occurs.
    pub fn reset_retaining_pools(&mut self) {
        self.bytes[0] = 0;
        for (offset, value) in [
            (0x04, 0),
            (0x08, 0x14a),
            (0x0c, 0x4188_0000),
            (0x10, 0x41e0_0000),
            (0x14, 0),
            (0x18, 0x391),
            (0x1c, 0),
            (0x20, 0xd9),
            (0x24, 0),
            (0x28, 0xfad),
            (0x2c, 0),
            (0x30, 0),
        ] {
            self.set_word(offset, value);
        }
        for begin in [0x40, 0x50, 0x60, 0x80] {
            self.set_word(begin + 4, self.word(begin));
        }
    }
    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }
    fn set_word(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}

/// Opaque actor identity for police-record lists. Native addresses are never
/// dereferenced; equality selects list membership only.
pub type PoliceActorId = u32;

/// Period timer shared by the police manager, ported from `S_timer`
/// (mafia-re game/tools.h: `{time, period}` u32 pair, default 200 ms).
/// `Reset` restarts with a new period; `Tick` accumulates game time and
/// fires past the period, subtracting it — all exact u32 arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoliceTimer {
    time_ms: u32,
    period_ms: u32,
}

impl PoliceTimer {
    pub const fn new() -> Self {
        Self {
            time_ms: 0,
            period_ms: 200,
        }
    }

    pub const fn period_ms(self) -> u32 {
        self.period_ms
    }

    pub const fn time_ms(self) -> u32 {
        self.time_ms
    }

    /// Original `Reset`: time back to zero with the new period.
    pub fn reset(&mut self, period_ms: u32) {
        self.time_ms = 0;
        self.period_ms = period_ms;
    }

    /// Original `Tick`: adds game time; once past the period, subtracts
    /// the period and reports the firing.
    pub fn tick(&mut self, elapsed_ms: u32) -> bool {
        self.time_ms = self.time_ms.wrapping_add(elapsed_ms);
        if self.time_ms <= self.period_ms {
            return false;
        }
        self.time_ms = self.time_ms.wrapping_sub(self.period_ms);
        true
    }
}

impl Default for PoliceTimer {
    fn default() -> Self {
        Self::new()
    }
}

/// Road-block car entry (`S_road_block_car`, 8 bytes: actor + free flag).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoliceRoadBlockCar {
    pub car: PoliceActorId,
    pub free: bool,
}

/// Road-block policeman entry (`S_road_block_iris`, same shape).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoliceRoadBlockIris {
    pub actor: PoliceActorId,
    pub free: bool,
}

/// Owned subset of `C_shvestky` state written by `GameInit` 0x49cd0.
/// Fields the native body does not touch (enemies, temporary cars, arrest
/// text, `unknown_38`) are outside this owner by construction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PoliceGameInitState {
    active: bool,
    records_timer: PoliceTimer,
    speed_limit: u32,
    speed_limit_arrest: u32,
    do_rici_timer: PoliceTimer,
    red_light_timer: PoliceTimer,
    unknown_24: PoliceTimer,
    unknown_2c: u32,
    unknown_30: u32,
    records: Vec<PolicePursuitRecord>,
    road_block_cars: Vec<PoliceRoadBlockCar>,
    road_block_irises: Vec<PoliceRoadBlockIris>,
    do_rici_actors: Vec<PoliceActorId>,
}

impl PoliceGameInitState {
    pub fn active(&self) -> bool {
        self.active
    }

    pub fn records_timer(&self) -> PoliceTimer {
        self.records_timer
    }

    pub fn speed_limit_bits(&self) -> u32 {
        self.speed_limit
    }

    pub fn speed_limit_arrest_bits(&self) -> u32 {
        self.speed_limit_arrest
    }

    pub fn speed_limit(&self) -> f32 {
        f32::from_bits(self.speed_limit)
    }

    pub fn speed_limit_arrest(&self) -> f32 {
        f32::from_bits(self.speed_limit_arrest)
    }

    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    pub fn road_block_car_count(&self) -> usize {
        self.road_block_cars.len()
    }

    pub fn road_block_iris_count(&self) -> usize {
        self.road_block_irises.len()
    }

    pub fn do_rici_actor_count(&self) -> usize {
        self.do_rici_actors.len()
    }

    /// Original `C_shvestky::GameInit` 0x499cd0 in native order: inactive,
    /// timer resets (330/913/217/4013 ms), 17.0/28.0 speed limits, zeroed
    /// words, and cleared lists. Untouched fields stay out of this owner.
    pub fn apply_game_init(&mut self) {
        self.active = false;
        self.records_timer.reset(330);
        self.speed_limit = 17.0f32.to_bits();
        self.speed_limit_arrest = 28.0f32.to_bits();
        self.records.clear();
        self.red_light_timer.reset(217);
        self.unknown_24.reset(4013);
        self.unknown_2c = 0;
        self.unknown_30 = 0;
        self.road_block_cars.clear();
        self.road_block_irises.clear();
        self.do_rici_timer.reset(913);
        self.do_rici_actors.clear();
    }
}
/// Opaque human identity for wanted-record lookup.
pub type PoliceHumanId = u32;

/// Cleared pursuit state written by `ReleaseWantedHuman` (`SHVSTATE_NONE`).
pub const SHVSTATE_NONE: u32 = 0;

/// Still-unimplemented police dispatch. Entries name the original PC
/// addresses; the vulgar working names are retired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoliceDispatchError {
    Unsupported { entry: u32 },
}

/// `C_shvestky::SendPoliceBackup` 0x49eee0 (`POLICE_SUPPORT`).
pub const SEND_POLICE_BACKUP_ENTRY: u32 = 0x49eee0;
/// `C_shvestky::SendPoliceToHuman` 0x49f980.
pub const SEND_POLICE_TO_HUMAN_ENTRY: u32 = 0x49f980;
/// `C_shvestky::FreeMyTempActor` 0x49e7e0.
pub const FREE_TEMP_ACTOR_ENTRY: u32 = 0x49e7e0;

pub fn send_police_backup(
    _human: PoliceHumanId,
    _unknown: u32,
    _range: f32,
) -> Result<(), PoliceDispatchError> {
    Err(PoliceDispatchError::Unsupported {
        entry: SEND_POLICE_BACKUP_ENTRY,
    })
}

pub fn send_police_to_human(
    _human: PoliceHumanId,
    _pos: [f32; 3],
    _range: f32,
    _unknown: bool,
    _unknown2: bool,
) -> Result<(), PoliceDispatchError> {
    Err(PoliceDispatchError::Unsupported {
        entry: SEND_POLICE_TO_HUMAN_ENTRY,
    })
}

/// Original `S_shvestky::RemoveAngryActor` 0x49f780: removes the actor from
/// one record's angry list with swap-with-back + pop. Returns whether the
/// actor was present.
pub fn remove_angry_actor_from_record(list: &mut Vec<PoliceActorId>, actor: PoliceActorId) -> bool {
    for i in (0..list.len()).rev() {
        if list[i] == actor {
            list[i] = *list.last().expect("indexed angry actor");
            list.pop();
            return true;
        }
    }
    false
}

/// Original `C_shvestky::RemoveAngryActor` 0x49f540: removes the actor from
/// every record's angry list. Iterates records from the back, matching the
/// native `for (i = records.size(); i--;)` order.
pub fn remove_angry_actor_everywhere(records: &mut [Vec<PoliceActorId>], actor: PoliceActorId) {
    for record in records.iter_mut().rev() {
        remove_angry_actor_from_record(record, actor);
    }
}

/// Typed pursuit record cleared by `ReleaseWantedHuman` 0x49f850. Field
/// order follows `C_shvestky.h`; unknown native words stay typed scalars
/// without invented gameplay meanings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicePursuitRecord {
    pub human: PoliceHumanId,
    pub flags: u32,
    pub flags2: u32,
    pub car_steal_time: u32,
    pub fire_time: u32,
    pub illegal_path_time: u32,
    pub collision_time: u32,
    pub crash_time: u32,
    pub boxing_time: u32,
    pub signal_time: u32,
    pub block_way: bool,
    pub offenses: u32,
    pub state: u32,
    pub new_state: u32,
    pub wanted_time: i32,
    pub hits_time: i32,
    pub hits: u32,
    pub run_over: u32,
    pub alerted: bool,
    pub nevi: bool,
    pub support_level: u32,
    pub rammed_car: u32,
}

impl PolicePursuitRecord {
    pub fn new(human: PoliceHumanId) -> Self {
        Self {
            human,
            flags: 0,
            flags2: 0,
            car_steal_time: 0,
            fire_time: 0,
            illegal_path_time: 0,
            collision_time: 0,
            crash_time: 0,
            boxing_time: 0,
            signal_time: 0,
            block_way: false,
            offenses: 0,
            state: SHVSTATE_NONE,
            new_state: SHVSTATE_NONE,
            wanted_time: -1,
            hits_time: -1,
            hits: 0,
            run_over: 0,
            alerted: false,
            nevi: false,
            support_level: 0,
            rammed_car: 0,
        }
    }
}

/// Original `C_shvestky::ReleaseWantedHuman` 0x49f850: clears the human's
/// record and returns whether one was found. With `add_offense`, a human
/// that is not the player (or while `unknown_38` is set) keeps one offense.
/// Iterates from the back, matching the native loop.
pub fn release_wanted_human(
    records: &mut [PolicePursuitRecord],
    human: PoliceHumanId,
    player: PoliceHumanId,
    unknown_38: bool,
    add_offense: bool,
) -> bool {
    for i in (0..records.len()).rev() {
        if records[i].human == human {
            let record = &mut records[i];
            record.flags = 0;
            record.flags2 = 0;
            record.car_steal_time = 0;
            record.fire_time = 0;
            record.illegal_path_time = 0;
            record.collision_time = 0;
            record.crash_time = 0;
            record.boxing_time = 0;
            record.signal_time = 0;
            record.block_way = false;
            if add_offense && (record.human != player || unknown_38) {
                record.offenses = record.offenses.wrapping_add(1);
            }
            record.state = SHVSTATE_NONE;
            record.new_state = SHVSTATE_NONE;
            record.wanted_time = -1;
            record.hits_time = -1;
            record.hits = 0;
            record.run_over = 0;
            record.alerted = false;
            record.nevi = false;
            record.support_level = 0;
            record.rammed_car = 0;
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_preserves_mode_arguments_and_unwritten_bytes() {
        let before = std::array::from_fn(|i| (i as u8).wrapping_mul(13).wrapping_add(71));
        let mut state = PoliceManagerRawState::from_bytes(before);
        state.apply_constructor_fields(0x42);
        assert_eq!(state.raw_mode(), before[0]);
        assert!(state.mode_is_nonzero());
        assert_eq!(
            state.script_argument_0_bits(),
            u32::from_le_bytes(before[0x34..0x38].try_into().unwrap())
        );
        assert_eq!(state.script_argument_1_raw(), before[0x39]);
        for i in 0..7 {
            assert_eq!(
                state.pool(i),
                Some(RawManagerPool {
                    begin_word: 0,
                    end_word: 0,
                    allocation_end_word: 0
                })
            );
            assert_eq!(state.as_bytes()[0x3c + i * 16], 0x42);
        }
        assert_eq!(state.pool(7), None);
        for offset in [
            1, 2, 3, 0x0c, 0x10, 0x2c, 0x30, 0x38, 0x3a, 0x3b, 0x3d, 0x4d, 0x9f,
        ] {
            assert_eq!(state.as_bytes()[offset], before[offset]);
        }
        for offset in [0x08, 0x18, 0x20, 0x28] {
            assert_eq!(state.word(offset), 200);
        }
    }

    #[test]
    fn reset_retains_all_storage_and_truncates_only_four_pools() {
        let mut bytes = [0xa5; 0xac];
        for index in 0..7 {
            let offset = 0x40 + index * 16;
            for (at, word) in [
                (offset, 0x1000 + index as u32 * 0x100),
                (offset + 4, 0x1080 + index as u32 * 0x100),
                (offset + 8, 0x1100 + index as u32 * 0x100),
            ] {
                bytes[at..at + 4].copy_from_slice(&word.to_le_bytes());
            }
        }
        let mut state = PoliceManagerRawState::from_bytes(bytes);
        let before = state.clone();
        state.reset_retaining_pools();
        assert!(!state.mode_is_nonzero());
        for i in 0..7 {
            let pool = state.pool(i).unwrap();
            let old = before.pool(i).unwrap();
            assert_eq!(pool.begin_word, old.begin_word);
            assert_eq!(pool.allocation_end_word, old.allocation_end_word);
            assert_eq!(
                pool.end_word,
                if [0, 1, 2, 4].contains(&i) {
                    old.begin_word
                } else {
                    old.end_word
                }
            );
            assert_eq!(state.as_bytes()[0x3c + i * 16], 0xa5);
        }
        assert_eq!(
            state.script_argument_0_bits(),
            before.script_argument_0_bits()
        );
        assert_eq!(
            state.script_argument_1_raw(),
            before.script_argument_1_raw()
        );
        assert_eq!(state.word(0x0c), 17f32.to_bits());
        assert_eq!(state.word(0x10), 28f32.to_bits());
        let once = state.clone();
        state.reset_retaining_pools();
        assert_eq!(state, once);
    }

    #[test]
    fn angry_list_removal_uses_swap_with_back() {
        let mut list = vec![10u32, 20, 30, 40];
        assert!(remove_angry_actor_from_record(&mut list, 20));
        assert_eq!(list, vec![10, 40, 30]);
        assert!(!remove_angry_actor_from_record(&mut list, 99));
        assert_eq!(list, vec![10, 40, 30]);
        assert!(remove_angry_actor_from_record(&mut list, 10));
        assert_eq!(list, vec![30, 40]);
    }

    #[test]
    fn angry_removal_everywhere_clears_all_records() {
        let mut records = vec![vec![1u32, 2], vec![2u32, 3], vec![4u32]];
        remove_angry_actor_everywhere(&mut records, 2);
        assert_eq!(records, vec![vec![1], vec![3], vec![4]]);
        remove_angry_actor_everywhere(&mut records, 9);
        assert_eq!(records, vec![vec![1], vec![3], vec![4]]);
    }

    fn pursuit_record(human: u32, offenses: u32) -> PolicePursuitRecord {
        let mut record = PolicePursuitRecord::new(human);
        record.flags = 5;
        record.flags2 = 6;
        record.fire_time = 100;
        record.state = 3;
        record.new_state = 4;
        record.wanted_time = 20000;
        record.hits_time = 10;
        record.hits = 2;
        record.alerted = true;
        record.nevi = true;
        record.support_level = 2;
        record.rammed_car = 0x1234;
        record.offenses = offenses;
        record
    }

    #[test]
    fn release_clears_record_and_counts_offense_for_non_player() {
        let mut records = vec![pursuit_record(7, 1), pursuit_record(9, 0)];
        assert!(release_wanted_human(&mut records, 9, 7, false, true));
        let cleared = &records[1];
        assert_eq!(cleared.flags, 0);
        assert_eq!(cleared.state, SHVSTATE_NONE);
        assert_eq!(cleared.wanted_time, -1);
        assert_eq!(cleared.hits_time, -1);
        assert!(!cleared.alerted);
        assert_eq!(cleared.offenses, 1);
        assert_eq!(records[0].offenses, 1);
    }

    #[test]
    fn release_skips_offense_for_player_without_unknown_38() {
        let mut records = vec![pursuit_record(7, 1)];
        assert!(release_wanted_human(&mut records, 7, 7, false, true));
        assert_eq!(records[0].offenses, 1);
        let mut records = vec![pursuit_record(7, 1)];
        assert!(release_wanted_human(&mut records, 7, 7, true, true));
        assert_eq!(records[0].offenses, 2);
        let mut records = vec![pursuit_record(7, 1)];
        assert!(release_wanted_human(&mut records, 7, 7, false, false));
        assert_eq!(records[0].offenses, 1);
        assert!(!release_wanted_human(&mut records, 99, 7, false, true));
    }

    #[test]
    fn dispatch_stubs_stay_explicit_unsupported() {
        assert_eq!(
            send_police_backup(1, 4, 200.0),
            Err(PoliceDispatchError::Unsupported {
                entry: SEND_POLICE_BACKUP_ENTRY
            })
        );
        assert_eq!(
            send_police_to_human(1, [0.0; 3], 150.0, false, false),
            Err(PoliceDispatchError::Unsupported {
                entry: SEND_POLICE_TO_HUMAN_ENTRY
            })
        );
    }

    #[test]
    fn timer_resets_and_ticks_with_exact_u32_arithmetic() {
        let mut timer = PoliceTimer::new();
        assert_eq!((timer.time_ms(), timer.period_ms()), (0, 200));
        assert!(!timer.tick(200));
        assert!(timer.tick(1));
        assert_eq!(timer.time_ms(), 1);
        timer.reset(330);
        assert_eq!((timer.time_ms(), timer.period_ms()), (0, 330));
        assert!(!timer.tick(330));
        assert!(timer.tick(1));
        // Wrapping arithmetic never panics at the u32 boundary.
        timer.reset(u32::MAX);
        assert!(!timer.tick(u32::MAX));
    }

    #[test]
    fn game_init_stores_exact_scalars_and_clears_lists() {
        let mut state = PoliceGameInitState::default();
        state.records.push(PolicePursuitRecord::new(3));
        state.road_block_cars.push(PoliceRoadBlockCar {
            car: 11,
            free: true,
        });
        state.road_block_irises.push(PoliceRoadBlockIris {
            actor: 13,
            free: false,
        });
        state.do_rici_actors.push(17);
        state.apply_game_init();
        assert!(!state.active());
        assert_eq!(state.records_timer().period_ms(), 330);
        assert_eq!(state.speed_limit(), 17.0);
        assert_eq!(state.speed_limit_bits(), 17.0f32.to_bits());
        assert_eq!(state.speed_limit_arrest(), 28.0);
        assert_eq!(state.record_count(), 0);
        assert_eq!(state.road_block_car_count(), 0);
        assert_eq!(state.road_block_iris_count(), 0);
        assert_eq!(state.do_rici_actor_count(), 0);
        // Second application is idempotent over cleared lists.
        state.apply_game_init();
        assert_eq!(state.record_count(), 0);
    }
}
