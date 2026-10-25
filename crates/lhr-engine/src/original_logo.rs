//! Descriptive PlayLogo caller projection, Game RVA0x15d460. AVI/Bink decoding,
//! IShow presentation and desktop file search are external dependencies. No video
//! is created or played by this module without an actual backend.
use crate::native_precision as fp;
use crate::original_system::{SystemError, SystemHandle};

/// A backend owns all created show IDs and must reject unknown/foreign handles.
/// A stopped caller leaves its show alive in that same backend; moving it to a
/// different owner or silently dropping it is not an admitted cleanup operation.
pub trait LogoBackend {
    fn find_game_file(&mut self, name: &[u8]) -> Result<Option<Vec<u8>>, SystemError>;
    fn create_show(&mut self) -> Result<SystemHandle, SystemError>;
    fn open_show(
        &mut self,
        show: SystemHandle,
        path: &[u8],
        flags: u32,
    ) -> Result<i32, SystemError>;
    fn screen_sx(&mut self) -> Result<i32, SystemError>;
    fn screen_sy(&mut self) -> Result<i32, SystemError>;
    fn set_rect(&mut self, show: SystemHandle, rect: [i32; 4]) -> Result<(), SystemError>;
    fn play(&mut self, show: SystemHandle) -> Result<(), SystemError>;
    fn render_until_key_or_end(&mut self, show: SystemHandle) -> Result<(), SystemError>;
    fn release_show(&mut self, show: SystemHandle) -> Result<(), SystemError>;
}

fn filename(name: &[u8], extension: &[u8]) -> Result<Vec<u8>, SystemError> {
    if name.is_empty()
        || name.contains(&0)
        || name.len() + usize::from(name.last() != Some(&b'.')) + extension.len() + 1 > 256
    {
        return Err(SystemError::Unsupported(
            "PlayLogo native filename buffer domain",
        ));
    }
    let mut out = name.to_vec();
    if name.last() != Some(&b'.') {
        out.push(b'.');
    }
    out.extend_from_slice(extension);
    Ok(out)
}

