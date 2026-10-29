//! Actual Installation/LoadNodes composition with authored files and allocations.
use super::*;
use crate::mission_components::{
    ComponentError, MissionAllocation, MissionAllocator, MissionComponentKind, MissionMemoryDomain,
    MissionMemoryDomainId, WebArrayKind,
};
use std::collections::BTreeSet;

struct Memory {
    domain: MissionMemoryDomainId,
    next: u32,
    live: BTreeSet<u32>,
    frees: Vec<u32>,
    arrays: usize,
    fail_arrays: bool,
}
impl Memory {
    fn new() -> Self {
        Self {
            domain: MissionMemoryDomainId::fresh(),
            next: 0x10000,
            live: BTreeSet::new(),
            frees: Vec::new(),
            arrays: 0,
            fail_arrays: false,
        }
    }
    fn owned(&mut self, bytes: u32) -> MissionAllocation {
        assert!(bytes <= 4 * 1024 * 1024, "synthetic allocation quota");
        let identity = self.next;
        self.next = self.next.checked_add(bytes + 16).unwrap();
        assert!(self.live.insert(identity));
        MissionAllocation {
            domain: self.domain,
            identity,
            bytes: vec![0xa5; bytes as usize],
        }
    }
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
        kind: MissionComponentKind,
    ) -> Result<Option<MissionAllocation>, Self::Error> {
        Ok(Some(self.owned(kind.allocation_size() as u32)))
    }
    fn free(&mut self, identity: u32) -> Result<(), Self::Error> {
        if identity != 0 && !self.live.remove(&identity) {
            return Err("unowned or repeated free");
        }
        self.frees.push(identity);
        Ok(())
    }
    fn open_effects_table(&mut self) -> Result<bool, Self::Error> {
        Err("unreached effects table")
    }
}
impl WebPathAllocator for Memory {
    fn allocate_web_array(
        &mut self,
        _kind: WebArrayKind,
        _elements: u32,
        byte_length: u32,
    ) -> Result<Option<MissionAllocation>, Self::Error> {
        self.arrays += 1;
        if self.fail_arrays {
            return Err("array allocation unavailable");
        }
        Ok(Some(self.owned(byte_length)))
    }
}
fn owner(kind: MissionComponentKind) -> (MissionComponent, Memory) {
    let mut memory = Memory::new();
    let allocation = memory.allocate(kind).unwrap().unwrap();
    let component = MissionComponent::construct(kind, allocation).unwrap();
    (component, memory)
}
fn close(component: &mut MissionComponent, memory: &mut Memory) {
    component.destroy(memory).unwrap();
    memory.free(component.identity()).unwrap();
    assert!(memory.live.is_empty());
}

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lhr-navigation-input-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, bytes: &[u8]) {
        std::fs::write(self.0.join("check.bin"), bytes).unwrap();
    }
    fn installation(&self) -> Installation {
        Installation::open(&self.0).unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn node(connections: u8, marker: u8) -> [u8; 30] {
    let mut bytes = [marker; 30];
    bytes[24..28].copy_from_slice(&0xf123_4567u32.to_le_bytes());
    bytes[28] = 200;
    bytes[29] = connections;
    bytes
}
fn payload(nodes: &[[u8; 30]], links: &[[u8; 8]]) -> Vec<u8> {
    let mut bytes = 0x01ab_cedfu32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&(nodes.len() as u32).to_le_bytes());
    for node in nodes {
        bytes.extend_from_slice(node);
    }
    for link in links {
        bytes.extend_from_slice(link);
    }
    bytes
}
fn load(
    scratch: &Scratch,
    component: &mut MissionComponent,
    memory: &mut Memory,
) -> Result<NavigationLoadOutcome, NavigationInputError<&'static str>> {
    load_installed_navigation(component, memory, &scratch.installation(), "check.bin")
}
fn returned(outcome: NavigationLoadOutcome) -> NavigationReadReceipt {
    match outcome {
        NavigationLoadOutcome::Returned(read) => read,
        NavigationLoadOutcome::Absent => panic!("authored input must exist"),
    }
}
fn prime(scratch: &Scratch, component: &mut MissionComponent, memory: &mut Memory) {
    scratch.write(&payload(&[node(1, 0x31)], &[[0; 8]]));
    assert_eq!(
        returned(load(scratch, component, memory).unwrap()).consumed_bytes,
        46
    );
    assert_eq!(component.web_path().unwrap().node_count(), 1);
    assert_eq!(memory.live.len(), 3);
}

