use super::*;
use crate::mission_components::{MissionComponent, MissionComponentKind, MissionMemoryDomain};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event {
    Allocate(WebArrayKind, u32, u32),
    Free(u32),
    Seek,
    Read(usize),
}
type Events = Rc<RefCell<Vec<Event>>>;
struct Memory {
    domain: MissionMemoryDomainId,
    events: Events,
    next: u32,
    allocations: usize,
    null_at: Option<usize>,
    allocation_error_at: Option<usize>,
    frees: usize,
    free_error_at: Option<usize>,
}
impl MissionMemoryDomain for Memory {
    fn mission_memory_domain(&self) -> MissionMemoryDomainId {
        self.domain
    }
}
impl MissionAllocator for Memory {
    type Error = &'static str;
    fn allocate(
        &mut self,
        _: MissionComponentKind,
    ) -> Result<Option<MissionAllocation>, Self::Error> {
        unreachable!()
    }
    fn free(&mut self, identity: u32) -> Result<(), Self::Error> {
        self.events.borrow_mut().push(Event::Free(identity));
        self.frees += 1;
        if self.free_error_at == Some(self.frees) {
            Err("free")
        } else {
            Ok(())
        }
    }
    fn open_effects_table(&mut self) -> Result<bool, Self::Error> {
        unreachable!()
    }
}
impl WebPathAllocator for Memory {
    fn allocate_web_array(
        &mut self,
        kind: WebArrayKind,
        elements: u32,
        byte_length: u32,
    ) -> Result<Option<MissionAllocation>, Self::Error> {
        self.events
            .borrow_mut()
            .push(Event::Allocate(kind, elements, byte_length));
        self.allocations += 1;
        if self.allocation_error_at == Some(self.allocations) {
            return Err("allocate");
        }
        if self.null_at == Some(self.allocations) {
            return Ok(None);
        }
        let identity = self.next;
        self.next += 0x1000;
        Ok(Some(MissionAllocation {
            domain: self.domain,
            identity,
            bytes: vec![0xa5; byte_length as usize],
        }))
    }
}
struct Input {
    bytes: Vec<u8>,
    cursor: usize,
    open: bool,
    writing: bool,
    bound: Option<Rc<Cell<i32>>>,
    seek_changes: VecDeque<i32>,
    limits: VecDeque<usize>,
    reads: usize,
    read_error_at: Option<usize>,
    events: Events,
}
impl Input {
    fn new(bytes: Vec<u8>, events: &Events) -> Self {
        let length = bytes.len() as i32;
        Self {
            bytes,
            cursor: 0,
            open: true,
            writing: false,
            bound: Some(Rc::new(Cell::new(length))),
            seek_changes: VecDeque::new(),
            limits: VecDeque::new(),
            reads: 0,
            read_error_at: None,
            events: events.clone(),
        }
    }
}
impl WebPathChunk for Input {
    type Error = &'static str;
    type Bound = Rc<Cell<i32>>;
    fn is_open(&self) -> bool {
        self.open
    }
    fn is_writing(&self) -> bool {
        self.writing
    }
    fn active_bound(&self) -> Option<Self::Bound> {
        self.bound.clone()
    }
    fn seek_current(&mut self) -> Result<i32, Self::Error> {
        self.events.borrow_mut().push(Event::Seek);
        if let Some(value) = self.seek_changes.pop_front() {
            self.bound.as_ref().unwrap().set(value);
        }
        Ok(self.cursor as i32)
    }
    fn read_bound(&self, bound: Self::Bound) -> Result<i32, Self::Error> {
        Ok(bound.get())
    }
    fn read_data(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error> {
        self.events
            .borrow_mut()
            .push(Event::Read(destination.len()));
        self.reads += 1;
        let limit = self.limits.pop_front().unwrap_or(usize::MAX);
        let count = destination
            .len()
            .min(self.bytes.len() - self.cursor)
            .min(limit);
        destination[..count].copy_from_slice(&self.bytes[self.cursor..self.cursor + count]);
        self.cursor += count;
        if self.read_error_at == Some(self.reads) {
            Err("read")
        } else {
            Ok(count)
        }
    }
}
fn setup() -> (MissionComponent, Memory, Events) {
    let domain = MissionMemoryDomainId::fresh();
    let events = Rc::new(RefCell::new(Vec::new()));
    let path = MissionComponent::construct(
        MissionComponentKind::WebPath,
        MissionAllocation {
            domain,
            identity: 0x7000,
            bytes: vec![0x39; 0x1d],
        },
    )
    .unwrap();
    let memory = Memory {
        domain,
        events: events.clone(),
        next: 0x10000,
        allocations: 0,
        null_at: None,
        allocation_error_at: None,
        frees: 0,
        free_error_at: None,
    };
    (path, memory, events)
}
fn node(connections: u8, marker: u8) -> [u8; NODE_SIZE] {
    let mut bytes = [marker; NODE_SIZE];
    put(&mut bytes, 0x18, 0xf123_4567);
    bytes[0x1c] = 200;
    bytes[0x1d] = connections;
    bytes
}
fn fixture(nodes: &[[u8; NODE_SIZE]], connections: &[[u8; 8]]) -> Vec<u8> {
    let mut out = 0x01ab_cedfu32.to_le_bytes().to_vec();
    out.extend_from_slice(&(nodes.len() as u32).to_le_bytes());
    for node in nodes {
        out.extend_from_slice(node);
    }
    for connection in connections {
        out.extend_from_slice(connection);
    }
    out
}
fn frees(events: &Events) -> Vec<u32> {
    events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            Event::Free(identity) => Some(*identity),
            _ => None,
        })
        .collect()
}
fn reads(events: &Events) -> Vec<usize> {
    events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            Event::Read(length) => Some(*length),
            _ => None,
        })
        .collect()
}

