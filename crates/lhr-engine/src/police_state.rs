//! Bounded police-manager registry: init/reset plus the person-registry
//! transitions, the do_rici tick slice and the pursuit-record flag helpers,
//! with working names.
//!
//! Existing bytes and the constructor's opaque allocator marker are explicit
//! inputs. Native pointer words remain opaque words, never Rust pointers. This
//! module owns the `records` registry, the `SetActive` scalar stores, the
//! road-block/temp-car/do_rici id lists and the record flag helpers; it
//! does not construct resources, run the full Tick/pursuit/arrest AI, or
//! implement the mission/world side of manager ON.
//!
//! PR3 parity (mafia-re `pr3` b0c02ef): `S_shvestky::RemoveAngryActor`
//! 0x49f780, `C_shvestky::RemoveAngryActor` 0x49f540 and
//! `ReleaseWantedHuman` 0x49f850. This slice adds `TickDoRiciActors` 0x49ea80,
//! `AddDoRiciActor` 0x49e820, `DelDoRiciActor` 0x49e9e0, `FreeMyTempActor`
//! 0x49e7e0, the `Tick` 0x49a300 timer/temp-car prefix, and the `Firing`
//! 0x49cb20 / `HumanBoxing` 0x49d030 / `ForceArrest` 0x49d130 record effects.
//! `SendPoliceBackup` 0x49eee0 and `SendPoliceToHuman` 0x49f980 remain
//! unimplemented stubs; the previous vulgar working names are retired in
//! favor of the English descriptive names above.
//!
//! Person registry (Game.exe SHA-256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000; mafia-re game/ai/C_shvestky.{h,cpp} at 113b5b6 are log-only
//! stubs for these three, so the transitions below are evidenced by a single
//! closed Ghidra read-only probe session instead (single-session recollection:
//! no ev IDs were retained, so re-derive the bodies before any differential
//! claim; contract ceiling holds): `C_shvestky::AddPerson` 0x49c7e0 (null/inactive
//! guards, duplicate scan from the back, cleared 0x80-byte record with the
//! human's car latched when it is `ACTOR_CAR`), `C_shvestky::DelPerson`
//! 0x49ca50 (same guards, back-to-front search, swap-with-back + pop) and the
//! `C_shvestky::SetActive` 0x49b860 registry prefix (same-state early return,
//! then the active/arrest-text/unknown stores). The rest of `SetActive`
//! (bridge-node collection, police-model preload, player auto-add) needs
//! mission/level owners and stays deferred, as do `DriveCar` 0x49cbb0,
//! `DalsiChalpiDoPartie` (`POLICE_SUPPORT`), `TestOverRed`,
//! `CreateShvestkyTest` and the rest of the full Tick/pursuit/arrest AI.
//! `DelPerson`'s per-angry-actor `DelDoRiciActor` 0x49e820 fan-out still needs
//! the records' angry-list owner and stays deferred with that handoff; the
//! removed record is returned so future wiring can enumerate it. `ForceArrest`
//! 0x49d130 is covered as a record effect only; its script parameters and any
//! mission fan-out stay deferred.
//!
//! VM dispatch: `POLICEMANAGER_ON` (135) already parses into
//! `StartupCommand::PoliceManagerOn`, but no live `ScriptEffects` backend owns
//! `PoliceGameInitState`, and the ON arm additionally needs the deferred
//! bridge/model/player-add owners; `POLICEMANAGER_ADD` (136) has no startup
//! command yet and needs a live actor-by-frame-name lookup plus a car-type
//! read, both ownerless. Both arms ride the unledgered `C_program::Process`
//! 0x46d3b0, so no overlay sub-opcode entries arise either way. Arms deferred
//! for those reasons; the registry behaviors they call are ready here.

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

/// Road-block enemy entry (`S_road_block_enemy`, 8 bytes: actor + free flag;
/// mafia-re game/ai/C_shvestky.h:130-134).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoliceRoadBlockEnemy {
    pub actor: PoliceActorId,
    pub free: bool,
}

/// Actor kinds distinguished by `C_shvestky::TickDoRiciActors` 0x49ea80
/// (mafia-re game/ai/C_shvestky.cpp:873-927): `ACTOR_ENTITY`, `ACTOR_CAR`,
/// and every other type, which goes at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoRiciActorKind {
    Entity,
    Car,
    Other,
}

/// Host-supplied per-actor facts for `tick_do_rici_actors` (marker-only
/// contract: the engine never dereferences actors; the host, which owns them,
/// snapshots these through its own accessors, as the original reads them
/// through the actor classes' accessors per mafia-re
/// game/ai/C_shvestky.cpp:850-856).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoRiciMark {
    pub kind: DoRiciActorKind,
    /// World position; the engine zeroes `y` and squares the ground-plane
    /// distance to the camera itself (`d.y = 0; d.Square()`).
    pub pos: [f32; 3],
    /// `C_actor::GetDead`.
    pub dead: bool,
    /// Entity `human_type == 4`.
    pub human_type_is_4: bool,
    /// Entity `AISTATEF_DISMISSED`.
    pub dismissed: bool,
    /// The entity's car, `Some` only when it reads as `ACTOR_CAR`
    /// (same convention as `add_person`'s `car`).
    pub car: Option<PoliceActorId>,
    /// The entity's seat in `car`.
    pub seat: u32,
    /// Whether any seat of the car has an owner.
    pub seats_occupied: bool,
    /// Whether a player/entity global actor has the car as its car or
    /// `using_car` (mafia-re game/ai/C_shvestky.cpp:910-919).
    pub referenced: bool,
    /// Whether the car's frame is on (`IsOn`, mafia-re
    /// game/ai/C_shvestky.cpp:922).
    pub frame_on: bool,
    /// Whether the car is a police car (`police_car`, mafia-re
    /// game/ai/C_shvestky.cpp:936-937).
    pub police_car: bool,
}

/// Host-owned backends for `tick_do_rici_actors` and `police_tick_prefix`.
/// Only host-owned slots appear here (`InvalidateActor`, radar removal);
/// mission/camera/HUD owners (`act_mission`, `mafiaIndicators`,
/// `GetSledovacka` internals, `C_traffic_car`) stay deferred and are applied
/// by the host behind these slots.
pub trait DoRiciHost {
    /// Marker facts for one listed actor; `None` when the host owns no such
    /// actor (treated as `Other`: evicted without car effects).
    fn probe(&self, actor: PoliceActorId) -> Option<DoRiciMark>;
    /// `C_shvestky::RemoveAngryActor` 0x49f540 fan-out: the records own no
    /// angry lists here, so the host applies it to its own lists.
    fn global_del_nasrany(&mut self, actor: PoliceActorId);
    /// `mafiaIndicators.RadarRemoveCar`.
    fn radar_remove_car(&mut self, car: PoliceActorId);
    /// `GetSledovacka()->RemoveCar`.
    fn sledovacka_remove_car(&mut self, car: PoliceActorId);
    /// `C_traffic_car::ChlupatejVystrik` for an evicted police car.
    fn traffic_car_release(&mut self);
    /// `C_game::InvalidateActor(actor, false)`.
    fn invalidate_actor(&mut self, actor: PoliceActorId);
    /// `C_game::RemoveTemporaryActor`.
    fn remove_temporary_actor(&mut self, actor: PoliceActorId);
    /// `car->SetOwner(NULL, seat, 0.0f)` when an evicted entity leaves its car
    /// (mafia-re game/ai/C_shvestky.cpp:887).
    fn empty_car_seat(&mut self, car: PoliceActorId, seat: u32);
}

/// Owned subset of `C_shvestky` state written by `GameInit` 0x499cd0, the
/// person-registry transitions (`SetActive` 0x49b860 prefix, `AddPerson`
/// 0x49c7e0, `DelPerson` 0x49ca50) and the do_rici tick slice
/// (`TickDoRiciActors` 0x49ea80, `AddDoRiciActor` 0x49e820, `DelDoRiciActor`
/// 0x49e9e0, `FreeMyTempActor` 0x49e7e0, the `Tick` 0x49a300 timer/temp-car
/// prefix). Bridge-node lists stay outside this owner by construction.
/// `GameInit` leaves the `SetActive` scalars
/// (`arrest_text`, `unknown_38`, `unknown_39`) alone, matching the native body,
/// which never stores them; before the first `SetActive` they are native
/// uninitialized (the native constructor is empty), so the typed `Default`
/// (zero/false) is an explicit host default, not an original value.
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
    /// Failure-menu text at +0x34 (`SetActive`'s; upstream notes 980000,
    /// "You've been arrested", as the usual script value).
    arrest_text: u32,
    /// Cleared by `SetActive`; read by `ReleaseWantedHuman` and Tick.
    unknown_38: bool,
    /// First byte of `unknown_39`; `SetActive`'s `unknown2` parameter.
    unknown_39: bool,
    records: Vec<PolicePursuitRecord>,
    road_block_cars: Vec<PoliceRoadBlockCar>,
    road_block_irises: Vec<PoliceRoadBlockIris>,
    road_block_enemies: Vec<PoliceRoadBlockEnemy>,
    do_rici_actors: Vec<PoliceActorId>,
    /// Police temporary cars on the radar (`temp_cars`, mafia-re
    /// game/ai/C_shvestky.h:159-160). Like the original `GameInit` 0x499cd0
    /// (mafia-re game/ai/C_shvestky.cpp:41-56), `apply_game_init` leaves this
    /// list alone; `GameDone` owns its teardown (deferred).
    temp_cars: Vec<PoliceActorId>,
}

impl PoliceGameInitState {
    /// Original `C_shvestky::GetActive` (mafia-re game/ai/C_shvestky.h:237-241
    /// at pin 113b5b6; `FUNCTION: GAME` VA 0x47b4e0, RVA 0x7b4e0): reads the
    /// manager's `active` flag. `PM_SHOWSYMBOL` and its group do nothing while
    /// it is set.
    pub fn active(&self) -> bool {
        self.active
    }

    pub fn records_timer(&self) -> PoliceTimer {
        self.records_timer
    }

    /// Working name (no separate native entry): reads the owned do_rici
    /// period timer (`S_timer` at +0x14, mafia-re game/ai/C_shvestky.h:144).
    /// The native timer fires `TickDoRiciActors` inline in `Tick` 0x49a300
    /// with no dedicated getter, so no RVA is claimed.
    pub fn do_rici_timer(&self) -> PoliceTimer {
        self.do_rici_timer
    }

    /// Working name (no separate native entry): reads the owned red-light
    /// period timer (`S_timer` at +0x1c, mafia-re game/ai/C_shvestky.h:145).
    /// The native timer gates `TestOverRed` inline in `Tick` 0x49a300 with no
    /// dedicated getter, so no RVA is claimed.
    pub fn red_light_timer(&self) -> PoliceTimer {
        self.red_light_timer
    }

