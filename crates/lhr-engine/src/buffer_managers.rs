//! Owned IB/VB manager allocation, partitioning and release projections.
//!
//! Independently recovered LS3DF PC entries: per-buffer allocate/free
//! 0x5aff0/0x5b1b0 and 0x68f10/0x690d0, manager allocate/free
//! 0x5b6c0/0x5b940 and 0x695e0/0x697c0, init 0x5ae00/0x68cf0,
//! destruction 0x5abf0/0x68af0, and reclamation 0x5b9d0/0x69820.
//! The original names C_IBManager/C_VBManager/C_IBuffer/C_VBuffer have PS2
//! provenance; the unified Rust owner registry is a host storage adapter.
//!
//! Positive nonwrapping extents and successful graphics locks are admitted.
//! New VB initialization requires nonzero stride; reuse intentionally ignores it.
//! Each arena is one self-contained original global IB/VB manager world;
//! cross-world/global-manager aliases are not admitted.
//! Native zero-size aliases, signed subtraction wraps and failed COM lock
//! results are outside this domain. Graphics rejection drives the native create
//! retries; backend Error means unavailable effects and poisons mutation.
//! Required allocator/graphics callbacks have no implicit no-op implementation.
//!
//! Manager traversal retains its original in-order sequence as num_free mutates.
//! New insertion is admitted only while that sequence is nondecreasing, when
//! upper-bound insertion is topology independent. Existing reuse remains allowed
//! in stale order. This does not reconstruct a general mutable-key native RB tree.
//! Raw owner descriptors stay stale after block free. Separate leases reject
//! stale wrapper frees; buffer destruction zeros only still-used descriptors.
//! This module does not attach copied VisualResourceBackend descriptors or GPU
//! rendering. One stable compatible backend domain must serve the whole arena.
//! All external allocations need explicit release while that backend is available;
//! dropping this Rust arena cannot invoke missing backend cleanup.

