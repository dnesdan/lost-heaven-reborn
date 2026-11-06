//! `C_driver::Tick` leading pure-state prefix (Game.exe chase AI, NOT LS3DF).
//!
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000. Original entry VA 0x43c4f0 / RVA 0x3c4f0, evidenced name
//! `C_driver::Tick` (mafia-re `game/ai/C_driver.h:73`, `game/ai/C_driver.cpp:16`
//! @ 113b5b6; the mafia-re body is a stub, so no Tick semantics come from it).
//! The state fields below come from the evidenced header instead
//! (`game/ai/C_driver.h:45-68`: `path` vector, `next_node` +0x170, the ms time
//! word `unknown_220` +0x220, `flags` +0x1dc).
//!
//! Native evidence re-observed read-only in the authoring probe session
//! (closed; recorded here, not re-observed in review; no decompiler output
//! is trusted for runtime behavior; the decode below is a read-only capstone
//! projection of observed entry bytes, used only to order the ported effects):
//! REA `analyze_function` 0x43c4f0 parent ev_92c7676ef333da2b6eae2222ed0c273f42851e5f5f1cdb0d10fdbde4a73d8f73
//! with summary ev_378af46a02176159807f7927628b161012345e7f6924a9138eab7deb3d31bec0
//! (`FUN_0043c4f0`, non-thunk, body 0x43c4f0-0x43f043 contiguous 11092 bytes,
//! 2881 instructions / 339 basic blocks / 43 callees / 4 callers / 1 parameter)
//! plus entry `read_bytes` 0x43c4f0+160B
//! ev_7215919d2bc144d2664ec3bb20f553e46df8ae8d96c85ebc4594fdfc66fcaaeb.
//! The full 11KB body is far too large for one prefix; this module ports the
//! largest coherent leading slice 0x43c4f0-0x43c571 and halts at the first
//! world effect, the `CALL 0x4cb710` at 0x43c572 (subject+0x70 method, one
//! zero argument). Everything from that call on (frame/physics/collision,
//! the 0x43c577 flag reads at +0x1dc/+0x19c and zero stores at +0x1c4/+0x1bc,
//! and the whole remaining ~10.7KB incl. the 0x43f035 guard-convergence tail)
//! is DEFERRED with explicit markers below and gets no function credit.
//!
//! Ported slice, in native order (thiscall, ECX = driver, stack `time: u32`):
//! 1. `MOV BYTE [subject+0xAC],0` — sole external write of the slice, routed
//!    through [`AiDriverTickBackend`]. The native code dereferences the +0x10
//!    word unconditionally, so a null subject would fault natively; the port
//!    treats a missing subject as an explicit host precondition and skips the
//!    backend call instead of faulting (documented divergence, pinned by test).
//! 2. Path-size snapshot into +0x64: native computes
//!    `([live+8]-[live+4])/0x70` with the 0x92492493 IMUL + SAR 6 + bias
//!    sequence (full signed division by 112 = `sizeof(S_car_node)`), yielding
//!    0 for the null/empty vector. Note the live pointers sit at +0x164/+0x168
//!    (4 bytes past the header's `path` offset 0x160); the port keeps no raw
//!    layout and stores `path.len()` as the dword instead. The store precedes
//!    all three guards, so early exits still publish the snapshot.
//! 3. Guards, each converging (unverified tail) at 0x43f035: snapshot == 0,
//!    then `next_node >= snapshot` (both unsigned compares), then a null
//!    subject re-test (unreachable after step 1 natively unless a callback
//!    nulled it; kept as defensive order fidelity).
//! 4. `time_accum (+0x21C) += time` with x86 wrapping `ADD` semantics. This is
//!    4 bytes before the header's `unknown_220` ms-time word, so the working
//!    name below is deliberately distinct from it.
//!
//! Name-collision note: `lhr_engine::driver_lifecycle::driver_tick` is the
//! LS3DF `I3D_driver::Tick` (RVA 0x15730, wrapping int add) and is NOT this
//! function. The symbol here carries the `ai_` qualifier so the two owners
//! can never be confused; the overlay entry must key on RVA 0x3c4f0.
//!
//! All member names except the evidenced `Tick`, `path` and `next_node` are
//! working names (NOT original). Unknown native fields acquire no invented
//! meanings; unrecovered bytes are simply absent from this owner.
//!
//! Precision policy: all arithmetic is integer (`u32` wrapping add, `usize`
//! length truncated to the native dword); no float, no host f64, no fused ops.
//! The tests in this module are hand-derived vectors authored without cargo
//! (no execution is claimed).

#![forbid(unsafe_code)]

