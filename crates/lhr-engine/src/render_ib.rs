#![forbid(unsafe_code)]
//! Index-buffer manager construction and VRAM reclamation.
//!
//! LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! RVAs 0x5b390 (`C_IBManager::C_IBManager`), 0x5b5c0
//! (`C_IBManager::~C_IBManager`), 0x5baa0
//! (`C_IBManager::m_FreeAllVRAMIBuffers`). Pinned mafia-re ab67b13
//! `ls3df/I3D/render/IBManager.cpp:221-239,341-355` and
//! `ls3df/I3D/render/IBManager.h:42-57`.
//!
//! Contract tier this wave: reviewed upstream source plus Rust tests, no
//! Ghidra body claimed. This extends the ledger-ported block semantics in
//! `crate::buffer_managers` (RVAs 0x5abf0/0x5ae00/0x5aff0/0x5b1b0/0x5b6c0/
//! 0x5b940/0x5b960/0x5b9d0/0x5ba30, never re-claimed here): only manager
//! construction/destruction order and the DEFAULT-pool restart scan are
//! ported. Device destruction stays behind a backend trait.

use crate::buffer_managers::Pool;

/// Minimum buffer indices (0x10000, the 0x20000 bytes of
/// `RB_MANAGER_MEM_SIZE`; IBManager.cpp:92-93).
pub const IB_MANAGER_MIN_INDICES: u32 = 0x10000;

/// Typed index-buffer manager: one pool word per live buffer in manager
/// order. Block partitioning itself stays with `buffer_managers.rs`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IbManager {
    /// Live buffer pools in manager order.
    pub pools: Vec<Pool>,
    /// `locked_buffers` reservation (256).
    pub locked_reserved: usize,
    /// `locked_buffers` grow size (256).
    pub locked_grow: usize,
}

/// Device-adjacent buffer destruction behind a backend trait.
pub trait IbManagerBackend {
    /// `delete buffer` at manager position `index` with this pool.
    fn destroy_ibuffer(&mut self, index: usize, pool: Pool);
}

/// LS3DF RVA 0x5b390 `C_IBManager::C_IBManager` (mafia-re
/// `ls3df/I3D/render/IBManager.cpp:221-229`): empty membership with the
/// 256-entry locked reservation. Pure CPU words; no device call.
pub fn new_ib_manager() -> IbManager {
    IbManager {
        pools: Vec::new(),
        locked_reserved: 256,
        locked_grow: 256,
    }
}

/// LS3DF RVA 0x5b5c0 `C_IBManager::~C_IBManager` (mafia-re
/// `ls3df/I3D/render/IBManager.cpp:231-239`): deletes every buffer in
/// manager order, then clears membership. No unreleased warning (unlike
/// `m_FreeAllIBuffers`); destruction effects stay behind the backend.
/// Order is host insertion order (same limitation as the FreeAll scan).
pub fn drop_ib_manager(manager: &mut IbManager, backend: &mut impl IbManagerBackend) {
    for (index, pool) in manager.pools.iter().enumerate() {
        backend.destroy_ibuffer(index, *pool);
    }
    manager.pools.clear();
}

/// LS3DF RVA 0x5baa0 `C_IBManager::m_FreeAllVRAMIBuffers` (mafia-re
/// `ls3df/I3D/render/IBManager.cpp:341-355`): deletes buffers in the
/// default pool, which a reset device loses. Restart-from-begin after each
/// delete (`goto restart`). Managed buffers are retained untouched.
/// Scan order is host insertion order, not the native sorted-multiset
/// order (disclosed); tests pin set membership, not sorted order.
pub fn free_all_vram_ibuffers(manager: &mut IbManager, backend: &mut impl IbManagerBackend) {
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
        backend.destroy_ibuffer(index, pool);
        manager.pools.remove(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Log {
        destroyed: Vec<(usize, Pool)>,
    }
    impl IbManagerBackend for Log {
        fn destroy_ibuffer(&mut self, index: usize, pool: Pool) {
            self.destroyed.push((index, pool));
        }
    }

    #[test]
    fn new_manager_is_empty_with_locked_reservation() {
        let manager = new_ib_manager();
        assert!(manager.pools.is_empty());
        assert_eq!(manager.locked_reserved, 256);
        assert_eq!(manager.locked_grow, 256);
        assert_eq!(IB_MANAGER_MIN_INDICES, 0x10000);
    }

    #[test]
    fn drop_deletes_every_buffer_in_order_then_clears() {
        let mut manager = IbManager {
            pools: vec![Pool::Default, Pool::Managed, Pool::Default],
            locked_reserved: 256,
            locked_grow: 256,
        };
        let mut backend = Log {
            destroyed: Vec::new(),
        };
        drop_ib_manager(&mut manager, &mut backend);
        assert_eq!(
            backend.destroyed,
            vec![(0, Pool::Default), (1, Pool::Managed), (2, Pool::Default),]
        );
        assert!(manager.pools.is_empty());
    }

    #[test]
    fn vram_walk_frees_only_default_with_restart() {
        let mut manager = IbManager {
            pools: vec![Pool::Default, Pool::Managed, Pool::Default, Pool::Managed],
            locked_reserved: 256,
            locked_grow: 256,
        };
        let mut backend = Log {
            destroyed: Vec::new(),
        };
        free_all_vram_ibuffers(&mut manager, &mut backend);
        assert_eq!(manager.pools, vec![Pool::Managed, Pool::Managed]);
        assert_eq!(backend.destroyed.len(), 2);
        for (_, pool) in &backend.destroyed {
            assert_eq!(*pool, Pool::Default);
        }
        backend.destroyed.clear();
        free_all_vram_ibuffers(&mut manager, &mut backend);
        assert!(backend.destroyed.is_empty());
    }
}