#[test]
fn root_reads_have_no_chunk_bound_and_preserve_the_unread_destination_suffix() {
    let mut input = RootNavigationInput {
        bytes: &[1, 2, 3],
        position: 0,
        read_calls: 0,
        short_reads: 0,
    };
    assert!(input.is_open());
    assert!(!input.is_writing());
    assert!(input.active_bound().is_none());
    assert_eq!(input.seek_current(), Ok(0));
    let mut first = [0xa5; 2];
    assert_eq!(input.read_data(&mut first), Ok(2));
    assert_eq!(first, [1, 2]);
    let mut last = [0xa5; 4];
    assert_eq!(input.read_data(&mut last), Ok(1));
    assert_eq!(last, [3, 0xa5, 0xa5, 0xa5]);
    assert_eq!(input.read_data(&mut []), Ok(0));
    let mut eof = [0x39; 2];
    assert_eq!(input.read_data(&mut eof), Ok(0));
    assert_eq!(eof, [0x39; 2]);
    assert_eq!(input.seek_current(), Ok(3));
    assert_eq!(
        input.receipt(),
        NavigationReadReceipt {
            source_bytes: 3,
            consumed_bytes: 3,
            read_calls: 4,
            short_reads: 2,
        }
    );
}

#[test]
fn absent_optional_input_does_not_clear_or_validate_the_existing_owner() {
    let scratch = Scratch::new();
    let (mut component, mut memory) = owner(MissionComponentKind::WebPath);
    prime(&scratch, &mut component, &mut memory);
    std::fs::remove_file(scratch.0.join("check.bin")).unwrap();
    let before = component.raw_bytes().to_vec();
    let frees = memory.frees.len();
    let domain = memory.domain;
    memory.domain = MissionMemoryDomainId::fresh();
    assert_eq!(
        load(&scratch, &mut component, &mut memory).unwrap(),
        NavigationLoadOutcome::Absent
    );
    assert_eq!(component.raw_bytes(), before);
    assert_eq!(memory.frees.len(), frees);
    assert_eq!(component.web_path().unwrap().node_count(), 1);
    memory.domain = domain;
    close(&mut component, &mut memory);
}

#[test]
fn actual_installation_payload_populates_the_same_owner_and_destroys_normally() {
    let scratch = Scratch::new();
    let (mut component, mut memory) = owner(MissionComponentKind::WebPath);
    let identity = component.identity();
    let nodes = [node(1, 0x31), node(1, 0x42)];
    let links = [[1, 0, 9, 0, 0, 0, 0x80, 0x3f], [0, 0, 7, 0, 0, 0, 0, 0x40]];
    scratch.write(&payload(&nodes, &links));
    let receipt = returned(load(&scratch, &mut component, &mut memory).unwrap());
    assert_eq!(
        receipt,
        NavigationReadReceipt {
            source_bytes: 84,
            consumed_bytes: 84,
            read_calls: 5,
            short_reads: 0,
        }
    );
    assert_eq!(component.identity(), identity);
    let graph = component.web_path().unwrap();
    assert_eq!((graph.node_count(), graph.node_capacity()), (2, 50));
    assert_eq!(&graph.node(0).unwrap().raw_bytes()[..24], &nodes[0][..24]);
    let first = graph.node(0).unwrap().connection(0).unwrap();
    assert_eq!(
        (first.node_index, first.word_2, first.distance_bits),
        (1, 9, 1f32.to_bits())
    );
    assert_eq!(
        graph.node(1).unwrap().connection(0).unwrap().distance_bits,
        2f32.to_bits()
    );
    assert_eq!(memory.live.len(), 4);
    close(&mut component, &mut memory);
}

#[test]
fn mismatched_signature_runs_the_loader_clear_before_returning_normally() {
    let scratch = Scratch::new();
    let (mut component, mut memory) = owner(MissionComponentKind::WebPath);
    prime(&scratch, &mut component, &mut memory);
    memory.frees.clear();
    scratch.write(b"WRONG trailing bytes");
    let receipt = returned(load(&scratch, &mut component, &mut memory).unwrap());
    assert_eq!(receipt.consumed_bytes, 4);
    assert_eq!(receipt.read_calls, 1);
    assert_eq!(receipt.short_reads, 0);
    assert_eq!(component.web_path().unwrap().node_count(), 0);
    assert_eq!(memory.live.len(), 1);
    assert_eq!(memory.frees.len(), 2);
    close(&mut component, &mut memory);
}

