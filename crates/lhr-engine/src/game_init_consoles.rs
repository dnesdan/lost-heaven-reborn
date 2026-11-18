//! The two C_game consoles, their owned text, and their registration owner.
//!
//! Reference: mafia-re fe7e74643f5415c358aa312dd4e7d9da5f60ddcc,
//! game/ui/console.cpp and game/mission/game.cpp. Primary static REA/Ghidra
//! evidence: Game.exe SHA-256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! Clear RVA 0xf5ed0 (ev_b1bdb12e...), ConsoleReg RVA 0xf6590
//! (ev_fff2b5cc...), constructor RVA 0xf5e20 (ev_4714ffba...), and
//! C_game::Init RVA 0x1a0810, span 0x1a0e52..=0x1a0f69 (ev_67ab357f...).
//! This is bounded owner/composition support, with no new function credit.
//! Rendering, Tick, OutText, ConsoleUnReg, and the rest of Init are not supplied.
#![forbid(unsafe_code)]

use std::ffi::CStr;
use std::fmt;

/// A selector within one GameInitConsoles owner, never a native pointer or a
/// handle to a console belonging to another owner. The source's numbered names
/// for these two game statics are researcher-assigned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsoleSlot {
    One,
    Two,
}

/// PC dword color components; a line uses -1 for the console's component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConsoleColor {
    pub r: i32,
    pub g: i32,
    pub b: i32,
}

/// Typed text storage only. Inserting a record here does not implement OutText's
/// replacement, copying, or line-position rules. The optional owned C string
/// represents the original new[] text, including delete[]'s null case.
#[derive(Debug)]
pub struct ConsoleLine {
    pub text: Option<Box<CStr>>,
    pub color: ConsoleColor,
    pub id: i32,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConsoleConfiguration {
    pub color: ConsoleColor,
    pub x: f32,
    pub y: f32,
    pub character_height: f32,
    pub line_spacing: f32,
    /// FIDIV dword in PC C_console::Tick makes this a signed 32-bit divisor.
    pub line_limit: i32,
    /// The stored 32-bit int; Tick compares its bits to elapsed as unsigned.
    pub line_time_ms: i32,
    pub upwards: bool,
    pub right_aligned: bool,
    pub reversed: bool,
}

#[derive(Debug)]
pub struct Console {
    pub text_lines: Vec<ConsoleLine>,
    pub configuration: ConsoleConfiguration,
    /// Clear's dword store; Tick's unsigned comparison uses all 32 bits.
    pub elapsed: u32,
}

impl Console {
    /// Only the game's zero-initialized statics are constructed here. Native
    /// C_console's constructor leaves elapsed unwritten; their process storage
    /// supplies its initial zero. This is not a claim about arbitrary native
    /// heap-allocated consoles or VC6 vector allocator/layout equivalence.
    fn game_static() -> Self {
        Self {
            text_lines: Vec::new(),
            configuration: ConsoleConfiguration {
                color: ConsoleColor {
                    r: 255,
                    g: 255,
                    b: 255,
                },
                x: f32::from_bits(0x3dcc_cccd),
                y: f32::from_bits(0x3dcc_cccd),
                character_height: f32::from_bits(0x3dcc_cccd),
                line_spacing: f32::from_bits(0x3de1_47ae),
                line_limit: 100,
                line_time_ms: 1000,
                upwards: false,
                right_aligned: false,
                reversed: false,
            },
            elapsed: 0,
        }
    }

    /// C_console::Clear, PC RVA 0xf5ed0: release text last-to-first, retain
    /// vector storage, then zero elapsed. Configuration is not reset.
    pub fn clear(&mut self) {
        for line in self.text_lines.iter_mut().rev() {
            drop(line.text.take());
        }
        self.text_lines.clear();
        self.elapsed = 0;
    }
}

/// ConsoleReg stores the full dword, including bits unknown to this fragment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConsoleFlags(u32);

impl ConsoleFlags {
    pub const CLEAR: Self = Self(1);
    pub const RENDER: Self = Self(2);

    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConsoleRegistration {
    pub console: ConsoleSlot,
    pub flags: ConsoleFlags,
    /// ConsoleReg uses INC dword, including wrapping at the signed boundary.
    pub count: i32,
}

/// All required host operations have no default implementation. The adapter
/// must borrow the session's actual ModelArena and existing random owner.
pub trait DriverAndConsolesHost {
    type TimeError;