/// `sizeof(S_car_node)` in bytes, evidenced by the native /112 magic divide
/// (IMUL 0x92492493, SAR 6, bias) over the live path-vector word pair.
pub const DRIVER_PATH_NODE_BYTES: usize = 0x70;

/// One path node. Content is unknown (mafia-re `S_car_node` members are
/// guesses except the layout size), so the element stays opaque; the prefix
/// only ever counts elements, never reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiDriverPathNode([u8; DRIVER_PATH_NODE_BYTES]);

impl AiDriverPathNode {
    /// Supply an opaque node without imposing any member interpretation.
    pub const fn from_bytes(bytes: [u8; DRIVER_PATH_NODE_BYTES]) -> Self {
        Self(bytes)
    }

    /// Raw node bytes, e.g. for future differential harnesses.
    pub const fn as_bytes(&self) -> &[u8; DRIVER_PATH_NODE_BYTES] {
        &self.0
    }
}

/// Opaque identity of the driven subject (native +0x10 word; working name —
/// likely the driven car, NOT proven: the constructor binding was not
/// re-observed this session). The word is never a host address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiDriverSubject {
    word: u32,
}

impl AiDriverSubject {
    /// Interpret a raw +0x10 word: zero is the native null, anything else is
    /// an opaque live identity.
    pub const fn from_word(word: u32) -> Option<Self> {
        if word == 0 {
            None
        } else {
            Some(Self { word })
        }
    }

    /// The opaque native identity word back.
    pub const fn word(self) -> u32 {
        self.word
    }
}

/// Typed driver-tick owner for the ported leading slice. One authoritative
/// owner holds the subject identity, the live path, the +0x64 snapshot, the
/// +0x170 cursor and the +0x21C accumulator; later slices extend this struct,
/// they do not fork it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiDriverTickState {
    /// Native +0x10 subject pointer (working name, see [`AiDriverSubject`]).
    pub subject: Option<AiDriverSubject>,
    /// Native `path` vector (`C_driver.h:51`); live words observed at
    /// +0x164/+0x168, 4 bytes past the header offset.
    pub path: Vec<AiDriverPathNode>,
    /// Native +0x64 path-size snapshot dword (working name, NOT original).
    pub path_len_snapshot: u32,
    /// Native `next_node` +0x170 (`C_driver.h:52`).
    pub next_node: u32,
    /// Native +0x21C wrapping time accumulator (working name, NOT original;
    /// distinct from the header's `unknown_220` ms-time word at +0x220).
    pub time_accum: u32,
}

impl Default for AiDriverTickState {
    fn default() -> Self {
        Self {
            subject: None,
            path: Vec::new(),
            path_len_snapshot: 0,
            next_node: 0,
            time_accum: 0,
        }
    }
}

/// World-effect boundary for the slice. The only external write in
/// 0x43c4f0-0x43c571 is the subject flag clear; the `CALL 0x4cb710` halt and
/// everything after it have no backend here yet (DEFERRED, see
/// [`ai_driver_tick_prefix`]).
pub trait AiDriverTickBackend {
    /// Native `MOV BYTE [subject+0xAC],0` (working name; that byte's meaning
    /// is unknown). Runs first, before the snapshot store.
    fn clear_subject_byte(&mut self);
}

/// DEFERRED world effects at and after the 0x43c572 halt (no backend, no
/// credit): the `CALL 0x4cb710` subject+0x70 method itself (one zero
/// argument), the post-call flag reads (+0x1dc/+0x19c), the zero stores
/// (+0x1c4/+0x1bc), and the remaining ~10.7KB of steering/frame/collision
/// work including the 0x43f035 guard-convergence tail.
pub const AI_DRIVER_TICK_DEFERRED_FROM: u32 = 0x43c572;

