#![forbid(unsafe_code)]
//! Vertex-buffer manager construction and VRAM reclamation.
//!
//! LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! RVAs 0x688b0 (`C_VBuffer::C_VBuffer`), 0x692b0 (`C_VBManager::C_VBManager`),
//! 0x694e0 (`C_VBManager::~C_VBManager`), 0x69950
//! (`C_VBManager::m_FreeAllVRAMVBuffers`). Pinned mafia-re ab67b13
//! `ls3df/I3D/render/VBManager.cpp:21-40,225-243,352-366` and
//! `ls3df/I3D/render/VBManager.h:42-58`.
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
}