    /// Working name (no separate native entry): reads the owned +0x24 period
    /// timer (`S_timer` at +0x24, mafia-re game/ai/C_shvestky.h:146, purpose
    /// not documented). The native timer is reset by `GameInit` 0x499cd0 and
    /// read inline with no dedicated getter, so no RVA is claimed and no
    /// gameplay meaning is attached.
    pub fn unknown_24_timer(&self) -> PoliceTimer {
        self.unknown_24
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

    pub fn road_block_enemy_count(&self) -> usize {
        self.road_block_enemies.len()
    }

    pub fn do_rici_actor_count(&self) -> usize {
        self.do_rici_actors.len()
    }

    pub fn temp_car_count(&self) -> usize {
        self.temp_cars.len()
    }

    pub fn temp_cars(&self) -> &[PoliceActorId] {
        &self.temp_cars
    }

    pub fn arrest_text(&self) -> u32 {
        self.arrest_text
    }

    pub fn unknown_38(&self) -> bool {
        self.unknown_38
    }

    pub fn unknown_39(&self) -> bool {
        self.unknown_39
    }

    /// Working name (no separate native entry): reads the owned +0x2c word
    /// (mafia-re game/ai/C_shvestky.h:147). `GameInit` 0x499cd0 clears it;
    /// no dedicated native getter exists, so no RVA is claimed and no
    /// gameplay meaning is attached.
    pub fn unknown_2c(&self) -> u32 {
        self.unknown_2c
    }

    /// Working name (no separate native entry): reads the owned +0x30 word
    /// (mafia-re game/ai/C_shvestky.h:148). `GameInit` 0x499cd0 clears it;
    /// no dedicated native getter exists, so no RVA is claimed and no
    /// gameplay meaning is attached.
    pub fn unknown_30(&self) -> u32 {
        self.unknown_30
    }

    /// Working name (no separate native entry): reports whether any record
    /// is filed for the human, scanning back to front like every native
    /// record lookup (mafia-re game/ai/C_shvestky.cpp:805-954 at pin 113b5b6,
    /// `PocetNasranych`/`TickDoRiciActors` order). No dedicated native
    /// getter exists, so no RVA is claimed.
    pub fn has_record_for_human(&self, human: PoliceHumanId) -> bool {
        for record in self.records.iter().rev() {
            if record.human == human {
                return true;
            }
        }
        false
    }

    /// Working name (no separate native entry): reports whether the actor is
    /// listed in `do_rici_actors`, scanning back to front like the native
    /// registry removals (mafia-re game/ai/C_shvestky.h:159-160,
    /// `AddDoRiciActor` duplicate guard shape). No dedicated native getter
    /// exists, so no RVA is claimed.
    pub fn is_do_rici_listed(&self, actor: PoliceActorId) -> bool {
        self.do_rici_actors.iter().rev().any(|&listed| listed == actor)
    }

    /// Working name (no separate native entry): reports whether the car is
    /// tracked in the police `temp_cars`, in stored order (mafia-re
    /// game/ai/C_shvestky.h:159-160, `FreeMyTempActor` linear search shape).
    /// No dedicated native getter exists, so no RVA is claimed.
    pub fn is_temp_car_tracked(&self, car: PoliceActorId) -> bool {
        self.temp_cars.iter().any(|&tracked| tracked == car)
    }

    /// Working name (no separate native entry): counts the `free` marks of
    /// `road_block_cars` (`S_road_block_car`, actor plus `free` at +0x4,
    /// mafia-re game/ai/C_shvestky.h:120-124). No PC outline is evidenced
    /// for the car count (unlike the enemy count at 0x49e530), so no RVA is
    /// claimed; the count is read inline by the road-block setup.
    pub fn count_free_road_block_cars(&self) -> u32 {
        self.road_block_cars
            .iter()
            .filter(|entry| entry.free)
            .count() as u32
    }

    /// Working name (no separate native entry): counts the `free` marks of
    /// `road_block_irises` (`S_road_block_iris`, actor plus `free` at +0x4,
    /// mafia-re game/ai/C_shvestky.h:125-129). No PC outline is evidenced
    /// for the iris count, so no RVA is claimed.
    pub fn count_free_road_block_irises(&self) -> u32 {
        self.road_block_irises
            .iter()
            .filter(|entry| entry.free)
            .count() as u32
    }

    /// Working name (no separate native entry): checks out the last free
    /// road-block car (`S_road_block_car`, mafia-re
    /// game/ai/C_shvestky.h:120-124), marking it used and returning its car
    /// (`None` when none is free). No PC outline is evidenced for the car
    /// checkout (unlike the iris checkout at 0x49e560), so no RVA is
    /// claimed; the shape follows the iris outline and the PS2
    /// `GetTempCar`/`FreeTempCar` family.
    pub fn take_free_road_block_car(&mut self) -> Option<PoliceActorId> {
        let index = self.road_block_cars.iter().rposition(|entry| entry.free)?;
        self.road_block_cars[index].free = false;
        Some(self.road_block_cars[index].car)
    }

    /// Original `C_shvestky::SetActive` 0x49b860 registry prefix: when the
    /// requested state already holds, the native body returns before any
    /// store; otherwise it records the flag, the failure-menu text, clears
    /// +0x38 and stores `unknown2` at +0x39, in that order. Returns whether
    /// any store happened. The remainder of the native body (bridge-node
    /// collection, police-model preload, player auto-add) needs mission/level
    /// owners and stays deferred.
    pub fn apply_set_active(&mut self, on: bool, arrest_text: u32, unknown2: bool) -> bool {
        if on == self.active {
            return false;
        }
        self.active = on;
        self.arrest_text = arrest_text;
        self.unknown_38 = false;
        self.unknown_39 = unknown2;
        true
    }

    /// Original `C_shvestky::AddPerson` 0x49c7e0 registry transition: a null
    /// human (identity zero, the native null) or an inactive manager adds
    /// nothing; a human that already has a record (scanned from the back, as
    /// natively) adds nothing. Otherwise a cleared record is appended and
    /// reported. `car` carries the human's current car when that car reads as
    /// `ACTOR_CAR` (native: `human->car` with `car->type == 4`), `None`
    /// otherwise; the caller owns that read. The native constructor also
    /// writes one uninterpreted byte at +0x70 from the human word's low byte
    /// (single closed probe session, bounds not retained; upstream
    /// C_shvestky.h documents `nasrany` at 0x70, so the store may overlap the
    /// vector head per upstream layout — wording softened accordingly);
    /// this owner does not model it. Returns whether a record was
    /// appended.
    pub fn add_person(&mut self, human: PoliceHumanId, car: Option<PoliceActorId>) -> bool {
        if human == 0 || !self.active {
            return false;
        }
        if self.records.iter().rev().any(|record| record.human == human) {
            return false;
        }
        let mut record = PolicePursuitRecord::new(human);
        record.car = car;
        self.records.push(record);
        true
    }

    /// Original `C_shvestky::DelPerson` 0x49ca50 registry transition: the
    /// same null/inactive/empty guards as `AddPerson`, then a back-to-front
    /// search for the human's record. The found record is removed with
    /// swap-with-back + pop (`swap_remove`, identical order) and returned so
    /// future `DelDoRiciActor` 0x49e820 wiring can enumerate it; the native
    /// body additionally routes each of the record's angry actors through
    /// `DelDoRiciActor` and frees the record storage, both deferred here.
    /// Returns the removed record, or `None` when no store happened.
    pub fn del_person(&mut self, human: PoliceHumanId) -> Option<PolicePursuitRecord> {
        if human == 0 || !self.active || self.records.is_empty() {
            return None;
        }
        let index = (0..self.records.len())
            .rev()
            .find(|&i| self.records[i].human == human)?;
        Some(self.records.swap_remove(index))
    }

    /// Original inline `C_shvestky::SetSpeed` (mafia-re game/ai/C_shvestky.h,
    /// accessor copy GAME 0x47b4f0): stores both limits. `POLICEMANAGER_SETSPEED`
    /// passes km/h scaled by 1/3.6; the caller owns that scaling.
    pub fn apply_set_speed(&mut self, speed_limit: f32, speed_limit_arrest: f32) {
        self.speed_limit = speed_limit.to_bits();
        self.speed_limit_arrest = speed_limit_arrest.to_bits();
    }

    /// Original `C_shvestky::GameInit` 0x499cd0 in native order: inactive,
    /// timer resets (330/913/217/4013 ms), 17.0/28.0 speed limits, zeroed
    /// words, and cleared lists. The `SetActive` scalars stay untouched, as
    /// natively, and so do the road-block enemies and temp cars, which the
    /// native body (mafia-re game/ai/C_shvestky.cpp:41-56) clears neither of
    /// (`GameDone` owns their teardown; deferred).
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

    /// Original `C_shvestky::AddDoRiciActor` 0x49e820 list transition
    /// (mafia-re game/ai/C_shvestky.cpp:838-842 is a log-only stub; shape per
    /// the GAME contract): appends the actor unless it is already listed.
    /// Returns whether the actor was appended.
    pub fn admit_do_rici(&mut self, actor: PoliceActorId) -> bool {
        if self.do_rici_actors.contains(&actor) {
            return false;
        }
        self.do_rici_actors.push(actor);
        true
    }

    /// Original `C_shvestky::DelDoRiciActor` 0x49e9e0 list transition
    /// (mafia-re game/ai/C_shvestky.cpp:844-848 is a log-only stub; shape per
    /// the GAME contract): removes the actor with swap-with-back + pop,
    /// searched back-to-front like the other registry removals here (the
    /// duplicate guard in `admit_do_rici` keeps at most one copy, so the
    /// direction is unobservable). Returns whether the actor was listed.
    pub fn evict_do_rici(&mut self, actor: PoliceActorId) -> bool {
        let Some(index) = self
            .do_rici_actors
            .iter()
            .rposition(|&listed| listed == actor)
        else {
            return false;
        };
        self.do_rici_actors.swap_remove(index);
        true
    }

    /// Original `C_shvestky::FreeMyTempActor` 0x49e7e0 ownership test
    /// (mafia-re game/ai/C_shvestky.cpp:831-836 is a log-only stub; freed-set
    /// per mafia-re game/ai/C_shvestky.h:117-134 and 229-231, PS2's
    /// `FreeTempCar`/`FreeTempEnemy`/`FreeTempIris` by entry type): marks the
    /// matching road-block car/enemy/policeman entry free and reports it, and
    /// releases a matching temporary car from the police's `temp_cars`
    /// tracking (order-preserving; the car itself stays in the game, which is
    /// why `TickDoRiciActors` skips its `RemoveTemporaryActor` on `true`).
    /// Reports `false` for any other actor.
    pub fn free_my_temp_actor(&mut self, actor: PoliceActorId) -> bool {
        for entry in &mut self.road_block_cars {
            if entry.car == actor {
                entry.free = true;
                return true;
            }
        }
        for entry in &mut self.road_block_enemies {
            if entry.actor == actor {
                entry.free = true;
                return true;
            }
        }
        for entry in &mut self.road_block_irises {
            if entry.actor == actor {
                entry.free = true;
                return true;
            }
        }
        if let Some(index) = self.temp_cars.iter().position(|&car| car == actor) {
            self.temp_cars.remove(index);
            return true;
        }
        false
    }

    /// Original `C_shvestky::TickDoRiciActors` 0x49ea80 (mafia-re
    /// game/ai/C_shvestky.cpp:858-954 with the PS2 order/shape notes there):
    /// evicts listed actors far from the camera on the ground plane
    /// (`dist_sq_xz > 22500`, i.e. 150 m; `y` forced to zero), switching on
    /// the host-supplied marker kind. An entity goes once it is that far, or
    /// at once by a dismissed flag where its `human_type` is 4; one in a car
    /// empties the car and reruns the loop for it when far, else the records
    /// forget it while it stays listed. A car goes once it is that far with
    /// no owner in its seats and no player/entity holding it, or once its
    /// frame is off; every other actor goes at once. Eviction runs
    /// `GlobalDelNasrany` plus the car radar/`sledovacka`/temp cleanup through
    /// the host, then swap-removes from the list; the `again` loop reruns the
    /// pass after an entity leaves its car, and `all` wipes any remainder.
    /// Mission/camera/HUD owners stay deferred behind `DoRiciHost`.
    pub fn tick_do_rici_actors(
        &mut self,
        camera: [f32; 3],
        all: bool,
        host: &mut impl DoRiciHost,
    ) {
        loop {
            let mut again = false;
            let mut i = 0;
            while i < self.do_rici_actors.len() {
                let actor = self.do_rici_actors[i];
                let mark = host.probe(actor);
                let mut remove = all;
                let mut empty_seat: Option<(PoliceActorId, u32)> = None;
                let mut forget_only = false;
                if !remove {
                    match mark {
                        Some(marked) if marked.kind == DoRiciActorKind::Entity => {
                            let dx = marked.pos[0] - camera[0];
                            let dz = marked.pos[2] - camera[2];
                            let dist_sq = dx * dx + dz * dz;
                            let mut entity_remove = dist_sq > DO_RICI_GROUND_DIST_SQ;
                            if !marked.dead && marked.human_type_is_4 {
                                entity_remove = marked.dismissed;
                            }
                            if entity_remove {
                                if let Some(car) = marked.car {
                                    entity_remove = dist_sq > DO_RICI_GROUND_DIST_SQ;
                                    if entity_remove {
                                        empty_seat = Some((car, marked.seat));
                                        again = true;
                                    } else {
                                        forget_only = true;
                                    }
                                }
                            }
                            remove = entity_remove;
                        }
                        Some(marked) if marked.kind == DoRiciActorKind::Car => {
                            let dx = marked.pos[0] - camera[0];
                            let dz = marked.pos[2] - camera[2];
                            let dist_sq = dx * dx + dz * dz;
                            if dist_sq > DO_RICI_GROUND_DIST_SQ {
                                remove = !marked.seats_occupied && !marked.referenced;
                            } else if !marked.frame_on {
                                remove = true;
                            }
                        }
                        _ => remove = true,
                    }
                }
                if forget_only {
                    host.global_del_nasrany(actor);
                    i += 1;
                    continue;
                }
                if remove {
                    if let Some((car, seat)) = empty_seat {
                        host.empty_car_seat(car, seat);
                    }
                    host.global_del_nasrany(actor);
                    let is_car = matches!(
                        mark,
                        Some(DoRiciMark {
                            kind: DoRiciActorKind::Car,
                            ..
                        })
                    );
                    let police_car = mark.map(|marked| marked.police_car).unwrap_or(false);
                    if is_car {
                        host.radar_remove_car(actor);
                        host.sledovacka_remove_car(actor);
                        if police_car {
                            host.traffic_car_release();
                        }
                    }
                    host.invalidate_actor(actor);
                    if !self.free_my_temp_actor(actor) {
                        host.invalidate_actor(actor);
                        host.remove_temporary_actor(actor);
                    }
                    self.do_rici_actors.swap_remove(i);
                } else {
                    i += 1;
                }
            }
            if !again {
                break;
            }
        }
        if all {
            self.do_rici_actors.clear();
        }
    }

    /// First block of `C_shvestky::Tick` 0x49a300 (mafia-re
    /// game/ai/C_shvestky.cpp:167-177): runs `TickDoRiciActors(false)` at each
    /// `do_rici_timer` period, then trims `temp_cars` past five from the
    /// front through the host backend. Camera/HUD/mission owners stay
    /// deferred behind `DoRiciHost`.
    pub fn police_tick_prefix(
        &mut self,
        camera: [f32; 3],
        time_ms: u32,
        host: &mut impl DoRiciHost,
    ) {
        if self.do_rici_timer.tick(time_ms) {
            self.tick_do_rici_actors(camera, false, host);
        }
        while self.temp_cars.len() > 5 {
            let car = self.temp_cars.remove(0);
            host.invalidate_actor(car);
            host.radar_remove_car(car);
            host.remove_temporary_actor(car);
        }
    }

    /// Road-block enemy availability at GAME VA 0x49e530 (RVA 0x9e530):
    /// Ghidra procedure with a 22-instruction body (REA ev_fe2410b6ff71)
    /// counting the `free` marks of `road_block_enemies` back to front
    /// (`S_road_block_enemy`, 8 bytes: actor plus `free` at +0x4, mafia-re
    /// game/ai/C_shvestky.h:130-134). Working name: no upstream symbol names
    /// this outline.
    pub fn count_free_road_block_enemies(&self) -> u32 {
        self.road_block_enemies
            .iter()
            .filter(|entry| entry.free)
            .count() as u32
    }

    /// Road-block policeman checkout at GAME VA 0x49e560 (RVA 0x9e560):
    /// Ghidra procedure with a 28-instruction body (REA ev_4c9c3cd8) taking
    /// the last free `road_block_iris` (`S_road_block_iris`, actor plus
    /// `free` at +0x4, mafia-re game/ai/C_shvestky.h:125-129), marking it
    /// used and returning its actor (`None` for the native null when none is
    /// free). Working name: the PS2 `FreeTempIris`/`GetTempIris` pair
    /// (header lines 228-231) documents the family; no PC symbol is
    /// evidenced for this outline.
    pub fn take_free_road_block_iris(&mut self) -> Option<PoliceActorId> {
        let index = self.road_block_irises.iter().rposition(|entry| entry.free)?;
        self.road_block_irises[index].free = false;
        Some(self.road_block_irises[index].actor)
    }

    /// Road-block enemy checkout at GAME VA 0x49e610 (RVA 0x9e610):
    /// Ghidra procedure with a 54-instruction body (REA ev_cc188f0d) counting
    /// the free `road_block_enemies`, drawing one pick through `C_actor::rnd`
    /// 0x408450 (nonnegative remainder of the free count, zero for a zero
    /// count, which cannot happen here), then taking the matching free entry
    /// from the back and marking it used (`None` for the native null when
    /// none is free or the pick runs past the end). The host owns the random
    /// source: `rand_mod` receives the free count (always at least 1) and
    /// returns the pick. Working name: the PS2 `GetTempEnemy` documents the
    /// family (mafia-re game/ai/C_shvestky.h:228-231); no PC symbol is
    /// evidenced for this outline.
    pub fn take_random_free_road_block_enemy(
        &mut self,
        rand_mod: impl FnOnce(u32) -> u32,
    ) -> Option<PoliceActorId> {
        let free = self.count_free_road_block_enemies();
        if free == 0 {
            return None;
        }
        let mut skip = rand_mod(free);
        for entry in self.road_block_enemies.iter_mut().rev() {
            if !entry.free {
                continue;
            }
            if skip == 0 {
                entry.free = false;
                return Some(entry.actor);
            }
            skip -= 1;
        }
        None
    }

    /// Record alerted flag at GAME VA 0x49d5b0 (RVA 0x9d5b0): Ghidra
    /// procedure with a 28-instruction body (REA ev_cf98a051) scanning the
    /// records back to front for the human and returning that record's
    /// `alerted` byte (`false` without a record). This matches the PS2
    /// `ChceHoOpichatKazdyPoldaNaOkresku` ("every cop in the district
    /// wants him") read of `alerted` (mafia-re game/ai/C_shvestky.h:91-93). Working
    /// name: no upstream symbol is evidenced for this outline.
    pub fn human_record_alerted(&self, human: PoliceHumanId) -> bool {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.alerted;
            }
        }
        false
    }

    /// Working name (no separate outline entry is claimed): manager lookup
    /// half of `C_shvestky::JakPicusNaTomJest` (GAME VA 0x49d390, RVA 0x9d390,
    /// already ported by `PolicePursuitRecord::status_word`): scans the
    /// records back to front for the human and returns that record's `state`
    /// (`SHVSTATE_NONE` without a record, mafia-re
    /// game/ai/C_shvestky.h:211). The single outline entry covers the lookup
    /// plus the field read, so no second RVA is claimed here.
    pub fn status_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.status_word();
            }
        }
        SHVSTATE_NONE
    }

    /// Working name (no separate outline entry is claimed): manager lookup
    /// half of `C_shvestky::GetCrashTime` (GAME VA 0x49d3e0, RVA 0x9d3e0,
    /// already ported by `PolicePursuitRecord::crash_time`): scans the
    /// records back to front for the human and returns that record's
    /// `crash_time` (`0` without a record, matching the log-only stub's zero
    /// at mafia-re game/ai/C_shvestky.cpp:791-795). The single outline entry
    /// covers the lookup plus the field read, so no second RVA is claimed.
    pub fn crash_time_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.crash_time();
            }
        }
        0
    }

    /// Working name (no separate outline entry is claimed): manager lookup
    /// half of `C_shvestky::GetFireTime` (GAME VA 0x49d420, RVA 0x9d420,
    /// already ported by `PolicePursuitRecord::fire_time`): scans the records
    /// back to front for the human and returns that record's `fire_time`
    /// (`0` without a record, matching the log-only stub's zero at mafia-re
    /// game/ai/C_shvestky.cpp:798-802). No second RVA is claimed.
    pub fn fire_time_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.fire_time();
            }
        }
        0
    }

    /// Working name (no separate outline entry is claimed): manager lookup
    /// half of `C_shvestky::PocetPrestupku` (GAME VA 0x49d520, RVA 0x9d520,
    /// already ported by `PolicePursuitRecord::offenses`): scans the records
    /// back to front for the human and returns that record's `offenses`
    /// count (`0` without a record, matching the log-only stub's zero at
    /// mafia-re game/ai/C_shvestky.cpp:1000-1004). No second RVA is claimed.
    pub fn offenses_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.offenses();
            }
        }
        0
    }

    /// Working name (no separate outline entry is claimed): manager lookup
    /// half of `C_shvestky::GetHumanStatus` (GAME VA 0x49d340, RVA 0x9d340,
    /// already ported by `PolicePursuitRecord::human_status`): scans the
    /// records back to front for the human and returns that record's status
    /// flag bits (`0` without a record). No second RVA is claimed.
    pub fn human_status_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.human_status();
            }
        }
        0
    }

    /// Working name (no separate outline entry is claimed): manager lookup
    /// half of `C_shvestky::NeviDoKterePiceVlezl` (GAME VA 0x49d560, RVA
    /// 0x9d560, already ported by `PolicePursuitRecord::is_nevi`): returns
    /// `false` while `unknown_38` is set, else scans the records back to
    /// front for the human and returns that record's `nevi` flag (`false`
    /// without a record, mafia-re game/ai/C_shvestky.h:94, 218-220). No
    /// second RVA is claimed.
    pub fn nevi_for_human(&self, human: PoliceHumanId) -> bool {
        if self.unknown_38 {
            return false;
        }
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.is_nevi();
            }
        }
        false
    }

    /// Working name (no separate outline entry is claimed): manager lookup
    /// half of `C_shvestky::Firing` (GAME VA 0x49cb20, RVA 0x9cb20, already
    /// ported by `PolicePursuitRecord::record_firing`): scans the records
    /// back to front for the human and latches `SHVF_FIRE` with the
    /// caller-supplied `fire_time` (`C_human::Shooting` owns the duration,
    /// mafia-re game/ai/C_shvestky.h:194-195). Returns whether a record was
    /// found. No second RVA is claimed.
    pub fn apply_firing_for_human(&mut self, human: PoliceHumanId, fire_time: u32) -> bool {
        for record in self.records.iter_mut().rev() {
            if record.human == human {
                record.record_firing(fire_time);
                return true;
            }
        }
        false
    }

    /// Working name (no separate outline entry is claimed): manager lookup
    /// half of `C_shvestky::HumanBoxing` (GAME VA 0x49d030, RVA 0x9d030,
    /// already ported by `PolicePursuitRecord::record_boxing`): scans the
    /// records back to front for the human and adds 5000 ms to `boxing_time`
    /// with `SHVF_BOXING` set (mafia-re game/ai/C_shvestky.h:205-206).
    /// Returns whether a record was found. No second RVA is claimed.
    pub fn apply_boxing_for_human(&mut self, human: PoliceHumanId) -> bool {
        for record in self.records.iter_mut().rev() {
            if record.human == human {
                record.record_boxing();
                return true;
            }
        }
        false
    }

    /// Working name (no separate outline entry is claimed): manager lookup
    /// half of `C_shvestky::ForceArrest` (GAME VA 0x49d130, RVA 0x9d130,
    /// already ported by `PolicePursuitRecord::record_force_arrest`): scans
    /// the records back to front for the human, sets `SHVF_FORCE_ARREST` and
    /// raises `new_state` to `SHVSTATE_ARREST`. Returns whether a record was
    /// found. The script `a`/`b` parameters need mission owners and stay
    /// deferred. No second RVA is claimed.
    pub fn apply_force_arrest_for_human(&mut self, human: PoliceHumanId) -> bool {
        for record in self.records.iter_mut().rev() {
            if record.human == human {
                record.record_force_arrest();
                return true;
            }
        }
        false
    }

    /// Working name (no separate outline entry is claimed): manager lookup
    /// half of `C_shvestky::NextNaSrackySoudruh` (GAME VA 0x49d220, RVA
    /// 0x9d220, already ported by `PolicePursuitRecord::record_run_over`):
    /// scans the records back to front for the human and counts one more
    /// run-over, raising `state`/`new_state` to `SHVSTATE_KILL` at six
    /// (mafia-re game/ai/C_shvestky.h:106-108). Returns whether a record was
    /// found. No second RVA is claimed.
    pub fn apply_run_over_for_human(&mut self, human: PoliceHumanId) -> bool {
        for record in self.records.iter_mut().rev() {
            if record.human == human {
                record.record_run_over();
                return true;
            }
        }
        false
    }

    /// Working name (no separate outline entry is claimed): manager lookup
    /// half of `C_shvestky::DriveCar` (GAME VA 0x49cbb0, RVA 0x9cbb0,
    /// already ported by `PolicePursuitRecord::apply_drive_car`): scans the
    /// records back to front for the human and follows `car` (`None` is the
    /// native null; `C_human` calls the native with the entered car and with
    /// null on exit). Returns whether a record was found. The `ACTOR_CAR`
    /// type read needs actor owners and stays deferred. No second RVA is
    /// claimed.
    pub fn apply_drive_car_for_human(
        &mut self,
        human: PoliceHumanId,
        car: Option<PoliceActorId>,
    ) -> bool {
        for record in self.records.iter_mut().rev() {
            if record.human == human {
                record.apply_drive_car(car);
                return true;
            }
        }
        false
    }

    /// Working name (no separate outline entry is claimed): manager lookup
    /// half of `C_shvestky::CarSteal` (GAME VA 0x49d0b0, RVA 0x9d0b0,
    /// already ported by `PolicePursuitRecord::record_car_steal`): scans the
    /// records back to front for the human, sets `SHVF_CAR_STEAL` and latches
    /// `car_steal_time` to 5000 (mafia-re game/ai/C_shvestky.h:24-25, 65).
    /// Returns whether a record was found. The native `bool, bool` pair has
    /// no evidenced meaning and stays deferred. No second RVA is claimed.
    pub fn apply_car_steal_for_human(&mut self, human: PoliceHumanId) -> bool {
        for record in self.records.iter_mut().rev() {
            if record.human == human {
                record.record_car_steal();
                return true;
            }
        }
        false
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `block_way` flag (`false` without a record). The native flag
    /// is read inline in `Tick` 0x49a300 (mafia-re
    /// game/ai/C_shvestky.cpp:388 at pin 113b5b6) with no dedicated
    /// function, so no RVA is claimed.
    pub fn block_way_for_human(&self, human: PoliceHumanId) -> bool {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.is_block_way();
            }
        }
        false
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `new_state` latch (`SHVSTATE_NONE` without a record), which
    /// `CreateShvestkyTest` raises from the flag arms and then promotes into
    /// `state` (mafia-re game/ai/C_shvestky.cpp:640-663 at pin 113b5b6). No
    /// dedicated function reads it natively, so no RVA is claimed.
    pub fn pending_state_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.pending_state();
            }
        }
        SHVSTATE_NONE
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `support_level` (`0` without a record), raised by
    /// `DalsiChalpiDoPartie` for the player's states 5 and 6 in
    /// `CreateShvestkyTest` (mafia-re game/ai/C_shvestky.cpp:721-736 at pin
    /// 113b5b6). No dedicated function reads it natively, so no RVA is
    /// claimed.
    pub fn support_level_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.support_level();
            }
        }
        0
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `wanted_time` (`-1` without a record, the cleared value).
    /// The native latch (`S_shvestky::wanted_time` at +0x44, mafia-re
    /// game/ai/C_shvestky.h:84-87) is read inline in `Tick` 0x49a300 and
    /// cleared to `-1` by `ReleaseWantedHuman` 0x49f850 with no dedicated
    /// getter, so no RVA is claimed.
    pub fn wanted_time_for_human(&self, human: PoliceHumanId) -> i32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.wanted_time();
            }
        }
        -1
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `wanted_time_start` (`0` without a record). The native word
    /// (`S_shvestky::wanted_time_start` at +0x48, mafia-re
    /// game/ai/C_shvestky.h:88-89) is stored by the wanted-clock refresh and
    /// read back by the HUD bar inline with no dedicated getter, so no RVA
    /// is claimed.
    pub fn wanted_time_start_for_human(&self, human: PoliceHumanId) -> i32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.wanted_time_start();
            }
        }
        0
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `signal_time` (`0` without a record). The native latch
    /// (`S_shvestky::signal_time` at +0x24, mafia-re
    /// game/ai/C_shvestky.h:71-73) is touched inline in `Tick` 0x49a300 with
    /// no dedicated getter, so no RVA is claimed.
    pub fn signal_time_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.signal_time();
            }
        }
        0
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `car_steal_time` (`0` without a record). The native latch
    /// (`S_shvestky::car_steal_time` at +0xc, mafia-re
    /// game/ai/C_shvestky.h:65) expires inline in `Tick` 0x49a300 with no
    /// dedicated getter, so no RVA is claimed.
    pub fn car_steal_time_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.car_steal_time();
            }
        }
        0
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `illegal_path_time` (`0` without a record). The native latch
    /// (`S_shvestky::illegal_path_time` at +0x14, mafia-re
    /// game/ai/C_shvestky.h:67) is maintained inline in `Tick` 0x49a300
    /// (mafia-re game/ai/C_shvestky.cpp:261-276 at pin 113b5b6) with no
    /// dedicated getter, so no RVA is claimed.
    pub fn illegal_path_time_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.illegal_path_time();
            }
        }
        0
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `collision_time` (`0` without a record). The native latch
    /// (`S_shvestky::collision_time` at +0x18, mafia-re
    /// game/ai/C_shvestky.h:68) expires inline in `Tick` 0x49a300 (mafia-re
    /// game/ai/C_shvestky.cpp:243-250 at pin 113b5b6) with no dedicated
    /// getter, so no RVA is claimed.
    pub fn collision_time_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.collision_time();
            }
        }
        0
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `boxing_time` (`0` without a record). The native latch
    /// (`S_shvestky::boxing_time` at +0x20, mafia-re
    /// game/ai/C_shvestky.h:70) expires inline in `Tick` 0x49a300 with no
    /// dedicated getter, so no RVA is claimed.
    pub fn boxing_time_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.boxing_time();
            }
        }
        0
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `hits_time` (`-1` without a record, the cleared value). The
    /// native latch (`S_shvestky::hits_time` at +0x40, mafia-re
    /// game/ai/C_shvestky.h:83-85) is forgotten inline in `Tick` 0x49a300
    /// (mafia-re game/ai/C_shvestky.cpp:327-331 at pin 113b5b6) with no
    /// dedicated getter, so no RVA is claimed.
    pub fn hits_time_for_human(&self, human: PoliceHumanId) -> i32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.hits_time();
            }
        }
        -1
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `hits` count (`0` without a record). The native count
    /// (`S_shvestky::hits` at +0x4c, mafia-re game/ai/C_shvestky.h:90) is
    /// maintained inline in `Tick` 0x49a300 with no dedicated getter, so no
    /// RVA is claimed.
    pub fn hits_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.hits();
            }
        }
        0
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `reason` word (`0` without a record). The native word
    /// (`S_shvestky::reason` at +0x64, `SHVREASON_*`, mafia-re
    /// game/ai/C_shvestky.h:39-45, 105) is written by the flag arms in
    /// `CreateShvestkyTest` with no dedicated getter, so no RVA is claimed.
    pub fn reason_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.reason();
            }
        }
        0
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `run_over` count (`0` without a record). The native count
    /// (`S_shvestky::run_over` at +0x68, mafia-re
    /// game/ai/C_shvestky.h:106-108) is maintained by
    /// `NextNaSrackySoudruh` 0x49d220 with no dedicated getter, so no RVA is
    /// claimed.
    pub fn run_over_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.run_over();
            }
        }
        0
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `known_car` latch (`None` without a record, the native
    /// null). The native field (`S_shvestky::known_car` at +0x54, mafia-re
    /// game/ai/C_shvestky.h:96-100) is compared inline in
    /// `CreateShvestkyTest` with no dedicated getter, so no RVA is claimed.
    pub fn known_car_for_human(&self, human: PoliceHumanId) -> Option<PoliceActorId> {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.known_car();
            }
        }
        None
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `known_on_foot` flag (`false` without a record). The native
    /// flag (`S_shvestky::known_on_foot` at +0x58, mafia-re
    /// game/ai/C_shvestky.h:100) shares the inline-only use of `known_car`,
    /// so no RVA is claimed.
    pub fn known_on_foot_for_human(&self, human: PoliceHumanId) -> bool {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.known_on_foot();
            }
        }
        false
    }

    /// Working name (no outline entry is claimed): reads the matched
    /// record's `rammed_car` word (`0` without a record). The native word
    /// (`S_shvestky::rammed_car` at +0x60, mafia-re
    /// game/ai/C_shvestky.h:104) is read inline in `CreateShvestkyTest`
    /// with no dedicated getter, so no RVA is claimed.
    pub fn rammed_car_for_human(&self, human: PoliceHumanId) -> u32 {
        for record in self.records.iter().rev() {
            if record.human == human {
                return record.rammed_car();
            }
        }
        0
    }

    /// Record road-block arm at GAME VA 0x49d2e0 (RVA 0x9d2e0): Ghidra
    /// procedure with a 35-instruction body (REA ev_e8aed070) that does
    /// nothing while the manager is inactive, then scans the records back to
    /// front for the human and, when that record is `alerted` with `state`
    /// above `SHVSTATE_ARREST`, sets `block_way`. This matches the documented
    /// `SerSeMuDoCesty` ("get in his way") effect: the police action comes
    /// up for such records only (mafia-re game/ai/C_shvestky.h:76-78).
    /// Working name: no upstream symbol is evidenced for this outline.
    pub fn apply_block_way_for_alerted_kill(&mut self, human: PoliceHumanId) {
        if !self.active {
            return;
        }
        for record in self.records.iter_mut().rev() {
            if record.human != human {
                continue;
            }
            if record.alerted && record.state > SHVSTATE_ARREST {
                record.block_way = true;
            }
            return;
        }
    }

    /// Record illegal-path arm at GAME VA 0x49cfd0 (RVA 0x9cfd0): Ghidra
    /// procedure with a 40-instruction body (REA ev_81f4a34e) that does
    /// nothing for a null car or while the manager is inactive, then scans
    /// the records back to front for the car, zeroes `illegal_path_time`
    /// unless `SHVF_ILLEGAL_PATH` is already set, and sets the bit. This
    /// matches the documented `CarIllegalPath` effect (mafia-re
    /// game/ai/C_shvestky.h:21).
    /// Working name: no upstream symbol is evidenced for this outline. The
    /// native null-car guard reads as `None` (`record.car` is `None` for the
    /// native null).
    pub fn apply_car_illegal_path(&mut self, car: Option<PoliceActorId>) {
        let Some(car) = car else {
            return;
        };
        if !self.active {
            return;
        }
        for record in self.records.iter_mut().rev() {
            if record.car != Some(car) {
                continue;
            }
            if record.flags & SHVF_ILLEGAL_PATH == 0 {
                record.illegal_path_time = 0;
            }
            record.flags |= SHVF_ILLEGAL_PATH;
            return;
        }
    }

    /// Record signal-mark arm at GAME VA 0x49d1a0 (RVA 0x9d1a0): Ghidra
    /// procedure with a 45-instruction body (REA ev_a51c6c45) that does
    /// nothing for a null human or while the manager is inactive, then scans
    /// the records back to front for the human and raises `new_state` to 1
    /// with `reason` 8 where lower, adds 4000 to `signal_time` clamped to
    /// 20000, and sets `SHVF_SIGNAL_MARK`. This matches the documented
    /// `DestroySignalMark` effect (a destroyed traffic sign, mafia-re
    /// game/ai/C_shvestky.h:71-73 and `C_shvestky.h:25`). Working name: no upstream symbol is evidenced
    /// for this outline.
    pub fn apply_destroy_signal_mark(&mut self, human: PoliceHumanId) {
        if !self.active {
            return;
        }
        for record in self.records.iter_mut().rev() {
            if record.human != human {
                continue;
            }
            if record.new_state < 1 {
                record.new_state = 1;
                record.reason = 8;
            }
            // Native `ADD`/`CMP`/`JLE`: keep the wrapped raise where it
            // tests signed `<= 20000`, else clamp.
            let raised = record.signal_time.wrapping_add(4000);
            record.signal_time = if raised as i32 <= 20000 { raised } else { 20000 };
            record.flags |= SHVF_SIGNAL_MARK;
            return;
        }
    }

    /// Record patrolman-hit arm at GAME VA 0x4a0040 (RVA 0xa0040): Ghidra
    /// procedure with a 44-instruction body (REA ev_8cb00a21) scanning the
    /// records back to front for the human, setting `SHVF_HIT_POLICEMAN`,
    /// then forcing `records_timer.time` up to `records_timer.period` where
    /// lower. This matches the documented `DalJsemPochuzkariDoDrzky` ("I
    /// punched the patrolman in the face") effect (`SHVSTATE_KILL`, mafia-re
    /// game/ai/C_shvestky.h:26-27). Working name: no upstream symbol is
    /// evidenced for this outline.
    pub fn apply_hit_policeman(&mut self, human: PoliceHumanId) {
        for record in self.records.iter_mut().rev() {
            if record.human != human {
                continue;
            }
            record.flags |= SHVF_HIT_POLICEMAN;
            // Native `SUB`/`JLE` on the timer words: force `time` up to
            // `period` only for a positive signed difference.
            if self
                .records_timer
                .period_ms
                .wrapping_sub(self.records_timer.time_ms) as i32
                > 0
            {
                self.records_timer.time_ms = self.records_timer.period_ms;
            }
            return;
        }
    }

    /// Record known-car refresh at GAME VA 0x4a0150 (RVA 0xa0150): Ghidra
    /// procedure with a 45-instruction body (REA ev_5af435a2) that does
    /// nothing while the manager is inactive, then scans the records back to
    /// front for the human and, where the human's current car
    /// (`C_human::car` at +0x98, mafia-re game/actors/C_human.h) still equals
    /// the record's `known_car`, stores the caller's car and on-foot flag.
    /// This matches the documented `ZalozSpisATahniDoRiti_Haleluja` ("open a
    /// file") refresh (mafia-re game/ai/C_shvestky.h:96-101). Working name:
    /// symbol is evidenced for this outline.
    pub fn refresh_record_known_car(
        &mut self,
        human: PoliceHumanId,
        human_car: Option<PoliceActorId>,
        known_on_foot: bool,
    ) {
        if !self.active {
            return;
        }
        for record in self.records.iter_mut().rev() {
            if record.human != human {
                continue;
            }
            if human_car == record.known_car {
                record.known_car = human_car;
                record.known_on_foot = known_on_foot;
            }
            return;
        }
    }

    /// Working name (no outline entry is claimed): the player-record HUD bar
    /// of `Tick` 0x49a300 (mafia-re game/ai/C_shvestky.cpp:424-450 at pin
    /// 113b5b6): for the player's record, an alerted record below its start
    /// reports the time left over the span (`wanted_time_start` minus the
    /// records-timer period, at least 1), clamped to 0..=1; an alerted record
    /// at or above its start reports full. `None` without a player record or
    /// without alert. The `mafiaIndicators` bar owner stays with the host.
    pub fn player_progress_fraction(&self, player: PoliceHumanId) -> Option<f32> {
        for record in self.records.iter().rev() {
            if record.human != player {
                continue;
            }
            if !record.alerted {
                return None;
            }
            let fraction =
                if record.wanted_time_start > 0 && record.wanted_time < record.wanted_time_start
                {
                    let mut left = record
                        .wanted_time
                        .wrapping_sub(self.records_timer.time_ms as i32);
                    if left > record.wanted_time {
                        left = record.wanted_time;
                    }
                    let mut total = record
                        .wanted_time_start
                        .wrapping_sub(self.records_timer.period_ms as i32);
                    if total < 1 {
                        total = 1;
                    }
                    left as f32 / total as f32
                } else {
                    1.0
                };
            return Some(fraction.clamp(0.0, 1.0));
        }
        None
    }

    /// Original `C_shvestky::~C_shvestky` 0x499c00 (GAME VA 0x499c00,
    /// RVA 0x99c00; mafia-re game/ai/C_shvestky.cpp:32-35 at pin 113b5b6):
    /// the native destructor body is empty (no stores). This owner holds no
    /// native allocations, so teardown is an explicit no-op marker; the
    /// state is left untouched.
    pub fn apply_dtor_empty(&mut self) {}
}

