//! Required car frame effects and a lease on the independent Game TCK owner.
use super::*;
use crate::car_effects::{CarEffectsBackend, CarTrackError};
use crate::native_transform::native_trig::car_resource_math as math;

impl<B: VehicleInitBackend> CarEffectsBackend for VehicleFrameBackend<'_, B> {
    fn car_normalize_frame_quaternion(&mut self, frame: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        let state = self
            .arena
            .graph
            .projected_state_mut(key)
            .map_err(ModelError::from)?;
        if !state.rotation.is_verified_input() {
            return Err(VehicleFrameError::NumericDomain("Normalize"));
        }
        state.rotation.normalize();
        Ok(())
    }
    fn car_set_frame_rotation_matrix(&mut self, frame: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        let state = self
            .arena
            .graph
            .projected_state_mut(key)
            .map_err(ModelError::from)?;
        state
            .local
            .try_set_rot3(state.rotation)
            .map_err(|_| VehicleFrameError::NumericDomain("SetRot3"))
    }
    fn car_animation_name(&mut self, id: u32) -> Result<Option<Vec<u8>>, Self::Error> {
        self.car_tracks
            .as_ref()
            .ok_or(VehicleFrameError::CarTrack(CarTrackError::OwnerRequired))?
            .animation_name(id)
            .map_err(VehicleFrameError::CarTrack)
    }
    fn car_track_placement(
        &mut self,
        mut memory: Option<VehicleComponentTailMemory<'_>>,
        name: &[u8],
        position: u32,
        direction: u32,
    ) -> Result<bool, Self::Error> {
        let installation = self.installation;
        let owner = self
            .car_tracks
            .as_mut()
            .ok_or(VehicleFrameError::CarTrack(CarTrackError::OwnerRequired))?;
        let track = owner
            .load_with(name, |path| {
                match installation
                    .ok_or(CarTrackError::InstallationRequired)?
                    .read_original_path(path)
                {
                    Ok(bytes) => Ok(Some(bytes)),
                    Err(error) if error.0 == "original asset unavailable" => Ok(None),
                    Err(error) => Err(CarTrackError::Asset(error.0)),
                }
            })
            .map_err(VehicleFrameError::CarTrack)?;
        let Some(track) = track else {
            return Ok(false);
        };
        let first = track
            .positions
            .first()
            .ok_or(VehicleFrameError::CarTrack(CarTrackError::MissingPositions))?;
        let last = track.positions.last().unwrap();
        let mut displacement = [0.; 3];
        for field in 0..3 {
            displacement[field] =
                math::subtract(f32::from_bits(first[field]), f32::from_bits(last[field]))
                    .map_err(|_| VehicleFrameError::CarTrack(CarTrackError::NumericDomain))?;
        }
        macro_rules! write {
            ($address:expr, $value:expr) => {
                write_projected(self, &mut memory, $address, $value)
            };
        }
        for (field, value) in displacement.into_iter().enumerate() {
            write!(position.wrapping_add(field as u32 * 4), value.to_bits())?;
        }
        if track.rotations.is_empty() {
            for (field, value) in [0u32, 0, 0x3f800000].into_iter().enumerate() {
                write!(direction.wrapping_add(field as u32 * 4), value)?;
            }
            return Ok(true);
        }
        let first = track.rotations.first().unwrap().map(f32::from_bits);
        let last = track.rotations.last().unwrap().map(f32::from_bits);
        let angle = math::animation_angle(first, last)
            .map_err(|_| VehicleFrameError::CarTrack(CarTrackError::NumericDomain))?;
        for (field, value) in [0u32, 0, 0x3f800000].into_iter().enumerate() {
            write!(direction.wrapping_add(field as u32 * 4), value)?;
        }
        let input_direction = read_projected_vector(self, &mut memory, direction)?;
        let rotated = math::rotate_y(input_direction, angle)
            .map_err(|_| VehicleFrameError::CarTrack(CarTrackError::NumericDomain))?;
        for (field, value) in rotated.into_iter().enumerate() {
            write!(direction.wrapping_add(field as u32 * 4), value.to_bits())?;
        }
        write!(direction.wrapping_add(4), 0)?;
        let input_direction = read_projected_vector(self, &mut memory, direction)?;
        let normalized = math::normalize_horizontal(input_direction)
            .map_err(|_| VehicleFrameError::CarTrack(CarTrackError::NumericDomain))?;
        match normalized {
            math::HorizontalNormalization::Unchanged => {}
            math::HorizontalNormalization::Axis { axis, value } => {
                write!(direction.wrapping_add(axis as u32 * 4), value.to_bits())?;
            }
            math::HorizontalNormalization::Scaled(values) => {
                for (field, value) in values.into_iter().enumerate() {
                    write!(direction.wrapping_add(field as u32 * 4), value.to_bits())?;
                }
            }
        }
        // Direction stores can alias the position output. The native final
        // rotation reads the live position, rather than the earlier delta.
        let live_position = read_projected_vector(self, &mut memory, position)?;
        let rotated = math::rotate_y(live_position, angle)
            .map_err(|_| VehicleFrameError::CarTrack(CarTrackError::NumericDomain))?;
        for (field, value) in rotated.into_iter().enumerate() {
            write!(position.wrapping_add(field as u32 * 4), value.to_bits())?;
        }
        Ok(true)
    }
}