use crate::visual_resources::{IndexBufferHandle, VertexBufferHandle};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT_MANAGER: AtomicU64 = AtomicU64::new(1);
macro_rules! key {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name {
            manager: u64,
            slot: u32,
        }
        impl $name {
            pub fn manager(self) -> u64 {
                self.manager
            }
            pub fn slot(self) -> u32 {
                self.slot
            }
        }
    };
}
key!(IndexBufferKey);
key!(VertexBufferKey);
key!(IndexOwnerKey);
key!(VertexOwnerKey);
key!(BlockKey);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferKey {
    Index(IndexBufferKey),
    Vertex(VertexBufferKey),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerKey {
    Index(IndexOwnerKey),
    Vertex(VertexOwnerKey),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferKind {
    Index,
    Vertex,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pool {
    Default,
    Managed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsBuffer {
    Index(IndexBufferHandle),
    Vertex(VertexBufferHandle),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationToken(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockedDataKey(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateResult {
    Created(GraphicsBuffer),
    Rejected(i32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicsRequest {
    pub kind: BufferKind,
    pub bytes: u32,
    pub usage: u32,
    pub pool: Pool,
    pub fvf: Option<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexRequest {
    pub fvf: u32,
    pub usage: u32,
    pub stride: u32,
    pub count: u32,
    pub format: u32,
    pub exclusive: bool,
}
/// Allocator tokens own actual external allocation lifetimes. Graphics handles
/// and locked-data identities remain unique/stable in this backend domain.
/// create Rejected is an expected failed HRESULT; Error may already have effects
/// and is never retried. lock must return a successful nonzero data identity;
/// original ignored Lock HRESULTs are not simulated as Rust recovery behavior.
/// Callbacks must not reenter the manager. driver_present models the original
/// global pointer test in destruction; successful Init requires that driver.
pub trait BufferBackend {
    type Error;
    fn allocate_buffer(&mut self, kind: BufferKind) -> Result<AllocationToken, Self::Error>;
    fn free_buffer(&mut self, allocation: AllocationToken) -> Result<(), Self::Error>;
    fn allocate_block(
        &mut self,
        kind: BufferKind,
        base: u32,
        count: u32,
    ) -> Result<AllocationToken, Self::Error>;
    fn free_block(&mut self, allocation: AllocationToken) -> Result<(), Self::Error>;
    fn create_graphics(&mut self, request: GraphicsRequest) -> Result<CreateResult, Self::Error>;
    fn discard_managed(&mut self) -> Result<(), Self::Error>;
    fn lock(&mut self, graphics: GraphicsBuffer, flags: u32) -> Result<LockedDataKey, Self::Error>;
    fn unlock(&mut self, graphics: GraphicsBuffer) -> Result<(), Self::Error>;
    fn release_graphics(&mut self, graphics: GraphicsBuffer) -> Result<(), Self::Error>;
    fn driver_present(&self) -> bool;
    fn recompute_vram(&mut self) -> Result<(), Self::Error>;
    fn warn_unreleased_buffer(&mut self, buffer: BufferKey) -> Result<(), Self::Error>;
    fn out_of_memory(
        &mut self,
        kind: BufferKind,
        bytes: u32,
        hresult: i32,
    ) -> Result<(), Self::Error>;
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Descriptor {
    pub buffer: Option<BufferKey>,
    pub base: u32,
    pub graphics: Option<GraphicsBuffer>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerLease {
    Vacant,
    Active { block: BlockKey, published: bool },
    Freed,
    Invalidated,
}
#[derive(Debug, PartialEq, Eq)]
pub struct OwnerState {
    pub key: OwnerKey,
    pub descriptor: Descriptor,
    pub lease: OwnerLease,
}
#[derive(Debug, PartialEq, Eq)]
pub struct BlockState {
    pub buffer: BufferKey,
    pub owner: Option<OwnerKey>,
    pub base: u32,
    pub count: u32,
    allocation: AllocationToken,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexConfiguration {
    pub fvf: u32,
    pub stride: Option<u32>,
    pub format: u32,
}
#[derive(Debug, PartialEq, Eq)]
pub struct BufferState {
    pub key: BufferKey,
    pub usage: u32,
    pub pool: Pool,
    pub capacity: u32,
    pub num_free: u32,
    pub vertex: Option<VertexConfiguration>,
    pub graphics: Option<GraphicsBuffer>,
    pub data: Option<LockedDataKey>,
    pub flags: u32,
    pub lock_index: Option<usize>,
    pub free_blocks: Vec<BlockKey>,
    pub used_blocks: Vec<BlockKey>,
    pub native_used_capacity: u32,
    allocation: AllocationToken,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferError {
    Poisoned,
    ForeignManager,
    IdentityOverflow,
    HostAllocation,
    InvalidExtent,
    InvalidBackendIdentity,
    DriverUnavailable,
    UnknownBuffer(BufferKey),
    UnknownOwner(OwnerKey),
    UnknownBlock(BlockKey),
    OwnerActive(OwnerKey),
    StaleOwner(OwnerKey),
    UnpublishedOwner(OwnerKey),
    WrongKind,
    UnsupportedManagerInsertion(BufferKind),
    InvalidLockState,
}
#[derive(Debug, PartialEq, Eq)]
pub enum OperationError<E> {
    State(BufferError),
    Backend(E),
}
impl<E> From<BufferError> for OperationError<E> {
    fn from(value: BufferError) -> Self {
        Self::State(value)
    }
}

#[derive(Debug)]
pub struct BufferManagers {
    identity: u64,
    buffers: Vec<Option<BufferState>>,
    owners: Vec<OwnerState>,
    blocks: Vec<Option<BlockState>>,
    index_order: Vec<BufferKey>,
    vertex_order: Vec<BufferKey>,
    index_locked: Vec<BufferKey>,
    vertex_locked: Vec<BufferKey>,
    poisoned: bool,
}
impl BufferKey {
    fn parts(self) -> (u64, u32) {
        match self {
            Self::Index(k) => (k.manager, k.slot),
            Self::Vertex(k) => (k.manager, k.slot),
        }
    }
    pub fn kind(self) -> BufferKind {
        match self {
            Self::Index(_) => BufferKind::Index,
            Self::Vertex(_) => BufferKind::Vertex,
        }
    }
}
impl OwnerKey {
    fn parts(self) -> (u64, u32) {
        match self {
            Self::Index(k) => (k.manager, k.slot),
            Self::Vertex(k) => (k.manager, k.slot),
        }
    }
    fn kind(self) -> BufferKind {
        match self {
            Self::Index(_) => BufferKind::Index,
            Self::Vertex(_) => BufferKind::Vertex,
        }
    }
}
impl BufferManagers {
    pub fn new() -> Result<Self, BufferError> {
        let identity = NEXT_MANAGER
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| BufferError::IdentityOverflow)?;
        let mut index_locked = Vec::new();
        let mut vertex_locked = Vec::new();
        index_locked
            .try_reserve_exact(256)
            .map_err(|_| BufferError::HostAllocation)?;
        vertex_locked
            .try_reserve_exact(256)
            .map_err(|_| BufferError::HostAllocation)?;
        Ok(Self {
            identity,
            buffers: Vec::new(),
            owners: Vec::new(),
            blocks: Vec::new(),
            index_order: Vec::new(),
            vertex_order: Vec::new(),
            index_locked,
            vertex_locked,
            poisoned: false,
        })
    }
    pub fn identity(&self) -> u64 {
        self.identity
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    fn available(&self) -> Result<(), BufferError> {
        if self.poisoned {
            Err(BufferError::Poisoned)
        } else {
            Ok(())
        }
    }
    fn index(&self, manager: u64, slot: u32) -> Result<usize, BufferError> {
        if manager != self.identity {
            return Err(BufferError::ForeignManager);
        }
        Ok(slot.checked_sub(1).ok_or(BufferError::IdentityOverflow)? as usize)
    }
    fn next_slot(len: usize) -> Result<u32, BufferError> {
        u32::try_from(len)
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(BufferError::IdentityOverflow)
    }
    fn call<T, E>(&mut self, result: Result<T, E>) -> Result<T, OperationError<E>> {
        result.map_err(|e| {
            self.poisoned = true;
            OperationError::Backend(e)
        })
    }
    fn invalid_backend<E>(&mut self) -> OperationError<E> {
        self.poisoned = true;
        BufferError::InvalidBackendIdentity.into()
    }
    pub fn buffer(&self, key: BufferKey) -> Result<&BufferState, BufferError> {
        self.buffers
            .get(self.index(key.parts().0, key.parts().1)?)
            .and_then(Option::as_ref)
            .filter(|b| b.key == key)
            .ok_or(BufferError::UnknownBuffer(key))
    }
    fn buffer_mut(&mut self, key: BufferKey) -> Result<&mut BufferState, BufferError> {
        let i = self.index(key.parts().0, key.parts().1)?;
        self.buffers
            .get_mut(i)
            .and_then(Option::as_mut)
            .filter(|b| b.key == key)
            .ok_or(BufferError::UnknownBuffer(key))
    }
    pub fn index_buffer(&self, key: IndexBufferKey) -> Result<&BufferState, BufferError> {
        self.buffer(BufferKey::Index(key))
    }
    pub fn vertex_buffer(&self, key: VertexBufferKey) -> Result<&BufferState, BufferError> {
        self.buffer(BufferKey::Vertex(key))
    }
    pub fn owner(&self, key: OwnerKey) -> Result<&OwnerState, BufferError> {
        self.owners
            .get(self.index(key.parts().0, key.parts().1)?)
            .filter(|o| o.key == key)
            .ok_or(BufferError::UnknownOwner(key))
    }
    fn owner_mut(&mut self, key: OwnerKey) -> Result<&mut OwnerState, BufferError> {
        let i = self.index(key.parts().0, key.parts().1)?;
        self.owners
            .get_mut(i)
            .filter(|o| o.key == key)
            .ok_or(BufferError::UnknownOwner(key))
    }
    pub fn index_owner(&self, key: IndexOwnerKey) -> Result<&OwnerState, BufferError> {
        self.owner(OwnerKey::Index(key))
    }
    pub fn vertex_owner(&self, key: VertexOwnerKey) -> Result<&OwnerState, BufferError> {
        self.owner(OwnerKey::Vertex(key))
    }
    pub fn block(&self, key: BlockKey) -> Result<&BlockState, BufferError> {
        self.blocks
            .get(self.index(key.manager, key.slot)?)
            .and_then(Option::as_ref)
            .ok_or(BufferError::UnknownBlock(key))
    }
    fn block_mut(&mut self, key: BlockKey) -> Result<&mut BlockState, BufferError> {
        let i = self.index(key.manager, key.slot)?;
        self.blocks
            .get_mut(i)
            .and_then(Option::as_mut)
            .ok_or(BufferError::UnknownBlock(key))
    }
    pub fn index_buffers(&self) -> &[BufferKey] {
        &self.index_order
    }
    pub fn vertex_buffers(&self) -> &[BufferKey] {
        &self.vertex_order
    }
    pub fn index_locked(&self) -> &[BufferKey] {
        &self.index_locked
    }
    pub fn vertex_locked(&self) -> &[BufferKey] {
        &self.vertex_locked
    }
    fn order(&self, kind: BufferKind) -> &Vec<BufferKey> {
        match kind {
            BufferKind::Index => &self.index_order,
            BufferKind::Vertex => &self.vertex_order,
        }
    }
    fn order_mut(&mut self, kind: BufferKind) -> &mut Vec<BufferKey> {
        match kind {
            BufferKind::Index => &mut self.index_order,
            BufferKind::Vertex => &mut self.vertex_order,
        }
    }
    fn locked(&self, kind: BufferKind) -> &Vec<BufferKey> {
        match kind {
            BufferKind::Index => &self.index_locked,
            BufferKind::Vertex => &self.vertex_locked,
        }
    }
    fn locked_mut(&mut self, kind: BufferKind) -> &mut Vec<BufferKey> {
        match kind {
            BufferKind::Index => &mut self.index_locked,
            BufferKind::Vertex => &mut self.vertex_locked,
        }
    }
    fn create_owner(&mut self, kind: BufferKind) -> Result<OwnerKey, BufferError> {
        self.available()?;
        let slot = Self::next_slot(self.owners.len())?;
        let key = match kind {
            BufferKind::Index => OwnerKey::Index(IndexOwnerKey {
                manager: self.identity,
                slot,
            }),
            BufferKind::Vertex => OwnerKey::Vertex(VertexOwnerKey {
                manager: self.identity,
                slot,
            }),
        };
        self.owners
            .try_reserve(1)
            .map_err(|_| BufferError::HostAllocation)?;
        self.owners.push(OwnerState {
            key,
            descriptor: Descriptor::default(),
            lease: OwnerLease::Vacant,
        });
        Ok(key)
    }
    pub fn create_index_owner(&mut self) -> Result<IndexOwnerKey, BufferError> {
        match self.create_owner(BufferKind::Index)? {
            OwnerKey::Index(k) => Ok(k),
            _ => unreachable!(),
        }
    }
    pub fn create_vertex_owner(&mut self) -> Result<VertexOwnerKey, BufferError> {
        match self.create_owner(BufferKind::Vertex)? {
            OwnerKey::Vertex(k) => Ok(k),
            _ => unreachable!(),
        }
    }
    fn owner_available(&self, key: OwnerKey) -> Result<(), BufferError> {
        if matches!(self.owner(key)?.lease, OwnerLease::Active { .. }) {
            Err(BufferError::OwnerActive(key))
        } else {
            Ok(())
        }
    }
    fn extent(count: u32) -> Result<(), BufferError> {
        if count == 0 || count > i32::MAX as u32 {
            Err(BufferError::InvalidExtent)
        } else {
            Ok(())
        }
    }
    fn new_block<B: BufferBackend + ?Sized>(
        &mut self,
        buffer: BufferKey,
        owner: Option<OwnerKey>,
        base: u32,
        count: u32,
        backend: &mut B,
    ) -> Result<BlockKey, OperationError<B::Error>> {
        let slot = match Self::next_slot(self.blocks.len()) {
            Ok(slot) => slot,
            Err(error) => {
                self.poisoned = true;
                return Err(error.into());
            }
        };
        if self.blocks.try_reserve(1).is_err() {
            self.poisoned = true;
            return Err(BufferError::HostAllocation.into());
        }
        let allocation = self.call(backend.allocate_block(buffer.kind(), base, count))?;
        if allocation.0 == 0 {
            return Err(self.invalid_backend());
        }
        let key = BlockKey {
            manager: self.identity,
            slot,
        };
        self.blocks.push(Some(BlockState {
            buffer,
            owner,
            base,
            count,
            allocation,
        }));
        Ok(key)
    }
    fn insert_free(&mut self, buffer: BufferKey, key: BlockKey) -> Result<(), BufferError> {
        let size = self.block(key)?.count;
        let position = self
            .buffer(buffer)?
            .free_blocks
            .iter()
            .position(|k| self.block(*k).unwrap().count > size)
            .unwrap_or(self.buffer(buffer)?.free_blocks.len());
        self.buffer_mut(buffer)?.free_blocks.insert(position, key);
        Ok(())
    }
    fn allocate_at<B: BufferBackend + ?Sized>(
        &mut self,
        buffer: BufferKey,
        owner: OwnerKey,
        count: u32,
        backend: &mut B,
    ) -> Result<Option<u32>, OperationError<B::Error>> {
        self.available()?;
        Self::extent(count)?;
        self.owner_available(owner)?;
        if buffer.kind() != owner.kind() {
            return Err(BufferError::WrongKind.into());
        }
        let chosen = self
            .buffer(buffer)?
            .free_blocks
            .iter()
            .position(|k| self.block(*k).unwrap().count >= count);
        let Some(position) = chosen else {
            return Ok(None);
        };
        let block = self.buffer(buffer)?.free_blocks[position];
        let base = self.block(block)?.base;
        let rest = self.block(block)?.count - count;
        self.buffer_mut(buffer)?
            .used_blocks
            .try_reserve(1)
            .map_err(|_| BufferError::HostAllocation)?;
        self.buffer_mut(buffer)?
            .free_blocks
            .try_reserve(1)
            .map_err(|_| BufferError::HostAllocation)?;
        if self.buffer(buffer)?.used_blocks.len()
            >= self.buffer(buffer)?.native_used_capacity as usize
        {
            let capacity = self
                .buffer(buffer)?
                .native_used_capacity
                .checked_add(256)
                .ok_or(BufferError::InvalidExtent)?;
            self.buffer_mut(buffer)?.native_used_capacity = capacity;
        }
        self.block_mut(block)?.owner = Some(owner);
        self.owner_mut(owner)?.lease = OwnerLease::Active {
            block,
            published: false,
        };
        self.buffer_mut(buffer)?.used_blocks.push(block);
        self.buffer_mut(buffer)?.free_blocks.remove(position);
        self.block_mut(block)?.count = count;
        self.buffer_mut(buffer)?.num_free -= count;
        if rest > 0 {
            let tail = self.new_block(buffer, None, base + count, rest, backend)?;
            self.insert_free(buffer, tail)?;
        }
        Ok(Some(base))
    }
    pub fn allocate_index_at<B: BufferBackend + ?Sized>(
        &mut self,
        buffer: IndexBufferKey,
        owner: IndexOwnerKey,
        count: u32,
        backend: &mut B,
    ) -> Result<Option<u32>, OperationError<B::Error>> {
        self.allocate_at(
            BufferKey::Index(buffer),
            OwnerKey::Index(owner),
            count,
            backend,
        )
    }
    pub fn allocate_vertex_at<B: BufferBackend + ?Sized>(
        &mut self,
        buffer: VertexBufferKey,
        owner: VertexOwnerKey,
        count: u32,
        backend: &mut B,
    ) -> Result<Option<u32>, OperationError<B::Error>> {
        self.allocate_at(
            BufferKey::Vertex(buffer),
            OwnerKey::Vertex(owner),
            count,
            backend,
        )
    }
    fn new_buffer<B: BufferBackend + ?Sized>(
        &mut self,
        kind: BufferKind,
        backend: &mut B,
    ) -> Result<BufferKey, OperationError<B::Error>> {
        let slot = Self::next_slot(self.buffers.len())?;
        self.buffers
            .try_reserve(1)
            .map_err(|_| BufferError::HostAllocation)?;
        self.order_mut(kind)
            .try_reserve(1)
            .map_err(|_| BufferError::HostAllocation)?;
        self.locked_mut(kind)
            .try_reserve(1)
            .map_err(|_| BufferError::HostAllocation)?;
        let mut used_blocks = Vec::new();
        used_blocks
            .try_reserve_exact(256)
            .map_err(|_| BufferError::HostAllocation)?;
        let mut free_blocks = Vec::new();
        free_blocks
            .try_reserve(1)
            .map_err(|_| BufferError::HostAllocation)?;
        let allocation = self.call(backend.allocate_buffer(kind))?;
        if allocation.0 == 0 {
            return Err(self.invalid_backend());
        }
        let key = match kind {
            BufferKind::Index => BufferKey::Index(IndexBufferKey {
                manager: self.identity,
                slot,
            }),
            BufferKind::Vertex => BufferKey::Vertex(VertexBufferKey {
                manager: self.identity,
                slot,
            }),
        };
        self.buffers.push(Some(BufferState {
            key,
            usage: 0,
            pool: Pool::Managed,
            capacity: 0,
            num_free: 0,
            vertex: (kind == BufferKind::Vertex).then_some(VertexConfiguration {
                fvf: 0,
                stride: None,
                format: 0,
            }),
            graphics: None,
            data: None,
            flags: 0,
            lock_index: None,
            free_blocks,
            used_blocks,
            native_used_capacity: 256,
            allocation,
        }));
        Ok(key)
    }
    fn init_buffer<B: BufferBackend + ?Sized>(
        &mut self,
        key: BufferKey,
        usage: u32,
        count: u32,
        vertex: Option<VertexRequest>,
        backend: &mut B,
    ) -> Result<bool, OperationError<B::Error>> {
        let (capacity, bytes, fvf) = match vertex {
            Some(v) => {
                let capacity = (0x20000 / v.stride).max(count);
                let bytes = capacity
                    .checked_mul(v.stride)
                    .filter(|n| *n <= i32::MAX as u32)
                    .ok_or(BufferError::InvalidExtent)?;
                (capacity, bytes, Some(v.fvf))
            }
            None => {
                let capacity = count.max(0x10000);
                let bytes = capacity
                    .checked_mul(2)
                    .filter(|n| *n <= i32::MAX as u32)
                    .ok_or(BufferError::InvalidExtent)?;
                (capacity, bytes, None)
            }
        };
        let pool = if usage & 0x200 == 0 && usage & 0x10 != 0 {
            Pool::Managed
        } else {
            Pool::Default
        };
        {
            let buffer = self.buffer_mut(key)?;
            buffer.usage = usage;
            buffer.capacity = capacity;
            buffer.num_free = capacity;
            buffer.pool = pool;
            if let Some(v) = vertex {
                buffer.vertex = Some(VertexConfiguration {
                    fvf: v.fvf,
                    stride: Some(v.stride),
                    format: v.format,
                });
            }
        }
        let mut last_error = 0;
        for attempt in 0..3 {
            if attempt == 1 {
                self.call(backend.discard_managed())?;
            }
            if attempt == 2 {
                self.buffer_mut(key)?.pool = Pool::Managed;
            }
            let request = GraphicsRequest {
                kind: key.kind(),
                bytes,
                usage,
                pool: self.buffer(key)?.pool,
                fvf,
            };
            match self.call(backend.create_graphics(request))? {
                CreateResult::Created(graphics) => {
                    self.buffer_mut(key)?.graphics = Some(graphics);
                    let valid = match graphics {
                        GraphicsBuffer::Index(h) => key.kind() == BufferKind::Index && h.0 != 0,
                        GraphicsBuffer::Vertex(h) => key.kind() == BufferKind::Vertex && h.0 != 0,
                    };
                    if !valid {
                        return Err(self.invalid_backend());
                    }
                    break;
                }
                CreateResult::Rejected(hr) => {
                    if hr >= 0 {
                        return Err(self.invalid_backend());
                    }
                    last_error = hr;
                }
            }
        }
        if self.buffer(key)?.graphics.is_none() {
            self.call(backend.out_of_memory(key.kind(), bytes, last_error))?;
            return Ok(false);
        }
        if key.kind() == BufferKind::Vertex {
            self.call(backend.recompute_vram())?;
        }
        let first = self.new_block(key, None, 0, capacity, backend)?;
        self.insert_free(key, first)?;
        if key.kind() == BufferKind::Index {
            self.call(backend.recompute_vram())?;
        }
        let graphics = self.buffer(key)?.graphics.unwrap();
        let data = self.call(backend.lock(graphics, (usage & 0x200) << 4))?;
        self.buffer_mut(key)?.data = Some(data);
        if data.0 == 0 {
            return Err(self.invalid_backend());
        }
        let lock_index = self.locked(key.kind()).len();
        self.buffer_mut(key)?.lock_index = Some(lock_index);
        self.locked_mut(key.kind()).push(key);
        Ok(true)
    }
    fn insertion_available(&self, kind: BufferKind) -> Result<(), BufferError> {
        for pair in self.order(kind).windows(2) {
            if self.buffer(pair[0])?.num_free > self.buffer(pair[1])?.num_free {
                return Err(BufferError::UnsupportedManagerInsertion(kind));
            }
        }
        Ok(())
    }
    fn insert_buffer(&mut self, key: BufferKey) -> Result<(), BufferError> {
        let free = self.buffer(key)?.num_free;
        let position = self
            .order(key.kind())
            .iter()
            .position(|k| self.buffer(*k).unwrap().num_free > free)
            .unwrap_or(self.order(key.kind()).len());
        self.order_mut(key.kind()).insert(position, key);
        Ok(())
    }
    fn publish(&mut self, owner: OwnerKey, key: BufferKey, base: u32) -> Result<(), BufferError> {
        let graphics = self.buffer(key)?.graphics;
        let state = self.owner_mut(owner)?;
        state.descriptor.buffer = Some(key);
        state.descriptor.base = base;
        state.descriptor.graphics = graphics;
        if let OwnerLease::Active { block, .. } = state.lease {
            state.lease = OwnerLease::Active {
                block,
                published: true,
            };
        }
        Ok(())
    }
    fn allocate<B: BufferBackend + ?Sized>(
        &mut self,
        owner: OwnerKey,
        usage: u32,
        count: u32,
        vertex: Option<VertexRequest>,
        backend: &mut B,
    ) -> Result<bool, OperationError<B::Error>> {
        self.available()?;
        Self::extent(count)?;
        self.owner_available(owner)?;
        let kind = owner.kind();
        if !vertex.is_some_and(|v| v.exclusive) {
            let length = self.order(kind).len();
            for position in 0..length {
                let key = self.order(kind)[position];
                let buffer = self.buffer(key)?;
                let compatible = buffer.usage == usage
                    && buffer.num_free >= count
                    && vertex.is_none_or(|v| buffer.vertex.unwrap().fvf == v.fvf);
                if compatible {
                    if let Some(base) = self.allocate_at(key, owner, count, backend)? {
                        self.publish(owner, key, base)?;
                        return Ok(true);
                    }
                }
            }
        }
        // No sorting or synthetic insertion key: only this new-node path is
        // unavailable when current values violate the retained tree order.
        if !backend.driver_present() {
            return Err(BufferError::DriverUnavailable.into());
        }
        if let Some(v) = vertex {
            if v.stride == 0
                || v.stride > i32::MAX as u32
                || v.count
                    .checked_mul(v.stride)
                    .is_none_or(|n| n > i32::MAX as u32)
            {
                return Err(BufferError::InvalidExtent.into());
            }
        } else if count.checked_mul(2).is_none_or(|n| n > i32::MAX as u32) {
            return Err(BufferError::InvalidExtent.into());
        }
        self.insertion_available(kind)?;
        let key = self.new_buffer(kind, backend)?;
        if !self.init_buffer(key, usage, count, vertex, backend)? {
            self.destroy_buffer(key, backend)?;
            return Ok(false);
        }
        self.insert_buffer(key)?;
        self.owner_mut(owner)?.descriptor.buffer = Some(key);
        let Some(base) = self.allocate_at(key, owner, count, backend)? else {
            self.poisoned = true;
            return Err(BufferError::InvalidExtent.into());
        };
        self.publish(owner, key, base)?;
        Ok(true)
    }
    pub fn allocate_index<B: BufferBackend + ?Sized>(
        &mut self,
        owner: IndexOwnerKey,
        usage: u32,
        count: u32,
        backend: &mut B,
    ) -> Result<bool, OperationError<B::Error>> {
        self.allocate(OwnerKey::Index(owner), usage, count, None, backend)
    }
    pub fn allocate_vertex<B: BufferBackend + ?Sized>(
        &mut self,
        owner: VertexOwnerKey,
        request: VertexRequest,
        backend: &mut B,
    ) -> Result<bool, OperationError<B::Error>> {
        self.allocate(
            OwnerKey::Vertex(owner),
            request.usage,
            request.count,
            Some(request),
            backend,
        )
    }
    fn free_at<B: BufferBackend + ?Sized>(
        &mut self,
        key: BufferKey,
        base: u32,
        backend: &mut B,
    ) -> Result<bool, OperationError<B::Error>> {
        self.available()?;
        let position = self
            .buffer(key)?
            .used_blocks
            .iter()
            .position(|b| self.block(*b).unwrap().base == base);
        let Some(position) = position else {
            return Ok(false);
        };
        let block = self.buffer(key)?.used_blocks[position];
        let size = self.block(block)?.count;
        self.buffer_mut(key)?
            .free_blocks
            .try_reserve(1)
            .map_err(|_| BufferError::HostAllocation)?;
        self.buffer_mut(key)?.num_free += size;
        self.buffer_mut(key)?.used_blocks.swap_remove(position);
        if let Some(owner) = self.block(block)?.owner {
            self.owner_mut(owner)?.lease = OwnerLease::Freed;
        }
        let mut start = base;
        let mut count = size;
        let mut joined = 0;
        loop {
            let neighbor =
                self.buffer(key)?
                    .free_blocks
                    .iter()
                    .enumerate()
                    .find_map(|(index, k)| {
                        let other = self.block(*k).unwrap();
                        if other.base == start + count {
                            Some((index, *k, 1))
                        } else if other.base + other.count == start {
                            Some((index, *k, 2))
                        } else {
                            None
                        }
                    });
            let Some((index, neighbor, direction)) = neighbor else {
                break;
            };
            let other = self.block(neighbor)?;
            if direction == 2 {
                start = other.base;
            }
            count += other.count;
            let allocation = other.allocation;
            self.buffer_mut(key)?.free_blocks.remove(index);
            self.call(backend.free_block(allocation))?;
            self.blocks[(neighbor.slot - 1) as usize] = None;
            joined |= direction;
            if joined == 3 {
                break;
            }
        }
        if joined != 0 {
            let block = self.block_mut(block)?;
            block.base = start;
            block.count = count;
        }
        self.insert_free(key, block)?;
        if key.kind() == BufferKind::Vertex
            && self.buffer(key)?.num_free == self.buffer(key)?.capacity
        {
            // The targeted VB path erases manager membership BEFORE destruction.
            if let Some(position) = self.order(key.kind()).iter().position(|k| *k == key) {
                self.order_mut(key.kind()).remove(position);
            }
            if let Err(error) = self.destroy_buffer(key, backend) {
                self.poisoned = true;
                return Err(error);
            }
        }
        Ok(true)
    }
    /// Working-name projection of the original address-based child free. The
    /// caller supplies a CURRENT base, not a historical owner descriptor: a base
    /// reused by another owner intentionally identifies that new block. Use
    /// free_index(owner) for lease-checked stale-descriptor protection.
    pub fn free_index_at<B: BufferBackend + ?Sized>(
        &mut self,
        key: IndexBufferKey,
        base: u32,
        backend: &mut B,
    ) -> Result<bool, OperationError<B::Error>> {
        self.free_at(BufferKey::Index(key), base, backend)
    }
    /// Address-based child free with the same current-base precondition as
    /// free_index_at. Lease-checked callers should use free_vertex(owner).
    pub fn free_vertex_at<B: BufferBackend + ?Sized>(
        &mut self,
        key: VertexBufferKey,
        base: u32,
        backend: &mut B,
    ) -> Result<bool, OperationError<B::Error>> {
        self.free_at(BufferKey::Vertex(key), base, backend)
    }
    fn free_owner<B: BufferBackend + ?Sized>(
        &mut self,
        owner: OwnerKey,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        self.available()?;
        let state = self.owner(owner)?;
        match state.lease {
            OwnerLease::Freed => return Err(BufferError::StaleOwner(owner).into()),
            OwnerLease::Active {
                published: false, ..
            } => return Err(BufferError::UnpublishedOwner(owner).into()),
            OwnerLease::Active { block, .. } => {
                let raw = self.block(block)?;
                if state.descriptor.buffer != Some(raw.buffer)
                    || state.descriptor.base != raw.base
                    || raw.owner != Some(owner)
                {
                    return Err(BufferError::StaleOwner(owner).into());
                }
            }
            _ => {}
        }
        if let Some(key) = state.descriptor.buffer {
            self.free_at(key, state.descriptor.base, backend)?;
        }
        Ok(())
    }
    pub fn free_index<B: BufferBackend + ?Sized>(
        &mut self,
        owner: IndexOwnerKey,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        self.free_owner(OwnerKey::Index(owner), backend)
    }
    pub fn free_vertex<B: BufferBackend + ?Sized>(
        &mut self,
        owner: VertexOwnerKey,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        self.free_owner(OwnerKey::Vertex(owner), backend)
    }
    fn destroy_buffer<B: BufferBackend + ?Sized>(
        &mut self,
        key: BufferKey,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        let state = self.buffer(key)?;
        if state.data.is_some() {
            let index = state.lock_index.ok_or(BufferError::InvalidLockState)?;
            if self.locked(key.kind()).get(index) != Some(&key) {
                return Err(BufferError::InvalidLockState.into());
            }
        }
        let used_count = state.used_blocks.len();
        for index in 0..used_count {
            let block = self.buffer(key)?.used_blocks[index];
            let state = self.block(block)?;
            let allocation = state.allocation;
            if let Some(owner) = state.owner {
                let state = self.owner_mut(owner)?;
                state.descriptor = Descriptor::default();
                state.lease = OwnerLease::Invalidated;
            }
            self.call(backend.free_block(allocation))?;
            self.blocks[(block.slot - 1) as usize] = None;
        }
        let free_count = self.buffer(key)?.free_blocks.len();
        for index in 0..free_count {
            let block = self.buffer(key)?.free_blocks[index];
            let allocation = self.block(block)?.allocation;
            self.call(backend.free_block(allocation))?;
            self.blocks[(block.slot - 1) as usize] = None;
        }
        if self.buffer(key)?.data.is_some() {
            let graphics = self
                .buffer(key)?
                .graphics
                .ok_or(BufferError::InvalidLockState)?;
            self.call(backend.unlock(graphics))?;
            if key.kind() == BufferKind::Index && self.buffer(key)?.flags & 0x10 == 0 {
                let data = self.call(backend.lock(graphics, 0))?;
                self.buffer_mut(key)?.data = Some(data);
                if data.0 == 0 {
                    return Err(self.invalid_backend());
                }
                self.call(backend.unlock(graphics))?;
            }
            self.buffer_mut(key)?.data = None;
            let index = self.buffer(key)?.lock_index.unwrap();
            let moved = *self
                .locked(key.kind())
                .last()
                .ok_or(BufferError::InvalidLockState)?;
            self.buffer_mut(moved)?.lock_index = Some(index);
            self.locked_mut(key.kind()).swap_remove(index);
            self.buffer_mut(key)?.lock_index = None;
        }
        if let Some(graphics) = self.buffer(key)?.graphics {
            self.call(backend.release_graphics(graphics))?;
        }
        if backend.driver_present() {
            self.call(backend.recompute_vram())?;
        }
        let allocation = self.buffer(key)?.allocation;
        self.call(backend.free_buffer(allocation))?;
        self.buffers[(key.parts().1 - 1) as usize] = None;
        Ok(())
    }
    fn sweep<B: BufferBackend + ?Sized>(
        &mut self,
        kind: BufferKind,
        unused_only: bool,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        self.available()?;
        loop {
            let chosen = self.order(kind).iter().copied().find(|k| {
                !unused_only
                    || self.buffer(*k).unwrap().num_free == self.buffer(*k).unwrap().capacity
            });
            let Some(key) = chosen else { break };
            if !unused_only
                && (!self.buffer(key)?.used_blocks.is_empty()
                    || self.buffer(key)?.free_blocks.len() != 1)
            {
                self.call(backend.warn_unreleased_buffer(key))?;
            }
            // Sweep paths retain membership until the destructor returns.
            if let Err(error) = self.destroy_buffer(key, backend) {
                self.poisoned = true;
                return Err(error);
            }
            let index = self.order(kind).iter().position(|k| *k == key).unwrap();
            self.order_mut(kind).remove(index);
        }
        Ok(())
    }
    pub fn reclaim_unused_indices<B: BufferBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        self.sweep(BufferKind::Index, true, backend)
    }
    pub fn reclaim_unused_vertices<B: BufferBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        self.sweep(BufferKind::Vertex, true, backend)
    }
    pub fn free_all_indices<B: BufferBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        self.sweep(BufferKind::Index, false, backend)
    }
    pub fn free_all_vertices<B: BufferBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        self.sweep(BufferKind::Vertex, false, backend)
    }
    fn unlock_all<B: BufferBackend + ?Sized>(
        &mut self,
        kind: BufferKind,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        self.available()?;
        let length = self.locked(kind).len();
        for index in 0..length {
            let key = self.locked(kind)[index];
            let graphics = self
                .buffer(key)?
                .graphics
                .ok_or(BufferError::InvalidLockState)?;
            self.call(backend.unlock(graphics))?;
            if kind == BufferKind::Index && self.buffer(key)?.flags & 0x10 == 0 {
                let data = self.call(backend.lock(graphics, 0))?;
                self.buffer_mut(key)?.data = Some(data);
                if data.0 == 0 {
                    return Err(self.invalid_backend());
                }
                self.call(backend.unlock(graphics))?;
            }
            self.buffer_mut(key)?.data = None;
            self.buffer_mut(key)?.lock_index = None;
        }
        self.locked_mut(kind).clear();
        Ok(())
    }
    pub fn unlock_all_indices<B: BufferBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        self.unlock_all(BufferKind::Index, backend)
    }
    pub fn unlock_all_vertices<B: BufferBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), OperationError<B::Error>> {
        self.unlock_all(BufferKind::Vertex, backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, VecDeque};
    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        NewBuffer(BufferKind, AllocationToken),
        NewBlock(BufferKind, u32, u32, AllocationToken),
        FreeBuffer(AllocationToken),
        FreeBlock(AllocationToken),
        Create(GraphicsRequest),
        Discard,
        Lock(GraphicsBuffer, u32),
        Unlock(GraphicsBuffer),
        Release(GraphicsBuffer),
        Vram,
        Warn(BufferKey),
        Oom(BufferKind, u32, i32),
    }
    struct Graphics {
        bytes: Vec<u8>,
        locked: bool,
    }
    struct Backend {
        next: u64,
        allocations: BTreeMap<u64, bool>,
        graphics: BTreeMap<u64, Graphics>,
        events: Vec<Event>,
        rejects: VecDeque<i32>,
        driver: bool,
        block_calls: usize,
        fail_block: Option<usize>,
        fail_release_after: bool,
    }
    impl Default for Backend {
        fn default() -> Self {
            Self {
                next: 1,
                allocations: BTreeMap::new(),
                graphics: BTreeMap::new(),
                events: Vec::new(),
                rejects: VecDeque::new(),
                driver: true,
                block_calls: 0,
                fail_block: None,
                fail_release_after: false,
            }
        }
    }
    impl Backend {
        fn next(&mut self) -> u64 {
            let n = self.next;
            self.next += 1;
            n
        }
        fn graphics_id(graphics: GraphicsBuffer) -> u64 {
            match graphics {
                GraphicsBuffer::Index(k) => k.0,
                GraphicsBuffer::Vertex(k) => k.0,
            }
        }
    }
    impl BufferBackend for Backend {
        type Error = &'static str;
        fn allocate_buffer(&mut self, kind: BufferKind) -> Result<AllocationToken, Self::Error> {
            let t = AllocationToken(self.next());
            self.allocations.insert(t.0, true);
            self.events.push(Event::NewBuffer(kind, t));
            Ok(t)
        }
        fn free_buffer(&mut self, t: AllocationToken) -> Result<(), Self::Error> {
            self.events.push(Event::FreeBuffer(t));
            assert_eq!(self.allocations.remove(&t.0), Some(true));
            Ok(())
        }
        fn allocate_block(
            &mut self,
            kind: BufferKind,
            base: u32,
            count: u32,
        ) -> Result<AllocationToken, Self::Error> {
            self.block_calls += 1;
            if self.fail_block == Some(self.block_calls) {
                return Err("block unavailable");
            }
            let t = AllocationToken(self.next());
            self.allocations.insert(t.0, false);
            self.events.push(Event::NewBlock(kind, base, count, t));
            Ok(t)
        }
        fn free_block(&mut self, t: AllocationToken) -> Result<(), Self::Error> {
            self.events.push(Event::FreeBlock(t));
            assert_eq!(self.allocations.remove(&t.0), Some(false));
            Ok(())
        }
        fn create_graphics(
            &mut self,
            request: GraphicsRequest,
        ) -> Result<CreateResult, Self::Error> {
            self.events.push(Event::Create(request));
            if let Some(hr) = self.rejects.pop_front() {
                return Ok(CreateResult::Rejected(hr));
            }
            let id = self.next();
            self.graphics.insert(
                id,
                Graphics {
                    bytes: vec![0; request.bytes as usize],
                    locked: false,
                },
            );
            Ok(CreateResult::Created(match request.kind {
                BufferKind::Index => GraphicsBuffer::Index(IndexBufferHandle(id)),
                BufferKind::Vertex => GraphicsBuffer::Vertex(VertexBufferHandle(id)),
            }))
        }
        fn discard_managed(&mut self) -> Result<(), Self::Error> {
            self.events.push(Event::Discard);
            Ok(())
        }
        fn lock(
            &mut self,
            graphics: GraphicsBuffer,
            flags: u32,
        ) -> Result<LockedDataKey, Self::Error> {
            self.events.push(Event::Lock(graphics, flags));
            let state = self.graphics.get_mut(&Self::graphics_id(graphics)).unwrap();
            assert!(!state.bytes.is_empty());
            assert!(!state.locked);
            state.locked = true;
            Ok(LockedDataKey(Self::graphics_id(graphics)))
        }
        fn unlock(&mut self, graphics: GraphicsBuffer) -> Result<(), Self::Error> {
            self.events.push(Event::Unlock(graphics));
            let state = self.graphics.get_mut(&Self::graphics_id(graphics)).unwrap();
            assert!(state.locked);
            state.locked = false;
            Ok(())
        }
        fn release_graphics(&mut self, graphics: GraphicsBuffer) -> Result<(), Self::Error> {
            self.events.push(Event::Release(graphics));
            let state = self.graphics.remove(&Self::graphics_id(graphics)).unwrap();
            assert!(!state.locked);
            if self.fail_release_after {
                Err("released then interrupted")
            } else {
                Ok(())
            }
        }
        fn driver_present(&self) -> bool {
            self.driver
        }
        fn recompute_vram(&mut self) -> Result<(), Self::Error> {
            assert!(self.driver);
            self.events.push(Event::Vram);
            Ok(())
        }
        fn warn_unreleased_buffer(&mut self, key: BufferKey) -> Result<(), Self::Error> {
            self.events.push(Event::Warn(key));
            Ok(())
        }
        fn out_of_memory(
            &mut self,
            kind: BufferKind,
            bytes: u32,
            hr: i32,
        ) -> Result<(), Self::Error> {
            self.events.push(Event::Oom(kind, bytes, hr));
            Ok(())
        }
    }
    fn index_key(managers: &BufferManagers, owner: IndexOwnerKey) -> IndexBufferKey {
        match managers
            .index_owner(owner)
            .unwrap()
            .descriptor
            .buffer
            .unwrap()
        {
            BufferKey::Index(k) => k,
            _ => panic!("wrong kind"),
        }
    }
    fn vertex_key(managers: &BufferManagers, owner: VertexOwnerKey) -> VertexBufferKey {
        match managers
            .vertex_owner(owner)
            .unwrap()
            .descriptor
            .buffer
            .unwrap()
        {
            BufferKey::Vertex(k) => k,
            _ => panic!("wrong kind"),
        }
    }
    fn vertex(count: u32) -> VertexRequest {
        VertexRequest {
            fvf: 0x112,
            usage: 0x10,
            stride: 32,
            count,
            format: 6,
            exclusive: false,
        }
    }

    #[test]
    fn index_partition_swap_remove_and_stale_owner_cannot_free_reused_base() {
        let mut m = BufferManagers::new().unwrap();
        let mut b = Backend::default();
        let a = m.create_index_owner().unwrap();
        let middle = m.create_index_owner().unwrap();
        let c = m.create_index_owner().unwrap();
        m.allocate_index(a, 0x10, 10, &mut b).unwrap();
        m.allocate_index(middle, 0x10, 20, &mut b).unwrap();
        m.allocate_index(c, 0x10, 30, &mut b).unwrap();
        let key = index_key(&m, a);
        let initial = m.index_buffer(key).unwrap().used_blocks.clone();
        assert_eq!(m.block(initial[0]).unwrap().base, 0);
        assert_eq!(m.block(initial[1]).unwrap().base, 10);
        assert_eq!(m.block(initial[2]).unwrap().base, 30);
        let raw = m.index_owner(middle).unwrap().descriptor;
        m.free_index(middle, &mut b).unwrap();
        assert_eq!(
            m.index_buffer(key).unwrap().used_blocks,
            [initial[0], initial[2]]
        );
        assert_eq!(
            m.block(initial[1]).unwrap().owner,
            Some(OwnerKey::Index(middle))
        );
        assert_eq!(m.index_owner(middle).unwrap().descriptor, raw);
        let replacement = m.create_index_owner().unwrap();
        m.allocate_index(replacement, 0x10, 20, &mut b).unwrap();
        assert_eq!(m.index_owner(replacement).unwrap().descriptor, raw);
        let before = b.events.len();
        assert_eq!(
            m.free_index(middle, &mut b),
            Err(BufferError::StaleOwner(OwnerKey::Index(middle)).into())
        );
        assert_eq!(b.events.len(), before);
        m.free_index(a, &mut b).unwrap();
        m.free_index(replacement, &mut b).unwrap();
        m.free_index(c, &mut b).unwrap();
        assert_eq!(m.index_buffers(), [BufferKey::Index(key)]);
        let state = m.index_buffer(key).unwrap();
        assert_eq!(state.num_free, state.capacity);
        assert_eq!(state.free_blocks.len(), 1);
        m.reclaim_unused_indices(&mut b).unwrap();
        assert!(m.index_buffer(key).is_err());
        assert_eq!(m.index_owner(middle).unwrap().descriptor, raw);
        assert!(b.graphics.is_empty() && b.allocations.is_empty());
    }
    #[test]
    fn equal_size_holes_choose_the_first_reinserted_record() {
        for reverse in [false, true] {
            let mut m = BufferManagers::new().unwrap();
            let mut b = Backend::default();
            let owners: Vec<_> = (0..5).map(|_| m.create_index_owner().unwrap()).collect();
            for &owner in &owners {
                m.allocate_index(owner, 0x10, 10, &mut b).unwrap();
            }
            let first = if reverse { owners[1] } else { owners[3] };
            let second = if reverse { owners[3] } else { owners[1] };
            m.free_index(first, &mut b).unwrap();
            m.free_index(second, &mut b).unwrap();
            let next = m.create_index_owner().unwrap();
            m.allocate_index(next, 0x10, 10, &mut b).unwrap();
            assert_eq!(
                m.index_owner(next).unwrap().descriptor.base,
                if reverse { 10 } else { 30 }
            );
            m.free_all_indices(&mut b).unwrap();
            assert!(b.allocations.is_empty());
        }
    }
    #[test]
    fn coalescing_scans_size_order_and_retains_reclaimed_owner() {
        let mut m = BufferManagers::new().unwrap();
        let mut b = Backend::default();
        let owners: Vec<_> = (0..4).map(|_| m.create_index_owner().unwrap()).collect();
        for (&owner, count) in owners.iter().zip([10, 20, 30, 10]) {
            m.allocate_index(owner, 0x10, count, &mut b).unwrap();
        }
        let key = index_key(&m, owners[0]);
        let records = m.index_buffer(key).unwrap().used_blocks.clone();
        let left = m.block(records[0]).unwrap().allocation;
        let right = m.block(records[2]).unwrap().allocation;
        m.free_index(owners[0], &mut b).unwrap();
        m.free_index(owners[2], &mut b).unwrap();
        b.events.clear();
        m.free_index(owners[1], &mut b).unwrap();
        assert_eq!(b.events, [Event::FreeBlock(left), Event::FreeBlock(right)]);
        let merged = m.block(records[1]).unwrap();
        assert_eq!(
            (merged.base, merged.count, merged.owner),
            (0, 60, Some(OwnerKey::Index(owners[1])))
        );
        m.free_all_indices(&mut b).unwrap();
    }
    #[test]
    fn vertex_last_free_reclaims_immediately_and_repairs_swapped_lock_index() {
        let mut m = BufferManagers::new().unwrap();
        let mut b = Backend::default();
        let owners: Vec<_> = (0..3).map(|_| m.create_vertex_owner().unwrap()).collect();
        for &owner in &owners {
            m.allocate_vertex(
                owner,
                VertexRequest {
                    exclusive: true,
                    ..vertex(10)
                },
                &mut b,
            )
            .unwrap();
        }
        let keys: Vec<_> = owners.iter().map(|&o| vertex_key(&m, o)).collect();
        let raw = m.vertex_owner(owners[1]).unwrap().descriptor;
        m.free_vertex(owners[1], &mut b).unwrap();
        assert!(m.vertex_buffer(keys[1]).is_err());
        assert_eq!(
            m.vertex_locked(),
            [BufferKey::Vertex(keys[0]), BufferKey::Vertex(keys[2])]
        );
        assert_eq!(m.vertex_buffer(keys[2]).unwrap().lock_index, Some(1));
        assert_eq!(m.vertex_owner(owners[1]).unwrap().descriptor, raw);
        m.free_all_vertices(&mut b).unwrap();
        assert!(b.graphics.is_empty() && b.allocations.is_empty());
    }
    #[test]
    fn forced_destruction_clears_used_descriptors_but_preserves_freed_ones() {
        let mut m = BufferManagers::new().unwrap();
        let mut b = Backend::default();
        let freed = m.create_index_owner().unwrap();
        let used = m.create_index_owner().unwrap();
        m.allocate_index(freed, 0x10, 10, &mut b).unwrap();
        m.allocate_index(used, 0x10, 20, &mut b).unwrap();
        let key = index_key(&m, freed);
        let raw = m.index_owner(freed).unwrap().descriptor;
        m.free_index(freed, &mut b).unwrap();
        b.events.clear();
        m.free_all_indices(&mut b).unwrap();
        assert_eq!(b.events[0], Event::Warn(BufferKey::Index(key)));
        assert_eq!(
            m.index_owner(used).unwrap().descriptor,
            Descriptor::default()
        );
        assert_eq!(m.index_owner(used).unwrap().lease, OwnerLease::Invalidated);
        assert_eq!(m.index_owner(freed).unwrap().descriptor, raw);
        assert!(b.graphics.is_empty());
    }
    #[test]
    fn stale_manager_order_allows_reuse_and_blocks_only_new_insertion_before_effects() {
        let mut m = BufferManagers::new().unwrap();
        let mut b = Backend::default();
        let owners: Vec<_> = (0..3).map(|_| m.create_index_owner().unwrap()).collect();
        for (&owner, count) in owners.iter().zip([60000, 60000, 10000]) {
            m.allocate_index(owner, 0x10, count, &mut b).unwrap();
        }
        let order = m.index_buffers().to_vec();
        m.free_index(owners[0], &mut b).unwrap();
        let free: Vec<_> = m
            .index_buffers()
            .iter()
            .map(|k| m.buffer(*k).unwrap().num_free)
            .collect();
        assert_eq!(free, [65536, 5536, 55536]);
        let reuse = m.create_index_owner().unwrap();
        m.allocate_index(reuse, 0x10, 100, &mut b).unwrap();
        assert_eq!(
            m.index_owner(reuse).unwrap().descriptor.buffer,
            Some(order[0])
        );
        assert_eq!(m.index_buffers(), order);
        let new = m.create_index_owner().unwrap();
        let before = b.events.len();
        assert_eq!(
            m.allocate_index(new, 0x20, 1, &mut b),
            Err(BufferError::UnsupportedManagerInsertion(BufferKind::Index).into())
        );
        assert_eq!(b.events.len(), before);
        assert_eq!(m.index_owner(new).unwrap().lease, OwnerLease::Vacant);
        assert!(!m.is_poisoned());
        m.free_all_indices(&mut b).unwrap();
    }
    #[test]
    fn reuse_ignores_requested_stride_format_and_does_not_need_driver() {
        let mut m = BufferManagers::new().unwrap();
        let mut b = Backend::default();
        let a = m.create_vertex_owner().unwrap();
        let other = m.create_vertex_owner().unwrap();
        m.allocate_vertex(a, vertex(10), &mut b).unwrap();
        let key = vertex_key(&m, a);
        b.driver = false;
        m.allocate_vertex(
            other,
            VertexRequest {
                stride: 0,
                format: 99,
                ..vertex(3)
            },
            &mut b,
        )
        .unwrap();
        assert_eq!(vertex_key(&m, other), key);
        assert_eq!(
            m.vertex_buffer(key).unwrap().vertex,
            Some(VertexConfiguration {
                fvf: 0x112,
                stride: Some(32),
                format: 6
            })
        );
        m.free_all_vertices(&mut b).unwrap();
        assert!(b.graphics.is_empty());
    }
    #[test]
    fn graphics_retries_change_only_third_pool_and_init_callback_order_differs() {
        for kind in [BufferKind::Index, BufferKind::Vertex] {
            let mut m = BufferManagers::new().unwrap();
            let mut b = Backend::default();
            b.rejects.extend([-1, -2]);
            match kind {
                BufferKind::Index => {
                    let o = m.create_index_owner().unwrap();
                    m.allocate_index(o, 0x200, 10, &mut b).unwrap();
                }
                BufferKind::Vertex => {
                    let o = m.create_vertex_owner().unwrap();
                    m.allocate_vertex(
                        o,
                        VertexRequest {
                            usage: 0x200,
                            ..vertex(10)
                        },
                        &mut b,
                    )
                    .unwrap();
                }
            }
            let pools: Vec<_> = b
                .events
                .iter()
                .filter_map(|e| {
                    if let Event::Create(r) = e {
                        Some(r.pool)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(pools, [Pool::Default, Pool::Default, Pool::Managed]);
            assert_eq!(b.events.iter().filter(|e| **e == Event::Discard).count(), 1);
            let block = b
                .events
                .iter()
                .position(|e| matches!(e, Event::NewBlock(..)))
                .unwrap();
            let vram = b.events.iter().position(|e| *e == Event::Vram).unwrap();
            assert_eq!(block < vram, kind == BufferKind::Index);
            assert!(b.events.iter().any(|e| matches!(e, Event::Lock(_, 0x2000))));
            let key = m.order(kind)[0];
            assert_eq!(m.buffer(key).unwrap().flags, 0);
            match kind {
                BufferKind::Index => m.free_all_indices(&mut b).unwrap(),
                BufferKind::Vertex => m.free_all_vertices(&mut b).unwrap(),
            };
            assert!(b.allocations.is_empty());
        }
    }
    #[test]
    fn all_three_graphics_rejections_return_false_and_release_partial_object() {
        for kind in [BufferKind::Index, BufferKind::Vertex] {
            let mut m = BufferManagers::new().unwrap();
            let mut b = Backend::default();
            b.rejects.extend([-1, -2, -3]);
            let success = match kind {
                BufferKind::Index => {
                    let o = m.create_index_owner().unwrap();
                    m.allocate_index(o, 0, 1, &mut b).unwrap()
                }
                BufferKind::Vertex => {
                    let o = m.create_vertex_owner().unwrap();
                    m.allocate_vertex(o, vertex(1), &mut b).unwrap()
                }
            };
            assert!(!success);
            assert!(!m.is_poisoned());
            assert!(m.order(kind).is_empty());
            assert!(m.locked(kind).is_empty());
            assert!(b.allocations.is_empty() && b.graphics.is_empty());
            assert!(b.events.iter().any(|e| matches!(e, Event::Oom(_, _, -3))));
            assert!(!b.events.iter().any(|e| matches!(e, Event::NewBlock(..))));
        }
    }
    #[test]
    fn split_backend_failure_preserves_published_buffer_and_partition_prefix_without_retry() {
        let mut m = BufferManagers::new().unwrap();
        let mut b = Backend {
            fail_block: Some(2),
            ..Default::default()
        };
        let owner = m.create_index_owner().unwrap();
        assert_eq!(
            m.allocate_index(owner, 0x10, 10, &mut b),
            Err(OperationError::Backend("block unavailable"))
        );
        assert!(m.is_poisoned());
        let state = m.index_owner(owner).unwrap();
        assert!(state.descriptor.buffer.is_some());
        assert_eq!(state.descriptor.graphics, None);
        assert!(matches!(
            state.lease,
            OwnerLease::Active {
                published: false,
                ..
            }
        ));
        let buffer = m.buffer(state.descriptor.buffer.unwrap()).unwrap();
        assert_eq!(buffer.num_free, 65526);
        assert_eq!(buffer.used_blocks.len(), 1);
        assert!(buffer.free_blocks.is_empty());
        let before = b.events.len();
        assert_eq!(
            m.allocate_index(owner, 0x10, 10, &mut b),
            Err(BufferError::Poisoned.into())
        );
        assert_eq!(b.events.len(), before);
    }
    #[test]
    fn effectful_destruction_failure_keeps_targeted_vs_sweep_membership_prefixes() {
        for kind in [BufferKind::Index, BufferKind::Vertex] {
            let mut m = BufferManagers::new().unwrap();
            let mut b = Backend::default();
            let (key, owner) = match kind {
                BufferKind::Index => {
                    let o = m.create_index_owner().unwrap();
                    m.allocate_index(o, 0x10, 1, &mut b).unwrap();
                    (BufferKey::Index(index_key(&m, o)), OwnerKey::Index(o))
                }
                BufferKind::Vertex => {
                    let o = m.create_vertex_owner().unwrap();
                    m.allocate_vertex(o, vertex(1), &mut b).unwrap();
                    (BufferKey::Vertex(vertex_key(&m, o)), OwnerKey::Vertex(o))
                }
            };
            b.fail_release_after = true;
            let result = match owner {
                OwnerKey::Index(_) => m.free_all_indices(&mut b),
                OwnerKey::Vertex(o) => m.free_vertex(o, &mut b),
            };
            assert_eq!(
                result,
                Err(OperationError::Backend("released then interrupted"))
            );
            assert!(m.is_poisoned());
            assert!(m.buffer(key).is_ok());
            assert_eq!(m.order(kind).contains(&key), kind == BufferKind::Index);
            assert!(b.graphics.is_empty());
            assert!(m.locked(kind).is_empty());
            let before = b.events.len();
            assert_eq!(
                m.reclaim_unused_indices(&mut b),
                Err(BufferError::Poisoned.into())
            );
            assert_eq!(b.events.len(), before);
            if kind == BufferKind::Index {
                assert_eq!(m.owner(owner).unwrap().descriptor, Descriptor::default());
            }
        }
    }
    #[test]
    fn unlock_all_preserves_ib_relock_sequence_and_vertex_single_unlock() {
        for kind in [BufferKind::Index, BufferKind::Vertex] {
            let mut m = BufferManagers::new().unwrap();
            let mut b = Backend::default();
            let key = match kind {
                BufferKind::Index => {
                    let o = m.create_index_owner().unwrap();
                    m.allocate_index(o, 0x10, 1, &mut b).unwrap();
                    BufferKey::Index(index_key(&m, o))
                }
                BufferKind::Vertex => {
                    let o = m.create_vertex_owner().unwrap();
                    m.allocate_vertex(o, vertex(1), &mut b).unwrap();
                    BufferKey::Vertex(vertex_key(&m, o))
                }
            };
            let graphics = m.buffer(key).unwrap().graphics.unwrap();
            b.events.clear();
            match kind {
                BufferKind::Index => m.unlock_all_indices(&mut b).unwrap(),
                BufferKind::Vertex => m.unlock_all_vertices(&mut b).unwrap(),
            };
            assert_eq!(
                b.events,
                if kind == BufferKind::Index {
                    vec![
                        Event::Unlock(graphics),
                        Event::Lock(graphics, 0),
                        Event::Unlock(graphics),
                    ]
                } else {
                    vec![Event::Unlock(graphics)]
                }
            );
            assert_eq!(m.buffer(key).unwrap().data, None);
            assert_eq!(m.buffer(key).unwrap().lock_index, None);
            assert!(m.locked(kind).is_empty());
            match kind {
                BufferKind::Index => m.free_all_indices(&mut b).unwrap(),
                BufferKind::Vertex => m.free_all_vertices(&mut b).unwrap(),
            };
        }
    }
    #[test]
    fn checked_extent_foreign_owner_no_fit_and_missing_free_have_no_effects() {
        let mut m = BufferManagers::new().unwrap();
        let mut foreign = BufferManagers::new().unwrap();
        let mut b = Backend::default();
        let owner = m.create_index_owner().unwrap();
        assert_eq!(
            m.allocate_index(owner, 0x10, 0, &mut b),
            Err(BufferError::InvalidExtent.into())
        );
        let other = foreign.create_index_owner().unwrap();
        assert_eq!(
            m.allocate_index(other, 0x10, 1, &mut b),
            Err(BufferError::ForeignManager.into())
        );
        assert!(b.events.is_empty());
        m.allocate_index(owner, 0x10, 65536, &mut b).unwrap();
        let key = index_key(&m, owner);
        let spare = m.create_index_owner().unwrap();
        let before = b.events.len();
        assert_eq!(m.allocate_index_at(key, spare, 1, &mut b), Ok(None));
        assert_eq!(m.free_index_at(key, 999, &mut b), Ok(false));
        assert_eq!(b.events.len(), before);
        assert_eq!(m.index_owner(spare).unwrap().lease, OwnerLease::Vacant);
        m.free_all_indices(&mut b).unwrap();
    }
}