/// Opaque human identity for wanted-record lookup.
pub type PoliceHumanId = u32;

/// Cleared pursuit state written by `ReleaseWantedHuman` (`SHVSTATE_NONE`).
pub const SHVSTATE_NONE: u32 = 0;
/// Arrest state raised by `ForceArrest` (`SHVSTATE_ARREST`, mafia-re
/// game/ai/C_shvestky.h:36).
pub const SHVSTATE_ARREST: u32 = 2;
/// Kill state raised by the sixth `NextNaSrackySoudruh` run-over
/// (`SHVSTATE_KILL`, mafia-re game/ai/C_shvestky.h:37, 106-108).
pub const SHVSTATE_KILL: u32 = 3;

/// `SHVF_FIRE` 0x2: `Firing`'s bit, cleared once `fire_time` runs out
/// (mafia-re game/ai/C_shvestky.h:17).
pub const SHVF_FIRE: u32 = 0x2;
/// `SHVF_BOXING` 0x200: `HumanBoxing`'s bit, cleared once `boxing_time` runs
/// out (mafia-re game/ai/C_shvestky.h:22).
pub const SHVF_BOXING: u32 = 0x200;
/// `SHVF_CAR_STEAL` 0x800: `CarSteal`'s bit, cleared once `car_steal_time`
/// runs out (mafia-re game/ai/C_shvestky.h:24-25, 65).
pub const SHVF_CAR_STEAL: u32 = 0x800;
/// `SHVF_FORCE_ARREST` 0x4000: `ForceArrest`'s bit, `SHVSTATE_ARREST`
/// (mafia-re game/ai/C_shvestky.h:28).
pub const SHVF_FORCE_ARREST: u32 = 0x4000;
/// `SHVF_ILLEGAL_PATH` 0x40: `CarIllegalPath`'s bit, counted up while the car
/// is faster than 11 (mafia-re game/ai/C_shvestky.h:21).
pub const SHVF_ILLEGAL_PATH: u32 = 0x40;
/// `SHVF_SIGNAL_MARK` 0x1000: `DestroySignalMark`'s bit (a destroyed traffic
/// sign), cleared with `SHVF_RED_LIGHT` (mafia-re game/ai/C_shvestky.h:25).
pub const SHVF_SIGNAL_MARK: u32 = 0x1000;
/// `SHVF_HIT_POLICEMAN` 0x2000: `DalJsemPochuzkariDoDrzky`'s bit (the human
/// punched a patrolman; `SHVSTATE_KILL`, mafia-re game/ai/C_shvestky.h:27).
pub const SHVF_HIT_POLICEMAN: u32 = 0x2000;
/// `SHVF_WEAPON` 0x1: an item of 5 to 15 in the right hand, on foot or in a
/// car (mafia-re game/ai/C_shvestky.h:16, 46-52).
pub const SHVF_WEAPON: u32 = 0x1;
/// `SHVF_SPEEDING` 0x4: the car faster than `C_shvestky::speed_limit`
/// (mafia-re game/ai/C_shvestky.h:18).
pub const SHVF_SPEEDING: u32 = 0x4;
/// `SHVF_CRASH` 0x38: `CarCollision`'s three bits, cleared once
/// `collision_time` runs out (mafia-re game/ai/C_shvestky.h:19-20).
pub const SHVF_CRASH: u32 = 0x38;
/// `SHVF_RED_LIGHT` 0x400: `TestOverRed`'s bit, cleared once `signal_time`
/// runs out (mafia-re game/ai/C_shvestky.h:23).
pub const SHVF_RED_LIGHT: u32 = 0x400;
/// `SHVF_RAM_POLICE_CAR` 0x8000: `StrcilJsemHoPoldumDoVyfuku`'s bit (a
/// police car rammed; mafia-re game/ai/C_shvestky.h:29-30).
pub const SHVF_RAM_POLICE_CAR: u32 = 0x8000;
/// `SHVF2_CRASH` 0x38: `flags2` crash bits, cleared once `crash_time` runs
/// out (mafia-re game/ai/C_shvestky.h:30-32).
pub const SHVF2_CRASH: u32 = 0x38;

/// Ground-plane squared eviction distance for `TickDoRiciActors`
/// (150 m; mafia-re game/ai/C_shvestky.cpp:869, 876, 884, 896).
pub const DO_RICI_GROUND_DIST_SQ: f32 = 22500.0;

/// Host-supplied car facts for [`PolicePursuitRecord::tick_decay_timers`]:
/// the record car's speed and its own `illegal_path` flag (mafia-re
/// game/ai/C_shvestky.cpp:263-267 at pin 113b5b6). `None` is the native null
/// car, whose missing speed takes the clear arm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IllegalPathCarProbe {
    pub speed: f32,
    pub illegal_path: bool,
}

/// Host-dispatched outcomes of [`PolicePursuitRecord::update_wanted_clock`]:
/// the `DobreMiPokuril_MuzeJitDom` release and the `VasekSelChlastatNaOslavu`
/// schedule refresh (mafia-re game/ai/C_shvestky.cpp:333-369 at pin 113b5b6)
/// need the mission/game owners and run behind these markers.
/// `announce_full_bar` is the player's `SetProgress(1.0)` on the HUD owner,
/// which likewise stays deferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WantedClockAction {
    Steady,
    Release,
    RefreshSchedule { announce_full_bar: bool },
}

/// Still-unimplemented police dispatch. Entries name the original PC
/// addresses; the vulgar working names are retired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoliceDispatchError {
    Unsupported { entry: u32 },
}

/// `C_shvestky::SetActive` 0x49b860 (`POLICEMANAGER_ON`).
pub const SET_ACTIVE_ENTRY: u32 = 0x49b860;
/// `C_shvestky::AddPerson` 0x49c7e0 (`POLICEMANAGER_ADD`).
pub const ADD_PERSON_ENTRY: u32 = 0x49c7e0;
/// `C_shvestky::DelPerson` 0x49ca50 (`POLICEMANAGER_DEL`).
pub const DEL_PERSON_ENTRY: u32 = 0x49ca50;
/// `C_shvestky::SendPoliceBackup` 0x49eee0 (`POLICE_SUPPORT`).
pub const SEND_POLICE_BACKUP_ENTRY: u32 = 0x49eee0;
/// `C_shvestky::SendPoliceToHuman` 0x49f980.
pub const SEND_POLICE_TO_HUMAN_ENTRY: u32 = 0x49f980;
/// `C_shvestky::FreeMyTempActor` 0x49e7e0.
pub const FREE_TEMP_ACTOR_ENTRY: u32 = 0x49e7e0;
/// `C_shvestky::TickDoRiciActors` 0x49ea80.
pub const TICK_DO_RICI_ENTRY: u32 = 0x49ea80;
/// `C_shvestky::AddDoRiciActor` 0x49e820.
pub const ADD_DO_RICI_ENTRY: u32 = 0x49e820;
/// `C_shvestky::DelDoRiciActor` 0x49e9e0.
pub const DEL_DO_RICI_ENTRY: u32 = 0x49e9e0;
/// `C_shvestky::Firing` 0x49cb20.
pub const FIRING_ENTRY: u32 = 0x49cb20;
/// `C_shvestky::HumanBoxing` 0x49d030.
pub const HUMAN_BOXING_ENTRY: u32 = 0x49d030;
/// `C_shvestky::ForceArrest` 0x49d130.
pub const FORCE_ARREST_ENTRY: u32 = 0x49d130;
/// `C_shvestky::NextNaSrackySoudruh` 0x49d220.
pub const RUN_OVER_ENTRY: u32 = 0x49d220;
/// `C_shvestky::JakPicusNaTomJest` 0x49d390.
pub const STATUS_WORD_ENTRY: u32 = 0x49d390;
/// `C_shvestky::GetCrashTime` 0x49d3e0.
pub const CRASH_TIME_ENTRY: u32 = 0x49d3e0;
/// `C_shvestky::GetFireTime` 0x49d420.
pub const FIRE_TIME_ENTRY: u32 = 0x49d420;
/// `C_shvestky::PocetNasranych` 0x49d460.
pub const ANGRY_COUNT_ENTRY: u32 = 0x49d460;
/// `C_shvestky::PocetNasranychZivych` 0x49d4b0.
pub const ANGRY_ALIVE_COUNT_ENTRY: u32 = 0x49d4b0;
/// `C_shvestky::PocetPrestupku` 0x49d520.
pub const OFFENSES_ENTRY: u32 = 0x49d520;
/// `C_shvestky::NeviDoKterePiceVlezl` 0x49d560.
pub const NEVI_ENTRY: u32 = 0x49d560;
/// `C_shvestky::~C_shvestky` 0x499c00 (empty destructor body).
pub const SHVESTKY_DTOR_ENTRY: u32 = 0x499c00;
/// `C_shvestky::S_shvestky::~S_shvestky` 0x49a120 (record destructor).
pub const RECORD_DTOR_ENTRY: u32 = 0x49a120;
/// `C_shvestky::DriveCar` 0x49cbb0.
pub const DRIVE_CAR_ENTRY: u32 = 0x49cbb0;
/// `C_shvestky::GetHumanStatus` 0x49d340.
pub const HUMAN_STATUS_ENTRY: u32 = 0x49d340;
/// `C_shvestky::CarSteal` 0x49d0b0.
pub const CAR_STEAL_ENTRY: u32 = 0x49d0b0;
/// `C_shvestky::CreateRoadBlock` 0x49d980 (PC only).
pub const CREATE_ROAD_BLOCK_ENTRY: u32 = 0x49d980;
/// `C_shvestky::DezoSebralGumyIRadio` 0x49a140.
pub const TAKE_TYRES_RADIO_ENTRY: u32 = 0x49a140;
/// `C_shvestky::RenamePoliceCar` 0x4a0230.
pub const RENAME_POLICE_CAR_ENTRY: u32 = 0x4a0230;
/// `C_shvestky::GameDone` 0x499dc0.
pub const GAME_DONE_ENTRY: u32 = 0x499dc0;
/// Road-block iris release at GAME VA 0x49e5a0 (RVA 0x9e5a0): frees the iris
/// and notifies the game/radar owners.
pub const FREE_ROAD_BLOCK_IRIS_ENTRY: u32 = 0x49e5a0;
/// Player-pursuit predicate at GAME VA 0x4a01c0 (RVA 0xa01c0): reads the
/// mission player through the game owner.
pub const PLAYER_PURSUIT_ACTIVE_ENTRY: u32 = 0x4a01c0;

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

/// Original `C_shvestky::CreateRoadBlock` 0x49d980 (GAME VA 0x49d980,
/// RVA 0x9d980; mafia-re game/ai/C_shvestky.cpp:825-829 at pin 113b5b6 is a
/// log-only stub; PC-only road-block setup that `Tick` 0x49a300 calls once a
/// police car comes near a block). The game's road-block set plus the
/// mission/actor owners stay outside this contract, so the boundary is
/// explicit.
pub fn create_road_block(
    _index: u32,
    _human: PoliceHumanId,
) -> Result<(), PoliceDispatchError> {
    Err(PoliceDispatchError::Unsupported {
        entry: CREATE_ROAD_BLOCK_ENTRY,
    })
}

/// Original `C_shvestky::DezoSebralGumyIRadio` 0x49a140 (GAME VA 0x49a140,
/// RVA 0x9a140; mafia-re game/ai/C_shvestky.cpp:1006-1010 at pin 113b5b6 is
/// a log-only stub; the car the human takes from another driver, told via
/// `C_human::Use_Actor`). The car/wheel owners stay outside this contract,
/// so the boundary is explicit.
pub fn take_car_tyres_and_radio(_car: PoliceActorId) -> Result<(), PoliceDispatchError> {
    Err(PoliceDispatchError::Unsupported {
        entry: TAKE_TYRES_RADIO_ENTRY,
    })
}

/// Original `C_shvestky::RenamePoliceCar` 0x4a0230 (GAME VA 0x4a0230,
/// RVA 0xa0230; mafia-re game/ai/C_shvestky.cpp:1018-1022 at pin 113b5b6 is
/// a log-only stub; a police car the human gets into, told via
/// `C_human::Use_Actor`). The car/frame-name owner stays outside this
/// contract, so the boundary is explicit.
pub fn rename_police_car(_car: PoliceActorId) -> Result<(), PoliceDispatchError> {
    Err(PoliceDispatchError::Unsupported {
        entry: RENAME_POLICE_CAR_ENTRY,
    })
}

/// Original `C_shvestky::GameDone` 0x499dc0 (GAME VA 0x499dc0,
/// RVA 0x99dc0; mafia-re game/ai/C_shvestky.cpp:61-139 at pin 113b5b6):
/// deletes the pursuit records, keeps the next mission's actors out of the
/// do_rici list, hands the temporary cars back to the game and the radar,
/// and tears down the road-block cars, policemen and enemies. Every owner
/// it touches (mission actors, game, radar, road blocks, global actors,
/// frames) stays outside this contract, so the boundary is explicit.
pub fn police_game_done() -> Result<(), PoliceDispatchError> {
    Err(PoliceDispatchError::Unsupported {
        entry: GAME_DONE_ENTRY,
    })
}

/// Road-block iris release at GAME VA 0x49e5a0 (RVA 0x9e5a0): Ghidra
/// procedure with a 44-instruction body (REA ev_ea3337a2) scanning the
/// road-block irises for the actor; where found, it marks the entry free and
/// notifies the game (`STUB: GAME` callee 0x5a7d40 through the mission global)
/// and radar (`STUB: GAME` callee 0x445bb0) owners before reporting success.
/// Both owners stay outside this contract, so the boundary is explicit.
/// Working name: the PS2 `FreeTempIris` documents the family (mafia-re
/// game/ai/C_shvestky.h:228-231); no PC symbol is evidenced for this
/// outline.
pub fn free_road_block_iris(_actor: PoliceActorId) -> Result<bool, PoliceDispatchError> {
    Err(PoliceDispatchError::Unsupported {
        entry: FREE_ROAD_BLOCK_IRIS_ENTRY,
    })
}

/// Player-pursuit predicate at GAME VA 0x4a01c0 (RVA 0xa01c0): Ghidra
/// procedure with a 48-instruction body (REA ev_db6707a2) reading the mission
/// player through the game owner (`STUB: GAME` global 0x63788c), reporting
/// whether any `PocetNasranychZivych` 0x49d4b0 angry survivor exists, else
/// whether some record is filed (`state` nonzero or `alerted`) without
/// `nevi`. The mission player owner stays outside this contract, so the
/// boundary is explicit. Working name: no upstream symbol is evidenced for
/// this outline.
pub fn player_pursuit_active() -> Result<bool, PoliceDispatchError> {
    Err(PoliceDispatchError::Unsupported {
        entry: PLAYER_PURSUIT_ACTIVE_ENTRY,
    })
}

/// Chase-status word of `C_sledovacka::S_ELEMENT` (`E_STATUS`, mafia-re
/// game/ai/C_sledovacka.h:23-28 at pin 113b5b6): `STATUS_DONE` for a car
/// without a chase, `STATUS_FAILED` for a lost chase, `STATUS_DRIVER_DEAD`
/// for a dead driver, `STATUS_NO_ENERGY` for a car without energy. The
/// follower's `Update` compares the word unsigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PursuitChaseStatus {
    /// `STATUS_DONE` (1): the chase is done; also the original's answer for
    /// a car without a chase.
    Done = 1,
    /// `STATUS_FAILED` (2): the chase lost its target.
    Failed = 2,
    /// `STATUS_DRIVER_DEAD` (3): the chase's driver is dead.
    DriverDead = 3,
    /// `STATUS_NO_ENERGY` (4): the chase's car is out of energy.
    NoEnergy = 4,
}

impl PursuitChaseStatus {
    pub const fn word(self) -> u32 {
        self as u32
    }
}

/// Still-unimplemented chase-manager (`C_sledovacka`, the pursuit owner)
/// calls. Entries name the original PC addresses; the chase-list,
/// driver, road and collision owners stay outside this contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SledovackaError {
    Unsupported { entry: u32 },
}

/// `C_sledovacka::AddCarHunt` 0x4a0760 (GAME VA 0x4a0760, RVA 0xa0760).
pub const SLEDOVACKA_ADD_CAR_HUNT_ENTRY: u32 = 0x4a0760;
/// `C_sledovacka::AddCarMoveTo` 0x4a1220 (GAME VA 0x4a1220, RVA 0xa1220).
pub const SLEDOVACKA_ADD_CAR_MOVE_TO_ENTRY: u32 = 0x4a1220;
/// `C_sledovacka::RemoveCar` 0x4a1c30 (GAME VA 0x4a1c30, RVA 0xa1c30).
pub const SLEDOVACKA_REMOVE_CAR_ENTRY: u32 = 0x4a1c30;
/// `C_sledovacka::RecompilePath` 0x4a1da0 (GAME VA 0x4a1da0, RVA 0xa1da0).
pub const SLEDOVACKA_RECOMPILE_PATH_ENTRY: u32 = 0x4a1da0;
/// `C_sledovacka::PreparePath` 0x4a28c0 (GAME VA 0x4a28c0, RVA 0xa28c0).
pub const SLEDOVACKA_PREPARE_PATH_ENTRY: u32 = 0x4a28c0;
/// `C_sledovacka::TickDriver` 0x4a2a80 (GAME VA 0x4a2a80, RVA 0xa2a80).
pub const SLEDOVACKA_TICK_DRIVER_ENTRY: u32 = 0x4a2a80;
/// `C_sledovacka::ActionStatus` 0x4a8670 (GAME VA 0x4a8670, RVA 0xa8670).
pub const SLEDOVACKA_ACTION_STATUS_ENTRY: u32 = 0x4a8670;
/// `C_sledovacka::SetActionStatus` 0x4a86d0 (GAME VA 0x4a86d0, RVA 0xa86d0).
pub const SLEDOVACKA_SET_ACTION_STATUS_ENTRY: u32 = 0x4a86d0;
/// `C_sledovacka::C_sledovacka` 0x4a0320 (constructor, `road_net(0)`).
pub const SLEDOVACKA_CTOR_ENTRY: u32 = 0x4a0320;
/// `C_sledovacka::Init` 0x4a0360 (bridge collection).
pub const SLEDOVACKA_INIT_ENTRY: u32 = 0x4a0360;
/// `C_sledovacka::Done` 0x4a0660 (driver/element teardown).
pub const SLEDOVACKA_DONE_ENTRY: u32 = 0x4a0660;

/// Original `C_sledovacka::ActionStatus` (mafia-re
/// game/ai/C_sledovacka.cpp:1193-1199 at pin 113b5b6; `STUB: GAME` VA
/// 0x4a8670, RVA 0xa8670): the original body is an unwritten stub whose only
/// evidenced effect is returning `STATUS_DONE`, documented there as the
/// answer for a car without a chase. This marker-only contract reports that
/// answer; the car-to-chase lookup needs the chase owner and stays deferred.
pub fn sledovacka_action_status_no_chase() -> PursuitChaseStatus {
    PursuitChaseStatus::Done
}

/// Original `C_sledovacka::AddCarHunt` (mafia-re
/// game/ai/C_sledovacka.cpp:76-82 at pin 113b5b6; `STUB: GAME` VA
/// 0x4a0760, RVA 0xa0760): the original only logs and returns false. The
/// chase-list, driver and road owners stay outside this contract, so the
/// boundary is explicit.
pub fn sledovacka_add_car_hunt(
    _car: PoliceActorId,
    _target: PoliceActorId,
    _unknown: u32,
    _unknown2: u32,
    _pos: [f32; 3],
    _unknown3: f32,
) -> Result<bool, SledovackaError> {
    Err(SledovackaError::Unsupported {
        entry: SLEDOVACKA_ADD_CAR_HUNT_ENTRY,
    })
}

/// Original `C_sledovacka::AddCarMoveTo` (mafia-re
/// game/ai/C_sledovacka.cpp:84-89 at pin 113b5b6; `STUB: GAME` VA
/// 0x4a1220, RVA 0xa1220): the original only logs and returns false. The
/// chase-list, driver and road owners stay outside this contract, so the
/// boundary is explicit.
pub fn sledovacka_add_car_move_to(
    _car: PoliceActorId,
    _pos: [f32; 3],
    _unknown: bool,
    _unknown2: bool,
    _unknown3: u32,
) -> Result<bool, SledovackaError> {
    Err(SledovackaError::Unsupported {
        entry: SLEDOVACKA_ADD_CAR_MOVE_TO_ENTRY,
    })
}

/// Original `C_sledovacka::RemoveCar` (mafia-re
/// game/ai/C_sledovacka.cpp:91-95 at pin 113b5b6; `STUB: GAME` VA
/// 0x4a1c30, RVA 0xa1c30; `CAR_REMOVE_DRIVER`'s): the original only logs. The
/// chase-list and driver owners stay outside this contract, so the boundary
/// is explicit.
pub fn sledovacka_remove_car(_car: PoliceActorId) -> Result<(), SledovackaError> {
    Err(SledovackaError::Unsupported {
        entry: SLEDOVACKA_REMOVE_CAR_ENTRY,
    })
}

/// Original `C_sledovacka::RecompilePath` (mafia-re
/// game/ai/C_sledovacka.cpp:1173-1178 at pin 113b5b6; `STUB: GAME` VA
/// 0x4a1da0, RVA 0xa1da0; `Tick` calls it for a car whose path ran out): the
/// original only logs and returns false. The chase-list and road owners stay
/// outside this contract, so the boundary is explicit.
pub fn sledovacka_recompile_path(_index: u32) -> Result<bool, SledovackaError> {
    Err(SledovackaError::Unsupported {
        entry: SLEDOVACKA_RECOMPILE_PATH_ENTRY,
    })
}

/// Original `C_sledovacka::PreparePath` (mafia-re
/// game/ai/C_sledovacka.cpp:1180-1185 at pin 113b5b6; `STUB: GAME` VA
/// 0x4a28c0, RVA 0xa28c0; `Tick` calls it for a car whose path ran out): the
/// original only logs and returns false. The chase-list and road owners stay
/// outside this contract, so the boundary is explicit.
pub fn sledovacka_prepare_path(_index: u32) -> Result<bool, SledovackaError> {
    Err(SledovackaError::Unsupported {
        entry: SLEDOVACKA_PREPARE_PATH_ENTRY,
    })
}

/// Original `C_sledovacka::TickDriver` (mafia-re
/// game/ai/C_sledovacka.cpp:1187-1191 at pin 113b5b6; `STUB: GAME` VA
/// 0x4a2a80, RVA 0xa2a80; a chase's driving by its element index): the
/// original only logs. The chase-list and driver owners stay outside this
/// contract, so the boundary is explicit.
pub fn sledovacka_tick_driver(_index: u32, _time_ms: u32) -> Result<(), SledovackaError> {
    Err(SledovackaError::Unsupported {
        entry: SLEDOVACKA_TICK_DRIVER_ENTRY,
    })
}

/// Original `C_sledovacka::SetActionStatus` (mafia-re
/// game/ai/C_sledovacka.cpp:1201-1205 at pin 113b5b6; `STUB: GAME` VA
/// 0x4a86d0, RVA 0xa86d0): the original only logs. The chase-list owner stays
/// outside this contract, so the boundary is explicit.
pub fn sledovacka_set_action_status(
    _car: PoliceActorId,
    _status: PursuitChaseStatus,
) -> Result<(), SledovackaError> {
    Err(SledovackaError::Unsupported {
        entry: SLEDOVACKA_SET_ACTION_STATUS_ENTRY,
    })
}

/// Owned subset of `C_sledovacka` chase-manager state written by the
/// constructor 0x4a0320 and `Done` 0x4a0660. The full `S_BRIDGE` (actor plus
/// road node) and `S_ELEMENT` (car, driver, target, path) owners stay outside
/// this contract (mission actors, drivers, roads); only opaque actor
/// identities plus the road-net presence bit are tracked here. `Init`
/// 0x4a0360 needs the mission actors/roads owners and stays deferred behind
/// `sledovacka_init_bridges`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SledovackaState {
    road_net_set: bool,
    bridges: Vec<PoliceActorId>,
    chase_cars: Vec<PoliceActorId>,
}

