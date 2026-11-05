#![forbid(unsafe_code)]
//! Vertex-buffer manager construction and VRAM reclamation.
//!
//! LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! RVAs 0x688b0 (`C_VBuffer::C_VBuffer`), 0x692b0 (`C_VBManager::C_VBManager`),
//! 0x694e0 (`C_VBManager::~C_VBManager`), 0x69950
//! (`C_VBManager::m_FreeAllVRAMVBuffers`), 0xaf80 (`C_VBuffer::Lock`),
//! 0xb060 (`C_VBuffer::Unlock`). Pinned mafia-re ab67b13
//! `ls3df/I3D/render/VBManager.cpp:21-40,225-243,352-366` and
//! `ls3df/I3D/render/VBManager.h:42-58,114-152`.
//!
//! Contract tier this wave: reviewed upstream source plus Rust tests, no
//! Ghidra body claimed. This extends the ledger-ported block semantics in
//! `crate::buffer_managers` (RVAs 0x68af0/0x68cf0/0x68f10/0x690d0/0x695e0/
//! 0x697c0/0x697e0/0x69820/0x698e0, never re-claimed here): only the initial
//! words, manager construction/destruction order and the DEFAULT-pool
//! restart scan are ported. Device destruction stays behind a backend trait.

use crate::buffer_managers::Pool;

/// Minimum buffer bytes (`RB_MANAGER_MEM_SIZE`, VBManager.h:17).
pub const RB_MANAGER_MEM_SIZE: u32 = 0x20000;
/// Initial vertex format (`VF_LVERTEX`, ls3df.h:379).
pub const VF_LVERTEX: u32 = 0;

/// Typed projection of the `C_VBuffer` constructor's initial words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VbufferInitialState {
    /// `data == null`.
    pub has_data: bool,
    /// `flags` (lock flags).
    pub flags: u32,
    /// `fvf`.
    pub fvf: u32,
    /// `usage`.
    pub usage: u32,
    /// `vb == null`.
    pub has_graphics: bool,
    /// `num_vertices`.
    pub num_vertices: u32,
    /// `num_free` (`= num_vertices`, hence 0 here).
    pub num_free: u32,
    /// `pool` (`D3DPOOL_MANAGED`).
    pub pool: Pool,
    /// `vertex_format` (`VF_LVERTEX`).
    pub vertex_format: u32,
    /// `lock_index` (-1 when unlocked).
    pub lock_index: i32,
    /// `used_blocks` reservation (256).
    pub used_reserved: usize,
}

/// LS3DF RVA 0x688b0 `C_VBuffer::C_VBuffer` (mafia-re
/// `ls3df/I3D/render/VBManager.cpp:21-40`): null data/graphics, zero
/// fvf/usage/counts, managed pool, `VF_LVERTEX`, unlocked, 256-entry used
/// reservation. Pure CPU words; no device call.
pub fn vbuffer_initial_state() -> VbufferInitialState {
    VbufferInitialState {
        has_data: false,
        flags: 0,
        fvf: 0,
        usage: 0,
        has_graphics: false,
        num_vertices: 0,
        num_free: 0,
        pool: Pool::Managed,
        vertex_format: VF_LVERTEX,
        lock_index: -1,
        used_reserved: 256,
    }
}

/// Typed vertex-buffer manager: one pool word per live buffer in manager
/// order. Block partitioning itself stays with `buffer_managers.rs`; this
/// tracks only construction/destruction membership for the VRAM walk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VbManager {
    /// Live buffer pools in manager order.
    pub pools: Vec<Pool>,
    /// `locked_buffers` reservation (256).
    pub locked_reserved: usize,
    /// `locked_buffers` grow size (256).
    pub locked_grow: usize,
}

/// Device-adjacent buffer destruction behind a backend trait.
pub trait VbManagerBackend {
    /// `delete buffer` at manager position `index` with this pool.
    fn destroy_vbuffer(&mut self, index: usize, pool: Pool);
}

/// LS3DF RVA 0x692b0 `C_VBManager::C_VBManager` (mafia-re
/// `ls3df/I3D/render/VBManager.cpp:225-233`): empty membership with the
/// 256-entry locked reservation. Pure CPU words; no device call.
pub fn new_vb_manager() -> VbManager {
    VbManager {
        pools: Vec::new(),
        locked_reserved: 256,
        locked_grow: 256,
    }
}

