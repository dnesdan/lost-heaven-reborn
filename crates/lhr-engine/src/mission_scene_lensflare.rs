//! Game LoadScene2 arm543e51..544062, SHA08398317… . No resources are acquired;
//! scalar mutations borrow the actual LENS frame created before this arm.
use super::*;
use crate::frame_transform::{FrameError, LensflareParameter, LensflareParameterUnavailable};

impl MissionSceneLoader {
    pub(super) fn read_lensflare_parameters(
        &mut self,
        arena: &mut ModelArena,
        input: &mut MissionSceneInput<'_>,
    ) -> Result<(), (u32, String)> {
        let Some(frame) = self.current_frame else {
            return input.close_current().map_err(|e| (0x544062, e));
        };
        let version = self.version[0].ok_or((
            0x543e59,
            "lensflare requires original version header".into(),
        ))?;
        if (version as i16) < 5 {
            return input.close_current().map_err(|e| (0x544062, e));
        }
        self.object_fields.flags |= 0x400;
        input.enter_current().map_err(|e| (0x543e86, e))?;
        while let Some(field) = input.next_chunk().map_err(|e| (0x543ec7, e))? {
            let (parameter, read_entry, close_entry, store_entry) = match field.kind {
                0xb112 => (LensflareParameter::Size, 0x543f3f, 0x543f48, 0x543f4d),
                0xb113 => (LensflareParameter::Intensity, 0x543f6d, 0x543f76, 0x543f7b),
                0xb114 => (
                    LensflareParameter::TestInterval,
                    0x543f9b,
                    0x543fa4,
                    0x543fb0,
                ),
                0xb115 => (LensflareParameter::FadeIn, 0x543fce, 0x543fd7, 0x543fdc),
                0xb116 => (LensflareParameter::FadeOut, 0x544008, 0x544011, 0x544022),
                0xb117 => (LensflareParameter::Range, 0x544042, 0x54404b, 0x544050),
                _ => {
                    return Err((
                        0x543f24,
                        format!("unavailable lensflare child {:04x}", field.kind),
                    ))
                }
            };
            let value = f32::from_bits(input.read_u32().map_err(|e| (read_entry, e))?);
            input.close_current().map_err(|e| (close_entry, e))?;
            let key = FrameKey(frame.identity());
            if arena.lookup_reference(key) != frame {
                return Err((store_entry, "stale lensflare frame".into()));
            }
            arena
                .graph
                .assign_lensflare_parameter(key, parameter, value)
                .map_err(|e| {
                    let entry = match e {
                        FrameError::LensflareParameterUnavailable(
                            LensflareParameterUnavailable::ExistingFadeOut,
                        ) => 0x543fe9,
                        FrameError::LensflareParameterUnavailable(
                            LensflareParameterUnavailable::ExistingFadeIn,
                        ) => 0x544016,
                        FrameError::LensflareParameterUnavailable(
                            LensflareParameterUnavailable::ScalarInput,
                        ) if matches!(parameter, LensflareParameter::TestInterval) => 0x543fa9,
                        _ => store_entry,
                    };
                    (entry, format!("{e:?}"))
                })?;
        }
        input.leave_container().map_err(|e| (0x544062, e))
    }
}
