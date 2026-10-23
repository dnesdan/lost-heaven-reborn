//! Bounded Game replay dependencies, with descriptive working names.
//!
//! Game SHA256 `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`:
//! busy leaf RVA `0x19e2d0`, state setter RVA `0x1989a0` (owner virtual slot 0).
//! This module does not construct a replay owner, load resources, or run updates.

/// Identity of an existing replay owner in admitted 32-bit storage. This token
/// does not establish object construction or grant access to native host memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayOwnerId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidReplayOwner(pub u32);

impl ReplayOwnerId {
    pub fn new(address: u32) -> Result<Self, InvalidReplayOwner> {
        if address == 0 || !address.is_multiple_of(4) || address.checked_add(0x128).is_none() {
            return Err(InvalidReplayOwner(address));
        }
        Ok(Self(address))
    }
    pub fn address(self) -> u32 {
        self.0
    }
}

/// Reads have no side effects. Every nonnull address must resolve to live,
/// admitted storage or return an error, including storage invalidated by a call.
/// Addresses are opaque native identities, never dereferenced host pointers.
pub trait ReplayReadBackend {
    type Error;
    /// Original global at VA 0x647a58, independent of owner storage.
    fn global_countdown(&self) -> Result<u32, Self::Error>;
    fn read_word(&self, address: u32) -> Result<u32, Self::Error>;
}