#[test]
fn populated_load_owns_actual_arrays_and_destroys_reverse_cookie_capacity() {
    let (mut path, mut memory, events) = setup();
    let nodes = [node(1, 0x31), node(1, 0x42)];
    let connections = [[1, 0, 9, 0, 0, 0, 0x80, 0x3f], [0, 0, 7, 0, 0, 0, 0, 0x40]];
    let mut input = Input::new(fixture(&nodes, &connections), &events);
    path.load_web_nodes(&mut memory, &mut input).unwrap();
    let view = path.web_path().unwrap();
    assert_eq!((view.node_count(), view.node_capacity()), (2, 50));
    assert_eq!(&view.node(0).unwrap().raw_bytes()[..24], &nodes[0][..24]);
    assert_eq!(view.node(0).unwrap().connection_capacity(), 1);
    assert_eq!(
        view.node(0).unwrap().connection(0),
        Some(WebConnection {
            node_index: 1,
            word_2: 9,
            distance_bits: 1f32.to_bits()
        })
    );
    assert_eq!(
        view.node(1).unwrap().connection(0).unwrap().distance_bits,
        2f32.to_bits()
    );
    assert_eq!(reads(&events), vec![4, 4, 60, 8, 8]);
    assert_eq!(input.cursor, input.bytes.len());
    path.destroy(&mut memory).unwrap();
    assert_eq!(frees(&events), vec![0x12000, 0x11000, 0x10000, 0]);
    assert!(path.web_path().is_none());
    assert!(matches!(
        path.destroy(&mut memory),
        Err(ComponentError::Interrupted)
    ));
}

#[test]
fn reload_disposes_old_graph_before_bad_signature_and_retains_other_path_fields() {
    let (mut path, mut memory, events) = setup();
    path.load_web_nodes(
        &mut memory,
        &mut Input::new(fixture(&[node(1, 0x22)], &[[1; 8]]), &events),
    )
    .unwrap();
    let retained = path.raw_bytes()[0x10..].to_vec();
    events.borrow_mut().clear();
    path.load_web_nodes(&mut memory, &mut Input::new(vec![0; 4], &events))
        .unwrap();
    assert_eq!(
        &events.borrow()[..3],
        &[Event::Free(0x11000), Event::Free(0x10000), Event::Seek]
    );
    assert_eq!(reads(&events), vec![4]);
    assert_eq!(&path.raw_bytes()[4..16], &[0; 12]);
    assert_eq!(&path.raw_bytes()[0x10..], retained);
    assert_eq!(path.web_path().unwrap().node_count(), 0);
}