    /// Read the live arena's normalized RS_FAST_FIND gate.
    fn fast_find_enabled(&mut self) -> bool;
    fn set_fast_find_enabled(&mut self, enabled: bool);

    /// Called after disabling fast-find. Return the exact 32 argument bits of
    /// srand(time(NULL)); PC Init pushes EAX unchanged into srand (RVA 0x210787,
    /// ev_7b6b666b...). Native time's failure value is still a
    /// value: 0xffff_ffff must seed the stream, not produce Err or a fallback.
    /// Err is reserved for an unavailable host dependency.
    fn time_seed(&mut self) -> Result<u32, Self::TimeError>;

    /// Delegate to the retained GameInlineState::seed_random / GameRandom::seed.
    /// Do not create or clone a random stream for this operation.
    fn seed_game_random(&mut self, seed: u32);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SavedFastFind {
    pub enabled: bool,
}

/// The disabled arena gate is a reached effect even when host time is missing.
/// The caller must retain saved_fast_find for the later native restore point;
/// there is no automatic rollback or implicit replay of this span.
#[derive(Debug, Eq, PartialEq)]
pub struct DriverAndConsolesError<E> {
    pub saved_fast_find: SavedFastFind,
    pub source: E,
}

impl<E: fmt::Display> fmt::Display for DriverAndConsolesError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "time(NULL) host dependency failed after disabling fast-find: {}",
            self.source
        )
    }
}

impl<E: std::error::Error + 'static> std::error::Error for DriverAndConsolesError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// The registrations select the actual two consoles held here. There is no
/// parallel native-address map, detached console projection, or private RNG.
#[derive(Debug)]
pub struct GameInitConsoles {
    console_1: Console,
    console_2: Console,
    registrations: Vec<ConsoleRegistration>,
}

impl Default for GameInitConsoles {
    fn default() -> Self {
        Self {
            console_1: Console::game_static(),
            console_2: Console::game_static(),
            registrations: Vec::new(),
        }
    }
}

impl GameInitConsoles {
    pub fn console(&self, slot: ConsoleSlot) -> &Console {
        match slot {
            ConsoleSlot::One => &self.console_1,
            ConsoleSlot::Two => &self.console_2,
        }
    }

    pub fn console_mut(&mut self, slot: ConsoleSlot) -> &mut Console {
        match slot {
            ConsoleSlot::One => &mut self.console_1,
            ConsoleSlot::Two => &mut self.console_2,
        }
    }

    pub fn registrations(&self) -> &[ConsoleRegistration] {
        &self.registrations
    }

    /// ConsoleReg, PC RVA 0xf6590: reverse lookup, increment an existing entry
    /// without replacing its flags, or append one registration with count 1.
    pub fn console_reg(&mut self, console: ConsoleSlot, flags: ConsoleFlags) {
        if let Some(existing) = self
            .registrations
            .iter_mut()
            .rev()
            .find(|r| r.console == console)
        {
            existing.count = existing.count.wrapping_add(1);
            return;
        }
        self.registrations.push(ConsoleRegistration {
            console,
            flags,
            count: 1,
        });
    }

