//! Shared original material/tyre resources for mission cars. Lazy host intake
//! uses the existing canonical compatibility allocations and typed global
//! publications. LoadTyres preserves reached effects and explicit boundaries.
#![forbid(unsafe_code)]
use crate::{
    driving_vehicle_tables::{
        convert_tyre_payload, materials, DrivingVehicleTables, InstallationTyreFile,
        RetainedTyreStream, TyreTableFile, MATERIAL_SOURCE_STRIDE,
    },
    vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError},
    vehicle_init::VehicleInitBackend,
};
use lhr_assets::Installation;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum VehicleTableError<E> {
    Host(E),
    MissingInstallation,
    Load(String),
    AllocationNull,
    Interrupted,
}

/// The reached native span. Error states keep their allocations/publications
/// and cannot be entered again; a completed call may be followed by a new call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TyreOperation {
    SetEmpty,
    Open,
    Read(TyreReadField),
    SourceSurfaceHeader,
    PayloadAllocation,
    Convert,
    SourceSurfaceAppend,
    PayloadRelease,
    SourceTyreAppend,
    FileClose,
    Replace,
    SourceRelease,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TyreReadField {
    Version,
    Count,
    Extent,
    Name,
    Id,
    SurfaceCount,
    Material,
    PayloadSize,
    Payload,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TyreLoadState {
    NeverStarted,
    At(TyreOperation),
    Complete(bool),
    Interrupted(TyreOperation),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TyreBoundary {
    ShortRead {
        field: TyreReadField,
        expected: usize,
        actual: usize,
    },
    ExtentOutsideOwnerDomain(u32),
    InvalidVector,
    NativeNullSurfaceHeader,
    UnownedAllocation(u32),
    UncertainRelease(u32),
    Projection(String),
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum VehicleTyreError<E> {
    Host(E),
    Boundary(TyreBoundary),
    AlreadyStarted,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LoadTyresError<E, F> {
    Owner(VehicleTyreError<E>),
    File(F),
}

// Compatibility storage only: these are the existing projection's actual
// allocations and vector publications, not another address-to-object map.
#[derive(Clone, Copy, Debug, Default)]
struct TableVector {
    begin: u32,
    end: u32,
    capacity: u32,
}
#[derive(Clone, Copy)]
enum VectorSlot {
    Tyres,
    SourceTyres,
    Surfaces(u32),
}
#[derive(Clone, Copy)]
enum VectorField {
    Begin,
    End,
    Capacity,
}
impl VectorField {
    fn offset(self) -> u32 {
        match self {
            Self::Begin => 4,
            Self::End => 8,
            Self::Capacity => 12,
        }
    }
}
#[derive(Clone, Copy)]
enum AppendKind {
    Helper,
    SetterSurface,
}
#[derive(Clone, Copy)]
enum RecordSource<'a> {
    Bytes(&'a [u8]),
    Memory(u32),
}
#[derive(Debug, Default)]
struct TyreStagingRow {
    id: u32,
    name: [u8; 32],
    source_extent: u32,
    surfaces: u32,
}
impl TyreStagingRow {
    fn record(&self) -> [u8; 0x28] {
        let mut row = [0; 0x28];
        row[..4].copy_from_slice(&self.id.to_le_bytes());
        row[4..36].copy_from_slice(&self.name);
        row[36..].copy_from_slice(&self.surfaces.to_le_bytes());
        row
    }
}
#[derive(Debug, Default)]
struct TyreTemporary {
    rows: TableVector,
    current: TyreStagingRow,
    payload: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Unloaded,
    Loading,
    Ready,
    Interrupted,
    Discarded,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableKind {
    SourceMaterials,
    Materials,
    #[cfg(test)]
    TyreSurfaces(usize),
    #[cfg(test)]
    TyreHeader(usize),
    #[cfg(test)]
    Tyres,
    TyreTemporary,
}
#[derive(Debug)]
struct TableLease {
    allocation: u32,
    bytes: u32,
    _kind: TableKind,
    release_started: bool,
}
#[derive(Debug)]
pub(crate) struct VehicleTableResources {
    phase: Phase,
    leases: Vec<TableLease>,
    source_materials: Option<u32>,
    source_material_count: Option<u32>,
    materials: Option<u32>,
    material_index: Option<[u8; 256]>,
    tyres_begin: Option<u32>,
    tyres_end: Option<u32>,
    tyres_capacity: Option<u32>,
    tyre_state: TyreLoadState,
    tyre_temporary: Option<TyreTemporary>,
    abandoned_tyre_loads: Vec<TyreTemporary>,
    retained_tyre_files: Vec<RetainedTyreStream>,
    discard_started: bool,
}
impl Default for VehicleTableResources {
    fn default() -> Self {
        Self {
            phase: Phase::Unloaded,
            leases: Vec::new(),
            source_materials: None,
            source_material_count: None,
            materials: None,
            material_index: None,
            tyres_begin: None,
            tyres_end: None,
            tyres_capacity: None,
            tyre_state: TyreLoadState::NeverStarted,
            tyre_temporary: None,
            abandoned_tyre_loads: Vec::new(),
            retained_tyre_files: Vec::new(),
            discard_started: false,
        }
    }
}
impl VehicleTableResources {
    pub(crate) fn contains(address: u32) -> bool {
        (0x63d8b0..0x63d9b0).contains(&address)
            || [0x63d9b4u32, 0x63d9c8, 0x63d9cc, 0x6d4c24, 0x6d4c28]
                .into_iter()
                .any(|base| (base..base + 4).contains(&address))
    }
    pub(crate) fn live_allocation_count(&self) -> usize {
        self.leases.len()
    }
    fn own<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        bytes: &[u8],
        kind: TableKind,
    ) -> Result<u32, VehicleTableError<B::Error>> {
        let size = u32::try_from(bytes.len())
            .map_err(|_| VehicleTableError::Load("Vehicle table allocation extent".into()))?;
        let allocation = host.allocate(size).map_err(VehicleTableError::Host)?;
        if allocation == 0 {
            return Err(VehicleTableError::AllocationNull);
        }
        // Record the real allocation before any required write can fail.
        self.leases.push(TableLease {
            allocation,
            bytes: size,
            _kind: kind,
            release_started: false,
        });
        for (offset, &byte) in bytes.iter().enumerate() {
            let address = allocation
                .checked_add(offset as u32)
                .ok_or_else(|| VehicleTableError::Load("Vehicle table address extent".into()))?;
            host.write_byte(address, byte)
                .map_err(VehicleTableError::Host)?;
        }
        Ok(allocation)
    }
    fn project_materials<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        source: &[u8],
        rows: &[u8],
        index: [u8; 256],
    ) -> Result<(), VehicleTableError<B::Error>> {
        let allocation = self.own(host, source, TableKind::SourceMaterials)?;
        self.source_materials = Some(allocation);
        self.source_material_count = Some((source.len() / MATERIAL_SOURCE_STRIDE) as u32);
        let allocation = self.own(host, rows, TableKind::Materials)?;
        self.material_index = Some(index);
        self.materials = Some(allocation);
        Ok(())
    }
    #[cfg(test)]
    fn project<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        tables: DrivingVehicleTables,
    ) -> Result<(), VehicleTableError<B::Error>> {
        self.project_materials(
            host,
            tables.source_material_bytes(),
            tables.material_bytes(),
            *tables.material_index_map(),
        )?;
        let mut tyre_rows = DrivingVehicleTables::user_tyre_record().to_vec();
        for (index, tyre) in tables.tyres().iter().enumerate() {
            let mut surfaces = Vec::with_capacity(tyre.surfaces.len() * 0x5c);
            for row in &tyre.surfaces {
                surfaces.extend_from_slice(row);
            }
            let begin = self.own(host, &surfaces, TableKind::TyreSurfaces(index))?;
            let end = begin
                .checked_add(surfaces.len() as u32)
                .ok_or_else(|| VehicleTableError::Load("Vehicle tyre surface extent".into()))?;
            let mut header = [0; 16];
            header[4..8].copy_from_slice(&begin.to_le_bytes());
            header[8..12].copy_from_slice(&end.to_le_bytes());
            header[12..16].copy_from_slice(&end.to_le_bytes());
            let header = self.own(host, &header, TableKind::TyreHeader(index))?;
            tyre_rows.extend_from_slice(&tyre.id.to_le_bytes());
            tyre_rows.extend_from_slice(&tyre.name_bytes);
            tyre_rows.extend_from_slice(&header.to_le_bytes());
        }
        let begin = self.own(host, &tyre_rows, TableKind::Tyres)?;
        self.tyres_begin = Some(begin);
        self.tyres_capacity = Some(begin + tyre_rows.len() as u32);
        self.tyres_end = Some(
            begin
                .checked_add(tyre_rows.len() as u32)
                .ok_or_else(|| VehicleTableError::Load("Vehicle tyre table extent".into()))?,
        );
        self.tyre_state = TyreLoadState::Complete(true);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn tyre_load_state(&self) -> TyreLoadState {
        self.tyre_state
    }

    fn tyre_begin(&mut self) -> Result<(), VehicleTyreError<()>> {
        if self.discard_started
            || matches!(
                self.tyre_state,
                TyreLoadState::At(_) | TyreLoadState::Interrupted(_)
            )
        {
            return Err(VehicleTyreError::AlreadyStarted);
        }
        self.tyre_state = TyreLoadState::At(TyreOperation::SetEmpty);
        Ok(())
    }
    fn tyre_owner_begin<E>(&mut self) -> Result<(), VehicleTyreError<E>> {
        self.tyre_begin()
            .map_err(|_| VehicleTyreError::AlreadyStarted)
    }
    fn tyre_finish<E>(&mut self, result: &Result<bool, E>) {
        if let TyreLoadState::At(operation) = self.tyre_state {
            self.tyre_state = match result {
                Ok(value) => TyreLoadState::Complete(*value),
                Err(_) => TyreLoadState::Interrupted(operation),
            };
        }
    }
    fn tyre_allocate<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        bytes: u32,
    ) -> Result<u32, VehicleTyreError<B::Error>> {
        // Host availability bound, never a native false result. No native
        // count is reinterpreted as signed-positive or clamped to this bound.
        if bytes > 4 * 1024 * 1024 {
            return Err(VehicleTyreError::Boundary(
                TyreBoundary::ExtentOutsideOwnerDomain(bytes),
            ));
        }
        let allocation = host.allocate(bytes).map_err(VehicleTyreError::Host)?;
        if allocation != 0 {
            self.leases.push(TableLease {
                allocation,
                bytes,
                _kind: TableKind::TyreTemporary,
                release_started: false,
            });
        }
        Ok(allocation)
    }
    fn tyre_free<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        allocation: u32,
    ) -> Result<(), VehicleTyreError<B::Error>> {
        if allocation == 0 {
            return host.free(0).map_err(VehicleTyreError::Host);
        }
        let index = self
            .leases
            .iter()
            .position(|lease| lease.allocation == allocation)
            .ok_or(VehicleTyreError::Boundary(TyreBoundary::UnownedAllocation(
                allocation,
            )))?;
        if self.leases[index].release_started {
            return Err(VehicleTyreError::Boundary(TyreBoundary::UncertainRelease(
                allocation,
            )));
        }
        // Consume before calling the destructive backend. Err may mean that
        // the free already happened. Neither native retry nor host discard
        // may replay that uncertain call.
        self.leases[index].release_started = true;
        host.free(allocation).map_err(VehicleTyreError::Host)?;
        self.leases.remove(index);
        Ok(())
    }
    fn tyre_vector_field<B: VehicleInitBackend>(
        &self,
        host: &mut B,
        slot: VectorSlot,
        field: VectorField,
    ) -> Result<u32, VehicleTyreError<B::Error>> {
        Ok(match slot {
            VectorSlot::Tyres => match field {
                VectorField::Begin => self.tyres_begin.unwrap_or(0),
                VectorField::End => self.tyres_end.unwrap_or(0),
                VectorField::Capacity => self.tyres_capacity.unwrap_or(0),
            },
            VectorSlot::SourceTyres => {
                let vector = self.tyre_temporary.as_ref().expect("active tyre load").rows;
                match field {
                    VectorField::Begin => vector.begin,
                    VectorField::End => vector.end,
                    VectorField::Capacity => vector.capacity,
                }
            }
            VectorSlot::Surfaces(header) => {
                if header == 0 {
                    return Err(VehicleTyreError::Boundary(
                        TyreBoundary::NativeNullSurfaceHeader,
                    ));
                }
                host.read_word(header.wrapping_add(field.offset()))
                    .map_err(VehicleTyreError::Host)?
            }
        })
    }
    fn tyre_vector_store<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        slot: VectorSlot,
        field: VectorField,
        value: u32,
    ) -> Result<(), VehicleTyreError<B::Error>> {
        match slot {
            VectorSlot::Tyres => match field {
                VectorField::Begin => self.tyres_begin = Some(value),
                VectorField::End => self.tyres_end = Some(value),
                VectorField::Capacity => self.tyres_capacity = Some(value),
            },
            VectorSlot::SourceTyres => {
                let vector = &mut self.tyre_temporary.as_mut().expect("active tyre load").rows;
                match field {
                    VectorField::Begin => vector.begin = value,
                    VectorField::End => vector.end = value,
                    VectorField::Capacity => vector.capacity = value,
                }
            }
            VectorSlot::Surfaces(header) => {
                if header == 0 {
                    return Err(VehicleTyreError::Boundary(
                        TyreBoundary::NativeNullSurfaceHeader,
                    ));
                }
                host.write_word(header.wrapping_add(field.offset()), value)
                    .map_err(VehicleTyreError::Host)?;
            }
        }
        Ok(())
    }
    fn tyre_range_len<E>(begin: u32, end: u32, stride: u32) -> Result<u32, VehicleTyreError<E>> {
        // Malformed or wrapped extents remain an explicit storage boundary.
        // Address formation itself follows the PC's 32-bit wrapping arithmetic.
        let bytes = end
            .checked_sub(begin)
            .filter(|bytes| *bytes % stride == 0)
            .ok_or(VehicleTyreError::Boundary(TyreBoundary::InvalidVector))?;
        if bytes > 4 * 1024 * 1024 {
            return Err(VehicleTyreError::Boundary(
                TyreBoundary::ExtentOutsideOwnerDomain(bytes),
            ));
        }
        Ok(bytes / stride)
    }
    fn tyre_size<B: VehicleInitBackend>(
        &self,
        host: &mut B,
        slot: VectorSlot,
        stride: u32,
    ) -> Result<u32, VehicleTyreError<B::Error>> {
        let begin = self.tyre_vector_field(host, slot, VectorField::Begin)?;
        if begin == 0 {
            return Ok(0);
        }
        let end = self.tyre_vector_field(host, slot, VectorField::End)?;
        Self::tyre_range_len(begin, end, stride)
    }
    fn tyre_copy_record<B: VehicleInitBackend>(
        host: &mut B,
        destination: u32,
        source: RecordSource<'_>,
        stride: u32,
    ) -> Result<(), VehicleTyreError<B::Error>> {
        // Both PC construction helpers test placement destination null before
        // REP MOVSD. Reread each source dword, including overlapping records.
        if destination == 0 {
            return Ok(());
        }
        for offset in (0..stride).step_by(4) {
            let word = match source {
                RecordSource::Bytes(bytes) => u32::from_le_bytes(
                    bytes[offset as usize..offset as usize + 4]
                        .try_into()
                        .unwrap(),
                ),
                RecordSource::Memory(address) => host
                    .read_word(address.wrapping_add(offset))
                    .map_err(VehicleTyreError::Host)?,
            };
            host.write_word(destination.wrapping_add(offset), word)
                .map_err(VehicleTyreError::Host)?;
        }
        Ok(())
    }
    fn tyre_copy_range<B: VehicleInitBackend>(
        host: &mut B,
        destination: u32,
        begin: u32,
        end: u32,
        stride: u32,
    ) -> Result<u32, VehicleTyreError<B::Error>> {
        let count = Self::tyre_range_len(begin, end, stride)?;
        let mut destination = destination;
        for index in 0..count {
            Self::tyre_copy_record(
                host,
                destination,
                RecordSource::Memory(begin.wrapping_add(index.wrapping_mul(stride))),
                stride,
            )?;
            destination = destination.wrapping_add(stride);
        }
        Ok(destination)
    }
    /// Append-at-end callers of PC helpers RVA 0x2d800/0x2de10 and the
    /// setter's inlined surface append. All fields are read at the reached
    /// native point, including begin after allocate, end after copying, and
    /// begin after free. Redirected live ranges are copied, never snapshotted.
    fn tyre_append<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        slot: VectorSlot,
        source: RecordSource<'_>,
        stride: u32,
        kind: AppendKind,
    ) -> Result<(), VehicleTyreError<B::Error>> {
        let insertion = self.tyre_vector_field(host, slot, VectorField::End)?;
        let end = match kind {
            AppendKind::Helper => self.tyre_vector_field(host, slot, VectorField::End)?,
            AppendKind::SetterSurface => insertion,
        };
        let capacity_end = self.tyre_vector_field(host, slot, VectorField::Capacity)?;
        let spare = Self::tyre_range_len(end, capacity_end, stride)?;
        if spare != 0 {
            // The actual callers insert at end. No detached insertion request
            // is accepted when an intervening read redirects that position.
            if insertion != end {
                return Err(VehicleTyreError::Boundary(TyreBoundary::InvalidVector));
            }
            Self::tyre_copy_record(host, end, source, stride)?;
            let live_end = self.tyre_vector_field(host, slot, VectorField::End)?;
            return self.tyre_vector_store(
                host,
                slot,
                VectorField::End,
                live_end.wrapping_add(stride),
            );
        }
        let begin = self.tyre_vector_field(host, slot, VectorField::Begin)?;
        let size = if begin == 0 {
            0
        } else {
            Self::tyre_range_len(begin, end, stride)?
        };
        let capacity = size
            .checked_add(size.max(1))
            .and_then(|count| count.checked_mul(stride))
            .ok_or(VehicleTyreError::Boundary(TyreBoundary::InvalidVector))?;
        let allocation = self.tyre_allocate(host, capacity)?;
        let live_begin = self.tyre_vector_field(host, slot, VectorField::Begin)?;
        let inserted_at = Self::tyre_copy_range(host, allocation, live_begin, insertion, stride)?;
        Self::tyre_copy_record(host, inserted_at, source, stride)?;
        let live_end = self.tyre_vector_field(host, slot, VectorField::End)?;
        Self::tyre_copy_range(
            host,
            inserted_at.wrapping_add(stride),
            insertion,
            live_end,
            stride,
        )?;
        let old_allocation = self.tyre_vector_field(host, slot, VectorField::Begin)?;
        self.tyre_free(host, old_allocation)?;
        // Helpers read begin just before publishing capacity; the inlined
        // setter computes size after that publication (RVA 0xc5a5c..0xc5a65).
        let saved_begin = match kind {
            AppendKind::Helper => Some(self.tyre_vector_field(host, slot, VectorField::Begin)?),
            AppendKind::SetterSurface => None,
        };
        self.tyre_vector_store(
            host,
            slot,
            VectorField::Capacity,
            allocation.wrapping_add(capacity),
        )?;
        let begin = match saved_begin {
            Some(begin) => begin,
            None => self.tyre_vector_field(host, slot, VectorField::Begin)?,
        };
        let size = if begin == 0 {
            0
        } else {
            let end = self.tyre_vector_field(host, slot, VectorField::End)?;
            Self::tyre_range_len(begin, end, stride)?
        };
        self.tyre_vector_store(host, slot, VectorField::Begin, allocation)?;
        self.tyre_vector_store(
            host,
            slot,
            VectorField::End,
            allocation.wrapping_add(size.wrapping_add(1).wrapping_mul(stride)),
        )
    }
    fn tyre_new_surface_header<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
    ) -> Result<u32, VehicleTyreError<B::Error>> {
        let header = self.tyre_allocate(host, 16)?;
        if header != 0 {
            // Byte zero is VC6's empty allocator object copied from an
            // uninitialized stack byte. It is neither a fabricated zero nor
            // consumed by the supported table operations. Only the three
            // evidenced dword pointer fields are initialized here.
            for offset in [4, 8, 12] {
                host.write_word(header.wrapping_add(offset), 0)
                    .map_err(VehicleTyreError::Host)?;
            }
        }
        Ok(header)
    }
    fn tyre_delete_surface_header<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        header: u32,
    ) -> Result<(), VehicleTyreError<B::Error>> {
        if header == 0 {
            return Ok(());
        }
        let begin = host
            .read_word(header.wrapping_add(4))
            .map_err(VehicleTyreError::Host)?;
        self.tyre_free(host, begin)?;
        for offset in [4, 8, 12] {
            host.write_word(header.wrapping_add(offset), 0)
                .map_err(VehicleTyreError::Host)?;
        }
        self.tyre_free(host, header)
    }
    fn close_tyres<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
    ) -> Result<(), VehicleTyreError<B::Error>> {
        let count = self.tyre_size(host, VectorSlot::Tyres, 0x28)?;
        for index in (0..count).rev() {
            // CloseTyres rereads global begin after each released header.
            let begin = self.tyres_begin.unwrap_or(0);
            let header = host
                .read_word(
                    begin
                        .wrapping_add(index.wrapping_mul(0x28))
                        .wrapping_add(0x24),
                )
                .map_err(VehicleTyreError::Host)?;
            self.tyre_delete_surface_header(host, header)?;
        }
        self.tyres_end = Some(self.tyres_begin.unwrap_or(0));
        Ok(())
    }
    fn set_tyres<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        source_present: bool,
    ) -> Result<bool, VehicleTyreError<B::Error>> {
        self.close_tyres(host)?;
        self.tyre_append(
            host,
            VectorSlot::Tyres,
            RecordSource::Bytes(&DrivingVehicleTables::user_tyre_record()),
            0x28,
            AppendKind::Helper,
        )?;
        if source_present {
            let mut index: u32 = 0;
            loop {
                let source_begin =
                    self.tyre_vector_field(host, VectorSlot::SourceTyres, VectorField::Begin)?;
                if index >= self.tyre_size(host, VectorSlot::SourceTyres, 0x28)? {
                    break;
                }
                self.tyre_append(
                    host,
                    VectorSlot::Tyres,
                    RecordSource::Memory(source_begin.wrapping_add(index.wrapping_mul(0x28))),
                    0x28,
                    AppendKind::Helper,
                )?;
                let header = self.tyre_new_surface_header(host)?;
                let last = self
                    .tyre_size(host, VectorSlot::Tyres, 0x28)?
                    .wrapping_sub(1);
                let begin = self.tyres_begin.unwrap_or(0);
                host.write_word(
                    begin
                        .wrapping_add(last.wrapping_mul(0x28))
                        .wrapping_add(0x24),
                    header,
                )
                .map_err(VehicleTyreError::Host)?;
                let mut surface_index: u32 = 0;
                loop {
                    // Source row and surface pointers remain live across the
                    // destination allocation/copy callbacks, as in SetTyres.
                    let source_begin =
                        self.tyre_vector_field(host, VectorSlot::SourceTyres, VectorField::Begin)?;
                    let source_header = host
                        .read_word(
                            source_begin
                                .wrapping_add(index.wrapping_mul(0x28))
                                .wrapping_add(0x24),
                        )
                        .map_err(VehicleTyreError::Host)?;
                    if surface_index
                        >= self.tyre_size(host, VectorSlot::Surfaces(source_header), 0x5c)?
                    {
                        break;
                    }
                    let surface_begin = self.tyre_vector_field(
                        host,
                        VectorSlot::Surfaces(source_header),
                        VectorField::Begin,
                    )?;
                    let last = self
                        .tyre_size(host, VectorSlot::Tyres, 0x28)?
                        .wrapping_sub(1);
                    let begin = self.tyres_begin.unwrap_or(0);
                    let destination = host
                        .read_word(
                            begin
                                .wrapping_add(last.wrapping_mul(0x28))
                                .wrapping_add(0x24),
                        )
                        .map_err(VehicleTyreError::Host)?;
                    self.tyre_append(
                        host,
                        VectorSlot::Surfaces(destination),
                        RecordSource::Memory(
                            surface_begin.wrapping_add(surface_index.wrapping_mul(0x5c)),
                        ),
                        0x5c,
                        AppendKind::SetterSurface,
                    )?;
                    surface_index += 1;
                }
                index += 1;
            }
        }
        Ok(true)
    }
    /// PC C_Vehicle::SetTyres(empty), RVA 0xc5820; CloseTyres RVA 0xc5b80.
    /// Same canonical resource owner as car Init's global compatibility view.
    /// Native old surface resources are released in reverse row order; main
    /// vector capacity survives. An uncertain destructive error locks entry.
    #[cfg(test)]
    pub(crate) fn set_tyres_empty<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
    ) -> Result<bool, VehicleTyreError<B::Error>> {
        self.tyre_owner_begin()?;
        let result = self.set_tyres(host, false);
        self.tyre_finish(&result);
        result
    }

    fn tyre_read<B: VehicleInitBackend, F: TyreTableFile>(
        &mut self,
        file: &mut F,
        field: TyreReadField,
        destination: &mut [u8],
    ) -> Result<(), LoadTyresError<B::Error, F::Error>> {
        self.tyre_state = TyreLoadState::At(TyreOperation::Read(field));
        let actual = file.read(destination).map_err(LoadTyresError::File)?;
        if actual != destination.len() {
            return Err(LoadTyresError::Owner(VehicleTyreError::Boundary(
                TyreBoundary::ShortRead {
                    field,
                    expected: destination.len(),
                    actual,
                },
            )));
        }
        Ok(())
    }
    fn tyre_read_word<B: VehicleInitBackend, F: TyreTableFile>(
        &mut self,
        file: &mut F,
        field: TyreReadField,
    ) -> Result<u32, LoadTyresError<B::Error, F::Error>> {
        let mut bytes = [0; 4];
        self.tyre_read::<B, F>(file, field, &mut bytes)?;
        Ok(u32::from_le_bytes(bytes))
    }
    fn release_tyre_source_rows<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        deep: bool,
    ) -> Result<(), VehicleTyreError<B::Error>> {
        if deep {
            for index in (0..self.tyre_size(host, VectorSlot::SourceTyres, 0x28)?).rev() {
                let begin =
                    self.tyre_vector_field(host, VectorSlot::SourceTyres, VectorField::Begin)?;
                let header = host
                    .read_word(
                        begin
                            .wrapping_add(index.wrapping_mul(0x28))
                            .wrapping_add(0x24),
                    )
                    .map_err(VehicleTyreError::Host)?;
                self.tyre_delete_surface_header(host, header)?;
            }
        }
        // S_TYRE destruction is trivial (RVA 0xd2040). The false payload
        // allocation exit frees only this array and leaks nested resources.
        let allocation = self
            .tyre_temporary
            .as_ref()
            .expect("active tyre load")
            .rows
            .begin;
        if deep {
            self.tyre_temporary
                .as_mut()
                .expect("active tyre load")
                .rows
                .end = allocation;
        }
        self.tyre_free(host, allocation)?;
        self.tyre_temporary.as_mut().expect("active tyre load").rows = TableVector::default();
        Ok(())
    }
    fn load_tyres_inner<B: VehicleInitBackend, F: TyreTableFile>(
        &mut self,
        host: &mut B,
        file: &mut F,
    ) -> Result<bool, LoadTyresError<B::Error, F::Error>> {
        self.set_tyres(host, false).map_err(LoadTyresError::Owner)?;
        self.tyre_state = TyreLoadState::At(TyreOperation::Open);
        if !file.open_tyres().map_err(LoadTyresError::File)? {
            // delete[] buffer (always null), then the empty temporary vector.
            self.tyre_free(host, 0).map_err(LoadTyresError::Owner)?;
            self.release_tyre_source_rows(host, false)
                .map_err(LoadTyresError::Owner)?;
            return Ok(false);
        }
        let version = self.tyre_read_word::<B, F>(file, TyreReadField::Version)?;
        if version != 1 {
            self.tyre_state = TyreLoadState::At(TyreOperation::FileClose);
            file.close().map_err(LoadTyresError::File)?;
            self.tyre_free(host, 0).map_err(LoadTyresError::Owner)?;
            self.release_tyre_source_rows(host, false)
                .map_err(LoadTyresError::Owner)?;
            return Ok(false);
        }
        // PC CMP/DEC are 32-bit nonzero loops, including high-bit counts.
        let mut remaining = self.tyre_read_word::<B, F>(file, TyreReadField::Count)?;
        while remaining != 0 {
            remaining = remaining.wrapping_sub(1);
            self.tyre_temporary
                .as_mut()
                .expect("active tyre load")
                .current = TyreStagingRow::default();
            let extent = self.tyre_read_word::<B, F>(file, TyreReadField::Extent)?;
            self.tyre_temporary
                .as_mut()
                .expect("active tyre load")
                .current
                .source_extent = extent;
            let mut name = [0; 32];
            let read = self.tyre_read::<B, F>(file, TyreReadField::Name, &mut name);
            self.tyre_temporary
                .as_mut()
                .expect("active tyre load")
                .current
                .name = name;
            read?;
            let mut id = [0; 4];
            let read = self.tyre_read::<B, F>(file, TyreReadField::Id, &mut id);
            self.tyre_temporary
                .as_mut()
                .expect("active tyre load")
                .current
                .id = u32::from_le_bytes(id);
            read?;
            let mut surfaces = self.tyre_read_word::<B, F>(file, TyreReadField::SurfaceCount)?;
            while surfaces != 0 {
                surfaces = surfaces.wrapping_sub(1);
                // This allocation precedes the material/size reads. A zero
                // surface count never allocates a header in the source row.
                if self
                    .tyre_temporary
                    .as_ref()
                    .expect("active tyre load")
                    .current
                    .surfaces
                    == 0
                {
                    self.tyre_state = TyreLoadState::At(TyreOperation::SourceSurfaceHeader);
                    let header = self
                        .tyre_new_surface_header(host)
                        .map_err(LoadTyresError::Owner)?;
                    self.tyre_temporary
                        .as_mut()
                        .expect("active tyre load")
                        .current
                        .surfaces = header;
                }
                let material = self.tyre_read_word::<B, F>(file, TyreReadField::Material)?;
                let size = self.tyre_read_word::<B, F>(file, TyreReadField::PayloadSize)?;
                self.tyre_state = TyreLoadState::At(TyreOperation::PayloadAllocation);
                let payload = self
                    .tyre_allocate(host, size)
                    .map_err(LoadTyresError::Owner)?;
                self.tyre_temporary
                    .as_mut()
                    .expect("active tyre load")
                    .payload = Some(payload);
                if payload == 0 {
                    // Native AL=false here; no dtaClose and no nested-vector
                    // destruction. Do not turn a different unavailable effect
                    // into this branch or opportunistically clean its leaks.
                    self.tyre_free(host, 0).map_err(LoadTyresError::Owner)?;
                    self.release_tyre_source_rows(host, false)
                        .map_err(LoadTyresError::Owner)?;
                    return Ok(false);
                }
                let mut bytes = vec![0; size as usize];
                self.tyre_state = TyreLoadState::At(TyreOperation::Read(TyreReadField::Payload));
                let actual = file.read(&mut bytes).map_err(LoadTyresError::File)?;
                if actual > bytes.len() {
                    return Err(LoadTyresError::Owner(VehicleTyreError::Boundary(
                        TyreBoundary::ShortRead {
                            field: TyreReadField::Payload,
                            expected: bytes.len(),
                            actual,
                        },
                    )));
                }
                // Only bytes the file actually supplied are initialized in
                // the real allocation. A short native read leaves the rest
                // unknown; stop at that boundary after the reached writes.
                for (offset, byte) in bytes[..actual].iter().copied().enumerate() {
                    host.write_byte(payload.wrapping_add(offset as u32), byte)
                        .map_err(|error| LoadTyresError::Owner(VehicleTyreError::Host(error)))?;
                }
                if actual != bytes.len() {
                    return Err(LoadTyresError::Owner(VehicleTyreError::Boundary(
                        TyreBoundary::ShortRead {
                            field: TyreReadField::Payload,
                            expected: bytes.len(),
                            actual,
                        },
                    )));
                }
                self.tyre_state = TyreLoadState::At(TyreOperation::Convert);
                if size < 72 {
                    return Err(LoadTyresError::Owner(VehicleTyreError::Boundary(
                        TyreBoundary::ExtentOutsideOwnerDomain(size),
                    )));
                }
                let mut input = [0; 72];
                for (offset, byte) in input.iter_mut().enumerate() {
                    *byte = host
                        .read_byte(payload.wrapping_add(offset as u32))
                        .map_err(|error| LoadTyresError::Owner(VehicleTyreError::Host(error)))?;
                }
                let surface = convert_tyre_payload(material, &input).map_err(|error| {
                    LoadTyresError::Owner(VehicleTyreError::Boundary(TyreBoundary::Projection(
                        error,
                    )))
                })?;
                let header = self
                    .tyre_temporary
                    .as_ref()
                    .expect("active tyre load")
                    .current
                    .surfaces;
                self.tyre_state = TyreLoadState::At(TyreOperation::SourceSurfaceAppend);
                self.tyre_append(
                    host,
                    VectorSlot::Surfaces(header),
                    RecordSource::Bytes(&surface),
                    0x5c,
                    AppendKind::Helper,
                )
                .map_err(LoadTyresError::Owner)?;
                self.tyre_state = TyreLoadState::At(TyreOperation::PayloadRelease);
                self.tyre_free(host, payload)
                    .map_err(LoadTyresError::Owner)?;
                self.tyre_temporary
                    .as_mut()
                    .expect("active tyre load")
                    .payload = None;
            }
            self.tyre_state = TyreLoadState::At(TyreOperation::SourceTyreAppend);
            let row = self
                .tyre_temporary
                .as_ref()
                .expect("active tyre load")
                .current
                .record();
            self.tyre_append(
                host,
                VectorSlot::SourceTyres,
                RecordSource::Bytes(&row),
                0x28,
                AppendKind::Helper,
            )
            .map_err(LoadTyresError::Owner)?;
        }
        self.tyre_state = TyreLoadState::At(TyreOperation::FileClose);
        file.close().map_err(LoadTyresError::File)?;
        self.tyre_free(host, 0).map_err(LoadTyresError::Owner)?;
        self.tyre_state = TyreLoadState::At(TyreOperation::Replace);
        self.set_tyres(host, true).map_err(LoadTyresError::Owner)?;
        self.tyre_state = TyreLoadState::At(TyreOperation::SourceRelease);
        self.release_tyre_source_rows(host, true)
            .map_err(LoadTyresError::Owner)?;
        Ok(true)
    }
    /// Ordered C_car::LoadTyres, PC RVA 0x1d480. Primary REA/Ghidra of Game.exe
    /// SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27:
    /// ev_e6374fb567f30facae51d883c6a5f7a9b4e8861060a4d6f693a584a9cf12d360,
    /// setter ev_168ac1df7181457557475dd33a232379a980f25c468f7ad73014bd84133f7532,
    /// close ev_d213fccf46e9e630a78ec5665f11295b1247c253565892172d20bc6a211ed38b.
    /// This bounded owner retains native false separately from unavailable IO,
    /// short reads, malformed storage and the existing numeric-domain errors.
    /// Its file and temporary resources remain owned at an interrupted prefix.
    /// No automatic replay, material/particle work or runtime parity is implied.
    pub(crate) fn load_tyres<B: VehicleInitBackend, F: TyreTableFile>(
        &mut self,
        host: &mut B,
        file: &mut F,
    ) -> Result<bool, LoadTyresError<B::Error, F::Error>> {
        self.tyre_owner_begin().map_err(LoadTyresError::Owner)?;
        if let Some(previous) = self.tyre_temporary.take() {
            self.abandoned_tyre_loads.push(previous);
        }
        self.tyre_temporary = Some(TyreTemporary::default());
        let result = self.load_tyres_inner(host, file);
        self.tyre_finish(&result);
        result
    }

    /// The caller supplies the active precision policy. An explicit load runs
    /// the existing replacement even after a completed lazy load.
    pub(crate) fn load_installed_tyres<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        installation: &Installation,
    ) -> Result<bool, VehicleTableError<B::Error>> {
        let mut file = InstallationTyreFile::new(installation);
        let result = crate::game_init_car_tables::load_tyres_from_file(self, host, &mut file);
        if let Some(stream) = file.into_open_stream() {
            self.retained_tyre_files.push(stream);
        }
        result.map_err(|error| match error {
            LoadTyresError::Owner(VehicleTyreError::Host(error)) => VehicleTableError::Host(error),
            LoadTyresError::Owner(VehicleTyreError::Boundary(error)) => {
                VehicleTableError::Load(format!("LoadTyres boundary: {error:?}"))
            }
            LoadTyresError::Owner(VehicleTyreError::AlreadyStarted) => {
                VehicleTableError::Interrupted
            }
            LoadTyresError::File(error) => VehicleTableError::Load(error),
        })
    }

    /// Unknown addresses never trigger IO, including after a failed known read.
    /// Caller holds the original precision policy while the existing projection
    /// performs its established arithmetic. A failed intake cannot be replayed.
    pub(crate) fn read_byte<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        installation: Option<&Installation>,
        address: u32,
    ) -> Result<Option<u8>, VehicleTableError<B::Error>> {
        if !Self::contains(address) {
            return Ok(None);
        }
        if self.discard_started {
            return Err(VehicleTableError::Interrupted);
        }
        let is_tyre = (0x63d9c8..0x63d9d0).contains(&address);
        if is_tyre {
            if self.tyre_state == TyreLoadState::NeverStarted {
                let Some(installation) = installation else {
                    self.phase = Phase::Interrupted;
                    self.tyre_state = TyreLoadState::Interrupted(TyreOperation::Open);
                    return Err(VehicleTableError::MissingInstallation);
                };
                self.load_installed_tyres(host, installation)?;
            }
            if !matches!(self.tyre_state, TyreLoadState::Complete(_)) {
                return Err(VehicleTableError::Interrupted);
            }
        } else {
            if self.phase == Phase::Unloaded {
                self.phase = Phase::Loading;
                let result = (|| {
                    let installation =
                        installation.ok_or(VehicleTableError::MissingInstallation)?;
                    let source = installation.read("tables/materials.def").map_err(|error| {
                        VehicleTableError::Load(format!("Original material definitions: {error}"))
                    })?;
                    let (rows, index) = materials(&source).map_err(VehicleTableError::Load)?;
                    self.project_materials(host, &source, &rows, index)
                })();
                self.phase = if result.is_ok() {
                    Phase::Ready
                } else {
                    Phase::Interrupted
                };
                result?;
            }
            if self.phase != Phase::Ready {
                return Err(VehicleTableError::Interrupted);
            }
        }
        Ok(self.published_byte(address))
    }
    fn published_byte(&self, address: u32) -> Option<u8> {
        if (0x63d8b0..0x63d9b0).contains(&address) {
            return self
                .material_index
                .as_ref()
                .map(|index| index[(address - 0x63d8b0) as usize]);
        }
        for (base, value) in [
            (0x63d9b4, self.materials),
            (0x63d9c8, self.tyres_begin),
            (0x63d9cc, self.tyres_end),
            (0x6d4c24, self.source_materials),
            (0x6d4c28, self.source_material_count),
        ] {
            if (base..base + 4).contains(&address) {
                return value.map(|word| word.to_le_bytes()[(address - base) as usize]);
            }
        }
        None
    }
    /// Additional host cleanup only, after caller proves all borrowing cars
    /// dead. Releases just these leases, in reverse acquisition order. Any
    /// failed discard consumes permission; no uncertain deallocation is retried.
    pub(crate) fn discard_host(
        &mut self,
        memory: &mut VehicleFrameMemory,
    ) -> Result<usize, VehicleMemoryError> {
        if self.discard_started {
            return Err(VehicleMemoryError::IntrinsicAdapterRequired);
        }
        self.discard_started = true;
        self.phase = Phase::Interrupted;
        let mut released = 0;
        while let Some(lease) = self.leases.last() {
            // VehicleFrameMemory reserves one physical byte for a zero-size request.
            if lease.release_started {
                return Err(VehicleMemoryError::IntrinsicAdapterRequired);
            }
            memory.discard_host_allocation(lease.allocation, lease.bytes.max(1))?;
            self.leases.pop();
            released += 1;
        }
        self.source_materials = None;
        self.source_material_count = None;
        self.materials = None;
        self.material_index = None;
        self.tyres_begin = None;
        self.tyres_end = None;
        self.tyres_capacity = None;
        self.retained_tyre_files.clear();
        self.phase = Phase::Discarded;
        Ok(released)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle_init::{
        VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
    };
    #[derive(Default)]
    struct Host {
        memory: VehicleFrameMemory,
        writes: usize,
        fail_at: Option<usize>,
    }
    impl VehicleInitBackend for Host {
        type Error = VehicleMemoryError;
        fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
            self.memory.read_byte(a)
        }
        fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
            self.memory.read_word(a)
        }
        fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Self::Error> {
            self.writes += 1;
            if self.fail_at == Some(self.writes) {
                return Err(VehicleMemoryError::Address(a));
            }
            self.memory.write_byte(a, v)
        }
        fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
            self.memory.write_word(a, v)
        }
        fn free(&mut self, a: u32) -> Result<(), Self::Error> {
            self.memory.free(a)
        }
        fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
            self.memory.allocate(n)
        }
        fn collect_frame(
            &mut self,
            _: &mut VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            Err(VehicleMemoryError::IntrinsicAdapterRequired)
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            Err(VehicleInitUnavailable::Effect(
                VehicleMemoryError::IntrinsicAdapterRequired,
            ))
        }
    }
    fn fixture() -> DrivingVehicleTables {
        let mut tyres = Vec::new();
        for word in [1u32, 1, 128] {
            tyres.extend_from_slice(&word.to_le_bytes());
        }
        let mut name = [0; 32];
        name[..9].copy_from_slice(b"synthetic");
        tyres.extend_from_slice(&name);
        for word in [42u32, 1, 13, 72] {
            tyres.extend_from_slice(&word.to_le_bytes());
        }
        for value in 1..=18 {
            tyres.extend_from_slice(&(value as f32).to_le_bytes());
        }
        DrivingVehicleTables::from_source_bytes(vec![0; MATERIAL_SOURCE_STRIDE], tyres).unwrap()
    }
    #[test]
    fn unknown_reads_do_not_load_and_known_missing_installation_latches() {
        let mut tables = VehicleTableResources::default();
        let mut host = Host::default();
        assert_eq!(tables.read_byte(&mut host, None, 0x1234), Ok(None));
        assert_eq!(tables.phase, Phase::Unloaded);
        assert_eq!(
            tables.read_byte(&mut host, None, 0x63d9c8),
            Err(VehicleTableError::MissingInstallation)
        );
        assert_eq!(
            tables.read_byte(&mut host, None, 0x63d9cc),
            Err(VehicleTableError::Interrupted)
        );
        assert_eq!(tables.read_byte(&mut host, None, 0x63d9b0), Ok(None));
        assert_eq!(host.memory.allocations().count(), 0);
    }
    #[test]
    fn actual_parser_projection_shares_typed_headers_and_discards_only_its_leases() {
        let mut tables = VehicleTableResources::default();
        let mut host = Host::default();
        let unrelated = host.allocate(7).unwrap();
        tables.project(&mut host, fixture()).unwrap();
        tables.phase = Phase::Ready;
        assert_eq!(tables.live_allocation_count(), 5);
        let begin = tables.tyres_begin.unwrap();
        let end = tables.tyres_end.unwrap();
        assert_eq!(end - begin, 80); // user row plus the synthetic source row
        assert_eq!(host.read_word(begin).unwrap(), 0);
        assert_eq!(host.read_word(begin + 40).unwrap(), 42);
        let header = host.read_word(begin + 76).unwrap();
        let surfaces = host.read_word(header.wrapping_add(4)).unwrap();
        assert_eq!(host.read_word(header + 8).unwrap() - surfaces, 0x5c);
        assert_eq!(host.read_word(surfaces).unwrap(), 13);
        assert_eq!(
            tables.read_byte(&mut host, None, 0x63d9c8).unwrap(),
            Some(begin.to_le_bytes()[0])
        );
        assert_eq!(tables.discard_host(&mut host.memory).unwrap(), 5);
        assert_eq!(
            host.memory.allocations().collect::<Vec<_>>(),
            [(unrelated, 7)]
        );
        assert!(tables.discard_host(&mut host.memory).is_err());
    }
    #[test]
    fn every_table_write_failure_retains_the_real_allocation_and_publication_prefix() {
        let mut baseline = Host::default();
        let mut complete = VehicleTableResources::default();
        complete.project(&mut baseline, fixture()).unwrap();
        for failure in 1..=baseline.writes {
            let mut host = Host {
                fail_at: Some(failure),
                ..Default::default()
            };
            let mut tables = VehicleTableResources::default();
            assert!(tables.project(&mut host, fixture()).is_err());
            assert_eq!(host.writes, failure);
            assert_eq!(
                tables.live_allocation_count(),
                host.memory.allocations().count()
            );
            // Source header is published only once all source bytes are written.
            assert_eq!(
                tables.source_materials.is_some(),
                failure > MATERIAL_SOURCE_STRIDE
            );
            let owned = tables.live_allocation_count();
            assert_eq!(tables.discard_host(&mut host.memory).unwrap(), owned);
            assert_eq!(host.memory.allocations().count(), 0);
        }
    }
}