pub trait ReplayBackend: ReplayReadBackend {
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error>;
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error>;
    /// Invoke the exact freshly resolved callee with this object and argument.
    /// Missing objects, tables and callees are errors. Synchronous mutations and
    /// reentry must operate on this same live backend, not copied collections.
    fn invoke(&mut self, object: u32, callee: u32, argument: u32) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayError<E> {
    Backend(E),
    MissingOwner,
    InvalidAddress(u32),
    InvalidVector { begin: u32, end: u32 },
    BudgetExhausted,
}

/// Full EAX result of the busy leaf. Only AL is the predicate: native upper
/// bytes retain the source word. A nonzero global never accesses the owner.
pub fn busy_return_word<B: ReplayReadBackend + ?Sized>(
    backend: &B,
    owner: Option<ReplayOwnerId>,
) -> Result<u32, ReplayError<B::Error>> {
    let global = backend.global_countdown().map_err(ReplayError::Backend)?;
    if global != 0 {
        return Ok((global & 0xffff_ff00) | 1);
    }
    let owner = owner.ok_or(ReplayError::MissingOwner)?;
    let state = backend
        .read_word(owner.0 + 0xe4)
        .map_err(ReplayError::Backend)?;
    Ok((state & 0xffff_ff00) | u32::from(state != 0 && state != 4))
}

pub fn is_busy<B: ReplayReadBackend + ?Sized>(
    backend: &B,
    owner: Option<ReplayOwnerId>,
) -> Result<bool, ReplayError<B::Error>> {
    Ok(busy_return_word(backend, owner)? as u8 != 0)
}

fn address<E>(base: u32, offset: u32) -> Result<u32, ReplayError<E>> {
    if base == 0 || !base.is_multiple_of(4) {
        return Err(ReplayError::InvalidAddress(base));
    }
    base.checked_add(offset)
        .ok_or(ReplayError::InvalidAddress(base))
}

fn element<B: ReplayReadBackend + ?Sized>(
    backend: &B,
    owner: ReplayOwnerId,
    vector_offset: u32,
    index: u32,
) -> Result<u32, ReplayError<B::Error>> {
    let base = backend
        .read_word(owner.0 + vector_offset)
        .map_err(ReplayError::Backend)?;
    let slot = address(base, index * 4)?;
    backend.read_word(slot).map_err(ReplayError::Backend)
}

fn call_element<B: ReplayBackend + ?Sized>(
    backend: &mut B,
    owner: ReplayOwnerId,
    vector_offset: u32,
    index: u32,
    slot: u32,
    argument: u32,
) -> Result<(), ReplayError<B::Error>> {
    // No count/null guard here: the native second callback rereads the slot
    // even when its predecessor has shrunk or replaced the vector.
    let object = element(backend, owner, vector_offset, index)?;
    let table = backend
        .read_word(address(object, 0)?)
        .map_err(ReplayError::Backend)?;
    let callee = backend
        .read_word(address(table, slot)?)
        .map_err(ReplayError::Backend)?;
    if callee == 0 {
        return Err(ReplayError::InvalidAddress(callee));
    }
    backend
        .invoke(object, callee, argument)
        .map_err(ReplayError::Backend)
}

/// Complete state setter within admitted storage and an explicit host work
/// limit. Always writes previous then current, including equal requests.
/// Changed 0/1/2 visit A then B; other states have no object effects.
///
/// Native arithmetic shifts a wrapping pointer difference and compares it
/// unsigned. Host admission requires aligned, ordered spans below 2 GiB;
/// malformed spans are rejected rather than represented as enormous vectors.
/// The budget counts visited objects, not callbacks. Each invocation, including
/// synchronous reentry, needs its own bound. Errors preserve the exact prefix.
pub fn set_state<B: ReplayBackend + ?Sized>(
    backend: &mut B,
    owner: ReplayOwnerId,
    requested: u32,
    remaining_visits: &mut usize,
) -> Result<(), ReplayError<B::Error>> {
    let old = backend
        .read_word(owner.0 + 0xe4)
        .map_err(ReplayError::Backend)?;
    backend
        .write_word(owner.0 + 0xe8, old)
        .map_err(ReplayError::Backend)?;
    let previous = backend
        .read_word(owner.0 + 0xe8)
        .map_err(ReplayError::Backend)?;
    backend
        .write_word(owner.0 + 0xe4, requested)
        .map_err(ReplayError::Backend)?;
    if previous == requested || requested > 2 {
        return Ok(());
    }
    for vector_offset in [0x18, 8] {
        let mut index = 0;
        loop {
            let begin = backend
                .read_word(owner.0 + vector_offset)
                .map_err(ReplayError::Backend)?;
            if begin == 0 {
                break;
            }
            let end = backend
                .read_word(owner.0 + vector_offset + 4)
                .map_err(ReplayError::Backend)?;
            let bytes = end.checked_sub(begin).filter(|bytes| {
                begin.is_multiple_of(4) && bytes.is_multiple_of(4) && *bytes <= i32::MAX as u32
            });
            let bytes = bytes.ok_or(ReplayError::InvalidVector { begin, end })?;
            if index >= bytes / 4 {
                break;
            }
            if *remaining_visits == 0 {
                return Err(ReplayError::BudgetExhausted);
            }
            *remaining_visits -= 1;
            let object = backend
                .read_word(address(begin, index * 4)?)
                .map_err(ReplayError::Backend)?;
            let flags_address = address(object, 4)?;
            let flags = backend
                .read_word(flags_address)
                .map_err(ReplayError::Backend)?;
            let set_flag = requested == 1 || (requested == 2 && vector_offset == 8);
            backend
                .write_word(
                    flags_address,
                    if set_flag {
                        flags | 0x10
                    } else {
                        flags & !0x10
                    },
                )
                .map_err(ReplayError::Backend)?;
            if vector_offset == 0x18 {
                let (first, second) = if requested == 2 {
                    ((0x1c, 0), (0x18, 1))
                } else {
                    ((0x18, 0), (0x1c, requested))
                };
                call_element(backend, owner, vector_offset, index, first.0, first.1)?;
                call_element(backend, owner, vector_offset, index, second.0, second.1)?;
            } else {
                call_element(
                    backend,
                    owner,
                    vector_offset,
                    index,
                    0x1c,
                    u32::from(requested != 0),
                )?;
            }
            index += 1;
        }
    }
    if requested == 0 {
        backend
            .write_byte(owner.0 + 0x128, 0)
            .map_err(ReplayError::Backend)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    const OWNER: u32 = 0x1000;
    const A: u32 = 0x2000;
    const B: u32 = 0x3000;
    const OBJECT: u32 = 0x4000;
    const TABLE: u32 = 0x5000;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Call {
        object: u32,
        callee: u32,
        argument: u32,
        flags: u32,
        current: u32,
        previous: u32,
        byte: u8,
    }
    struct Memory {
        words: BTreeMap<u32, u32>,
        byte: u8,
        global: u32,
        reads: RefCell<Vec<u32>>,
        writes: Vec<(u32, u32)>,
        calls: Vec<Call>,
        mode: u8,
        failed_read: Option<u32>,
    }
    impl Memory {
        fn new() -> Self {
            let mut memory = Self {
                words: BTreeMap::from([
                    (OWNER + 0xe4, 3),
                    (OWNER + 0xe8, 99),
                    (OWNER + 0x18, A),
                    (OWNER + 0x1c, A + 4),
                    (OWNER + 8, B),
                    (OWNER + 12, B + 4),
                    (A, OBJECT),
                    (A + 4, OBJECT + 0x100),
                    (B, OBJECT + 0x200),
                ]),
                byte: 0xa5,
                global: 0,
                reads: RefCell::new(Vec::new()),
                writes: Vec::new(),
                calls: Vec::new(),
                mode: 0,
                failed_read: None,
            };
            for n in 0..4 {
                memory.words.extend([
                    (OBJECT + n * 0x100, TABLE + n * 0x100),
                    (OBJECT + n * 0x100 + 4, 0xa5a5_ffe7),
                    (TABLE + n * 0x100 + 0x18, 0x6000 + n * 0x100 + 0x18),
                    (TABLE + n * 0x100 + 0x1c, 0x6000 + n * 0x100 + 0x1c),
                ]);
            }
            memory
        }
        fn owner() -> ReplayOwnerId {
            ReplayOwnerId::new(OWNER).unwrap()
        }
    }
    impl ReplayReadBackend for Memory {
        type Error = &'static str;
        fn global_countdown(&self) -> Result<u32, Self::Error> {
            Ok(self.global)
        }
        fn read_word(&self, address: u32) -> Result<u32, Self::Error> {
            self.reads.borrow_mut().push(address);
            if self.failed_read == Some(address) {
                return Err("unavailable read");
            }
            self.words.get(&address).copied().ok_or("missing storage")
        }
    }
    impl ReplayBackend for Memory {
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
            *self.words.get_mut(&address).ok_or("missing storage")? = value;
            self.writes.push((address, value));
            Ok(())
        }
        fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
            if address != OWNER + 0x128 {
                return Err("missing byte");
            }
            self.byte = value;
            Ok(())
        }
        fn invoke(&mut self, object: u32, callee: u32, argument: u32) -> Result<(), Self::Error> {
            self.calls.push(Call {
                object,
                callee,
                argument,
                flags: self.words[&(object + 4)],
                current: self.words[&(OWNER + 0xe4)],
                previous: self.words[&(OWNER + 0xe8)],
                byte: self.byte,
            });
            if self.calls.len() == 1 {
                match self.mode {
                    1 => {
                        // Replace both object and table before the second call;
                        // shrink logical size while retaining physical slot.
                        self.words.insert(A, OBJECT + 0x100);
                        self.words.insert(OBJECT + 0x100, TABLE + 0x300);
                        self.words.insert(OWNER + 0x1c, A);
                    }
                    2 => {
                        // Grow A and replace B's base before either is reread.
                        self.words.insert(OWNER + 0x1c, A + 8);
                        self.words.insert(OWNER + 8, B + 4);
                        self.words.insert(OWNER + 12, B + 8);
                        self.words.insert(B + 4, OBJECT + 0x300);
                    }
                    3 => set_state(self, Self::owner(), 4, &mut 1).unwrap(),
                    4 => {
                        self.byte = 0x77;
                        return Err("unavailable callback");
                    }
                    5 => {
                        // The second call uses the rebased vector, without a
                        // length check. Its flag word is deliberately untouched.
                        self.words.insert(OWNER + 0x18, A + 4);
                        self.words.insert(OWNER + 0x1c, A + 4);
                    }
                    6 => {
                        self.words.insert(OWNER + 0x18, 0);
                    }
                    _ => {}
                }
            }
            Ok(())
        }
    }

