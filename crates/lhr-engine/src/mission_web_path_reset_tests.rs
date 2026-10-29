//! Synthetic graph ownership and error-order tests; no original game files.
use super::*;
use crate::mission_components::{
    MissionAllocation, MissionAllocator, MissionComponentKind, MissionMemoryDomain, WebArrayKind,
    WebPathAllocator, WebPathChunk,
};

struct Memory {
    domain: MissionMemoryDomainId,
    next: u32,
    allocations: usize,
    frees: Vec<u32>,
}
impl MissionMemoryDomain for Memory {
    fn mission_memory_domain(&self) -> MissionMemoryDomainId {
        self.domain
    }
}
impl MissionAllocator for Memory {
    type Error = Infallible;
    fn allocate(
        &mut self,
        _: MissionComponentKind,
    ) -> std::result::Result<Option<MissionAllocation>, Infallible> {
        unreachable!("the fixture constructs its component explicitly")
    }
    fn free(&mut self, identity: u32) -> std::result::Result<(), Infallible> {
        self.frees.push(identity);
        Ok(())
    }
    fn open_effects_table(&mut self) -> std::result::Result<bool, Infallible> {
        unreachable!("navigation does not open effects")
    }
}
impl WebPathAllocator for Memory {
    fn allocate_web_array(
        &mut self,
        _: WebArrayKind,
        _: u32,
        byte_length: u32,
    ) -> std::result::Result<Option<MissionAllocation>, Infallible> {
        let identity = self.next;
        self.next += byte_length + 16;
        self.allocations += 1;
        Ok(Some(MissionAllocation {
            domain: self.domain,
            identity,
            bytes: vec![0xa5; byte_length as usize],
        }))
    }
}
struct Chunk {
    bytes: Vec<u8>,
    position: usize,
}
impl WebPathChunk for Chunk {
    type Error = Infallible;
    type Bound = ();
    fn is_open(&self) -> bool {
        true
    }
    fn is_writing(&self) -> bool {
        false
    }
    fn active_bound(&self) -> Option<()> {
        None
    }
    fn seek_current(&mut self) -> std::result::Result<i32, Infallible> {
        Ok(self.position as i32)
    }
    fn read_bound(&self, _: ()) -> std::result::Result<i32, Infallible> {
        unreachable!()
    }
    fn read_data(&mut self, destination: &mut [u8]) -> std::result::Result<usize, Infallible> {
        let length = destination.len().min(self.bytes.len() - self.position);
        destination[..length].copy_from_slice(&self.bytes[self.position..self.position + length]);
        self.position += length;
        Ok(length)
    }
}
fn empty() -> (MissionComponent, Memory) {
    let memory = Memory {
        domain: MissionMemoryDomainId::fresh(),
        next: 0x1000,
        allocations: 0,
        frees: Vec::new(),
    };
    let owner = MissionComponent::construct(
        MissionComponentKind::WebPath,
        MissionAllocation {
            domain: memory.domain,
            identity: 1,
            bytes: vec![0xa5; 0x1d],
        },
    )
    .unwrap();
    (owner, memory)
}
fn fixture() -> (MissionComponent, Memory) {
    let (mut owner, mut memory) = empty();
    let mut bytes = 0x01ab_cedfu32.to_le_bytes().to_vec();
    bytes.extend(2u32.to_le_bytes());
    for count in [2, 1] {
        let mut node = [0xa5; 0x1e];
        node[0xc..0xe].copy_from_slice(&0xffffu16.to_le_bytes());
        node[0x1d] = count;
        bytes.extend(node);
    }
    for (index, flags, distance) in [
        (1u16, 0xffffu16, 0x3f80_0000u32),
        (0, 0x5000, 0x4000_0000),
        (0, 0xd123, 0x4040_0000),
    ] {
        bytes.extend(index.to_le_bytes());
        bytes.extend(flags.to_le_bytes());
        bytes.extend(distance.to_le_bytes());
    }
    let mut chunk = Chunk { bytes, position: 0 };
    owner.load_web_nodes(&mut memory, &mut chunk).unwrap();
    assert_eq!(chunk.position, chunk.bytes.len());
    (owner, memory)
}
fn nodes(owner: &mut MissionComponent) -> &mut NodeArray {
    owner.web_path.as_mut().unwrap().nodes.as_mut().unwrap()
}
fn flag(owner: &MissionComponent, index: usize) -> u16 {
    owner.web_path().unwrap().node(index).unwrap().flags()
}
fn link_flag(owner: &MissionComponent, node: usize, connection: usize) -> u16 {
    owner
        .web_path()
        .unwrap()
        .node(node)
        .unwrap()
        .connection(connection)
        .unwrap()
        .word_2
}
fn image(owner: &MissionComponent) -> Vec<Vec<u8>> {
    let mut result = vec![owner.raw_bytes().to_vec()];
    if let Some(nodes) = owner
        .web_path
        .as_ref()
        .and_then(|graph| graph.nodes.as_ref())
    {
        result.push(nodes.allocation.bytes.clone());
        for connection in &nodes.connections {
            result.push(
                connection
                    .as_ref()
                    .map_or_else(Vec::new, |array| array.allocation.bytes.clone()),
            );
        }
    }
    result
}

