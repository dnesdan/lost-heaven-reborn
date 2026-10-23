//! Continuation of the original Init component-discovery prefix.
//!
//! This projection retains external native ownership and never completes Init.
//! Effect methods correspond to actual calls, rather than successful substitutes
//! for inline arithmetic. All arithmetic assumes Game's x87 PC24/RN policy.
use super::*;
use crate::{
    native_matrix::{checked_f32_add, checked_f32_mul},
    native_transform::SMatrix,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleComponentsFrontier {
    /// Original 0x4be5b4: first DOOR record, after native name ordering.
    DoorSetup { index: u32 },
    /// Original 0x4be937: BALLAST discovery after an empty DOOR collection.
    BallastEnumeration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleComponentsOutcome {
    Rejected(VehicleInitRejection),
    UnavailableFrontier(VehicleComponentsFrontier),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VehicleComponentsUnavailable<E> {
    Init(VehicleInitUnavailable<E>),
    /// Only a frontier produced by this very state can be continued, once.
    NoContinuation,
    /// Portable arithmetic cannot represent the required x87 intermediate.
    NumericDomain(&'static str),
}

impl<E> From<VehicleInitUnavailable<E>> for VehicleComponentsUnavailable<E> {
    fn from(value: VehicleInitUnavailable<E>) -> Self {
        Self::Init(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleComponentsQuery {
    BodyCollision,
    Door,
}
impl VehicleComponentsQuery {
    pub fn pattern(self) -> &'static [u8] {
        match self {
            Self::BodyCollision => b"BODYCOL*",
            Self::Door => b"DOOR*",
        }
    }
    pub fn mask(self) -> u32 {
        match self {
            Self::BodyCollision => 0x10020,
            Self::Door => 0x10001,
        }
    }
}

/// Known fields of Init's native BODYCOL callback context. The callback's
/// collection is the vehicle's existing native container at offset 0x4ac.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VehicleBodyCollisionContext {
    pub name_length_with_nul: u32,
    pub model: u32,
}

/// Required effects of actual original callees and frame virtual methods.
///
/// Vehicle storage passed to a call is the same projection as `as_bytes()`.
/// It is not external addressable memory; an implementation must preserve each
/// actual callee's field-write/allocation/reference order and completed failure
/// prefix. Methods have no defaults and do not imply their callee is ported.
/// Normalizing and SetRot3 methods operate at frame+0x9c and frame+0x50,
/// respectively. Frame matrix preparation is the actual Game 0x40de20 call;
/// it is not a substitute for the inline direction normalization below.
///
/// Native enumeration has the same traversal/callback-return requirements as
/// `VehicleInitBackend::enumerate`. BODYCOL callback 0x4d1060 and postpasses
/// 0x4d1490/0x4d17f0 remain explicit recursive-container dependencies. No
/// reference is implicitly acquired/released by constructing Rust values.
pub trait VehicleComponentsBackend: VehicleInitBackend {
    fn prepare_frame_matrix(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn normalize_frame_quaternion(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn set_frame_rotation_matrix(&mut self, frame: u32) -> Result<(), Self::Error>;
    /// Actual 0x42cff0, this=vehicle+0x450; value contains position then direction.
    fn insert_exhaust(
        &mut self,
        vehicle: &mut [u8],
        insertion: u32,
        value: [u32; 6],
    ) -> Result<(), Self::Error>;
    /// Actual 0x60efd0, this=vehicle+0x1fb0.
    fn insert_special_point(
        &mut self,
        vehicle: &mut [u8],
        insertion: u32,
        point: [u32; 3],
    ) -> Result<(), Self::Error>;
    /// Actual 0x548040, this=vehicle+0x1ff8. Only the first word and
    /// low byte of the second word are initialized in this stack record;
    /// the three remaining bytes have no projected value or fidelity claim.
    fn insert_special_control(
        &mut self,
        vehicle: &mut [u8],
        insertion: u32,
        kind: u32,
        enabled: u8,
    ) -> Result<(), Self::Error>;
    /// Actual optional indirect call vehicle+0x31c, ECX=vehicle+0xc74 value,
    /// EDX=frame, stack arguments=(control_pointer,index).
    fn special_collision_callback(
        &mut self,
        function: u32,
        context: u32,
        frame: u32,
        control_pointer: u32,
        index: u32,
    ) -> Result<(), Self::Error>;
    /// Frame virtual +0x2c, with the original parent and flags arguments.
    fn link_frame(&mut self, frame: u32, parent: u32, flags: u32) -> Result<(), Self::Error>;
    /// Model virtual +0x5c; preserves its actual ownership/release behavior.
    fn remove_model_child(&mut self, model: u32, frame: u32) -> Result<(), Self::Error>;
    fn enumerate_components(
        &mut self,
        model: u32,
        query: VehicleComponentsQuery,
        target: &mut VehicleComponentsEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>>;
    /// Actual callback 0x4d1060, including its raw callback return value.
    fn body_collision_callback(
        &mut self,
        vehicle: &mut [u8],
        context: VehicleBodyCollisionContext,
        frame: u32,
    ) -> Result<u32, Self::Error>;
    /// Actual recursive postpass 0x4d1490 using the same native context.
    fn finish_body_collision_callbacks(
        &mut self,
        vehicle: &mut [u8],
        context: VehicleBodyCollisionContext,
    ) -> Result<(), Self::Error>;
    /// Actual LS3DF UpdateWMatrixProc thunk 0x60fc30.
    fn update_model_world_matrix(&mut self, model: u32) -> Result<(), Self::Error>;
    /// Actual 0x4d17f0, ECX=vehicle+0x4ac, EDX=vehicle+0xd28;
    /// stack arguments=(model,name_length_with_nul,0).
    fn initialize_body_collision(
        &mut self,
        vehicle: &mut [u8],
        model: u32,
        name_length_with_nul: u32,
    ) -> Result<(), Self::Error>;
    /// Actual CRT decimal conversion 0x610bf5 at the supplied name suffix.
    fn parse_component_number(&mut self, address: u32) -> Result<i32, Self::Error>;
}

pub struct VehicleComponentsEnumeration<'a> {
    target: ComponentEnumerationTarget<'a>,
}
enum ComponentEnumerationTarget<'a> {
    BodyCollision(&'a mut [u8], VehicleBodyCollisionContext),
    Collect(&'a mut VehicleInitFrameVector),
}
impl VehicleComponentsEnumeration<'_> {
    pub fn visit<B: VehicleComponentsBackend + ?Sized>(
        &mut self,
        frame: u32,
        backend: &mut B,
    ) -> Result<u32, VehicleInitUnavailable<B::Error>> {
        match &mut self.target {
            ComponentEnumerationTarget::BodyCollision(vehicle, context) => backend
                .body_collision_callback(vehicle, *context, frame)
                .map_err(VehicleInitUnavailable::Effect),
            ComponentEnumerationTarget::Collect(vector) => {
                backend
                    .collect_frame(vector, frame)
                    .map_err(VehicleInitUnavailable::Effect)?;
                Ok(0)
            }
        }
    }
}

pub(super) fn read_words<B: VehicleInitBackend, const N: usize>(
    backend: &mut B,
    address: u32,
) -> Result<[u32; N], VehicleComponentsUnavailable<B::Error>> {
    let mut out = [0; N];
    for (index, value) in out.iter_mut().enumerate() {
        *value = backend
            .read_word(address.wrapping_add(index as u32 * 4))
            .map_err(VehicleInitUnavailable::Effect)?;
    }
    Ok(out)
}
pub(super) fn matrix(words: [u32; 16]) -> SMatrix {
    SMatrix {
        elements: words.map(f32::from_bits),
    }
}
#[derive(Clone, Copy)]
pub(super) enum MatrixSource {
    Local(SMatrix),
    External(u32),
}
#[derive(Clone, Copy)]
pub(super) enum PointSource {
    Local([u32; 3]),
    External(u32),
}
// The existing checked matrix primitive has the same LS3DF export and
// association. Read operands here in its instruction order; HEAD and SPECCOLL
// do not copy a complete matrix and never read its homogeneous column.
pub(super) fn transform_sources<B: VehicleInitBackend>(
    backend: &mut B,
    matrix: MatrixSource,
    point: PointSource,
) -> Result<[u32; 3], VehicleComponentsUnavailable<B::Error>> {
    fn word<B: VehicleInitBackend>(
        backend: &mut B,
        address: u32,
    ) -> Result<f32, VehicleComponentsUnavailable<B::Error>> {
        backend
            .read_word(address)
            .map(f32::from_bits)
            .map_err(VehicleInitUnavailable::Effect)
            .map_err(Into::into)
    }
    let vector = |backend: &mut B, index: u32| match point {
        PointSource::Local(values) => Ok(f32::from_bits(values[index as usize])),
        PointSource::External(address) => word(backend, address.wrapping_add(index * 4)),
    };
    let element = |backend: &mut B, index: u32| match matrix {
        MatrixSource::Local(value) => Ok(value.elements[index as usize]),
        MatrixSource::External(address) => word(backend, address.wrapping_add(index * 4)),
    };
    let fail = |_| VehicleComponentsUnavailable::NumericDomain("component point transform");
    let mut result = [0; 3];
    for column in 0..3 {
        let x = vector(backend, 0)?;
        let first = checked_f32_mul(x, element(backend, column)?).map_err(fail)?;
        let translated = checked_f32_add(first, element(backend, 12 + column)?).map_err(fail)?;
        let y = vector(backend, 1)?;
        let second = checked_f32_mul(y, element(backend, 4 + column)?).map_err(fail)?;
        let accumulated = checked_f32_add(translated, second).map_err(fail)?;
        let z = vector(backend, 2)?;
        let third = checked_f32_mul(z, element(backend, 8 + column)?).map_err(fail)?;
        result[column as usize] = checked_f32_add(accumulated, third).map_err(fail)?.to_bits();
    }
    Ok(result)
}

// Original inline (Y*Y + Z*Z) + X*X, then PC24 subtraction before
// comparison with the exact widened f32 constant stored as a double.
pub(super) fn normalize_direction<E>(
    words: [u32; 3],
) -> Result<[u32; 3], VehicleComponentsUnavailable<E>> {
    let fail = || VehicleComponentsUnavailable::NumericDomain("component direction normalization");
    let [x, y, z] = words.map(f32::from_bits);
    let squared = checked_f32_add(
        checked_f32_add(
            checked_f32_mul(y, y).map_err(|_| fail())?,
            checked_f32_mul(z, z).map_err(|_| fail())?,
        )
        .map_err(|_| fail())?,
        checked_f32_mul(x, x).map_err(|_| fail())?,
    )
    .map_err(|_| fail())?;
    let delta = checked_f32_add(squared, -1.).map_err(|_| fail())?;
    if f64::from(delta.abs()) < f64::from(1e-8_f32) {
        return Ok(words);
    }
    let mut vector = [x, y, z];
    if squared < 1e-8_f32 {
        let axis = if x != 0. {
            0
        } else if z != 0. {
            2
        } else {
            1
        };
        vector[axis] = if vector[axis] < 0. { -1. } else { 1. };
    } else {
        let length = squared.sqrt();
        if !length.is_normal() {
            return Err(fail());
        }
        let reciprocal = 1. / length;
        let exact = 1. / f64::from(length);
        if !reciprocal.is_normal()
            || exact.abs() < f64::from(f32::MIN_POSITIVE)
            || exact.abs() > f64::from(f32::MAX)
        {
            return Err(fail());
        }
        for value in &mut vector {
            *value = checked_f32_mul(*value, reciprocal).map_err(|_| fail())?;
        }
    }
    Ok(vector.map(f32::to_bits))
}

impl VehicleInitState {
    // The opt-in tail has an explicit stack mapping. Mirror reached prefix
    // stores that overlap its record sources; legacy callers have no mapping.
    fn component_stack_words(&mut self, offset: usize, values: &[u32]) {
        if let Some(scratch) = self.tail_scratch.as_mut() {
            for (index, value) in values.iter().enumerate() {
                let start = offset + index * 4;
                scratch.bytes[start..start + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
    }
    /// Executes the established prefix and its component continuation. It never
    /// reports successful Init, and retains the same rejection/ownership rules.
    pub fn init_components<B: VehicleComponentsBackend>(
        &mut self,
        backend: &mut B,
        model: u32,
    ) -> Result<VehicleComponentsOutcome, VehicleComponentsUnavailable<B::Error>> {
        match self.init_prefix(backend, model)? {
            VehicleInitOutcome::Rejected(reason) => Ok(VehicleComponentsOutcome::Rejected(reason)),
            VehicleInitOutcome::UnavailableFrontier(_) => self.continue_components(backend),
        }
    }

    /// Continues only an actual frontier returned by this state's `init_prefix`.
    /// The capability is consumed before any operation. An unavailable call
    /// retains completed effects and cannot be replayed over native ownership.
    /// This is not a general facility for jumping into arbitrary Init state.
    pub fn continue_components<B: VehicleComponentsBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<VehicleComponentsOutcome, VehicleComponentsUnavailable<B::Error>> {
        use VehicleInitUnavailable::Effect;
        let frontier = self
            .continuation
            .take()
            .ok_or(VehicleComponentsUnavailable::NoContinuation)?;
        match frontier {
            VehicleInitFrontier::ExhaustTransform { index } => {
                let mut index = index;
                loop {
                    let body = self.load(0xe68)?;
                    backend.prepare_frame_matrix(body).map_err(Effect)?;
                    let mut body_matrix = matrix(read_words(backend, body.wrapping_add(0x50))?);
                    let frame = self.scratch_frame(backend, index)?;
                    let position = transform_sources(
                        backend,
                        MatrixSource::Local(body_matrix),
                        PointSource::External(frame.wrapping_add(0x80)),
                    )?;
                    // Native zeros copied translation before transforming a direction.
                    body_matrix.elements[12..15].fill(0.);
                    let frame = self.scratch_frame(backend, index)?;
                    let flags = backend
                        .read_word(frame.wrapping_add(0xac))
                        .map_err(Effect)?;
                    let direction = if flags & 0x10 == 0 {
                        backend
                            .write_word(frame.wrapping_add(0xac), flags | 0x10)
                            .map_err(Effect)?;
                        backend.normalize_frame_quaternion(frame).map_err(Effect)?;
                        backend.set_frame_rotation_matrix(frame).map_err(Effect)?;
                        read_words(backend, frame.wrapping_add(0x70))?
                    } else {
                        normalize_direction(read_words(backend, frame.wrapping_add(0x70))?)?
                    };
                    let direction = transform_sources(
                        backend,
                        MatrixSource::Local(body_matrix),
                        PointSource::Local(direction),
                    )?;
                    self.component_stack_words(
                        0x8c,
                        &[
                            position[0],
                            position[1],
                            position[2],
                            direction[0],
                            direction[1],
                            direction[2],
                        ],
                    );
                    let insertion = self.load(0x458)?;
                    backend
                        .insert_exhaust(
                            &mut self.bytes,
                            insertion,
                            [
                                position[0],
                                position[1],
                                position[2],
                                direction[0],
                                direction[1],
                                direction[2],
                            ],
                        )
                        .map_err(Effect)?;
                    self.detach_scratch_frame(backend, index)?;
                    if index == 0 {
                        break;
                    }
                    index = index.wrapping_sub(1);
                }
                self.scratch.as_mut().unwrap().end = self.scratch.unwrap().begin;
                self.enumerate_special_collision(backend)?;
                self.special_collision_components(backend)?;
                self.head_component(backend, true)?;
                self.scan_component_name(backend)?;
            }
            VehicleInitFrontier::SpecialCollisionTransform => {
                self.special_collision_components(backend)?;
                self.head_component(backend, true)?;
                self.scan_component_name(backend)?;
            }
            VehicleInitFrontier::HeadTransform => {
                self.head_component(backend, false)?;
                self.scan_component_name(backend)?;
            }
            VehicleInitFrontier::BodyCollisionEnumeration => {}
        }
        let model = self.load(0x34c)?;
        let context = VehicleBodyCollisionContext {
            name_length_with_nul: self.name_length,
            model,
        };
        backend.enumerate_components(
            model,
            VehicleComponentsQuery::BodyCollision,
            &mut VehicleComponentsEnumeration {
                target: ComponentEnumerationTarget::BodyCollision(&mut self.bytes, context),
            },
        )?;
        backend
            .finish_body_collision_callbacks(&mut self.bytes, context)
            .map_err(Effect)?;
        let model = self.load(0x34c)?;
        if backend
            .read_byte(model.wrapping_add(0xac))
            .map_err(Effect)?
            & 0x20
            == 0
        {
            backend.update_model_world_matrix(model).map_err(Effect)?;
        }
        // Original REP MOVSD copies the model matrix into the vehicle before
        // the inverse dependency, so unsupported arithmetic retains that copy.
        for index in 0..16 {
            let value = backend
                .read_word(model.wrapping_add(0x10 + index * 4))
                .map_err(Effect)?;
            self.store(0xd28 + index * 4, value)?;
        }
        let mut words = [0; 16];
        for (index, value) in words.iter_mut().enumerate() {
            *value = self.load(0xd28 + index as u32 * 4)?;
        }
        let (inverse, _) = matrix(words)
            .inverse()
            .map_err(|_| VehicleComponentsUnavailable::NumericDomain("model matrix inverse"))?;
        for (index, value) in inverse.elements.into_iter().enumerate() {
            self.store(0xd28 + index as u32 * 4, value.to_bits())?;
        }
        self.scan_component_name(backend)?;
        let model = self.load(0x34c)?;
        backend
            .initialize_body_collision(&mut self.bytes, model, self.name_length)
            .map_err(Effect)?;
        self.component_scratch = Some(VehicleInitFrameVector {
            begin: 0,
            end: 0,
            capacity: 0,
        });
        let model = self.load(0x34c)?;
        backend.enumerate_components(
            model,
            VehicleComponentsQuery::Door,
            &mut VehicleComponentsEnumeration {
                target: ComponentEnumerationTarget::Collect(
                    self.component_scratch.as_mut().unwrap(),
                ),
            },
        )?;
        self.scan_component_name(backend)?;
        let suffix_offset = self.name_length.wrapping_sub(1).wrapping_add(5);
        let mut index = 0;
        while index < self.component_count() {
            if self.component_number(backend, index, suffix_offset)? != index as i32 {
                let mut next = index.wrapping_add(1);
                while next < self.component_count() {
                    if self.component_number(backend, next, suffix_offset)? == index as i32 {
                        let vector = self.component_scratch.unwrap();
                        let current_address = vector.begin.wrapping_add(index.wrapping_mul(4));
                        let next_address = vector.begin.wrapping_add(next.wrapping_mul(4));
                        let current = backend.read_word(current_address).map_err(Effect)?;
                        let other = backend.read_word(next_address).map_err(Effect)?;
                        backend.write_word(next_address, current).map_err(Effect)?;
                        // The native code reloads the vector base after the first store.
                        let current_address = self
                            .component_scratch
                            .unwrap()
                            .begin
                            .wrapping_add(index.wrapping_mul(4));
                        backend.write_word(current_address, other).map_err(Effect)?;
                        break;
                    }
                    next = next.wrapping_add(1);
                }
            }
            index = index.wrapping_add(1);
        }
        let frontier = if self.component_count() != 0 {
            VehicleComponentsFrontier::DoorSetup { index: 0 }
        } else {
            VehicleComponentsFrontier::BallastEnumeration
        };
        self.component_tail = Some(frontier);
        Ok(VehicleComponentsOutcome::UnavailableFrontier(frontier))
    }

    pub fn component_scratch(&self) -> Option<VehicleInitFrameVector> {
        self.component_scratch
    }

    fn component_count(&self) -> u32 {
        let vector = self.component_scratch.unwrap();
        if vector.begin == 0 {
            0
        } else {
            ((vector.end.wrapping_sub(vector.begin) as i32) >> 2) as u32
        }
    }

    fn component_number<B: VehicleComponentsBackend>(
        &mut self,
        backend: &mut B,
        index: u32,
        suffix_offset: u32,
    ) -> Result<i32, VehicleComponentsUnavailable<B::Error>> {
        use VehicleInitUnavailable::Effect;
        let frame = backend
            .read_word(
                self.component_scratch
                    .unwrap()
                    .begin
                    .wrapping_add(index.wrapping_mul(4)),
            )
            .map_err(Effect)?;
        let name = backend
            .read_word(frame.wrapping_add(0x100))
            .map_err(Effect)?;
        backend
            .parse_component_number(
                (if name == 0 { 0x63d2a0 } else { name }).wrapping_add(suffix_offset),
            )
            .map_err(Effect)
            .map_err(Into::into)
    }

    fn scratch_frame<B: VehicleInitBackend>(
        &self,
        backend: &mut B,
        index: u32,
    ) -> Result<u32, VehicleComponentsUnavailable<B::Error>> {
        backend
            .read_word(
                self.scratch
                    .unwrap()
                    .begin
                    .wrapping_add(index.wrapping_mul(4)),
            )
            .map_err(VehicleInitUnavailable::Effect)
            .map_err(Into::into)
    }
    fn detach_scratch_frame<B: VehicleComponentsBackend>(
        &mut self,
        backend: &mut B,
        index: u32,
    ) -> Result<(), VehicleComponentsUnavailable<B::Error>> {
        let frame = self.scratch_frame(backend, index)?;
        backend
            .link_frame(frame, 0, 0)
            .map_err(VehicleInitUnavailable::Effect)?;
        let model = self.load(0x34c)?;
        let frame = self.scratch_frame(backend, index)?;
        backend
            .remove_model_child(model, frame)
            .map_err(VehicleInitUnavailable::Effect)?;
        Ok(())
    }
    fn enumerate_special_collision<B: VehicleComponentsBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), VehicleComponentsUnavailable<B::Error>> {
        backend.enumerate(
            self.load(0x34c)?,
            VehicleInitQuery::SpecialCollision,
            &mut VehicleInitEnumeration {
                target: EnumerationTarget::Collect(self.scratch.as_mut().unwrap()),
            },
        )?;
        Ok(())
    }
    fn special_collision_components<B: VehicleComponentsBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), VehicleComponentsUnavailable<B::Error>> {
        use VehicleInitUnavailable::Effect;
        let mut index = 0;
        loop {
            let vector = self.scratch.unwrap();
            let count = ((vector.end.wrapping_sub(vector.begin) as i32) >> 2) as u32;
            if vector.begin == 0 || index >= count {
                break;
            }
            let frame = self.scratch_frame(backend, index)?;
            let mut bounds = [0; 6];
            for (index, value) in bounds.iter_mut().enumerate() {
                *value = backend
                    .read_word(frame.wrapping_add(0x140 + index as u32 * 4))
                    .map_err(Effect)?;
                self.component_stack_words(0x8c + index * 4, &[*value]);
            }
            let frame = self.scratch_frame(backend, index)?;
            backend.prepare_frame_matrix(frame).map_err(Effect)?;
            let first = transform_sources(
                backend,
                MatrixSource::External(frame.wrapping_add(0x50)),
                PointSource::Local([bounds[0], bounds[1], bounds[2]]),
            )?;
            let insertion = self.load(0x1fb8)?;
            backend
                .insert_special_point(&mut self.bytes, insertion, first)
                .map_err(Effect)?;
            let frame = self.scratch_frame(backend, index)?;
            backend.prepare_frame_matrix(frame).map_err(Effect)?;
            let second = transform_sources(
                backend,
                MatrixSource::External(frame.wrapping_add(0x50)),
                PointSource::Local([bounds[3], bounds[4], bounds[5]]),
            )?;
            let insertion = self.load(0x1fb8)?;
            backend
                .insert_special_point(&mut self.bytes, insertion, second)
                .map_err(Effect)?;
            let insertion = self.load(0x2000)?;
            backend
                .insert_special_control(&mut self.bytes, insertion, 0x40, 1)
                .map_err(Effect)?;
            let function = self.load(0x31c)?;
            if function != 0 {
                let control_pointer = self.load::<B::Error>(0x2000)?.wrapping_sub(8);
                let frame = self.scratch_frame(backend, index)?;
                let context = self.load(0xc74)?;
                backend
                    .special_collision_callback(function, context, frame, control_pointer, index)
                    .map_err(Effect)?;
            }
            self.detach_scratch_frame(backend, index)?;
            index = index.wrapping_add(1);
        }
        Ok(())
    }
    fn head_component<B: VehicleComponentsBackend>(
        &mut self,
        backend: &mut B,
        enumerate: bool,
    ) -> Result<(), VehicleComponentsUnavailable<B::Error>> {
        use VehicleInitUnavailable::Effect;
        if enumerate {
            self.head = 0;
            backend.enumerate(
                self.load(0x34c)?,
                VehicleInitQuery::Head,
                &mut VehicleInitEnumeration {
                    target: EnumerationTarget::Head(&mut self.head),
                },
            )?;
        }
        if self.head == 0 {
            for offset in [0x2d4, 0x2d8, 0x2dc] {
                self.store(offset, 0)?;
            }
        } else {
            let body = self.load(0xe68)?;
            backend.prepare_frame_matrix(body).map_err(Effect)?;
            let point = transform_sources(
                backend,
                MatrixSource::External(body.wrapping_add(0x50)),
                PointSource::External(self.head.wrapping_add(0x80)),
            )?;
            for (index, value) in point.into_iter().enumerate() {
                self.store(0x2d4 + index as u32 * 4, value)?;
            }
            backend.link_frame(self.head, 0, 0).map_err(Effect)?;
            let model = self.load(0x34c)?;
            backend
                .remove_model_child(model, self.head)
                .map_err(Effect)?;
            self.head = 0;
        }
        Ok(())
    }
    fn scan_component_name<B: VehicleInitBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), VehicleComponentsUnavailable<B::Error>> {
        let mut name = self.load(0x4bc)?;
        let mut remaining = u32::MAX;
        while remaining != 0 {
            let byte = backend
                .read_byte(name)
                .map_err(VehicleInitUnavailable::Effect)?;
            name = name.wrapping_add(1);
            remaining -= 1;
            if byte == 0 {
                break;
            }
        }
        self.name_length = !remaining;
        Ok(())
    }
}

/// Opt-in production composition of the three actual BODYCOL functions with
/// the existing Init continuation. Other native dependencies retain their
/// explicit backend contracts. The backend must provide disjoint, addressable
/// callback stack scratch described by `storage`.
pub struct VehicleBodycolComponents<'a, B: ?Sized> {
    backend: &'a mut B,
    storage: crate::vehicle_bodycol::VehicleBodycolStorage,
    exhaust_scratch: Option<&'a mut VehicleComponentTailScratch>,
}
impl<'a, B: ?Sized> VehicleBodycolComponents<'a, B> {
    pub fn new(backend: &'a mut B, storage: crate::vehicle_bodycol::VehicleBodycolStorage) -> Self {
        Self {
            backend,
            storage,
            exhaust_scratch: None,
        }
    }
    pub(super) fn with_exhaust_records(
        backend: &'a mut B,
        storage: crate::vehicle_bodycol::VehicleBodycolStorage,
        scratch: &'a mut VehicleComponentTailScratch,
    ) -> Self {
        Self {
            backend,
            storage,
            exhaust_scratch: Some(scratch),
        }
    }
}
fn bodycol_init_error<E>(
    error: VehicleInitUnavailable<E>,
) -> VehicleInitUnavailable<crate::vehicle_bodycol::VehicleBodycolUnavailable<E>> {
    match error {
        VehicleInitUnavailable::Storage(address) => VehicleInitUnavailable::Storage(address),
        VehicleInitUnavailable::Effect(error) => VehicleInitUnavailable::Effect(
            crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect(error),
        ),
        VehicleInitUnavailable::AlreadyStarted => VehicleInitUnavailable::AlreadyStarted,
    }
}
impl<B: VehicleComponentsBackend + crate::vehicle_bodycol::VehicleBodycolBackend> VehicleInitBackend
    for VehicleBodycolComponents<'_, B>
{
    type Error = crate::vehicle_bodycol::VehicleBodycolUnavailable<B::Error>;
    fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
        self.backend
            .read_byte(address)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
        self.backend
            .read_word(address)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
        self.backend
            .write_byte(address, value)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
        self.backend
            .write_word(address, value)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
        self.backend
            .allocate(bytes)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn free(&mut self, allocation: u32) -> Result<(), Self::Error> {
        self.backend
            .free(allocation)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn collect_frame(
        &mut self,
        vector: &mut VehicleInitFrameVector,
        frame: u32,
    ) -> Result<(), Self::Error> {
        self.backend
            .collect_frame(vector, frame)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn enumerate(
        &mut self,
        model: u32,
        query: VehicleInitQuery,
        target: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.backend
            .enumerate(model, query, target)
            .map_err(bodycol_init_error)
    }
}
impl<B: VehicleComponentsBackend + crate::vehicle_bodycol::VehicleBodycolBackend>
    VehicleComponentsBackend for VehicleBodycolComponents<'_, B>
{
    fn prepare_frame_matrix(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.backend
            .prepare_frame_matrix(frame)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn normalize_frame_quaternion(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.backend
            .normalize_frame_quaternion(frame)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn set_frame_rotation_matrix(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.backend
            .set_frame_rotation_matrix(frame)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn insert_exhaust(
        &mut self,
        vehicle: &mut [u8],
        insertion: u32,
        value: [u32; 6],
    ) -> Result<(), Self::Error> {
        if let Some(scratch) = self.exhaust_scratch.as_deref_mut() {
            return super::component_tail::insert_exhaust_projection(
                self.backend,
                vehicle,
                self.storage.vehicle_address,
                scratch,
                insertion,
                value,
            )
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect);
        }
        self.backend
            .insert_exhaust(vehicle, insertion, value)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn insert_special_point(
        &mut self,
        vehicle: &mut [u8],
        insertion: u32,
        point: [u32; 3],
    ) -> Result<(), Self::Error> {
        self.backend
            .insert_special_point(vehicle, insertion, point)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn insert_special_control(
        &mut self,
        vehicle: &mut [u8],
        insertion: u32,
        kind: u32,
        enabled: u8,
    ) -> Result<(), Self::Error> {
        self.backend
            .insert_special_control(vehicle, insertion, kind, enabled)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn special_collision_callback(
        &mut self,
        function: u32,
        context: u32,
        frame: u32,
        control_pointer: u32,
        index: u32,
    ) -> Result<(), Self::Error> {
        self.backend
            .special_collision_callback(function, context, frame, control_pointer, index)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn link_frame(&mut self, frame: u32, parent: u32, flags: u32) -> Result<(), Self::Error> {
        self.backend
            .link_frame(frame, parent, flags)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn remove_model_child(&mut self, model: u32, frame: u32) -> Result<(), Self::Error> {
        self.backend
            .remove_model_child(model, frame)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn enumerate_components(
        &mut self,
        model: u32,
        query: VehicleComponentsQuery,
        target: &mut VehicleComponentsEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        if let ComponentEnumerationTarget::BodyCollision(vehicle, context) = &mut target.target {
            let mut actual = crate::vehicle_bodycol::VehicleBodycolEnumeration::components(
                vehicle,
                self.storage,
                *context,
            );
            self.backend
                .bodycol_enumerate(model, query.pattern(), query.mask(), &mut actual)
                .map_err(VehicleInitUnavailable::Effect)
        } else {
            self.backend
                .enumerate_components(model, query, target)
                .map_err(bodycol_init_error)
        }
    }
    fn body_collision_callback(
        &mut self,
        vehicle: &mut [u8],
        context: VehicleBodyCollisionContext,
        frame: u32,
    ) -> Result<u32, Self::Error> {
        let mut projection = crate::vehicle_bodycol::VehicleBodycolProjection::new(
            self.backend,
            vehicle,
            self.storage,
        )?;
        crate::vehicle_bodycol::bodycol_callback(
            &mut projection,
            self.storage.vehicle_address.wrapping_add(0x4ac),
            context,
            frame,
            self.storage.callback_scratch,
        )
    }
    fn finish_body_collision_callbacks(
        &mut self,
        vehicle: &mut [u8],
        context: VehicleBodyCollisionContext,
    ) -> Result<(), Self::Error> {
        let mut projection = crate::vehicle_bodycol::VehicleBodycolProjection::new(
            self.backend,
            vehicle,
            self.storage,
        )?;
        crate::vehicle_bodycol::bodycol_descendant_pass(
            &mut projection,
            self.storage.vehicle_address.wrapping_add(0x4ac),
            context.name_length_with_nul,
            self.storage.descendant_scratch,
        )
    }
    fn update_model_world_matrix(&mut self, model: u32) -> Result<(), Self::Error> {
        self.backend
            .update_model_world_matrix(model)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
    fn initialize_body_collision(
        &mut self,
        vehicle: &mut [u8],
        model: u32,
        name_length_with_nul: u32,
    ) -> Result<(), Self::Error> {
        let mut projection = crate::vehicle_bodycol::VehicleBodycolProjection::new(
            self.backend,
            vehicle,
            self.storage,
        )?;
        crate::vehicle_bodycol::bodycol_transform_detach(
            &mut projection,
            self.storage.vehicle_address.wrapping_add(0x4ac),
            self.storage.vehicle_address.wrapping_add(0xd28),
            model,
            name_length_with_nul,
            false,
        )
        .map(|_| ())
    }
    fn parse_component_number(&mut self, address: u32) -> Result<i32, Self::Error> {
        self.backend
            .parse_component_number(address)
            .map_err(crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    const MODEL: u32 = 0x1000;
    const BODY: u32 = 0x2000;
    const EXHAUST_A: u32 = 0x3000;
    const EXHAUST_B: u32 = 0x4000;
    const SPECIAL: u32 = 0x5000;
    const HEAD: u32 = 0x6000;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        ReadByte(u32),
        ReadWord(u32),
        WriteByte(u32, u8),
        WriteWord(u32, u32),
        Allocate(u32),
        Free(u32),
        Enumerate(VehicleInitQuery),
        EnumerateComponents(VehicleComponentsQuery),
        Collect(u32),
        Prepare(u32),
        Normalize(u32),
        SetRotation(u32),
        Exhaust([u32; 6]),
        Point([u32; 3]),
        Control(u32, u8),
        Callback(u32, u32, u32, u32, u32),
        Link(u32, u32, u32),
        Remove(u32, u32),
        BodyCollision(u32),
        FinishBody,
        World(u32),
        InitializeBody(u32, u32),
        Parse(u32),
        BodycolEnumerate(u32),
        TailEnumerate(u32, VehicleComponentTailQuery),
        Materialize(u32),
        Window(u32),
        SeatCallback(u32, u32, u32),
    }
    #[derive(Clone)]
    struct Backend {
        memory: BTreeMap<u32, u8>,
        events: Vec<Event>,
        fail: Option<usize>,
        components: bool,
        doors: bool,
        next_vector: u32,
        callback_returns: Vec<u32>,
        actual_bodycol: bool,
        allocation_cursor: Option<u32>,
        tail_empty_seats: bool,
        tail_mutation: Option<(u32, u32)>,
        tail_alias_writes: Vec<(u32, u32, u32)>,
        tail_link_failure: Option<u32>,
    }
    fn put_state(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    impl Backend {
        fn new(components: bool, doors: bool) -> Self {
            let mut result = Self {
                memory: BTreeMap::new(),
                events: vec![],
                fail: None,
                components,
                doors,
                next_vector: 0x10000,
                callback_returns: vec![],
                actual_bodycol: false,
                allocation_cursor: None,
                tail_empty_seats: false,
                tail_mutation: None,
                tail_alias_writes: vec![],
                tail_link_failure: None,
            };
            result.put(MODEL + 4, 1);
            result.put(MODEL + 0x110, 9);
            result.put(MODEL + 0x100, 0x11000);
            result.text(0x11000, b"synthetic\0");
            result.put(MODEL + 0xac, 0);
            let mut world = SMatrix::identity();
            world.elements[0] = 2.;
            world.elements[12] = 6.;
            result.put_matrix(MODEL + 0x10, world);
            let mut body = SMatrix::identity();
            body.elements[0] = 2.;
            body.elements[5] = 3.;
            body.elements[10] = 4.;
            body.elements[12..15].copy_from_slice(&[10., 20., 30.]);
            result.put_matrix(BODY + 0x50, body);
            for frame in [EXHAUST_A, EXHAUST_B, HEAD] {
                for (index, value) in [1f32, 2., 3.].into_iter().enumerate() {
                    result.put(frame + 0x80 + index as u32 * 4, value.to_bits());
                }
            }
            result.put(EXHAUST_A + 0xac, 0x10);
            for (index, value) in [0f32, 0., 2.].into_iter().enumerate() {
                result.put(EXHAUST_A + 0x70 + index as u32 * 4, value.to_bits());
            }
            result.put(EXHAUST_B + 0xac, 0);
            let mut special = SMatrix::identity();
            special.elements[12] = 100.;
            result.put_matrix(SPECIAL + 0x50, special);
            for (index, value) in [1f32, 2., 3., 4., 5., 6.].into_iter().enumerate() {
                result.put(SPECIAL + 0x140 + index as u32 * 4, value.to_bits());
            }
            for index in 0..3 {
                let frame = 0x7000 + index * 0x100;
                let name = 0x12000 + index * 0x100;
                result.put(frame + 0x100, name);
                result.text(name, format!("synthetic.DOOR{index}\0").as_bytes());
            }
            result
        }
        fn event(&mut self, event: Event) -> Result<(), &'static str> {
            let index = self.events.len();
            self.events.push(event);
            if self.fail == Some(index) {
                Err("unavailable")
            } else {
                Ok(())
            }
        }
        fn text(&mut self, address: u32, bytes: &[u8]) {
            for (index, byte) in bytes.iter().enumerate() {
                self.memory.insert(address + index as u32, *byte);
            }
        }
        fn put(&mut self, address: u32, word: u32) {
            self.text(address, &word.to_le_bytes());
        }
        fn put_matrix(&mut self, address: u32, value: SMatrix) {
            for (index, value) in value.elements.into_iter().enumerate() {
                self.put(address + index as u32 * 4, value.to_bits());
            }
        }
        fn word(&self, address: u32) -> u32 {
            u32::from_le_bytes(std::array::from_fn(|index| {
                self.memory[&(address + index as u32)]
            }))
        }
    }
    impl VehicleInitBackend for Backend {
        type Error = &'static str;
        fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
            self.event(Event::ReadByte(address))?;
            self.memory.get(&address).copied().ok_or("missing byte")
        }
        fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
            self.event(Event::ReadWord(address))?;
            let mut out = [0; 4];
            for (index, byte) in out.iter_mut().enumerate() {
                *byte = *self
                    .memory
                    .get(&address.wrapping_add(index as u32))
                    .ok_or("missing word")?;
            }
            Ok(u32::from_le_bytes(out))
        }
        fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
            self.event(Event::WriteByte(address, value))?;
            self.memory.insert(address, value);
            Ok(())
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
            self.event(Event::WriteWord(address, value))?;
            self.put(address, value);
            Ok(())
        }
        fn free(&mut self, allocation: u32) -> Result<(), Self::Error> {
            self.event(Event::Free(allocation))
        }
        fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
            self.event(Event::Allocate(bytes))?;
            if let Some(cursor) = &mut self.allocation_cursor {
                let result = *cursor;
                *cursor += bytes.max(0x100).next_multiple_of(0x100);
                Ok(result)
            } else {
                Ok(0x13000)
            }
        }
        fn collect_frame(
            &mut self,
            vector: &mut VehicleInitFrameVector,
            frame: u32,
        ) -> Result<(), Self::Error> {
            self.event(Event::Collect(frame))?;
            if vector.begin == 0 {
                vector.begin = self.next_vector;
                vector.end = vector.begin;
                vector.capacity = vector.begin + 0x100;
                self.next_vector += 0x100;
            }
            self.put(vector.end, frame);
            vector.end += 4;
            Ok(())
        }
        fn enumerate(
            &mut self,
            model: u32,
            query: VehicleInitQuery,
            target: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            assert_eq!(model, MODEL);
            self.event(Event::Enumerate(query))
                .map_err(VehicleInitUnavailable::Effect)?;
            let frames = match query {
                VehicleInitQuery::Body => vec![BODY],
                VehicleInitQuery::Exhaust if self.components => vec![EXHAUST_A, EXHAUST_B],
                VehicleInitQuery::SpecialCollision if self.components => vec![SPECIAL],
                VehicleInitQuery::Head if self.components => vec![HEAD],
                _ => vec![],
            };
            for frame in frames {
                let result = target.visit(frame, self)?;
                self.callback_returns.push(result);
            }
            Ok(())
        }
    }
    impl VehicleComponentsBackend for Backend {
        fn prepare_frame_matrix(&mut self, frame: u32) -> Result<(), Self::Error> {
            self.event(Event::Prepare(frame))
        }
        fn normalize_frame_quaternion(&mut self, frame: u32) -> Result<(), Self::Error> {
            self.event(Event::Normalize(frame))
        }
        fn set_frame_rotation_matrix(&mut self, frame: u32) -> Result<(), Self::Error> {
            self.event(Event::SetRotation(frame))?;
            for (index, value) in [1f32, 0., 0.].into_iter().enumerate() {
                self.put(frame + 0x70 + index as u32 * 4, value.to_bits());
            }
            Ok(())
        }
        fn insert_exhaust(
            &mut self,
            _vehicle: &mut [u8],
            _insertion: u32,
            value: [u32; 6],
        ) -> Result<(), Self::Error> {
            self.event(Event::Exhaust(value))
        }
        fn insert_special_point(
            &mut self,
            _vehicle: &mut [u8],
            _insertion: u32,
            value: [u32; 3],
        ) -> Result<(), Self::Error> {
            self.event(Event::Point(value))
        }
        fn insert_special_control(
            &mut self,
            vehicle: &mut [u8],
            _insertion: u32,
            kind: u32,
            enabled: u8,
        ) -> Result<(), Self::Error> {
            self.event(Event::Control(kind, enabled))?;
            put_state(vehicle, 0x2000, 0x14008);
            Ok(())
        }
        fn special_collision_callback(
            &mut self,
            function: u32,
            context: u32,
            frame: u32,
            pointer: u32,
            index: u32,
        ) -> Result<(), Self::Error> {
            self.event(Event::Callback(function, context, frame, pointer, index))
        }
        fn link_frame(&mut self, frame: u32, parent: u32, flags: u32) -> Result<(), Self::Error> {
            self.event(Event::Link(frame, parent, flags))?;
            if self.tail_link_failure == Some(frame) {
                Err("unavailable link")
            } else {
                Ok(())
            }
        }
        fn remove_model_child(&mut self, model: u32, frame: u32) -> Result<(), Self::Error> {
            self.event(Event::Remove(model, frame))
        }
        fn enumerate_components(
            &mut self,
            model: u32,
            query: VehicleComponentsQuery,
            target: &mut VehicleComponentsEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            assert_eq!(model, MODEL);
            self.event(Event::EnumerateComponents(query))
                .map_err(VehicleInitUnavailable::Effect)?;
            let frames = match query {
                VehicleComponentsQuery::BodyCollision => vec![0x7400],
                VehicleComponentsQuery::Door if self.doors => vec![0x7200, 0x7000, 0x7100],
                _ => vec![],
            };
            for frame in frames {
                let result = target.visit(frame, self)?;
                self.callback_returns.push(result);
            }
            Ok(())
        }
        fn body_collision_callback(
            &mut self,
            vehicle: &mut [u8],
            context: VehicleBodyCollisionContext,
            frame: u32,
        ) -> Result<u32, Self::Error> {
            assert_eq!(
                context,
                VehicleBodyCollisionContext {
                    model: MODEL,
                    name_length_with_nul: 10
                }
            );
            assert!(!self.actual_bodycol, "legacy BODYCOL callback reached");
            self.event(Event::BodyCollision(frame))?;
            put_state(vehicle, 0x4b0, 0x15000);
            Ok(0x55)
        }
        fn finish_body_collision_callbacks(
            &mut self,
            _vehicle: &mut [u8],
            _context: VehicleBodyCollisionContext,
        ) -> Result<(), Self::Error> {
            assert!(!self.actual_bodycol, "legacy BODYCOL postpass reached");
            self.event(Event::FinishBody)
        }
        fn update_model_world_matrix(&mut self, model: u32) -> Result<(), Self::Error> {
            self.event(Event::World(model))
        }
        fn initialize_body_collision(
            &mut self,
            vehicle: &mut [u8],
            model: u32,
            length: u32,
        ) -> Result<(), Self::Error> {
            assert!(!self.actual_bodycol, "legacy BODYCOL transform reached");
            self.event(Event::InitializeBody(model, length))?;
            assert_eq!(
                u32::from_le_bytes(vehicle[0xd28..0xd2c].try_into().unwrap()),
                0.5f32.to_bits()
            );
            assert_eq!(
                u32::from_le_bytes(vehicle[0xd58..0xd5c].try_into().unwrap()),
                (-3f32).to_bits()
            );
            Ok(())
        }
        fn parse_component_number(&mut self, address: u32) -> Result<i32, Self::Error> {
            self.event(Event::Parse(address))?;
            Ok(i32::from(self.memory[&address] - b'0'))
        }
    }
    impl crate::vehicle_bodycol::VehicleBodycolBackend for Backend {
        fn bodycol_update_world(&mut self, frame: u32) -> Result<(), Self::Error> {
            self.event(Event::World(frame))?;
            let flags = self.word(frame + 0xac);
            self.put(frame + 0xac, flags | 0x20);
            Ok(())
        }
        fn bodycol_link_frame(
            &mut self,
            frame: u32,
            parent: u32,
            flags: u32,
        ) -> Result<(), Self::Error> {
            self.link_frame(frame, parent, flags)
        }
        fn bodycol_remove_model_child(
            &mut self,
            model: u32,
            frame: u32,
        ) -> Result<(), Self::Error> {
            self.remove_model_child(model, frame)
        }
        fn bodycol_enumerate(
            &mut self,
            frame: u32,
            pattern: &[u8],
            mask: u32,
            target: &mut crate::vehicle_bodycol::VehicleBodycolEnumeration<'_>,
        ) -> Result<(), crate::vehicle_bodycol::VehicleBodycolUnavailable<Self::Error>> {
            use crate::vehicle_bodycol::VehicleBodycolUnavailable::Effect;
            assert!(self.actual_bodycol);
            assert_eq!(mask, 0x10020);
            self.event(Event::BodycolEnumerate(frame)).map_err(Effect)?;
            let frames: &[u32] = match frame {
                MODEL => {
                    assert_eq!(pattern, b"BODYCOL*");
                    &[0x16000, 0x18000]
                }
                0x16000 => {
                    assert_eq!(pattern, b"*");
                    &[0x18000]
                }
                0x18000 => {
                    assert_eq!(pattern, b"*");
                    &[0x1a000]
                }
                _ => panic!("unexpected BODYCOL enumeration"),
            };
            for frame in frames {
                let value = target.visit(*frame, self)?;
                assert_eq!(value, 0);
            }
            Ok(())
        }
    }
    impl VehicleComponentTailBackend for Backend {
        fn materialize_frame_quaternion(&mut self, frame: u32) -> Result<(), Self::Error> {
            self.event(Event::Materialize(frame))?;
            for (i, value) in [0f32, 0., 0., 1.].into_iter().enumerate() {
                self.put(frame + 0x9c + i as u32 * 4, value.to_bits());
            }
            Ok(())
        }
        fn find_window(&mut self, door: u32) -> Result<u32, Self::Error> {
            self.event(Event::Window(door))?;
            Ok(if door == 0x7000 { 0xa000 } else { 0 })
        }
        fn enumerate_tail(
            &mut self,
            root: u32,
            query: VehicleComponentTailQuery,
            target: &mut VehicleComponentTailEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            self.event(Event::TailEnumerate(root, query))
                .map_err(VehicleInitUnavailable::Effect)?;
            let frames: &[u32] = match query {
                VehicleComponentTailQuery::Ballast => {
                    assert_eq!(root, BODY);
                    &[0x8200, 0x8000, 0x8100]
                }
                VehicleComponentTailQuery::Seat if !self.tail_empty_seats => {
                    assert_eq!(root, MODEL);
                    &[0x9200, 0x9000, 0x9100]
                }
                VehicleComponentTailQuery::Seat => &[],
            };
            for frame in frames {
                assert_eq!(target.visit(*frame, self)?, 0);
            }
            Ok(())
        }
        fn seat_callback(
            &mut self,
            mut memory: VehicleComponentTailMemory<'_>,
            function: u32,
            context: u32,
            frame: u32,
        ) -> Result<(), Self::Error> {
            let x = if frame == 0x9000 {
                12f32
            } else if frame == 0x9100 {
                14f32
            } else {
                16f32
            };
            assert_eq!(memory.read_word(0x29080), Some(x.to_bits()));
            assert_eq!(memory.read_word(0x29084), Some(26f32.to_bits()));
            assert_eq!(memory.read_word(0x29088), Some(42f32.to_bits()));
            if frame != 0x9000 {
                assert_eq!(memory.read_word(0x29018), Some((-1f32).to_bits()));
                assert_eq!(memory.read_word(0x29020), Some(4f32.to_bits()));
                assert_eq!(memory.read_word(0x29030), Some((-1f32).to_bits()));
                assert_eq!(memory.read_word(0x29138), Some((-4f32).to_bits()));
            }
            if let Some((offset, value)) = self.tail_mutation {
                memory.write_word(0x100000 + offset, value).unwrap();
            }
            for (trigger, address, value) in &self.tail_alias_writes {
                if *trigger == frame {
                    memory.write_word(*address, *value).unwrap();
                }
            }
            self.event(Event::SeatCallback(function, context, frame))
        }
    }
    fn tail_storage() -> VehicleComponentTailStorage {
        VehicleComponentTailStorage {
            bodycol: actual_bodycol_storage(),
            scratch: VehicleComponentTailScratch {
                address: 0x29000,
                bytes: [0xa5; 0x13c],
            },
        }
    }
    fn tail_backend() -> Backend {
        let mut b = actual_bodycol_backend();
        b.doors = true;
        for (base, kind) in [(0x7000, "DOOR"), (0x8000, "BALLAST"), (0x9000, "SEAT")] {
            for i in 0..3 {
                let frame = base + i * 0x100;
                b.put(frame + 0x120, BODY);
                b.put(frame + 0xac, if i == 0 { 0 } else { 0x18 });
                b.put(frame + 0xc, 0xb000);
                let name = frame + 0x40000;
                b.put(frame + 0x100, name);
                // Duplicate BALLAST numbers exercise the native insertion/rewrite.
                let number = if kind == "BALLAST" && i == 1 { 0 } else { i };
                b.text(name, format!("synthetic.{kind}{number}\0").as_bytes());
                for (j, value) in [1. + i as f32, 2., 3.].into_iter().enumerate() {
                    b.put(frame + 0x80 + j as u32 * 4, value.to_bits());
                }
                for (j, value) in [0f32, 0., -2.].into_iter().enumerate() {
                    b.put(frame + 0x70 + j as u32 * 4, value.to_bits());
                }
                for (j, value) in [0f32, 0., 0., 1.].into_iter().enumerate() {
                    b.put(frame + 0x9c + j as u32 * 4, value.to_bits());
                }
            }
        }
        b.text(0xb000, b"[I]\0");
        for (i, value) in [4f32, 5., 6.].into_iter().enumerate() {
            b.put(0xa080 + i as u32 * 4, value.to_bits());
        }
        b
    }
    #[test]
    fn component_tail_composes_actual_bodycol_records_and_live_callback() {
        let mut backend = tail_backend();
        backend.tail_mutation = Some((0x304, 0));
        let mut state = state();
        state.store::<()>(0x304, 0x1234).unwrap();
        assert_eq!(
            state.init_component_tail(&mut backend, MODEL, tail_storage()),
            Ok(VehicleComponentTailOutcome::UnavailableFrontier(
                VehicleComponentTailFrontier::WheelEnumeration
            ))
        );
        let doors = state.load::<()>(0xcb4).unwrap();
        let seats = state.load::<()>(0xca4).unwrap();
        assert_eq!(state.load::<()>(0xcb8).unwrap() - doors, 3 * 88);
        assert_eq!(state.load::<()>(0xca8).unwrap() - seats, 3 * 36);
        assert_eq!(backend.word(doors), 0x7000);
        assert_eq!(backend.word(doors + 20), 12f32.to_bits());
        assert_eq!(backend.word(doors + 52), 0xa000);
        assert_eq!(backend.word(doors + 76), 4.5f32.to_bits());
        assert_eq!(backend.word(doors + 88 + 52), 0);
        for i in 0..3 {
            assert_eq!(backend.word(seats + i * 36 + 32), doors + i * 88);
        }
        let ballast = state.load::<()>(0xcc4).unwrap();
        assert_eq!(backend.word(ballast), 16f32.to_bits());
        assert_eq!(backend.word(ballast + 28), 12f32.to_bits());
        assert_eq!(backend.word(ballast + 56), 14f32.to_bits());
        assert_eq!(state.load::<()>(0x304), Ok(0));
        assert_eq!(
            backend
                .events
                .iter()
                .filter(|e| matches!(e, Event::SeatCallback(..)))
                .count(),
            1
        );
        assert!(backend.events.contains(&Event::Materialize(0x7000)));
        assert!(!backend.events.iter().any(|e| matches!(
            e,
            Event::BodyCollision(_) | Event::FinishBody | Event::InitializeBody(..)
        )));
        let before = backend.events.len();
        assert_eq!(
            state.continue_component_tail(&mut backend, tail_storage()),
            Err(VehicleComponentTailUnavailable::NoContinuation)
        );
        assert_eq!(backend.events.len(), before);
        let retained_scratch = state.component_tail_scratch().cloned();
        let retained_vehicle = state.as_bytes().to_vec();
        let mut retry_storage = tail_storage();
        retry_storage.scratch.bytes.fill(0x77);
        assert_eq!(
            state.init_component_tail(&mut backend, MODEL, retry_storage),
            Err(VehicleComponentTailUnavailable::Prefix(
                VehicleComponentsUnavailable::Init(VehicleInitUnavailable::AlreadyStarted)
            ))
        );
        assert_eq!(state.component_tail_scratch(), retained_scratch.as_ref());
        assert_eq!(state.as_bytes(), retained_vehicle);
        assert_eq!(backend.events.len(), before);
    }
    #[test]
    fn component_tail_connects_actual_exhaust_records_before_bodycol() {
        let mut backend = tail_backend();
        backend.components = true;
        let mut state = state();
        assert_eq!(
            state.init_component_tail(&mut backend, MODEL, tail_storage()),
            Ok(VehicleComponentTailOutcome::UnavailableFrontier(
                VehicleComponentTailFrontier::WheelEnumeration
            ))
        );
        let begin = state.load::<()>(0x454).unwrap();
        assert_eq!(state.load::<()>(0x458).unwrap() - begin, 48);
        assert_eq!(backend.word(begin), 12f32.to_bits());
        assert_eq!(backend.word(begin + 12), 2f32.to_bits());
        assert!(!backend
            .events
            .iter()
            .any(|e| matches!(e, Event::Exhaust(_))));
        let door_enumeration = backend
            .events
            .iter()
            .position(|e| *e == Event::EnumerateComponents(VehicleComponentsQuery::Door))
            .unwrap();
        let door_prepare = backend
            .events
            .iter()
            .enumerate()
            .skip(door_enumeration)
            .find(|(_, e)| **e == Event::Prepare(BODY))
            .unwrap()
            .0;
        let bounds_read = backend
            .events
            .iter()
            .position(|e| *e == Event::ReadWord(SPECIAL + 0x148))
            .unwrap();
        for (fail, expected) in [
            (door_prepare, [1f32, 2., 3., 4., 5., 6.]),
            (bounds_read, [1f32, 2., 42., 0., 0., 4.]),
        ] {
            let mut backend = tail_backend();
            backend.components = true;
            backend.fail = Some(fail);
            let mut stopped = VehicleInitState::from_bytes(vec![0; 0x2008]);
            stopped.store::<()>(0x31c, 0x12345678).unwrap();
            stopped.store::<()>(0xc74, 0xabcdef).unwrap();
            assert!(stopped
                .init_component_tail(&mut backend, MODEL, tail_storage())
                .is_err());
            let scratch = &stopped.component_tail_scratch().unwrap().bytes;
            for (index, value) in expected.into_iter().enumerate() {
                let offset = 0x8c + index * 4;
                assert_eq!(
                    u32::from_le_bytes(scratch[offset..offset + 4].try_into().unwrap()),
                    value.to_bits()
                );
            }
        }
        let allocation = backend
            .events
            .iter()
            .position(|e| *e == Event::Allocate(24))
            .unwrap();
        let mut backend = tail_backend();
        backend.components = true;
        backend.fail = Some(allocation);
        let mut failed = VehicleInitState::from_bytes(vec![0; 0x2008]);
        assert!(failed
            .init_component_tail(&mut backend, MODEL, tail_storage())
            .is_err());
        assert_eq!(failed.load::<()>(0x454), Ok(0));
        assert!(failed.component_tail_scratch().is_some());
        assert!(!backend
            .events
            .iter()
            .any(|e| matches!(e, Event::BodycolEnumerate(_))));
    }
    #[test]
    fn component_tail_preserves_each_unavailable_effect_prefix_and_rejects_retry() {
        let mut baseline = tail_backend();
        let mut initial = state();
        initial.store::<()>(0x304, 0x1234).unwrap();
        initial
            .init_components(
                &mut VehicleBodycolComponents::new(&mut baseline, actual_bodycol_storage()),
                MODEL,
            )
            .unwrap();
        let prefix = baseline.events.len();
        let start = baseline.clone();
        initial
            .continue_component_tail(&mut baseline, tail_storage())
            .unwrap();
        for fail in prefix..baseline.events.len() {
            let mut backend = start.clone();
            backend.fail = Some(fail);
            let mut state = state();
            state.store::<()>(0x304, 0x1234).unwrap();
            let mut prefix_backend = tail_backend();
            state
                .init_components(
                    &mut VehicleBodycolComponents::new(
                        &mut prefix_backend,
                        actual_bodycol_storage(),
                    ),
                    MODEL,
                )
                .unwrap();
            assert!(
                state
                    .continue_component_tail(&mut backend, tail_storage())
                    .is_err(),
                "event {fail}"
            );
            assert_eq!(backend.events, &baseline.events[..=fail], "event {fail}");
            let stopped = state.as_bytes().to_vec();
            let events = backend.events.len();
            assert_eq!(
                state.continue_component_tail(&mut backend, tail_storage()),
                Err(VehicleComponentTailUnavailable::NoContinuation)
            );
            assert_eq!(state.as_bytes(), stopped);
            assert_eq!(backend.events.len(), events);
        }
    }
    #[test]
    fn component_tail_requires_real_capability_and_consumes_invalid_overlay() {
        let mut backend = tail_backend();
        let mut state = state();
        assert_eq!(
            state.continue_component_tail(&mut backend, tail_storage()),
            Err(VehicleComponentTailUnavailable::NoContinuation)
        );
        assert!(backend.events.is_empty());
        state
            .init_components(
                &mut VehicleBodycolComponents::new(&mut backend, actual_bodycol_storage()),
                MODEL,
            )
            .unwrap();
        let before = backend.events.len();
        let mut storage = tail_storage();
        storage.scratch.address = storage.bodycol.vehicle_address + 0x100;
        assert_eq!(
            state.continue_component_tail(&mut backend, storage),
            Err(VehicleComponentTailUnavailable::ProjectionStorage)
        );
        assert_eq!(backend.events.len(), before);
        assert_eq!(
            state.continue_component_tail(&mut backend, tail_storage()),
            Err(VehicleComponentTailUnavailable::NoContinuation)
        );
        assert_eq!(backend.events.len(), before);
    }
    #[test]
    fn component_tail_callback_mutation_survives_unavailability() {
        let mut baseline = tail_backend();
        let mut state0 = state();
        state0.store::<()>(0x304, 0x1234).unwrap();
        state0
            .init_component_tail(&mut baseline, MODEL, tail_storage())
            .unwrap();
        let fail = baseline
            .events
            .iter()
            .position(|e| matches!(e, Event::SeatCallback(..)))
            .unwrap();
        let mut backend = tail_backend();
        backend.fail = Some(fail);
        backend.tail_mutation = Some((0x34c, 0xfeed));
        let mut state = state();
        state.store::<()>(0x304, 0x1234).unwrap();
        assert!(state
            .init_component_tail(&mut backend, MODEL, tail_storage())
            .is_err());
        assert_eq!(state.load::<()>(0x34c), Ok(0xfeed));
        assert_ne!(state.load::<()>(0xca4), Ok(0));
        assert!(!backend.events.contains(&Event::Link(0x9000, 0, 0)));
    }
    #[test]
    fn component_tail_callback_vector_aliases_read_live_vehicle_and_stack_before_failure() {
        for target in [0x10034c, 0x290f0] {
            let mut backend = tail_backend();
            backend.put(target, 0); // Deliberately stale external backing.
            backend.tail_link_failure = Some(MODEL);
            backend.tail_alias_writes = vec![(0x9000, 0x2904c, target)];
            if target == 0x290f0 {
                backend.tail_alias_writes.push((0x9000, target, MODEL));
            }
            let mut state = state();
            state.store::<()>(0x304, 0x1234).unwrap();
            assert_eq!(
                state.init_component_tail(&mut backend, MODEL, tail_storage()),
                Err(VehicleComponentTailUnavailable::Tail(
                    VehicleComponentsUnavailable::Init(VehicleInitUnavailable::Effect(
                        "unavailable link"
                    ))
                ))
            );
            assert_eq!(backend.events.last(), Some(&Event::Link(MODEL, 0, 0)));
            assert!(!backend.events.contains(&Event::ReadWord(target)));
            assert!(!backend.events.contains(&Event::Remove(MODEL, MODEL)));
            assert_eq!(backend.word(target), 0);
            let vehicle = state.as_bytes().to_vec();
            let scratch = state.component_tail_scratch().cloned();
            let events = backend.events.clone();
            assert_eq!(
                state.continue_component_tail(&mut backend, tail_storage()),
                Err(VehicleComponentTailUnavailable::NoContinuation)
            );
            assert_eq!(state.as_bytes(), vehicle);
            assert_eq!(state.component_tail_scratch(), scratch.as_ref());
            assert_eq!(backend.events, events);
        }
    }
    #[test]
    fn component_tail_pairing_writes_live_aliased_vehicle_and_stack_records() {
        for destination in [0x100100, 0x29100] {
            let mut backend = tail_backend();
            backend.put(destination + 32, 0xdeadbeef);
            backend.tail_alias_writes = vec![
                (0x9200, 0x100ca4, destination),
                (0x9200, 0x100ca8, destination + 36),
            ];
            let mut state = state();
            state.store::<()>(0x304, 0x1234).unwrap();
            assert_eq!(
                state.init_component_tail(&mut backend, MODEL, tail_storage()),
                Ok(VehicleComponentTailOutcome::UnavailableFrontier(
                    VehicleComponentTailFrontier::WheelEnumeration
                ))
            );
            let doors = state.load::<()>(0xcb4).unwrap();
            let actual = if destination == 0x100100 {
                state.load::<()>(0x120).unwrap()
            } else {
                u32::from_le_bytes(
                    state.component_tail_scratch().unwrap().bytes[0x120..0x124]
                        .try_into()
                        .unwrap(),
                )
            };
            assert_eq!(actual, doors);
            assert_eq!(backend.word(destination + 32), 0xdeadbeef);
            assert!(!backend.events.iter().any(
                |event| matches!(event,Event::WriteWord(address,_) if *address==destination+32)
            ));
        }
    }
    #[test]
    fn component_tail_missing_seat_frees_native_vectors_and_numeric_failure_is_bounded() {
        let mut backend = tail_backend();
        backend.tail_empty_seats = true;
        let mut state = state();
        assert_eq!(
            state.init_component_tail(&mut backend, MODEL, tail_storage()),
            Ok(VehicleComponentTailOutcome::Rejected(
                VehicleInitRejection::MissingSeat
            ))
        );
        let frees = backend
            .events
            .iter()
            .rev()
            .filter_map(|e| {
                if let Event::Free(a) = e {
                    Some(*a)
                } else {
                    None
                }
            })
            .take(4)
            .collect::<Vec<_>>();
        assert_eq!(frees, [0, 0x10000, 0x10100, 0]);
        assert_eq!(state.scratch().unwrap().begin, 0);
        assert_eq!(state.component_scratch().unwrap().begin, 0);
        let mut backend = tail_backend();
        backend.put(0xa084, f32::INFINITY.to_bits());
        let mut failed = VehicleInitState::from_bytes(vec![0; 0x2008]);
        assert_eq!(
            failed.init_component_tail(&mut backend, MODEL, tail_storage()),
            Err(VehicleComponentTailUnavailable::Tail(
                VehicleComponentsUnavailable::NumericDomain("window position subtraction")
            ))
        );
        assert_eq!(failed.load::<()>(0xcb4), Ok(0));
        assert!(backend.events.contains(&Event::Window(0x7000)));
    }
    fn actual_bodycol_backend() -> Backend {
        let mut backend = Backend::new(false, false);
        backend.actual_bodycol = true;
        backend.allocation_cursor = Some(0x30000);
        for (frame, parent, text, position, translation) in [
            (
                0x16000,
                MODEL,
                b"synthetic.BODYCOL0\0".as_slice(),
                [1., 2., 3.],
                4.,
            ),
            (
                0x18000,
                0x16000,
                b"synthetic.BODYCOL1\0".as_slice(),
                [2., 4., 6.],
                10.,
            ),
            (
                0x1a000,
                0x18000,
                b"synthetic.SOLID0\0".as_slice(),
                [1., 2., 3.],
                0.,
            ),
        ] {
            backend.put(frame + 0x120, parent);
            backend.put(frame + 0x100, frame + 0x1000);
            backend.put(frame + 0xac, 0x20);
            backend.text(frame + 0x1000, text);
            let mut matrix = SMatrix::identity();
            matrix.elements[12] = translation;
            backend.put_matrix(frame + 0x10, matrix);
            for (index, number) in position.into_iter().enumerate() {
                backend.put(frame + 0x80 + index as u32 * 4, f32::to_bits(number));
            }
            for (index, number) in [-1f32, -2., -3., 1., 2., 3.].into_iter().enumerate() {
                backend.put(frame + 0x140 + index as u32 * 4, number.to_bits());
            }
        }
        backend
    }
    fn actual_bodycol_storage() -> crate::vehicle_bodycol::VehicleBodycolStorage {
        crate::vehicle_bodycol::VehicleBodycolStorage {
            vehicle_address: 0x100000,
            callback_scratch: crate::vehicle_bodycol::VehicleBodycolScratch {
                record: 0x28000,
                leading_byte: 0xa5,
                nested_byte: 0x5a,
            },
            descendant_scratch: 0x28100,
        }
    }
    #[test]
    fn actual_bodycol_composes_with_init_and_nested_descendant_ownership() {
        let mut backend = actual_bodycol_backend();
        let mut state = state();
        let mut adapter = VehicleBodycolComponents::new(&mut backend, actual_bodycol_storage());
        assert_eq!(
            state.init_components(&mut adapter, MODEL),
            Ok(VehicleComponentsOutcome::UnavailableFrontier(
                VehicleComponentsFrontier::BallastEnumeration
            ))
        );
        let top = state.load::<()>(0x4b0).unwrap();
        assert_eq!(backend.word(top + 0x20), 0);
        let nested = backend.word(top + 0x14);
        assert_eq!(backend.word(nested + 0x20), 0);
        let descendant = backend.word(nested + 4);
        assert_eq!(backend.word(descendant), 0);
        assert_eq!(f32::from_bits(backend.word(descendant + 4)), 2.5);
        let removed: Vec<_> = backend
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Remove(MODEL, frame) => Some(*frame),
                _ => None,
            })
            .collect();
        assert_eq!(removed, [0x1a000, 0x18000, 0x16000]);
    }
    #[test]
    fn actual_bodycol_effect_failures_preserve_live_projection_and_consumed_frontier() {
        let mut reference = actual_bodycol_backend();
        let mut complete = state();
        complete
            .init_components(
                &mut VehicleBodycolComponents::new(&mut reference, actual_bodycol_storage()),
                MODEL,
            )
            .unwrap();
        for (failure, event) in reference.events.iter().enumerate() {
            if !matches!(event, Event::Remove(MODEL, 0x1a000 | 0x18000 | 0x16000)) {
                continue;
            }
            let mut backend = actual_bodycol_backend();
            backend.fail = Some(failure);
            let mut state = state();
            assert!(state
                .init_components(
                    &mut VehicleBodycolComponents::new(&mut backend, actual_bodycol_storage()),
                    MODEL
                )
                .is_err());
            let top = state.load::<()>(0x4b0).unwrap();
            assert_ne!(top, 0, "live header stores survive failure");
            assert_eq!(backend.events, reference.events[..=failure]);
            let nested = backend.word(top + 0x14);
            let retained = if matches!(event, Event::Remove(MODEL, 0x1a000)) {
                backend.word(backend.word(nested + 4))
            } else if matches!(event, Event::Remove(MODEL, 0x18000)) {
                backend.word(nested + 0x20)
            } else {
                backend.word(top + 0x20)
            };
            assert_ne!(
                retained, 0,
                "slot is nulled only after model removal succeeds"
            );
            assert_eq!(
                state.continue_components(&mut VehicleBodycolComponents::new(
                    &mut backend,
                    actual_bodycol_storage()
                )),
                Err(VehicleComponentsUnavailable::NoContinuation)
            );
        }
    }
    fn state() -> VehicleInitState {
        let mut result = VehicleInitState::from_bytes(vec![0; 0x2008]);
        result.store::<()>(0x31c, 0x12345678).unwrap();
        result.store::<()>(0xc74, 0xabcdef).unwrap();
        result
    }

    #[test]
    fn component_continuation_preserves_transforms_callbacks_and_native_collection_order() {
        let mut backend = Backend::new(true, true);
        let mut state = state();
        assert_eq!(
            state.init_components(&mut backend, MODEL),
            Ok(VehicleComponentsOutcome::UnavailableFrontier(
                VehicleComponentsFrontier::DoorSetup { index: 0 }
            ))
        );
        let exhaust: Vec<_> = backend
            .events
            .iter()
            .filter_map(|event| {
                if let Event::Exhaust(value) = event {
                    Some(value.map(f32::from_bits))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            exhaust,
            [[12., 26., 42., 2., 0., 0.], [12., 26., 42., 0., 0., 4.]]
        );
        let points: Vec<_> = backend
            .events
            .iter()
            .filter_map(|event| {
                if let Event::Point(value) = event {
                    Some(value.map(f32::from_bits))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(points, [[101., 2., 3.], [104., 5., 6.]]);
        let removed: Vec<_> = backend
            .events
            .iter()
            .filter_map(|event| {
                if let Event::Remove(_, frame) = event {
                    Some(*frame)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(removed, [EXHAUST_B, EXHAUST_A, SPECIAL, HEAD]);
        assert!(backend
            .events
            .contains(&Event::Callback(0x12345678, 0xabcdef, SPECIAL, 0x14000, 0)));
        assert_eq!(state.load::<()>(0x2d4), Ok(12f32.to_bits()));
        assert_eq!(state.load::<()>(0x2d8), Ok(26f32.to_bits()));
        assert_eq!(state.load::<()>(0x2dc), Ok(42f32.to_bits()));
        assert_eq!(state.head(), 0);
        assert!(backend.callback_returns.contains(&0x55));
        let vector = state.component_scratch().unwrap();
        assert_eq!(
            [
                backend.word(vector.begin),
                backend.word(vector.begin + 4),
                backend.word(vector.begin + 8)
            ],
            [0x7000, 0x7100, 0x7200]
        );
        assert_eq!(
            state.scratch().unwrap().end - state.scratch().unwrap().begin,
            4
        );
        assert_eq!(
            state.continue_components(&mut backend),
            Err(VehicleComponentsUnavailable::NoContinuation)
        );
    }

    #[test]
    fn every_component_effect_failure_keeps_its_prefix_and_consumes_continuation() {
        let mut baseline = Backend::new(true, true);
        let mut initial = state();
        initial.init_prefix(&mut baseline, MODEL).unwrap();
        let prefix_len = baseline.events.len();
        initial.continue_components(&mut baseline).unwrap();
        for fail in prefix_len..baseline.events.len() {
            let mut backend = Backend::new(true, true);
            let mut state = state();
            state.init_prefix(&mut backend, MODEL).unwrap();
            backend.fail = Some(fail);
            assert_eq!(
                state.continue_components(&mut backend),
                Err(VehicleComponentsUnavailable::Init(
                    VehicleInitUnavailable::Effect("unavailable")
                )),
                "effect {fail}"
            );
            assert_eq!(backend.events, baseline.events[..=fail], "effect {fail}");
            let retained = state.as_bytes().to_vec();
            let events = backend.events.clone();
            assert_eq!(
                state.continue_components(&mut backend),
                Err(VehicleComponentsUnavailable::NoContinuation)
            );
            assert_eq!(state.as_bytes(), retained);
            assert_eq!(backend.events, events);
        }
    }

    #[test]
    fn only_an_actual_frontier_authorizes_continuation_and_empty_doors_still_retain_vector() {
        let mut backend = Backend::new(false, false);
        let mut state = state();
        assert_eq!(
            state.continue_components(&mut backend),
            Err(VehicleComponentsUnavailable::NoContinuation)
        );
        assert!(backend.events.is_empty());
        assert_eq!(
            state.init_prefix(&mut backend, MODEL),
            Ok(VehicleInitOutcome::UnavailableFrontier(
                VehicleInitFrontier::BodyCollisionEnumeration
            ))
        );
        assert_eq!(
            state.continue_components(&mut backend),
            Ok(VehicleComponentsOutcome::UnavailableFrontier(
                VehicleComponentsFrontier::BallastEnumeration
            ))
        );
        assert_eq!(
            state.component_scratch(),
            Some(VehicleInitFrameVector {
                begin: 0,
                end: 0,
                capacity: 0
            })
        );
        assert_eq!(
            state.init_prefix(&mut backend, MODEL),
            Err(VehicleInitUnavailable::AlreadyStarted)
        );
    }

    #[test]
    fn unsupported_direction_retains_flag_prefix_and_rejects_unneeded_matrix_reads() {
        let mut backend = Backend::new(true, false);
        let mut state = state();
        backend.put(EXHAUST_A + 0x70, f32::NAN.to_bits());
        assert_eq!(
            state.init_components(&mut backend, MODEL),
            Err(VehicleComponentsUnavailable::NumericDomain(
                "component direction normalization"
            ))
        );
        assert_eq!(backend.word(EXHAUST_B + 0xac), 0x10);
        assert!(backend
            .events
            .iter()
            .any(|e| matches!(e, Event::Remove(_, EXHAUST_B))));
        assert!(!backend
            .events
            .iter()
            .any(|e| matches!(e, Event::Remove(_, EXHAUST_A))));
        // A point transform uses the twelve affine fields and never reads the
        // homogeneous column, even if it is unavailable to the external host.
        let mut backend = Backend::new(false, false);
        for index in [3, 7, 11, 15] {
            for byte in 0..4 {
                backend.memory.remove(&(BODY + 0x50 + index * 4 + byte));
            }
        }
        let point = transform_sources(
            &mut backend,
            MatrixSource::External(BODY + 0x50),
            PointSource::Local([1f32.to_bits(), 2f32.to_bits(), 3f32.to_bits()]),
        )
        .unwrap();
        assert_eq!(point.map(f32::from_bits), [12., 26., 42.]);
    }
}