    #[test]
    fn busy_preserves_full_return_word_and_global_precedence() {
        let mut memory = Memory::new();
        for global in [1, 4, 0x100, 0x8000_0000, u32::MAX] {
            memory.global = global;
            assert_eq!(busy_return_word(&memory, None), Ok(global & !255 | 1));
            assert_eq!(is_busy(&memory, Some(Memory::owner())), Ok(true));
        }
        assert!(memory.reads.borrow().is_empty());
        memory.global = 0;
        assert_eq!(is_busy(&memory, None), Err(ReplayError::MissingOwner));
        for (state, expected) in [
            (0, 0),
            (4, 0),
            (1, 1),
            (2, 1),
            (3, 1),
            (0x100, 0x101),
            (0x104, 0x101),
            (0x8000_0000, 0x8000_0001),
            (u32::MAX, 0xffff_ff01),
        ] {
            memory.words.insert(OWNER + 0xe4, state);
            assert_eq!(
                busy_return_word(&memory, Some(Memory::owner())),
                Ok(expected)
            );
            assert_eq!(
                is_busy(&memory, Some(Memory::owner())),
                Ok(expected as u8 != 0)
            );
        }
        assert!(memory.writes.is_empty());
        assert!(memory.calls.is_empty());
    }

    #[test]
    fn setter_callback_order_flags_and_final_byte_cover_all_branches() {
        for requested in [0, 1, 2, 4, 0x100, u32::MAX] {
            let mut memory = Memory::new();
            set_state(&mut memory, Memory::owner(), requested, &mut 8).unwrap();
            assert_eq!(
                &memory.writes[..2],
                &[(OWNER + 0xe8, 3), (OWNER + 0xe4, requested)]
            );
            if requested > 2 {
                assert!(memory.calls.is_empty());
                assert_eq!(memory.byte, 0xa5);
                continue;
            }
            let callbacks: Vec<_> = memory
                .calls
                .iter()
                .map(|c| (c.object, c.callee, c.argument))
                .collect();
            let expected = if requested == 2 {
                vec![
                    (OBJECT, 0x601c, 0),
                    (OBJECT, 0x6018, 1),
                    (OBJECT + 0x200, 0x621c, 1),
                ]
            } else {
                vec![
                    (OBJECT, 0x6018, 0),
                    (OBJECT, 0x601c, requested),
                    (OBJECT + 0x200, 0x621c, requested),
                ]
            };
            assert_eq!(callbacks, expected);
            for call in &memory.calls {
                assert_eq!(
                    (call.current, call.previous, call.byte),
                    (requested, 3, 0xa5)
                );
                let set = requested == 1 || (requested == 2 && call.object == OBJECT + 0x200);
                assert_eq!(call.flags, if set { 0xa5a5_fff7 } else { 0xa5a5_ffe7 });
            }
            assert_eq!(memory.byte, if requested == 0 { 0 } else { 0xa5 });
        }
    }

