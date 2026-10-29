//! C_web_path::DeRiciConns, upstream-attributed PC entry 0x54cd20.
//! Source comparison: mafia-re dc867c3238488de27d094bb79b6550e392566e3c,
//! game/ai/pathweb.cpp and pathweb.h. This is not original-runtime validation.
//! The named operation borrows the existing graph. Packed offsets and inert
//! allocation-identity checks stay in the private storage views below.
#![forbid(unsafe_code)]

use super::{CONNECTION_SIZE, NodeArray, WebPathBoundary, boundary, word};
use crate::mission_components::{ComponentError, MissionComponent, MissionMemoryDomainId};
use std::convert::Infallible;

type Result<T> = std::result::Result<T, ComponentError<Infallible>>;

// Names express the evidenced bit values, not an invented gameplay meaning.
const RESET_FLAGS: u16 = 0x1000 | 0x4000;

impl MissionComponent {
    /// Reset the two evidenced flags in active nodes and their active links.
    /// Visit nodes and links in reverse order, writing each node before its
    /// links. Spare capacity and all other fields remain unchanged.
    ///
    /// This supplies a dependency of C_game::Init, not GameInit or pathfinding.
    /// Invalid reached storage preserves prior writes and interrupts the owner;
    /// it is not repaired, skipped or automatically replayed. No allocator runs.
    pub fn de_rici_conns(&mut self) -> Result<()> {
        self.web_path_leaf()?;
        if !self.constructed {
            return Err(ComponentError::ConstructorIncomplete);
        }
        let result = self.reset_web_connection_flags();
        if result.is_err() {
            self.interrupted = true;
        }
        result
    }

    fn reset_web_connection_flags(&mut self) -> Result<()> {
        let mut path = LivePathFlags { owner: self };
        for index in (0..path.active_node_count()).rev() {
            let mut node = path.node(index)?;
            node.clear_flags(RESET_FLAGS);
            for connection in (0..node.active_connection_count()).rev() {
                node.clear_connection_flags(connection, RESET_FLAGS)?;
            }
        }
        Ok(())
    }
}

// Narrow adapter over already-owned packed records, never decoded snapshots.
struct LivePathFlags<'a> {
    owner: &'a mut MissionComponent,
}
impl LivePathFlags<'_> {
    fn active_node_count(&self) -> usize {
        word(&self.owner.allocation.bytes, 0xc) as usize
    }

    fn node(&mut self, index: usize) -> Result<LiveNodeFlags<'_>> {
        let graph = self
            .owner
            .web_path
            .as_mut()
            .ok_or_else(|| boundary(WebPathBoundary::NullNodeDestination))?;
        // Deliberately reached only for an active node: an empty path must not
        // inspect the unused node pointer, just as the source loop does not.
        graph.check(&self.owner.allocation.bytes)?;
        let nodes = graph
            .nodes
            .as_mut()
            .ok_or_else(|| boundary(WebPathBoundary::NullNodeDestination))?;
        if nodes.freed || nodes.allocation.domain != self.owner.allocation.domain {
            return Err(boundary(WebPathBoundary::ForeignStorage));
        }
        nodes
            .node(index)
            .ok_or_else(|| boundary(WebPathBoundary::NodeOutsideAllocation))?;
        Ok(LiveNodeFlags {
            nodes,
            index,
            domain: self.owner.allocation.domain,
        })
    }
}

struct LiveNodeFlags<'a> {
    nodes: &'a mut NodeArray,
    index: usize,
    domain: MissionMemoryDomainId,
}
impl LiveNodeFlags<'_> {
    fn clear_flags(&mut self, mask: u16) {
        // node() proved this record present; this borrowed view cannot resize it.
        let bytes = self.nodes.node_mut(self.index).unwrap();
        clear_word_flags(&mut bytes[0xc..0xe], mask);
    }

    fn active_connection_count(&self) -> usize {
        usize::from(self.nodes.node(self.index).unwrap()[0x1d])
    }

    fn clear_connection_flags(&mut self, index: usize, mask: u16) -> Result<()> {
        // Validate only when the native loop actually dereferences a link.
        // In particular, preserve the preceding node flag write on failure.
        let expected = word(self.nodes.node(self.index).unwrap(), 0x18);
        let owner = self
            .nodes
            .connections
            .get_mut(self.index)
            .ok_or_else(|| boundary(WebPathBoundary::ConnectionOutsideAllocation))?;
        let pointer = owner.as_ref().map_or(0, super::ConnectionArray::pointer);
        if pointer != expected {
            return Err(boundary(WebPathBoundary::ForeignStorage));
        }
        let connections = owner
            .as_mut()
            .ok_or_else(|| boundary(WebPathBoundary::NullConnectionDestination))?;
        if connections.freed || connections.allocation.domain != self.domain {
            return Err(boundary(WebPathBoundary::ForeignStorage));
        }
        let start = index * CONNECTION_SIZE;
        let record = connections
            .data_mut()
            .get_mut(start..start + CONNECTION_SIZE)
            .ok_or_else(|| boundary(WebPathBoundary::ConnectionOutsideAllocation))?;
        clear_word_flags(&mut record[2..4], mask);
        Ok(())
    }
}

fn clear_word_flags(bytes: &mut [u8], mask: u16) {
    let flags = u16::from_le_bytes([bytes[0], bytes[1]]) & !mask;
    bytes.copy_from_slice(&flags.to_le_bytes());
}

#[cfg(test)]
#[path = "mission_web_path_reset_tests.rs"]
mod tests;
