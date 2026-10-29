//! Host composition for the original car level-table dependencies. Handles
//! retain the actual selected original asset and cursor across native calls.
//! Frame, actor, audio and world effects remain with the inner live backend.
use super::*;
use std::collections::BTreeMap;

struct LevelTableHandle {
    bytes: Vec<u8>,
    position: u64,
}

#[derive(Default)]
pub(super) struct CarRuntimeGameState {
    handles: BTreeMap<u32, LevelTableHandle>,
    next_handle: u32,
}
impl CarRuntimeGameState {
    pub(super) fn open_handles(&self) -> usize {
        self.handles.len()
    }
    /// Releases host file buffers after a stopped owner is retired. This is
    /// host cleanup, not a claim that original GameDone closed leaked handles.
    pub(super) fn release_host_handles(&mut self) {
        self.handles.clear();
    }
}

#[derive(Debug)]
pub(super) enum CarRuntimeGameError<E> {
    Backend(E),
    Asset(String),
    InvalidPath,
    OpenFlags(u32),
    Handle(u32),
    HandleCapacity,
    Helper {
        entry: u32,
        failure: Box<CarGameInitUnavailable<CarRuntimeGameError<E>>>,
    },
    SourceParameters(Box<VehicleWheelsUnavailable<CarRuntimeGameError<E>>>),
}
impl<E: From<crate::vehicle_init::VehicleUninitializedStorage>>
    From<crate::vehicle_init::VehicleUninitializedStorage> for CarRuntimeGameError<E>
{
    fn from(error: crate::vehicle_init::VehicleUninitializedStorage) -> Self {
        Self::Backend(E::from(error))
    }
}

impl<E: std::fmt::Debug> std::fmt::Display for CarRuntimeGameError<E> {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Backend(error) => write!(output, "GameInit backend: {error:?}"),
            Self::Asset(error) => write!(output, "GameInit original asset: {error}"),
            Self::InvalidPath => output.write_str("GameInit original path unavailable"),
            Self::OpenFlags(flags) => write!(output, "GameInit file flags unavailable: {flags:#x}"),
            Self::Handle(handle) => write!(output, "GameInit file handle unavailable: {handle:#x}"),
            Self::HandleCapacity => output.write_str("GameInit host handle capacity exhausted"),
            Self::Helper { entry, failure } => {
                write!(output, "GameInit helper {entry:#x}: {failure:?}")
            }
            Self::SourceParameters(failure) => {
                write!(output, "GameInit source parameters: {failure:?}")
            }
        }
    }
}

pub(super) struct CarRuntimeGameBackend<'a, B> {
    inner: &'a mut B,
    installation: &'a lhr_assets::Installation,
    state: &'a mut CarRuntimeGameState,
    /// Native ESP after the outer C_car::GameInit prologue. Each support
    /// dependency receives the stack after its own actual prologue below.
    stack: u32,
}
impl<'a, B> CarRuntimeGameBackend<'a, B> {
    pub(super) fn new(
        inner: &'a mut B,
        installation: &'a lhr_assets::Installation,
        state: &'a mut CarRuntimeGameState,
        stack: u32,
    ) -> Self {
        Self {
            inner,
            installation,
            state,
            stack,
        }
    }
}

fn init_error<E>(
    error: VehicleInitUnavailable<E>,
) -> VehicleInitUnavailable<CarRuntimeGameError<E>> {
    match error {
        VehicleInitUnavailable::Storage(address) => VehicleInitUnavailable::Storage(address),
        VehicleInitUnavailable::Effect(error) => {
            VehicleInitUnavailable::Effect(CarRuntimeGameError::Backend(error))
        }
        VehicleInitUnavailable::AlreadyStarted => VehicleInitUnavailable::AlreadyStarted,
        VehicleInitUnavailable::ProjectionBackendUnavailable => {
            VehicleInitUnavailable::ProjectionBackendUnavailable
        }
    }
}

