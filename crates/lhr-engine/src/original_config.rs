//! Recovered packed configuration shared by the supplied Setup and Game builds.
//!
//! Setup SHA256 c6abff923ecedafc26be3f1abdb8fe286f3a0be1e787ea7b659b6c01a077d25d:
//! defaults RVA0x2d00, video-memory probe RVA0x2e00, configuration branches of
//! RVA0x10a0/0x14f0 and format-choice behavior of RVA0x35f0. Game defaults are
//! RVA0x1bf520, SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! Names below are descriptive. No Windows UI, driver enumeration or registry
//! implementation is supplied; the host binds storage and capability backends.

pub const CONFIG_BYTES: usize = 63;
pub const SETTINGS_KEY: &str = "Software\\Illusion Softworks\\Mafia";
pub const SETTINGS_VALUE: &str = "LS3D_setup";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginalConfig {
    bytes: [u8; CONFIG_BYTES],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConfigSizeError;
impl Default for OriginalConfig {
    fn default() -> Self {
        let mut config = Self {
            bytes: [0; CONFIG_BYTES],
        };
        config.bytes[0] = 20;
        for (offset, value) in [
            (2, 800),
            (6, 600),
            (10, 32),
            (39, 0x3f800000),
            (43, 0x3f800000),
            (47, 2),
            (51, 2),
            (55, 3),
            (59, 2),
        ] {
            config.set_dword(offset, value);
        }
        for offset in [22, 26, 27, 30, 31, 33, 38] {
            config.bytes[offset] = 1;
        }
        config
    }
}
impl OriginalConfig {
    /// Lossless storage admission, not validation for a renderer or driver. The
    /// original load branch can accept an incoming version byte other than20.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ConfigSizeError> {
        Ok(Self {
            bytes: bytes.try_into().map_err(|_| ConfigSizeError)?,
        })
    }
    pub fn as_bytes(&self) -> &[u8; CONFIG_BYTES] {
        &self.bytes
    }
    pub fn into_bytes(self) -> [u8; CONFIG_BYTES] {
        self.bytes
    }
    pub fn version(&self) -> u8 {
        self.bytes[0]
    }
    pub fn adapter_index(&self) -> u8 {
        self.bytes[1]
    }
    pub fn width(&self) -> u32 {
        self.dword(2)
    }
    pub fn height(&self) -> u32 {
        self.dword(6)
    }
    pub fn color_depth(&self) -> u32 {
        self.dword(10)
    }
    pub fn refresh_rate(&self) -> u32 {
        self.dword(14)
    }
    /// Original choice codes: English0, Czech1, German2. Unknown values are
    /// retained; they are not silently converted into a default language.
    pub fn language_code(&self) -> u32 {
        self.dword(34)
    }
    fn dword(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }
    fn set_dword(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    /// The recovered profile-selection branch only. Actual display enumeration
    /// and later mode validation remain separate requirements. The signed tier
    /// comparisons are preserved; the normal probe produces0..4096.
    pub fn apply_memory_profile(&mut self, tier: i32) {
        for (offset, value) in [
            (19, 0),
            (31, 1),
            (20, 0),
            (18, 0),
            (21, 1),
            (22, 1),
            (23, 0),
            (29, 0),
            (32, 0),
        ] {
            self.bytes[offset] = value;
        }
        self.set_dword(14, 0);
        let (width, height, depth, fields) = if tier < 9 {
            (640, 480, 16, [0, 0, 1, 1])
        } else if tier < 17 {
            (800, 600, 16, [1, 0, 1, 1])
        } else if tier < 33 {
            (800, 600, 32, [1, 0, 0, 1])
        } else if tier < 65 {
            (1024, 768, 32, [1, 0, 0, 1])
        } else {
            (1280, 1024, 32, [1, 1, 0, 0])
        };
        self.set_dword(2, width);
        self.set_dword(6, height);
        self.set_dword(10, depth);
        for (offset, value) in [26, 28, 24, 27].into_iter().zip(fields) {
            self.bytes[offset] = value;
        }
    }
    /// Apply one valid, already-selected native mode's mask. Only the stored
    /// 16/32 depth is clamped. Antialiasing remains unchanged in the source blob:
    /// native control-selection requests alone do not update that stored byte.
    /// An empty list can still receive request0; this is not a supported mode.
    pub fn mode_choices(&mut self, mask: u32) -> ModeChoices {
        let mut depths = Vec::with_capacity(2);
        if mask & 1 != 0 {
            depths.push(16);
            if mask & 2 == 0 && self.color_depth() == 32 {
                self.set_dword(10, 16);
            }
        }
        if mask & 2 != 0 {
            depths.push(32);
            if mask & 1 == 0 && self.color_depth() == 16 {
                self.set_dword(10, 32);
            }
        }
        let requested_depth = usize::from(self.color_depth() != 16 && mask & 3 == 3);
        // This raw family byte selects native mask bits4/8 versus16/32. Its
        // broader renderer interpretation is not established by this adapter.
        let family = self.bytes[21] != 0;
        let pair = if family { 0x30 } else { 0x0c };
        let stored_aa = self.bytes[18];
        let mut requested_antialiasing = if stored_aa == 0 || mask & pair != pair {
            Some(0)
        } else {
            None
        };
        let mut antialiasing = vec![0];
        for (flag, value) in if family {
            [(0x10, 2), (0x20, 4)]
        } else {
            [(4, 2), (8, 4)]
        } {
            if mask & flag != 0 {
                antialiasing.push(value);
                if u32::from(stored_aa) == value {
                    requested_antialiasing = Some(antialiasing.len() - 1);
                }
            }
        }
        ModeChoices {
            depths,
            requested_depth,
            antialiasing,
            requested_antialiasing,
        }
    }
    /// Native configuration read policy. Query starts with defaults and closes
    /// the opened key even on query failure. Type is deliberately ignored. The
    /// unusual version test reads the CURRENT config, not incoming query bytes.
    pub fn load<B: SettingsStore>(&mut self, store: &mut B) -> bool {
        let Ok(handle) = store.open_read(SETTINGS_KEY) else {
            return false;
        };
        let mut incoming = Self::default().bytes;
        let query = store.query(handle, SETTINGS_VALUE, &mut incoming);
        store.close(handle);
        if query.status == 0 && query.returned_bytes == CONFIG_BYTES as u32 && self.version() == 20
        {
            self.bytes = incoming;
            true
        } else {
            false
        }
    }
    /// Settings branch of the original accept action. Close follows every write
    /// attempt, including failure. The original UI closes even on persistence
    /// failure; returned status here is an explicit host observation convenience.
    pub fn save<B: SettingsStore>(&self, store: &mut B) -> SaveStatus {
        let handle = match store.create_write(SETTINGS_KEY, 0xf003f) {
            Ok(handle) => handle,
            Err(status) => {
                return SaveStatus {
                    create_status: status,
                    write_status: None,
                }
            }
        };
        let status = store.write(handle, SETTINGS_VALUE, 3, &self.bytes);
        store.close(handle);
        SaveStatus {
            create_status: 0,
            write_status: Some(status),
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct ModeChoices {
    pub depths: Vec<u32>,
    pub requested_depth: usize,
    pub antialiasing: Vec<u32>,
    pub requested_antialiasing: Option<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryStatus {
    pub status: u32,
    pub returned_bytes: u32,
    pub value_type: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveStatus {
    pub create_status: u32,
    pub write_status: Option<u32>,
}
/// The backend owns handles and must bind actual storage semantics. No implicit
/// fallback, fabricated registry content or no-op persistence is supplied.
/// Map native status0 to Ok(handle), and every nonzero open/create status to Err.
pub trait SettingsStore {
    type Handle: Copy;
    fn open_read(&mut self, key: &str) -> Result<Self::Handle, u32>;
    fn query(
        &mut self,
        handle: Self::Handle,
        name: &str,
        data: &mut [u8; CONFIG_BYTES],
    ) -> QueryStatus;
    fn create_write(&mut self, key: &str, access: u32) -> Result<Self::Handle, u32>;
    fn write(
        &mut self,
        handle: Self::Handle,
        name: &str,
        value_type: u32,
        data: &[u8; CONFIG_BYTES],
    ) -> u32;
    fn close(&mut self, handle: Self::Handle);
}
/// Required default-adapter probe callbacks. The query caps are the original
/// four DWORDs. Resource ownership stays with this backend; no callback defaults.
/// Negative signed HRESULTs map to Err; nonnegative results require valid outputs.
pub trait VideoMemoryProbe {
    type Handle: Copy;
    type Error;
    fn create_default(&mut self) -> Result<Self::Handle, Self::Error>;
    fn query_memory(&mut self, handle: Self::Handle, caps: [u32; 4]) -> Result<u32, Self::Error>;
    fn release(&mut self, handle: Self::Handle);
}
/// RVA0x2e00 callback/value contract. Native failure returns0. A successful
/// sub-MiB report returns1; the integral MiB count is rounded UP to a power of2.
/// Native query failure does not call Release; backend lifetime cleanup is a
/// separate responsibility and must not be confused with this observed sequence.
pub fn probe_memory_tier<B: VideoMemoryProbe>(backend: &mut B) -> u32 {
    let Ok(handle) = backend.create_default() else {
        return 0;
    };
    let Ok(bytes) = backend.query_memory(handle, [0x10000000, 0, 0, 0]) else {
        return 0;
    };
    backend.release(handle);
    let mb = bytes >> 20;
    mb.max(1).next_power_of_two()
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Store {
        bytes: [u8; CONFIG_BYTES],
        open: u32,
        query: QueryStatus,
        create: u32,
        write: u32,
        events: Vec<u8>,
    }
    impl Default for Store {
        fn default() -> Self {
            Self {
                bytes: [7; CONFIG_BYTES],
                open: 0,
                query: QueryStatus {
                    status: 0,
                    returned_bytes: 63,
                    value_type: 3,
                },
                create: 0,
                write: 0,
                events: vec![],
            }
        }
    }
    impl SettingsStore for Store {
        type Handle = u32;
        fn open_read(&mut self, key: &str) -> Result<u32, u32> {
            assert_eq!(key, SETTINGS_KEY);
            self.events.push(2);
            if self.open == 0 {
                Ok(99)
            } else {
                Err(self.open)
            }
        }
        fn query(&mut self, handle: u32, name: &str, data: &mut [u8; 63]) -> QueryStatus {
            assert_eq!(handle, 99);
            assert_eq!(name, SETTINGS_VALUE);
            self.events.push(3);
            data.copy_from_slice(&self.bytes);
            self.query
        }
        fn create_write(&mut self, key: &str, access: u32) -> Result<u32, u32> {
            assert_eq!(key, SETTINGS_KEY);
            assert_eq!(access, 0xf003f);
            self.events.push(6);
            if self.create == 0 {
                Ok(99)
            } else {
                Err(self.create)
            }
        }
        fn write(&mut self, handle: u32, name: &str, value_type: u32, data: &[u8; 63]) -> u32 {
            assert_eq!(handle, 99);
            assert_eq!(name, SETTINGS_VALUE);
            assert_eq!(value_type, 3);
            self.events.push(7);
            self.bytes = *data;
            self.write
        }
        fn close(&mut self, handle: u32) {
            assert_eq!(handle, 99);
            self.events.push(4);
        }
    }
    #[test]
    fn defaults_and_lossless_packed_admission() {
        let c = OriginalConfig::default();
        assert_eq!(
            (
                c.version(),
                c.adapter_index(),
                c.width(),
                c.height(),
                c.color_depth(),
                c.refresh_rate(),
                c.language_code()
            ),
            (20, 0, 800, 600, 32, 0, 0)
        );
        let raw = std::array::from_fn::<_, 63, _>(|i| (i as u8).wrapping_mul(13));
        assert_eq!(OriginalConfig::from_bytes(&raw).unwrap().into_bytes(), raw);
        for size in [0, 62, 64] {
            assert!(OriginalConfig::from_bytes(&vec![0; size]).is_err());
        }
    }
    #[test]
    fn incoming_version_and_type_are_not_current_version_guard() {
        let mut c = OriginalConfig::default();
        let mut b = Store::default();
        b.query.value_type = 1;
        assert!(c.load(&mut b));
        assert_eq!(c.version(), 7);
        assert_eq!(c.as_bytes(), &[7; 63]);
        assert_eq!(b.events, [2, 3, 4]);
        b.events.clear();
        assert!(!c.load(&mut b));
        assert_eq!(b.events, [2, 3, 4]);
        assert_eq!(c.as_bytes(), &[7; 63]);
    }
    #[test]
    fn failed_reads_and_wrong_lengths_preserve_current_state() {
        for (open, status, len, events) in [
            (2, 0, 63, vec![2]),
            (0, 234, 63, vec![2, 3, 4]),
            (0, 0, 62, vec![2, 3, 4]),
            (0, 0, 64, vec![2, 3, 4]),
        ] {
            let mut c = OriginalConfig::default();
            let before = c.clone();
            let mut b = Store {
                open,
                query: QueryStatus {
                    status,
                    returned_bytes: len,
                    value_type: 3,
                },
                ..Store::default()
            };
            assert!(!c.load(&mut b));
            assert_eq!(c, before);
            assert_eq!(b.events, events);
        }
    }
    #[test]
    fn writes_close_even_after_failure_and_preserve_all_bytes() {
        let c = OriginalConfig::from_bytes(&[0xa5; 63]).unwrap();
        for (create, write, expected) in [
            (0, 0, vec![6, 7, 4]),
            (0, 5, vec![6, 7, 4]),
            (2, 0, vec![6]),
        ] {
            let mut b = Store {
                create,
                write,
                ..Store::default()
            };
            let result = c.save(&mut b);
            assert_eq!(
                result,
                SaveStatus {
                    create_status: create,
                    write_status: if create == 0 { Some(write) } else { None }
                }
            );
            assert_eq!(b.events, expected);
            if create == 0 {
                assert_eq!(b.bytes, [0xa5; 63]);
            }
        }
    }
    #[test]
    fn original_signed_profile_boundaries() {
        for (tier, width, height, depth) in [
            (-1, 640, 480, 16),
            (8, 640, 480, 16),
            (9, 800, 600, 16),
            (16, 800, 600, 16),
            (17, 800, 600, 32),
            (32, 800, 600, 32),
            (33, 1024, 768, 32),
            (64, 1024, 768, 32),
            (65, 1280, 1024, 32),
        ] {
            let mut c = OriginalConfig::from_bytes(&[0xa5; 63]).unwrap();
            c.apply_memory_profile(tier);
            assert_eq!(
                (c.width(), c.height(), c.color_depth(), c.refresh_rate()),
                (width, height, depth, 0)
            );
            assert_eq!(c.version(), 0xa5);
            assert_eq!(c.language_code(), 0xa5a5a5a5);
        }
    }
    #[test]
    fn mask_clamps_depth_but_does_not_rewrite_stored_antialiasing() {
        let mut c = OriginalConfig::default();
        c.bytes[18] = 4;
        let choices = c.mode_choices(1);
        assert_eq!(c.color_depth(), 16);
        assert_eq!(choices.depths, [16]);
        assert_eq!(choices.antialiasing, [0]);
        assert_eq!(c.bytes[18], 4);
        c.bytes[21] = 1;
        let choices = c.mode_choices(0x32);
        assert_eq!(c.color_depth(), 32);
        assert_eq!(choices.depths, [32]);
        assert_eq!(choices.antialiasing, [0, 2, 4]);
        assert_eq!(choices.requested_antialiasing, Some(2));
        c.bytes[18] = 255;
        let choices = c.mode_choices(0x32);
        assert_eq!(choices.requested_antialiasing, None);
        let choices = c.mode_choices(0);
        assert!(choices.depths.is_empty());
        assert_eq!(choices.requested_depth, 0);
        assert_eq!(c.color_depth(), 32);
    }
    struct Probe {
        create_fails: bool,
        query_fails: bool,
        bytes: u32,
        events: Vec<u8>,
    }
    impl VideoMemoryProbe for Probe {
        type Handle = u32;
        type Error = ();
        fn create_default(&mut self) -> Result<u32, ()> {
            self.events.push(9);
            if self.create_fails {
                Err(())
            } else {
                Ok(1)
            }
        }
        fn query_memory(&mut self, h: u32, caps: [u32; 4]) -> Result<u32, ()> {
            assert_eq!(h, 1);
            assert_eq!(caps, [0x10000000, 0, 0, 0]);
            self.events.push(10);
            if self.query_fails {
                Err(())
            } else {
                Ok(self.bytes)
            }
        }
        fn release(&mut self, h: u32) {
            assert_eq!(h, 1);
            self.events.push(11);
        }
    }
    #[test]
    fn original_memory_rounding_and_asymmetric_release_order() {
        for (bytes, tier) in [
            (0, 1),
            (0xfffff, 1),
            (9 << 20, 16),
            (17 << 20, 32),
            (u32::MAX, 4096),
        ] {
            let mut b = Probe {
                create_fails: false,
                query_fails: false,
                bytes,
                events: vec![],
            };
            assert_eq!(probe_memory_tier(&mut b), tier);
            assert_eq!(b.events, [9, 10, 11]);
        }
        for (create_fails, query_fails, events) in
            [(true, false, vec![9]), (false, true, vec![9, 10])]
        {
            let mut b = Probe {
                create_fails,
                query_fails,
                bytes: 0,
                events: vec![],
            };
            assert_eq!(probe_memory_tier(&mut b), 0);
            assert_eq!(b.events, events);
        }
    }
}
