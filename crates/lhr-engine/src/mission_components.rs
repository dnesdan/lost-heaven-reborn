//! Seeded PC mission support objects. Game SHA-256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! C_tshift ctor RVA0x1604f0/dtor0x12000; C_roads ctor0x159cd0/
//! dtor0x159d00; C_web_path inline constructor in0x13f230/dtor0x14a1e0.
//! Web node dtor0x149f90 and empty connection dtor0x4990.
//! Effects is an upstream descriptive name; its ctor0x49080 calls Load0x49380.
//! Constructors preserve padding and allocation seeds. Cleanup below admits
//! constructor-empty path/road arrays only. Loaded effects have an owned Rust
//! tree; its host allocations never masquerade as native x86 pointer words.

#[path = "mission_game.rs"]
mod game;
pub use game::{GameConstructorGlobals, GameConstructorInputs};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionComponentKind {
    Tshift,
    WebPath,
    Roads,
    Effects,
    Game,
}
impl MissionComponentKind {
    pub fn allocation_size(self) -> usize {
        match self {
            Self::Tshift => 0x10,
            Self::WebPath => 0x1d,
            Self::Roads => 0x24,
            Self::Effects => 0x38,
            Self::Game => 0x36ac,
        }
    }
}
/// A real allocation supplied by the caller's allocator, with its original
/// identity and untouched storage. Identity zero belongs to the null branch.
pub struct MissionAllocation {
    pub domain: MissionMemoryDomainId,
    pub identity: u32,
    pub bytes: Vec<u8>,
}
/// Unforgeable host allocator domain; clones identify the SAME allocator owner,
/// not permission to reuse identities in another allocation table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MissionMemoryDomainId(u64);
impl MissionMemoryDomainId {
    pub fn fresh() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self(
            NEXT.try_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| n.checked_add(1),
            )
            .expect("mission memory domain exhausted"),
        )
    }
}
pub trait MissionMemoryDomain {
    fn mission_memory_domain(&self) -> MissionMemoryDomainId;
}
pub trait MissionAllocator: MissionMemoryDomain {
    type Error;
    fn allocate(
        &mut self,
        kind: MissionComponentKind,
    ) -> Result<Option<MissionAllocation>, Self::Error>;
    /// Original scalar free primitive, including its null-pointer calls.
    fn free(&mut self, identity: u32) -> Result<(), Self::Error>;
    /// Actual dtaOpen("tables\\effects.tbl",0): false is the genuine -1
    /// result. On true the backend retains the opened stream; successful
    /// C_chunk seeks, stream state and parsing remain separate dependencies.
    fn open_effects_table(&mut self) -> Result<bool, Self::Error>;
    /// Consume the retained genuine stream and close it. None preserves the
    /// explicit dependency boundary for allocators without stream access.
    fn read_effects_table(&mut self) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(None)
    }
}
#[derive(Debug)]
pub enum ComponentError<E> {
    Allocator(E),
    InvalidAllocation,
    Unavailable {
        kind: MissionComponentKind,
        offset: usize,
    },
    Interrupted,
    EffectsTableStreamUnavailable,
    EffectsTable(crate::effects_table::DataChunkError),
    ConstructorIncomplete,
    ConstructorAlreadyComplete,
    ForeignDomain,
}
pub struct MissionComponent {
    kind: MissionComponentKind,
    allocation: MissionAllocation,
    interrupted: bool,
    constructed: bool,
    destroyed: bool,
    effects: Option<crate::effects_table::EffectsTable>,
}
impl MissionComponent {
    pub fn construct(
        kind: MissionComponentKind,
        allocation: MissionAllocation,
    ) -> Result<Self, ComponentError<std::convert::Infallible>> {
        if allocation.identity == 0 || allocation.bytes.len() != kind.allocation_size() {
            return Err(ComponentError::InvalidAllocation);
        }
        let mut out = Self {
            kind,
            allocation,
            interrupted: false,
            constructed: true,
            destroyed: false,
            effects: None,
        };
        match kind {
            MissionComponentKind::Tshift => out.vector(0, (out.identity() >> 24) as u8),
            MissionComponentKind::WebPath => {
                out.put(0x10, 50);
                out.put(0x14, 0);
                out.put(0x18, 0);
                out.allocation.bytes[0x1c] = 0;
                out.put(0, 0x6251f8);
                for offset in [4, 8, 12] {
                    out.put(offset, 0);
                }
            }
            MissionComponentKind::Roads => {
                out.vector(4, (out.identity() >> 24) as u8);
                out.vector(0x14, (out.identity() >> 24) as u8);
                out.put(0, 0x625370);
            }
            // Both have mandatory calls and therefore no literal-success arm.
            kind => return Err(ComponentError::Unavailable { kind, offset: 0 }),
        }
        Ok(out)
    }
    pub fn construct_effects_prefix(
        allocation: MissionAllocation,
        stack_control: u8,
    ) -> Result<Self, ComponentError<std::convert::Infallible>> {
        if allocation.identity == 0
            || allocation.bytes.len() != MissionComponentKind::Effects.allocation_size()
        {
            return Err(ComponentError::InvalidAllocation);
        }
        let mut out = Self {
            kind: MissionComponentKind::Effects,
            allocation,
            interrupted: false,
            constructed: false,
            destroyed: false,
            effects: None,
        };
        for offset in [0, 0x10, 0x20] {
            out.vector(offset, stack_control);
        }
        out.put(0x18, 0); // Load's Clear executes before dtaOpen.
        Ok(out)
    }
    /// Finish ctor449080 through the actual opened table or missing-file
    /// branch. The Rust tree owns loaded children without inventing x86 pointers.
    pub fn finish_effects_constructor<A: MissionAllocator>(
        &mut self,
        allocator: &mut A,
    ) -> Result<(), ComponentError<A::Error>> {
        if self.kind != MissionComponentKind::Effects {
            return Err(ComponentError::Unavailable {
                kind: self.kind,
                offset: 0,
            });
        }
        if self.allocation.domain != allocator.mission_memory_domain() {
            return Err(ComponentError::ForeignDomain);
        }
        if self.interrupted {
            return Err(ComponentError::Interrupted);
        }
        if self.constructed {
            return Err(ComponentError::ConstructorAlreadyComplete);
        }
        let result = (|| {
            match allocator
                .open_effects_table()
                .map_err(ComponentError::Allocator)?
            {
                true => {
                    let bytes = allocator
                        .read_effects_table()
                        .map_err(ComponentError::Allocator)?
                        .ok_or(ComponentError::EffectsTableStreamUnavailable)?;
                    let table = self.effects.get_or_insert_with(Default::default);
                    table.load(&bytes).map_err(ComponentError::EffectsTable)?;
                    // Chunk stack storage is owned by the bounded Rust reader;
                    // its host allocation is released with that reader.
                }
                false => allocator.free(0).map_err(ComponentError::Allocator)?,
            }
            self.put(0x30, u32::MAX);
            Ok(())
        })();
        match result {
            Ok(()) => self.constructed = true,
            Err(_) => self.interrupted = true,
        }
        result
    }
    pub fn construct_game(
        allocation: MissionAllocation,
        inputs: GameConstructorInputs,
        globals: &mut GameConstructorGlobals,
    ) -> Result<Self, ComponentError<std::convert::Infallible>> {
        if allocation.identity == 0
            || allocation.bytes.len() != MissionComponentKind::Game.allocation_size()
        {
            return Err(ComponentError::InvalidAllocation);
        }
        let mut out = Self {
            kind: MissionComponentKind::Game,
            allocation,
            interrupted: false,
            constructed: true,
            destroyed: false,
            effects: None,
        };
        let identity = out.identity();
        game::construct(&mut out.allocation.bytes, identity, inputs, globals);
        Ok(out)
    }
    pub fn effects(&self) -> Option<&crate::effects_table::EffectsTable> {
        self.effects.as_ref()
    }
    pub fn kind(&self) -> MissionComponentKind {
        self.kind
    }
    pub fn identity(&self) -> u32 {
        self.allocation.identity
    }
    pub fn raw_bytes(&self) -> &[u8] {
        &self.allocation.bytes
    }
    pub(crate) fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(
            self.allocation.bytes[offset..offset + 4]
                .try_into()
                .unwrap(),
        )
    }
    pub(crate) fn put(&mut self, offset: usize, value: u32) {
        self.allocation.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn vector(&mut self, offset: usize, control: u8) {
        self.allocation.bytes[offset] = control;
        for delta in [4, 8, 12] {
            self.put(offset + delta, 0);
        }
    }
    fn web_path_leaf(&self) -> Result<(), ComponentError<std::convert::Infallible>> {
        if self.interrupted || self.destroyed {
            return Err(ComponentError::Interrupted);
        }
        if self.kind != MissionComponentKind::WebPath {
            return Err(ComponentError::Unavailable {
                kind: self.kind,
                offset: 0,
            });
        }
        Ok(())
    }
    /// Folded original DrawConnections4aabe0 is a literal returning leaf.
    pub fn draw_connections(&self) -> Result<(), ComponentError<std::convert::Infallible>> {
        self.web_path_leaf()
    }
    /// Folded original SelectLine5e3fd0 returns integer zero (RET8).
    pub fn select_line(
        &self,
        _first: i32,
        _second: i32,
    ) -> Result<i32, ComponentError<std::convert::Infallible>> {
        self.web_path_leaf()?;
        Ok(0)
    }
    /// Folded original DebugFindPath46ca70 is a literal returning leaf.
    pub fn debug_find_path(
        &self,
        _first: u16,
        _second: u16,
    ) -> Result<(), ComponentError<std::convert::Infallible>> {
        self.web_path_leaf()
    }
    /// Effects Clear44a350: release owned children and retain vector capacity.
    /// Foreign populated x86 vectors still require their actual ownership.
    pub fn clear_effects(&mut self) -> Result<(), ComponentError<std::convert::Infallible>> {
        if self.interrupted || self.destroyed {
            return Err(ComponentError::Interrupted);
        }
        if !self.constructed {
            return Err(ComponentError::ConstructorIncomplete);
        }
        if self.kind != MissionComponentKind::Effects {
            return Err(ComponentError::Unavailable {
                kind: self.kind,
                offset: 0,
            });
        }
        if let Some(table) = &mut self.effects {
            table.clear();
        }
        let begin = self.word(0x14);
        if begin != 0 && self.word(0x18) != begin {
            return Err(ComponentError::Unavailable {
                kind: self.kind,
                offset: 0x14,
            });
        }
        self.put(0x18, begin);
        Ok(())
    }
    /// Real direct destructor body; scalar object free remains the caller's next
    /// operation. Nonempty unsupported ownership stops before touching it.
    pub fn destroy<A: MissionAllocator>(
        &mut self,
        allocator: &mut A,
    ) -> Result<(), ComponentError<A::Error>> {
        if self.allocation.domain != allocator.mission_memory_domain() {
            return Err(ComponentError::ForeignDomain);
        }
        if !self.constructed {
            return Err(ComponentError::ConstructorIncomplete);
        }
        if self.destroyed {
            return Err(ComponentError::Interrupted);
        }
        if self.interrupted {
            return Err(ComponentError::Interrupted);
        }
        if self.kind == MissionComponentKind::Game {
            let result = game::destroy_empty(&mut self.allocation.bytes, |offset, value| {
                if value != 0 {
                    return Err(ComponentError::Unavailable {
                        kind: MissionComponentKind::Game,
                        offset,
                    });
                }
                allocator.free(0).map_err(ComponentError::Allocator)
            });
            if result.is_err() {
                self.interrupted = true;
            }
            if result.is_ok() {
                self.destroyed = true;
            }
            return result;
        }
        let fields: &[usize] = match self.kind {
            MissionComponentKind::Tshift => &[4],
            MissionComponentKind::WebPath => {
                self.put(0, 0x6251f8);
                if self.word(4) != 0 {
                    return Err(ComponentError::Unavailable {
                        kind: self.kind,
                        offset: 4,
                    });
                }
                &[0x14]
            }
            MissionComponentKind::Roads => {
                self.put(0, 0x625370);
                &[0x18, 8]
            }
            MissionComponentKind::Effects => {
                if let Some(mut table) = self.effects.take() {
                    table.clear();
                }
                if self.word(0x14) != 0 || self.word(0x18) != 0 {
                    return Err(ComponentError::Unavailable {
                        kind: self.kind,
                        offset: 0x14,
                    });
                }
                self.put(0x18, self.word(0x14));
                &[0x24, 0x14, 4]
            }
            kind => return Err(ComponentError::Unavailable { kind, offset: 0 }),
        };
        for &offset in fields {
            if self.word(offset) != 0 {
                return Err(ComponentError::Unavailable {
                    kind: self.kind,
                    offset,
                });
            }
            if let Err(error) = allocator.free(0) {
                self.interrupted = true;
                return Err(ComponentError::Allocator(error));
            }
            if self.kind != MissionComponentKind::WebPath {
                for delta in [0, 4, 8] {
                    self.put(offset + delta, 0);
                }
            }
        }
        self.destroyed = true;
        Ok(())
    }
}