#[test]
fn reset_mask_preserves_every_other_bit_for_all_u16_values() {
    for flags in 0..=u16::MAX {
        let mut bytes = flags.to_le_bytes();
        clear_word_flags(&mut bytes, RESET_FLAGS);
        let actual = u16::from_le_bytes(bytes);
        assert_eq!(actual & 0x5000, 0);
        assert_eq!(actual & 0xafff, flags & 0xafff);
    }
}

#[test]
fn loaded_graph_changes_only_active_flags_and_preserves_spare_storage() {
    let (mut owner, mut memory) = fixture();
    owner.resize_web_connections(0, 4, &mut memory).unwrap();
    nodes(&mut owner).node_mut(2).unwrap()[0xc..0xe].fill(0xff);
    let links = nodes(&mut owner).connections[0].as_mut().unwrap();
    links.data_mut()[2 * CONNECTION_SIZE..].fill(0xff);
    let mut expected = image(&owner);
    for node_index in 0..2 {
        let at = 4 + node_index * super::super::NODE_SIZE + 0xc;
        clear_word_flags(&mut expected[1][at..at + 2], RESET_FLAGS);
        for connection in 0..[2, 1][node_index] {
            let at = 4 + connection * CONNECTION_SIZE + 2;
            clear_word_flags(&mut expected[2 + node_index][at..at + 2], RESET_FLAGS);
        }
    }
    let allocation_count = memory.allocations;
    let frees = memory.frees.clone();
    owner.de_rici_conns().unwrap();
    assert_eq!(image(&owner), expected);
    assert_eq!(memory.allocations, allocation_count);
    assert_eq!(memory.frees, frees);
    assert_eq!((flag(&owner, 0), flag(&owner, 1)), (0xafff, 0xafff));
    assert_eq!(link_flag(&owner, 1, 0), 0x8123);
    owner.de_rici_conns().unwrap();
    assert_eq!(image(&owner), expected);
    owner.destroy(&mut memory).unwrap();
    assert!(owner.web_path().is_none());
}

#[test]
fn empty_path_never_inspects_its_unused_node_pointer() {
    let (mut owner, memory) = empty();
    owner.put(4, 0x1234);
    let before = image(&owner);
    owner.de_rici_conns().unwrap();
    assert_eq!(image(&owner), before);
    assert_eq!(memory.allocations, 0);
    assert!(memory.frees.is_empty());
}

#[test]
fn zero_connection_count_never_checks_unused_link_storage() {
    let (mut owner, _) = fixture();
    let record = nodes(&mut owner).node_mut(0).unwrap();
    record[0x1d] = 0;
    super::super::put(record, 0x18, 0x1234);
    nodes(&mut owner).connections[0].as_mut().unwrap().freed = true;
    let links_before = nodes(&mut owner).connections[0]
        .as_ref()
        .unwrap()
        .allocation
        .bytes
        .clone();
    owner.de_rici_conns().unwrap();
    assert_eq!(flag(&owner, 0), 0xafff);
    assert_eq!(
        nodes(&mut owner).connections[0]
            .as_ref()
            .unwrap()
            .allocation
            .bytes,
        links_before
    );
}

#[test]
fn reverse_node_order_and_node_before_link_failure_keep_the_reached_prefix() {
    let (mut owner, _) = fixture();
    super::super::put(nodes(&mut owner).node_mut(0).unwrap(), 0x18, 0x1234);
    assert!(matches!(
        owner.de_rici_conns(),
        Err(ComponentError::WebPath(WebPathBoundary::ForeignStorage))
    ));
    assert_eq!((flag(&owner, 0), flag(&owner, 1)), (0xafff, 0xafff));
    let view = owner.web_path().unwrap();
    assert_eq!(view.node(1).unwrap().connection(0).unwrap().word_2, 0x8123);
    assert_eq!(view.node(0).unwrap().connection(0).unwrap().word_2, 0xffff);
    assert_eq!(view.node(0).unwrap().connection(1).unwrap().word_2, 0x5000);
    let after = image(&owner);
    assert!(matches!(
        owner.de_rici_conns(),
        Err(ComponentError::Interrupted)
    ));
    assert_eq!(image(&owner), after);
}