impl<B: VehicleInitBackend> VehicleInitBackend for CarRuntimeGameBackend<'_, B> {
    type Error = CarRuntimeGameError<B::Error>;
    fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
        // Immutable original literal, not an invented host pathname. The
        // generic frame adapter does not otherwise need this Game literal.
        if let Some(index) = address.checked_sub(0x63236c) {
            if let Some(byte) = b"tables\\vehicles.bin\0".get(index as usize) {
                return Ok(*byte);
            }
        }
        self.inner
            .read_byte(address)
            .map_err(CarRuntimeGameError::Backend)
    }
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
        self.inner
            .read_word(address)
            .map_err(CarRuntimeGameError::Backend)
    }
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
        self.inner
            .write_byte(address, value)
            .map_err(CarRuntimeGameError::Backend)
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
        self.inner
            .write_word(address, value)
            .map_err(CarRuntimeGameError::Backend)
    }
    fn free(&mut self, allocation: u32) -> Result<(), Self::Error> {
        self.inner
            .free(allocation)
            .map_err(CarRuntimeGameError::Backend)
    }
    fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
        self.inner
            .allocate(bytes)
            .map_err(CarRuntimeGameError::Backend)
    }
    fn allocate_projected(
        &mut self,
        memory: &mut VehicleComponentTailMemory<'_>,
        bytes: u32,
    ) -> Result<u32, Self::Error> {
        self.inner
            .allocate_projected(memory, bytes)
            .map_err(CarRuntimeGameError::Backend)
    }
    fn free_projected(
        &mut self,
        memory: &mut VehicleComponentTailMemory<'_>,
        allocation: u32,
    ) -> Result<(), Self::Error> {
        self.inner
            .free_projected(memory, allocation)
            .map_err(CarRuntimeGameError::Backend)
    }
    fn collect_frame(
        &mut self,
        vector: &mut VehicleInitFrameVector,
        frame: u32,
    ) -> Result<(), Self::Error> {
        self.inner
            .collect_frame(vector, frame)
            .map_err(CarRuntimeGameError::Backend)
    }
    fn collect_frame_projected(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        header: u32,
        frame: u32,
    ) -> Result<u32, VehicleInitUnavailable<Self::Error>> {
        self.inner
            .collect_frame_projected(memory, header, frame)
            .map_err(init_error)
    }
    fn enumerate(
        &mut self,
        model: u32,
        query: VehicleInitQuery,
        target: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.inner
            .enumerate(model, query, target)
            .map_err(init_error)
    }
}

impl<B: VehicleSourceParametersBackend> VehicleSourceParametersBackend
    for CarRuntimeGameBackend<'_, B>
{
    fn parameter_update_frame_world(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.inner
            .parameter_update_frame_world(frame)
            .map_err(CarRuntimeGameError::Backend)
    }
    fn parameter_frame_world_scale(&mut self, frame: u32) -> Result<[u32; 3], Self::Error> {
        self.inner
            .parameter_frame_world_scale(frame)
            .map_err(CarRuntimeGameError::Backend)
    }
    fn parameter_initialize_sounds(
        &mut self,
        memory: Option<VehicleComponentTailMemory<'_>>,
        vehicle: u32,
        source: u32,
    ) -> Result<(), Self::Error> {
        self.inner
            .parameter_initialize_sounds(memory, vehicle, source)
            .map_err(CarRuntimeGameError::Backend)
    }
}