#[test]
fn short_or_skipped_signature_never_consumes_invented_stack_bytes() {
    for open in [false, true] {
        let (mut path, mut memory, events) = setup();
        let mut input = Input::new(0x01ab_cedfu32.to_le_bytes().to_vec(), &events);
        input.open = open;
        input.limits.push_back(2);
        match path.load_web_nodes(&mut memory, &mut input) {
            Err(WebPathLoadError::UninitializedSignature { initialized_prefix }) => {
                assert_eq!(
                    initialized_prefix,
                    if open { vec![0xdf, 0xce] } else { vec![] }
                );
            }
            other => panic!("unexpected result: {other:?}"),
        }
        assert_eq!(memory.allocations, 0);
        let reached = events.borrow().len();
        assert!(matches!(
            path.load_web_nodes(&mut memory, &mut input),
            Err(WebPathLoadError::Component(ComponentError::Interrupted))
        ));
        assert_eq!(events.borrow().len(), reached);
    }
}

#[test]
fn chunk_bound_is_reread_after_seek_and_skipped_node_reads_keep_constructor_state() {
    let (mut path, mut memory, events) = setup();
    let mut input = Input::new(fixture(&[node(0, 0x22)], &[]), &events);
    input.seek_changes.push_back(2);
    assert!(matches!(
        path.load_web_nodes(&mut memory, &mut input),
        Err(WebPathLoadError::UninitializedSignature { .. })
    ));
    assert_eq!(&*events.borrow(), &[Event::Seek]);

    let (mut path, mut memory, events) = setup();
    let mut input = Input::new(fixture(&[node(0, 0x22)], &[]), &events);
    input.bound.as_ref().unwrap().set(8);
    path.load_web_nodes(&mut memory, &mut input).unwrap();
    assert_eq!(input.cursor, 8);
    let view = path.web_path().unwrap();
    let loaded = view.node(0).unwrap();
    assert_eq!(word(loaded.raw_bytes(), 0x10), 50);
    assert_eq!(loaded.connection_count(), 0);
    assert_eq!(reads(&events), vec![4, 4, 0]);
}

#[test]
fn partial_count_node_and_connection_writes_preserve_the_native_destination_seeds() {
    let (mut path, mut memory, events) = setup();
    let mut bytes = 0x01ab_cedfu32.to_le_bytes().to_vec();
    bytes.push(1); // Only one byte of the count is delivered by dtaRead.
    bytes.extend_from_slice(&[0; NODE_SIZE]);
    let mut input = Input::new(bytes, &events);
    input.bound = None;
    input.limits = [4, 1].into();
    path.load_web_nodes(&mut memory, &mut input).unwrap();
    assert_eq!(path.web_path().unwrap().node_count(), 1);

    let (mut path, mut memory, events) = setup();
    let mut input = Input::new(fixture(&[node(1, 0x73)], &[[7; 8]]), &events);
    input.limits = [4, 4, 12].into();
    path.load_web_nodes(&mut memory, &mut input).unwrap();
    let view = path.web_path().unwrap();
    let loaded = view.node(0).unwrap();
    assert_eq!(&loaded.raw_bytes()[..12], &[0x73; 12]);
    assert_eq!(word(loaded.raw_bytes(), 0x10), 50);
    assert_eq!(loaded.connection_count(), 0);

    let (mut path, mut memory, events) = setup();
    let mut input = Input::new(
        fixture(&[node(1, 0x73)], &[[1, 0, 2, 0, 3, 4, 5, 6]]),
        &events,
    );
    input.limits = [4, 4, 30, 4].into();
    path.load_web_nodes(&mut memory, &mut input).unwrap();
    assert_eq!(
        path.web_path().unwrap().node(0).unwrap().connection(0),
        Some(WebConnection {
            node_index: 1,
            word_2: 2,
            distance_bits: 0xa5a5_a5a5,
        })
    );
}