#[cfg(test)]
mod tyre_loader_tests {
    use super::*;
    use crate::vehicle_init::{
        VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery, VehicleInitUnavailable,
    };
    use std::{cell::RefCell, rc::Rc};

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        Allocate(u32),
        Free(u32),
        ReadWord(u32),
        ReadByte(u32),
        WriteWord(u32, u32),
        WriteByte(u32, u8),
        Open,
        Read(usize, usize),
        Close,
    }
    type Trace = Rc<RefCell<Vec<Event>>>;

    #[derive(Default)]
    struct LoaderHost {
        memory: VehicleFrameMemory,
        trace: Trace,
        effects: usize,
        fail_effect: Option<usize>,
        null_allocation: Option<usize>,
        allocations: Vec<(u32, u32)>,
        allocation_calls: usize,
        free_then_error: Option<u32>,
        // Hooks mutate the actual memory owner; they are not another table map.
        on_allocate: Option<(usize, Vec<(u32, u32)>)>,
        on_free: Option<(u32, Vec<(u32, u32)>)>,
        on_write: Option<(u32, Vec<(u32, u32)>)>,
    }
    impl LoaderHost {
        fn effect(&mut self, event: Event) -> Result<(), VehicleMemoryError> {
            self.effects += 1;
            self.trace.borrow_mut().push(event);
            if self.fail_effect == Some(self.effects) {
                Err(VehicleMemoryError::IntrinsicAdapterRequired)
            } else {
                Ok(())
            }
        }
        fn stores(&mut self, stores: Vec<(u32, u32)>) -> Result<(), VehicleMemoryError> {
            for (address, value) in stores {
                self.memory.write_word(address, value)?;
            }
            Ok(())
        }
        fn bytes(&mut self, address: u32, count: usize) -> Vec<u8> {
            (0..count)
                .map(|offset| self.memory.read_byte(address + offset as u32).unwrap())
                .collect()
        }
    }
    impl VehicleInitBackend for LoaderHost {
        type Error = VehicleMemoryError;
        fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
            self.effect(Event::ReadByte(address))?;
            self.memory.read_byte(address)
        }
        fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
            self.effect(Event::ReadWord(address))?;
            self.memory.read_word(address)
        }
        fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
            self.effect(Event::WriteByte(address, value))?;
            self.memory.write_byte(address, value)
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
            self.effect(Event::WriteWord(address, value))?;
            self.memory.write_word(address, value)?;
            if self.on_write.as_ref().is_some_and(|(at, _)| *at == address) {
                let (_, stores) = self.on_write.take().unwrap();
                self.stores(stores)?;
            }
            Ok(())
        }
        fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
            self.effect(Event::Allocate(bytes))?;
            self.allocation_calls += 1;
            let allocation = if self.null_allocation == Some(self.allocation_calls) {
                0
            } else {
                self.memory.allocate(bytes)?
            };
            self.allocations.push((bytes, allocation));
            if self
                .on_allocate
                .as_ref()
                .is_some_and(|(call, _)| *call == self.allocation_calls)
            {
                let (_, stores) = self.on_allocate.take().unwrap();
                self.stores(stores)?;
            }
            Ok(allocation)
        }
        fn free(&mut self, allocation: u32) -> Result<(), Self::Error> {
            self.effect(Event::Free(allocation))?;
            self.memory.free(allocation)?;
            if self
                .on_free
                .as_ref()
                .is_some_and(|(at, _)| *at == allocation)
            {
                let (_, stores) = self.on_free.take().unwrap();
                self.stores(stores)?;
            }
            if self.free_then_error == Some(allocation) {
                return Err(VehicleMemoryError::Address(allocation));
            }
            Ok(())
        }
        fn collect_frame(
            &mut self,
            _: &mut VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            Err(VehicleMemoryError::IntrinsicAdapterRequired)
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            Err(VehicleInitUnavailable::Effect(
                VehicleMemoryError::IntrinsicAdapterRequired,
            ))
        }
    }

    struct File {
        trace: Trace,
        bytes: Vec<u8>,
        position: usize,
        open: bool,
        present: bool,
        fail_open: bool,
        fail_close: bool,
        fail_read_at: Option<usize>,
    }
    impl File {
        fn new(host: &LoaderHost, bytes: Vec<u8>) -> Self {
            Self {
                trace: Rc::clone(&host.trace),
                bytes,
                position: 0,
                open: false,
                present: true,
                fail_open: false,
                fail_close: false,
                fail_read_at: None,
            }
        }
    }
    impl TyreTableFile for File {
        type Error = &'static str;
        fn open_tyres(&mut self) -> Result<bool, Self::Error> {
            self.trace.borrow_mut().push(Event::Open);
            if self.fail_open {
                return Err("open");
            }
            assert!(!self.open);
            self.open = self.present;
            Ok(self.present)
        }
        fn read(&mut self, destination: &mut [u8]) -> Result<usize, Self::Error> {
            assert!(self.open);
            self.trace
                .borrow_mut()
                .push(Event::Read(self.position, destination.len()));
            if self.fail_read_at == Some(self.position) {
                return Err("read");
            }
            let actual = destination.len().min(self.bytes.len() - self.position);
            destination[..actual]
                .copy_from_slice(&self.bytes[self.position..self.position + actual]);
            self.position += actual;
            Ok(actual)
        }
        fn close(&mut self) -> Result<(), Self::Error> {
            self.trace.borrow_mut().push(Event::Close);
            assert!(self.open);
            self.open = false;
            if self.fail_close {
                return Err("close");
            }
            Ok(())
        }
    }
    fn word(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    fn source(counts: &[u32]) -> Vec<u8> {
        let mut bytes = Vec::new();
        word(&mut bytes, 1);
        word(&mut bytes, counts.len() as u32);
        for (index, count) in counts.iter().copied().enumerate() {
            word(&mut bytes, 0xffff_fffc); // extent is read, not used as a length
            bytes.extend_from_slice(&[b'A' + index as u8; 32]);
            word(&mut bytes, 0x8000_0042 + index as u32);
            word(&mut bytes, count);
            for surface in 0..count {
                word(&mut bytes, 0xf000_0000 + surface);
                word(&mut bytes, 72);
                for value in 1..=18 {
                    bytes.extend_from_slice(&(value as f32).to_le_bytes());
                }
            }
        }
        bytes
    }
    fn user_row(tables: &VehicleTableResources, host: &mut LoaderHost) {
        let begin = tables.tyres_begin.unwrap();
        assert_eq!(
            host.bytes(begin, 40),
            DrivingVehicleTables::user_tyre_record()
        );
    }
    fn surface_header(
        tables: &VehicleTableResources,
        host: &mut LoaderHost,
        row: u32,
    ) -> (u32, u32) {
        let header = host
            .memory
            .read_word(tables.tyres_begin.unwrap() + row * 40 + 36)
            .unwrap();
        let data = host.memory.read_word(header + 4).unwrap();
        (header, data)
    }
    fn assert_locked(tables: &mut VehicleTableResources, host: &mut LoaderHost, file: &mut File) {
        let prefix = host.trace.borrow().clone();
        assert_eq!(
            tables.load_tyres(host, file),
            Err(LoadTyresError::Owner(VehicleTyreError::AlreadyStarted))
        );
        assert_eq!(
            tables.set_tyres_empty(host),
            Err(VehicleTyreError::AlreadyStarted)
        );
        assert_eq!(*host.trace.borrow(), prefix);
    }

    #[test]
    fn reset_releases_old_surfaces_backwards_before_open_and_retains_main_capacity() {
        let mut tables = VehicleTableResources::default();
        let mut host = LoaderHost::default();
        let mut initial = File::new(&host, source(&[1, 1]));
        assert_eq!(tables.load_tyres(&mut host, &mut initial), Ok(true));
        let first = surface_header(&tables, &mut host, 1);
        let last = surface_header(&tables, &mut host, 2);
        let main = (tables.tyres_begin, tables.tyres_capacity);
        host.trace.borrow_mut().clear();
        let mut missing = File::new(&host, Vec::new());
        missing.present = false;
        assert_eq!(tables.load_tyres(&mut host, &mut missing), Ok(false));
        let trace = host.trace.borrow().clone();
        let at_open = trace
            .iter()
            .position(|event| *event == Event::Open)
            .unwrap();
        let releases: Vec<_> = trace[..at_open]
            .iter()
            .filter_map(|event| {
                if let Event::Free(at) = event {
                    Some(*at)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(releases, [last.1, last.0, first.1, first.0]);
        assert!(!trace[..at_open]
            .iter()
            .any(|event| matches!(event, Event::Allocate(_))));
        assert_eq!((tables.tyres_begin, tables.tyres_capacity), main);
        assert_eq!(tables.tyres_end.unwrap() - tables.tyres_begin.unwrap(), 40);
        user_row(&tables, &mut host);
        assert_eq!(tables.live_allocation_count(), 1);
        assert_eq!(tables.tyre_load_state(), TyreLoadState::Complete(false));
    }

    #[test]
    fn missing_and_version_mismatch_are_native_false_but_io_error_is_interrupted() {
        for (present, version, close) in [(false, 1u32, false), (true, 2, true)] {
            let mut tables = VehicleTableResources::default();
            let mut host = LoaderHost::default();
            let mut file = File::new(&host, version.to_le_bytes().to_vec());
            file.present = present;
            assert_eq!(tables.load_tyres(&mut host, &mut file), Ok(false));
            assert_eq!(host.trace.borrow().contains(&Event::Close), close);
            assert!(!file.open);
            user_row(&tables, &mut host);
            assert_eq!(tables.tyre_load_state(), TyreLoadState::Complete(false));
        }
        let mut tables = VehicleTableResources::default();
        let mut host = LoaderHost::default();
        let mut file = File::new(&host, source(&[]));
        file.fail_open = true;
        assert_eq!(
            tables.load_tyres(&mut host, &mut file),
            Err(LoadTyresError::File("open"))
        );
        user_row(&tables, &mut host);
        assert_eq!(
            tables.tyre_load_state(),
            TyreLoadState::Interrupted(TyreOperation::Open)
        );
        assert_locked(&mut tables, &mut host, &mut file);
    }

    #[test]
    fn zero_count_closes_then_resets_to_real_user_row_and_ignores_trailing_bytes() {
        let mut tables = VehicleTableResources::default();
        let mut host = LoaderHost::default();
        let mut bytes = source(&[]);
        bytes.extend_from_slice(b"unused tail");
        let mut file = File::new(&host, bytes);
        // Exercise the caller helper, borrowing the same canonical table owner.
        assert_eq!(
            crate::game_init_car_tables::load_tyres_from_file(&mut tables, &mut host, &mut file),
            Ok(true)
        );
        assert_eq!(file.position, 8);
        assert!(!file.open);
        user_row(&tables, &mut host);
        assert_eq!(tables.live_allocation_count(), 1);
        assert_eq!(tables.tyres_end.unwrap() - tables.tyres_begin.unwrap(), 40);
        assert_eq!(
            tables.tyres_capacity.unwrap() - tables.tyres_begin.unwrap(),
            40
        );
    }

    #[test]
    fn successful_load_deep_copies_exact_bits_then_releases_source_backwards() {
        let mut tables = VehicleTableResources::default();
        let mut host = LoaderHost::default();
        let bytes = source(&[1, 1]);
        let mut file = File::new(&host, bytes.clone());
        assert_eq!(tables.load_tyres(&mut host, &mut file), Ok(true));
        assert_eq!(file.position, bytes.len());
        assert!(!file.open);
        user_row(&tables, &mut host);
        let begin = tables.tyres_begin.unwrap();
        assert_eq!(tables.tyres_end.unwrap() - begin, 120);
        assert_eq!(tables.tyres_capacity.unwrap() - begin, 160);
        let expected = [
            0xf000_0000u32,
            0x447a_0000,
            0x3d0e_fa2e,
            0x4040_0000,
            0x4080_0000,
            0x40a0_0000,
            0x40c0_0000,
            0x3f80_0000,
            0x40e0_0000,
            0x4100_0000,
            0x3f80_0000,
            0x4110_0000,
            0x461c_4000,
            0x3e44_97ff,
            0x4140_0000,
            0x4150_0000,
            0x4160_0000,
            0x4170_0000,
            0x3f80_0000,
            0x4180_0000,
            0x4188_0000,
            0x3f80_0000,
            0x4190_0000,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
        for row in 1..=2 {
            assert_eq!(
                host.memory.read_word(begin + row * 40).unwrap(),
                0x8000_0041 + row
            );
            assert_eq!(host.bytes(begin + row * 40 + 4, 32), [b'@' + row as u8; 32]);
            let (header, data) = surface_header(&tables, &mut host, row);
            assert_eq!(host.bytes(data, 92), expected);
            assert_eq!(host.memory.read_word(header + 8).unwrap(), data + 92);
            assert_eq!(
                host.memory.read_byte(header),
                Err(VehicleMemoryError::Uninitialized(header))
            );
        }
        let trace = host.trace.borrow().clone();
        let close = trace
            .iter()
            .position(|event| *event == Event::Close)
            .unwrap();
        let source_headers: Vec<_> = host
            .allocations
            .iter()
            .filter(|(size, _)| *size == 16)
            .take(2)
            .map(|(_, at)| *at)
            .collect();
        let source_surfaces: Vec<_> = host
            .allocations
            .iter()
            .filter(|(size, _)| *size == 92)
            .take(2)
            .map(|(_, at)| *at)
            .collect();
        let source_rows = host
            .allocations
            .iter()
            .find(|(size, _)| *size == 80)
            .unwrap()
            .1;
        let releases: Vec<_> = trace[close + 1..]
            .iter()
            .filter_map(|event| {
                if let Event::Free(at) = event {
                    Some(*at)
                } else {
                    None
                }
            })
            .collect();
        assert!(releases.ends_with(&[
            source_surfaces[1],
            source_headers[1],
            source_surfaces[0],
            source_headers[0],
            source_rows
        ]));
        assert_eq!(tables.live_allocation_count(), 5);
        assert_eq!(
            tables.live_allocation_count(),
            host.memory.allocations().count()
        );
        assert!(tables.tyre_temporary.as_ref().unwrap().rows.begin == 0);
        assert_eq!(tables.tyre_load_state(), TyreLoadState::Complete(true));
    }

    #[test]
    fn high_bit_counts_enter_the_next_native_read_instead_of_becoming_empty() {
        for surface_count in [false, true] {
            let mut tables = VehicleTableResources::default();
            let mut host = LoaderHost::default();
            let (bytes, field) = if surface_count {
                let mut bytes = source(&[0]);
                bytes[48..52].copy_from_slice(&0x8000_0000u32.to_le_bytes());
                (bytes, TyreReadField::Material)
            } else {
                let mut bytes = source(&[]);
                bytes[4..8].copy_from_slice(&0x8000_0000u32.to_le_bytes());
                (bytes, TyreReadField::Extent)
            };
            let mut file = File::new(&host, bytes);
            assert_eq!(
                tables.load_tyres(&mut host, &mut file),
                Err(LoadTyresError::Owner(VehicleTyreError::Boundary(
                    TyreBoundary::ShortRead {
                        field,
                        expected: 4,
                        actual: 0
                    }
                )))
            );
            assert!(file.open);
            if surface_count {
                assert_ne!(tables.tyre_temporary.as_ref().unwrap().current.surfaces, 0);
            }
            assert_locked(&mut tables, &mut host, &mut file);
        }
    }

    #[test]
    fn zero_surface_row_stops_after_close_and_published_destination_header() {
        let mut tables = VehicleTableResources::default();
        let mut host = LoaderHost::default();
        let mut file = File::new(&host, source(&[0]));
        assert_eq!(
            tables.load_tyres(&mut host, &mut file),
            Err(LoadTyresError::Owner(VehicleTyreError::Boundary(
                TyreBoundary::NativeNullSurfaceHeader
            )))
        );
        assert!(!file.open);
        let begin = tables.tyres_begin.unwrap();
        assert_eq!(tables.tyres_end.unwrap() - begin, 80);
        let (header, data) = surface_header(&tables, &mut host, 1);
        assert_ne!(header, 0);
        assert_eq!(data, 0);
        assert_ne!(tables.tyre_temporary.as_ref().unwrap().rows.begin, 0);
        assert_eq!(
            tables.tyre_load_state(),
            TyreLoadState::Interrupted(TyreOperation::Replace)
        );
        assert_locked(&mut tables, &mut host, &mut file);
    }

    #[test]
    fn null_payload_is_false_without_close_or_nested_cleanup_even_after_prior_row() {
        for prior_row in [false, true] {
            let mut tables = VehicleTableResources::default();
            let mut host = LoaderHost {
                null_allocation: Some(if prior_row { 7 } else { 3 }),
                ..Default::default()
            };
            let mut file = File::new(&host, source(if prior_row { &[1, 1] } else { &[1] }));
            assert_eq!(tables.load_tyres(&mut host, &mut file), Ok(false));
            assert!(file.open);
            assert!(!host.trace.borrow().contains(&Event::Close));
            user_row(&tables, &mut host);
            assert_eq!(tables.tyre_load_state(), TyreLoadState::Complete(false));
            assert_ne!(tables.tyre_temporary.as_ref().unwrap().current.surfaces, 0);
            assert_eq!(tables.tyre_temporary.as_ref().unwrap().rows.begin, 0);
            assert_eq!(
                tables.live_allocation_count(),
                if prior_row { 4 } else { 2 }
            );
            assert_eq!(
                tables.live_allocation_count(),
                host.memory.allocations().count()
            );
        }
    }

    #[test]
    fn partial_payload_initializes_only_actual_bytes_and_never_converts_unknown_tail() {
        let mut tables = VehicleTableResources::default();
        let mut host = LoaderHost::default();
        let mut bytes = source(&[1]);
        bytes.truncate(65); // 60-byte prefix, then five payload bytes
        let mut file = File::new(&host, bytes.clone());
        assert_eq!(
            tables.load_tyres(&mut host, &mut file),
            Err(LoadTyresError::Owner(VehicleTyreError::Boundary(
                TyreBoundary::ShortRead {
                    field: TyreReadField::Payload,
                    expected: 72,
                    actual: 5
                }
            )))
        );
        let payload = tables.tyre_temporary.as_ref().unwrap().payload.unwrap();
        assert_eq!(host.bytes(payload, 5), bytes[60..65]);
        assert_eq!(
            host.memory.read_byte(payload + 5),
            Err(VehicleMemoryError::Uninitialized(payload + 5))
        );
        assert!(file.open);
        assert_eq!(
            tables.tyre_load_state(),
            TyreLoadState::Interrupted(TyreOperation::Read(TyreReadField::Payload))
        );
        assert_locked(&mut tables, &mut host, &mut file);
    }

    #[test]
    fn file_errors_and_unavailable_payload_extents_preserve_the_reached_boundary() {
        for position in [0, 12, 44, 52, 56, 60] {
            let mut tables = VehicleTableResources::default();
            let mut host = LoaderHost::default();
            let mut file = File::new(&host, source(&[1]));
            file.fail_read_at = Some(position);
            assert_eq!(
                tables.load_tyres(&mut host, &mut file),
                Err(LoadTyresError::File("read"))
            );
            assert!(file.open);
            assert_eq!(file.position, position);
            assert_locked(&mut tables, &mut host, &mut file);
        }
        for size in [0u32, 71, 0x8000_0000] {
            let mut tables = VehicleTableResources::default();
            let mut host = LoaderHost::default();
            let mut bytes = source(&[1]);
            bytes[56..60].copy_from_slice(&size.to_le_bytes());
            if size < 72 {
                bytes.truncate(60 + size as usize);
            }
            let mut file = File::new(&host, bytes);
            assert_eq!(
                tables.load_tyres(&mut host, &mut file),
                Err(LoadTyresError::Owner(VehicleTyreError::Boundary(
                    TyreBoundary::ExtentOutsideOwnerDomain(size)
                )))
            );
            assert!(file.open);
            assert_locked(&mut tables, &mut host, &mut file);
        }
        let mut tables = VehicleTableResources::default();
        let mut host = LoaderHost::default();
        let mut file = File::new(&host, source(&[1]));
        file.fail_close = true; // the close effect happened before its error
        assert_eq!(
            tables.load_tyres(&mut host, &mut file),
            Err(LoadTyresError::File("close"))
        );
        assert!(!file.open);
        user_row(&tables, &mut host);
        assert_eq!(tables.tyres_end.unwrap() - tables.tyres_begin.unwrap(), 40);
        assert_ne!(tables.tyre_temporary.as_ref().unwrap().rows.begin, 0);
        assert_locked(&mut tables, &mut host, &mut file);
    }

    #[test]
    fn every_host_failure_keeps_the_exact_reached_prefix_and_blocks_reentry() {
        let mut complete = VehicleTableResources::default();
        let mut baseline = LoaderHost::default();
        let mut file = File::new(&baseline, source(&[2, 1]));
        assert_eq!(complete.load_tyres(&mut baseline, &mut file), Ok(true));
        let events = baseline.trace.borrow().clone();
        for failure in 1..=baseline.effects {
            let mut tables = VehicleTableResources::default();
            let mut host = LoaderHost {
                fail_effect: Some(failure),
                ..Default::default()
            };
            let mut file = File::new(&host, source(&[2, 1]));
            assert_eq!(
                tables.load_tyres(&mut host, &mut file),
                Err(LoadTyresError::Owner(VehicleTyreError::Host(
                    VehicleMemoryError::IntrinsicAdapterRequired
                ))),
                "failure {failure}"
            );
            assert_eq!(host.effects, failure);
            let prefix = host.trace.borrow().clone();
            assert_eq!(prefix, events[..prefix.len()], "failure {failure}");
            assert_eq!(
                tables.live_allocation_count(),
                host.memory.allocations().count()
            );
            assert_locked(&mut tables, &mut host, &mut file);
        }
    }

    #[test]
    fn uncertain_successful_free_is_not_replayed_by_loader_reset_or_host_discard() {
        let mut tables = VehicleTableResources::default();
        let mut host = LoaderHost::default();
        let mut initial = File::new(&host, source(&[1]));
        assert_eq!(tables.load_tyres(&mut host, &mut initial), Ok(true));
        let (_, data) = surface_header(&tables, &mut host, 1);
        host.trace.borrow_mut().clear();
        host.free_then_error = Some(data);
        let mut file = File::new(&host, source(&[]));
        assert_eq!(
            tables.load_tyres(&mut host, &mut file),
            Err(LoadTyresError::Owner(VehicleTyreError::Host(
                VehicleMemoryError::Address(data)
            )))
        );
        assert!(!host.trace.borrow().contains(&Event::Open));
        assert!(!host.memory.allocations().any(|(at, _)| at == data));
        assert!(tables
            .leases
            .iter()
            .any(|lease| lease.allocation == data && lease.release_started));
        assert_locked(&mut tables, &mut host, &mut file);
        assert_eq!(
            tables.discard_host(&mut host.memory),
            Err(VehicleMemoryError::IntrinsicAdapterRequired)
        );
        let remaining: Vec<_> = host.memory.allocations().collect();
        assert_eq!(
            tables.discard_host(&mut host.memory),
            Err(VehicleMemoryError::IntrinsicAdapterRequired)
        );
        assert_eq!(host.memory.allocations().collect::<Vec<_>>(), remaining);
        assert_eq!(
            host.trace
                .borrow()
                .iter()
                .filter(|event| **event == Event::Free(data))
                .count(),
            1
        );
    }

    fn vector(host: &mut LoaderHost, tables: &mut VehicleTableResources) -> (u32, u32) {
        let header = tables.tyre_new_surface_header(host).unwrap();
        let data = tables.tyre_allocate(host, 92).unwrap();
        for offset in (0..92).step_by(4) {
            host.memory.write_word(data + offset, 0x1111_1111).unwrap();
        }
        host.memory.write_word(header + 4, data).unwrap();
        host.memory.write_word(header + 8, data + 92).unwrap();
        host.memory.write_word(header + 12, data + 92).unwrap();
        (header, data)
    }

    #[test]
    fn helper_rereads_begin_after_allocate_and_size_after_free() {
        let mut tables = VehicleTableResources::default();
        let mut host = LoaderHost::default();
        let (header, old) = vector(&mut host, &mut tables);
        let redirected = tables.tyre_allocate(&mut host, 92).unwrap();
        assert_eq!(redirected, old + 92);
        host.on_allocate = Some((host.allocation_calls + 1, vec![(header + 4, redirected)]));
        host.on_free = Some((redirected, vec![(header + 4, 0), (header + 8, 0)]));
        let start = host.trace.borrow().len();
        tables
            .tyre_append(
                &mut host,
                VectorSlot::Surfaces(header),
                RecordSource::Bytes(&[0x22; 92]),
                92,
                AppendKind::Helper,
            )
            .unwrap();
        let new = host.memory.read_word(header + 4).unwrap();
        assert_eq!(host.bytes(new, 92), [0x22; 92]); // redirected prefix is empty
        assert_eq!(host.memory.read_word(header + 8).unwrap(), new + 92);
        assert_eq!(host.memory.read_word(header + 12).unwrap(), new + 184);
        assert!(host.memory.allocations().any(|(at, _)| at == old));
        assert!(!host.memory.allocations().any(|(at, _)| at == redirected));
        assert!(host.trace.borrow()[start..].contains(&Event::Free(redirected)));
    }

    #[test]
    fn setter_surface_size_is_read_after_capacity_store_while_helper_saves_begin_before_it() {
        for kind in [AppendKind::Helper, AppendKind::SetterSurface] {
            let mut tables = VehicleTableResources::default();
            let mut host = LoaderHost::default();
            let (header, old) = vector(&mut host, &mut tables);
            host.on_write = Some((header + 12, vec![(header + 4, 0), (header + 8, 0)]));
            let result = tables.tyre_append(
                &mut host,
                VectorSlot::Surfaces(header),
                RecordSource::Bytes(&[0x33; 92]),
                92,
                kind,
            );
            assert!(!host.memory.allocations().any(|(at, _)| at == old));
            let capacity = host.memory.read_word(header + 12).unwrap();
            match kind {
                AppendKind::Helper => {
                    assert_eq!(
                        result,
                        Err(VehicleTyreError::Boundary(TyreBoundary::InvalidVector))
                    );
                    assert_eq!(host.memory.read_word(header + 4).unwrap(), 0);
                }
                AppendKind::SetterSurface => {
                    assert_eq!(result, Ok(()));
                    let begin = host.memory.read_word(header + 4).unwrap();
                    assert_eq!(host.memory.read_word(header + 8).unwrap(), begin + 92);
                    assert_eq!(capacity, begin + 184);
                }
            }
        }
    }

    #[test]
    fn material_projection_and_reads_do_not_replace_or_reopen_completed_tyres() {
        let mut tables = VehicleTableResources::default();
        let mut host = LoaderHost::default();
        let mut file = File::new(&host, source(&[1]));
        assert_eq!(tables.load_tyres(&mut host, &mut file), Ok(true));
        let tyre = (
            tables.tyres_begin,
            tables.tyres_end,
            tables.tyres_capacity,
            tables.tyre_load_state(),
        );
        let material = vec![0; MATERIAL_SOURCE_STRIDE];
        let (rows, index) = materials(&material).unwrap();
        tables
            .project_materials(&mut host, &material, &rows, index)
            .unwrap();
        tables.phase = Phase::Ready;
        host.trace.borrow_mut().clear();
        assert!(tables
            .read_byte(&mut host, None, 0x63d9b4)
            .unwrap()
            .is_some());
        assert!(tables
            .read_byte(&mut host, None, 0x63d9c8)
            .unwrap()
            .is_some());
        assert_eq!(
            (
                tables.tyres_begin,
                tables.tyres_end,
                tables.tyres_capacity,
                tables.tyre_load_state()
            ),
            tyre
        );
        assert!(host.trace.borrow().is_empty());
    }
}
