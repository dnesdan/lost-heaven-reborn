//! Descriptive working names for the PC Windows caller helpers (Game RVAs
//! 0x1662a0..0x166440). Host callbacks implement the actual operating-system
//! operations; this module does not fabricate Windows drive/window services.

use crate::original_system::{SystemError, SystemHandle};

pub trait WindowsBackend {
    fn open_read_exclusive(&mut self, path: &[u8]) -> Result<Option<SystemHandle>, SystemError>;
    fn close_file_handle(&mut self, file: SystemHandle) -> Result<(), SystemError>;
    fn drive_type(&mut self, root: &[u8]) -> Result<u32, SystemError>;
    fn volume_information(&mut self, root: &[u8]) -> Result<bool, SystemError>;
    fn find_first_file(&mut self, path: &[u8]) -> Result<Option<SystemHandle>, SystemError>;
    fn find_close(&mut self, search: SystemHandle) -> Result<(), SystemError>;
    fn load_string(
        &mut self,
        instance: SystemHandle,
        id: u32,
        limit: i32,
    ) -> Result<(i32, Vec<u8>), SystemError>;
    fn load_icon_one(
        &mut self,
        instance: SystemHandle,
    ) -> Result<Option<SystemHandle>, SystemError>;
    fn set_class_icon(
        &mut self,
        window: SystemHandle,
        icon: SystemHandle,
    ) -> Result<(), SystemError>;
}

/// The returned buffer is the actual last attempted path even on native false.
/// Bounded ASCII names avoid the original unchecked strcpy/sprintf overrun.
pub fn find_game_file(
    backend: &mut impl WindowsBackend,
    name: &[u8],
    path: &mut Vec<u8>,
) -> Result<bool, SystemError> {
    if name.contains(&0) || name.len() + b"X:\\MafiaGame\\".len() >= 256 {
        return Err(SystemError::Unsupported(
            "FindGameFile native path buffer domain",
        ));
    }
    path.clear();
    path.extend_from_slice(name);
    if let Some(file) = backend.open_read_exclusive(path)? {
        backend.close_file_handle(file)?;
        return Ok(true);
    }
    for drive in b'C'..=b'Z' {
        path.clear();
        path.extend_from_slice(&[drive, b':', b'\\']);
        backend.drive_type(path)?;
        if backend.volume_information(path)? {
            path.extend_from_slice(b"MafiaGame\\");
            path.extend_from_slice(name);
            if let Some(file) = backend.open_read_exclusive(path)? {
                backend.close_file_handle(file)?;
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn file_exists(backend: &mut impl WindowsBackend, path: &[u8]) -> Result<bool, SystemError> {
    if let Some(search) = backend.find_first_file(path)? {
        backend.find_close(search)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// The supplied PC binary's canonical entry is four bytes: XOR EAX,EAX;
/// INC EAX; RET. The drive-search block after it is unreachable and receives no
/// implementation credit. No Windows callbacks execute through this entry.
pub fn check_cd(_backend: &mut impl WindowsBackend) -> Result<bool, SystemError> {
    Ok(true)
}

/// WaitForCD's first canonical CheckCD call always returns true in this binary.
/// Its retry/message/presentation branch is therefore unreachable here.
pub fn wait_for_cd(
    backend: &mut impl WindowsBackend,
    _graph: Option<SystemHandle>,
) -> Result<bool, SystemError> {
    check_cd(backend)
}

pub fn load_resource_string(
    backend: &mut impl WindowsBackend,
    instance: SystemHandle,
    id: u32,
    limit: i32,
) -> Result<(i32, Vec<u8>), SystemError> {
    backend.load_string(instance, id, limit)
}

pub fn set_window_icon(
    backend: &mut impl WindowsBackend,
    instance: SystemHandle,
    window: SystemHandle,
) -> Result<(), SystemError> {
    if let Some(icon) = backend.load_icon_one(instance)? {
        backend.set_class_icon(window, icon)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Fixture {
        paths: Vec<Vec<u8>>,
        closed: Vec<SystemHandle>,
        existing: Vec<u8>,
        opens: usize,
        probes: usize,
    }
    impl WindowsBackend for Fixture {
        fn open_read_exclusive(&mut self, p: &[u8]) -> Result<Option<SystemHandle>, SystemError> {
            self.paths.push(p.to_vec());
            self.opens += 1;
            Ok((p == self.existing).then_some(SystemHandle(7)))
        }
        fn close_file_handle(&mut self, h: SystemHandle) -> Result<(), SystemError> {
            self.closed.push(h);
            Ok(())
        }
        fn drive_type(&mut self, _: &[u8]) -> Result<u32, SystemError> {
            self.probes += 1;
            Ok(0)
        }
        fn volume_information(&mut self, r: &[u8]) -> Result<bool, SystemError> {
            Ok(r[0] == b'C')
        }
        fn find_first_file(&mut self, p: &[u8]) -> Result<Option<SystemHandle>, SystemError> {
            self.paths.push(p.to_vec());
            Ok((p == self.existing).then_some(SystemHandle(8)))
        }
        fn find_close(&mut self, h: SystemHandle) -> Result<(), SystemError> {
            self.closed.push(h);
            Ok(())
        }
        fn load_string(
            &mut self,
            _: SystemHandle,
            _: u32,
            _: i32,
        ) -> Result<(i32, Vec<u8>), SystemError> {
            Err(SystemError::Unsupported("resource"))
        }
        fn load_icon_one(&mut self, _: SystemHandle) -> Result<Option<SystemHandle>, SystemError> {
            Err(SystemError::Unsupported("icon"))
        }
        fn set_class_icon(&mut self, _: SystemHandle, _: SystemHandle) -> Result<(), SystemError> {
            Err(SystemError::Unsupported("window"))
        }
    }
    #[test]
    fn original_check_cd_and_wait_return_true_without_any_platform_callback() {
        let mut f = Fixture::default();
        assert_eq!(check_cd(&mut f), Ok(true));
        assert_eq!(wait_for_cd(&mut f, Some(SystemHandle(99))), Ok(true));
        assert!(f.paths.is_empty());
        assert!(f.closed.is_empty());
        assert_eq!(f.probes, 0);
    }
    #[test]
    fn file_search_preserves_last_failed_path_and_local_success_avoids_drive_probes() {
        let mut f = Fixture::default();
        let mut path = vec![];
        assert_eq!(find_game_file(&mut f, b"a1.dta", &mut path), Ok(false));
        assert_eq!(path, b"Z:\\");
        assert_eq!(f.probes, 24);
        let mut f = Fixture {
            existing: b"a1.dta".to_vec(),
            ..Default::default()
        };
        assert_eq!(find_game_file(&mut f, b"a1.dta", &mut path), Ok(true));
        assert_eq!(f.probes, 0);
        assert_eq!(f.closed, vec![SystemHandle(7)]);
    }
}