/// Existing packed C_web_node storage (0x1e bytes). No node constructor is
/// reconstructed by this adoption boundary. The connections are an actual
/// separately allocated C++ array with a four-byte count cookie.
pub struct WebNode {
    bytes: [u8; 0x1e],
    connections: Option<MissionAllocation>,
    destroyed: bool,
    interrupted: bool,
}
impl WebNode {
    pub fn adopt(
        bytes: [u8; 0x1e],
        connections: Option<MissionAllocation>,
    ) -> Result<Self, ComponentError<std::convert::Infallible>> {
        let pointer = u32::from_le_bytes(bytes[0x18..0x1c].try_into().unwrap());
        match &connections {
            None if pointer == 0 => {}
            Some(allocation)
                if allocation.identity != 0
                    && allocation.identity.checked_add(4) == Some(pointer)
                    && allocation.bytes.len() >= 4 =>
            {
                let count = u32::from_le_bytes(allocation.bytes[..4].try_into().unwrap());
                if count
                    .checked_mul(8)
                    .and_then(|n| n.checked_add(4))
                    .map(|n| n as usize)
                    != Some(allocation.bytes.len())
                {
                    return Err(ComponentError::InvalidAllocation);
                }
            }
            _ => return Err(ComponentError::InvalidAllocation),
        }
        Ok(Self {
            bytes,
            connections,
            destroyed: false,
            interrupted: false,
        })
    }
    pub fn raw_bytes(&self) -> &[u8; 0x1e] {
        &self.bytes
    }
    pub fn is_destroyed(&self) -> bool {
        self.destroyed
    }
    /// Original C_web_node destructor549f90: a null array does nothing;
    /// otherwise reverse element destruction invokes the literal empty404990
    /// leaf, then frees the allocation at conns-4. Node bytes stay untouched.
    pub fn destroy<A: MissionAllocator>(
        &mut self,
        allocator: &mut A,
    ) -> Result<(), ComponentError<A::Error>> {
        if self.destroyed || self.interrupted {
            return Err(ComponentError::Interrupted);
        }
        if let Some(connections) = &self.connections {
            if connections.domain != allocator.mission_memory_domain() {
                return Err(ComponentError::ForeignDomain);
            }
            if let Err(error) = allocator.free(connections.identity) {
                self.interrupted = true;
                return Err(ComponentError::Allocator(error));
            }
        }
        self.destroyed = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Memory {
        domain: MissionMemoryDomainId,
        frees: Vec<u32>,
    }
    impl MissionMemoryDomain for Memory {
        fn mission_memory_domain(&self) -> MissionMemoryDomainId {
            self.domain
        }
    }
    impl MissionAllocator for Memory {
        type Error = ();
        fn allocate(&mut self, _: MissionComponentKind) -> Result<Option<MissionAllocation>, ()> {
            Ok(None)
        }
        fn free(&mut self, identity: u32) -> Result<(), ()> {
            self.frees.push(identity);
            Ok(())
        }
        fn open_effects_table(&mut self) -> Result<bool, ()> {
            Ok(false)
        }
    }
    #[test]
    fn web_node_destroys_owned_connection_array_once_without_changing_node_storage() {
        let domain = MissionMemoryDomainId::fresh();
        let mut memory = Memory {
            domain,
            frees: Vec::new(),
        };
        let mut bytes = [0xa5; 0x1e];
        bytes[0x18..0x1c].copy_from_slice(&0x6d00_0004u32.to_le_bytes());
        let mut connections = vec![0x39; 4 + 3 * 8];
        connections[..4].copy_from_slice(&3u32.to_le_bytes());
        let mut node = WebNode::adopt(
            bytes,
            Some(MissionAllocation {
                domain,
                identity: 0x6d00_0000,
                bytes: connections,
            }),
        )
        .unwrap();
        let original_domain = memory.domain;
        memory.domain = MissionMemoryDomainId::fresh();
        assert!(matches!(
            node.destroy(&mut memory),
            Err(ComponentError::ForeignDomain)
        ));
        assert!(memory.frees.is_empty());
        memory.domain = original_domain;
        node.destroy(&mut memory).unwrap();
        assert_eq!(node.raw_bytes(), &bytes);
        assert_eq!(memory.frees, vec![0x6d00_0000]);
        assert!(node.destroy(&mut memory).is_err());
        assert_eq!(memory.frees.len(), 1);
    }
    #[test]
    fn literal_web_path_leaves_and_effects_completion_obey_lifetime_phase() {
        let domain = MissionMemoryDomainId::fresh();
        let mut memory = Memory {
            domain,
            frees: Vec::new(),
        };
        let path = MissionComponent::construct(
            MissionComponentKind::WebPath,
            MissionAllocation {
                domain,
                identity: 0x6d00_0000,
                bytes: vec![0xa5; 0x1d],
            },
        )
        .unwrap();
        path.draw_connections().unwrap();
        assert_eq!(path.select_line(-9, 17).unwrap(), 0);
        path.debug_find_path(1, 65535).unwrap();
        let mut effects = MissionComponent::construct_effects_prefix(
            MissionAllocation {
                domain,
                identity: 0x6d00_1000,
                bytes: vec![0xa5; 0x38],
            },
            0x39,
        )
        .unwrap();
        assert!(matches!(
            effects.destroy(&mut memory),
            Err(ComponentError::ConstructorIncomplete)
        ));
        effects.finish_effects_constructor(&mut memory).unwrap();
        effects.clear_effects().unwrap();
        assert!(matches!(
            effects.finish_effects_constructor(&mut memory),
            Err(ComponentError::ConstructorAlreadyComplete)
        ));
        assert_eq!(memory.frees, vec![0]);
        effects.destroy(&mut memory).unwrap();
        assert_eq!(memory.frees, vec![0, 0, 0, 0]);
    }
}
