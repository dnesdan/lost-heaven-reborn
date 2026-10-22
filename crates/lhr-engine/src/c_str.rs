//! Bounded C_str lifetime subset for LS3DF PC SHA-256
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Scoped RVAs: C_rep release 0x6fec0, C_str destructor 0x6fee0 and
//! const-char assignment 0x6ff00. Representation construction follows the
//! writes inlined by assignment (dependency constructor 0x6fe20).
//! C_str has original PS2 name provenance; C_rep/member labels are researched.
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
    pub fn create_rep<B: CStrBackend + ?Sized>(
        &mut self,
        bytes: &[u8],
        backend: &mut B,
    ) -> Result<Option<CRepKey>, StringOperationError<B::Error>> {
        self.allocate_rep(bytes, true, backend)
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
        let r = self.reps[(key.slot - 1) as usize].as_mut().unwrap();
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
    /// No equality shortcut: nonnull empty allocates9, before old release. A
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
    /// Nondeleting destructor preserves its raw rep pointer; no second release.
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
