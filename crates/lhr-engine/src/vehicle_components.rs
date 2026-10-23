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

fn read_words<B: VehicleInitBackend, const N: usize>(
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
fn matrix(words: [u32; 16]) -> SMatrix {
    SMatrix {
        elements: words.map(f32::from_bits),
    }
}
#[derive(Clone, Copy)]
enum MatrixSource {
    Local(SMatrix),
    External(u32),
}
#[derive(Clone, Copy)]
enum PointSource {
    Local([u32; 3]),
    External(u32),
}
// The existing checked matrix primitive has the same LS3DF export and
// association. Read operands here in its instruction order; HEAD and SPECCOLL
// do not copy a complete matrix and never read its homogeneous column.
fn transform_sources<B: VehicleInitBackend>(
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
fn normalize_direction<E>(words: [u32; 3]) -> Result<[u32; 3], VehicleComponentsUnavailable<E>> {
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
        Ok(VehicleComponentsOutcome::UnavailableFrontier(
            if self.component_count() != 0 {
                VehicleComponentsFrontier::DoorSetup { index: 0 }
            } else {
                VehicleComponentsFrontier::BallastEnumeration
            },
        ))
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
            let bounds: [u32; 6] = read_words(backend, frame.wrapping_add(0x140))?;
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
            Ok(0x13000)
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
            self.event(Event::Link(frame, parent, flags))
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
            self.event(Event::BodyCollision(frame))?;
            put_state(vehicle, 0x4b0, 0x15000);
            Ok(0x55)
        }
        fn finish_body_collision_callbacks(
            &mut self,
            _vehicle: &mut [u8],
            _context: VehicleBodyCollisionContext,
        ) -> Result<(), Self::Error> {
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