/// `C_driver::Tick` leading prefix, native bytes 0x43c4f0-0x43c571.
///
/// Effect order matches the entry decode: subject-byte clear (via backend,
/// skipped only for the null subject that would fault natively), path-size
/// snapshot store, the three unsigned guards (empty path, cursor past end,
/// null subject), then the wrapping time accumulation. Any guard halts the
/// tick with the snapshot already published and no accumulation. The halt at
/// [`AI_DRIVER_TICK_DEFERRED_FROM`] is a plain return here; the deferred call
/// is never replayed automatically.
pub fn ai_driver_tick_prefix(
    state: &mut AiDriverTickState,
    time: u32,
    backend: &mut impl AiDriverTickBackend,
) {
    if state.subject.is_some() {
        backend.clear_subject_byte();
    }
    // Native dword store of the element count (byte-size/112 for 0x70-byte
    // nodes); path lengths fit u32 in practice, so the `as` cast is exact
    // for all inhabitable states.
    let snapshot = state.path.len() as u32;
    state.path_len_snapshot = snapshot;
    if snapshot == 0 {
        return;
    }
    if state.next_node >= snapshot {
        return;
    }
    if state.subject.is_none() {
        return;
    }
    state.time_accum = state.time_accum.wrapping_add(time);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct LogBackend {
        clears: u32,
    }

    impl AiDriverTickBackend for LogBackend {
        fn clear_subject_byte(&mut self) {
            self.clears += 1;
        }
    }

    fn node() -> AiDriverPathNode {
        AiDriverPathNode::from_bytes([0u8; DRIVER_PATH_NODE_BYTES])
    }

    fn state_with(
        subject_word: u32,
        path_len: usize,
        next_node: u32,
        accum: u32,
    ) -> AiDriverTickState {
        AiDriverTickState {
            subject: AiDriverSubject::from_word(subject_word),
            path: vec![node(); path_len],
            path_len_snapshot: 0xdead_beef,
            next_node,
            time_accum: accum,
        }
    }

    #[test]
    fn empty_path_publishes_zero_snapshot_and_skips_accum() {
        let mut state = state_with(0x77, 0, 0, 7);
        let mut backend = LogBackend { clears: 0 };
        ai_driver_tick_prefix(&mut state, 5, &mut backend);
        assert_eq!(backend.clears, 1, "subject clear runs before the guard");
        assert_eq!(state.path_len_snapshot, 0);
        assert_eq!(state.time_accum, 7, "halted tick must not accumulate");
    }

    #[test]
    fn cursor_past_end_publishes_snapshot_and_skips_accum() {
        for next_node in [2u32, 3, u32::MAX] {
            let mut state = state_with(0x77, 2, next_node, 9);
            let mut backend = LogBackend { clears: 0 };
            ai_driver_tick_prefix(&mut state, 4, &mut backend);
            assert_eq!(backend.clears, 1);
            assert_eq!(state.path_len_snapshot, 2);
            assert_eq!(state.time_accum, 9);
        }
    }

    #[test]
    fn cursor_at_last_node_still_ticks() {
        let mut state = state_with(0x77, 3, 2, 100);
        let mut backend = LogBackend { clears: 0 };
        ai_driver_tick_prefix(&mut state, 23, &mut backend);
        assert_eq!(state.path_len_snapshot, 3);
        assert_eq!(state.time_accum, 123);
    }

    #[test]
    fn null_subject_skips_backend_clear_but_stores_snapshot() {
        // Native would fault on the unconditional [subject+0xAC] dereference;
        // the port records the host-precondition skip and keeps guard order.
        let mut state = state_with(0, 2, 0, 11);
        let mut backend = LogBackend { clears: 0 };
        ai_driver_tick_prefix(&mut state, 6, &mut backend);
        assert_eq!(backend.clears, 0);
        assert_eq!(state.path_len_snapshot, 2);
        assert_eq!(state.time_accum, 11, "null-subject guard halts the tick");
    }

    #[test]
    fn time_accumulation_wraps_like_x86_add() {
        let mut state = state_with(0x77, 1, 0, 0xffff_fff0);
        let mut backend = LogBackend { clears: 0 };
        ai_driver_tick_prefix(&mut state, 0x20, &mut backend);
        assert_eq!(state.time_accum, 0x10);
        ai_driver_tick_prefix(&mut state, 0xffff_fff0, &mut backend);
        assert_eq!(state.time_accum, 0);
        assert_eq!(backend.clears, 2);
    }

    #[test]
    fn snapshot_tracks_path_length_as_native_dword() {
        for len in [1usize, 2, 3, 5, 17] {
            let mut state = state_with(0x77, len, 0, 0);
            let mut backend = LogBackend { clears: 0 };
            ai_driver_tick_prefix(&mut state, 0, &mut backend);
            assert_eq!(state.path_len_snapshot, len as u32);
            assert_eq!(state.time_accum, 0);
        }
    }

    #[test]
    fn subject_word_zero_is_null_and_element_span_is_evidence() {
        assert_eq!(AiDriverSubject::from_word(0), None);
        assert_eq!(
            AiDriverSubject::from_word(1).map(AiDriverSubject::word),
            Some(1)
        );
        assert_eq!(
            std::mem::size_of::<AiDriverPathNode>(),
            DRIVER_PATH_NODE_BYTES,
            "opaque element must stay 112B to match the native /112 divisor"
        );
    }
}
