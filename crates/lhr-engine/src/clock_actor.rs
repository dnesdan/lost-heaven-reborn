//! C_clock derived storage for the original PC actor lifecycle (kind 34).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor VA0x56add0, Init0x56ae20, GameInit0x56ae90, GameDone0x56afd0,
//! AI0x56afe0, vtable VA0x625648, 0x88 bytes (PS2's 0xa0). Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Reference sibling
//! mafia-re @ 113b5b6 game/actors/c_clock.h:8 (vtable) + game/actors/c_clock.cpp:8-9
//! (constructor), :14-15 (Init), :31-32 (GameInit), :54-55 (GameDone), :73-74 (AI);
//! factory game/mission/Mission.cpp:211 (CreateActor GAME 0x53f7d0, ACTOR_CLOCK 0x22).
//! Offsets below are byte offsets into the derived object; the base C_actor
//! occupies 0x70. Member names follow mafia-re; the visual names Mala/Velka/
//! Sekundova are mafia-re's. No PS2 member layout is assumed.

/// Actual untouched allocation bytes of one 0x88-byte clock object. The
/// constructor stores only the vtable and kind marker; every other word is
/// admitted untouched allocation, not interpreted native state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClockSeed {
    pub bytes_00_87: [u8; 0x88],
}

/// Owned clock bytes after construction and Init/GameInit/AI effects. Words
/// the constructor stores are observable state, not interpreted native
/// pointers: hand words are frame identities, never host pointers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClockState {
    bytes: [u8; 0x88],
}

