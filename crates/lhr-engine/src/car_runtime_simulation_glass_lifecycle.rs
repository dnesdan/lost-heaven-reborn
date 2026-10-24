//! The standalone game caller pairs the original glass manager Init/Done
//! once and releases its outer vector storage only after Done consumes frames.
use super::*;
impl<B> CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    pub(crate) fn tick_glass(&mut self, milliseconds: u32) -> Result<(), String> {
        let app = self.read_word(0x63788c)?;
        let game = self.read_word(app + 0x24)?;
        crate::break_plain_glass_update::glass_tick(self, game + 0x208, milliseconds)
            .map_err(failure)
    }
    pub(crate) fn prepare_glass_services(&mut self) -> Result<(), String> {
        if self.state.glass_init_started {
            return Err("Glass initialization retained an interrupted prefix".into());
        }
        self.state.glass_init_started = true;
        let app = self.read_word(0x63788c)?;
        let game = self.read_word(app + 0x24)?;
        if !crate::break_plain_glass_lifecycle::glass_init(self, game + 0x208, 5)
            .map_err(failure)?
        {
            return Err("Original glass manager allocation returned null".into());
        }
        Ok(())
    }
    pub(crate) fn cleanup_glass_services(&mut self) -> Result<(), String> {
        if !self.state.glass_init_started {
            return Ok(());
        }
        if self.state.glass_cleanup_started {
            return Err("Glass cleanup retained an interrupted prefix".into());
        }
        self.state.glass_cleanup_started = true;
        let app = self.read_word(0x63788c)?;
        let game = self.read_word(app + 0x24)?;
        crate::break_plain_glass::cleanup_glass_pending(self).map_err(failure)?;
        crate::break_plain_glass_lifecycle::glass_done(self, game + 0x208).map_err(failure)?;
        crate::break_plain_glass_lifecycle::glass_free_caller_storage(self, game + 0x208)
            .map_err(failure)
    }
}
