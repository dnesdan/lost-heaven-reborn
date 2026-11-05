#![forbid(unsafe_code)]
//! Refcounted C_str byte strings for LS3DF PC SHA-256
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Scoped RVAs: C_rep constructor (s,len) 0x6fe20, C_rep constructor
//! (s1,s2) 0x6fe60, C_rep release 0x6fec0, C_rep placement delete 0x6fed0,
//! C_str destructor 0x6fee0, const-char assignment 0x6ff00, operator+
//! 0x6ff80 and subscript copy-on-write 0x70100. Upstream reference mafia-re
//! common/C_str.h:18-162 @ ab67b13. C_str/C_rep member names and the
//! Release/operator spellings carry original PS2 name provenance
//! (C_str.h:6-11); key/arena/backend labels are host-researched.
//! Host keys/budgets reject stale or owned releases; native decrements themselves
//! wrap and delete only on exact zero. Only positive counts <=i32::MAX admitted.
//! Explicit allocator effects are required. Host slots are not native object
//! allocations; Drop cannot invoke the required allocator cleanup.
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_ARENA: AtomicU64 = AtomicU64::new(1);
macro_rules! key {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name {
            arena: u64,
            slot: u32,
        }
        impl $name {
            pub fn arena(self) -> u64 {
                self.arena
            }
            pub fn slot(self) -> u32 {
                self.slot
            }
        }
    };
}
key!(CStrKey);
key!(CRepKey);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StringAllocation(pub u64);
/// Use one compatible live allocator identity domain throughout an arena.
/// Every successful result transfers a unique allocation across all arenas
/// sharing that backend. Errors may follow effects; they stop further mutation.
pub trait CStrBackend {
    type Error;
    /// Exactly len+9 bytes. None models the native returning-null allocation
    /// branch, not a claim about the original allocator's global OOM handling.
    fn allocate_string(&mut self, bytes: u32) -> Result<Option<StringAllocation>, Self::Error>;
    fn free_string(&mut self, allocation: StringAllocation) -> Result<(), Self::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringError {
    Poisoned,
    ForeignArena,
    IdentityOverflow,
    HostAllocation,
    InvalidExtent,
    InvalidAllocation,
    UnknownString(CStrKey),
    UnknownRep(CRepKey),
    Destructed(CStrKey),
    ReferenceOverflow,
    ReferenceOwned,
    InvalidView,
}
#[derive(Debug, PartialEq, Eq)]
pub enum StringOperationError<E> {
    State(StringError),
    Backend(E),
}
impl<E> From<StringError> for StringOperationError<E> {
    fn from(e: StringError) -> Self {
        Self::State(e)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepView {
    pub rep: CRepKey,
    pub offset: u32,
}
#[derive(Debug, PartialEq, Eq)]
pub struct CRep {
    pub references: u32,
    pub external_references: u32,
    pub size: u32,
    /// Data bytes excluding the implicit terminating NUL at data[size].
    pub bytes: Vec<u8>,
    pub allocation: StringAllocation,
}
#[derive(Debug, PartialEq, Eq)]
pub struct CStr {
    pub rep: Option<CRepKey>,
    /// Host diagnostic for a new rep retained before interrupted old release.
    pub pending: Option<CRepKey>,
    pub destructed: bool,
}
pub struct CStrings {
    identity: u64,
    strings: Vec<Option<CStr>>,
    reps: Vec<Option<CRep>>,
    poisoned: bool,
}
impl CStrings {
    pub fn new() -> Result<Self, StringError> {
        let identity = NEXT_ARENA
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| StringError::IdentityOverflow)?;
        Ok(Self {
            identity,
            strings: Vec::new(),
            reps: Vec::new(),
            poisoned: false,
        })
    }
    /// Scalar-allocation admission guard shared with managed-record adoption.
    pub(crate) fn owns_allocation(&self, raw: u64) -> bool {
        self.reps.iter().flatten().any(|r| r.allocation.0 == raw)
    }
    pub(crate) fn allocation_tokens(&self) -> impl Iterator<Item = u64> + '_ {
        self.reps.iter().flatten().map(|r| r.allocation.0)
    }
    pub fn identity(&self) -> u64 {
        self.identity
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    fn available(&self) -> Result<(), StringError> {
        if self.poisoned {
            Err(StringError::Poisoned)
        } else {
            Ok(())
        }
    }
    fn index(&self, arena: u64, slot: u32) -> Result<usize, StringError> {
        if arena != self.identity {
            Err(StringError::ForeignArena)
        } else {
            slot.checked_sub(1)
                .map(|n| n as usize)
                .ok_or(StringError::IdentityOverflow)
        }
    }
    pub fn string(&self, k: CStrKey) -> Result<&CStr, StringError> {
        self.strings
            .get(self.index(k.arena, k.slot)?)
            .and_then(Option::as_ref)
            .ok_or(StringError::UnknownString(k))
    }
    pub fn rep(&self, k: CRepKey) -> Result<&CRep, StringError> {
        self.reps
            .get(self.index(k.arena, k.slot)?)
            .and_then(Option::as_ref)
            .ok_or(StringError::UnknownRep(k))
    }
    fn live_string(&self, k: CStrKey) -> Result<(), StringError> {
        if self.string(k)?.destructed {
            Err(StringError::Destructed(k))
        } else {
            Ok(())
        }
    }
    fn reserve<T>(v: &mut Vec<T>) -> Result<u32, StringError> {
        let slot = u32::try_from(v.len())
            .ok()
            .and_then(|v| v.checked_add(1))
            .ok_or(StringError::IdentityOverflow)?;
        v.try_reserve(1).map_err(|_| StringError::HostAllocation)?;
        Ok(slot)
    }
    pub(crate) fn validate_source(bytes: &[u8]) -> Result<(), StringError> {
        let len = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        if len > i32::MAX as usize - 9 {
            Err(StringError::InvalidExtent)
        } else {
            Ok(())
        }
    }
    pub(crate) fn reserve_strings(&mut self, count: usize) -> Result<(), StringError> {
        self.available()?;
        self.strings
            .len()
            .checked_add(count)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(StringError::IdentityOverflow)?;
        self.strings
            .try_reserve(count)
            .map_err(|_| StringError::HostAllocation)
    }
    pub fn create_null(&mut self) -> Result<CStrKey, StringError> {
        self.available()?;
        let slot = Self::reserve(&mut self.strings)?;
        self.strings.push(Some(CStr {
            rep: None,
            pending: None,
            destructed: false,
        }));
        Ok(CStrKey {
            arena: self.identity,
            slot,
        })
    }
    fn increment(&mut self, key: CRepKey, external: bool) -> Result<(), StringError> {
        let count = self.rep(key)?.references;
        if count >= i32::MAX as u32 {
            return Err(StringError::ReferenceOverflow);
        }
        let r = self.reps[(key.slot - 1) as usize].as_mut().unwrap();
        r.references += 1;
        if external {
            r.external_references += 1;
        }
        Ok(())
    }
    fn allocate_rep<B: CStrBackend + ?Sized>(
        &mut self,
        bytes: &[u8],
        external: bool,
        backend: &mut B,
    ) -> Result<Option<CRepKey>, StringOperationError<B::Error>> {
        self.available()?;
        let len = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        let size = u32::try_from(len)
            .ok()
            .filter(|n| *n <= i32::MAX as u32 - 9)
            .ok_or(StringError::InvalidExtent)?;
        let slot = Self::reserve(&mut self.reps)?;
        let mut content = Vec::new();
        content
            .try_reserve(len)
            .map_err(|_| StringError::HostAllocation)?;
        content.extend_from_slice(&bytes[..len]);
        let allocation = match backend.allocate_string(size + 9) {
            Ok(v) => v,
            Err(e) => {
                self.poisoned = true;
                return Err(StringOperationError::Backend(e));
            }
        };
        let Some(allocation) = allocation else {
            return Ok(None);
        };
        if allocation.0 == 0
            || self
                .reps
                .iter()
                .flatten()
                .any(|r| r.allocation == allocation)
        {
            self.poisoned = true;
            return Err(StringError::InvalidAllocation.into());
        }
        self.reps.push(Some(CRep {
            references: 1,
            external_references: u32::from(external),
            size,
            bytes: content,
            allocation,
        }));
        Ok(Some(CRepKey {
            arena: self.identity,
            slot,
        }))
    }
    /// LS3DF RVA 0x6fe20. Evidenced name C_rep::C_rep (mafia-re
    /// common/C_str.h:18-25): copies the bytes, stores the size, sets ref 1
    /// and terminates. Host length is the first NUL cut (the strlen done by
    /// the C_str wrappers); the backend request is exactly len+9 for the two
    /// count words, the bytes and the terminator. Caller-owns: the external
    /// count starts at 1 and release_rep frees the backend allocation at the
    /// exact zero transition.
    pub fn create_rep<B: CStrBackend + ?Sized>(
        &mut self,
        bytes: &[u8],
        backend: &mut B,
    ) -> Result<Option<CRepKey>, StringOperationError<B::Error>> {
        self.allocate_rep(bytes, true, backend)
    }
    fn allocate_concat_rep<B: CStrBackend + ?Sized>(
        &mut self,
        first: &[u8],
        second: &[u8],
        external: bool,
        backend: &mut B,
    ) -> Result<Option<CRepKey>, StringOperationError<B::Error>> {
        self.available()?;
        let total = first
            .len()
            .checked_add(second.len())
            .ok_or(StringError::InvalidExtent)?;
        let size = u32::try_from(total)
            .ok()
            .filter(|n| *n <= i32::MAX as u32 - 9)
            .ok_or(StringError::InvalidExtent)?;
        let slot = Self::reserve(&mut self.reps)?;
        let mut content = Vec::new();
        content
            .try_reserve(total)
            .map_err(|_| StringError::HostAllocation)?;
        content.extend_from_slice(first);
        content.extend_from_slice(second);
        let allocation = match backend.allocate_string(size + 9) {
            Ok(v) => v,
            Err(e) => {
                self.poisoned = true;
                return Err(StringOperationError::Backend(e));
            }
        };
        let Some(allocation) = allocation else {
            return Ok(None);
        };
        if allocation.0 == 0
            || self
                .reps
                .iter()
                .flatten()
                .any(|r| r.allocation == allocation)
        {
            self.poisoned = true;
            return Err(StringError::InvalidAllocation.into());
        }
        self.reps.push(Some(CRep {
            references: 1,
            external_references: u32::from(external),
            size,
            bytes: content,
            allocation,
        }));
        Ok(Some(CRepKey {
            arena: self.identity,
            slot,
        }))
    }
    /// LS3DF RVA 0x6fe60. Evidenced name C_rep::C_rep (mafia-re
    /// common/C_str.h:27-35): verbatim memcpy of both sides with no NUL scan
    /// (the operator+ wrapper strlen-cuts the suffix first), size len1+len2,
    /// ref 1, backend request total+9. Caller-owns standalone rep, mirroring
    /// create_rep.
    pub fn create_concat_rep<B: CStrBackend + ?Sized>(
        &mut self,
        first: &[u8],
        second: &[u8],
        backend: &mut B,
    ) -> Result<Option<CRepKey>, StringOperationError<B::Error>> {
        self.allocate_concat_rep(first, second, true, backend)
    }
    pub fn retain_rep(&mut self, key: CRepKey) -> Result<(), StringError> {
        self.available()?;
        self.increment(key, true)
    }
    fn release_reference<B: CStrBackend + ?Sized>(
        &mut self,
        key: CRepKey,
        backend: &mut B,
    ) -> Result<(), StringOperationError<B::Error>> {
        // Total function (forbid_unsafe module): stale keys and out-of-range
        // slots report UnknownRep instead of panicking.
        let index = (key.slot as usize)
            .checked_sub(1)
            .filter(|&i| i < self.reps.len())
            .ok_or(StringError::UnknownRep(key))?;
        let r = self.reps[index]
            .as_mut()
            .ok_or(StringError::UnknownRep(key))?;
        r.references -= 1;
        if r.references != 0 {
            return Ok(());
        }
        let allocation = r.allocation;
        if let Err(e) = backend.free_string(allocation) {
            self.poisoned = true;
            return Err(StringOperationError::Backend(e));
        }
        self.reps[(key.slot - 1) as usize] = None;
        Ok(())
    }
    /// LS3DF RVA 0x6fec0. Evidenced name C_rep::Release (mafia-re
    /// common/C_str.h:43-50): drops one caller-owned hold; the backend
    /// allocation frees (native delete[] over the char buffer) only on the
    /// exact zero transition. Releasing an internally owned rep reports
    /// ReferenceOwned instead of decrementing. RVA 0x6fec0 is already
    /// ledger-credited (frozen port "LS3DF C_str ownership"); this takes NO
    /// overlay entry (no double count).
    pub fn release_rep<B: CStrBackend + ?Sized>(
        &mut self,
        key: CRepKey,
        backend: &mut B,
    ) -> Result<(), StringOperationError<B::Error>> {
        self.available()?;
        if self.rep(key)?.external_references == 0 {
            return Err(StringError::ReferenceOwned.into());
        }
        self.reps[(key.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .external_references -= 1;
        self.release_reference(key, backend)
    }
    /// LS3DF RVA 0x6fed0. Evidenced spelling C_rep::operator delete
    /// (mafia-re common/C_str.h:60-66; placement form carrying the length
    /// word, invoked by constructor-throw unwinding): frees the backend
    /// allocation and clears the slot unconditionally, with no refcount gate
    /// (native delete[] (char *)p). The caller must hold the only reference:
    /// the rep must never have been published to a C_str, and any stale key
    /// reports UnknownRep afterwards.
    pub fn delete_rep<B: CStrBackend + ?Sized>(
        &mut self,
        key: CRepKey,
        backend: &mut B,
    ) -> Result<(), StringOperationError<B::Error>> {
        self.available()?;
        // The caller must hold the only reference: refuse while any live
        // non-destructed string still names this rep (native trusts the
        // caller and would dangle; the host reports ReferenceOwned).
        let held = self
            .strings
            .iter()
            .flatten()
            .any(|s| !s.destructed && s.rep == Some(key));
        if held {
            return Err(StringError::ReferenceOwned.into());
        }
        let allocation = self.rep(key)?.allocation;
        if let Err(e) = backend.free_string(allocation) {
            self.poisoned = true;
            return Err(StringOperationError::Backend(e));
        }
        self.reps[(key.slot - 1) as usize] = None;
        Ok(())
    }
    /// Host fresh-copy adapter, retaining a separate C_str-owned rep reference.
    pub fn copy_string(&mut self, source: CStrKey) -> Result<CStrKey, StringError> {
        self.available()?;
        self.live_string(source)?;
        let rep = self.string(source)?.rep;
        let slot = Self::reserve(&mut self.strings)?;
        if let Some(rep) = rep {
            self.increment(rep, false)?;
        }
        self.strings.push(Some(CStr {
            rep,
            pending: None,
            destructed: false,
        }));
        Ok(CStrKey {
            arena: self.identity,
            slot,
        })
    }
    /// Host ownership alias intake. This is not an additional reconstructed entry.
    pub fn alias_into<B: CStrBackend + ?Sized>(
        &mut self,
        dest: CStrKey,
        source: CStrKey,
        backend: &mut B,
    ) -> Result<(), StringOperationError<B::Error>> {
        self.available()?;
        self.live_string(dest)?;
        self.live_string(source)?;
        let new = self.string(source)?.rep;
        if let Some(rep) = new {
            self.increment(rep, false)?;
        }
        self.publish(dest, new, backend)
    }
    fn publish<B: CStrBackend + ?Sized>(
        &mut self,
        dest: CStrKey,
        new: Option<CRepKey>,
        backend: &mut B,
    ) -> Result<(), StringOperationError<B::Error>> {
        let old = self.string(dest)?.rep;
        self.strings[(dest.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .pending = new;
        if let Some(old) = old {
            self.release_reference(old, backend)?;
        }
        let target = self.strings[(dest.slot - 1) as usize].as_mut().unwrap();
        target.rep = new;
        target.pending = None;
        Ok(())
    }
    /// LS3DF RVA 0x6ff00 (already ledger-credited, frozen port "LS3DF C_str
    /// ownership": NO overlay entry here). Evidenced name C_str::operator=
    /// (mafia-re common/C_str.h:107-124): NULL first, then strlen plus the (s,len)
    /// constructor, old release and publish. No equality shortcut: nonnull
    /// empty allocates 9, before old release. A
    /// returning-null allocation still releases old and publishes a null rep.
    pub fn assign<B: CStrBackend + ?Sized>(
        &mut self,
        dest: CStrKey,
        source: Option<&[u8]>,
        backend: &mut B,
    ) -> Result<(), StringOperationError<B::Error>> {
        self.available()?;
        self.live_string(dest)?;
        let new = source
            .map(|s| self.allocate_rep(s, false, backend))
            .transpose()?
            .flatten();
        self.publish(dest, new, backend)
    }
    /// LS3DF RVA 0x6ff80. Evidenced name C_str::operator+ (mafia-re
    /// common/C_str.h:137-148): a null source string behaves as C_str(cp), so
    /// a null suffix yields a null string and bytes allocate a fresh rep. A
    /// live source joins its exact stored bytes (native memcpy with the known
    /// length) with the suffix cut at its first NUL (native strlen), through
    /// the (s1,s2) constructor. A returning-null allocation publishes a null
    /// string. The result is a new caller-owned C_str holding a C_str-owned
    /// rep. A missing suffix on a live source is outside the native domain
    /// (it would strlen NULL) and is treated as an empty suffix instead of
    /// faulting.
    pub fn concat<B: CStrBackend + ?Sized>(
        &mut self,
        source: CStrKey,
        suffix: Option<&[u8]>,
        backend: &mut B,
    ) -> Result<CStrKey, StringOperationError<B::Error>> {
        self.available()?;
        self.live_string(source)?;
        let tail = suffix.map(|s| &s[..s.iter().position(|b| *b == 0).unwrap_or(s.len())]);
        let slot = Self::reserve(&mut self.strings)?;
        let new = match self.string(source)?.rep {
            None => match tail {
                None => None,
                Some(tail) => self.allocate_rep(tail, false, backend)?,
            },
            Some(base) => {
                let tail = tail.unwrap_or(&[]);
                let mut head = Vec::new();
                head.try_reserve(self.rep(base)?.size as usize)
                    .map_err(|_| StringError::HostAllocation)?;
                head.extend_from_slice(&self.rep(base)?.bytes);
                self.allocate_concat_rep(&head, tail, false, backend)?
            }
        };
        self.strings.push(Some(CStr {
            rep: new,
            pending: None,
            destructed: false,
        }));
        Ok(CStrKey {
            arena: self.identity,
            slot,
        })
    }
    pub fn view(&self, view: RepView) -> Result<&[u8], StringError> {
        let rep = self.rep(view.rep)?;
        rep.bytes
            .get(view.offset as usize..)
            .ok_or(StringError::InvalidView)
    }
    pub fn assign_from_view<B: CStrBackend + ?Sized>(
        &mut self,
        dest: CStrKey,
        source: RepView,
        backend: &mut B,
    ) -> Result<(), StringOperationError<B::Error>> {
        self.available()?;
        self.live_string(dest)?;
        let bytes = self.view(source)?;
        let mut copy = Vec::new();
        copy.try_reserve(bytes.len())
            .map_err(|_| StringError::HostAllocation)?;
        copy.extend_from_slice(bytes);
        self.assign(dest, Some(&copy), backend)
    }
    /// LS3DF RVA 0x70100. Evidenced name C_str::operator[] (mafia-re
    /// common/C_str.h:150-162): copy-on-write cell access. A shared rep
    /// (references != 1) detaches first through the (s,len) constructor with
    /// the accessor-read length: the fresh copy allocates before the old hold
    /// releases (native new-then-Release order), and a returning-null
    /// allocation releases the old hold, publishes null and reports
    /// InvalidView since there is no cell to yield. Host domain is narrowed:
    /// null strings, destructed handles and positions at or past the size
    /// report InvalidView (the native indexes unchecked, including the
    /// terminator cell), and positions are u32 so negatives are
    /// unrepresentable.
    pub fn index_mut<B: CStrBackend + ?Sized>(
        &mut self,
        key: CStrKey,
        pos: u32,
        backend: &mut B,
    ) -> Result<&mut u8, StringOperationError<B::Error>> {
        self.available()?;
        self.live_string(key)?;
        let current = self.string(key)?.rep.ok_or(StringError::InvalidView)?;
        let size = self.rep(current)?.size;
        if pos >= size {
            return Err(StringError::InvalidView.into());
        }
        if self.rep(current)?.references != 1 {
            let mut copy = Vec::new();
            copy.try_reserve(size as usize)
                .map_err(|_| StringError::HostAllocation)?;
            copy.extend_from_slice(&self.rep(current)?.bytes);
            match self.allocate_concat_rep(&copy, &[], false, backend)? {
                Some(fresh) => self.publish(key, Some(fresh), backend)?,
                None => {
                    self.publish(key, None, backend)?;
                    return Err(StringError::InvalidView.into());
                }
            }
        }
        let owned = self.string(key)?.rep.ok_or(StringError::InvalidView)?;
        let slot = (owned.slot - 1) as usize;
        let cell = self.reps[slot]
            .as_mut()
            .unwrap()
            .bytes
            .get_mut(pos as usize)
            .ok_or(StringError::InvalidView)?;
        Ok(cell)
    }
    /// LS3DF RVA 0x6fee0 (already ledger-credited, frozen port "LS3DF C_str
    /// ownership": NO overlay entry here). Evidenced name C_str::~C_str
    /// (mafia-re common/C_str.h:88-94): releases the held rep, if any, and marks the
    /// handle destructed. Nondeleting destructor preserves its raw rep
    /// pointer; no second release.
    pub fn destruct_string<B: CStrBackend + ?Sized>(
        &mut self,
        key: CStrKey,
        backend: &mut B,
    ) -> Result<(), StringOperationError<B::Error>> {
        self.available()?;
        self.live_string(key)?;
        if let Some(rep) = self.string(key)?.rep {
            self.release_reference(rep, backend)?;
        }
        self.strings[(key.slot - 1) as usize]
            .as_mut()
            .unwrap()
            .destructed = true;
        Ok(())
    }
    pub fn destroy_string<B: CStrBackend + ?Sized>(
        &mut self,
        key: CStrKey,
        backend: &mut B,
    ) -> Result<(), StringOperationError<B::Error>> {
        self.destruct_string(key, backend)?;
        self.strings[(key.slot - 1) as usize] = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestBackend {
        next: u64,
        live: BTreeMap<u64, u32>,
        nulls: usize,
    }

    impl TestBackend {
        fn new() -> Self {
            Self {
                next: 1,
                live: BTreeMap::new(),
                nulls: 0,
            }
        }
    }

    impl CStrBackend for TestBackend {
        type Error = &'static str;
        fn allocate_string(&mut self, bytes: u32) -> Result<Option<StringAllocation>, Self::Error> {
            if self.nulls > 0 {
                self.nulls -= 1;
                return Ok(None);
            }
            let allocation = StringAllocation(self.next);
            self.next += 1;
            assert!(self.live.insert(allocation.0, bytes).is_none());
            Ok(Some(allocation))
        }
        fn free_string(&mut self, allocation: StringAllocation) -> Result<(), Self::Error> {
            assert!(self.live.remove(&allocation.0).is_some());
            Ok(())
        }
    }

    #[test]
    fn rep_ctor_copies_bytes_and_requests_len_plus_nine() {
        let mut strings = CStrings::new().unwrap();
        let mut backend = TestBackend::new();
        let rep = strings.create_rep(b"hello", &mut backend).unwrap().unwrap();
        assert_eq!(strings.rep(rep).unwrap().references, 1);
        assert_eq!(strings.rep(rep).unwrap().external_references, 1);
        assert_eq!(strings.rep(rep).unwrap().size, 5);
        assert_eq!(strings.rep(rep).unwrap().bytes, b"hello");
        let allocation = strings.rep(rep).unwrap().allocation.0;
        assert_eq!(backend.live[&allocation], 14);
        let cut = strings
            .create_rep(b"ab\x00cd", &mut backend)
            .unwrap()
            .unwrap();
        assert_eq!(strings.rep(cut).unwrap().size, 2);
        assert_eq!(strings.rep(cut).unwrap().bytes, b"ab");
        let empty = strings.create_rep(b"", &mut backend).unwrap().unwrap();
        assert_eq!(strings.rep(empty).unwrap().size, 0);
        let empty_allocation = strings.rep(empty).unwrap().allocation.0;
        assert_eq!(backend.live[&empty_allocation], 9);
    }

    #[test]
    fn concat_rep_joins_verbatim_with_combined_size() {
        let mut strings = CStrings::new().unwrap();
        let mut backend = TestBackend::new();
        let rep = strings
            .create_concat_rep(b"ab", b"cde", &mut backend)
            .unwrap()
            .unwrap();
        assert_eq!(strings.rep(rep).unwrap().size, 5);
        assert_eq!(strings.rep(rep).unwrap().bytes, b"abcde");
        assert_eq!(strings.rep(rep).unwrap().references, 1);
        assert_eq!(strings.rep(rep).unwrap().external_references, 1);
        let allocation = strings.rep(rep).unwrap().allocation.0;
        assert_eq!(backend.live[&allocation], 14);
        let both_empty = strings
            .create_concat_rep(b"", b"", &mut backend)
            .unwrap()
            .unwrap();
        assert_eq!(strings.rep(both_empty).unwrap().size, 0);
        assert_eq!(strings.rep(both_empty).unwrap().bytes, b"");
    }

    #[test]
    fn release_decrements_and_frees_at_zero() {
        let mut strings = CStrings::new().unwrap();
        let mut backend = TestBackend::new();
        let rep = strings.create_rep(b"x", &mut backend).unwrap().unwrap();
        strings.retain_rep(rep).unwrap();
        assert_eq!(strings.rep(rep).unwrap().references, 2);
        strings.release_rep(rep, &mut backend).unwrap();
        assert_eq!(strings.rep(rep).unwrap().references, 1);
        strings.release_rep(rep, &mut backend).unwrap();
        assert!(strings.rep(rep).is_err());
        assert!(backend.live.is_empty());
        let owned = strings.create_null().unwrap();
        strings.assign(owned, Some(b"y"), &mut backend).unwrap();
        let inner = strings.string(owned).unwrap().rep.unwrap();
        assert_eq!(
            strings.release_rep(inner, &mut backend),
            Err(StringOperationError::State(StringError::ReferenceOwned))
        );
    }

    #[test]
    fn delete_rep_refuses_while_a_live_string_holds_the_rep() {
        // Caller-obligation guard: a rep published to a live C_str reports
        // ReferenceOwned instead of dangling it (native trusts the caller).
        let mut strings = CStrings::new().unwrap();
        let mut backend = TestBackend::new();
        let rep = strings.create_rep(b"held", &mut backend).unwrap().unwrap();
        let owner = strings.create_null().unwrap();
        strings.publish(owner, Some(rep), &mut backend).unwrap();
        assert!(matches!(
            strings.delete_rep(rep, &mut backend),
            Err(StringOperationError::State(StringError::ReferenceOwned))
        ));
    }
    #[test]
    fn delete_rep_frees_unconditionally_without_refcount_gate() {
        let mut strings = CStrings::new().unwrap();
        let mut backend = TestBackend::new();
        let rep = strings.create_rep(b"gone", &mut backend).unwrap().unwrap();
        strings.retain_rep(rep).unwrap();
        assert_eq!(strings.rep(rep).unwrap().references, 2);
        strings.delete_rep(rep, &mut backend).unwrap();
        assert!(strings.rep(rep).is_err());
        assert!(backend.live.is_empty());
        assert!(strings.delete_rep(rep, &mut backend).is_err());
    }

    #[test]
    fn assign_char_ptr_replaces_and_releases_old() {
        let mut strings = CStrings::new().unwrap();
        let mut backend = TestBackend::new();
        let key = strings.create_null().unwrap();
        strings.assign(key, Some(b"old"), &mut backend).unwrap();
        let first = strings.string(key).unwrap().rep.unwrap();
        strings.assign(key, Some(b"newer"), &mut backend).unwrap();
        let second = strings.string(key).unwrap().rep.unwrap();
        assert_ne!(first, second);
        assert_eq!(strings.rep(second).unwrap().bytes, b"newer");
        assert_eq!(backend.live.len(), 1);
        strings.assign(key, None, &mut backend).unwrap();
        assert_eq!(strings.string(key).unwrap().rep, None);
        assert!(backend.live.is_empty());
        strings.assign(key, Some(b""), &mut backend).unwrap();
        let empty = strings.string(key).unwrap().rep.unwrap();
        assert_eq!(strings.rep(empty).unwrap().size, 0);
        assert_eq!(backend.live.len(), 1);
    }

    #[test]
    fn concat_appends_suffix_without_touching_source() {
        let mut strings = CStrings::new().unwrap();
        let mut backend = TestBackend::new();
        let base = strings.create_null().unwrap();
        strings.assign(base, Some(b"ab"), &mut backend).unwrap();
        let base_rep = strings.string(base).unwrap().rep.unwrap();
        let joined = strings.concat(base, Some(b"cde"), &mut backend).unwrap();
        let joined_rep = strings.string(joined).unwrap().rep.unwrap();
        assert_ne!(joined_rep, base_rep);
        assert_eq!(strings.rep(joined_rep).unwrap().bytes, b"abcde");
        assert_eq!(strings.rep(joined_rep).unwrap().external_references, 0);
        assert_eq!(strings.string(base).unwrap().rep, Some(base_rep));
        assert_eq!(strings.rep(base_rep).unwrap().bytes, b"ab");
        assert_eq!(strings.rep(base_rep).unwrap().references, 1);
        let cut = strings
            .concat(base, Some(b"c\x00de"), &mut backend)
            .unwrap();
        let cut_rep = strings.string(cut).unwrap().rep.unwrap();
        assert_eq!(strings.rep(cut_rep).unwrap().bytes, b"abc");
        let copy = strings.concat(base, None, &mut backend).unwrap();
        let copy_rep = strings.string(copy).unwrap().rep.unwrap();
        assert_ne!(copy_rep, base_rep);
        assert_eq!(strings.rep(copy_rep).unwrap().bytes, b"ab");
        let null = strings.create_null().unwrap();
        let from_null = strings.concat(null, Some(b"xy"), &mut backend).unwrap();
        let from_null_rep = strings.string(from_null).unwrap().rep.unwrap();
        assert_eq!(strings.rep(from_null_rep).unwrap().bytes, b"xy");
        let null_result = strings.concat(null, None, &mut backend).unwrap();
        assert_eq!(strings.string(null_result).unwrap().rep, None);
        backend.nulls = 1;
        let unpublished = strings.concat(base, Some(b"tail"), &mut backend).unwrap();
        assert_eq!(strings.string(unpublished).unwrap().rep, None);
        assert_eq!(strings.rep(base_rep).unwrap().bytes, b"ab");
    }

    #[test]
    fn index_mut_detaches_shared_rep_before_write() {
        let mut strings = CStrings::new().unwrap();
        let mut backend = TestBackend::new();
        let first = strings.create_null().unwrap();
        strings.assign(first, Some(b"hello"), &mut backend).unwrap();
        let shared = strings.copy_string(first).unwrap();
        let rep = strings.string(first).unwrap().rep.unwrap();
        assert_eq!(strings.rep(rep).unwrap().references, 2);
        *strings.index_mut(first, 0, &mut backend).unwrap() = b'J';
        let detached = strings.string(first).unwrap().rep.unwrap();
        assert_ne!(detached, rep);
        assert_eq!(strings.rep(detached).unwrap().bytes, b"Jello");
        assert_eq!(strings.rep(detached).unwrap().references, 1);
        assert_eq!(strings.string(shared).unwrap().rep, Some(rep));
        assert_eq!(strings.rep(rep).unwrap().bytes, b"hello");
        assert_eq!(strings.rep(rep).unwrap().references, 1);
        let before = strings.string(shared).unwrap().rep.unwrap();
        *strings.index_mut(shared, 1, &mut backend).unwrap() = b'E';
        assert_eq!(strings.string(shared).unwrap().rep, Some(before));
        assert_eq!(strings.rep(before).unwrap().bytes, b"hEllo");
        // Admitted gap: native defines data[size] (terminator cell); the host
        // has no terminator cell and reports InvalidView instead.
        assert!(strings.index_mut(shared, 5, &mut backend).is_err());
        let null = strings.create_null().unwrap();
        assert!(strings.index_mut(null, 0, &mut backend).is_err());
        strings.destruct_string(shared, &mut backend).unwrap();
        assert!(strings.index_mut(shared, 0, &mut backend).is_err());
    }

    #[test]
    fn destructor_releases_only_the_destroyed_handle() {
        let mut strings = CStrings::new().unwrap();
        let mut backend = TestBackend::new();
        let first = strings.create_null().unwrap();
        strings.assign(first, Some(b"keep"), &mut backend).unwrap();
        let rep = strings.string(first).unwrap().rep.unwrap();
        let second = strings.copy_string(first).unwrap();
        strings.destroy_string(first, &mut backend).unwrap();
        assert_eq!(strings.rep(rep).unwrap().references, 1);
        assert_eq!(strings.rep(rep).unwrap().bytes, b"keep");
        strings.destroy_string(second, &mut backend).unwrap();
        assert!(strings.rep(rep).is_err());
        assert!(backend.live.is_empty());
    }
}