    /// Exact C_game::Init span ending before segments.Load. Keep this owner
    /// across Init/Done; resetting the registry here would lose native counts.
    pub fn driver_and_consoles<H: DriverAndConsolesHost>(
        &mut self,
        host: &mut H,
    ) -> Result<SavedFastFind, DriverAndConsolesError<H::TimeError>> {
        let saved = SavedFastFind {
            enabled: host.fast_find_enabled(),
        };
        host.set_fast_find_enabled(false);
        let seed = host.time_seed().map_err(|source| DriverAndConsolesError {
            saved_fast_find: saved,
            source,
        })?;
        host.seed_game_random(seed);

        self.console_1.clear();
        let configuration = &mut self.console_1.configuration;
        configuration.color.r = 255;
        configuration.color.g = 255;
        configuration.color.b = 255;
        configuration.x = f32::from_bits(0x3c23_d70a);
        configuration.y = f32::from_bits(0x3f63_d70a);
        configuration.character_height = f32::from_bits(0x3d0f_5c29);
        configuration.line_spacing = f32::from_bits(0x3ce5_6042);
        configuration.upwards = true;
        configuration.right_aligned = false;
        configuration.reversed = false;
        configuration.line_limit = 20;
        configuration.line_time_ms = 10_000;
        self.console_reg(ConsoleSlot::One, ConsoleFlags::RENDER);

        self.console_2.clear();
        let configuration = &mut self.console_2.configuration;
        configuration.color.r = 128;
        configuration.color.g = 255;
        configuration.color.b = 128;
        configuration.x = f32::from_bits(0x3c23_d70a);
        configuration.y = f32::from_bits(0x3ee6_6666);
        configuration.character_height = f32::from_bits(0x3d1b_a5e3);
        configuration.line_spacing = f32::from_bits(0x3ce5_6042);
        configuration.upwards = false;
        configuration.right_aligned = false;
        configuration.reversed = false;
        configuration.line_limit = 20;
        // The native second-console span has no line_time_ms store.
        self.console_reg(
            ConsoleSlot::Two,
            ConsoleFlags::CLEAR.union(ConsoleFlags::RENDER),
        );
        Ok(saved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_housekeeping::GameRandom;
    use std::ffi::CString;

    #[derive(Debug, Eq, PartialEq)]
    enum Effect {
        ReadFastFind(bool),
        WriteFastFind(bool),
        ReadTime,
        Seed(u32),
    }

    struct Host {
        fast_find: bool,
        time: Result<u32, &'static str>,
        random: GameRandom,
        effects: Vec<Effect>,
    }

    impl DriverAndConsolesHost for Host {
        type TimeError = &'static str;

        fn fast_find_enabled(&mut self) -> bool {
            self.effects.push(Effect::ReadFastFind(self.fast_find));
            self.fast_find
        }
        fn set_fast_find_enabled(&mut self, enabled: bool) {
            self.effects.push(Effect::WriteFastFind(enabled));
            self.fast_find = enabled;
        }
        fn time_seed(&mut self) -> Result<u32, Self::TimeError> {
            assert!(!self.fast_find);
            self.effects.push(Effect::ReadTime);
            self.time
        }
        fn seed_game_random(&mut self, seed: u32) {
            self.effects.push(Effect::Seed(seed));
            self.random.seed(seed);
        }
    }

    fn host(time: Result<u32, &'static str>) -> Host {
        Host {
            fast_find: true,
            time,
            random: GameRandom::default(),
            effects: Vec::new(),
        }
    }

    fn line(text: &[u8]) -> ConsoleLine {
        ConsoleLine {
            text: Some(CString::new(text).unwrap().into_boxed_c_str()),
            color: ConsoleColor {
                r: -1,
                g: -1,
                b: -1,
            },
            id: -1,
            x: -1.0,
            y: -1.0,
        }
    }

    fn float_bits(configuration: &ConsoleConfiguration) -> [u32; 4] {
        [
            configuration.x.to_bits(),
            configuration.y.to_bits(),
            configuration.character_height.to_bits(),
            configuration.line_spacing.to_bits(),
        ]
    }

    #[test]
    fn static_consoles_have_observed_constructor_values_and_zero_storage_elapsed() {
        let owner = GameInitConsoles::default();
        assert!(owner.registrations().is_empty());
        for slot in [ConsoleSlot::One, ConsoleSlot::Two] {
            let console = owner.console(slot);
            assert!(console.text_lines.is_empty());
            assert_eq!(console.elapsed, 0);
            assert_eq!(
                console.configuration.color,
                ConsoleColor {
                    r: 255,
                    g: 255,
                    b: 255
                }
            );
            assert_eq!(
                float_bits(&console.configuration),
                [0x3dcc_cccd, 0x3dcc_cccd, 0x3dcc_cccd, 0x3de1_47ae]
            );
            assert_eq!(
                (
                    console.configuration.line_limit,
                    console.configuration.line_time_ms
                ),
                (100, 1000)
            );
            assert!(!console.configuration.upwards);
            assert!(!console.configuration.right_aligned);
            assert!(!console.configuration.reversed);
        }
    }

    #[test]
    fn clear_owns_byte_text_retains_capacity_and_preserves_configuration() {
        let mut owner = GameInitConsoles::default();
        let console = owner.console_mut(ConsoleSlot::One);
        console.text_lines.reserve(8);
        console
            .text_lines
            .extend([line(&[0x80, 0xff]), line(b"last")]);
        assert_eq!(
            console.text_lines[0].text.as_ref().unwrap().to_bytes(),
            &[0x80, 0xff]
        );
        console.elapsed = u32::MAX;
        console.configuration.line_time_ms = -7;
        console.configuration.line_limit = -3;
        console.configuration.right_aligned = true;
        let configuration = console.configuration;
        let capacity = console.text_lines.capacity();
        console.clear();
        assert!(console.text_lines.is_empty());
        assert_eq!(console.text_lines.capacity(), capacity);
        assert_eq!(console.elapsed, 0);
        assert_eq!(console.configuration, configuration);
        console.clear();
        assert_eq!(console.text_lines.capacity(), capacity);
        assert_eq!(console.configuration, configuration);
    }

    #[test]
    fn console_reg_keeps_insertion_order_original_flags_and_live_console_text() {
        let mut owner = GameInitConsoles::default();
        owner
            .console_mut(ConsoleSlot::One)
            .text_lines
            .push(line(b"keep"));
        owner.console_mut(ConsoleSlot::One).elapsed = 123;
        let original = ConsoleFlags::from_bits(0x8000_0010);
        owner.console_reg(ConsoleSlot::One, original);
        owner.console_reg(ConsoleSlot::Two, ConsoleFlags::CLEAR);
        owner.console_reg(ConsoleSlot::One, ConsoleFlags::RENDER);
        assert_eq!(
            owner.registrations(),
            &[
                ConsoleRegistration {
                    console: ConsoleSlot::One,
                    flags: original,
                    count: 2
                },
                ConsoleRegistration {
                    console: ConsoleSlot::Two,
                    flags: ConsoleFlags::CLEAR,
                    count: 1
                },
            ]
        );
        assert_eq!(owner.registrations()[0].flags.bits(), 0x8000_0010);
        assert_eq!(owner.console(ConsoleSlot::One).elapsed, 123);
        assert_eq!(
            owner.console(ConsoleSlot::One).text_lines[0]
                .text
                .as_ref()
                .unwrap()
                .to_bytes(),
            b"keep"
        );
    }

    #[test]
    fn registration_count_wraps_as_the_observed_dword_increment() {
        let mut owner = GameInitConsoles::default();
        owner.console_reg(ConsoleSlot::One, ConsoleFlags::RENDER);
        owner.registrations[0].count = i32::MAX;
        owner.console_reg(ConsoleSlot::One, ConsoleFlags::CLEAR);
        assert_eq!(owner.registrations()[0].count, i32::MIN);
        assert_eq!(owner.registrations()[0].flags, ConsoleFlags::RENDER);
    }

    #[test]
    fn native_span_seeds_existing_consumers_before_clearing_and_configuring_both_consoles() {
        let mut owner = GameInitConsoles::default();
        for slot in [ConsoleSlot::One, ConsoleSlot::Two] {
            let console = owner.console_mut(slot);
            console.text_lines.push(line(b"old"));
            console.elapsed = 456;
            console.configuration.right_aligned = true;
            console.configuration.reversed = true;
        }
        owner
            .console_mut(ConsoleSlot::Two)
            .configuration
            .line_time_ms = -77;
        let mut host = host(Ok(0));
        let mut existing_consumer = host.random.source();
        assert_eq!(existing_consumer.next_raw(), Some(41));
        let saved = owner.driver_and_consoles(&mut host).unwrap();
        assert_eq!(saved, SavedFastFind { enabled: true });
        assert!(!host.fast_find);
        assert_eq!(
            host.effects,
            [
                Effect::ReadFastFind(true),
                Effect::WriteFastFind(false),
                Effect::ReadTime,
                Effect::Seed(0)
            ]
        );
        assert_eq!(existing_consumer.next_raw(), Some(38));
        let first = owner.console(ConsoleSlot::One);
        let second = owner.console(ConsoleSlot::Two);
        assert_eq!(
            first.configuration.color,
            ConsoleColor {
                r: 255,
                g: 255,
                b: 255
            }
        );
        assert_eq!(
            second.configuration.color,
            ConsoleColor {
                r: 128,
                g: 255,
                b: 128
            }
        );
        assert_eq!(
            float_bits(&first.configuration),
            [0x3c23_d70a, 0x3f63_d70a, 0x3d0f_5c29, 0x3ce5_6042]
        );
        assert_eq!(
            float_bits(&second.configuration),
            [0x3c23_d70a, 0x3ee6_6666, 0x3d1b_a5e3, 0x3ce5_6042]
        );
        assert!(first.configuration.upwards);
        assert!(!second.configuration.upwards);
        assert_eq!(
            (
                first.configuration.line_limit,
                second.configuration.line_limit
            ),
            (20, 20)
        );
        assert_eq!(
            (
                first.configuration.line_time_ms,
                second.configuration.line_time_ms
            ),
            (10_000, -77)
        );
        for console in [first, second] {
            assert!(console.text_lines.is_empty());
            assert_eq!(console.elapsed, 0);
            assert!(!console.configuration.right_aligned);
            assert!(!console.configuration.reversed);
        }
        assert_eq!(
            owner.registrations(),
            &[
                ConsoleRegistration {
                    console: ConsoleSlot::One,
                    flags: ConsoleFlags::RENDER,
                    count: 1
                },
                ConsoleRegistration {
                    console: ConsoleSlot::Two,
                    flags: ConsoleFlags::from_bits(3),
                    count: 1
                },
            ]
        );
    }

    #[test]
    fn missing_host_time_preserves_reached_gate_effect_without_rng_or_console_effects() {
        let mut owner = GameInitConsoles::default();
        owner
            .console_mut(ConsoleSlot::One)
            .text_lines
            .push(line(b"keep one"));
        owner
            .console_mut(ConsoleSlot::Two)
            .text_lines
            .push(line(b"keep two"));
        owner.console_mut(ConsoleSlot::One).elapsed = 45;
        owner.console_mut(ConsoleSlot::Two).elapsed = 67;
        owner.console_reg(ConsoleSlot::Two, ConsoleFlags::from_bits(0x10));
        let first_configuration = owner.console(ConsoleSlot::One).configuration;
        let second_configuration = owner.console(ConsoleSlot::Two).configuration;
        let mut host = host(Err("time owner unavailable"));
        let state = host.random.state();
        let failure = owner.driver_and_consoles(&mut host).unwrap_err();
        assert_eq!(failure.saved_fast_find, SavedFastFind { enabled: true });
        assert_eq!(failure.source, "time owner unavailable");
        assert!(!host.fast_find);
        assert_eq!(
            host.effects,
            [
                Effect::ReadFastFind(true),
                Effect::WriteFastFind(false),
                Effect::ReadTime
            ]
        );
        assert_eq!(host.random.state(), state);
        assert_eq!(
            owner.console(ConsoleSlot::One).configuration,
            first_configuration
        );
        assert_eq!(
            owner.console(ConsoleSlot::Two).configuration,
            second_configuration
        );
        assert_eq!(owner.console(ConsoleSlot::One).elapsed, 45);
        assert_eq!(owner.console(ConsoleSlot::Two).elapsed, 67);
        assert_eq!(
            owner.console(ConsoleSlot::One).text_lines[0]
                .text
                .as_ref()
                .unwrap()
                .to_bytes(),
            b"keep one"
        );
        assert_eq!(
            owner.console(ConsoleSlot::Two).text_lines[0]
                .text
                .as_ref()
                .unwrap()
                .to_bytes(),
            b"keep two"
        );
        assert_eq!(
            owner.registrations(),
            &[ConsoleRegistration {
                console: ConsoleSlot::Two,
                flags: ConsoleFlags::from_bits(0x10),
                count: 1
            },]
        );
    }

    #[test]
    fn native_time_failure_bits_seed_without_a_fallback_and_false_gate_is_retained() {
        let mut owner = GameInitConsoles::default();
        let mut host = host(Ok(u32::MAX));
        host.fast_find = false;
        let mut existing_consumer = host.random.source();
        assert!(!owner.driver_and_consoles(&mut host).unwrap().enabled);
        assert_eq!(existing_consumer.next_raw(), Some(35));
        assert_eq!(
            host.effects,
            [
                Effect::ReadFastFind(false),
                Effect::WriteFastFind(false),
                Effect::ReadTime,
                Effect::Seed(u32::MAX)
            ]
        );
    }

    #[test]
    fn repeated_native_span_retains_registration_counts_and_preexisting_flags() {
        let mut owner = GameInitConsoles::default();
        owner.console_reg(ConsoleSlot::Two, ConsoleFlags::from_bits(0x40));
        owner.console_reg(ConsoleSlot::One, ConsoleFlags::CLEAR);
        let mut host = host(Ok(1));
        assert!(owner.driver_and_consoles(&mut host).unwrap().enabled);
        assert!(!owner.driver_and_consoles(&mut host).unwrap().enabled);
        assert_eq!(
            owner.registrations(),
            &[
                ConsoleRegistration {
                    console: ConsoleSlot::Two,
                    flags: ConsoleFlags::from_bits(0x40),
                    count: 3
                },
                ConsoleRegistration {
                    console: ConsoleSlot::One,
                    flags: ConsoleFlags::CLEAR,
                    count: 3
                },
            ]
        );
    }
}