impl ClockState {
    pub(crate) fn allocation(seed: ClockSeed) -> Self {
        Self {
            bytes: seed.bytes_00_87,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn put_word(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(
            self.bytes[offset..offset + 4]
                .try_into()
                .expect("clock word"),
        )
    }

    fn float(&self, offset: usize) -> f32 {
        f32::from_le_bytes(
            self.bytes[offset..offset + 4]
                .try_into()
                .expect("clock float"),
        )
    }

    /// Original constructor body (mafia-re c_clock.cpp:8-9): only the base
    /// construction plus the vtable store and kind marker, which live on the
    /// base owner (see construct_clock). No derived word is stored.
    pub(crate) fn complete_constructor(&mut self) {}

    /// Original C_clock::Init hand words (mafia-re c_clock.cpp:14-29): the
    /// resolved Mala/Velka/Sekundova frame identities at +0x70/+0x74/+0x78.
    /// A missing second hand stores zero, matching the native null member.
    pub(crate) fn set_hand_frames(&mut self, hour: u32, minute: u32, second: u32) {
        self.put_word(0x70, hour);
        self.put_word(0x74, minute);
        self.put_word(0x78, second);
    }

    /// Hour-hand frame identity resolved by Init at +0x70; zero means none.
    pub fn hand_hour(&self) -> u32 {
        self.word(0x70)
    }

    /// Minute-hand frame identity resolved by Init at +0x74; zero means none.
    pub fn hand_minute(&self) -> u32 {
        self.word(0x74)
    }

    /// Second-hand frame identity resolved by Init at +0x78; zero means the
    /// clock lacks one, in which case AI skips the second-hand turn.
    pub fn hand_second(&self) -> u32 {
        self.word(0x78)
    }

    /// Milliseconds accumulated since the last minute hand turn (+0x7c).
    pub fn elapsed(&self) -> u32 {
        self.word(0x7c)
    }

    /// Hour-hand angle in radians (+0x80).
    pub fn angle_hour(&self) -> f32 {
        self.float(0x80)
    }

    /// Minute-hand angle in radians (+0x84).
    pub fn angle_minute(&self) -> f32 {
        self.float(0x84)
    }

    /// Original C_clock::GameInit derived math (mafia-re c_clock.cpp:31-52):
    /// zero the elapsed/angles, then set both hands from the computer's time.
    /// The caller supplies the GetLocalTime hour/minute words; SYSTEMTIME
    /// words are u16. Deferred to a future frame-effect owner (disclosed,
    /// not ported): base `C_actor::GameInit`, `unknown_65`, `GetLocalTime`
    /// sourcing, and the `SetDir`/`Update` applications for hour/minute and
    /// conditional second hands. The reset second-hand direction (0.0) and
    /// the hand-radian helpers expose computed values only; no caller
    /// applies them yet, and tick has no frame-backend wiring.
    pub(crate) fn game_init(&mut self, hour: u16, minute: u16) {
        use std::f32::consts::PI;
        // Float policy: host f32 with native strict comparisons verbatim
        // (hour/minute u16 inputs exactly representable; x87 extended
        // intermediates not established — disclosed).
        self.put_word(0x7c, 0);
        self.put_word(0x80, 0.0f32.to_bits());
        self.put_word(0x84, 0.0f32.to_bits());
        let angle_hour = (hour as u32 * 2) as f32 * PI / 12.0;
        let angle_minute = (minute as u32 * 2) as f32 * PI / 60.0;
        self.put_word(0x80, angle_hour.to_bits());
        self.put_word(0x84, angle_minute.to_bits());
    }

    /// Original C_clock::AI (mafia-re c_clock.cpp:73-96): add the tick time,
    /// turn the minute hand each minute past (and the hour hand each of its
    /// turns), and turn the second hand with the time. Unsigned wrap and the
    /// strict float comparisons are verbatim. Frame SetDir/Update effects stay
    /// with a future effect owner; their radian arguments are exposed by the
    /// hand-direction helpers.
    pub(crate) fn advance(&mut self, time: u32) {
        use std::f32::consts::PI;
        let elapsed = self.elapsed().wrapping_add(time);
        self.put_word(0x7c, elapsed);
        if elapsed > 60000 {
            self.put_word(0x7c, elapsed - 60000);
            let mut angle_minute = self.angle_minute() + 2.0 * PI / 60.0;
            if angle_minute > 2.0 * PI {
                angle_minute -= 2.0 * PI;
                let mut angle_hour = self.angle_hour() + PI / 12.0;
                if angle_hour > 2.0 * PI {
                    angle_hour -= 2.0 * PI;
                }
                self.put_word(0x80, angle_hour.to_bits());
            }
            self.put_word(0x84, angle_minute.to_bits());
        }
    }

    /// Hour-hand SetDir radian argument: PI minus the hour angle plus the
    /// minute share (mafia-re c_clock.cpp:44,79).
    pub fn hour_hand_direction(&self) -> f32 {
        use std::f32::consts::PI;
        PI - (self.angle_hour() + self.angle_minute() / 12.0)
    }

    /// Minute-hand SetDir radian argument: PI minus the minute angle
    /// (mafia-re c_clock.cpp:46,81).
    pub fn minute_hand_direction(&self) -> f32 {
        use std::f32::consts::PI;
        PI - self.angle_minute()
    }

    /// Second-hand SetDir radian argument used by AI: PI minus the elapsed
    /// share of the minute (mafia-re c_clock.cpp:86).
    pub fn second_hand_direction(&self) -> f32 {
        use std::f32::consts::PI;
        PI - (self.elapsed() as f32) * 2.0 * PI / 60000.0
    }

    /// Second-hand SetDir radian argument used by GameInit: the hand starts
    /// at 0 (mafia-re c_clock.cpp:49), not at the AI formula value.
    pub fn second_hand_reset_direction() -> f32 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constructed() -> ClockState {
        let mut state = ClockState::allocation(ClockSeed {
            bytes_00_87: [0xaa; 0x88],
        });
        state.complete_constructor();
        state
    }

    #[test]
    fn constructor_stores_no_derived_words() {
        let state = constructed();
        assert_eq!(state.bytes, [0xaa; 0x88]);
    }

    #[test]
    fn hand_words_and_zero_time_round_trip() {
        let mut state = constructed();
        state.set_hand_frames(0x10203040, 0x50607080, 0);
        assert_eq!(state.hand_hour(), 0x10203040);
        assert_eq!(state.hand_minute(), 0x50607080);
        assert_eq!(state.hand_second(), 0);
        state.game_init(0, 0);
        assert_eq!(state.elapsed(), 0);
        assert_eq!(state.angle_hour(), 0.0);
        assert_eq!(state.angle_minute(), 0.0);
        assert_eq!(state.minute_hand_direction(), std::f32::consts::PI);
        assert_eq!(state.hour_hand_direction(), std::f32::consts::PI);
        assert_eq!(ClockState::second_hand_reset_direction(), 0.0);
    }

    #[test]
    fn game_init_sets_hands_from_computer_time() {
        let mut state = constructed();
        state.game_init(3, 30);
        let epsilon = 1e-6;
        assert!((state.angle_hour() - std::f32::consts::FRAC_PI_2).abs() < epsilon);
        assert!((state.angle_minute() - std::f32::consts::PI).abs() < epsilon);
        assert_eq!(state.elapsed(), 0);
        let mut midnight = constructed();
        midnight.game_init(0, 0);
        assert_eq!(midnight.angle_hour(), 0.0);
        assert_eq!(midnight.angle_minute(), 0.0);
    }

    #[test]
    fn ai_turns_minute_hand_each_minute_past() {
        let mut state = constructed();
        state.game_init(0, 0);
        state.advance(60000);
        assert_eq!(state.elapsed(), 60000);
        assert_eq!(state.angle_minute(), 0.0);
        state.advance(1);
        assert_eq!(state.elapsed(), 1);
        let epsilon = 1e-6;
        assert!((state.angle_minute() - 2.0 * std::f32::consts::PI / 60.0).abs() < epsilon);
        assert_eq!(state.angle_hour(), 0.0);
    }

    #[test]
    fn ai_wraps_minute_and_hour_turns_verbatim() {
        use std::f32::consts::PI;
        let mut state = constructed();
        state.set_hand_frames(1, 2, 3);
        state.put_word(0x7c, 0);
        state.put_word(0x84, (2.0 * PI - PI / 60.0).to_bits());
        state.put_word(0x80, (2.0 * PI - PI / 24.0).to_bits());
        state.advance(60001);
        assert_eq!(state.elapsed(), 1);
        let epsilon = 1e-5;
        assert!((state.angle_minute() - PI / 60.0).abs() < epsilon);
        assert!(
            (state.angle_hour() - (2.0 * PI - PI / 24.0 + PI / 12.0 - 2.0 * PI)).abs() < epsilon
        );
    }

    #[test]
    fn ai_second_hand_follows_elapsed_share() {
        let mut state = constructed();
        state.game_init(10, 15);
        state.advance(15000);
        assert_eq!(state.elapsed(), 15000);
        let epsilon = 1e-5;
        assert!((state.second_hand_direction() - std::f32::consts::FRAC_PI_2).abs() < epsilon);
    }
}