/// LS3DF RVA 0x694e0 `C_VBManager::~C_VBManager` (mafia-re
/// `ls3df/I3D/render/VBManager.cpp:235-243`): deletes every buffer in
/// manager order, then clears membership. Order is host insertion order (same limitation
/// as the FreeAll scan above). No unreleased warning (unlike
/// `m_FreeAllVBuffers`); destruction effects stay behind the backend.
pub fn drop_vb_manager(manager: &mut VbManager, backend: &mut impl VbManagerBackend) {
    for (index, pool) in manager.pools.iter().enumerate() {
        backend.destroy_vbuffer(index, *pool);
    }
    manager.pools.clear();
}

/// LS3DF RVA 0x69950 `C_VBManager::m_FreeAllVRAMVBuffers` (mafia-re
/// `ls3df/I3D/render/VBManager.cpp:352-366`): deletes buffers in the
/// default pool, which a reset device loses. Restart-from-begin after each
/// delete (`goto restart`), so erasure never skips a successor. Managed
/// buffers are retained untouched.
/// Scan order is host insertion order, not the native sorted-multiset
/// order (disclosed); tests pin set membership, not sorted order.
pub fn free_all_vram_vbuffers(manager: &mut VbManager, backend: &mut impl VbManagerBackend) {
    loop {
        let mut found: Option<usize> = None;
        for (index, pool) in manager.pools.iter().enumerate() {
            if *pool == Pool::Default {
                found = Some(index);
                break;
            }
        }
        let Some(index) = found else {
            break;
        };
        let pool = manager.pools[index];
        backend.destroy_vbuffer(index, pool);
        manager.pools.remove(index);
    }
}

/// `D3DLOCK_READONLY` bit tested by `C_VBuffer::Lock`.
pub const D3DLOCK_READONLY: u32 = 0x10;

/// Typed projection of the lock-relevant `C_VBuffer` words. `locked` holds
/// buffer ids in `locked_buffers` order; each id indexes `states`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VbufferState {
    /// `data != null`.
    pub has_data: bool,
    /// `flags` (the `D3DLOCK_*` flags it is locked with).
    pub flags: u32,
    /// `stride` (the size of a vertex).
    pub stride: u32,
    /// `lock_index` (its place in the locked list, -1 if unlocked).
    pub lock_index: i32,
}

impl VbufferState {
    /// Unlocked buffer with this stride.
    pub fn unlocked(stride: u32) -> Self {
        Self {
            has_data: false,
            flags: 0,
            stride,
            lock_index: -1,
        }
    }
}

/// Device lock/unlock behind a backend trait.
pub trait VbufferLockBackend {
    /// `vb->Lock(0, 0, &data, flags)`.
    fn lock_vbuffer(&mut self, flags: u32);
    /// `vb->Unlock()`.
    fn unlock_vbuffer(&mut self);
}

/// LS3DF RVA 0xaf80 `C_VBuffer::Lock` (mafia-re
/// `ls3df/I3D/render/VBManager.h:114`; `ENGINE_VERSION >= 395` branch —
/// the shipped binary's version gate is unevidenced here, so the >=395
/// shape is a disclosed assumption; method name is PS2-evidenced, the `Lock` working-name note in the
/// header marks the guessed free-function spelling, NOT the method).
/// Returns the byte offset `stride * vertex_index` for `data + stride *
/// index`. A locked buffer whose stored flags are writable, or any locked
/// buffer re-locked read-only, returns without a device call. A read-only
/// buffer re-locked for writing device-unlocks then re-locks. Otherwise the
/// flags store, the device locks, and an unlocked buffer appends to the
/// locked list. Device lock failure is outside the admitted domain (the
/// original ignores the `HRESULT`); the port assumes success and marks
/// `has_data`. No floating-point work.
pub fn vbuffer_lock(
    states: &mut [VbufferState],
    locked: &mut Vec<usize>,
    buffer: usize,
    vertex_index: u32,
    lock_flags: u32,
    backend: &mut impl VbufferLockBackend,
) -> u32 {
    if states[buffer].has_data {
        if states[buffer].flags & D3DLOCK_READONLY == 0 || lock_flags & D3DLOCK_READONLY != 0 {
            return states[buffer].stride.wrapping_mul(vertex_index);
        }
        backend.unlock_vbuffer();
    }
    states[buffer].flags = lock_flags;
    backend.lock_vbuffer(lock_flags);
    states[buffer].has_data = true;
    if states[buffer].lock_index < 0 {
        states[buffer].lock_index = locked.len() as i32;
        locked.push(buffer);
    }
    states[buffer].stride.wrapping_mul(vertex_index)
}

