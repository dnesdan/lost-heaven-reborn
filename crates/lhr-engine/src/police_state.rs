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
/// `SHVF_FORCE_ARREST` 0x4000: `ForceArrest`'s bit, `SHVSTATE_ARREST`
/// (mafia-re game/ai/C_shvestky.h:28).
pub const SHVF_FORCE_ARREST: u32 = 0x4000;

/// Ground-plane squared eviction distance for `TickDoRiciActors`
/// (150 m; mafia-re game/ai/C_shvestky.cpp:869, 876, 884, 896).
pub const DO_RICI_GROUND_DIST_SQ: f32 = 22500.0;

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
            hits_time: -1,
            hits: 0,
            run_over: 0,
            alerted: false,
            nevi: false,
            support_level: 0,
            rammed_car: 0,
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
}
