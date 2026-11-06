//! Descriptive working names for the PC Windows caller helpers (Game RVAs
//! 0x166160 and 0x1662a0..0x166440). Host callbacks implement the actual operating-system
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

/// WinMain executable-directory chop (descriptive working name; `WinMain` is
/// the standard Windows entry symbol, Game VA 0x566160, RVA 0x166160).
///
/// Pure prefix of the 84-byte contiguous body [0x566160-0x5661b3]: after the
/// host supplies the module path bytes (native `GetModuleFileNameA` intake,
/// caller-provided here), the body runs `_strrchr(path, '\\')` and, only when
/// found, stores NUL at the hit (`*cp = 0`). Forward slashes are ordinary
/// bytes; a missing backslash leaves the buffer unchanged (the native call
/// then receives the bare filename); a trailing backslash is itself removed.
///
/// `path` carries the C-string payload without its terminator. Game SHA-256
/// 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
/// REA ev_6fcc2ee9e70e9a8843fa01f79ded45dec1a2ab398853a48495996374403c3c11
/// (Ghidra FUN_00566160, 84B, 26 asm, callees GetModuleFileNameA, _strrchr
/// 0x610760, FUN_0061d13c, FUN_005bebe0; re-observed this session);
/// mafia-re 113b5b6 game/WinMain.cpp:9-21.
///
/// DEFERRED halt points past this chop (no tree owners, no dispatch here):
/// `_chdir` (FUN_0061d13c, VA 0x61d13c, RVA 0x21d13c; REA
/// ev_c54b6ac668ae5842ac77f37b96e623f6ca05b2a3273b203332f9d8c6690c773c:
/// `SetCurrentDirectoryA` plus drive-letter `SetEnvironmentVariableA`) has no
/// host current-directory backend, and `main` (FUN_005bebe0, VA 0x5bebe0,
/// 2276B [0x5bebe0-0x5bf4c3]) is not reconstructed: only its Init/Close
/// fragments exist as real `original_system::SystemState` owners, so a full
/// WinMain-to-main dispatch stays deferred and claims no function credit
/// beyond this chop.
pub fn winmain_exe_dir_prefix(path: &mut Vec<u8>) {
    if let Some(cut) = path.iter().rposition(|byte| *byte == b'\\') {
        path.truncate(cut);
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
    fn winmain_chop_keeps_native_strrchr_backslash_semantics() {
        let vectors: &[(&[u8], &[u8])] = &[
            (b"C:\\Mafia\\Game.exe", b"C:\\Mafia"),
            (b"A\\B\\C.exe", b"A\\B"),
            (b"C:\\Mafia\\", b"C:\\Mafia"),
            (b"C:\\", b"C:"),
            (b"Game.exe", b"Game.exe"),
            (b"C:/Mafia/Game.exe", b"C:/Mafia/Game.exe"),
            (b"", b""),
        ];
        for (input, expected) in vectors {
            let mut path = input.to_vec();
            winmain_exe_dir_prefix(&mut path);
            assert_eq!(path, expected.to_vec(), "input {input:?}");
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