/// LS3DF RVA 0xb060 `C_VBuffer::Unlock` (mafia-re
/// `ls3df/I3D/render/VBManager.h:142`; method name is PS2-evidenced, the
/// free-function spelling is a working name). No-op when unlocked.
/// Otherwise the device unlocks, `has_data` clears, the locked list erases
/// at `lock_index` with the last entry moved into its place (the native
/// `C_list::erase` swap), the moved buffer inherits the erased index, and
/// the erased `lock_index` resets to -1. No floating-point work.
pub fn vbuffer_unlock(
    states: &mut [VbufferState],
    locked: &mut Vec<usize>,
    buffer: usize,
    backend: &mut impl VbufferLockBackend,
) {
    if !states[buffer].has_data {
        return;
    }
    backend.unlock_vbuffer();
    states[buffer].has_data = false;
    // Native moves the last locked buffer into the erased slot. The host
    // locates the slot by value, so a stale index still restores the list
    // invariant instead of panicking (native UB) or stranding a duplicate.
    if let Some(pos) = locked.iter().position(|&b| b == buffer) {
        let last = locked[locked.len() - 1];
        states[last].lock_index = pos as i32;
        locked.swap_remove(pos);
    }
    states[buffer].lock_index = -1;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Log {
        destroyed: Vec<(usize, Pool)>,
    }
    impl VbManagerBackend for Log {
        fn destroy_vbuffer(&mut self, index: usize, pool: Pool) {
            self.destroyed.push((index, pool));
        }
    }

    #[test]
    fn initial_state_pins_constructor_words() {
        let initial = vbuffer_initial_state();
        assert!(!initial.has_data);
        assert!(!initial.has_graphics);
        assert_eq!(initial.flags, 0);
        assert_eq!(initial.fvf, 0);
        assert_eq!(initial.usage, 0);
        assert_eq!(initial.num_vertices, 0);
        assert_eq!(initial.num_free, 0);
        assert_eq!(initial.pool, Pool::Managed);
        assert_eq!(initial.vertex_format, VF_LVERTEX);
        assert_eq!(initial.lock_index, -1);
        assert_eq!(initial.used_reserved, 256);
        assert_eq!(RB_MANAGER_MEM_SIZE, 0x20000);
    }

    #[test]
    fn new_manager_is_empty_with_locked_reservation() {
        let manager = new_vb_manager();
        assert!(manager.pools.is_empty());
        assert_eq!(manager.locked_reserved, 256);
        assert_eq!(manager.locked_grow, 256);
    }

    #[test]
    fn drop_deletes_every_buffer_in_order_then_clears() {
        let mut manager = VbManager {
            pools: vec![Pool::Managed, Pool::Default, Pool::Managed],
            locked_reserved: 256,
            locked_grow: 256,
        };
        let mut backend = Log {
            destroyed: Vec::new(),
        };
        drop_vb_manager(&mut manager, &mut backend);
        assert_eq!(
            backend.destroyed,
            vec![(0, Pool::Managed), (1, Pool::Default), (2, Pool::Managed),]
        );
        assert!(manager.pools.is_empty());
    }

    #[test]
    fn vram_walk_frees_only_default_with_restart() {
        let mut manager = VbManager {
            pools: vec![
                Pool::Managed,
                Pool::Default,
                Pool::Managed,
                Pool::Default,
                Pool::Default,
            ],
            locked_reserved: 256,
            locked_grow: 256,
        };
        let mut backend = Log {
            destroyed: Vec::new(),
        };
        free_all_vram_vbuffers(&mut manager, &mut backend);
        // Restart scan destroys original indices 1, 3, 4 in that order;
        // reported indices are pre-removal positions at each restart.
        assert_eq!(manager.pools, vec![Pool::Managed, Pool::Managed]);
        assert_eq!(backend.destroyed.len(), 3);
        for (_, pool) in &backend.destroyed {
            assert_eq!(*pool, Pool::Default);
        }
        // Steady state is quiet.
        backend.destroyed.clear();
        free_all_vram_vbuffers(&mut manager, &mut backend);
        assert!(backend.destroyed.is_empty());
    }

    struct LockLog {
        events: Vec<String>,
    }
    impl VbufferLockBackend for LockLog {
        fn lock_vbuffer(&mut self, flags: u32) {
            self.events.push(format!("lock:{flags}"));
        }
        fn unlock_vbuffer(&mut self) {
            self.events.push("unlock".to_string());
        }
    }

    #[test]
    fn lock_caches_writable_and_readonly_paths() {
        let mut states = vec![VbufferState::unlocked(12), VbufferState::unlocked(20)];
        let mut locked = Vec::new();
        let mut backend = LockLog { events: Vec::new() };
        assert_eq!(
            vbuffer_lock(&mut states, &mut locked, 0, 3, 0, &mut backend),
            36
        );
        assert!(states[0].has_data);
        assert_eq!(states[0].lock_index, 0);
        assert_eq!(locked, vec![0]);
        assert_eq!(backend.events, vec!["lock:0"]);
        backend.events.clear();
        assert_eq!(
            vbuffer_lock(&mut states, &mut locked, 0, 4, 0, &mut backend),
            48
        );
        assert!(backend.events.is_empty(), "writable relock is cached");
        assert_eq!(locked, vec![0]);
        backend.events.clear();
        assert_eq!(
            vbuffer_lock(
                &mut states,
                &mut locked,
                0,
                1,
                D3DLOCK_READONLY,
                &mut backend
            ),
            12
        );
        assert!(backend.events.is_empty(), "read-only relock is cached");
        assert_eq!(
            vbuffer_lock(
                &mut states,
                &mut locked,
                1,
                2,
                D3DLOCK_READONLY,
                &mut backend
            ),
            40
        );
        assert_eq!(locked, vec![0, 1]);
        backend.events.clear();
        assert_eq!(
            vbuffer_lock(&mut states, &mut locked, 1, 0, 0, &mut backend),
            0
        );
        assert_eq!(backend.events, vec!["unlock", "lock:0"]);
        assert_eq!(locked, vec![0, 1], "re-lock keeps the list place");
    }

    #[test]
    fn unlock_erases_with_last_moved_into_place() {
        let mut states = vec![VbufferState::unlocked(8), VbufferState::unlocked(8)];
        let mut locked = Vec::new();
        let mut backend = LockLog { events: Vec::new() };
        vbuffer_lock(&mut states, &mut locked, 0, 0, 0, &mut backend);
        vbuffer_lock(&mut states, &mut locked, 1, 0, 0, &mut backend);
        assert_eq!(locked, vec![0, 1]);
        backend.events.clear();
        vbuffer_unlock(&mut states, &mut locked, 0, &mut backend);
        assert_eq!(backend.events, vec!["unlock"]);
        assert!(!states[0].has_data);
        assert_eq!(states[0].lock_index, -1);
        assert_eq!(locked, vec![1]);
        assert_eq!(states[1].lock_index, 0, "moved last inherits the index");
        backend.events.clear();
        vbuffer_unlock(&mut states, &mut locked, 0, &mut backend);
        assert!(backend.events.is_empty(), "unlocked unlock is quiet");
        vbuffer_unlock(&mut states, &mut locked, 1, &mut backend);
        assert!(locked.is_empty());
        assert_eq!(states[1].lock_index, -1);
    }

    #[test]
    fn unlock_with_stale_index_repairs_list_by_value() {
        // Hardened vs native UB: a corrupt lock word still restores the
        // list invariant (entry removed, survivor indexes fixed).
        let mut states = vec![VbufferState::unlocked(8)];
        let mut locked = Vec::new();
        let mut backend = LockLog { events: Vec::new() };
        vbuffer_lock(&mut states, &mut locked, 0, 0, 0, &mut backend);
        states[0].lock_index = 7;
        backend.events.clear();
        vbuffer_unlock(&mut states, &mut locked, 0, &mut backend);
        assert_eq!(backend.events, vec!["unlock"]);
        assert_eq!(states[0].lock_index, -1);
        assert!(locked.is_empty());
        // Relock after repair pushes exactly once (no stranded duplicate).
        vbuffer_lock(&mut states, &mut locked, 0, 0, 0, &mut backend);
        assert_eq!(locked, vec![0]);
    }
}
