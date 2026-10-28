//! Owned PC navigation dependency, Game SHA-256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! LoadNodes RVA14a8c0, path Resize14a220, node Resize149fc0;
//! callback constructors14d2c0/14d2b0 and destructors149f90/14a1e0.
//! Class names follow pinned mafia-re's attributed PS2 correspondence. New
//! field names below are descriptive; no PS2 symbol verification is claimed.
use super::{ComponentError, MissionAllocation, MissionAllocator, MissionMemoryDomainId};

const NODE_SIZE: usize = 0x1e;
const CONNECTION_SIZE: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebArrayKind {
    Nodes,
    Connections,
}

/// Allocation storage includes the native four-byte array cookie. The caller
/// supplies actual allocation seeds and identities in its existing memory
/// domain. Returning None represents native operator new returning null.
pub trait WebPathAllocator: MissionAllocator {
    fn allocate_web_array(
        &mut self,
        kind: WebArrayKind,
        elements: u32,
        byte_length: u32,
    ) -> Result<Option<MissionAllocation>, Self::Error>;
}

/// Live C_chunk boundary. Bound identifies the actual last stack cell captured
/// before dtaSeek, rather than a copy of its value. read_bound must reread that
/// cell AFTER seek_current; stale/freed backing is an explicit backend error.
/// read_data performs dtaRead into the supplied actual destination, returning
/// the initialized prefix length. Short reads may change that prefix and the
/// cursor; LoadNodes ignores native Read's boolean result.
pub trait WebPathChunk {
    type Error;
    type Bound;
    fn is_open(&self) -> bool;
    fn is_writing(&self) -> bool;
    fn active_bound(&self) -> Option<Self::Bound>;
    fn seek_current(&mut self) -> Result<i32, Self::Error>;
    fn read_bound(&self, bound: Self::Bound) -> Result<i32, Self::Error>;
    fn read_data(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebPathBoundary {
    ForeignStorage,
    AllocationExtent,
    CapacityDoesNotAdvance,
    CapacityWrap,
    NullNodeDestination,
    NullConnectionDestination,
    NodeOutsideAllocation,
    ConnectionOutsideAllocation,
    InvalidReadCount,
}

#[derive(Debug)]
pub enum WebPathLoadError<A, C> {
    Component(ComponentError<A>),
    Chunk(C),
    /// Native signature storage has no initializer. When a skipped/short read
    /// leaves unknown bytes, stop at their first use, after the disposal prefix.
    /// These bytes are the initialized prefix only, never invented stack seeds.
    UninitializedSignature {
        initialized_prefix: Vec<u8>,
    },
}

fn boundary<E>(reason: WebPathBoundary) -> ComponentError<E> {
    ComponentError::WebPath(reason)
}
fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn put(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

struct ConnectionArray {
    allocation: MissionAllocation,
    freed: bool,
}
impl ConnectionArray {
    fn pointer(&self) -> u32 {
        self.allocation.identity + 4
    }
    fn data(&self) -> &[u8] {
        &self.allocation.bytes[4..]
    }
    fn data_mut(&mut self) -> &mut [u8] {
        &mut self.allocation.bytes[4..]
    }
    fn destroy<A: MissionAllocator>(
        &mut self,
        allocator: &mut A,
    ) -> Result<(), ComponentError<A::Error>> {
        if self.freed {
            return Err(ComponentError::Interrupted);
        }
        // The reverse cookie-count element destructor404990 is empty.
        allocator
            .free(self.allocation.identity)
            .map_err(ComponentError::Allocator)?;
        self.freed = true;
        Ok(())
    }
}

struct NodeArray {
    allocation: MissionAllocation,
    connections: Vec<Option<ConnectionArray>>,
    // Native replacement allocations exist before the old array is released.
    // Keep them owned even if a subsequent host callback interrupts the call.
    pending_connections: Vec<ConnectionArray>,
    rejected_allocations: Vec<MissionAllocation>,
    freed: bool,
}
impl NodeArray {
    fn pointer(&self) -> u32 {
        self.allocation.identity + 4
    }
    fn node(&self, index: usize) -> Option<&[u8]> {
        let start = 4usize.checked_add(index.checked_mul(NODE_SIZE)?)?;
        self.allocation.bytes.get(start..start + NODE_SIZE)
    }
    fn node_mut(&mut self, index: usize) -> Option<&mut [u8]> {
        let start = 4usize.checked_add(index.checked_mul(NODE_SIZE)?)?;
        self.allocation.bytes.get_mut(start..start + NODE_SIZE)
    }
    fn check_connections<E>(&self, index: usize) -> Result<(), ComponentError<E>> {
        let node = self
            .node(index)
            .ok_or_else(|| boundary(WebPathBoundary::NodeOutsideAllocation))?;
        let pointer = self.connections[index]
            .as_ref()
            .map_or(0, ConnectionArray::pointer);
        if word(node, 0x18) != pointer {
            return Err(boundary(WebPathBoundary::ForeignStorage));
        }
        Ok(())
    }
    fn resize_connections<A: WebPathAllocator>(
        &mut self,
        index: usize,
        requested: u16,
        allocator: &mut A,
    ) -> Result<(), ComponentError<A::Error>> {
        let node = self
            .node(index)
            .ok_or_else(|| boundary(WebPathBoundary::NodeOutsideAllocation))?;
        if u16::from(node[0x1c]) >= requested {
            return Ok(());
        }
        // CMP is unsigned16, while capacity and allocation count use low8.
        self.node_mut(index).unwrap()[0x1c] = requested as u8;
        let count = u32::from(requested as u8);
        let replacement = allocate_array(
            allocator,
            WebArrayKind::Connections,
            count,
            CONNECTION_SIZE,
            &mut self.rejected_allocations,
        )?;
        if let Some(allocation) = replacement {
            self.pending_connections.push(ConnectionArray {
                allocation,
                freed: false,
            });
        }
        self.check_connections(index)?;
        let mut copied = 0;
        while copied < usize::from(self.node(index).unwrap()[0x1d]) {
            let start = copied * CONNECTION_SIZE;
            let source = self.connections[index]
                .as_ref()
                .and_then(|array| array.data().get(start..start + CONNECTION_SIZE))
                .ok_or_else(|| boundary(WebPathBoundary::ConnectionOutsideAllocation))?;
            let destination = self
                .pending_connections
                .last_mut()
                .ok_or_else(|| boundary(WebPathBoundary::NullConnectionDestination))?
                .data_mut()
                .get_mut(start..start + CONNECTION_SIZE)
                .ok_or_else(|| boundary(WebPathBoundary::ConnectionOutsideAllocation))?;
            destination.copy_from_slice(source);
            copied += 1;
        }
        if let Some(old) = &mut self.connections[index] {
            old.destroy(allocator)?;
        }
        self.connections[index] = self.pending_connections.pop();
        let pointer = self.connections[index]
            .as_ref()
            .map_or(0, ConnectionArray::pointer);
        put(self.node_mut(index).unwrap(), 0x18, pointer);
        Ok(())
    }
    fn destroy<A: MissionAllocator>(
        &mut self,
        allocator: &mut A,
    ) -> Result<(), ComponentError<A::Error>> {
        if self.freed {
            return Err(ComponentError::Interrupted);
        }
        // Destruction uses the allocation cookie, including capacity nodes
        // beyond the path's active count, in reverse order.
        let count = word(&self.allocation.bytes, 0) as usize;
        for index in (0..count).rev() {
            self.check_connections(index)?;
            if let Some(connections) = &mut self.connections[index] {
                connections.destroy(allocator)?;
            }
        }
        allocator
            .free(self.allocation.identity)
            .map_err(ComponentError::Allocator)?;
        self.freed = true;
        Ok(())
    }
}

fn allocate_array<A: WebPathAllocator>(
    allocator: &mut A,
    kind: WebArrayKind,
    count: u32,
    stride: usize,
    rejected: &mut Vec<MissionAllocation>,
) -> Result<Option<MissionAllocation>, ComponentError<A::Error>> {
    let length = count.wrapping_mul(stride as u32).wrapping_add(4);
    let Some(mut allocation) = allocator
        .allocate_web_array(kind, count, length)
        .map_err(ComponentError::Allocator)?
    else {
        return Ok(None);
    };
    if allocation.domain != allocator.mission_memory_domain() {
        rejected.push(allocation);
        return Err(ComponentError::ForeignDomain);
    }
    if allocation.identity == 0
        || allocation.identity.checked_add(4).is_none()
        || allocation.bytes.len() != length as usize
    {
        rejected.push(allocation);
        return Err(ComponentError::InvalidAllocation);
    }
    if allocation.bytes.len() < 4 {
        rejected.push(allocation);
        return Err(boundary(WebPathBoundary::AllocationExtent));
    }
    put(&mut allocation.bytes, 0, count);
    // Cookie storage precedes the CRT constructor helper. Wrapped extents and
    // the helper's signed-count edge remain unsupported here; retain the
    // allocation and reached cookie write without widening x86 arithmetic.
    let complete_length = count
        .checked_mul(stride as u32)
        .and_then(|n| n.checked_add(4));
    if complete_length != Some(length) || (count as i32) < 0 {
        rejected.push(allocation);
        return Err(boundary(WebPathBoundary::AllocationExtent));
    }
    for element in allocation.bytes[4..].chunks_exact_mut(stride) {
        match kind {
            WebArrayKind::Connections => {
                // ctor54d2b0 leaves distance's allocation seed untouched.
                element[..4].fill(0);
            }
            WebArrayKind::Nodes => {
                // ctor54d2c0 writes every byte of the packed30-byte node.
                element.fill(0);
                put(element, 0x10, 50);
            }
        }
    }
    Ok(Some(allocation))
}

#[derive(Default)]
pub(crate) struct WebPathGraph {
    nodes: Option<NodeArray>,
    pending_nodes: Option<NodeArray>,
    rejected_allocations: Vec<MissionAllocation>,
}
impl WebPathGraph {
    fn check<E>(&self, path: &[u8]) -> Result<(), ComponentError<E>> {
        if word(path, 4) != self.nodes.as_ref().map_or(0, NodeArray::pointer) {
            return Err(boundary(WebPathBoundary::ForeignStorage));
        }
        Ok(())
    }
    pub(crate) fn resize_connections<A: WebPathAllocator>(
        &mut self,
        path: &[u8],
        index: u32,
        requested: u16,
        allocator: &mut A,
    ) -> Result<(), ComponentError<A::Error>> {
        self.check(path)?;
        if index >= word(path, 0xc) {
            return Err(boundary(WebPathBoundary::NodeOutsideAllocation));
        }
        self.nodes
            .as_mut()
            .ok_or_else(|| boundary(WebPathBoundary::NullNodeDestination))?
            .resize_connections(index as usize, requested, allocator)
    }
    pub(crate) fn destroy_nodes<A: MissionAllocator>(
        &mut self,
        path: &[u8],
        allocator: &mut A,
    ) -> Result<(), ComponentError<A::Error>> {
        self.check(path)?;
        if let Some(nodes) = &mut self.nodes {
            nodes.destroy(allocator)?;
        }
        Ok(())
    }
    pub(crate) fn resize<A: WebPathAllocator>(
        &mut self,
        path: &mut [u8],
        requested: u32,
        allocator: &mut A,
    ) -> Result<(), ComponentError<A::Error>> {
        let mut capacity = word(path, 8);
        if capacity >= requested {
            return Ok(());
        }
        let quantum = word(path, 0x10);
        if quantum == 0 {
            return Err(boundary(WebPathBoundary::CapacityDoesNotAdvance));
        }
        let steps = u64::from(requested - capacity).div_ceil(u64::from(quantum));
        capacity = u32::try_from(u64::from(capacity) + steps * u64::from(quantum))
            .map_err(|_| boundary(WebPathBoundary::CapacityWrap))?;
        // Native capacity changes before allocation or any deep-copy calls.
        put(path, 8, capacity);
        self.pending_nodes = allocate_array(
            allocator,
            WebArrayKind::Nodes,
            capacity,
            NODE_SIZE,
            &mut self.rejected_allocations,
        )?
        .map(|allocation| NodeArray {
            allocation,
            connections: (0..capacity).map(|_| None).collect(),
            pending_connections: Vec::new(),
            rejected_allocations: Vec::new(),
            freed: false,
        });
        self.check(path)?;
        let mut index = 0;
        while index < word(path, 0xc) as usize {
            let old = self
                .nodes
                .as_ref()
                .and_then(|nodes| nodes.node(index))
                .ok_or_else(|| boundary(WebPathBoundary::NodeOutsideAllocation))?;
            let requested = u16::from(old[0x1d]);
            let replacement = self
                .pending_nodes
                .as_mut()
                .ok_or_else(|| boundary(WebPathBoundary::NullNodeDestination))?;
            replacement.resize_connections(index, requested, allocator)?;
            let source = self.nodes.as_ref().unwrap().node(index).unwrap();
            let destination = replacement
                .node_mut(index)
                .ok_or_else(|| boundary(WebPathBoundary::NodeOutsideAllocation))?;
            // Native copy order: xyz, word_e, word_c, active byte, +10,+14.
            destination[..12].copy_from_slice(&source[..12]);
            destination[0xe..0x10].copy_from_slice(&source[0xe..0x10]);
            destination[0xc..0xe].copy_from_slice(&source[0xc..0xe]);
            destination[0x1d] = source[0x1d];
            destination[0x10..0x18].copy_from_slice(&source[0x10..0x18]);
            let length = usize::from(source[0x1d]) * CONNECTION_SIZE;
            if length != 0 {
                let source = self.nodes.as_ref().unwrap().connections[index]
                    .as_ref()
                    .and_then(|array| array.data().get(..length))
                    .ok_or_else(|| boundary(WebPathBoundary::ConnectionOutsideAllocation))?;
                let destination = replacement.connections[index]
                    .as_mut()
                    .ok_or_else(|| boundary(WebPathBoundary::NullConnectionDestination))?
                    .data_mut()
                    .get_mut(..length)
                    .ok_or_else(|| boundary(WebPathBoundary::ConnectionOutsideAllocation))?;
                destination.copy_from_slice(source);
            }
            index += 1;
        }
        self.destroy_nodes(path, allocator)?;
        self.nodes = self.pending_nodes.take();
        put(path, 4, self.nodes.as_ref().map_or(0, NodeArray::pointer));
        Ok(())
    }
    pub(crate) fn load<A: WebPathAllocator, C: WebPathChunk>(
        &mut self,
        path: &mut [u8],
        allocator: &mut A,
        chunk: &mut C,
    ) -> Result<(), WebPathLoadError<A::Error, C::Error>> {
        use WebPathLoadError::Component;
        self.destroy_nodes(path, allocator).map_err(Component)?;
        self.nodes = None;
        put(path, 8, 0);
        put(path, 0xc, 0);
        put(path, 4, 0);
        let mut signature = [0; 4]; // Scratch; unknown bytes are never consumed.
        let initialized = read_chunk(chunk, &mut signature)?;
        if initialized != 4 {
            return Err(WebPathLoadError::UninitializedSignature {
                initialized_prefix: signature[..initialized].to_vec(),
            });
        }
        if u32::from_le_bytes(signature) != 0x01ab_cedf {
            return Ok(());
        }
        let mut count = [0; 4]; // Native LoadNodes explicitly initializes count.
        read_chunk(chunk, &mut count)?;
        let count = u32::from_le_bytes(count);
        self.resize(path, count, allocator).map_err(Component)?;
        put(path, 0xc, count);
        let length = count.wrapping_mul(NODE_SIZE as u32) as usize;
        if begin_read_chunk(chunk, length)? {
            if length == 0 {
                finish_read_chunk(chunk, &mut [])?;
            } else {
                let destination = self
                    .nodes
                    .as_mut()
                    .ok_or_else(|| Component(boundary(WebPathBoundary::NullNodeDestination)))?
                    .allocation
                    .bytes
                    .get_mut(4..4 + length)
                    .ok_or_else(|| Component(boundary(WebPathBoundary::NodeOutsideAllocation)))?;
                finish_read_chunk(chunk, destination)?;
            }
        }
        let mut index = 0;
        while index < word(path, 0xc) as usize {
            let nodes = self
                .nodes
                .as_mut()
                .ok_or_else(|| Component(boundary(WebPathBoundary::NullNodeDestination)))?;
            let node = nodes
                .node_mut(index)
                .ok_or_else(|| Component(boundary(WebPathBoundary::NodeOutsideAllocation)))?;
            let count = node[0x1d];
            // Discard persisted pointers/capacity before constructing ownership.
            node[0x1c] = 0;
            node[0x1d] = 0;
            put(node, 0x18, 0);
            nodes
                .resize_connections(index, u16::from(count), allocator)
                .map_err(Component)?;
            nodes.node_mut(index).unwrap()[0x1d] = count;
            let length = usize::from(nodes.node(index).unwrap()[0x1d]) * CONNECTION_SIZE;
            if begin_read_chunk(chunk, length)? {
                if length == 0 {
                    finish_read_chunk(chunk, &mut [])?;
                } else {
                    let destination = nodes.connections[index]
                        .as_mut()
                        .ok_or_else(|| {
                            Component(boundary(WebPathBoundary::NullConnectionDestination))
                        })?
                        .data_mut()
                        .get_mut(..length)
                        .ok_or_else(|| {
                            Component(boundary(WebPathBoundary::ConnectionOutsideAllocation))
                        })?;
                    finish_read_chunk(chunk, destination)?;
                }
            }
            index += 1;
        }
        Ok(())
    }
    pub(crate) fn view<'a>(
        &'a self,
        path: &'a [u8],
        domain: MissionMemoryDomainId,
    ) -> WebPathView<'a> {
        WebPathView {
            graph: self,
            path,
            domain,
        }
    }
}

fn read_chunk<A, C: WebPathChunk>(
    chunk: &mut C,
    destination: &mut [u8],
) -> Result<usize, WebPathLoadError<A, C::Error>> {
    if !begin_read_chunk(chunk, destination.len())? {
        return Ok(0);
    }
    finish_read_chunk(chunk, destination)
}
fn begin_read_chunk<A, C: WebPathChunk>(
    chunk: &mut C,
    length: usize,
) -> Result<bool, WebPathLoadError<A, C::Error>> {
    if !chunk.is_open() || chunk.is_writing() {
        return Ok(false);
    }
    if let Some(bound) = chunk.active_bound() {
        let position = chunk.seek_current().map_err(WebPathLoadError::Chunk)?;
        let end = chunk.read_bound(bound).map_err(WebPathLoadError::Chunk)?;
        let remaining = end.wrapping_sub(position).max(0);
        if (length as i32) > remaining {
            return Ok(false);
        }
    }
    Ok(true)
}
fn finish_read_chunk<A, C: WebPathChunk>(
    chunk: &mut C,
    destination: &mut [u8],
) -> Result<usize, WebPathLoadError<A, C::Error>> {
    let initialized = chunk
        .read_data(destination)
        .map_err(WebPathLoadError::Chunk)?;
    if initialized > destination.len() {
        return Err(WebPathLoadError::Component(boundary(
            WebPathBoundary::InvalidReadCount,
        )));
    }
    Ok(initialized)
}

pub struct WebPathView<'a> {
    graph: &'a WebPathGraph,
    path: &'a [u8],
    domain: MissionMemoryDomainId,
}
impl<'a> WebPathView<'a> {
    pub fn node_count(&self) -> u32 {
        word(self.path, 0xc)
    }
    pub fn node_capacity(&self) -> u32 {
        word(self.path, 8)
    }
    pub fn memory_domain(&self) -> MissionMemoryDomainId {
        self.domain
    }
    pub fn node(&self, index: usize) -> Option<WebPathNodeView<'a>> {
        if index >= self.node_count() as usize {
            return None;
        }
        let nodes = self.graph.nodes.as_ref()?;
        if nodes.freed {
            return None;
        }
        Some(WebPathNodeView {
            bytes: nodes.node(index)?,
            connections: nodes.connections[index].as_ref(),
        })
    }
}
pub struct WebPathNodeView<'a> {
    bytes: &'a [u8],
    connections: Option<&'a ConnectionArray>,
}
impl WebPathNodeView<'_> {
    pub fn raw_bytes(&self) -> &[u8] {
        self.bytes
    }
    /// Packed navigation flags, without assigning meanings to unknown bits.
    pub fn flags(&self) -> u16 {
        u16::from_le_bytes(self.bytes[0xc..0xe].try_into().unwrap())
    }
    pub fn connection_count(&self) -> u8 {
        self.bytes[0x1d]
    }
    pub fn connection_capacity(&self) -> u8 {
        self.bytes[0x1c]
    }
    pub fn connection(&self, index: usize) -> Option<WebConnection> {
        if index >= usize::from(self.connection_count()) {
            return None;
        }
        let connections = self.connections?;
        if connections.freed {
            return None;
        }
        let bytes = connections.data().get(index * 8..index * 8 + 8)?;
        Some(WebConnection {
            node_index: u16::from_le_bytes(bytes[..2].try_into().unwrap()),
            word_2: u16::from_le_bytes(bytes[2..4].try_into().unwrap()),
            distance_bits: word(bytes, 4),
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebConnection {
    pub node_index: u16,
    pub word_2: u16,
    pub distance_bits: u32,
}

#[cfg(test)]
#[path = "mission_web_path_tests.rs"]
mod tests;

#[path = "mission_web_path_reset.rs"]
mod reset;