    #[test]
    fn equal_state_still_writes_previous_without_reading_vectors_or_clearing_byte() {
        for requested in [0, 1, 2, 4, u32::MAX] {
            let mut memory = Memory::new();
            memory.words.insert(OWNER + 0xe4, requested);
            set_state(&mut memory, Memory::owner(), requested, &mut 0).unwrap();
            assert_eq!(
                memory.writes,
                [(OWNER + 0xe8, requested), (OWNER + 0xe4, requested)]
            );
            assert_eq!(*memory.reads.borrow(), [OWNER + 0xe4, OWNER + 0xe8]);
            assert_eq!(memory.byte, 0xa5);
        }
    }

    #[test]
    fn callback_replacement_shrink_and_rebase_preserve_second_call_without_flag_rewrite() {
        for mode in [1, 5] {
            let mut memory = Memory::new();
            memory.mode = mode;
            set_state(&mut memory, Memory::owner(), 1, &mut 8).unwrap();
            assert_eq!(memory.calls.len(), 3);
            let second = &memory.calls[1];
            assert_eq!(second.object, OBJECT + 0x100);
            assert_eq!(second.callee, if mode == 1 { 0x631c } else { 0x611c });
            assert_eq!(second.flags, 0xa5a5_ffe7);
            assert_eq!(memory.writes.len(), 4);
        }
    }

