//! Host resources share the existing frame and heap projection. The temporary
//! service view has no host delegate, preventing accidental recursive dispatch.
use super::*;

#[derive(Clone, Copy, Debug)]
pub struct VehicleFrameDamageNotification {
    pub function: u32,
    pub context: u32,
    pub frame: u32,
    pub part_type: u16,
    pub point: [f32; 3],
    pub impulse: [f32; 3],
    /// The fourth stack argument is discarded by C_car's 0x426290 wrapper.
    pub changed: bool,
}

pub trait VehicleFrameGameHost<E> {
    fn audio(&self) -> Option<crate::runtime_audio::RuntimeAudio> {
        None
    }
    fn init_effect(
        &mut self,
        _: &mut dyn CarGameInitBackend<Error = VehicleFrameError<E>>,
        _: VehicleComponentTailMemory<'_>,
        effect: CarGameInitEffect,
    ) -> Result<u32, VehicleFrameError<E>> {
        Err(VehicleFrameError::UnavailableGameEffect(format!(
            "{effect:?}"
        )))
    }
    fn resource_effect(
        &mut self,
        _: &mut dyn CarGameInitBackend<Error = VehicleFrameError<E>>,
        _: VehicleComponentTailMemory<'_>,
        effect: VehicleResourceEffect,
    ) -> Result<u32, VehicleFrameError<E>> {
        Err(VehicleFrameError::UnavailableGameEffect(format!(
            "{effect:?}"
        )))
    }
    fn support_effect(
        &mut self,
        _: &mut dyn CarGameInitBackend<Error = VehicleFrameError<E>>,
        _: VehicleComponentTailMemory<'_>,
        effect: CarGameInitSupportEffect,
    ) -> Result<u32, VehicleFrameError<E>> {
        Err(VehicleFrameError::UnavailableGameEffect(format!(
            "{effect:?}"
        )))
    }
    fn done_effect(
        &mut self,
        _: &mut dyn CarGameInitBackend<Error = VehicleFrameError<E>>,
        _: VehicleComponentTailMemory<'_>,
        effect: CarGameDoneEffect,
    ) -> Result<(), VehicleFrameError<E>> {
        Err(VehicleFrameError::UnavailableGameEffect(format!(
            "{effect:?}"
        )))
    }
    fn virtual_call(
        &mut self,
        _: &mut dyn CarGameInitBackend<Error = VehicleFrameError<E>>,
        _: u32,
        _: u32,
        _: &[u32],
    ) -> Result<Option<u32>, VehicleFrameError<E>> {
        Ok(None)
    }
    fn damage_notification(
        &mut self,
        _: &mut dyn CarGameInitBackend<Error = VehicleFrameError<E>>,
        _: VehicleComponentTailMemory<'_>,
        effect: VehicleFrameDamageNotification,
    ) -> Result<(), VehicleFrameError<E>> {
        Err(VehicleFrameError::UnavailableGameEffect(format!(
            "{effect:?}"
        )))
    }
}
impl<B: VehicleComponentTailBackend> VehicleFrameBackend<'_, B> {
    fn with_game_service(
        &mut self,
        call: impl FnOnce(
            &mut dyn VehicleFrameGameHost<B::Error>,
            &mut dyn CarGameInitBackend<Error = VehicleFrameError<B::Error>>,
        ) -> Result<u32, VehicleFrameError<B::Error>>,
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        let game =
            self.game_host
                .as_deref_mut()
                .ok_or(VehicleFrameError::UnavailableFrameMethod(
                    "GameInit host not bound",
                ))?;
        let mut view = VehicleFrameBackend {
            arena: &mut *self.arena,
            host: &mut *self.host,
            installation: self.installation,
            car_tracks: self.car_tracks.as_deref_mut(),
            game_init_stack: self.game_init_stack,
            game_host: None,
        };
        call(game, &mut view)
    }
    pub(super) fn game_host_init(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: CarGameInitEffect,
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        self.with_game_service(|g, b| g.init_effect(b, memory, effect))
    }
    pub(super) fn game_host_resource(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: VehicleResourceEffect,
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        self.with_game_service(|g, b| g.resource_effect(b, memory, effect))
    }
    pub(super) fn game_host_support(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: CarGameInitSupportEffect,
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        self.with_game_service(|g, b| g.support_effect(b, memory, effect))
    }
    pub(super) fn game_host_virtual(
        &mut self,
        object: u32,
        slot: u32,
        args: &[u32],
    ) -> Result<Option<u32>, VehicleFrameError<B::Error>> {
        if self.game_host.is_none() {
            return Ok(None);
        }
        let mut result = None;
        self.with_game_service(|g, b| {
            result = g.virtual_call(b, object, slot, args)?;
            Ok(0)
        })?;
        Ok(result)
    }
    pub(super) fn game_host_done(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: CarGameDoneEffect,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        self.with_game_service(|g, b| {
            g.done_effect(b, memory, effect)?;
            Ok(0)
        })
        .map(|_| ())
    }
    pub(crate) fn game_host_damage_notification(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: VehicleFrameDamageNotification,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        self.with_game_service(|g, b| {
            g.damage_notification(b, memory, effect)?;
            Ok(0)
        })
        .map(|_| ())
    }
}