#[test]
fn reverse_link_order_rejects_last_truncated_link_before_earlier_link_write() {
    let (mut owner, _) = fixture();
    nodes(&mut owner).connections[0]
        .as_mut()
        .unwrap()
        .allocation
        .bytes
        .truncate(4 + CONNECTION_SIZE);
    assert!(matches!(
        owner.de_rici_conns(),
        Err(ComponentError::WebPath(
            WebPathBoundary::ConnectionOutsideAllocation
        ))
    ));
    assert_eq!(flag(&owner, 0), 0xafff);
    assert_eq!(link_flag(&owner, 0, 0), 0xffff);
    assert_eq!(link_flag(&owner, 1, 0), 0x8123);
}

#[test]
fn missing_link_owner_fails_after_node_write() {
    let (mut owner, _) = fixture();
    super::super::put(nodes(&mut owner).node_mut(1).unwrap(), 0x18, 0);
    nodes(&mut owner).connections[1] = None;
    assert!(matches!(
        owner.de_rici_conns(),
        Err(ComponentError::WebPath(
            WebPathBoundary::NullConnectionDestination
        ))
    ));
    assert_eq!((flag(&owner, 0), flag(&owner, 1)), (0xffff, 0xafff));
}

#[test]
fn active_node_pointer_mismatch_leaves_all_records_unchanged() {
    let (mut owner, _) = fixture();
    owner.put(4, 0x1234);
    let before = image(&owner);
    assert!(matches!(
        owner.de_rici_conns(),
        Err(ComponentError::WebPath(WebPathBoundary::ForeignStorage))
    ));
    assert_eq!(image(&owner), before);
}

#[test]
fn missing_node_owner_is_not_treated_as_an_empty_graph() {
    let (mut owner, _) = empty();
    owner.put(0xc, 1);
    assert!(matches!(
        owner.de_rici_conns(),
        Err(ComponentError::WebPath(
            WebPathBoundary::NullNodeDestination
        ))
    ));
}

#[test]
fn node_count_outside_allocation_fails_at_first_reverse_visit() {
    let (mut owner, _) = fixture();
    owner.put(0xc, 51);
    let before = image(&owner);
    assert!(matches!(
        owner.de_rici_conns(),
        Err(ComponentError::WebPath(
            WebPathBoundary::NodeOutsideAllocation
        ))
    ));
    assert_eq!(image(&owner), before);
}

#[test]
fn freed_and_foreign_node_storage_is_never_modified() {
    for foreign in [false, true] {
        let (mut owner, _) = fixture();
        if foreign {
            nodes(&mut owner).allocation.domain = MissionMemoryDomainId::fresh();
        } else {
            nodes(&mut owner).freed = true;
        }
        let before = image(&owner);
        assert!(matches!(
            owner.de_rici_conns(),
            Err(ComponentError::WebPath(WebPathBoundary::ForeignStorage))
        ));
        assert_eq!(image(&owner), before);
    }
}

#[test]
fn freed_and_foreign_connection_storage_preserves_node_first_order() {
    for foreign in [false, true] {
        let (mut owner, _) = fixture();
        let array = nodes(&mut owner).connections[1].as_mut().unwrap();
        if foreign {
            array.allocation.domain = MissionMemoryDomainId::fresh();
        } else {
            array.freed = true;
        }
        let before = array.allocation.bytes.clone();
        assert!(matches!(
            owner.de_rici_conns(),
            Err(ComponentError::WebPath(WebPathBoundary::ForeignStorage))
        ));
        assert_eq!((flag(&owner, 0), flag(&owner, 1)), (0xffff, 0xafff));
        assert_eq!(
            nodes(&mut owner).connections[1]
                .as_ref()
                .unwrap()
                .allocation
                .bytes,
            before
        );
    }
}

#[test]
fn invalid_component_lifecycle_is_rejected_without_graph_changes() {
    for state in 0..4 {
        let (mut owner, _) = fixture();
        match state {
            0 => owner.interrupted = true,
            1 => owner.destroyed = true,
            2 => owner.constructed = false,
            _ => owner.kind = MissionComponentKind::Roads,
        }
        let before = image(&owner);
        assert!(owner.de_rici_conns().is_err());
        assert_eq!(image(&owner), before);
    }
}