impl<B: CarGameInitSupportBackend + VehicleSourceParametersBackend> CarGameInitBackend
    for CarRuntimeGameBackend<'_, B>
{
    fn game_init_effect(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: CarGameInitEffect,
    ) -> Result<u32, Self::Error> {
        if let CarGameInitEffect::CarDependency {
            entry,
            car,
            argument,
        } = effect
        {
            // Saved registers/local allocation are part of each original
            // prologue. The caller's PUSH arguments and CALL also consume ESP.
            let decrement = match entry {
                0x41e270 => Some(0x22b4),
                0x41ea00 => Some(0x18),
                0x429bb0 => Some(0xc),
                0x423340 => Some(0x30),
                0x407cd0 => Some(0xc),
                _ => None,
            };
            if let Some(decrement) = decrement {
                return car_game_init_support(
                    self,
                    memory,
                    car,
                    self.stack.wrapping_sub(decrement),
                    entry,
                    argument,
                )
                .map_err(|failure| CarRuntimeGameError::Helper {
                    entry,
                    failure: Box::new(failure),
                });
            }
        }
        self.inner
            .game_init_effect(memory, effect)
            .map_err(CarRuntimeGameError::Backend)
    }
}

impl<B: CarGameInitSupportBackend + VehicleSourceParametersBackend> CarGameInitSupportBackend
    for CarRuntimeGameBackend<'_, B>
{
    fn car_game_init_support_effect(
        &mut self,
        mut memory: VehicleComponentTailMemory<'_>,
        effect: CarGameInitSupportEffect,
    ) -> Result<u32, Self::Error> {
        match effect {
            CarGameInitSupportEffect::OpenLevelTable { path, flags } => {
                if flags != 0 {
                    return Err(CarRuntimeGameError::OpenFlags(flags));
                }
                let mut name = Vec::new();
                for i in 0..32768u32 {
                    let byte = memory.read_byte_with_backend(self, path.wrapping_add(i))?;
                    if byte == 0 {
                        break;
                    }
                    if !byte.is_ascii() {
                        return Err(CarRuntimeGameError::InvalidPath);
                    }
                    name.push(byte);
                    if i == 32767 {
                        return Err(CarRuntimeGameError::InvalidPath);
                    }
                }
                let name =
                    std::str::from_utf8(&name).map_err(|_| CarRuntimeGameError::InvalidPath)?;
                let bytes = match self.installation.read_original_path(name) {
                    Ok(bytes) => bytes,
                    Err(error) if error.0 == "original asset unavailable" => return Ok(u32::MAX),
                    Err(error) => return Err(CarRuntimeGameError::Asset(error.0)),
                };
                let handle = self
                    .state
                    .next_handle
                    .checked_add(1)
                    .filter(|&handle| handle != u32::MAX)
                    .ok_or(CarRuntimeGameError::HandleCapacity)?;
                self.state.next_handle = handle;
                self.state
                    .handles
                    .insert(handle, LevelTableHandle { bytes, position: 0 });
                Ok(handle)
            }
            CarGameInitSupportEffect::ReadLevelTable {
                handle,
                target,
                bytes,
            } => {
                let file = self
                    .state
                    .handles
                    .get(&handle)
                    .ok_or(CarRuntimeGameError::Handle(handle))?;
                let remaining = (file.bytes.len() as u64).saturating_sub(file.position);
                let count = u64::from(bytes).min(remaining) as u32;
                for offset in 0..count {
                    let file = self.state.handles.get(&handle).unwrap();
                    let byte = file.bytes[file.position as usize];
                    memory.write_byte_with_backend(self, target.wrapping_add(offset), byte)?;
                    self.state.handles.get_mut(&handle).unwrap().position += 1;
                }
                Ok(count)
            }
            CarGameInitSupportEffect::SeekLevelTable {
                handle,
                offset,
                origin,
            } => {
                let file = self
                    .state
                    .handles
                    .get_mut(&handle)
                    .ok_or(CarRuntimeGameError::Handle(handle))?;
                let base = match origin {
                    0 => 0i64,
                    1 => i64::try_from(file.position)
                        .map_err(|_| CarRuntimeGameError::Handle(handle))?,
                    2 => file.bytes.len() as i64,
                    _ => return Ok(u32::MAX),
                };
                let Some(position) = base
                    .checked_add(i64::from(offset as i32))
                    .filter(|&position| position >= 0)
                else {
                    return Ok(u32::MAX);
                };
                file.position = position as u64;
                Ok(0)
            }
            CarGameInitSupportEffect::CloseLevelTable { handle } => {
                self.state
                    .handles
                    .remove(&handle)
                    .ok_or(CarRuntimeGameError::Handle(handle))?;
                Ok(0)
            }
            CarGameInitSupportEffect::ApplySourceParameters {
                vehicle,
                source,
                flags,
            } => car_apply_level_source_parameters(self, memory, vehicle, source, flags)
                .map(u32::from)
                .map_err(|failure| CarRuntimeGameError::SourceParameters(Box::new(failure))),
            _ => self
                .inner
                .car_game_init_support_effect(memory, effect)
                .map_err(CarRuntimeGameError::Backend),
        }
    }
}

