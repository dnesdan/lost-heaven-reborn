//! Shared host adapters for an explicitly supplied inspection viewport and the
//! same existing Installation. They do not establish original GameInit state,
//! create another asset installation, or own scene/texture resources.

/// Caller-supplied screen state. Native constructor/projection code still reads
/// the values through its actual SceneScreen and DriverSceneScreen interfaces.
/// A CLI probe must describe these as its chosen host dimensions, not as an
/// observation of an original game window or the current desktop.
pub struct InspectionScreen {
    pub size: [i32; 2],
    pub viewport: [i32; 4],
}

impl crate::scene_resources::SceneScreen for InspectionScreen {
    type Error = std::convert::Infallible;

    fn screen_sy(&mut self) -> Result<i32, Self::Error> {
        Ok(self.size[1])
    }

    fn screen_sx(&mut self) -> Result<i32, Self::Error> {
        Ok(self.size[0])
    }
}

impl crate::driver_scene::DriverSceneScreen for InspectionScreen {
    fn viewport_rectangle(&mut self) -> Result<[i32; 4], Self::Error> {
        Ok(self.viewport)
    }
}

/// A borrowed read adapter. Its Installation remains the caller's actual
/// archive/loose-file owner, with all source precedence and IO errors preserved.
pub struct InstallationImages<'a>(pub &'a lhr_assets::Installation);

impl crate::itexture::TextureImageSource for InstallationImages<'_> {
    fn read_image(&mut self, path: &[u8]) -> Result<Vec<u8>, crate::itexture::TextureImageError> {
        use crate::itexture::TextureImageError;
        let path = std::str::from_utf8(path).map_err(|_| TextureImageError::Unsupported)?;
        self.0.read_original_path(path).map_err(|error| {
            if error.0 == "original asset unavailable" {
                TextureImageError::Missing
            } else {
                TextureImageError::Backend(error.to_string())
            }
        })
    }
}
