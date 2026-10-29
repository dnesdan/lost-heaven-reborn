//! Shared original material/tyre resources for mission cars. Lazy host intake
//! reuses the established source parser and projection; it reconstructs no
//! additional native bootstrap entry. Globals are typed publication fields.
#![forbid(unsafe_code)]
use crate::{
    driving_vehicle_tables::{DrivingVehicleTables, MATERIAL_SOURCE_STRIDE},
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
    TyreSurfaces(usize),
    TyreHeader(usize),
    Tyres,
}
#[derive(Debug)]
struct TableLease {
    allocation: u32,
    bytes: u32,
    _kind: TableKind,
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
    fn project<B: VehicleInitBackend>(
        &mut self,
        host: &mut B,
        tables: DrivingVehicleTables,
    ) -> Result<(), VehicleTableError<B::Error>> {
        let source = self.own(
            host,
            tables.source_material_bytes(),
            TableKind::SourceMaterials,
        )?;
        self.source_materials = Some(source);
        self.source_material_count =
            Some((tables.source_material_bytes().len() / MATERIAL_SOURCE_STRIDE) as u32);
        let materials = self.own(host, tables.material_bytes(), TableKind::Materials)?;
        self.material_index = Some(*tables.material_index_map());
        self.materials = Some(materials);
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
        self.tyres_end = Some(
            begin
                .checked_add(tyre_rows.len() as u32)
                .ok_or_else(|| VehicleTableError::Load("Vehicle tyre table extent".into()))?,
        );
        Ok(())
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
        if self.phase == Phase::Unloaded {
            self.phase = Phase::Loading;
            let result = (|| {
                let installation = installation.ok_or(VehicleTableError::MissingInstallation)?;
                let tables =
                    DrivingVehicleTables::load(installation).map_err(VehicleTableError::Load)?;
                self.project(host, tables)
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
        let surfaces = host.read_word(header + 4).unwrap();
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
