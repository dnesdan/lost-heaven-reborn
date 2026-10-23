//! Original CENTRE and final bounds preparation within C_Vehicle::Init.
use super::components::{matrix, MatrixSource, PointSource};
use super::wheels::WheelsResult;
use super::*;

pub trait VehicleInitFinishBackend: VehicleComponentsBackend {
    /// Model EnumFrames: pattern CENTRE, mask0x20, single-slot callback0x4d0ea0.
    fn enumerate_init_centre(
        &mut self,
        model: u32,
        target: &mut VehicleInitCentreEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>>;
    /// Optional indirect vehicle+0x31c call, ECX=context, EDX=frame, args(control,-1).
    fn init_centre_callback(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        function: u32,
        context: u32,
        frame: u32,
        control: u32,
    ) -> Result<(), Self::Error>;
    /// Original model virtual+0x18 after CENTRE removal.
    fn update_init_model(&mut self, model: u32) -> Result<(), Self::Error>;
    /// Original LS3DF UpdateHRBoundVol, guarded by frame+0xac bit0x40.
    fn update_init_model_bounds(&mut self, model: u32) -> Result<(), Self::Error>;
}
pub struct VehicleInitCentreEnumeration<'a> {
    state: &'a mut VehicleInitState,
}
impl VehicleInitCentreEnumeration<'_> {
    pub fn visit(&mut self, frame: u32) -> u32 {
        self.state.tail_store(0x7c, frame);
        0
    }
}
impl VehicleInitState {
    /// Inline portion0x4bf607..0x4bf981. A true return means CENTRE/bounds are
    /// prepared, never that Init or the final source-parameter helper succeeded.
    pub(super) fn finish_init_geometry<B: VehicleInitFinishBackend>(
        &mut self,
        b: &mut B,
    ) -> WheelsResult<B, bool> {
        use VehicleInitUnavailable::Effect;
        let model = self.load(0x34c)?;
        self.tail_store(0x7c, 0);
        b.enumerate_init_centre(model, &mut VehicleInitCentreEnumeration { state: self })?;
        if self.tail_word(0x7c) == 0 {
            return Ok(false);
        }
        let function = self.load::<B::Error>(0x31c)?;
        *self
            .bytes
            .get_mut(0x1ff4)
            .ok_or(VehicleInitUnavailable::Storage(0x1ff4))? = 1;
        self.store(0x1ff0, 0x40)?;
        if function != 0 {
            let offset = self.tail_word(0x1c);
            let begin = self.tail_word(0x6c);
            let frame = self.wheel_word(b, begin.wrapping_add(offset))?;
            let context = self.load(0xc74)?;
            let control = self.tail_vehicle_address.unwrap().wrapping_add(0x1ff0);
            b.init_centre_callback(self.live_tail_memory(), function, context, frame, control)
                .map_err(Effect)?;
        }
        let centre = self.tail_word(0x7c);
        for index in 0..6 {
            let value = self.wheel_word(b, centre.wrapping_add(0x140 + index * 4))?;
            self.tail_store(0x10c + index as usize * 4, value);
        }
        for (input, output) in [(0x10c, 0x364), (0x118, 0x370)] {
            let centre = self.tail_word(0x7c);
            b.prepare_frame_matrix(centre).map_err(Effect)?;
            let point = [
                self.tail_word(input),
                self.tail_word(input + 4),
                self.tail_word(input + 8),
            ];
            let value = self.tail_transform(
                b,
                MatrixSource::External(centre.wrapping_add(0x50)),
                PointSource::Local(point),
                0x80,
            )?;
            for (index, word) in value.into_iter().enumerate() {
                self.store(output + index as u32 * 4, word)?;
            }
        }
        for output in [0x364, 0x370] {
            let body = self.load::<B::Error>(0xe68)?;
            b.prepare_frame_matrix(body).map_err(Effect)?;
            let base = self.tail_vehicle_address.unwrap();
            let value = self.tail_transform(
                b,
                MatrixSource::External(body.wrapping_add(0x50)),
                PointSource::External(base.wrapping_add(output)),
                0x80,
            )?;
            for (index, word) in value.into_iter().enumerate() {
                self.store(output + index as u32 * 4, word)?;
            }
        }
        b.link_frame(self.tail_word(0x7c), 0, 0).map_err(Effect)?;
        let model = self.load(0x34c)?;
        b.remove_model_child(model, self.tail_word(0x7c))
            .map_err(Effect)?;
        let model = self.load(0x34c)?;
        b.update_init_model(model).map_err(Effect)?;
        let model = self.load::<B::Error>(0x34c)?;
        if self.wheel_byte(b, model.wrapping_add(0xac))? & 0x40 == 0 {
            b.update_init_model_bounds(model).map_err(Effect)?;
        }
        for index in 0..6 {
            let value = self.wheel_word(b, model.wrapping_add(0xb0 + index * 4))?;
            self.tail_store(0x10c + index as usize * 4, value);
        }
        for index in 0..3 {
            let value = crate::native_matrix::checked_f32_add(
                f32::from_bits(self.tail_word(0x118 + index * 4)),
                -f32::from_bits(self.tail_word(0x10c + index * 4)),
            )
            .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            self.store(0x164 + index as u32 * 4, value.to_bits())?;
        }
        self.init_model_scale(b)?;
        // These temporary stores precede the corresponding vehicle copies.
        for index in 0..3 {
            let value = crate::native_matrix::checked_f32_mul(
                f32::from_bits(self.tail_word(0x28 + index * 4)),
                f32::from_bits(self.load::<B::Error>(0x164 + index as u32 * 4)?),
            )
            .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            self.tail_store(0x10 + index * 4, value.to_bits());
        }
        self.store(0x158, self.tail_word(0x10))?;
        self.tail_store(0x28, self.tail_word(0x10c) ^ 0x80000000);
        self.store(0x15c, self.tail_word(0x14))?;
        self.tail_store(0x2c, self.tail_word(0x110) ^ 0x80000000);
        self.store(0x160, self.tail_word(0x18))?;
        self.store(0x17c, self.tail_word(0x28))?;
        self.tail_store(0x30, self.tail_word(0x114) ^ 0x80000000);
        self.store(0x180, self.tail_word(0x2c))?;
        self.store(0x184, self.tail_word(0x30))?;
        self.init_model_scale(b)?;
        for index in 0..3 {
            let value = crate::native_matrix::checked_f32_mul(
                f32::from_bits(self.tail_word(0x28 + index * 4)),
                f32::from_bits(self.load::<B::Error>(0x17c + index as u32 * 4)?),
            )
            .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            self.tail_store(0x10 + index * 4, value.to_bits());
        }
        for index in 0..3 {
            self.store(0x170 + index as u32 * 4, self.tail_word(0x10 + index * 4))?;
        }
        Ok(true)
    }
    fn init_model_scale<B: VehicleInitFinishBackend>(&mut self, b: &mut B) -> WheelsResult<B, ()> {
        let model = self.load::<B::Error>(0x34c)?;
        if self.wheel_word(b, model.wrapping_add(0x120))? == 0 {
            self.init_geometry_saved_esi = Some(model.wrapping_add(0x90));
            for index in 0..3 {
                let value = self.wheel_word(b, model.wrapping_add(0x90 + index * 4))?;
                self.tail_store(0x28 + index as usize * 4, value);
            }
        } else {
            self.init_geometry_saved_esi = Some(model.wrapping_add(0x10));
            if self.wheel_byte(b, model.wrapping_add(0xac))? & 0x20 == 0 {
                b.update_model_world_matrix(model)
                    .map_err(VehicleInitUnavailable::Effect)?;
            }
            let mut words = [0; 16];
            for (index, word) in words.iter_mut().enumerate() {
                *word = self.wheel_word(b, model.wrapping_add(0x10 + index as u32 * 4))?;
            }
            let scale = matrix(words)
                .try_get_scale()
                .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            for (index, value) in scale.into_iter().enumerate() {
                self.tail_store(0x28 + index * 4, value.to_bits());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleInitGeometryOutcome {
    /// Original required CENTRE enumeration was empty. Temporary Init vectors
    /// have been released; persistent component/wheel ownership is retained.
    MissingCentre,
    /// Original VA0x4bf981, before the final helper0x4bfa50. Still incomplete.
    FinalSetup,
}
impl VehicleInitState {
    /// Continues the genuine wheel-order capability through InitAxis, UpdateAxis
    /// and final geometric preparation. Any unavailable effect consumes it.
    pub fn continue_init_geometry<
        B: VehicleAxisBackend + VehicleAxisUpdateBackend + VehicleInitFinishBackend,
    >(
        &mut self,
        b: &mut B,
    ) -> WheelsResult<B, VehicleInitGeometryOutcome> {
        if !std::mem::take(&mut self.wheel_setup_ready) {
            return Err(VehicleWheelsUnavailable::NoContinuation);
        }
        let stack = self
            .tail_scratch
            .as_ref()
            .unwrap()
            .address
            .wrapping_sub(0xf4);
        self.setup_axes(b, stack)?;
        self.update_axes(b)?;
        if !self.finish_init_geometry(b)? {
            self.release_init_vectors(b)?;
            return Ok(VehicleInitGeometryOutcome::MissingCentre);
        }
        self.final_setup_ready = true;
        Ok(VehicleInitGeometryOutcome::FinalSetup)
    }
}