impl<B> CarGameDoneBackend for CarRuntimeGameBackend<'_, B>
where
    B: CarGameDoneBackend + CarGameInitSupportBackend + VehicleSourceParametersBackend,
{
    fn game_done_effect(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: CarGameDoneEffect,
    ) -> Result<(), Self::Error> {
        self.inner
            .game_done_effect(memory, effect)
            .map_err(CarRuntimeGameError::Backend)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Host(VehicleFrameMemory);
    impl VehicleInitBackend for Host {
        type Error = VehicleMemoryError;
        fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
            self.0.read_byte(a)
        }
        fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
            self.0.read_word(a)
        }
        fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Self::Error> {
            self.0.write_byte(a, v)
        }
        fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
            self.0.write_word(a, v)
        }
        fn free(&mut self, a: u32) -> Result<(), Self::Error> {
            self.0.free(a)
        }
        fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
            self.0.allocate(n)
        }
        fn collect_frame(
            &mut self,
            v: &mut VehicleInitFrameVector,
            f: u32,
        ) -> Result<(), Self::Error> {
            self.0.collect_frame(v, f)
        }
        fn enumerate(
            &mut self,
            m: u32,
            q: VehicleInitQuery,
            t: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            self.0.enumerate(m, q, t)
        }
    }
    impl CarGameInitBackend for Host {
        fn game_init_effect(
            &mut self,
            _: VehicleComponentTailMemory<'_>,
            _: CarGameInitEffect,
        ) -> Result<u32, Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
    }
    impl CarGameInitSupportBackend for Host {
        fn car_game_init_support_effect(
            &mut self,
            _: VehicleComponentTailMemory<'_>,
            _: CarGameInitSupportEffect,
        ) -> Result<u32, Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
    }
    impl VehicleSourceParametersBackend for Host {
        fn parameter_update_frame_world(&mut self, _: u32) -> Result<(), Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn parameter_frame_world_scale(&mut self, _: u32) -> Result<[u32; 3], Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn parameter_initialize_sounds(
            &mut self,
            _: Option<VehicleComponentTailMemory<'_>>,
            _: u32,
            _: u32,
        ) -> Result<(), Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
    }
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn installation() -> (Directory, lhr_assets::Installation) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lhr-car-table-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join("tables")).unwrap();
        std::fs::write(path.join("tables/vehicles.bin"), b"abcdefgh").unwrap();
        let original = lhr_assets::Installation::open(&path).unwrap();
        (Directory(path), original)
    }
    fn car() -> (VehicleInitState, VehicleComponentTailScratch) {
        let car = VehicleInitState::from_car_bytes(
            0x10000,
            vec![0; 0x221c],
            CarInitScratch {
                address: 0x800000,
                bytes: [0; CAR_INIT_STACK_BYTES],
            },
            CarInitRegisters {
                ebx: 0,
                ebp: 0,
                esi: 0,
                edi: 0,
            },
        )
        .unwrap();
        (
            car,
            VehicleComponentTailScratch {
                address: 0x810000,
                bytes: [0; VEHICLE_INIT_STACK_BYTES],
            },
        )
    }
    #[test]
    fn original_file_handles_keep_independent_cursors_and_short_reads_leave_tail_untouched() {
        let (_directory, installation) = installation();
        let mut state = CarRuntimeGameState::default();
        let mut host = Host(VehicleFrameMemory::default());
        host.0.map_bytes(0x600000, &[0xa5; 16]).unwrap();
        let mut backend =
            CarRuntimeGameBackend::new(&mut host, &installation, &mut state, 0x810000);
        let (mut car, mut scratch) = car();
        let mut memory = car.car_live_memory(&mut scratch).unwrap();
        let mut effect = |backend: &mut CarRuntimeGameBackend<'_, Host>, effect| {
            backend
                .car_game_init_support_effect(memory.reborrow(), effect)
                .unwrap()
        };
        let first = effect(
            &mut backend,
            CarGameInitSupportEffect::OpenLevelTable {
                path: 0x63236c,
                flags: 0,
            },
        );
        let second = effect(
            &mut backend,
            CarGameInitSupportEffect::OpenLevelTable {
                path: 0x63236c,
                flags: 0,
            },
        );
        assert_ne!(first, second);
        assert_eq!(
            effect(
                &mut backend,
                CarGameInitSupportEffect::SeekLevelTable {
                    handle: first,
                    offset: 6,
                    origin: 0
                }
            ),
            0
        );
        assert_eq!(
            effect(
                &mut backend,
                CarGameInitSupportEffect::ReadLevelTable {
                    handle: first,
                    target: 0x600000,
                    bytes: 8
                }
            ),
            2
        );
        assert_eq!(
            effect(
                &mut backend,
                CarGameInitSupportEffect::ReadLevelTable {
                    handle: second,
                    target: 0x600008,
                    bytes: 3
                }
            ),
            3
        );
        assert_eq!(
            backend.read_word(0x600000).unwrap(),
            u32::from_le_bytes([b'g', b'h', 0xa5, 0xa5])
        );
        assert_eq!(
            backend.read_word(0x600008).unwrap(),
            u32::from_le_bytes([b'a', b'b', b'c', 0xa5])
        );
        effect(
            &mut backend,
            CarGameInitSupportEffect::CloseLevelTable { handle: first },
        );
        assert_eq!(backend.state.open_handles(), 1);
        assert!(
            matches!(backend.car_game_init_support_effect(memory.reborrow(),CarGameInitSupportEffect::CloseLevelTable {handle:first}),Err(CarRuntimeGameError::Handle(h)) if h==first)
        );
        backend.state.release_host_handles();
        assert_eq!(backend.state.open_handles(), 0);
    }
    #[test]
    fn unavailable_destination_retains_file_and_completed_write_prefix() {
        let (_directory, installation) = installation();
        let mut state = CarRuntimeGameState::default();
        let mut host = Host(VehicleFrameMemory::default());
        host.0.map_bytes(0x600000, &[0xa5; 2]).unwrap();
        let mut backend =
            CarRuntimeGameBackend::new(&mut host, &installation, &mut state, 0x810000);
        let (mut car, mut scratch) = car();
        let mut memory = car.car_live_memory(&mut scratch).unwrap();
        let handle = backend
            .car_game_init_support_effect(
                memory.reborrow(),
                CarGameInitSupportEffect::OpenLevelTable {
                    path: 0x63236c,
                    flags: 0,
                },
            )
            .unwrap();
        assert!(backend
            .car_game_init_support_effect(
                memory.reborrow(),
                CarGameInitSupportEffect::ReadLevelTable {
                    handle,
                    target: 0x600000,
                    bytes: 4
                }
            )
            .is_err());
        assert_eq!(backend.read_byte(0x600000).unwrap(), b'a');
        assert_eq!(backend.read_byte(0x600001).unwrap(), b'b');
        assert_eq!(backend.state.handles[&handle].position, 2);
        assert_eq!(backend.state.open_handles(), 1);
    }
}