    #[test]
    fn callback_growth_and_b_replacement_are_read_live() {
        let mut memory = Memory::new();
        memory.mode = 2;
        set_state(&mut memory, Memory::owner(), 2, &mut 8).unwrap();
        let objects: Vec<_> = memory.calls.iter().map(|c| c.object).collect();
        assert_eq!(
            objects,
            [
                OBJECT,
                OBJECT,
                OBJECT + 0x100,
                OBJECT + 0x100,
                OBJECT + 0x300
            ]
        );
        assert_eq!(memory.words[&(OBJECT + 0x300 + 4)] & 0x10, 0x10);
        assert_eq!(memory.words[&(OBJECT + 0x200 + 4)] & 0x10, 0);
    }

    #[test]
    fn reentrant_state_changes_do_not_switch_outer_branch_or_prevent_final_clear() {
        let mut memory = Memory::new();
        memory.mode = 3;
        set_state(&mut memory, Memory::owner(), 0, &mut 8).unwrap();
        assert_eq!(memory.words[&(OWNER + 0xe4)], 4);
        assert_eq!(memory.words[&(OWNER + 0xe8)], 0);
        assert_eq!(memory.calls[1].argument, 0);
        assert_eq!(memory.calls[1].current, 4);
        assert_eq!(memory.calls.len(), 3);
        assert_eq!(memory.byte, 0);
    }

    #[test]
    fn errors_and_host_limits_preserve_partial_mutations() {
        let mut memory = Memory::new();
        memory.mode = 4;
        assert_eq!(
            set_state(&mut memory, Memory::owner(), 0, &mut 8),
            Err(ReplayError::Backend("unavailable callback"))
        );
        assert_eq!(memory.calls.len(), 1);
        assert_eq!(memory.byte, 0x77);
        assert_eq!(memory.words[&(OWNER + 0xe4)], 0);
        assert_eq!(memory.writes.len(), 3);
        let mut memory = Memory::new();
        assert_eq!(
            set_state(&mut memory, Memory::owner(), 1, &mut 0),
            Err(ReplayError::BudgetExhausted)
        );
        assert_eq!(memory.writes.len(), 2);
        assert!(memory.calls.is_empty());
        let mut memory = Memory::new();
        memory.failed_read = Some(OWNER + 0xe8);
        assert_eq!(
            set_state(&mut memory, Memory::owner(), 1, &mut 8),
            Err(ReplayError::Backend("unavailable read"))
        );
        assert_eq!(memory.writes, [(OWNER + 0xe8, 3)]);
        assert_eq!(memory.words[&(OWNER + 0xe4)], 3);
        let mut memory = Memory::new();
        memory.mode = 6;
        assert_eq!(
            set_state(&mut memory, Memory::owner(), 1, &mut 8),
            Err(ReplayError::InvalidAddress(0))
        );
        assert_eq!(memory.calls.len(), 1);
    }

    #[test]
    fn null_begin_skips_end_and_malformed_spans_are_explicit_host_errors() {
        let mut memory = Memory::new();
        memory.words.insert(OWNER + 0x18, 0);
        memory.words.insert(OWNER + 8, 0);
        memory.failed_read = Some(OWNER + 0x1c);
        set_state(&mut memory, Memory::owner(), 0, &mut 0).unwrap();
        assert_eq!(memory.byte, 0);
        assert!(!memory.reads.borrow().contains(&(OWNER + 0x1c)));
        for end in [A - 4, A + 1, A + 0x8000_0000] {
            let mut memory = Memory::new();
            memory.words.insert(OWNER + 0x1c, end);
            assert_eq!(
                set_state(&mut memory, Memory::owner(), 1, &mut 8),
                Err(ReplayError::InvalidVector { begin: A, end })
            );
            assert_eq!(memory.writes.len(), 2);
        }
        for address in [0, 1, u32::MAX, 0xffff_ff00] {
            assert_eq!(
                ReplayOwnerId::new(address),
                Err(InvalidReplayOwner(address))
            );
        }
    }
}