impl SledovackaState {
    /// Original `C_sledovacka::C_sledovacka` 0x4a0320 (GAME VA 0x4a0320,
    /// RVA 0xa0320; mafia-re game/ai/C_sledovacka.cpp:29-33 at pin 113b5b6):
    /// `road_net(0)` with empty bridge/chase lists.
    pub fn new() -> Self {
        Self {
            road_net_set: false,
            bridges: Vec::new(),
            chase_cars: Vec::new(),
        }
    }

    pub fn road_net_set(&self) -> bool {
        self.road_net_set
    }

    pub fn bridge_count(&self) -> usize {
        self.bridges.len()
    }

    pub fn chase_car_count(&self) -> usize {
        self.chase_cars.len()
    }

    /// Chase-membership test at GAME VA 0x4a0700 (RVA 0xa0700): Ghidra
    /// procedure with a 37-instruction body (REA ev_2778375e) dividing the
    /// chase-element span by `S_ELEMENT` (0x44 bytes, mafia-re
    /// game/ai/C_sledovacka.h:38-55) and scanning back to front for the car,
    /// reporting whether one element drives it. Working name: no upstream
    /// symbol is evidenced for this outline; the tracked `chase_cars` stand
    /// in for the native elements, whose drivers/paths stay outside this
    /// contract.
    pub fn sledovacka_has_car(&self, car: PoliceActorId) -> bool {
        self.chase_cars.iter().rev().any(|&chase| chase == car)
    }

    /// Original `C_sledovacka::Done` 0x4a0660 (GAME VA 0x4a0660,
    /// RVA 0xa0660; mafia-re game/ai/C_sledovacka.cpp:67-74 at pin 113b5b6):
    /// deletes each element's driver, then clears the elements and the
    /// bridges. Driver deletion needs the driver owner and stays deferred;
    /// this clears the tracked lists and the road-net bit.
    pub fn apply_done(&mut self) {
        self.bridges.clear();
        self.chase_cars.clear();
        self.road_net_set = false;
    }
}

