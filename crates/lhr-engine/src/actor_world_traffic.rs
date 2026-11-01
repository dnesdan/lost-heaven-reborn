//! Actual derived source lifecycle. Only native heap and input boundaries are
//! dependencies; no unavailable Tick/GameInit/spawn behavior is supplied.
use super::*;
use crate::traffic_generator::{
    TrafficDataReader, TrafficPointerArray, TrafficRecord, TrafficRecordSlot, TrafficWeights,
    TrafficWord,
};

fn effect(detail: String) -> ActorWorldError {
    ActorWorldError::Effect {
        stage: "traffic_generator",
        detail,
    }
}
impl ActorWorld {
    pub fn construct_traffic_generator<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: TrafficGeneratorSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0xb7).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        // Base constructor and actual static registration precede every derived
        // store. Until this returns there is no constructed derived tail.
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("owned constructor identity");
        self.actors[i].traffic = Some(TrafficGeneratorState::construct(seed));
        self.actors[i].state.vtable = ActorVtable::TrafficGenerator;
        self.actors[i].state.word_10 = 18;
        Ok(actor)
    }
    pub fn traffic_generator(
        &self,
        actor: ActorReferenceId,
    ) -> Result<&TrafficGeneratorState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .traffic
            .as_ref()
            .ok_or(ActorWorldError::NotTrafficGenerator)
    }
    pub fn inspect_traffic_generator(
        &self,
        actor: ActorReferenceId,
    ) -> Option<&TrafficGeneratorState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .traffic
            .as_ref()
    }
    fn traffic_mut(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<&mut TrafficGeneratorState, ActorWorldError> {
        let i = self.index(actor)?;
        self.actors[i]
            .traffic
            .as_mut()
            .ok_or(ActorWorldError::NotTrafficGenerator)
    }
    fn traffic_count(&self, actor: ActorReferenceId) -> Result<u32, ActorWorldError> {
        self.traffic_generator(actor)?
            .count_bits()
            .ok_or_else(|| effect("unknown traffic count".into()))
    }
    fn traffic_allocate<B: ActorAllocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        kind: TrafficBackingKind,
        bytes: u32,
        backend: &mut B,
    ) -> Result<Option<TrafficAllocationId>, ActorWorldError> {
        let allocation = backend
            .allocate_traffic_backing(self, actor, kind, bytes)
            .map_err(effect)?;
        if let Some(id) = allocation {
            if !id.matches(actor, kind, bytes) || !self.traffic_mut(actor)?.admit_allocation(id) {
                return Err(effect("foreign or aliased traffic allocation owner".into()));
            }
        }
        Ok(allocation)
    }
    fn traffic_free_old_records<B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        // MOV count before the loop; pointer backing is reread at each slot.
        let count = self.traffic_count(actor)?;
        if count > 65_536 {
            return Err(effect("traffic old-count host boundary".into()));
        }
        for index in (0..count as usize).rev() {
            let slot = self
                .traffic_generator(actor)?
                .records
                .as_ref()
                .and_then(|p| p.records.get(index))
                .ok_or_else(|| effect("traffic count/backing mismatch".into()))?;
            let allocation = match slot {
                TrafficRecordSlot::Null => None,
                TrafficRecordSlot::Owned(record) => Some(record.allocation),
                TrafficRecordSlot::Unknown | TrafficRecordSlot::Released(_) => {
                    return Err(effect("unknown or already freed traffic record".into()))
                }
            };
            backend
                .free_traffic_backing(self, actor, TrafficBackingKind::Record, allocation)
                .map_err(effect)?;
            if let Some(allocation) = allocation {
                self.traffic_mut(actor)?.records.as_mut().unwrap().records[index] =
                    TrafficRecordSlot::Released(allocation);
            }
        }
        let allocation = self
            .traffic_generator(actor)?
            .records
            .as_ref()
            .map(|p| p.allocation);
        backend
            .free_traffic_backing(self, actor, TrafficBackingKind::RecordPointers, allocation)
            .map_err(effect)?;
        self.traffic_mut(actor)?.records = None;
        Ok(())
    }
    fn traffic_free_weights<B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let allocation = self
            .traffic_generator(actor)?
            .weights
            .as_ref()
            .map(|p| p.allocation);
        backend
            .free_traffic_backing(self, actor, TrafficBackingKind::Weights, allocation)
            .map_err(effect)?;
        self.traffic_mut(actor)?.weights = None;
        Ok(())
    }
    pub fn load_traffic_generator_data<
        R: TrafficDataReader + ?Sized,
        B: ActorAllocationBackend + ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        input: &mut R,
        backend: &mut B,
    ) -> Result<bool, ActorWorldError> {
        let i = self.ready(actor)?;
        self.traffic_generator(actor)?;
        // Pre-effect latch survives both Err and caught unwind. A repeated
        // invocation cannot replay old frees or overwrite partial publication.
        self.actors[i].phase = ActorPhase::LoadingInterrupted;
        let result = self.load_traffic_inner(actor, input, backend);
        if result.is_ok() {
            self.actors[i].phase = ActorPhase::Ready;
        }
        result
    }
    fn load_traffic_inner<
        R: TrafficDataReader + ?Sized,
        B: ActorAllocationBackend + ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        input: &mut R,
        backend: &mut B,
    ) -> Result<bool, ActorWorldError> {
        let mut version = TrafficWord::unknown();
        input.read_bounded(version.destination()).map_err(effect)?;
        let version = version
            .bits()
            .ok_or_else(|| effect("unknown traffic version at branch".into()))?;
        if version > 1 {
            return Ok(true);
        }
        input
            .read_bounded(&mut self.traffic_mut(actor)?.enabled)
            .map_err(effect)?;
        for index in 0..5 {
            input
                .read_bounded(self.traffic_mut(actor)?.scalar_bits[index].destination())
                .map_err(effect)?;
        }
        if version == 1 {
            input
                .read_bounded(self.traffic_mut(actor)?.total.destination())
                .map_err(effect)?;
        } else {
            self.traffic_mut(actor)?.total = TrafficWord::known(0);
        }
        self.traffic_free_old_records(actor, backend)?;
        input
            .read_bounded(self.traffic_mut(actor)?.count.destination())
            .map_err(effect)?;
        let count = self.traffic_count(actor)?;
        if count != 0 {
            let allocation = self.traffic_allocate(
                actor,
                TrafficBackingKind::RecordPointers,
                count.wrapping_mul(4),
                backend,
            )?;
            if let Some(allocation) = allocation {
                self.traffic_mut(actor)?.records = Some(TrafficPointerArray {
                    allocation,
                    records: Vec::new(),
                });
                // Reached host storage boundary, after native allocation and
                // publication. It does not prevalidate the whole source payload.
                if count > 65_536 {
                    return Err(effect("traffic count host boundary".into()));
                }
                let records = &mut self.traffic_mut(actor)?.records.as_mut().unwrap().records;
                records
                    .try_reserve_exact(count as usize)
                    .map_err(|_| ActorWorldError::HostAllocation)?;
                records.resize_with(count as usize, || TrafficRecordSlot::Unknown);
            }
        }
        let mut index = 0usize;
        while (index as i32) < (self.traffic_count(actor)? as i32) {
            if index >= 65_536 {
                return Err(effect("traffic record loop host boundary".into()));
            }
            let allocation =
                self.traffic_allocate(actor, TrafficBackingKind::Record, 17, backend)?;
            self.traffic_mut(actor)?.pending_record = allocation.map(|allocation| TrafficRecord {
                allocation,
                bytes: [None; 17],
            });
            // malloc occurs before the native pointer-array store, even when
            // the backing is null or its host extent is insufficient.
            let traffic = self.traffic_mut(actor)?;
            let pending = traffic.pending_record.take();
            let Some(slot) = traffic
                .records
                .as_mut()
                .and_then(|p| p.records.get_mut(index))
            else {
                traffic.pending_record = pending;
                return Err(effect("traffic pointer publication outside backing".into()));
            };
            *slot = pending.map_or(TrafficRecordSlot::Null, TrafficRecordSlot::Owned);
            match slot {
                TrafficRecordSlot::Owned(record) => {
                    input.read_bounded(&mut record.bytes).map_err(effect)?;
                }
                TrafficRecordSlot::Null => {
                    input.read_null_bounded(17).map_err(effect)?;
                }
                _ => unreachable!("slot was just published"),
            }
            index += 1;
        }
        self.traffic_free_weights(actor, backend)?;
        let count = self.traffic_count(actor)?;
        if count != 0 {
            let allocation = self.traffic_allocate(
                actor,
                TrafficBackingKind::Weights,
                count.wrapping_mul(4),
                backend,
            )?;
            if let Some(allocation) = allocation {
                self.traffic_mut(actor)?.weights = Some(TrafficWeights {
                    allocation,
                    bytes: Vec::new(),
                });
                if count > 65_536 {
                    return Err(effect("traffic weight host boundary".into()));
                }
                let bytes = &mut self.traffic_mut(actor)?.weights.as_mut().unwrap().bytes;
                bytes
                    .try_reserve_exact(count as usize * 4)
                    .map_err(|_| ActorWorldError::HostAllocation)?;
                bytes.resize(count as usize * 4, None);
                input.read_bounded(bytes).map_err(effect)?;
            } else {
                input
                    .read_null_bounded(count.wrapping_mul(4) as usize)
                    .map_err(effect)?;
            }
            if version == 0 && (self.traffic_count(actor)? as i32) > 0 {
                let count = self.traffic_count(actor)?;
                for index in 0..count as usize {
                    let value = self
                        .traffic_generator(actor)?
                        .weights
                        .as_ref()
                        .and_then(|p| p.word(index))
                        .ok_or_else(|| effect("unknown traffic weight at sum".into()))?;
                    let total = self
                        .traffic_generator(actor)?
                        .total_bits()
                        .ok_or_else(|| effect("unknown traffic total".into()))?;
                    self.traffic_mut(actor)?.total = TrafficWord::known(total.wrapping_add(value));
                }
            }
        }
        Ok(true)
    }
    pub fn destroy_traffic_generator<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        self.traffic_generator(actor)?;
        self.actors[i].phase = ActorPhase::DestructionInterrupted;
        self.actors[i].state.vtable = ActorVtable::TrafficGenerator;
        self.traffic_free_old_records(actor, backend)?;
        self.traffic_free_weights(actor, backend)?;
        self.traffic_mut(actor)?.base_destruction_entered = true;
        self.destroy_base_inner(actor, frames)
    }
    pub fn delete_traffic_generator<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_traffic_generator(actor, frames, backend)?;
        if argument & 1 != 0 {
            backend.free_actor(self, actor.identity()).map_err(effect)?;
        }
        Ok(())
    }
    pub fn release_traffic_generator<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        self.traffic_generator(actor)?;
        let references = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = references;
        if references == 0 {
            self.delete_traffic_generator(actor, 1, frames, backend)?;
        }
        Ok(references)
    }
    /// Explicit terminal host cleanup of interrupted derived intake. Known host
    /// owners are discarded without replaying uncertain native frees. If base
    /// destruction was already entered its partial prefix remains unavailable.
    pub fn discard_traffic_generator_host<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::LoadingInterrupted
                | ActorPhase::InitializationInterrupted
                | ActorPhase::DestructionInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.traffic_generator(actor)?.base_destruction_entered {
            return Err(effect(
                "traffic inherited destruction prefix requires explicit recovery".into(),
            ));
        }
        backend
            .discard_traffic_backings_host(self, actor)
            .map_err(effect)?;
        let traffic = self.traffic_mut(actor)?;
        traffic.records = None;
        traffic.weights = None;
        traffic.pending_record = None;
        traffic.base_destruction_entered = true;
        self.destroy_base_inner(actor, frames)?;
        backend.free_actor(self, actor.identity()).map_err(effect)
    }
}

#[cfg(test)]
#[path = "traffic_generator_tests.rs"]
mod tests;
