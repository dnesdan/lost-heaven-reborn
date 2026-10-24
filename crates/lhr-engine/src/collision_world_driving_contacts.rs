//! Connects the owned original static query to the wheel-contact backend.
//!
//! Primitive identities stay opaque: property reads use the contact trait's
//! semantic boundary and never turn a host tag into a native memory address.
//! The underlying backend still owns all live memory, frame, filter and impact
//! effects. A filter must understand the same world identities if it uses them;
//! this adapter does not replace an unavailable callback with a successful one.

use crate::{
    collision_query::{
        CollisionLineOutputs, CollisionLineRequest, CollisionQueryError, CollisionTraversalState,
    },
    collision_world_driving::DrivingCollisionWorld,
    vehicle_contacts::{VehicleContactEffect, VehicleContactHit, VehicleContactsBackend},
    vehicle_init::{
        VehicleComponentTailMemory, VehicleInitBackend, VehicleInitEnumeration,
        VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
    },
};

/// Borrow the same backend used by initialization/contact effects. The mapper
/// preserves its existing error type while making static-query failures explicit.
pub struct DrivingWorldContacts<'world, 'backend, B, F> {
    pub world: &'world DrivingCollisionWorld,
    pub backend: &'backend mut B,
    pub traversal: CollisionTraversalState,
    map_query_error: F,
}

impl<'world, 'backend, B, F> DrivingWorldContacts<'world, 'backend, B, F> {
    pub fn new(
        world: &'world DrivingCollisionWorld,
        backend: &'backend mut B,
        map_query_error: F,
    ) -> Self {
        Self {
            world,
            backend,
            traversal: CollisionTraversalState::default(),
            map_query_error,
        }
    }
}

impl<B, F> VehicleInitBackend for DrivingWorldContacts<'_, '_, B, F>
where
    B: VehicleContactsBackend,
    F: FnMut(CollisionQueryError) -> B::Error,
{
    type Error = B::Error;
    fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
        self.backend.read_byte(address)
    }
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
        self.backend.read_word(address)
    }
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
        self.backend.write_byte(address, value)
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
        self.backend.write_word(address, value)
    }
    fn free(&mut self, allocation: u32) -> Result<(), Self::Error> {
        self.backend.free(allocation)
    }
    fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
        self.backend.allocate(bytes)
    }
    fn allocate_projected(
        &mut self,
        memory: &mut VehicleComponentTailMemory<'_>,
        bytes: u32,
    ) -> Result<u32, Self::Error> {
        self.backend.allocate_projected(memory, bytes)
    }
    fn free_projected(
        &mut self,
        memory: &mut VehicleComponentTailMemory<'_>,
        allocation: u32,
    ) -> Result<(), Self::Error> {
        self.backend.free_projected(memory, allocation)
    }
    fn collect_frame(
        &mut self,
        vector: &mut VehicleInitFrameVector,
        frame: u32,
    ) -> Result<(), Self::Error> {
        self.backend.collect_frame(vector, frame)
    }
    fn collect_frame_projected(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        header: u32,
        frame: u32,
    ) -> Result<u32, VehicleInitUnavailable<Self::Error>> {
        self.backend.collect_frame_projected(memory, header, frame)
    }
    fn enumerate(
        &mut self,
        model: u32,
        query: VehicleInitQuery,
        target: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.backend.enumerate(model, query, target)
    }
}

impl<B, F> VehicleContactsBackend for DrivingWorldContacts<'_, '_, B, F>
where
    B: VehicleContactsBackend,
    F: FnMut(CollisionQueryError) -> B::Error,
{
    fn contact_test_line_v(
        &mut self,
        request: CollisionLineRequest,
        outputs: &mut CollisionLineOutputs,
    ) -> Result<Option<VehicleContactHit>, Self::Error> {
        self.world
            .test_line_v(&request, outputs, &mut self.traversal)
            .map(|hit| {
                hit.map(|hit| VehicleContactHit {
                    primitive: hit.primitive,
                })
            })
            .map_err(&mut self.map_query_error)
    }
    fn contact_primitive_properties(&mut self, primitive: u32) -> Result<u32, Self::Error> {
        match self.world.primitive(primitive) {
            Some(info) => Ok(info.properties),
            None => self.backend.contact_primitive_properties(primitive),
        }
    }
    fn contact_primitive_material(&mut self, primitive: u32) -> Result<u8, Self::Error> {
        match self.world.primitive(primitive) {
            Some(info) => Ok(info.material()),
            None => self.backend.contact_primitive_material(primitive),
        }
    }
    fn contact_filter(
        &mut self,
        callback: u32,
        context: u32,
        primitive: u32,
        wheel_index: u32,
    ) -> Result<u32, Self::Error> {
        self.backend
            .contact_filter(callback, context, primitive, wheel_index)
    }
    fn contact_update_frame_world(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.backend.contact_update_frame_world(frame)
    }
    fn contact_frame_direction(&mut self, frame: u32) -> Result<[f32; 3], Self::Error> {
        self.backend.contact_frame_direction(frame)
    }
    fn contact_effect(
        &mut self,
        vehicle: u32,
        effect: VehicleContactEffect<'_>,
    ) -> Result<(), Self::Error> {
        self.backend.contact_effect(vehicle, effect)
    }
}