fn write_projected<B: VehicleInitBackend>(
    backend: &mut B,
    memory: &mut Option<VehicleComponentTailMemory<'_>>,
    address: u32,
    value: u32,
) -> Result<(), B::Error> {
    match memory.as_mut() {
        Some(memory) => memory.write_word_with_backend(backend, address, value),
        None => backend.write_word(address, value),
    }
}
fn read_projected_vector<B: VehicleInitBackend>(
    backend: &mut B,
    memory: &mut Option<VehicleComponentTailMemory<'_>>,
    address: u32,
) -> Result<[f32; 3], B::Error> {
    let mut vector = [0.; 3];
    for (field, value) in vector.iter_mut().enumerate() {
        let address = address.wrapping_add(field as u32 * 4);
        *value = f32::from_bits(match memory.as_mut() {
            Some(memory) => memory.read_word_with_backend(backend, address)?,
            None => backend.read_word(address)?,
        });
    }
    Ok(vector)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car_effects::CarTrackResources;

    fn track_bytes(rotations: &[[f32; 3]]) -> Vec<u8> {
        let mut words = vec![4u32, 0, 0, 0, 0, 0, 0, 1000, 20, 2];
        for value in [[4f32, 5., 6.], [1., 2., 3.]].into_iter().flatten() {
            words.push(value.to_bits());
        }
        words.push(rotations.len() as u32);
        for vector in rotations {
            words.extend(vector.map(f32::to_bits));
        }
        words.into_iter().flat_map(u32::to_le_bytes).collect()
    }
    fn cached_track(rotations: &[[f32; 3]]) -> CarTrackResources {
        let bytes = track_bytes(rotations);
        let mut owner = CarTrackResources::from_unique_catalog(vec![]).unwrap();
        owner
            .load_with(b"synthetic.tck", |_| Ok(Some(bytes.clone())))
            .unwrap();
        owner
    }
    #[test]
    fn uncached_track_uses_original_windows_path_then_retains_cached_metadata() {
        struct Directory(std::path::PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                std::fs::remove_dir_all(&self.0).unwrap();
            }
        }
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("lhr-car-track-{}-{unique}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        let directory = Directory(path);
        std::fs::create_dir(directory.0.join("anims")).unwrap();
        let file = directory.0.join("anims/synthetic.tck");
        std::fs::write(&file, track_bytes(&[])).unwrap();
        let installation = lhr_assets::Installation::open(&directory.0).unwrap();
        let mut arena = ModelArena::default();
        let mut memory = VehicleFrameMemory::default();
        memory.map_bytes(0x1000, &[0xa5; 24]).unwrap();
        let mut owner = CarTrackResources::from_unique_catalog(vec![]).unwrap();
        {
            let mut backend =
                VehicleFrameBackend::with_installation(&mut arena, &mut memory, &installation)
                    .with_car_track_resources(&mut owner);
            assert_eq!(
                backend.car_track_placement(None, b"synthetic.tck", 0x1000, 0x100c),
                Ok(true)
            );
            std::fs::remove_file(&file).unwrap();
            assert_eq!(
                backend.car_track_placement(None, b"SYNTHETIC.TCK", 0x1000, 0x100c),
                Ok(true)
            );
        }
        assert_eq!(owner.cached_tracks().count(), 1);
        assert_eq!(owner.cached_tracks().next().unwrap().0, b"synthetic.tck");
        assert_eq!(memory.read_word(0x1000).unwrap(), 3f32.to_bits());
        assert_eq!(memory.read_word(0x1014).unwrap(), 1f32.to_bits());
    }
    #[test]
    fn cached_placement_needs_no_installation_and_survives_car_backend_lease() {
        let mut arena = ModelArena::default();
        let mut memory = VehicleFrameMemory::default();
        memory.map_bytes(0x1000, &[0xa5; 24]).unwrap();
        let mut owner = cached_track(&[]);
        {
            let mut b = VehicleFrameBackend::new(&mut arena, &mut memory)
                .with_car_track_resources(&mut owner);
            assert_eq!(
                b.car_track_placement(None, b"SYNTHETIC.TCK", 0x1000, 0x100c),
                Ok(true)
            );
        }
        assert_eq!(owner.cached_tracks().count(), 1);
        assert_eq!(memory.read_word(0x1000).unwrap(), 3f32.to_bits());
        assert_eq!(memory.read_word(0x1004).unwrap(), 3f32.to_bits());
        assert_eq!(memory.read_word(0x1008).unwrap(), 3f32.to_bits());
        assert_eq!(memory.read_word(0x100c).unwrap(), 0);
        assert_eq!(memory.read_word(0x1010).unwrap(), 0);
        assert_eq!(memory.read_word(0x1014).unwrap(), 1f32.to_bits());
        owner.reset_track_cache();
        assert_eq!(owner.cached_tracks().count(), 0);
    }
    #[test]
    fn unavailable_rotation_keeps_completed_position_and_cached_metadata() {
        let mut arena = ModelArena::default();
        let mut memory = VehicleFrameMemory::default();
        memory.map_bytes(0x1000, &[0xa5; 24]).unwrap();
        let mut owner = cached_track(&[[f32::NAN, 0., 1.], [0., 0., 1.]]);
        {
            let mut b = VehicleFrameBackend::new(&mut arena, &mut memory)
                .with_car_track_resources(&mut owner);
            assert_eq!(
                b.car_track_placement(None, b"synthetic.tck", 0x1000, 0x100c),
                Err(VehicleFrameError::CarTrack(CarTrackError::NumericDomain))
            );
        }
        assert_eq!(memory.read_word(0x1000).unwrap(), 3f32.to_bits());
        assert_eq!(memory.read_word(0x100c).unwrap(), 0xa5a5a5a5);
        assert_eq!(owner.cached_tracks().count(), 1);
    }
}