/// Original `C_sledovacka::Init` 0x4a0360 (GAME VA 0x4a0360, RVA 0xa0360;
/// mafia-re game/ai/C_sledovacka.cpp:44-64 at pin 113b5b6): collects the
/// mission's bridge actors with the nearer road node of the link
/// `GetNearNode` finds. The mission actors/roads owners stay outside this
/// contract, so the boundary is explicit.
pub fn sledovacka_init_bridges() -> Result<(), SledovackaError> {
    Err(SledovackaError::Unsupported {
        entry: SLEDOVACKA_INIT_ENTRY,
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

/// Record state raise at GAME VA 0x49f7e0 (RVA 0x9f7e0): Ghidra procedure
/// with a 47-instruction body (REA ev_7b59b75a) scanning the records back to
/// front for the human and, where that record's angry list is non-empty with
/// a free slot, raising `state` to 2 (`SHVSTATE_ARREST`), or to 3
/// (`SHVSTATE_KILL`) for `use_kill`, where lower. The caller keeps each
/// record's angry list; `angry` runs index-aligned with `records`, and pairs
/// past the shared prefix stay unreachable (a host admission the native
/// inline vector cannot hit). Working name: no upstream symbol is evidenced
/// for this outline.
pub fn raise_record_state_for_human(
    records: &mut [PolicePursuitRecord],
    angry: &[Vec<PoliceActorId>],
    human: PoliceHumanId,
    use_kill: bool,
) {
    let target = 2 + u32::from(use_kill);
    let shared = records.len().min(angry.len());
    for index in (0..shared).rev() {
        if records[index].human != human {
            continue;
        }
        let angry_list = &angry[index];
        if angry_list.is_empty() || angry_list.capacity() <= angry_list.len() {
            return;
        }
        if records[index].state < target {
            records[index].state = target;
        }
        return;
    }
}

/// Original `C_shvestky::S_shvestky::~S_shvestky` 0x49a120 (GAME VA 0x49a120,
/// RVA 0x9a120; mafia-re game/ai/C_shvestky.cpp:141-145 at pin 113b5b6 is a
/// log-only stub): destroys one pursuit record, whose owned subset here is
/// its caller-owned angry list. Record storage itself is Rust-owned, so this
/// clears that list only.
pub fn drop_record_angry_list(list: &mut Vec<PoliceActorId>) {
    list.clear();
}

/// Typed pursuit record cleared by `ReleaseWantedHuman` 0x49f850. Field
/// order follows `C_shvestky.h`; unknown native words stay typed scalars
/// without invented gameplay meanings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicePursuitRecord {
    pub human: PoliceHumanId,
    /// The human's car at `AddPerson` time when it reads as `ACTOR_CAR`
    /// (`None` for the native null otherwise); `DriveCar` owns later updates.
    pub car: Option<PoliceActorId>,
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
    /// The start of `wanted_time` (`wanted_time_start` at +0x48, mafia-re
    /// game/ai/C_shvestky.h:88): the wanted-clock refresh stores the fresh
    /// span here; the HUD bar reads it back. Cleared with the record.
    pub wanted_time_start: i32,
    pub hits_time: i32,
    pub hits: u32,
    pub run_over: u32,
    pub alerted: bool,
    pub nevi: bool,
    /// The human's known car (`known_car`, mafia-re game/ai/C_shvestky.h:99):
    /// the car `ZalozSpisATahniDoRiti_Haleluja` filed the human in. Native
    /// null reads as `None`.
    pub known_car: Option<PoliceActorId>,
    /// Whether the filed human is known on foot (`known_on_foot`, mafia-re
    /// game/ai/C_shvestky.h:100).
    pub known_on_foot: bool,
    pub support_level: u32,
    pub rammed_car: u32,
    /// Why the record's state went up (`reason`, `SHVREASON_*`, mafia-re
    /// game/ai/C_shvestky.h:39-45). Upstream leaves 8 (written by the
    /// signal-mark outline) unnamed; the raw word is kept.
    pub reason: u32,
}

impl PolicePursuitRecord {
    pub fn new(human: PoliceHumanId) -> Self {
        Self {
            human,
            car: None,
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
            wanted_time_start: 0,
            hits_time: -1,
            hits: 0,
            run_over: 0,
            alerted: false,
            nevi: false,
            known_car: None,
            known_on_foot: false,
            support_level: 0,
            rammed_car: 0,
            reason: 0,
        }
    }

    /// Record effect of `C_shvestky::Firing` 0x49cb20 (mafia-re
    /// game/ai/C_shvestky.cpp:1012-1016 is a log-only stub; effect per
    /// game/ai/C_shvestky.h:194-195: `SHVF_FIRE` set, `fire_time`). The
    /// duration value itself is caller-owned: no duration is evidenced
    /// upstream (`C_human::Shooting` at game/actors/C_human.cpp:7298, 7403
    /// passes none), so the caller supplies the latched `fire_time`.
    pub fn record_firing(&mut self, fire_time: u32) {
        self.flags |= SHVF_FIRE;
        self.fire_time = fire_time;
    }

    /// Record effect of `C_shvestky::HumanBoxing` 0x49d030 (mafia-re
    /// game/ai/C_shvestky.cpp:993-997 is a log-only stub; effect per
    /// game/ai/C_shvestky.h:70, 205-206: `SHVF_BOXING` set, `boxing_time`
    /// 5000 more).
    pub fn record_boxing(&mut self) {
        self.flags |= SHVF_BOXING;
        self.boxing_time = self.boxing_time.wrapping_add(5000);
    }

    /// Record effect of `C_shvestky::ForceArrest` 0x49d130 (mafia-re
    /// game/ai/C_shvestky.cpp:770-774 is a log-only stub; effect per
    /// game/ai/C_shvestky.h:28 and the `SHVF_FORCE_ARREST` arm of
    /// `CreateShvestkyTest` at game/ai/C_shvestky.cpp:561-565): flag set,
    /// `new_state` raised to `SHVSTATE_ARREST`. The script `a`/`b` parameters
    /// need mission owners and stay deferred.
    pub fn record_force_arrest(&mut self) {
        self.flags |= SHVF_FORCE_ARREST;
        if self.new_state < SHVSTATE_ARREST {
            self.new_state = SHVSTATE_ARREST;
        }
    }

    /// Record effect of `C_shvestky::NextNaSrackySoudruh` 0x49d220 (mafia-re
    /// game/ai/C_shvestky.cpp:988-991 is a log-only stub; effect per
    /// game/ai/C_shvestky.h:106-108: the driver's `run_over` count, 6 making
    /// a `CarCollision` hit of a human `SHVSTATE_KILL`). Counts one more
    /// run-over; at six raises `state` (and `new_state`, which
    /// `CreateShvestkyTest` at game/ai/C_shvestky.cpp:640-663 would promote)
    /// to `SHVSTATE_KILL`. Exact hit arbitration inside `CarCollision` stays
    /// deferred with the game/actor owners; this is the marker-only contract.
    pub fn record_run_over(&mut self) {
        self.run_over = self.run_over.wrapping_add(1);
        if self.run_over >= 6 {
            self.state = SHVSTATE_KILL;
            if self.new_state < SHVSTATE_KILL {
                self.new_state = SHVSTATE_KILL;
            }
        }
    }

    /// Original `C_shvestky::JakPicusNaTomJest` 0x49d390 record read
    /// (mafia-re game/ai/C_shvestky.cpp:784-788 is a log-only stub; read per
    /// game/ai/C_shvestky.h:211: the record's `state`, 0 without a record).
    /// The human-to-record lookup needs the game owner and stays deferred;
    /// this is the marker-only contract over the owned record.
    pub fn status_word(&self) -> u32 {
        self.state
    }

    /// Original `C_shvestky::GetCrashTime` 0x49d3e0 record read (mafia-re
    /// game/ai/C_shvestky.cpp:791-795 is a log-only stub; read per
    /// game/ai/C_shvestky.h:69, 214: the record's `crash_time`).
    pub fn crash_time(&self) -> u32 {
        self.crash_time
    }

    /// Original `C_shvestky::GetFireTime` 0x49d420 record read (mafia-re
    /// game/ai/C_shvestky.cpp:798-802 is a log-only stub; read per
    /// game/ai/C_shvestky.h:66, 215: the record's `fire_time`, paired with
    /// `record_firing`).
    pub fn fire_time(&self) -> u32 {
        self.fire_time
    }

    /// Original `C_shvestky::PocetPrestupku` 0x49d520 record read (mafia-re
    /// game/ai/C_shvestky.cpp:1000-1004 is a log-only stub; read per
    /// game/ai/C_shvestky.h:74-75, 213: the record's `offenses` count).
    pub fn offenses(&self) -> u32 {
        self.offenses
    }

    /// Original `C_shvestky::NeviDoKterePiceVlezl` 0x49d560 record read
    /// (mafia-re game/ai/C_shvestky.cpp:819-823 is a log-only stub; read per
    /// game/ai/C_shvestky.h:94, 218-220: the record's `nevi` flag).
    pub fn is_nevi(&self) -> bool {
        self.nevi
    }

    /// Record effect of `C_shvestky::DriveCar` 0x49cbb0 (GAME VA 0x49cbb0,
    /// RVA 0x9cbb0; mafia-re game/ai/C_shvestky.cpp:764-768 at pin 113b5b6
    /// is a log-only stub): the record's `car` follows the human's car
    /// (`C_human` calls it with the entered car and with null on exit).
    /// The human-to-record lookup and the `ACTOR_CAR` type read need the
    /// game/actor owners and stay deferred; this is the marker-only
    /// contract over the owned record.
    pub fn apply_drive_car(&mut self, car: Option<PoliceActorId>) {
        self.car = car;
    }

    /// Record effect of `C_shvestky::CarSteal` 0x49d0b0 (GAME VA 0x49d0b0,
    /// RVA 0x9d0b0; mafia-re game/ai/C_shvestky.cpp:981-985 at pin 113b5b6
    /// is a log-only stub; effect per game/ai/C_shvestky.h:24-25, 65:
    /// `SHVF_CAR_STEAL` set, `car_steal_time` 5000, cleared once the time
    /// runs out). The native `bool a, bool b` pair has no evidenced meaning
    /// (header notes the meanings unknown) and stays deferred, as does the
    /// human-to-record lookup with the game owner.
    pub fn record_car_steal(&mut self) {
        self.flags |= SHVF_CAR_STEAL;
        self.car_steal_time = 5000;
    }

    /// Original `C_shvestky::GetHumanStatus` 0x49d340 (GAME VA 0x49d340,
    /// RVA 0x9d340; mafia-re game/ai/C_shvestky.cpp:776-781 at pin 113b5b6
    /// is a log-only stub returning zero): the `GET_PM` script command
    /// `CMD_GET_PM_STATE` tests `p[2] & GetHumanStatus(...)` (mafia-re
    /// game/mission/C_program.cpp:5262-5273), i.e. the record's status flag
    /// bits. The human-to-record lookup needs the game owner and stays
    /// deferred; this is the marker-only contract over the owned record.
    /// Whether the native word also folds `flags2` into the high half is
    /// unresolved (stub body, no word evidence) and stays deferred.
    pub fn human_status(&self) -> u32 {
        self.flags
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF_WEAPON` bit (mafia-re game/ai/C_shvestky.h:16). The native flag
    /// is maintained inline in `Tick`/`CreateShvestkyTest` with no dedicated
    /// getter, so no RVA is claimed.
    pub fn has_weapon(&self) -> bool {
        self.flags & SHVF_WEAPON != 0
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF_SPEEDING` bit (mafia-re game/ai/C_shvestky.h:18, car faster
    /// than `speed_limit`). Inline native use only, so no RVA is claimed.
    pub fn is_speeding(&self) -> bool {
        self.flags & SHVF_SPEEDING != 0
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF_CRASH` bits (mafia-re game/ai/C_shvestky.h:19-20). Inline
    /// native use only, so no RVA is claimed.
    pub fn has_crash(&self) -> bool {
        self.flags & SHVF_CRASH != 0
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF_RED_LIGHT` bit (mafia-re game/ai/C_shvestky.h:23). Inline
    /// native use only, so no RVA is claimed.
    pub fn has_red_light(&self) -> bool {
        self.flags & SHVF_RED_LIGHT != 0
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF_CAR_STEAL` bit (mafia-re game/ai/C_shvestky.h:24-25, paired
    /// with `record_car_steal`). Inline native use only, so no RVA is
    /// claimed.
    pub fn has_car_steal(&self) -> bool {
        self.flags & SHVF_CAR_STEAL != 0
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF_FORCE_ARREST` bit (mafia-re game/ai/C_shvestky.h:28, paired
    /// with `record_force_arrest`). Inline native use only, so no RVA is
    /// claimed.
    pub fn has_force_arrest(&self) -> bool {
        self.flags & SHVF_FORCE_ARREST != 0
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF_RAM_POLICE_CAR` bit (mafia-re game/ai/C_shvestky.h:29-30).
    /// Inline native use only, so no RVA is claimed.
    pub fn has_ram_police_car(&self) -> bool {
        self.flags & SHVF_RAM_POLICE_CAR != 0
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `wanted_time` latch. The native reads it inline in `Tick` 0x49a300
    /// (HUD bar, `DobreMiPokuril_MuzeJitDom` countdown) with no dedicated
    /// function, so no RVA is claimed.
    pub fn wanted_time(&self) -> i32 {
        self.wanted_time
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `wanted_time_start` word. The native stores the fresh span here on the
    /// wanted-clock refresh and reads it back for the HUD bar inline with no
    /// dedicated function, so no RVA is claimed.
    pub fn wanted_time_start(&self) -> i32 {
        self.wanted_time_start
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `signal_time` latch (`SHVF_RED_LIGHT`/`SHVF_SIGNAL_MARK`, cleared once
    /// the time runs out). The native touches it inline in `Tick` 0x49a300
    /// with no dedicated function, so no RVA is claimed.
    pub fn signal_time(&self) -> u32 {
        self.signal_time
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `car_steal_time` latch, paired with `record_car_steal`. The native
    /// expiry runs inline in `Tick` 0x49a300 with no dedicated function, so
    /// no RVA is claimed.
    pub fn car_steal_time(&self) -> u32 {
        self.car_steal_time
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `illegal_path_time` latch (`SHVF_ILLEGAL_PATH`, counted up while the
    /// car is faster than 11, at most 5000; `SHVREASON_ILLEGAL_PATH` past
    /// 5800). The native expiry runs inline in `Tick` 0x49a300 (mafia-re
    /// game/ai/C_shvestky.cpp:261-276 at pin 113b5b6) with no dedicated
    /// function, so no RVA is claimed.
    pub fn illegal_path_time(&self) -> u32 {
        self.illegal_path_time
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `collision_time` latch (`SHVF_CRASH`, the strongest `CarCollision`
    /// hit, at most 4000). The native expiry runs inline in `Tick` 0x49a300
    /// (mafia-re game/ai/C_shvestky.cpp:243-250 at pin 113b5b6) with no
    /// dedicated function, so no RVA is claimed.
    pub fn collision_time(&self) -> u32 {
        self.collision_time
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `hits_time` latch (the humans the car hit, forgotten once the time
    /// runs out). The native expiry runs inline in `Tick` 0x49a300 (mafia-re
    /// game/ai/C_shvestky.cpp:327-331 at pin 113b5b6) with no dedicated
    /// function, so no RVA is claimed.
    pub fn hits_time(&self) -> i32 {
        self.hits_time
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `hits` count (the humans the car hit below `SHVSTATE_KILL`).
    /// The native count is maintained inline in `Tick` 0x49a300 (mafia-re
    /// game/ai/C_shvestky.cpp:327-331 at pin 113b5b6) with no dedicated
    /// function, so no RVA is claimed.
    pub fn hits(&self) -> u32 {
        self.hits
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `block_way` flag (the police action is on the human once alerted;
    /// `Tick` 0x49a300 only raises road blocks for such records, mafia-re
    /// game/ai/C_shvestky.cpp:388 at pin 113b5b6). No dedicated function
    /// reads it natively, so no RVA is claimed.
    pub fn is_block_way(&self) -> bool {
        self.block_way
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `alerted` flag (`NabonzovalJsemHo`'s, also raised by `ForceArrest`).
    /// The native flag is read inline in `Tick` 0x49a300 and
    /// `CreateShvestkyTest` (mafia-re game/ai/C_shvestky.cpp at pin 113b5b6)
    /// with no dedicated function, so no RVA is claimed.
    pub fn is_alerted(&self) -> bool {
        self.alerted
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `support_level` (the more policemen the player's states 5 and 6
    /// brought, raised by `DalsiChalpiDoPartie` in `CreateShvestkyTest`,
    /// mafia-re game/ai/C_shvestky.cpp:721-736 at pin 113b5b6). No dedicated
    /// function reads it natively, so no RVA is claimed.
    pub fn support_level(&self) -> u32 {
        self.support_level
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `rammed_car` word (`StrcilJsemHoPoldumDoVyfuku`'s police car). The
    /// native word is read inline in `CreateShvestkyTest` (mafia-re
    /// game/ai/C_shvestky.cpp at pin 113b5b6) with no dedicated function, so
    /// no RVA is claimed.
    pub fn rammed_car(&self) -> u32 {
        self.rammed_car
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `new_state` latch, which `CreateShvestkyTest` raises from the flag
    /// arms and then promotes into `state` (mafia-re
    /// game/ai/C_shvestky.cpp:640-663 at pin 113b5b6). The companion
    /// `status_word` reads `state`; no dedicated function reads `new_state`
    /// natively, so no RVA is claimed.
    pub fn pending_state(&self) -> u32 {
        self.new_state
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `human` identity (`S_shvestky::human` at +0x0, mafia-re
    /// game/ai/C_shvestky.h:61). The native lookup by human runs inline in
    /// each `C_shvestky` effect with no dedicated getter, so no RVA is
    /// claimed.
    pub fn human(&self) -> PoliceHumanId {
        self.human
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `car` latch (`S_shvestky::car` at +0x4, mafia-re
    /// game/ai/C_shvestky.h:62; `DriveCar`/`AddPerson` owner). The native
    /// field is written by `DriveCar` 0x49cbb0 and read inline in `Tick`
    /// 0x49a300 with no dedicated getter, so no RVA is claimed. `None` is
    /// the native null.
    pub fn car(&self) -> Option<PoliceActorId> {
        self.car
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `flags2` word (`S_shvestky::flags2` at +0xa, `SHVF2_*`, mafia-re
    /// game/ai/C_shvestky.h:30-32, 64). The native word is maintained
    /// inline in `Tick` 0x49a300 with no dedicated getter, so no RVA is
    /// claimed.
    pub fn flags2(&self) -> u32 {
        self.flags2
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `boxing_time` latch (`S_shvestky::boxing_time` at +0x20, paired with
    /// `record_boxing`, mafia-re game/ai/C_shvestky.h:70). The native expiry
    /// runs inline in `Tick` 0x49a300 with no dedicated getter, so no RVA is
    /// claimed.
    pub fn boxing_time(&self) -> u32 {
        self.boxing_time
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `known_car` latch (`S_shvestky::known_car` at +0x54, mafia-re
    /// game/ai/C_shvestky.h:96-100; filed by `ZalozSpisATahniDoRiti_Haleluja`).
    /// The native field is compared inline in `CreateShvestkyTest` with no
    /// dedicated getter, so no RVA is claimed. `None` is the native null.
    pub fn known_car(&self) -> Option<PoliceActorId> {
        self.known_car
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `known_on_foot` flag (`S_shvestky::known_on_foot` at +0x58, mafia-re
    /// game/ai/C_shvestky.h:100). Same inline-only native use as
    /// `known_car`, so no RVA is claimed.
    pub fn known_on_foot(&self) -> bool {
        self.known_on_foot
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `reason` word (`S_shvestky::reason` at +0x64, `SHVREASON_*`, mafia-re
    /// game/ai/C_shvestky.h:39-45). The native word is written by the flag
    /// arms in `CreateShvestkyTest` with no dedicated getter, so no RVA is
    /// claimed.
    pub fn reason(&self) -> u32 {
        self.reason
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `run_over` count (`S_shvestky::run_over` at +0x68, mafia-re
    /// game/ai/C_shvestky.h:106-108; `NextNaSrackySoudruh` owner). The
    /// native count is maintained by that effect with no dedicated getter,
    /// so no RVA is claimed.
    pub fn run_over(&self) -> u32 {
        self.run_over
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF_FIRE` bit (mafia-re game/ai/C_shvestky.h:17, `Firing`'s bit,
    /// cleared once `fire_time` runs out). The native flag is maintained
    /// inline in `Tick` 0x49a300 with no dedicated getter, so no RVA is
    /// claimed.
    pub fn has_fire(&self) -> bool {
        self.flags & SHVF_FIRE != 0
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF_BOXING` bit (mafia-re game/ai/C_shvestky.h:22, `HumanBoxing`'s
    /// bit, cleared once `boxing_time` runs out). Inline native use only,
    /// so no RVA is claimed.
    pub fn has_boxing(&self) -> bool {
        self.flags & SHVF_BOXING != 0
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF_ILLEGAL_PATH` bit (mafia-re game/ai/C_shvestky.h:21,
    /// `CarIllegalPath`'s bit, counted up while the car is faster than 11).
    /// Inline native use only, so no RVA is claimed.
    pub fn has_illegal_path(&self) -> bool {
        self.flags & SHVF_ILLEGAL_PATH != 0
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF_SIGNAL_MARK` bit (mafia-re game/ai/C_shvestky.h:25, a destroyed
    /// traffic sign, cleared with `SHVF_RED_LIGHT`). Inline native use only,
    /// so no RVA is claimed.
    pub fn has_signal_mark(&self) -> bool {
        self.flags & SHVF_SIGNAL_MARK != 0
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF_HIT_POLICEMAN` bit (mafia-re game/ai/C_shvestky.h:26-27,
    /// `DalJsemPochuzkariDoDrzky`'s bit, `SHVSTATE_KILL`). Inline native use
    /// only, so no RVA is claimed.
    pub fn has_hit_policeman(&self) -> bool {
        self.flags & SHVF_HIT_POLICEMAN != 0
    }

    /// Working name (no separate native entry): reads the owned record's
    /// `SHVF2_CRASH` bits (mafia-re game/ai/C_shvestky.h:30-32, `flags2`
    /// crash bits, cleared once `crash_time` runs out). The native word is
    /// maintained inline in `Tick` 0x49a300 with no dedicated getter, so no
    /// RVA is claimed.
    pub fn has_flags2_crash(&self) -> bool {
        self.flags2 & SHVF2_CRASH != 0
    }

    /// Working name (no outline entry is claimed): the latch-decay slice of
    /// `Tick` 0x49a300 (mafia-re game/ai/C_shvestky.cpp:243-312, 327-332 at
    /// pin 113b5b6): counts each latch down by the records-timer period with
    /// the native wrapping subtract-then-signed-compare, clearing the
    /// matching bits at zero (`SHVF_CRASH`/`SHVF2_CRASH`, `SHVF_BOXING`,
    /// `SHVF_RED_LIGHT`/`SHVF_SIGNAL_MARK`, `SHVF_CAR_STEAL`, `SHVF_FIRE`).
    /// `car` carries the record car's probe (`None` for the native null):
    /// past 11 m/s the illegal-path time counts up (clamped at 5000, bit
    /// dropped where the car lost its illegal path), otherwise the bit
    /// clears and the time zeroes. `hits_time` only resets `hits` past zero
    /// (a zero rest keeps the count, as natively). The angry-list prune, the
    /// wanted clock and the state machine run around this slice in their own
    /// owners. The caller passes the records-timer period, the cadence the
    /// slice runs at.
    pub fn tick_decay_timers(&mut self, period_ms: u32, car: Option<IllegalPathCarProbe>) {
        if self.collision_time != 0 {
            let left = self.collision_time.wrapping_sub(period_ms);
            if left as i32 <= 0 {
                self.collision_time = 0;
                self.flags &= !SHVF_CRASH;
            } else {
                self.collision_time = left;
            }
        }
        if self.crash_time != 0 {
            let left = self.crash_time.wrapping_sub(period_ms);
            if left as i32 <= 0 {
                self.crash_time = 0;
                self.flags2 &= !SHVF2_CRASH;
            } else {
                self.crash_time = left;
            }
        }
        if self.flags & SHVF_ILLEGAL_PATH != 0 {
            match car {
                Some(probe) if probe.speed > 11.0 => {
                    self.illegal_path_time = self.illegal_path_time.wrapping_add(period_ms);
                    if !probe.illegal_path {
                        self.flags &= !SHVF_ILLEGAL_PATH;
                    }
                    if self.illegal_path_time > 5000 {
                        self.illegal_path_time = 5000;
                    }
                }
                _ => {
                    self.flags &= !SHVF_ILLEGAL_PATH;
                    self.illegal_path_time = 0;
                }
            }
        }
        if self.boxing_time != 0 {
            let left = self.boxing_time.wrapping_sub(period_ms);
            if left as i32 <= 0 {
                self.boxing_time = 0;
                self.flags &= !SHVF_BOXING;
            } else {
                self.boxing_time = left;
            }
        }
        if self.signal_time != 0 {
            let left = self.signal_time.wrapping_sub(period_ms);
            if left as i32 <= 0 {
                self.signal_time = 0;
                self.flags &= !(SHVF_RED_LIGHT | SHVF_SIGNAL_MARK);
            } else {
                self.signal_time = left;
            }
        }
        if self.car_steal_time != 0 {
            let left = self.car_steal_time.wrapping_sub(period_ms);
            if left as i32 <= 0 {
                self.car_steal_time = 0;
                self.flags &= !SHVF_CAR_STEAL;
            } else {
                self.car_steal_time = left;
            }
        }
        if self.fire_time != 0 {
            let left = self.fire_time.wrapping_sub(period_ms);
            if left as i32 <= 0 {
                self.fire_time = 0;
                self.flags &= !SHVF_FIRE;
            } else {
                self.fire_time = left;
            }
        }
        if self.hits_time > 0 {
            self.hits_time = self.hits_time.wrapping_sub(period_ms as i32);
            if self.hits_time < 0 {
                self.hits = 0;
            }
        }
    }

    /// Working name (no outline entry is claimed): the weapon scan of `Tick`
    /// 0x49a300 (mafia-re game/ai/C_shvestky.cpp:230-238 at pin 113b5b6):
    /// sets `SHVF_WEAPON` while the right-hand item reads 5-15
    /// (mafia-re game/ai/C_shvestky.h:46-52), else clears it. The
    /// `C_traffic_generator::PlayerHoMaVytaseneho` call for a flag set
    /// without a car needs the traffic owner and stays deferred; this keeps
    /// the owned flag store.
    pub fn apply_weapon_scan(&mut self, item_in_hand: i32) {
        if (5..=15).contains(&item_in_hand) {
            self.flags |= SHVF_WEAPON;
        } else {
            self.flags &= !SHVF_WEAPON;
        }
    }

    /// Working name (no outline entry is claimed): the speeding scan of
    /// `Tick` 0x49a300 (mafia-re game/ai/C_shvestky.cpp:239-242 at pin
    /// 113b5b6): sets `SHVF_SPEEDING` while the record's car (`None` for the
    /// native null) outruns `speed_limit`, else clears it. The caller owns
    /// the speed read and the `POLICEMANAGER_SETSPEED` km/h scaling.
    pub fn apply_speeding_scan(&mut self, car_speed: Option<f32>, speed_limit: f32) {
        if car_speed.is_some_and(|speed| speed > speed_limit) {
            self.flags |= SHVF_SPEEDING;
        } else {
            self.flags &= !SHVF_SPEEDING;
        }
    }

    /// Working name (no outline entry is claimed): the wanted-clock slice of
    /// `Tick` 0x49a300 (mafia-re game/ai/C_shvestky.cpp:333-369 at pin
    /// 113b5b6): past `SHVSTATE_NONE`, an empty angry list counts
    /// `wanted_time` down (kept at -1 while not alerted) and releases the
    /// human at zero, else clamps the fire/collision/boxing windows to 2000;
    /// an alerted record with company refreshes its span from the state
    /// (20000/40000/60000) into `wanted_time_start`. The release
    /// (`DobreMiPokuril_MuzeJitDom`) and the refresh
    /// (`VasekSelChlastatNaOslavu`, the player's `SetProgress(1.0)`) need
    /// mission/game/HUD owners and run behind the returned markers; this
    /// keeps the owned stores.
    pub fn update_wanted_clock(
        &mut self,
        period_ms: u32,
        angry_is_empty: bool,
        is_player: bool,
    ) -> WantedClockAction {
        if self.state == SHVSTATE_NONE {
            return WantedClockAction::Steady;
        }
        if angry_is_empty {
            self.wanted_time = self.wanted_time.wrapping_sub(period_ms as i32);
            if !self.alerted {
                self.wanted_time = -1;
            }
            if self.wanted_time < 0 {
                return WantedClockAction::Release;
            }
            if self.fire_time > 2000 {
                self.fire_time = 2000;
            }
            if self.collision_time > 2000 {
                self.collision_time = 2000;
            }
            if self.boxing_time > 2000 {
                self.boxing_time = 2000;
            }
            return WantedClockAction::Steady;
        }
        if self.alerted {
            self.wanted_time = match self.state {
                1 => 20000,
                SHVSTATE_ARREST => 40000,
                SHVSTATE_KILL => 60000,
                _ => self.wanted_time,
            };
            self.wanted_time_start = self.wanted_time;
            return WantedClockAction::RefreshSchedule {
                announce_full_bar: is_player,
            };
        }
        WantedClockAction::Steady
    }

    /// Working name (no outline entry is claimed): the speeding arm of the
    /// inlined `CreateShvestkyTest` (mafia-re game/ai/C_shvestky.cpp:542-560
    /// at pin 113b5b6): with `SHVF_SPEEDING` set and a car present, a speed
    /// past the arrest limit raises `new_state` to `SHVSTATE_ARREST`, else to
    /// 1; both store `SHVREASON_SPEEDING` (2), each only where lower. The
    /// caller owns the speed read.
    pub fn raise_pending_for_speeding(&mut self, car_speed: Option<f32>, speed_limit_arrest: f32) {
        if self.flags & SHVF_SPEEDING == 0 {
            return;
        }
        let Some(speed) = car_speed else {
            return;
        };
        if speed > speed_limit_arrest {
            if self.new_state < SHVSTATE_ARREST {
                self.new_state = SHVSTATE_ARREST;
                self.reason = 2;
            }
        } else if self.new_state < 1 {
            self.new_state = 1;
            self.reason = 2;
        }
    }

    /// Working name (no outline entry is claimed): the weapon arm of the
    /// inlined `CreateShvestkyTest` (mafia-re game/ai/C_shvestky.cpp:593-623
    /// at pin 113b5b6): with `new_state` below 1 and `SHVF_WEAPON` set, a
    /// human off foot takes `SHVSTATE_ARREST` (`SHVREASON_WEAPON`, 0); in a
    /// car of `ACTOR_CAR` type only an aimed hand with an in-range item does,
    /// 6-14 from a taken seat and 6-9 from the driver's seat
    /// (`SHVITEM_CAR_FIRST/LAST`, `SHVITEM_DRIVER_LAST`, mafia-re
    /// game/ai/C_shvestky.h:48-52). `in_car`/`car_is_actor_car`/`aimed`/`seat`
    /// are host-owned human reads.
    pub fn raise_pending_for_weapon(
        &mut self,
        in_car: bool,
        car_is_actor_car: bool,
        aimed: bool,
        seat: u32,
        item: i32,
    ) {
        if self.new_state >= 1 || self.flags & SHVF_WEAPON == 0 {
            return;
        }
        if in_car {
            if !car_is_actor_car || !aimed {
                return;
            }
            let last = if seat == 0 { 9 } else { 14 };
            if !(6..=last).contains(&item) {
                return;
            }
        }
        self.new_state = SHVSTATE_ARREST;
        self.reason = 0;
    }

    /// Working name (no outline entry is claimed): the car-steal arm of the
    /// inlined `CreateShvestkyTest` (mafia-re game/ai/C_shvestky.cpp:624-628
    /// at pin 113b5b6): with the bit set and `new_state` below arrest, takes
    /// arrest with `SHVREASON_CAR_STEAL` (7).
    pub fn raise_pending_for_car_steal(&mut self) {
        if self.flags & SHVF_CAR_STEAL != 0 && self.new_state < SHVSTATE_ARREST {
            self.new_state = SHVSTATE_ARREST;
            self.reason = 7;
        }
    }

    /// Working name (no outline entry is claimed): the illegal-path arm of
    /// the inlined `CreateShvestkyTest` (mafia-re game/ai/C_shvestky.cpp:629-634
    /// at pin 113b5b6): with the bit set, a time past 5800 and `new_state`
    /// below 1, takes 1 with `SHVREASON_ILLEGAL_PATH` (4).
    pub fn raise_pending_for_illegal_path(&mut self) {
        if self.flags & SHVF_ILLEGAL_PATH != 0
            && self.illegal_path_time > 5800
            && self.new_state < 1
        {
            self.new_state = 1;
            self.reason = 4;
        }
    }

    /// Working name (no outline entry is claimed): the force-arrest arm of
    /// the inlined `CreateShvestkyTest` (mafia-re game/ai/C_shvestky.cpp:561-565
    /// at pin 113b5b6): with the bit set and `new_state` below arrest, takes
    /// arrest (no reason store, as natively).
    pub fn raise_pending_for_force_arrest(&mut self) {
        if self.flags & SHVF_FORCE_ARREST != 0 && self.new_state < SHVSTATE_ARREST {
            self.new_state = SHVSTATE_ARREST;
        }
    }

    /// Working name (no outline entry is claimed): the patrolman-hit arm of
    /// the inlined `CreateShvestkyTest` (mafia-re game/ai/C_shvestky.cpp:566-573
    /// at pin 113b5b6): with the bit set and `new_state` below kill, takes
    /// kill with `SHVREASON_HIT_POLICEMAN` (9).
    pub fn raise_pending_for_hit_policeman(&mut self) {
        if self.flags & SHVF_HIT_POLICEMAN != 0 && self.new_state < SHVSTATE_KILL {
            self.new_state = SHVSTATE_KILL;
            self.reason = 9;
        }
    }

    /// Working name (no outline entry is claimed): the police-ram arm of the
    /// inlined `CreateShvestkyTest` (mafia-re game/ai/C_shvestky.cpp:574-592
    /// at pin 113b5b6): with the bit set, a hard ram (above 9.7) takes
    /// arrest, else 1, both with `SHVREASON_RAM_POLICE_CAR` (0xa), each only
    /// where lower. `hard_ram` carries the caller's ram-speed reading (the
    /// native `S_shvestky::hard_ram` latch, mafia-re game/ai/C_shvestky.h:109,
    /// stays host-owned).
    pub fn raise_pending_for_ram_police_car(&mut self, hard_ram: bool) {
        if self.flags & SHVF_RAM_POLICE_CAR == 0 {
            return;
        }
        if hard_ram {
            if self.new_state < SHVSTATE_ARREST {
                self.new_state = SHVSTATE_ARREST;
                self.reason = 0xa;
            }
        } else if self.new_state < 1 {
            self.new_state = 1;
            self.reason = 0xa;
        }
    }
}

/// Original `C_shvestky::PocetNasranych` 0x49d460 (mafia-re
/// game/ai/C_shvestky.cpp:805-809 is a log-only stub; count per
/// game/ai/C_shvestky.h:111-113, 216: the length of the human's `nasrany`
/// list). The human-to-record lookup needs the game owner and stays
/// deferred; this is the marker-only contract over the caller-owned list.
pub fn angry_count(angry: &[PoliceActorId]) -> usize {
    angry.len()
}

/// Original `C_shvestky::PocetNasranychZivych` 0x49d4b0 (mafia-re
/// game/ai/C_shvestky.cpp:812-816 is a log-only stub; count per
/// game/ai/C_shvestky.h:111-113, 217: the `nasrany` actors that are not dead
/// via `C_actor::GetDead`). The host owns actors and supplies the deadness
/// probe; the human-to-record lookup stays deferred with the game owner.
pub fn angry_alive_count(
    angry: &[PoliceActorId],
    is_dead: impl Fn(PoliceActorId) -> bool,
) -> usize {
    angry.iter().copied().filter(|&actor| !is_dead(actor)).count()
}

/// Working name (no outline entry is claimed): the add half of one record's
/// angry list (PS2 `AddNasrany`, mafia-re game/ai/C_shvestky.h:111-113; the
/// PC keeps only the `DelNasrany` outline 0x49f540, so no add RVA is
/// claimed): appends the actor unless already listed. Returns whether the
/// actor was appended; mirrors the `admit_do_rici` duplicate guard.
pub fn admit_angry_actor(list: &mut Vec<PoliceActorId>, actor: PoliceActorId) -> bool {
    if list.contains(&actor) {
        return false;
    }
    list.push(actor);
    true
}

/// Working name (no outline entry is claimed): the dead-prune of one
/// record's angry list inside `Tick` 0x49a300 (mafia-re
/// game/ai/C_shvestky.cpp:314-326 at pin 113b5b6): drops the actors the host
/// reports dead (`C_actor::GetDead`) with swap-with-back + pop in place.
/// The host owns actors and supplies the deadness probe.
pub fn retain_living_angry(
    list: &mut Vec<PoliceActorId>,
    is_dead: impl Fn(PoliceActorId) -> bool,
) {
    let mut index = 0;
    while index < list.len() {
        if is_dead(list[index]) {
            list.swap_remove(index);
        } else {
            index += 1;
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
    fn rva_9e530_count_free_road_block_enemies_counts_back_to_front() {
        // GAME VA 0x49e530 (RVA 0x9e530): counts nonzero `free` marks.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.count_free_road_block_enemies(), 0);
        state.road_block_enemies.push(PoliceRoadBlockEnemy {
            actor: 1,
            free: true,
        });
        state.road_block_enemies.push(PoliceRoadBlockEnemy {
            actor: 2,
            free: false,
        });
        state.road_block_enemies.push(PoliceRoadBlockEnemy {
            actor: 3,
            free: true,
        });
        assert_eq!(state.count_free_road_block_enemies(), 2);
    }

    #[test]
    fn rva_9e560_take_free_road_block_iris_takes_last_free() {
        // GAME VA 0x49e560 (RVA 0x9e560): backward scan, marks used.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.take_free_road_block_iris(), None);
        for (actor, free) in [(1, false), (2, true), (3, true)] {
            state.road_block_irises.push(PoliceRoadBlockIris { actor, free });
        }
        assert_eq!(state.take_free_road_block_iris(), Some(3));
        assert!(!state.road_block_irises[2].free);
        assert_eq!(state.take_free_road_block_iris(), Some(2));
        assert_eq!(state.take_free_road_block_iris(), None);
    }

    #[test]
    fn rva_9e610_take_random_free_road_block_enemy_skips_pick() {
        // GAME VA 0x49e610 (RVA 0x9e610): `C_actor::rnd` 0x408450 pick over
        // the free count, second backward scan skips that many free entries.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.take_random_free_road_block_enemy(|_| 0), None);
        for (actor, free) in [(1, true), (2, false), (3, true), (4, true)] {
            state.road_block_enemies.push(PoliceRoadBlockEnemy { actor, free });
        }
        assert_eq!(state.take_random_free_road_block_enemy(|_| 0), Some(4));
        assert_eq!(state.take_random_free_road_block_enemy(|_| 1), Some(1));
        assert!(!state.road_block_enemies[0].free);
        // A pick past the free end takes nothing (native falls through).
        assert_eq!(state.take_random_free_road_block_enemy(|count| count), None);
    }

    #[test]
    fn rva_a0700_sledovacka_has_car_matches_chase_list() {
        // GAME VA 0x4a0700 (RVA 0xa0700): element search over the chase list.
        let mut chases = SledovackaState::new();
        assert!(!chases.sledovacka_has_car(7));
        chases.chase_cars.push(7);
        chases.chase_cars.push(9);
        assert!(chases.sledovacka_has_car(9));
        assert!(chases.sledovacka_has_car(7));
        assert!(!chases.sledovacka_has_car(8));
    }

    #[test]
    fn rva_9d5b0_human_record_alerted_reads_matched_record() {
        // GAME VA 0x49d5b0 (RVA 0x9d5b0): backward scan, `alerted` or false.
        let mut state = PoliceGameInitState::default();
        assert!(!state.human_record_alerted(7));
        let mut first = PolicePursuitRecord::new(7);
        first.alerted = false;
        let mut second = PolicePursuitRecord::new(9);
        second.alerted = true;
        state.records.push(first);
        state.records.push(second);
        assert!(!state.human_record_alerted(7));
        assert!(state.human_record_alerted(9));
        state.records[1].alerted = false;
        assert!(!state.human_record_alerted(9));
    }

    #[test]
    fn rva_9d2e0_block_way_arms_only_alerted_kill_records() {
        // GAME VA 0x49d2e0 (RVA 0x9d2e0): `SerSeMuDoCesty` outline.
        let mut state = PoliceGameInitState::default();
        let mut record = PolicePursuitRecord::new(7);
        record.alerted = true;
        record.state = SHVSTATE_KILL;
        state.records.push(record);
        // Inactive manager stores nothing (native active guard).
        state.apply_block_way_for_alerted_kill(7);
        assert!(!state.records[0].block_way);
        state.active = true;
        state.apply_block_way_for_alerted_kill(99);
        assert!(!state.records[0].block_way);
        state.apply_block_way_for_alerted_kill(7);
        assert!(state.records[0].block_way);
        // Arrest state (2) does not arm (`JBE` over the state word).
        let mut low = PolicePursuitRecord::new(8);
        low.alerted = true;
        low.state = SHVSTATE_ARREST;
        state.records.push(low);
        state.apply_block_way_for_alerted_kill(8);
        assert!(!state.records[1].block_way);
    }

    #[test]
    fn rva_9cfd0_car_illegal_path_sets_flag_and_clears_timer() {
        // GAME VA 0x49cfd0 (RVA 0x9cfd0): `CarIllegalPath` outline.
        let mut state = PoliceGameInitState::default();
        state.active = true;
        let mut record = PolicePursuitRecord::new(7);
        record.car = Some(11);
        record.illegal_path_time = 1200;
        state.records.push(record);
        state.apply_car_illegal_path(None);
        assert_eq!(state.records[0].flags & SHVF_ILLEGAL_PATH, 0);
        state.apply_car_illegal_path(Some(12));
        assert_eq!(state.records[0].flags & SHVF_ILLEGAL_PATH, 0);
        state.apply_car_illegal_path(Some(11));
        assert_ne!(state.records[0].flags & SHVF_ILLEGAL_PATH, 0);
        assert_eq!(state.records[0].illegal_path_time, 0);
        // A set bit keeps the running timer (no clear arm).
        state.records[0].illegal_path_time = 800;
        state.apply_car_illegal_path(Some(11));
        assert_eq!(state.records[0].illegal_path_time, 800);
    }

    #[test]
    fn rva_9d1a0_destroy_signal_mark_latches_time_and_reason() {
        // GAME VA 0x49d1a0 (RVA 0x9d1a0): `DestroySignalMark` outline.
        let mut state = PoliceGameInitState::default();
        let mut record = PolicePursuitRecord::new(7);
        record.signal_time = 15000;
        state.records.push(record);
        state.apply_destroy_signal_mark(7);
        assert_eq!(state.records[0].signal_time, 15000);
        state.active = true;
        state.apply_destroy_signal_mark(99);
        assert_eq!(state.records[0].signal_time, 15000);
        state.apply_destroy_signal_mark(7);
        assert_eq!(state.records[0].new_state, 1);
        assert_eq!(state.records[0].reason, 8);
        assert_eq!(state.records[0].signal_time, 19000);
        assert_ne!(state.records[0].flags & SHVF_SIGNAL_MARK, 0);
        // The 4000 ms raise clamps at 20000 and keeps a raised state.
        state.records[0].signal_time = 19000;
        state.records[0].new_state = 2;
        state.apply_destroy_signal_mark(7);
        assert_eq!(state.records[0].signal_time, 20000);
        assert_eq!(state.records[0].new_state, 2);
    }

    #[test]
    fn rva_a0040_hit_policeman_sets_flag_and_expires_timer() {
        // GAME VA 0x4a0040 (RVA 0xa0040): `DalJsemPochuzkariDoDrzky` outline.
        let mut state = PoliceGameInitState::default();
        state.records.push(PolicePursuitRecord::new(7));
        state.apply_hit_policeman(99);
        assert_eq!(state.records[0].flags & SHVF_HIT_POLICEMAN, 0);
        state.apply_hit_policeman(7);
        assert_ne!(state.records[0].flags & SHVF_HIT_POLICEMAN, 0);
        // `records_timer.time` climbs to `period` only where lower.
        assert_eq!(
            (state.records_timer.time_ms(), state.records_timer.period_ms()),
            (200, 200)
        );
        state.records_timer.reset(500);
        state.records_timer.tick(100);
        state.apply_hit_policeman(7);
        assert_eq!(state.records_timer.time_ms(), 500);
        // A non-positive signed difference leaves the timer alone.
        state.records_timer.time_ms = 600;
        state.apply_hit_policeman(7);
        assert_eq!(state.records_timer.time_ms(), 600);
    }

    #[test]
    fn rva_a0150_refresh_record_known_car_updates_matching_file() {
        // GAME VA 0x4a0150 (RVA 0xa0150): `ZalozSpisATahniDoRiti_Haleluja`
        // refresh; the caller reads `C_human::car` (+0x98).
        let mut state = PoliceGameInitState::default();
        let mut record = PolicePursuitRecord::new(7);
        record.known_car = Some(11);
        state.records.push(record);
        state.refresh_record_known_car(7, Some(11), true);
        assert_eq!(state.records[0].known_car, Some(11));
        state.active = true;
        state.refresh_record_known_car(7, Some(12), true);
        assert_eq!(state.records[0].known_car, Some(11));
        assert!(!state.records[0].known_on_foot);
        state.refresh_record_known_car(7, Some(11), true);
        assert_eq!(state.records[0].known_car, Some(11));
        assert!(state.records[0].known_on_foot);
        state.refresh_record_known_car(99, Some(11), false);
        assert!(state.records[0].known_on_foot);
    }

    #[test]
    fn rva_9f7e0_raise_record_state_needs_angry_room() {
        // GAME VA 0x49f7e0 (RVA 0x9f7e0): raises `state` to 2/3 where lower
        // with a non-empty angry list that still has a free slot.
        let mut records = vec![PolicePursuitRecord::new(7)];
        let mut angry = vec![Vec::<PoliceActorId>::with_capacity(4)];
        raise_record_state_for_human(&mut records, &angry, 99, false);
        assert_eq!(records[0].state, SHVSTATE_NONE);
        raise_record_state_for_human(&mut records, &angry, 7, false);
        assert_eq!(records[0].state, SHVSTATE_NONE);
        angry[0].push(3);
        raise_record_state_for_human(&mut records, &angry, 7, false);
        assert_eq!(records[0].state, SHVSTATE_ARREST);
        raise_record_state_for_human(&mut records, &angry, 7, true);
        assert_eq!(records[0].state, SHVSTATE_KILL);
        raise_record_state_for_human(&mut records, &angry, 7, false);
        assert_eq!(records[0].state, SHVSTATE_KILL);
    }

    #[test]
    fn rva_9e5a0_free_road_block_iris_reports_unsupported() {
        // GAME VA 0x49e5a0 (RVA 0x9e5a0): the game/radar notify owners stay
        // outside this contract.
        assert_eq!(FREE_ROAD_BLOCK_IRIS_ENTRY, 0x49e5a0);
        assert_eq!(
            free_road_block_iris(5),
            Err(PoliceDispatchError::Unsupported {
                entry: FREE_ROAD_BLOCK_IRIS_ENTRY
            })
        );
    }

    #[test]
    fn rva_a01c0_player_pursuit_active_reports_unsupported() {
        // GAME VA 0x4a01c0 (RVA 0xa01c0): the mission player owner stays
        // outside this contract.
        assert_eq!(PLAYER_PURSUIT_ACTIVE_ENTRY, 0x4a01c0);
        assert_eq!(
            player_pursuit_active(),
            Err(PoliceDispatchError::Unsupported {
                entry: PLAYER_PURSUIT_ACTIVE_ENTRY
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

    #[test]
    fn game_init_leaves_set_active_scalars_alone() {
        let mut state = PoliceGameInitState::default();
        assert!(state.apply_set_active(true, 980000, true));
        state.records.push(PolicePursuitRecord::new(3));
        state.apply_game_init();
        assert!(!state.active());
        assert_eq!(state.record_count(), 0);
        // Native GameInit never stores these; only SetActive does.
        assert_eq!(state.arrest_text(), 980000);
        assert!(!state.unknown_38());
        assert!(state.unknown_39());
    }

    #[test]
    fn set_active_same_state_stores_nothing() {
        let mut state = PoliceGameInitState::default();
        assert!(!state.apply_set_active(false, 980000, true));
        assert!(!state.active());
        assert_eq!(state.arrest_text(), 0);
        assert!(!state.unknown_39());
        assert!(state.apply_set_active(true, 111, false));
        assert!(!state.apply_set_active(true, 222, true));
        assert_eq!(state.arrest_text(), 111);
        assert!(!state.unknown_39());
    }

    #[test]
    fn set_active_flip_stores_flag_text_and_clears_unknown_38() {
        let mut state = PoliceGameInitState::default();
        assert!(state.apply_set_active(true, 980000, true));
        assert!(state.active());
        assert_eq!(state.arrest_text(), 980000);
        assert!(!state.unknown_38());
        assert!(state.unknown_39());
        assert!(state.apply_set_active(false, 42, false));
        assert!(!state.active());
        assert_eq!(state.arrest_text(), 42);
        assert!(!state.unknown_39());
    }

    #[test]
    fn add_person_guards_null_inactive_and_duplicates() {
        let mut state = PoliceGameInitState::default();
        assert!(!state.add_person(7, None));
        assert_eq!(state.record_count(), 0);
        assert!(state.apply_set_active(true, 980000, false));
        assert!(!state.add_person(0, None));
        assert_eq!(state.record_count(), 0);
        assert!(state.add_person(7, None));
        assert!(state.add_person(9, Some(51)));
        assert_eq!(state.record_count(), 2);
        assert!(!state.add_person(7, None));
        assert!(!state.add_person(9, None));
        assert_eq!(state.record_count(), 2);
        let cleared = &state.records[0];
        assert_eq!(cleared.human, 7);
        assert_eq!(cleared.car, None);
        assert_eq!(cleared.state, SHVSTATE_NONE);
        assert_eq!(cleared.new_state, SHVSTATE_NONE);
        assert_eq!(cleared.wanted_time, -1);
        assert_eq!(cleared.hits_time, -1);
        assert_eq!(state.records[1].car, Some(51));
    }

    #[test]
    fn del_person_guards_null_inactive_and_missing() {
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.del_person(7), None);
        assert!(state.apply_set_active(true, 980000, false));
        assert_eq!(state.del_person(0), None);
        assert_eq!(state.del_person(7), None);
        assert!(state.add_person(7, None));
        assert!(state.apply_set_active(false, 0, false));
        assert_eq!(state.del_person(7), None);
        assert_eq!(state.record_count(), 1);
    }

    #[test]
    fn del_person_removes_backmost_with_swap_with_back() {
        let mut state = PoliceGameInitState::default();
        assert!(state.apply_set_active(true, 980000, false));
        for human in [7, 9, 11, 13] {
            assert!(state.add_person(human, None));
        }
        // Middle removal moves the tail into the hole (not a shifting remove).
        let removed = state.del_person(9).expect("staged record");
        assert_eq!(removed.human, 9);
        assert_eq!(
            state.records.iter().map(|record| record.human).collect::<Vec<_>>(),
            vec![7, 13, 11]
        );
        // Manually staged duplicate with a car marker: the back-to-front
        // search removes the trailing copy, not the earlier one.
        let mut duplicate = PolicePursuitRecord::new(13);
        duplicate.car = Some(99);
        state.records.push(duplicate);
        let removed = state.del_person(13).expect("duplicate record");
        assert_eq!(removed.car, Some(99));
        assert_eq!(
            state.records.iter().map(|record| record.human).collect::<Vec<_>>(),
            vec![7, 13, 11]
        );
        assert_eq!(state.del_person(99), None);
        assert_eq!(state.record_count(), 3);
    }

    #[test]
    fn add_del_round_trip_restores_registry() {
        let mut state = PoliceGameInitState::default();
        assert!(state.apply_set_active(true, 980000, false));
        assert!(state.add_person(7, Some(60)));
        let removed = state.del_person(7).expect("added record");
        assert_eq!(removed.car, Some(60));
        assert_eq!(state.record_count(), 0);
        assert!(state.add_person(7, None));
        assert_eq!(state.records[0].car, None);
    }

    #[test]
    fn set_speed_stores_exact_limit_bits() {
        let mut state = PoliceGameInitState::default();
        state.apply_game_init();
        state.apply_set_speed(17.0 / 3.6, 28.0 / 3.6);
        assert_eq!(state.speed_limit(), 17.0 / 3.6);
        assert_eq!(state.speed_limit_arrest(), 28.0 / 3.6);
        assert_eq!(state.speed_limit_bits(), (17.0f32 / 3.6).to_bits());
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum HostCall {
        GlobalDelNasrany(PoliceActorId),
        RadarRemove(PoliceActorId),
        SledovackaRemove(PoliceActorId),
        TrafficRelease,
        Invalidate(PoliceActorId),
        RemoveTemporary(PoliceActorId),
        EmptySeat { car: PoliceActorId, seat: u32 },
    }

    struct FakeHost {
        marks: Vec<(PoliceActorId, DoRiciMark)>,
        calls: Vec<HostCall>,
    }

    impl FakeHost {
        fn with_marks(marks: Vec<(PoliceActorId, DoRiciMark)>) -> Self {
            Self {
                marks,
                calls: Vec::new(),
            }
        }
    }

    impl DoRiciHost for FakeHost {
        fn probe(&self, actor: PoliceActorId) -> Option<DoRiciMark> {
            self.marks
                .iter()
                .find(|(listed, _)| *listed == actor)
                .map(|(_, mark)| *mark)
        }

        fn global_del_nasrany(&mut self, actor: PoliceActorId) {
            self.calls.push(HostCall::GlobalDelNasrany(actor));
        }

        fn radar_remove_car(&mut self, car: PoliceActorId) {
            self.calls.push(HostCall::RadarRemove(car));
        }

        fn sledovacka_remove_car(&mut self, car: PoliceActorId) {
            self.calls.push(HostCall::SledovackaRemove(car));
        }

        fn traffic_car_release(&mut self) {
            self.calls.push(HostCall::TrafficRelease);
        }

        fn invalidate_actor(&mut self, actor: PoliceActorId) {
            self.calls.push(HostCall::Invalidate(actor));
        }

        fn remove_temporary_actor(&mut self, actor: PoliceActorId) {
            self.calls.push(HostCall::RemoveTemporary(actor));
        }

        fn empty_car_seat(&mut self, car: PoliceActorId, seat: u32) {
            self.calls.push(HostCall::EmptySeat { car, seat });
        }
    }

    fn base_mark(kind: DoRiciActorKind, pos: [f32; 3]) -> DoRiciMark {
        DoRiciMark {
            kind,
            pos,
            dead: false,
            human_type_is_4: false,
            dismissed: false,
            car: None,
            seat: 0,
            seats_occupied: false,
            referenced: false,
            frame_on: true,
            police_car: false,
        }
    }

    const CAMERA: [f32; 3] = [0.0, 0.0, 0.0];

    #[test]
    fn do_rici_admit_guards_duplicates() {
        let mut state = PoliceGameInitState::default();
        assert!(state.admit_do_rici(11));
        assert!(state.admit_do_rici(13));
        assert!(!state.admit_do_rici(11));
        assert_eq!(state.do_rici_actor_count(), 2);
    }

    #[test]
    fn do_rici_evict_swap_removes() {
        let mut state = PoliceGameInitState::default();
        assert!(state.admit_do_rici(11));
        assert!(state.admit_do_rici(13));
        assert!(state.admit_do_rici(17));
        assert!(state.evict_do_rici(13));
        assert_eq!(state.do_rici_actors, vec![11, 17]);
        assert!(!state.evict_do_rici(99));
        assert_eq!(state.do_rici_actor_count(), 2);
    }

    #[test]
    fn free_my_temp_actor_marks_road_block_entries() {
        let mut state = PoliceGameInitState::default();
        state.road_block_cars.push(PoliceRoadBlockCar {
            car: 5,
            free: false,
        });
        state.road_block_enemies.push(PoliceRoadBlockEnemy {
            actor: 6,
            free: false,
        });
        state.road_block_irises.push(PoliceRoadBlockIris {
            actor: 7,
            free: false,
        });
        assert!(state.free_my_temp_actor(5));
        assert!(state.road_block_cars[0].free);
        assert!(state.free_my_temp_actor(6));
        assert!(state.road_block_enemies[0].free);
        assert!(state.free_my_temp_actor(7));
        assert!(state.road_block_irises[0].free);
        assert!(!state.free_my_temp_actor(99));
    }

    #[test]
    fn free_my_temp_actor_releases_temp_car_tracking() {
        let mut state = PoliceGameInitState::default();
        state.temp_cars.extend([40, 41, 42]);
        assert!(state.free_my_temp_actor(41));
        assert_eq!(state.temp_cars(), &[40, 42]);
        assert!(!state.free_my_temp_actor(41));
    }

    #[test]
    fn tick_evicts_far_entity_without_car() {
        let mut state = PoliceGameInitState::default();
        assert!(state.admit_do_rici(11));
        // `y` is forced to zero: height must not save a far actor.
        let mut host = FakeHost::with_marks(vec![(
            11,
            base_mark(DoRiciActorKind::Entity, [200.0, 500.0, 0.0]),
        )]);
        state.tick_do_rici_actors(CAMERA, false, &mut host);
        assert_eq!(state.do_rici_actor_count(), 0);
        assert_eq!(
            host.calls,
            vec![
                HostCall::GlobalDelNasrany(11),
                HostCall::Invalidate(11),
                HostCall::Invalidate(11),
                HostCall::RemoveTemporary(11),
            ]
        );
    }

    #[test]
    fn tick_keeps_near_entity_and_boundary() {
        let mut state = PoliceGameInitState::default();
        assert!(state.admit_do_rici(11));
        assert!(state.admit_do_rici(12));
        let mut host = FakeHost::with_marks(vec![
            (11, base_mark(DoRiciActorKind::Entity, [10.0, 0.0, 0.0])),
            // Exactly 150 m is not past 22500, so it stays.
            (12, base_mark(DoRiciActorKind::Entity, [150.0, 0.0, 0.0])),
        ]);
        state.tick_do_rici_actors(CAMERA, false, &mut host);
        assert_eq!(state.do_rici_actors, vec![11, 12]);
        assert!(host.calls.is_empty());
    }

    #[test]
    fn tick_evicted_entity_in_far_car_empties_it_and_reruns() {
        let mut state = PoliceGameInitState::default();
        assert!(state.admit_do_rici(21));
        assert!(state.admit_do_rici(22));
        let mut entity = base_mark(DoRiciActorKind::Entity, [200.0, 0.0, 0.0]);
        entity.human_type_is_4 = true;
        entity.dismissed = true;
        entity.car = Some(22);
        entity.seat = 3;
        let mut car = base_mark(DoRiciActorKind::Car, [200.0, 0.0, 0.0]);
        car.police_car = true;
        let mut host = FakeHost::with_marks(vec![(21, entity), (22, car)]);
        state.tick_do_rici_actors(CAMERA, false, &mut host);
        assert_eq!(state.do_rici_actor_count(), 0);
        assert_eq!(
            host.calls,
            vec![
                HostCall::EmptySeat { car: 22, seat: 3 },
                HostCall::GlobalDelNasrany(21),
                HostCall::Invalidate(21),
                HostCall::Invalidate(21),
                HostCall::RemoveTemporary(21),
                HostCall::GlobalDelNasrany(22),
                HostCall::RadarRemove(22),
                HostCall::SledovackaRemove(22),
                HostCall::TrafficRelease,
                HostCall::Invalidate(22),
                HostCall::Invalidate(22),
                HostCall::RemoveTemporary(22),
            ]
        );
    }

    #[test]
    fn tick_dismissed_entity_in_near_car_is_only_forgotten() {
        let mut state = PoliceGameInitState::default();
        assert!(state.admit_do_rici(21));
        let mut entity = base_mark(DoRiciActorKind::Entity, [10.0, 0.0, 0.0]);
        entity.human_type_is_4 = true;
        entity.dismissed = true;
        entity.car = Some(22);
        let mut host = FakeHost::with_marks(vec![(21, entity)]);
        state.tick_do_rici_actors(CAMERA, false, &mut host);
        assert_eq!(state.do_rici_actors, vec![21]);
        assert_eq!(host.calls, vec![HostCall::GlobalDelNasrany(21)]);
    }

    #[test]
    fn tick_keeps_far_car_with_owner_or_reference() {
        let mut state = PoliceGameInitState::default();
        assert!(state.admit_do_rici(31));
        assert!(state.admit_do_rici(32));
        let mut seated = base_mark(DoRiciActorKind::Car, [200.0, 0.0, 0.0]);
        seated.seats_occupied = true;
        let mut held = base_mark(DoRiciActorKind::Car, [200.0, 0.0, 0.0]);
        held.referenced = true;
        let mut host = FakeHost::with_marks(vec![(31, seated), (32, held)]);
        state.tick_do_rici_actors(CAMERA, false, &mut host);
        assert_eq!(state.do_rici_actors, vec![31, 32]);
        assert!(host.calls.is_empty());
    }

    #[test]
    fn tick_other_kind_goes_at_once_without_car_effects() {
        let mut state = PoliceGameInitState::default();
        assert!(state.admit_do_rici(61));
        let mut host = FakeHost::with_marks(vec![(
            61,
            base_mark(DoRiciActorKind::Other, [10.0, 0.0, 0.0]),
        )]);
        state.tick_do_rici_actors(CAMERA, false, &mut host);
        assert_eq!(state.do_rici_actor_count(), 0);
        assert_eq!(
            host.calls,
            vec![
                HostCall::GlobalDelNasrany(61),
                HostCall::Invalidate(61),
                HostCall::Invalidate(61),
                HostCall::RemoveTemporary(61),
            ]
        );
    }

    #[test]
    fn tick_evicts_frame_off_car_near_camera() {
        let mut state = PoliceGameInitState::default();
        assert!(state.admit_do_rici(33));
        let mut car = base_mark(DoRiciActorKind::Car, [10.0, 0.0, 0.0]);
        car.frame_on = false;
        let mut host = FakeHost::with_marks(vec![(33, car)]);
        state.tick_do_rici_actors(CAMERA, false, &mut host);
        assert_eq!(state.do_rici_actor_count(), 0);
        assert_eq!(
            host.calls,
            vec![
                HostCall::GlobalDelNasrany(33),
                HostCall::RadarRemove(33),
                HostCall::SledovackaRemove(33),
                HostCall::Invalidate(33),
                HostCall::Invalidate(33),
                HostCall::RemoveTemporary(33),
            ]
        );
    }

    #[test]
    fn tick_skips_game_removal_for_owned_temp_actor() {
        let mut state = PoliceGameInitState::default();
        assert!(state.admit_do_rici(41));
        state.temp_cars.push(41);
        let mut host = FakeHost::with_marks(vec![(
            41,
            base_mark(DoRiciActorKind::Entity, [200.0, 0.0, 0.0]),
        )]);
        state.tick_do_rici_actors(CAMERA, false, &mut host);
        assert_eq!(state.do_rici_actor_count(), 0);
        assert!(state.temp_cars().is_empty());
        assert_eq!(
            host.calls,
            vec![
                HostCall::GlobalDelNasrany(41),
                HostCall::Invalidate(41),
            ]
        );
    }

    #[test]
    fn tick_all_evicts_everything_and_wipes_remainder() {
        let mut state = PoliceGameInitState::default();
        assert!(state.admit_do_rici(11));
        assert!(state.admit_do_rici(12));
        let mut host = FakeHost::with_marks(vec![(
            11,
            base_mark(DoRiciActorKind::Entity, [10.0, 0.0, 0.0]),
        )]);
        state.tick_do_rici_actors(CAMERA, true, &mut host);
        assert_eq!(state.do_rici_actor_count(), 0);
        // Actor 11 is near but `all` still evicts it; actor 12 has no mark
        // and goes at once without car effects.
        assert_eq!(
            host.calls,
            vec![
                HostCall::GlobalDelNasrany(11),
                HostCall::Invalidate(11),
                HostCall::Invalidate(11),
                HostCall::RemoveTemporary(11),
                HostCall::GlobalDelNasrany(12),
                HostCall::Invalidate(12),
                HostCall::Invalidate(12),
                HostCall::RemoveTemporary(12),
            ]
        );
    }

    #[test]
    fn tick_prefix_fires_do_rici_on_timer_and_trims_temp_cars_from_front() {
        let mut state = PoliceGameInitState::default();
        assert!(state.admit_do_rici(11));
        state.temp_cars.extend([50, 51, 52, 53, 54, 55, 56]);
        let mut host = FakeHost::with_marks(vec![(
            11,
            base_mark(DoRiciActorKind::Entity, [200.0, 0.0, 0.0]),
        )]);
        // Default 200 ms period: no do_rici pass yet, but the trim runs.
        state.police_tick_prefix(CAMERA, 0, &mut host);
        assert_eq!(state.do_rici_actors, vec![11]);
        assert_eq!(state.temp_cars(), &[52, 53, 54, 55, 56]);
        assert_eq!(
            host.calls,
            vec![
                HostCall::Invalidate(50),
                HostCall::RadarRemove(50),
                HostCall::RemoveTemporary(50),
                HostCall::Invalidate(51),
                HostCall::RadarRemove(51),
                HostCall::RemoveTemporary(51),
            ]
        );
        host.calls.clear();
        // Past the period the do_rici pass runs too.
        state.police_tick_prefix(CAMERA, 201, &mut host);
        assert_eq!(state.do_rici_actor_count(), 0);
        assert_eq!(
            host.calls,
            vec![
                HostCall::GlobalDelNasrany(11),
                HostCall::Invalidate(11),
                HostCall::Invalidate(11),
                HostCall::RemoveTemporary(11),
            ]
        );
    }

    #[test]
    fn record_firing_sets_flag_and_time() {
        let mut record = PolicePursuitRecord::new(7);
        record.record_firing(4000);
        assert_eq!(record.flags & SHVF_FIRE, SHVF_FIRE);
        assert_eq!(record.fire_time, 4000);
        record.record_firing(1500);
        assert_eq!(record.fire_time, 1500);
    }

    #[test]
    fn record_boxing_sets_flag_and_adds_time() {
        let mut record = PolicePursuitRecord::new(7);
        record.record_boxing();
        assert_eq!(record.flags & SHVF_BOXING, SHVF_BOXING);
        assert_eq!(record.boxing_time, 5000);
        record.record_boxing();
        assert_eq!(record.boxing_time, 10000);
    }

    #[test]
    fn record_force_arrest_sets_flag_and_raises_new_state() {
        let mut record = PolicePursuitRecord::new(7);
        record.record_force_arrest();
        assert_eq!(record.flags & SHVF_FORCE_ARREST, SHVF_FORCE_ARREST);
        assert_eq!(record.new_state, SHVSTATE_ARREST);
        assert_eq!(record.state, SHVSTATE_NONE);
        record.new_state = 3;
        record.record_force_arrest();
        assert_eq!(record.new_state, 3);
    }

    #[test]
    fn record_run_over_counts_and_raises_kill_at_six() {
        let mut record = PolicePursuitRecord::new(7);
        record.record_run_over();
        assert_eq!(record.run_over, 1);
        assert_eq!(record.state, SHVSTATE_NONE);
        assert_eq!(record.status_word(), SHVSTATE_NONE);
        record.run_over = 5;
        record.record_run_over();
        assert_eq!(record.run_over, 6);
        assert_eq!(record.state, SHVSTATE_KILL);
        assert_eq!(record.new_state, SHVSTATE_KILL);
        assert_eq!(record.status_word(), SHVSTATE_KILL);
    }

    #[test]
    fn record_status_crash_fire_offenses_nevi_read_fields() {
        let mut record = PolicePursuitRecord::new(7);
        record.state = SHVSTATE_ARREST;
        assert_eq!(record.status_word(), SHVSTATE_ARREST);
        record.crash_time = 1234;
        assert_eq!(record.crash_time(), 1234);
        record.record_firing(4000);
        assert_eq!(record.fire_time(), 4000);
        record.offenses = 2;
        assert_eq!(record.offenses(), 2);
        assert!(!record.is_nevi());
        record.nevi = true;
        assert!(record.is_nevi());
    }

    #[test]
    fn angry_counts_length_and_live_filter() {
        assert_eq!(angry_count(&[]), 0);
        assert_eq!(angry_count(&[11]), 1);
        assert_eq!(angry_count(&[11, 13, 17]), 3);
        let angry = vec![11u32, 13, 17];
        assert_eq!(angry_alive_count(&angry, |actor| actor == 13), 2);
        assert_eq!(angry_alive_count(&[], |_| true), 0);
        assert_eq!(angry_alive_count(&angry, |_| false), 3);
    }

    #[test]
    fn game_init_leaves_enemies_and_temp_cars_alone() {
        let mut state = PoliceGameInitState::default();
        state.road_block_enemies.push(PoliceRoadBlockEnemy {
            actor: 6,
            free: true,
        });
        state.temp_cars.push(40);
        state.apply_game_init();
        assert_eq!(state.road_block_enemy_count(), 1);
        assert_eq!(state.temp_car_count(), 1);
        assert_eq!(state.do_rici_actor_count(), 0);
    }

    #[test]
    fn rva_7b4e0_get_active_reads_registry_flag() {
        // C_shvestky::GetActive GAME VA 0x47b4e0 (RVA 0x7b4e0).
        let mut state = PoliceGameInitState::default();
        assert!(!state.active());
        assert!(state.apply_set_active(true, 980000, false));
        assert!(state.active());
        assert!(state.apply_set_active(false, 0, true));
        assert!(!state.active());
    }

    #[test]
    fn rva_a8670_action_status_no_chase_is_done() {
        // C_sledovacka::ActionStatus GAME VA 0x4a8670 (RVA 0xa8670).
        assert_eq!(SLEDOVACKA_ACTION_STATUS_ENTRY, 0x4a8670);
        assert_eq!(sledovacka_action_status_no_chase(), PursuitChaseStatus::Done);
        assert_eq!(PursuitChaseStatus::Done.word(), 1);
        assert_eq!(PursuitChaseStatus::Failed.word(), 2);
        assert_eq!(PursuitChaseStatus::DriverDead.word(), 3);
        assert_eq!(PursuitChaseStatus::NoEnergy.word(), 4);
    }

    #[test]
    fn rva_a0760_add_car_hunt_reports_unsupported() {
        // C_sledovacka::AddCarHunt GAME VA 0x4a0760 (RVA 0xa0760).
        assert_eq!(SLEDOVACKA_ADD_CAR_HUNT_ENTRY, 0x4a0760);
        assert_eq!(
            sledovacka_add_car_hunt(3, 7, 0, 0, [0.0; 3], 0.0),
            Err(SledovackaError::Unsupported { entry: 0x4a0760 })
        );
    }

    #[test]
    fn rva_a1220_add_car_move_to_reports_unsupported() {
        // C_sledovacka::AddCarMoveTo GAME VA 0x4a1220 (RVA 0xa1220).
        assert_eq!(SLEDOVACKA_ADD_CAR_MOVE_TO_ENTRY, 0x4a1220);
        assert_eq!(
            sledovacka_add_car_move_to(3, [1.0, 2.0, 3.0], false, true, 0),
            Err(SledovackaError::Unsupported { entry: 0x4a1220 })
        );
    }

    #[test]
    fn rva_a1c30_remove_car_reports_unsupported() {
        // C_sledovacka::RemoveCar GAME VA 0x4a1c30 (RVA 0xa1c30).
        assert_eq!(SLEDOVACKA_REMOVE_CAR_ENTRY, 0x4a1c30);
        assert_eq!(
            sledovacka_remove_car(3),
            Err(SledovackaError::Unsupported { entry: 0x4a1c30 })
        );
    }

    #[test]
    fn rva_a1da0_recompile_path_reports_unsupported() {
        // C_sledovacka::RecompilePath GAME VA 0x4a1da0 (RVA 0xa1da0).
        assert_eq!(SLEDOVACKA_RECOMPILE_PATH_ENTRY, 0x4a1da0);
        assert_eq!(
            sledovacka_recompile_path(2),
            Err(SledovackaError::Unsupported { entry: 0x4a1da0 })
        );
    }

    #[test]
    fn rva_a28c0_prepare_path_reports_unsupported() {
        // C_sledovacka::PreparePath GAME VA 0x4a28c0 (RVA 0xa28c0).
        assert_eq!(SLEDOVACKA_PREPARE_PATH_ENTRY, 0x4a28c0);
        assert_eq!(
            sledovacka_prepare_path(2),
            Err(SledovackaError::Unsupported { entry: 0x4a28c0 })
        );
    }

    #[test]
    fn rva_a2a80_tick_driver_reports_unsupported() {
        // C_sledovacka::TickDriver GAME VA 0x4a2a80 (RVA 0xa2a80).
        assert_eq!(SLEDOVACKA_TICK_DRIVER_ENTRY, 0x4a2a80);
        assert_eq!(
            sledovacka_tick_driver(2, 16),
            Err(SledovackaError::Unsupported { entry: 0x4a2a80 })
        );
    }

    #[test]
    fn rva_a86d0_set_action_status_reports_unsupported() {
        // C_sledovacka::SetActionStatus GAME VA 0x4a86d0 (RVA 0xa86d0).
        assert_eq!(SLEDOVACKA_SET_ACTION_STATUS_ENTRY, 0x4a86d0);
        assert_eq!(
            sledovacka_set_action_status(3, PursuitChaseStatus::Failed),
            Err(SledovackaError::Unsupported { entry: 0x4a86d0 })
        );
    }

    #[test]
    fn rva_99c00_dtor_empty_stores_nothing() {
        // C_shvestky::~C_shvestky GAME VA 0x499c00 (RVA 0x99c00): empty body.
        assert_eq!(SHVESTKY_DTOR_ENTRY, 0x499c00);
        let mut state = PoliceGameInitState::default();
        assert!(state.apply_set_active(true, 980000, true));
        let before = state.clone();
        state.apply_dtor_empty();
        assert_eq!(state, before);
    }

    #[test]
    fn rva_9a120_record_dtor_clears_angry_list() {
        // C_shvestky::S_shvestky::~S_shvestky GAME VA 0x49a120 (RVA 0x9a120).
        assert_eq!(RECORD_DTOR_ENTRY, 0x49a120);
        let mut list = vec![11u32, 13, 17];
        drop_record_angry_list(&mut list);
        assert!(list.is_empty());
        drop_record_angry_list(&mut list);
        assert!(list.is_empty());
    }

    #[test]
    fn rva_9cbb0_drive_car_sets_and_clears_car() {
        // C_shvestky::DriveCar GAME VA 0x49cbb0 (RVA 0x9cbb0).
        assert_eq!(DRIVE_CAR_ENTRY, 0x49cbb0);
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(record.car, None);
        record.apply_drive_car(Some(60));
        assert_eq!(record.car, Some(60));
        record.apply_drive_car(None);
        assert_eq!(record.car, None);
    }

    #[test]
    fn rva_9d340_human_status_reports_flags() {
        // C_shvestky::GetHumanStatus GAME VA 0x49d340 (RVA 0x9d340).
        assert_eq!(HUMAN_STATUS_ENTRY, 0x49d340);
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(record.human_status(), 0);
        record.record_firing(4000);
        assert_eq!(record.human_status(), SHVF_FIRE);
        assert_eq!(record.human_status() & SHVF_FIRE, SHVF_FIRE);
        assert_eq!(record.human_status() & SHVF_CAR_STEAL, 0);
    }

    #[test]
    fn rva_9d0b0_car_steal_sets_flag_and_time() {
        // C_shvestky::CarSteal GAME VA 0x49d0b0 (RVA 0x9d0b0).
        assert_eq!(CAR_STEAL_ENTRY, 0x49d0b0);
        let mut record = PolicePursuitRecord::new(7);
        record.record_car_steal();
        assert_eq!(record.flags & SHVF_CAR_STEAL, SHVF_CAR_STEAL);
        assert_eq!(record.car_steal_time(), 5000);
        assert_eq!(record.human_status() & SHVF_CAR_STEAL, SHVF_CAR_STEAL);
    }

    #[test]
    fn rva_9d980_create_road_block_reports_unsupported() {
        // C_shvestky::CreateRoadBlock GAME VA 0x49d980 (RVA 0x9d980).
        assert_eq!(CREATE_ROAD_BLOCK_ENTRY, 0x49d980);
        assert_eq!(
            create_road_block(2, 7),
            Err(PoliceDispatchError::Unsupported { entry: 0x49d980 })
        );
    }

    #[test]
    fn rva_9a140_take_tyres_radio_reports_unsupported() {
        // C_shvestky::DezoSebralGumyIRadio GAME VA 0x49a140 (RVA 0x9a140).
        assert_eq!(TAKE_TYRES_RADIO_ENTRY, 0x49a140);
        assert_eq!(
            take_car_tyres_and_radio(9),
            Err(PoliceDispatchError::Unsupported { entry: 0x49a140 })
        );
    }

    #[test]
    fn rva_a0230_rename_police_car_reports_unsupported() {
        // C_shvestky::RenamePoliceCar GAME VA 0x4a0230 (RVA 0xa0230).
        assert_eq!(RENAME_POLICE_CAR_ENTRY, 0x4a0230);
        assert_eq!(
            rename_police_car(9),
            Err(PoliceDispatchError::Unsupported { entry: 0x4a0230 })
        );
    }

    #[test]
    fn rva_99dc0_game_done_reports_unsupported() {
        // C_shvestky::GameDone GAME VA 0x499dc0 (RVA 0x99dc0): teardown
        // needs mission/game/radar/actor owners, so it stays a boundary.
        assert_eq!(GAME_DONE_ENTRY, 0x499dc0);
        assert_eq!(
            police_game_done(),
            Err(PoliceDispatchError::Unsupported { entry: 0x499dc0 })
        );
    }

    #[test]
    fn rva_a0320_sledovacka_new_has_no_road_net() {
        // C_sledovacka::C_sledovacka GAME VA 0x4a0320 (RVA 0xa0320).
        assert_eq!(SLEDOVACKA_CTOR_ENTRY, 0x4a0320);
        let state = SledovackaState::new();
        assert!(!state.road_net_set());
        assert_eq!(state.bridge_count(), 0);
        assert_eq!(state.chase_car_count(), 0);
        assert_eq!(state, SledovackaState::default());
    }

    #[test]
    fn rva_a0360_sledovacka_init_reports_unsupported() {
        // C_sledovacka::Init GAME VA 0x4a0360 (RVA 0xa0360).
        assert_eq!(SLEDOVACKA_INIT_ENTRY, 0x4a0360);
        assert_eq!(
            sledovacka_init_bridges(),
            Err(SledovackaError::Unsupported { entry: 0x4a0360 })
        );
    }

    #[test]
    fn rva_a0660_sledovacka_done_forgets_lists() {
        // C_sledovacka::Done GAME VA 0x4a0660 (RVA 0xa0660).
        assert_eq!(SLEDOVACKA_DONE_ENTRY, 0x4a0660);
        let mut state = SledovackaState {
            road_net_set: true,
            bridges: vec![11, 13],
            chase_cars: vec![21],
        };
        state.apply_done();
        assert!(!state.road_net_set());
        assert_eq!(state.bridge_count(), 0);
        assert_eq!(state.chase_car_count(), 0);
        assert_eq!(state, SledovackaState::new());
    }

    #[test]
    fn wanted_time_reports_latched_value() {
        // Working name: no separate native entry (inline in Tick 0x49a300).
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(record.wanted_time(), -1);
        record.wanted_time = 20000;
        assert_eq!(record.wanted_time(), 20000);
    }

    #[test]
    fn signal_time_reports_latched_value() {
        // Working name: no separate native entry (inline in Tick 0x49a300).
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(record.signal_time(), 0);
        record.signal_time = 4000;
        assert_eq!(record.signal_time(), 4000);
    }

    #[test]
    fn car_steal_time_pairs_with_car_steal_flag() {
        // Working name: no separate native entry (inline expiry in Tick).
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(record.car_steal_time(), 0);
        record.record_car_steal();
        assert_eq!(record.car_steal_time(), 5000);
    }

    #[test]
    fn record_latch_counters_report_staged_times() {
        // Working names: no separate native entries (inline Tick expiry).
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(record.illegal_path_time(), 0);
        assert_eq!(record.collision_time(), 0);
        assert_eq!(record.hits_time(), -1);
        assert_eq!(record.hits(), 0);
        record.illegal_path_time = 5800;
        record.collision_time = 4000;
        record.hits_time = 12;
        record.hits = 3;
        assert_eq!(record.illegal_path_time(), 5800);
        assert_eq!(record.collision_time(), 4000);
        assert_eq!(record.hits_time(), 12);
        assert_eq!(record.hits(), 3);
    }

    #[test]
    fn record_action_flags_report_staged_state() {
        // Working names: no separate native entries (inline Tick reads).
        let mut record = PolicePursuitRecord::new(7);
        assert!(!record.is_block_way());
        assert!(!record.is_alerted());
        assert_eq!(record.support_level(), 0);
        assert_eq!(record.rammed_car(), 0);
        assert_eq!(record.pending_state(), SHVSTATE_NONE);
        record.block_way = true;
        record.alerted = true;
        record.support_level = 2;
        record.rammed_car = 0x1234;
        record.new_state = SHVSTATE_ARREST;
        assert!(record.is_block_way());
        assert!(record.is_alerted());
        assert_eq!(record.support_level(), 2);
        assert_eq!(record.rammed_car(), 0x1234);
        assert_eq!(record.pending_state(), SHVSTATE_ARREST);
    }

    #[test]
    fn identity_working_name_reads_human_and_car() {
        // Working names: S_shvestky human (+0x0) and car (+0x4); no dedicated
        // native getters, reads are inline in the effects and Tick.
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(record.human(), 7);
        assert_eq!(record.car(), None);
        record.apply_drive_car(Some(21));
        assert_eq!(record.car(), Some(21));
        record.apply_drive_car(None);
        assert_eq!(record.car(), None);
    }

    #[test]
    fn flags2_working_name_reads_second_flag_word() {
        // Working name: S_shvestky flags2 (+0xa, SHVF2_*); inline Tick only.
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(record.flags2(), 0);
        record.flags2 = SHVF2_CRASH;
        assert_eq!(record.flags2(), SHVF2_CRASH);
    }

    #[test]
    fn boxing_time_working_name_pairs_with_boxing_flag() {
        // Working name: S_shvestky boxing_time (+0x20); inline Tick expiry.
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(record.boxing_time(), 0);
        record.record_boxing();
        assert_eq!(record.boxing_time(), 5000);
        assert_eq!(record.flags & SHVF_BOXING, SHVF_BOXING);
    }

    #[test]
    fn known_car_working_name_reads_filed_car_and_foot_flag() {
        // Working names: S_shvestky known_car (+0x54) and known_on_foot
        // (+0x58); compared inline in CreateShvestkyTest only.
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(record.known_car(), None);
        assert!(!record.known_on_foot());
        record.known_car = Some(33);
        record.known_on_foot = true;
        assert_eq!(record.known_car(), Some(33));
        assert!(record.known_on_foot());
    }

    #[test]
    fn reason_working_name_reads_state_reason() {
        // Working name: S_shvestky reason (+0x64, SHVREASON_*); written by the
        // CreateShvestkyTest flag arms only.
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(record.reason(), 0);
        record.reason = 8;
        assert_eq!(record.reason(), 8);
    }

    #[test]
    fn run_over_working_name_counts_run_overs() {
        // Working name: S_shvestky run_over (+0x68); maintained by the
        // NextNaSrackySoudruh effect only.
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(record.run_over(), 0);
        record.record_run_over();
        assert_eq!(record.run_over(), 1);
        assert_eq!(record.status_word(), SHVSTATE_NONE);
    }

    #[test]
    fn timers_working_name_read_period_timers() {
        // Working names: +0x14/+0x1c/+0x24 S_timer latches; no dedicated
        // native getters, reads are inline in Tick 0x49a300.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.do_rici_timer().period_ms(), 200);
        assert_eq!(state.red_light_timer().period_ms(), 200);
        assert_eq!(state.unknown_24_timer().period_ms(), 200);
        state.apply_game_init();
        assert_eq!(state.do_rici_timer().period_ms(), 913);
        assert_eq!(state.red_light_timer().period_ms(), 217);
        assert_eq!(state.unknown_24_timer().period_ms(), 4013);
        assert_eq!(state.records_timer().period_ms(), 330);
    }

    #[test]
    fn unknown_words_working_name_read_cleared_words() {
        // Working names: +0x2c/+0x30 words; GameInit clears them, no
        // dedicated native getters and no gameplay meaning attached.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.unknown_2c(), 0);
        assert_eq!(state.unknown_30(), 0);
        state.unknown_2c = 5;
        state.unknown_30 = 9;
        assert_eq!(state.unknown_2c(), 5);
        assert_eq!(state.unknown_30(), 9);
        state.apply_game_init();
        assert_eq!(state.unknown_2c(), 0);
        assert_eq!(state.unknown_30(), 0);
    }

    #[test]
    fn free_car_count_working_name_counts_free_marks() {
        // Working name: S_road_block_car free marks; no PC outline.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.count_free_road_block_cars(), 0);
        state.road_block_cars.push(PoliceRoadBlockCar {
            car: 1,
            free: true,
        });
        state.road_block_cars.push(PoliceRoadBlockCar {
            car: 2,
            free: false,
        });
        state.road_block_cars.push(PoliceRoadBlockCar {
            car: 3,
            free: true,
        });
        assert_eq!(state.count_free_road_block_cars(), 2);
    }

    #[test]
    fn free_iris_count_working_name_counts_free_marks() {
        // Working name: S_road_block_iris free marks; no PC outline.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.count_free_road_block_irises(), 0);
        state.road_block_irises.push(PoliceRoadBlockIris {
            actor: 1,
            free: false,
        });
        state.road_block_irises.push(PoliceRoadBlockIris {
            actor: 2,
            free: true,
        });
        assert_eq!(state.count_free_road_block_irises(), 1);
    }

    #[test]
    fn take_car_working_name_takes_last_free() {
        // Working name: car checkout following the iris 0x49e560 shape and
        // the PS2 GetTempCar family; no PC outline, so no RVA is claimed.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.take_free_road_block_car(), None);
        for (car, free) in [(1, false), (2, true), (3, true)] {
            state.road_block_cars.push(PoliceRoadBlockCar { car, free });
        }
        assert_eq!(state.take_free_road_block_car(), Some(3));
        assert!(!state.road_block_cars[2].free);
        assert_eq!(state.take_free_road_block_car(), Some(2));
        assert_eq!(state.take_free_road_block_car(), None);
        assert_eq!(state.count_free_road_block_cars(), 0);
    }

    #[test]
    fn weapon_flag_working_name_reads_bit() {
        // Working name: SHVF_WEAPON 0x1; inline native use only.
        let mut record = PolicePursuitRecord::new(7);
        assert!(!record.has_weapon());
        record.flags |= SHVF_WEAPON;
        assert!(record.has_weapon());
        record.flags &= !SHVF_WEAPON;
        assert!(!record.has_weapon());
    }

    #[test]
    fn speeding_flag_working_name_reads_bit() {
        // Working name: SHVF_SPEEDING 0x4; inline native use only.
        let mut record = PolicePursuitRecord::new(7);
        assert!(!record.is_speeding());
        record.flags |= SHVF_SPEEDING;
        assert!(record.is_speeding());
    }

    #[test]
    fn crash_bits_working_name_read_any_bit() {
        // Working name: SHVF_CRASH 0x38; inline native use only.
        let mut record = PolicePursuitRecord::new(7);
        assert!(!record.has_crash());
        record.flags |= 0x10;
        assert!(record.has_crash());
        record.flags &= !SHVF_CRASH;
        assert!(!record.has_crash());
        assert_eq!(SHVF_CRASH, 0x38);
    }

    #[test]
    fn red_light_flag_working_name_reads_bit() {
        // Working name: SHVF_RED_LIGHT 0x400; inline native use only.
        let mut record = PolicePursuitRecord::new(7);
        assert!(!record.has_red_light());
        record.flags |= SHVF_RED_LIGHT;
        assert!(record.has_red_light());
    }

    #[test]
    fn car_steal_flag_working_name_pairs_with_effect() {
        // Working name: SHVF_CAR_STEAL 0x800; paired with record_car_steal.
        let mut record = PolicePursuitRecord::new(7);
        assert!(!record.has_car_steal());
        record.record_car_steal();
        assert!(record.has_car_steal());
        assert_eq!(record.car_steal_time(), 5000);
    }

    #[test]
    fn force_arrest_flag_working_name_pairs_with_effect() {
        // Working name: SHVF_FORCE_ARREST 0x4000; paired with
        // record_force_arrest.
        let mut record = PolicePursuitRecord::new(7);
        assert!(!record.has_force_arrest());
        record.record_force_arrest();
        assert!(record.has_force_arrest());
        assert_eq!(record.pending_state(), SHVSTATE_ARREST);
    }

    #[test]
    fn ram_police_car_flag_working_name_reads_bit() {
        // Working name: SHVF_RAM_POLICE_CAR 0x8000; inline native use only.
        let mut record = PolicePursuitRecord::new(7);
        assert!(!record.has_ram_police_car());
        record.flags |= SHVF_RAM_POLICE_CAR;
        assert!(record.has_ram_police_car());
        record.flags &= !SHVF_RAM_POLICE_CAR;
        assert!(!record.has_ram_police_car());
    }

    #[test]
    fn flag_constants_match_reference_words() {
        // mafia-re game/ai/C_shvestky.h:16-30 at pin 113b5b6.
        assert_eq!(SHVF_WEAPON, 0x1);
        assert_eq!(SHVF_SPEEDING, 0x4);
        assert_eq!(SHVF_RED_LIGHT, 0x400);
        assert_eq!(SHVF_RAM_POLICE_CAR, 0x8000);
    }

    #[test]
    fn combined_record_flags_report_each_bit() {
        // Working names: each SHVF_* reader selects only its own bit(s).
        let mut record = PolicePursuitRecord::new(7);
        record.flags = SHVF_WEAPON | SHVF_RED_LIGHT;
        assert!(record.has_weapon());
        assert!(record.has_red_light());
        assert!(!record.is_speeding());
        assert!(!record.has_crash());
        assert!(!record.has_car_steal());
        assert!(!record.has_force_arrest());
        assert!(!record.has_ram_police_car());
        assert_eq!(record.human_status(), SHVF_WEAPON | SHVF_RED_LIGHT);
    }

    #[test]
    fn road_block_car_and_iris_counts_stay_independent() {
        // Working names: car/iris/enemy free counts track separate lists.
        let mut state = PoliceGameInitState::default();
        state.road_block_cars.push(PoliceRoadBlockCar {
            car: 1,
            free: true,
        });
        state.road_block_irises.push(PoliceRoadBlockIris {
            actor: 2,
            free: false,
        });
        state.road_block_enemies.push(PoliceRoadBlockEnemy {
            actor: 3,
            free: true,
        });
        assert_eq!(state.count_free_road_block_cars(), 1);
        assert_eq!(state.count_free_road_block_irises(), 0);
        assert_eq!(state.count_free_road_block_enemies(), 1);
    }

    #[test]
    fn status_for_human_reads_latest_match_or_none() {
        // Working name: lookup half of JakPicusNaTomJest; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.status_for_human(7), 0);
        let mut first = PolicePursuitRecord::new(7);
        first.state = SHVSTATE_ARREST;
        let mut second = PolicePursuitRecord::new(7);
        second.state = SHVSTATE_KILL;
        state.records.push(first);
        state.records.push(second);
        assert_eq!(state.status_for_human(7), SHVSTATE_KILL);
        assert_eq!(state.status_for_human(9), SHVSTATE_NONE);
    }

    #[test]
    fn crash_time_for_human_reads_latest_match_or_zero() {
        // Working name: lookup half of GetCrashTime; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.crash_time_for_human(7), 0);
        let mut first = PolicePursuitRecord::new(7);
        first.crash_time = 400;
        let mut second = PolicePursuitRecord::new(7);
        second.crash_time = 900;
        state.records.push(first);
        state.records.push(second);
        assert_eq!(state.crash_time_for_human(7), 900);
    }

    #[test]
    fn fire_time_for_human_reads_latest_match_or_zero() {
        // Working name: lookup half of GetFireTime; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.fire_time_for_human(7), 0);
        let mut record = PolicePursuitRecord::new(7);
        record.fire_time = 1500;
        state.records.push(record);
        assert_eq!(state.fire_time_for_human(7), 1500);
        assert_eq!(state.fire_time_for_human(8), 0);
    }

    #[test]
    fn offenses_for_human_reads_latest_match_or_zero() {
        // Working name: lookup half of PocetPrestupku; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.offenses_for_human(7), 0);
        let mut first = PolicePursuitRecord::new(7);
        first.offenses = 2;
        let mut second = PolicePursuitRecord::new(9);
        second.offenses = 5;
        state.records.push(first);
        state.records.push(second);
        assert_eq!(state.offenses_for_human(7), 2);
        assert_eq!(state.offenses_for_human(9), 5);
    }

    #[test]
    fn human_status_for_human_reads_latest_flags_or_zero() {
        // Working name: lookup half of GetHumanStatus; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.human_status_for_human(7), 0);
        let mut record = PolicePursuitRecord::new(7);
        record.flags = SHVF_WEAPON | SHVF_SPEEDING;
        state.records.push(record);
        assert_eq!(
            state.human_status_for_human(7),
            SHVF_WEAPON | SHVF_SPEEDING
        );
    }

    #[test]
    fn nevi_for_human_gates_on_unknown_38_and_record() {
        // Working name: lookup half of NeviDoKterePiceVlezl.
        let mut state = PoliceGameInitState::default();
        let mut record = PolicePursuitRecord::new(7);
        record.nevi = true;
        state.records.push(record);
        assert!(state.nevi_for_human(7));
        assert!(!state.nevi_for_human(9));
        state.unknown_38 = true;
        assert!(!state.nevi_for_human(7));
    }

    #[test]
    fn apply_firing_for_human_latches_latest_record() {
        // Working name: lookup half of Firing.
        let mut state = PoliceGameInitState::default();
        assert!(!state.apply_firing_for_human(7, 1200));
        state.records.push(PolicePursuitRecord::new(7));
        state.records.push(PolicePursuitRecord::new(7));
        assert!(state.apply_firing_for_human(7, 1200));
        assert_eq!(state.records[0].fire_time(), 0);
        assert_eq!(state.records[1].fire_time(), 1200);
        assert_ne!(state.records[1].human_status() & SHVF_FIRE, 0);
        assert!(!state.apply_firing_for_human(9, 1200));
    }

    #[test]
    fn apply_boxing_for_human_adds_time_to_latest_record() {
        // Working name: lookup half of HumanBoxing.
        let mut state = PoliceGameInitState::default();
        assert!(!state.apply_boxing_for_human(7));
        let mut record = PolicePursuitRecord::new(7);
        record.boxing_time = 1000;
        state.records.push(record);
        assert!(state.apply_boxing_for_human(7));
        assert_eq!(state.records[0].boxing_time(), 6000);
        assert_ne!(state.records[0].human_status() & SHVF_BOXING, 0);
    }

    #[test]
    fn apply_force_arrest_for_human_raises_latest_record() {
        // Working name: lookup half of ForceArrest.
        let mut state = PoliceGameInitState::default();
        assert!(!state.apply_force_arrest_for_human(7));
        state.records.push(PolicePursuitRecord::new(7));
        assert!(state.apply_force_arrest_for_human(7));
        assert_ne!(state.records[0].human_status() & SHVF_FORCE_ARREST, 0);
        assert_eq!(state.records[0].pending_state(), SHVSTATE_ARREST);
    }

    #[test]
    fn apply_run_over_for_human_counts_latest_record() {
        // Working name: lookup half of NextNaSrackySoudruh.
        let mut state = PoliceGameInitState::default();
        assert!(!state.apply_run_over_for_human(7));
        state.records.push(PolicePursuitRecord::new(7));
        assert!(state.apply_run_over_for_human(7));
        assert_eq!(state.records[0].run_over(), 1);
        assert_eq!(state.records[0].status_word(), SHVSTATE_NONE);
        for _ in 0..5 {
            assert!(state.apply_run_over_for_human(7));
        }
        assert_eq!(state.records[0].run_over(), 6);
        assert_eq!(state.records[0].status_word(), SHVSTATE_KILL);
    }

    #[test]
    fn apply_drive_car_for_human_follows_latest_record() {
        // Working name: lookup half of DriveCar; None is the native null.
        let mut state = PoliceGameInitState::default();
        assert!(!state.apply_drive_car_for_human(7, Some(11)));
        state.records.push(PolicePursuitRecord::new(7));
        assert!(state.apply_drive_car_for_human(7, Some(11)));
        assert_eq!(state.records[0].car(), Some(11));
        assert!(state.apply_drive_car_for_human(7, None));
        assert_eq!(state.records[0].car(), None);
        assert!(!state.apply_drive_car_for_human(9, Some(11)));
    }

    #[test]
    fn apply_car_steal_for_human_latches_latest_record() {
        // Working name: lookup half of CarSteal.
        let mut state = PoliceGameInitState::default();
        assert!(!state.apply_car_steal_for_human(7));
        state.records.push(PolicePursuitRecord::new(7));
        assert!(state.apply_car_steal_for_human(7));
        assert_ne!(state.records[0].human_status() & SHVF_CAR_STEAL, 0);
        assert_eq!(state.records[0].car_steal_time(), 5000);
    }

    #[test]
    fn block_way_for_human_reads_latest_match_or_false() {
        // Working name: inline-only block_way read; false without a record.
        let mut state = PoliceGameInitState::default();
        assert!(!state.block_way_for_human(7));
        let mut first = PolicePursuitRecord::new(7);
        first.block_way = false;
        let mut second = PolicePursuitRecord::new(7);
        second.block_way = true;
        state.records.push(first);
        state.records.push(second);
        assert!(state.block_way_for_human(7));
    }

    #[test]
    fn pending_state_for_human_reads_latest_match_or_none() {
        // Working name: inline-only new_state read; NONE without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.pending_state_for_human(7), SHVSTATE_NONE);
        let mut record = PolicePursuitRecord::new(7);
        record.new_state = SHVSTATE_ARREST;
        state.records.push(record);
        assert_eq!(state.pending_state_for_human(7), SHVSTATE_ARREST);
    }

    #[test]
    fn support_level_for_human_reads_latest_match_or_zero() {
        // Working name: inline-only support_level read; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.support_level_for_human(7), 0);
        let mut record = PolicePursuitRecord::new(7);
        record.support_level = 2;
        state.records.push(record);
        assert_eq!(state.support_level_for_human(7), 2);
        assert_eq!(state.support_level_for_human(8), 0);
    }

    #[test]
    fn wanted_time_for_human_reads_latest_match_or_cleared() {
        // Working name: inline-only wanted_time read; -1 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.wanted_time_for_human(7), -1);
        let mut first = PolicePursuitRecord::new(7);
        first.wanted_time = 20000;
        let mut second = PolicePursuitRecord::new(7);
        second.wanted_time = 40000;
        state.records.push(first);
        state.records.push(second);
        assert_eq!(state.wanted_time_for_human(7), 40000);
        assert_eq!(state.wanted_time_for_human(8), -1);
    }

    #[test]
    fn wanted_time_start_for_human_reads_latest_match_or_zero() {
        // Working name: inline-only wanted_time_start read; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.wanted_time_start_for_human(7), 0);
        let mut record = PolicePursuitRecord::new(7);
        record.wanted_time_start = 20000;
        state.records.push(record);
        assert_eq!(state.wanted_time_start_for_human(7), 20000);
        assert_eq!(state.wanted_time_start_for_human(8), 0);
    }

    #[test]
    fn signal_time_for_human_reads_latest_match_or_zero() {
        // Working name: inline-only signal_time read; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.signal_time_for_human(7), 0);
        let mut first = PolicePursuitRecord::new(7);
        first.signal_time = 4000;
        let mut second = PolicePursuitRecord::new(7);
        second.signal_time = 8000;
        state.records.push(first);
        state.records.push(second);
        assert_eq!(state.signal_time_for_human(7), 8000);
    }

    #[test]
    fn car_steal_time_for_human_reads_latest_match_or_zero() {
        // Working name: inline-only car_steal_time read; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.car_steal_time_for_human(7), 0);
        let mut record = PolicePursuitRecord::new(7);
        record.record_car_steal();
        state.records.push(record);
        assert_eq!(state.car_steal_time_for_human(7), 5000);
        assert_eq!(state.car_steal_time_for_human(8), 0);
    }

    #[test]
    fn illegal_path_time_for_human_reads_latest_match_or_zero() {
        // Working name: inline-only illegal_path_time read; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.illegal_path_time_for_human(7), 0);
        let mut record = PolicePursuitRecord::new(7);
        record.illegal_path_time = 1200;
        state.records.push(record);
        assert_eq!(state.illegal_path_time_for_human(7), 1200);
    }

    #[test]
    fn collision_time_for_human_reads_latest_match_or_zero() {
        // Working name: inline-only collision_time read; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.collision_time_for_human(7), 0);
        let mut record = PolicePursuitRecord::new(7);
        record.collision_time = 4000;
        state.records.push(record);
        assert_eq!(state.collision_time_for_human(7), 4000);
    }

    #[test]
    fn boxing_time_for_human_reads_latest_match_or_zero() {
        // Working name: inline-only boxing_time read; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.boxing_time_for_human(7), 0);
        let mut record = PolicePursuitRecord::new(7);
        record.record_boxing();
        state.records.push(record);
        assert_eq!(state.boxing_time_for_human(7), 5000);
    }

    #[test]
    fn hits_time_for_human_reads_latest_match_or_cleared() {
        // Working name: inline-only hits_time read; -1 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.hits_time_for_human(7), -1);
        let mut record = PolicePursuitRecord::new(7);
        record.hits_time = 5000;
        state.records.push(record);
        assert_eq!(state.hits_time_for_human(7), 5000);
    }

    #[test]
    fn hits_for_human_reads_latest_match_or_zero() {
        // Working name: inline-only hits read; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.hits_for_human(7), 0);
        let mut record = PolicePursuitRecord::new(7);
        record.hits = 3;
        state.records.push(record);
        assert_eq!(state.hits_for_human(7), 3);
        assert_eq!(state.hits_for_human(8), 0);
    }

    #[test]
    fn reason_for_human_reads_latest_match_or_zero() {
        // Working name: inline-only reason read; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.reason_for_human(7), 0);
        let mut record = PolicePursuitRecord::new(7);
        record.reason = 8;
        state.records.push(record);
        assert_eq!(state.reason_for_human(7), 8);
    }

    #[test]
    fn run_over_for_human_reads_latest_match_or_zero() {
        // Working name: inline-only run_over read; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.run_over_for_human(7), 0);
        let mut record = PolicePursuitRecord::new(7);
        record.record_run_over();
        record.record_run_over();
        state.records.push(record);
        assert_eq!(state.run_over_for_human(7), 2);
    }

    #[test]
    fn known_car_for_human_reads_latest_match_or_none() {
        // Working name: inline-only known_car read; None without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.known_car_for_human(7), None);
        let mut record = PolicePursuitRecord::new(7);
        record.known_car = Some(11);
        state.records.push(record);
        assert_eq!(state.known_car_for_human(7), Some(11));
        assert_eq!(state.known_car_for_human(8), None);
    }

    #[test]
    fn known_on_foot_for_human_reads_latest_match_or_false() {
        // Working name: inline-only known_on_foot read; false without a record.
        let mut state = PoliceGameInitState::default();
        assert!(!state.known_on_foot_for_human(7));
        let mut record = PolicePursuitRecord::new(7);
        record.known_on_foot = true;
        state.records.push(record);
        assert!(state.known_on_foot_for_human(7));
        assert!(!state.known_on_foot_for_human(8));
    }

    #[test]
    fn rammed_car_for_human_reads_latest_match_or_zero() {
        // Working name: inline-only rammed_car read; 0 without a record.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.rammed_car_for_human(7), 0);
        let mut record = PolicePursuitRecord::new(7);
        record.rammed_car = 13;
        state.records.push(record);
        assert_eq!(state.rammed_car_for_human(7), 13);
    }

    #[test]
    fn fire_flag_working_name_reads_bit() {
        // Working name: SHVF_FIRE 0x2; paired with record_firing.
        let mut record = PolicePursuitRecord::new(7);
        assert!(!record.has_fire());
        record.record_firing(4000);
        assert!(record.has_fire());
        assert_eq!(record.fire_time(), 4000);
        record.flags &= !SHVF_FIRE;
        assert!(!record.has_fire());
    }

    #[test]
    fn boxing_flag_working_name_reads_bit() {
        // Working name: SHVF_BOXING 0x200; paired with record_boxing.
        let mut record = PolicePursuitRecord::new(7);
        assert!(!record.has_boxing());
        record.record_boxing();
        assert!(record.has_boxing());
        assert_eq!(record.boxing_time(), 5000);
    }

    #[test]
    fn illegal_path_flag_working_name_reads_bit() {
        // Working name: SHVF_ILLEGAL_PATH 0x40; inline native use only.
        let mut record = PolicePursuitRecord::new(7);
        assert!(!record.has_illegal_path());
        record.flags |= SHVF_ILLEGAL_PATH;
        assert!(record.has_illegal_path());
        record.flags &= !SHVF_ILLEGAL_PATH;
        assert!(!record.has_illegal_path());
    }

    #[test]
    fn signal_mark_flag_working_name_reads_bit() {
        // Working name: SHVF_SIGNAL_MARK 0x1000; set by the signal-mark arm.
        let mut state = PoliceGameInitState::default();
        state.active = true;
        state.records.push(PolicePursuitRecord::new(7));
        assert!(!state.records[0].has_signal_mark());
        state.apply_destroy_signal_mark(7);
        assert!(state.records[0].has_signal_mark());
    }

    #[test]
    fn hit_policeman_flag_working_name_reads_bit() {
        // Working name: SHVF_HIT_POLICEMAN 0x2000; set by the hit arm.
        let mut state = PoliceGameInitState::default();
        state.records.push(PolicePursuitRecord::new(7));
        assert!(!state.records[0].has_hit_policeman());
        state.apply_hit_policeman(7);
        assert!(state.records[0].has_hit_policeman());
    }

    #[test]
    fn flags2_crash_working_name_reads_any_bit() {
        // Working name: SHVF2_CRASH 0x38 on the second flag word.
        let mut record = PolicePursuitRecord::new(7);
        assert!(!record.has_flags2_crash());
        record.flags2 |= 0x10;
        assert!(record.has_flags2_crash());
        record.flags2 &= !SHVF2_CRASH;
        assert!(!record.has_flags2_crash());
        assert_eq!(SHVF2_CRASH, 0x38);
    }

    #[test]
    fn has_record_for_human_scans_back_to_front() {
        // Working name: existence half of the record lookups.
        let mut state = PoliceGameInitState::default();
        assert!(!state.has_record_for_human(7));
        state.records.push(PolicePursuitRecord::new(9));
        assert!(!state.has_record_for_human(7));
        assert!(state.has_record_for_human(9));
        state.records.push(PolicePursuitRecord::new(7));
        assert!(state.has_record_for_human(7));
    }

    #[test]
    fn is_do_rici_listed_tracks_admit_and_evict() {
        // Working name: membership half of the do_rici duplicate guard.
        let mut state = PoliceGameInitState::default();
        assert!(!state.is_do_rici_listed(11));
        assert!(state.admit_do_rici(11));
        assert!(state.is_do_rici_listed(11));
        assert!(!state.is_do_rici_listed(13));
        assert!(state.evict_do_rici(11));
        assert!(!state.is_do_rici_listed(11));
    }

    #[test]
    fn is_temp_car_tracked_tracks_staged_cars() {
        // Working name: membership half of the temp_cars search.
        let mut state = PoliceGameInitState::default();
        assert!(!state.is_temp_car_tracked(40));
        state.temp_cars.push(40);
        state.temp_cars.push(41);
        assert!(state.is_temp_car_tracked(40));
        assert!(state.is_temp_car_tracked(41));
        assert!(!state.is_temp_car_tracked(42));
        assert!(state.free_my_temp_actor(40));
        assert!(!state.is_temp_car_tracked(40));
        assert!(state.is_temp_car_tracked(41));
    }

    #[test]
    fn tick_decay_timers_clears_expired_latches_and_counts() {
        // Working name: Tick 0x49a300 records slice; inlined, no RVA claimed.
        let mut record = PolicePursuitRecord::new(7);
        record.flags = SHVF_CRASH
            | SHVF_BOXING
            | SHVF_RED_LIGHT
            | SHVF_SIGNAL_MARK
            | SHVF_CAR_STEAL
            | SHVF_FIRE
            | SHVF_ILLEGAL_PATH;
        record.flags2 = SHVF2_CRASH;
        record.collision_time = 330;
        record.crash_time = 100;
        record.illegal_path_time = 4900;
        record.boxing_time = 500;
        record.signal_time = 500;
        record.car_steal_time = 500;
        record.fire_time = 500;
        record.hits_time = 400;
        record.hits = 2;
        let car = IllegalPathCarProbe {
            speed: 20.0,
            illegal_path: true,
        };
        record.tick_decay_timers(330, Some(car));
        assert_eq!(record.collision_time, 0);
        assert_eq!(record.flags & SHVF_CRASH, 0);
        assert_eq!(record.crash_time, 0);
        assert_eq!(record.flags2 & SHVF2_CRASH, 0);
        // 4900 + 330 clamps at 5000 with the bit kept.
        assert_eq!(record.illegal_path_time, 5000);
        assert_ne!(record.flags & SHVF_ILLEGAL_PATH, 0);
        assert_eq!(record.boxing_time, 170);
        assert_ne!(record.flags & SHVF_BOXING, 0);
        assert_eq!(record.signal_time, 170);
        assert_eq!(record.car_steal_time, 170);
        assert_eq!(record.fire_time, 170);
        assert_eq!(record.hits_time, 70);
        assert_eq!(record.hits, 2);
        // A lost illegal path clears the bit; over-long decays clear theirs;
        // a negative hits rest drops the count.
        record.tick_decay_timers(
            200,
            Some(IllegalPathCarProbe {
                speed: 20.0,
                illegal_path: false,
            }),
        );
        assert_eq!(record.flags & SHVF_ILLEGAL_PATH, 0);
        assert_eq!(record.boxing_time, 0);
        assert_eq!(record.flags & SHVF_BOXING, 0);
        assert_eq!(record.signal_time, 0);
        assert_eq!(record.flags & (SHVF_RED_LIGHT | SHVF_SIGNAL_MARK), 0);
        assert_eq!(record.car_steal_time, 0);
        assert_eq!(record.fire_time, 0);
        assert_eq!(record.flags & SHVF_FIRE, 0);
        assert_eq!(record.hits_time, -130);
        assert_eq!(record.hits, 0);
        // A null car clears the illegal-path pair outright.
        record.flags |= SHVF_ILLEGAL_PATH;
        record.illegal_path_time = 800;
        record.tick_decay_timers(330, None);
        assert_eq!(record.flags & SHVF_ILLEGAL_PATH, 0);
        assert_eq!(record.illegal_path_time, 0);
        // An exact-period rest keeps a zero hits count.
        record.hits_time = 330;
        record.hits = 4;
        record.tick_decay_timers(330, None);
        assert_eq!(record.hits_time, 0);
        assert_eq!(record.hits, 4);
    }

    #[test]
    fn weapon_scan_sets_and_clears_weapon_bit() {
        // Working name: Tick 0x49a300 weapon scan; inlined, no RVA claimed.
        let mut record = PolicePursuitRecord::new(7);
        record.apply_weapon_scan(4);
        assert_eq!(record.flags & SHVF_WEAPON, 0);
        for item in [5, 10, 15] {
            record.apply_weapon_scan(item);
            assert_ne!(record.flags & SHVF_WEAPON, 0);
        }
        record.apply_weapon_scan(16);
        assert_eq!(record.flags & SHVF_WEAPON, 0);
        // Other bits survive the clear arm.
        record.flags |= SHVF_SPEEDING;
        record.apply_weapon_scan(0);
        assert_eq!(record.flags & SHVF_SPEEDING, SHVF_SPEEDING);
        assert_eq!(record.flags & SHVF_WEAPON, 0);
    }

    #[test]
    fn speeding_scan_sets_and_clears_speeding_bit() {
        // Working name: Tick 0x49a300 speeding scan; inlined, no RVA claimed.
        let mut record = PolicePursuitRecord::new(7);
        record.apply_speeding_scan(None, 17.0);
        assert!(!record.is_speeding());
        record.apply_speeding_scan(Some(17.0), 17.0);
        assert!(!record.is_speeding());
        record.apply_speeding_scan(Some(17.5), 17.0);
        assert!(record.is_speeding());
        record.apply_speeding_scan(None, 17.0);
        assert!(!record.is_speeding());
    }

    #[test]
    fn angry_admit_guards_duplicates() {
        // Working name: PS2 AddNasrany add half; no PC outline, no RVA.
        let mut list = vec![10u32, 20];
        assert!(!admit_angry_actor(&mut list, 10));
        assert_eq!(list, vec![10, 20]);
        assert!(admit_angry_actor(&mut list, 30));
        assert_eq!(list, vec![10, 20, 30]);
    }

    #[test]
    fn angry_retain_drops_dead_with_swap_remove() {
        // Working name: Tick 0x49a300 dead-prune; inlined, no RVA claimed.
        let mut list = vec![10u32, 20, 30, 40];
        retain_living_angry(&mut list, |actor| actor == 20 || actor == 30);
        assert_eq!(list, vec![10, 40]);
        retain_living_angry(&mut list, |_| false);
        assert_eq!(list, vec![10, 40]);
        retain_living_angry(&mut list, |_| true);
        assert!(list.is_empty());
    }

    #[test]
    fn wanted_clock_releases_clamps_and_refreshes() {
        // Working name: Tick 0x49a300 wanted slice; inlined, no RVA claimed.
        let mut record = PolicePursuitRecord::new(7);
        assert_eq!(
            record.update_wanted_clock(330, true, true),
            WantedClockAction::Steady
        );
        // A countdown past zero with nobody angry releases the human.
        let mut record = PolicePursuitRecord::new(7);
        record.state = 1;
        record.wanted_time = 200;
        assert_eq!(
            record.update_wanted_clock(330, true, false),
            WantedClockAction::Release
        );
        assert_eq!(record.wanted_time, -1);
        // An alerted countdown clamps the windows and stays steady.
        let mut record = PolicePursuitRecord::new(7);
        record.state = SHVSTATE_ARREST;
        record.alerted = true;
        record.wanted_time = 5000;
        record.fire_time = 5000;
        record.collision_time = 100;
        record.boxing_time = 3000;
        assert_eq!(
            record.update_wanted_clock(330, true, false),
            WantedClockAction::Steady
        );
        assert_eq!(record.wanted_time, 4670);
        assert_eq!(record.fire_time, 2000);
        assert_eq!(record.collision_time, 100);
        assert_eq!(record.boxing_time, 2000);
        // Alerted company refreshes the span from the state.
        let mut record = PolicePursuitRecord::new(7);
        record.state = SHVSTATE_ARREST;
        record.alerted = true;
        assert_eq!(
            record.update_wanted_clock(330, false, true),
            WantedClockAction::RefreshSchedule {
                announce_full_bar: true
            }
        );
        assert_eq!(record.wanted_time, 40000);
        assert_eq!(record.wanted_time_start, 40000);
        let mut record = PolicePursuitRecord::new(7);
        record.state = SHVSTATE_KILL;
        record.alerted = true;
        assert_eq!(
            record.update_wanted_clock(330, false, false),
            WantedClockAction::RefreshSchedule {
                announce_full_bar: false
            }
        );
        assert_eq!(record.wanted_time, 60000);
        assert_eq!(record.wanted_time_start, 60000);
        // Angry company without alert stays steady and keeps the span.
        let mut record = PolicePursuitRecord::new(7);
        record.state = 1;
        record.wanted_time = 7000;
        assert_eq!(
            record.update_wanted_clock(330, false, false),
            WantedClockAction::Steady
        );
        assert_eq!(record.wanted_time, 7000);
    }

    #[test]
    fn player_progress_fraction_computes_hud_bar() {
        // Working name: Tick 0x49a300 HUD bar; inlined, no RVA claimed.
        let mut state = PoliceGameInitState::default();
        assert_eq!(state.player_progress_fraction(7), None);
        let mut record = PolicePursuitRecord::new(7);
        record.alerted = true;
        record.wanted_time = 10000;
        record.wanted_time_start = 20000;
        state.records.push(record);
        // Default timer is 0/200: left = 10000 - 0 over 20000 - 200.
        assert_eq!(
            state.player_progress_fraction(7),
            Some(10000.0f32 / 19800.0f32)
        );
        // A record at its start reports full; other humans report none.
        state.records[0].wanted_time = 20000;
        assert_eq!(state.player_progress_fraction(7), Some(1.0));
        assert_eq!(state.player_progress_fraction(9), None);
        // A non-alerted player record reports none.
        state.records[0].alerted = false;
        assert_eq!(state.player_progress_fraction(7), None);
        // A negative rest clamps at zero over a span floored at one.
        state.records[0].alerted = true;
        state.records[0].wanted_time = -50;
        state.records[0].wanted_time_start = 100;
        assert_eq!(state.player_progress_fraction(7), Some(0.0));
    }

    #[test]
    fn pending_raise_speeding_splits_arrest_and_level_one() {
        // Working name: CreateShvestkyTest speeding arm; no RVA claimed.
        let mut record = PolicePursuitRecord::new(7);
        record.raise_pending_for_speeding(Some(30.0), 28.0);
        assert_eq!(record.new_state, SHVSTATE_NONE);
        record.flags |= SHVF_SPEEDING;
        record.raise_pending_for_speeding(None, 28.0);
        assert_eq!(record.new_state, SHVSTATE_NONE);
        record.raise_pending_for_speeding(Some(20.0), 28.0);
        assert_eq!(record.new_state, 1);
        assert_eq!(record.reason, 2);
        record.raise_pending_for_speeding(Some(30.0), 28.0);
        assert_eq!(record.new_state, SHVSTATE_ARREST);
        assert_eq!(record.reason, 2);
        // A higher pending state never steps down.
        record.new_state = SHVSTATE_KILL;
        record.reason = 9;
        record.raise_pending_for_speeding(Some(20.0), 28.0);
        assert_eq!(record.new_state, SHVSTATE_KILL);
        assert_eq!(record.reason, 9);
    }

    #[test]
    fn pending_raise_weapon_needs_range_aim_and_seat() {
        // Working name: CreateShvestkyTest weapon arm; no RVA claimed.
        let mut record = PolicePursuitRecord::new(7);
        record.flags |= SHVF_WEAPON;
        record.raise_pending_for_weapon(false, false, false, 0, 0);
        assert_eq!(record.new_state, SHVSTATE_ARREST);
        assert_eq!(record.reason, 0);
        // A raised state blocks the arm.
        let mut record = PolicePursuitRecord::new(7);
        record.flags |= SHVF_WEAPON;
        record.new_state = 1;
        record.raise_pending_for_weapon(false, false, false, 0, 0);
        assert_eq!(record.new_state, 1);
        // A non-ACTOR_CAR car and an unaimed hand raise nothing.
        let mut record = PolicePursuitRecord::new(7);
        record.flags |= SHVF_WEAPON;
        record.raise_pending_for_weapon(true, false, true, 0, 6);
        assert_eq!(record.new_state, SHVSTATE_NONE);
        let mut record = PolicePursuitRecord::new(7);
        record.flags |= SHVF_WEAPON;
        record.raise_pending_for_weapon(true, true, false, 0, 6);
        assert_eq!(record.new_state, SHVSTATE_NONE);
        // The driver seat takes 6-9; a taken seat takes 6-14.
        let mut record = PolicePursuitRecord::new(7);
        record.flags |= SHVF_WEAPON;
        record.raise_pending_for_weapon(true, true, true, 0, 9);
        assert_eq!(record.new_state, SHVSTATE_ARREST);
        let mut record = PolicePursuitRecord::new(7);
        record.flags |= SHVF_WEAPON;
        record.raise_pending_for_weapon(true, true, true, 0, 10);
        assert_eq!(record.new_state, SHVSTATE_NONE);
        record.raise_pending_for_weapon(true, true, true, 1, 10);
        assert_eq!(record.new_state, SHVSTATE_ARREST);
    }

    #[test]
    fn pending_raise_car_steal_takes_arrest_once() {
        // Working name: CreateShvestkyTest car-steal arm; no RVA claimed.
        let mut record = PolicePursuitRecord::new(7);
        record.raise_pending_for_car_steal();
        assert_eq!(record.new_state, SHVSTATE_NONE);
        record.flags |= SHVF_CAR_STEAL;
        record.raise_pending_for_car_steal();
        assert_eq!((record.new_state, record.reason), (SHVSTATE_ARREST, 7));
        record.new_state = SHVSTATE_KILL;
        record.raise_pending_for_car_steal();
        assert_eq!(record.new_state, SHVSTATE_KILL);
    }

    #[test]
    fn pending_raise_illegal_path_needs_time_past_5800() {
        // Working name: CreateShvestkyTest illegal-path arm; no RVA claimed.
        let mut record = PolicePursuitRecord::new(7);
        record.flags |= SHVF_ILLEGAL_PATH;
        record.illegal_path_time = 5800;
        record.raise_pending_for_illegal_path();
        assert_eq!(record.new_state, SHVSTATE_NONE);
        record.illegal_path_time = 5801;
        record.raise_pending_for_illegal_path();
        assert_eq!((record.new_state, record.reason), (1, 4));
        record.new_state = SHVSTATE_ARREST;
        record.raise_pending_for_illegal_path();
        assert_eq!(record.new_state, SHVSTATE_ARREST);
    }

    #[test]
    fn pending_raise_force_arrest_takes_arrest_without_reason() {
        // Working name: CreateShvestkyTest force-arrest arm; no RVA claimed.
        let mut record = PolicePursuitRecord::new(7);
        record.raise_pending_for_force_arrest();
        assert_eq!(record.new_state, SHVSTATE_NONE);
        record.flags |= SHVF_FORCE_ARREST;
        record.raise_pending_for_force_arrest();
        assert_eq!(record.new_state, SHVSTATE_ARREST);
        assert_eq!(record.reason, 0);
    }

    #[test]
    fn pending_raise_hit_policeman_takes_kill() {
        // Working name: CreateShvestkyTest patrolman-hit arm; no RVA claimed.
        let mut record = PolicePursuitRecord::new(7);
        record.flags |= SHVF_HIT_POLICEMAN;
        record.raise_pending_for_hit_policeman();
        assert_eq!((record.new_state, record.reason), (SHVSTATE_KILL, 9));
        record.reason = 0;
        record.raise_pending_for_hit_policeman();
        assert_eq!(record.new_state, SHVSTATE_KILL);
        assert_eq!(record.reason, 0);
    }

    #[test]
    fn pending_raise_ram_splits_hard_and_soft() {
        // Working name: CreateShvestkyTest police-ram arm; no RVA claimed.
        let mut record = PolicePursuitRecord::new(7);
        record.flags |= SHVF_RAM_POLICE_CAR;
        record.raise_pending_for_ram_police_car(false);
        assert_eq!((record.new_state, record.reason), (1, 0xa));
        let mut record = PolicePursuitRecord::new(7);
        record.flags |= SHVF_RAM_POLICE_CAR;
        record.raise_pending_for_ram_police_car(true);
        assert_eq!((record.new_state, record.reason), (SHVSTATE_ARREST, 0xa));
        record.new_state = SHVSTATE_KILL;
        record.reason = 9;
        record.raise_pending_for_ram_police_car(true);
        assert_eq!((record.new_state, record.reason), (SHVSTATE_KILL, 9));
    }
}