/// Unavailable host calls preserve all prior effects and live show ownership in
/// the backend. No invented Release/rollback follows an unsupported operation.
/// Numeric support covers finite PC24 band arithmetic and an exactly representable
/// FILD screen height and logo dimensions in 1..=65536; other x87 exponent/exception
/// domains stop explicitly.
pub fn play_logo(
    backend: &mut impl LogoBackend,
    name: &[u8],
    size: [f32; 2],
) -> Result<bool, SystemError> {
    let avi_name = filename(name, b"avi")?;
    let (path, avi) = if let Some(path) = backend.find_game_file(&avi_name)? {
        (path, true)
    } else {
        let bik_name = filename(name, b"bik")?;
        let Some(path) = backend.find_game_file(&bik_name)? else {
            return Ok(false);
        };
        (path, false)
    };
    let show = backend.create_show()?;
    if backend.open_show(show, &path, 0)? != 0 {
        if !avi {
            backend.release_show(show)?;
            return Ok(false);
        }
        let bik_name = filename(name, b"bik")?;
        let Some(path) = backend.find_game_file(&bik_name)? else {
            backend.release_show(show)?;
            return Ok(false);
        };
        if backend.open_show(show, &path, 0)? != 0 {
            backend.release_show(show)?;
            return Ok(false);
        }
    }
    let screen_height = backend.screen_sy()?;
    if screen_height.unsigned_abs() > (1 << 24)
        || !size
            .into_iter()
            .all(|value| (1.0..=65536.0).contains(&value))
    {
        return Err(SystemError::Unsupported("PlayLogo x87 band numeric domain"));
    }
    let ratio = fp::div(size[1], size[0]);
    let scaled = fp::mul(ratio, f32::from_bits(0x3faaaaab));
    let factor = fp::sub(1.0, scaled);
    let band_float = fp::mul(screen_height as f32, factor);
    if !band_float.is_finite()
        || f64::from(band_float) < f64::from(i32::MIN)
        || f64::from(band_float) > f64::from(i32::MAX)
    {
        return Err(SystemError::Unsupported("PlayLogo ftol numeric domain"));
    }
    let band = band_float.trunc() as i32;
    fp::record_precision(band_float != band as f32);
    let mut rect = [
        0,
        band / 2,
        backend.screen_sx()?,
        backend.screen_sy()?.wrapping_sub(band),
    ];
    let double_height = backend.screen_sy()?.wrapping_mul(6) / 3;
    if backend.screen_sx()? > double_height {
        rect[0] = 0;
        rect[2] = backend.screen_sx()? / 2;
    }
    let triple_height = backend.screen_sy()?.wrapping_mul(9) / 3;
    if backend.screen_sx()? > triple_height {
        rect[0] = backend.screen_sx()? / 3;
        rect[2] = backend.screen_sx()? / 3;
    }
    backend.set_rect(show, rect)?;
    backend.play(show)?;
    backend.render_until_key_or_end(show)?;
    backend.release_show(show)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        width: i32,
        height: i32,
        missing_avi: bool,
        fail_avi: bool,
        files: Vec<Vec<u8>>,
        opened: Vec<Vec<u8>>,
        rect: Option<[i32; 4]>,
        effects: Vec<&'static str>,
    }
    impl Fixture {
        fn new(width: i32, height: i32) -> Self {
            Self {
                width,
                height,
                missing_avi: false,
                fail_avi: false,
                files: vec![],
                opened: vec![],
                rect: None,
                effects: vec![],
            }
        }
    }
    impl LogoBackend for Fixture {
        fn find_game_file(&mut self, p: &[u8]) -> Result<Option<Vec<u8>>, SystemError> {
            self.files.push(p.to_vec());
            Ok((!self.missing_avi || !p.ends_with(b"avi")).then(|| p.to_vec()))
        }
        fn create_show(&mut self) -> Result<SystemHandle, SystemError> {
            self.effects.push("create");
            Ok(SystemHandle(1))
        }
        fn open_show(&mut self, _: SystemHandle, p: &[u8], _: u32) -> Result<i32, SystemError> {
            self.opened.push(p.to_vec());
            Ok(if self.fail_avi && p.ends_with(b"avi") {
                -1
            } else {
                0
            })
        }
        fn screen_sx(&mut self) -> Result<i32, SystemError> {
            self.effects.push("sx");
            Ok(self.width)
        }
        fn screen_sy(&mut self) -> Result<i32, SystemError> {
            self.effects.push("sy");
            Ok(self.height)
        }
        fn set_rect(&mut self, _: SystemHandle, r: [i32; 4]) -> Result<(), SystemError> {
            self.rect = Some(r);
            self.effects.push("rect");
            Ok(())
        }
        fn play(&mut self, _: SystemHandle) -> Result<(), SystemError> {
            self.effects.push("play");
            Ok(())
        }
        fn render_until_key_or_end(&mut self, _: SystemHandle) -> Result<(), SystemError> {
            self.effects.push("render");
            Ok(())
        }
        fn release_show(&mut self, _: SystemHandle) -> Result<(), SystemError> {
            self.effects.push("release");
            Ok(())
        }
    }
    #[test]
    fn failed_avi_reuses_show_for_bink_and_preserves_native_triple_monitor_rectangle() {
        let mut f = Fixture::new(2400, 600);
        f.fail_avi = true;
        assert_eq!(play_logo(&mut f, b"logo.", [800.0, 600.0]), Ok(true));
        assert_eq!(f.files, vec![b"logo.avi".to_vec(), b"logo.bik".to_vec()]);
        assert_eq!(f.opened, f.files);
        assert_eq!(f.rect, Some([800, 0, 800, 600]));
        assert_eq!(
            f.effects,
            vec![
                "create", "sy", "sx", "sy", "sy", "sx", "sx", "sy", "sx", "sx", "sx", "rect",
                "play", "render", "release"
            ]
        );
    }
    #[test]
    fn missing_avi_and_letterbox_band_keep_original_asymmetric_bottom() {
        let mut f = Fixture::new(800, 600);
        f.missing_avi = true;
        assert_eq!(play_logo(&mut f, b"logo2", [704.0, 400.0]), Ok(true));
        assert_eq!(f.rect, Some([0, 72, 800, 455]));
        assert_eq!(f.opened, vec![b"logo2.bik".to_vec()]);
    }
    #[test]
    fn unsupported_numeric_domain_preserves_unreleased_show_after_open() {
        let mut f = Fixture::new(800, i32::MAX);
        assert_eq!(
            play_logo(&mut f, b"logo", [800.0, 600.0]),
            Err(SystemError::Unsupported("PlayLogo x87 band numeric domain"))
        );
        assert_eq!(f.effects, vec!["create", "sy"]);
        assert_eq!(f.opened.len(), 1);
    }
}