#[test]
fn interrupted_read_retains_actual_partial_node_storage_and_blocks_replay() {
    let (mut path, mut memory, events) = setup();
    let mut input = Input::new(fixture(&[node(1, 0x19)], &[[5; 8]]), &events);
    input.limits = [4, 4, 8].into();
    input.read_error_at = Some(3);
    assert!(matches!(
        path.load_web_nodes(&mut memory, &mut input),
        Err(WebPathLoadError::Chunk("read"))
    ));
    let view = path.web_path().unwrap();
    assert_eq!((view.node_count(), view.node_capacity()), (1, 50));
    assert_eq!(&view.node(0).unwrap().raw_bytes()[..8], &[0x19; 8]);
    assert_eq!(word(view.node(0).unwrap().raw_bytes(), 0x10), 50);
    assert!(matches!(
        path.destroy(&mut memory),
        Err(ComponentError::Interrupted)
    ));
    assert!(frees(&events).is_empty());
}

#[test]
fn allocation_failure_and_null_destination_keep_reached_capacity_and_count_prefixes() {
    for null in [false, true] {
        let (mut path, mut memory, events) = setup();
        if null {
            memory.null_at = Some(1);
        } else {
            memory.allocation_error_at = Some(1);
        }
        let mut input = Input::new(fixture(&[node(0, 0)], &[]), &events);
        let result = path.load_web_nodes(&mut memory, &mut input);
        if null {
            assert!(matches!(
                result,
                Err(WebPathLoadError::Component(ComponentError::WebPath(
                    WebPathBoundary::NullNodeDestination
                )))
            ));
        } else {
            assert!(matches!(
                result,
                Err(WebPathLoadError::Component(ComponentError::Allocator(
                    "allocate"
                )))
            ));
        }
        let view = path.web_path().unwrap();
        assert_eq!(view.node_capacity(), 50);
        assert_eq!(view.node_count(), u32::from(null));
        assert_eq!(reads(&events), vec![4, 4]);
        assert_eq!(input.cursor, 8);
    }
}

#[test]
fn node_unsigned16_growth_truncates_to_byte_and_path_resize_deep_copies_active_connections() {
    let (mut path, mut memory, events) = setup();
    let nodes = [node(1, 0x13), node(1, 0x24)];
    let connections = [[1, 0, 3, 0, 1, 2, 3, 4], [0, 0, 5, 0, 5, 6, 7, 8]];
    path.load_web_nodes(
        &mut memory,
        &mut Input::new(fixture(&nodes, &connections), &events),
    )
    .unwrap();
    path.resize_web_connections(0, 0x101, &mut memory).unwrap();
    assert_eq!(
        path.web_path()
            .unwrap()
            .node(0)
            .unwrap()
            .connection_capacity(),
        1
    );
    path.resize_web_connections(0, 4, &mut memory).unwrap();
    assert_eq!(
        path.web_path()
            .unwrap()
            .node(0)
            .unwrap()
            .connection_capacity(),
        4
    );
    events.borrow_mut().clear();
    path.resize_web_nodes(51, &mut memory).unwrap();
    let view = path.web_path().unwrap();
    assert_eq!((view.node_count(), view.node_capacity()), (2, 100));
    for (index, original) in nodes.iter().enumerate() {
        let loaded = view.node(index).unwrap();
        assert_eq!(&loaded.raw_bytes()[..24], &original[..24]);
        assert_eq!(loaded.connection_capacity(), 1);
        assert_eq!(loaded.connection_count(), 1);
        assert_eq!(
            loaded.connection(0).unwrap().distance_bits,
            word(&connections[index], 4)
        );
    }
    assert_eq!(
        &events.borrow()[..3],
        &[
            Event::Allocate(WebArrayKind::Nodes, 100, 3004),
            Event::Allocate(WebArrayKind::Connections, 1, 12),
            Event::Allocate(WebArrayKind::Connections, 1, 12),
        ]
    );
    assert_eq!(frees(&events), vec![0x12000, 0x14000, 0x10000]);
}