#[test]
fn short_signature_retains_disposal_prefix_and_rejects_destructive_replay() {
    for size in 0..4 {
        let scratch = Scratch::new();
        let (mut component, mut memory) = owner(MissionComponentKind::WebPath);
        prime(&scratch, &mut component, &mut memory);
        memory.frees.clear();
        let signature = 0x01ab_cedfu32.to_le_bytes();
        scratch.write(&signature[..size]);
        let error = load(&scratch, &mut component, &mut memory).unwrap_err();
        match error {
            NavigationInputError::Load {
                error: WebPathLoadError::UninitializedSignature { initialized_prefix },
                read,
            } => {
                assert_eq!(initialized_prefix, signature[..size]);
                assert_eq!(
                    (read.consumed_bytes, read.read_calls, read.short_reads),
                    (size, 1, 1)
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(memory.frees.len(), 2);
        assert_eq!(memory.live.len(), 1);
        assert_eq!(component.web_path().unwrap().node_count(), 0);
        let frees = memory.frees.clone();
        assert!(matches!(
            load(&scratch, &mut component, &mut memory),
            Err(NavigationInputError::Load {
                error: WebPathLoadError::Component(ComponentError::Interrupted),
                read: NavigationReadReceipt {
                    consumed_bytes: 0,
                    read_calls: 0,
                    ..
                },
            })
        ));
        assert_eq!(memory.frees, frees);
        // The graph's explicit interrupted-owner state is not cleared by a test.
    }
}

#[test]
fn short_zero_count_is_reported_without_inventing_a_strict_read_failure() {
    for initialized in 0..4 {
        let scratch = Scratch::new();
        let (mut component, mut memory) = owner(MissionComponentKind::WebPath);
        let mut bytes = 0x01ab_cedfu32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[0; 4][..initialized]);
        scratch.write(&bytes);
        let read = returned(load(&scratch, &mut component, &mut memory).unwrap());
        assert_eq!(
            (read.consumed_bytes, read.read_calls, read.short_reads),
            (4 + initialized, 3, 1)
        );
        assert_eq!(component.web_path().unwrap().node_count(), 0);
        assert_eq!(memory.arrays, 0);
        close(&mut component, &mut memory);
    }
}

#[test]
fn short_node_read_preserves_the_constructed_record_suffix() {
    let scratch = Scratch::new();
    let (mut component, mut memory) = owner(MissionComponentKind::WebPath);
    let bytes = payload(&[node(0, 0x31)], &[]);
    scratch.write(&bytes[..10]);
    let read = returned(load(&scratch, &mut component, &mut memory).unwrap());
    assert_eq!(
        (read.consumed_bytes, read.read_calls, read.short_reads),
        (10, 4, 1)
    );
    let graph = component.web_path().unwrap();
    assert_eq!(graph.node_count(), 1);
    let record = graph.node(0).unwrap();
    assert_eq!(&record.raw_bytes()[..2], &[0x31; 2]);
    assert_eq!(&record.raw_bytes()[16..20], &50u32.to_le_bytes());
    assert_eq!(record.connection_count(), 0);
    close(&mut component, &mut memory);
}

#[test]
fn short_connection_read_keeps_allocator_seed_for_unwritten_distance() {
    let scratch = Scratch::new();
    let (mut component, mut memory) = owner(MissionComponentKind::WebPath);
    let bytes = payload(&[node(1, 0x31)], &[[0x34, 0x12, 0x77, 0x66, 1, 2, 3, 4]]);
    scratch.write(&bytes[..41]);
    let read = returned(load(&scratch, &mut component, &mut memory).unwrap());
    assert_eq!(
        (read.consumed_bytes, read.read_calls, read.short_reads),
        (41, 4, 1)
    );
    let link = component
        .web_path()
        .unwrap()
        .node(0)
        .unwrap()
        .connection(0)
        .unwrap();
    assert_eq!(
        (link.node_index, link.word_2, link.distance_bits),
        (0x1234, 0x77, 0xa5a5_a5a5)
    );
    close(&mut component, &mut memory);
}

#[test]
fn trailing_bytes_are_not_promoted_into_nodes_or_forced_to_eof() {
    let scratch = Scratch::new();
    let (mut component, mut memory) = owner(MissionComponentKind::WebPath);
    let mut bytes = payload(&[node(0, 0x31)], &[]);
    let consumed = bytes.len();
    bytes.extend_from_slice(b"unconsumed");
    scratch.write(&bytes);
    let read = returned(load(&scratch, &mut component, &mut memory).unwrap());
    assert_eq!(read.source_bytes, bytes.len());
    assert_eq!(read.consumed_bytes, consumed);
    assert_eq!(read.short_reads, 0);
    assert_eq!(component.web_path().unwrap().node_count(), 1);
    close(&mut component, &mut memory);
}

#[test]
fn resolver_errors_are_not_absence_and_do_not_enter_the_loader() {
    let scratch = Scratch::new();
    let (mut component, mut memory) = owner(MissionComponentKind::WebPath);
    prime(&scratch, &mut component, &mut memory);
    let before = component.raw_bytes().to_vec();
    let frees = memory.frees.clone();
    let installation = scratch.installation();
    assert!(matches!(
        load_installed_navigation(&mut component, &mut memory, &installation, "../check.bin"),
        Err(NavigationInputError::Source(_))
    ));
    std::fs::remove_file(scratch.0.join("check.bin")).unwrap();
    std::fs::create_dir(scratch.0.join("check.bin")).unwrap();
    assert!(matches!(
        load_installed_navigation(&mut component, &mut memory, &installation, "check.bin"),
        Err(NavigationInputError::Source(_))
    ));
    assert_eq!(component.raw_bytes(), before);
    assert_eq!(memory.frees, frees);
    assert_eq!(component.web_path().unwrap().node_count(), 1);
    close(&mut component, &mut memory);
}

#[test]
fn allocation_failure_reports_read_prefix_and_never_replays_prior_frees() {
    let scratch = Scratch::new();
    let (mut component, mut memory) = owner(MissionComponentKind::WebPath);
    prime(&scratch, &mut component, &mut memory);
    memory.frees.clear();
    memory.fail_arrays = true;
    assert!(matches!(
        load(&scratch, &mut component, &mut memory),
        Err(NavigationInputError::Load {
            error: WebPathLoadError::Component(ComponentError::Allocator(
                "array allocation unavailable"
            )),
            read: NavigationReadReceipt {
                consumed_bytes: 8,
                read_calls: 2,
                ..
            },
        })
    ));
    assert_eq!(memory.frees.len(), 2);
    assert_eq!(memory.live.len(), 1);
    let frees = memory.frees.clone();
    assert!(matches!(
        load(&scratch, &mut component, &mut memory),
        Err(NavigationInputError::Load {
            error: WebPathLoadError::Component(ComponentError::Interrupted),
            read: NavigationReadReceipt {
                consumed_bytes: 0,
                read_calls: 0,
                ..
            },
        })
    ));
    assert_eq!(memory.frees, frees);
}

#[test]
fn actual_owner_rejection_happens_before_any_payload_read() {
    for foreign in [false, true] {
        let scratch = Scratch::new();
        scratch.write(&payload(&[], &[]));
        let kind = if foreign {
            MissionComponentKind::WebPath
        } else {
            MissionComponentKind::Roads
        };
        let (mut component, mut memory) = owner(kind);
        let before = component.raw_bytes().to_vec();
        let domain = memory.domain;
        if foreign {
            memory.domain = MissionMemoryDomainId::fresh();
        }
        let error = load(&scratch, &mut component, &mut memory).unwrap_err();
        match error {
            NavigationInputError::Load {
                error: WebPathLoadError::Component(error),
                read,
            } => {
                if foreign {
                    assert!(matches!(error, ComponentError::ForeignDomain));
                } else {
                    assert!(matches!(error, ComponentError::Unavailable { .. }));
                }
                assert_eq!((read.read_calls, read.consumed_bytes), (0, 0));
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert_eq!(component.raw_bytes(), before);
        assert!(memory.frees.is_empty());
        assert_eq!(memory.arrays, 0);
        memory.domain = domain;
        close(&mut component, &mut memory);
    }
}