#[test]
fn populated_destruction_stops_at_failed_free_without_replaying_a_completed_child() {
    let (mut path, mut memory, events) = setup();
    path.load_web_nodes(
        &mut memory,
        &mut Input::new(
            fixture(&[node(1, 1), node(1, 2)], &[[1; 8], [2; 8]]),
            &events,
        ),
    )
    .unwrap();
    events.borrow_mut().clear();
    memory.free_error_at = Some(2);
    assert!(matches!(
        path.destroy(&mut memory),
        Err(ComponentError::Allocator("free"))
    ));
    assert_eq!(frees(&events), vec![0x12000, 0x11000]);
    assert!(matches!(
        path.destroy(&mut memory),
        Err(ComponentError::Interrupted)
    ));
    assert_eq!(frees(&events), vec![0x12000, 0x11000]);
}

#[test]
fn failed_old_array_free_retains_both_graphs_and_null_connections_stop_at_the_reached_read() {
    let (mut path, mut memory, events) = setup();
    path.load_web_nodes(
        &mut memory,
        &mut Input::new(
            fixture(&[node(1, 1), node(1, 2)], &[[1; 8], [2; 8]]),
            &events,
        ),
    )
    .unwrap();
    memory.free_error_at = Some(1);
    assert!(matches!(
        path.resize_web_nodes(51, &mut memory),
        Err(ComponentError::Allocator("free"))
    ));
    let graph = path.web_path.as_ref().unwrap();
    assert_eq!(graph.nodes.as_ref().unwrap().allocation.identity, 0x10000);
    let pending = graph.pending_nodes.as_ref().unwrap();
    assert_eq!(pending.allocation.identity, 0x13000);
    assert_eq!(
        pending.connections[0].as_ref().unwrap().allocation.identity,
        0x14000
    );
    assert_eq!(
        pending.connections[1].as_ref().unwrap().allocation.identity,
        0x15000
    );
    assert!(!pending.freed);
    assert_eq!(word(path.raw_bytes(), 4), 0x10004);
    assert_eq!(path.web_path().unwrap().node_capacity(), 100);
    assert_eq!(frees(&events), vec![0x12000]);
    assert!(matches!(
        path.resize_web_nodes(51, &mut memory),
        Err(ComponentError::Interrupted)
    ));

    let (mut path, mut memory, events) = setup();
    memory.null_at = Some(2);
    let mut input = Input::new(fixture(&[node(1, 0x22)], &[[1; 8]]), &events);
    assert!(matches!(
        path.load_web_nodes(&mut memory, &mut input),
        Err(WebPathLoadError::Component(ComponentError::WebPath(
            WebPathBoundary::NullConnectionDestination
        )))
    ));
    let view = path.web_path().unwrap();
    let loaded = view.node(0).unwrap();
    assert_eq!(loaded.connection_count(), 1);
    assert_eq!(loaded.connection_capacity(), 1);
    assert_eq!(word(loaded.raw_bytes(), 0x18), 0);
    assert_eq!(loaded.connection(0), None);
    assert_eq!(reads(&events), vec![4, 4, 30]);
    assert_eq!(events.borrow().last(), Some(&Event::Seek));
    assert_eq!(input.cursor, 38);
}

#[test]
fn unsupported_wrapped_constructor_keeps_the_reached_cookie_and_allocation_owner() {
    let (mut path, mut memory, events) = setup();
    path.put(0x10, 1);
    assert!(matches!(
        path.resize_web_nodes(0x8000_0000, &mut memory),
        Err(ComponentError::WebPath(WebPathBoundary::AllocationExtent))
    ));
    assert_eq!(
        &*events.borrow(),
        &[Event::Allocate(WebArrayKind::Nodes, 0x8000_0000, 4)]
    );
    assert_eq!(path.web_path().unwrap().node_capacity(), 0x8000_0000);
    let graph = path.web_path.as_ref().unwrap();
    assert_eq!(graph.rejected_allocations.len(), 1);
    assert_eq!(
        graph.rejected_allocations[0].bytes,
        0x8000_0000u32.to_le_bytes()
    );
    assert_eq!(graph.rejected_allocations[0].identity, 0x10000);
    assert!(matches!(
        path.destroy(&mut memory),
        Err(ComponentError::Interrupted)
    ));
    assert!(frees(&events).is_empty());
}
