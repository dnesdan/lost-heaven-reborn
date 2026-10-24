//! GameInit effects on the existing frame arena and canonical car projection.
//! Unbound world/audio/model-resource operations retain their reached failure.
use super::*;
#[path = "vehicle_frame_skid.rs"]
mod skid;

fn game_failure<E>(error: CarGameInitUnavailable<VehicleFrameError<E>>) -> VehicleFrameError<E> {
    match error {
        CarGameInitUnavailable::Effect(error) => error,
        CarGameInitUnavailable::NumericDomain(operation) => {
            VehicleFrameError::NumericDomain(operation)
        }
        CarGameInitUnavailable::Dependency { entry } => {
            VehicleFrameError::UnavailableGameDependency(entry)
        }
        CarGameInitUnavailable::Storage => {
            VehicleFrameError::UnavailableFrameMethod("GameInit projection storage")
        }
    }
}

impl<B: VehicleComponentTailBackend> VehicleFrameBackend<'_, B> {
    fn game_stack(&self, entry: u32) -> Result<u32, VehicleFrameError<B::Error>> {
        self.game_init_stack
            .ok_or(VehicleFrameError::UnavailableGameDependency(entry))
    }
    pub(super) fn game_virtual(
        &mut self,
        object: u32,
        slot: u32,
        arguments: &[u32],
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        if let Some(result) = self.game_audio_virtual(object, slot, arguments)? {
            return Ok(result);
        }
        if let Some(result) = self.game_host_virtual(object, slot, arguments)? {
            return Ok(result);
        }
        if (RESOURCE_BASE..0xc100_0000).contains(&object) {
            if slot != 0 || !arguments.is_empty() {
                return Err(VehicleFrameError::UnavailableFrameMethod(
                    "GameInit texture virtual",
                ));
            }
            let key = self.texture_key(object)?;
            self.arena.release_cpu_texture(key)?;
            return Ok(self
                .arena
                .loaded_visual_resources()
                .and_then(|r| r.backend().texture_references(key))
                .unwrap_or(0));
        }
        if let Some(result) = self.game_mesh_virtual(object, slot, arguments)? {
            return Ok(result);
        }
        if (0xc400_0000..0xc500_0000).contains(&object) {
            if let (0x14, [texture]) = (slot, arguments) {
                let material = self.material_key(object)?;
                let texture = if *texture == 0 {
                    None
                } else {
                    Some(self.texture_key(*texture)?)
                };
                self.arena
                    .visual_resource_owner_mut()?
                    .game_set_material_texture(material, texture)
                    .map_err(ModelError::LoadedVisual)?;
                return Ok(0);
            }
            return Err(VehicleFrameError::UnavailableFrameMethod(
                "GameInit material virtual",
            ));
        }
        let key = self.frame_key(object)?;
        match (slot, arguments) {
            (0, []) => {
                let references = self.arena.frame(key)?.references;
                self.arena.release(key)?;
                Ok(references.wrapping_sub(1))
            }
            (0x18, []) => {
                self.arena.graph.update(key).map_err(ModelError::from)?;
                Ok(0)
            }
            (0x24, [enabled]) => {
                self.arena.set_frame_on(key, *enabled)?;
                Ok(0)
            }
            (0x54, [light]) if self.arena.state(key)?.kind == FrameKind::Sector => {
                let light = self.frame_key(*light)?;
                self.arena
                    .sector_delete_light(crate::scene_ownership::SectorKey(key), light)
                    .map_err(VehicleFrameError::from)
            }
            (0x54, [mode]) | (0x5c, [mode]) => {
                let kind = self.arena.state(key)?.kind;
                if slot == 0x5c && kind == FrameKind::Light {
                    self.arena
                        .graph
                        .set_light_diffuse_power(key, f32::from_bits(*mode))
                        .map_err(ModelError::from)?;
                    return Ok(0);
                }
                if !matches!(
                    kind,
                    FrameKind::Mesh | FrameKind::Singlemesh | FrameKind::Projector
                ) {
                    return Err(VehicleFrameError::UnavailableFrameMethod(
                        "GameInit visual mode",
                    ));
                }
                let flags = &mut self
                    .arena
                    .graph
                    .projected_state_mut(key)
                    .map_err(ModelError::from)?
                    .visual_flags;
                let (first, second) = if slot == 0x54 {
                    (0x200, 0x400)
                } else {
                    (0x2000, 0x4000)
                };
                match mode {
                    0 => *flags &= !(first | second),
                    1 => *flags = (*flags & !second) | first,
                    2 => *flags |= first | second,
                    _ => {}
                }
                Ok(0)
            }
            (0x70, [texture]) if self.arena.state(key)?.kind == FrameKind::Projector => {
                let texture = if *texture == 0 {
                    None
                } else {
                    Some(self.texture_key(*texture)?)
                };
                self.arena.set_projector_texture(key, texture)?;
                Ok(0)
            }
            (0x2c, [parent, flags]) => {
                self.bodycol_link_frame(object, *parent, *flags)?;
                Ok(0)
            }
            (0x30, [index]) => {
                let mut child = self.arena.links(key)?.child;
                for _ in 0..*index {
                    let current = child.ok_or(VehicleFrameError::Address(*index))?;
                    child = self.arena.links(current)?.next;
                }
                let child = child.ok_or(VehicleFrameError::Address(*index))?;
                self.frame_address(child)
            }
            _ => Err(VehicleFrameError::UnavailableFrameMethod(
                "GameInit frame virtual",
            )),
        }
    }
    pub(crate) fn game_vehicle_dependency(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        vehicle: u32,
        entry: u32,
        arguments: [u32; 2],
    ) -> Result<u32, VehicleFrameError<B::Error>>
    where
        B: VehicleComponentTailBackend,
    {
        use VehicleGameInitResourceDependency as Resource;
        use VehicleGameInitSmallDependency as Small;
        // Vehicle GameInit's full saved-register frame is main ESP-0xfc.
        // These workers take only an output scratch slot; glass specifically
        // uses the first argument at that caller ESP-8, main ESP-0x104.
        let stack = self.game_stack(entry)?.wrapping_sub(0x104);
        let small = match entry {
            0x4d54e0 => Some(Small::InitDamage),
            0x4ce190 => Some(Small::ClearBodyCollAddTrack {
                header: arguments[0],
            }),
            0x4ce290 => Some(Small::AddProjectorLightsToSectors),
            0x4cdcc0 => Some(Small::SetDynamicCollsOn {
                first: arguments[0] as u8,
                second: arguments[1] as u8,
            }),
            _ => None,
        };
        if let Some(dependency) = small {
            vehicle_game_init_small_dependency(self, memory, vehicle, stack, dependency)
                .map_err(game_failure)?;
            return Ok(0);
        }
        if entry == 0x4d6840 {
            self.game_update_axle_visuals(memory, vehicle)?;
            return Ok(0);
        }
        let dependency = match entry {
            0x4cc6e0 => Resource::SetSeatMass {
                index: arguments[0] as i32,
                mass: arguments[1],
            },
            0x4eb5e0 => Resource::ReleaseSounds,
            0x4eac70 => Resource::InitSounds {
                source: arguments[0],
            },
            0x4eb110 => Resource::AddSoundsToSectors,
            0x4d5150 => Resource::InitDeformGlassTexture {
                first: arguments[0],
                second: arguments[1],
            },
            0x4d51f0 => Resource::InitDeform {
                preserve_vertices: arguments[0] as u8 != 0,
            },
            0x4eec80 => Resource::StopSounds,
            0x4d7750 => Resource::DoDoors {
                seconds: arguments[0],
            },
            0x4d7cd0 => Resource::DoLights {
                seconds: arguments[0],
            },
            _ => return Err(VehicleFrameError::UnavailableGameDependency(entry)),
        };
        vehicle_game_init_resource_dependency(self, memory, vehicle, stack, dependency)
            .map_err(game_failure)?;
        Ok(0)
    }
}

impl<B: VehicleComponentTailBackend> CarGameInitBackend for VehicleFrameBackend<'_, B> {
    fn game_init_effect(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: CarGameInitEffect,
    ) -> Result<u32, Self::Error> {
        match effect {
            CarGameInitEffect::CarDependency {
                entry: 0x405fa0,
                car,
                ..
            } => {
                // CALL at41ed1c, then SUB10/PUSHESI and the nonnull-frame
                // PUSHEBP/PUSHEDI at405fc4/405fc8. No detached actor storage.
                let stack = self.game_stack(0x405fa0)?.wrapping_sub(0x20);
                actor_game_init(self, memory, car, stack).map_err(game_failure)?;
                Ok(0)
            }
            CarGameInitEffect::CarDependency {
                entry: 0x407cd0,
                car,
                argument,
            } => {
                // Debustize uses no scratch; its support worker still receives
                // this call's evidenced post-prologue context from Actor.
                let stack = self.game_stack(0x407cd0)?.wrapping_sub(0x24);
                car_game_init_support(self, memory, car, stack, 0x407cd0, argument)
                    .map_err(game_failure)
            }
            CarGameInitEffect::CarDependency {
                entry: 0x407d10,
                car: frame,
                ..
            } => {
                self.game_clear_visual_shadows(frame)?;
                Ok(0)
            }
            CarGameInitEffect::CarDependency { .. } => self.game_host_init(memory, effect),
            CarGameInitEffect::VehicleDependency {
                entry,
                vehicle,
                arguments,
            } => self.game_vehicle_dependency(memory, vehicle, entry, arguments),
            CarGameInitEffect::FrameVirtual {
                frame,
                slot,
                arguments,
                count,
            } => {
                let arguments = arguments.get(..usize::from(count)).ok_or(
                    VehicleFrameError::UnavailableFrameMethod("GameInit virtual argument count"),
                )?;
                self.game_virtual(frame, slot, arguments)
            }
            CarGameInitEffect::LinkFrame {
                frame,
                parent,
                flags,
            } => {
                self.bodycol_link_frame(frame, parent, flags)?;
                Ok(0)
            }
            CarGameInitEffect::SetFrameOn { frame, enabled } => {
                let key = self.frame_key(frame)?;
                self.arena.set_frame_on(key, u32::from(enabled))?;
                Ok(0)
            }
            CarGameInitEffect::SetFrameDirection { frame, direction } => {
                self.set_axis_frame_direction(frame, direction)?;
                Ok(0)
            }
            CarGameInitEffect::MaterializeQuaternion { frame } => {
                self.materialize_game_quaternion(frame)?;
                Ok(0)
            }
            CarGameInitEffect::NormalizeFrameQuaternion { frame } => {
                self.normalize_frame_quaternion(frame)?;
                Ok(0)
            }
            CarGameInitEffect::SetFrameRotationMatrix { frame } => {
                self.set_frame_rotation_matrix(frame)?;
                Ok(0)
            }
            CarGameInitEffect::UpdateFrame { frame } => {
                let key = self.frame_key(frame)?;
                self.arena.graph.update(key).map_err(ModelError::from)?;
                Ok(0)
            }
            CarGameInitEffect::UpdateWorldMatrix { frame } => {
                self.bodycol_update_world(frame)?;
                Ok(0)
            }
            CarGameInitEffect::RemoveModelFrame { model, frame } => {
                self.bodycol_remove_model_child(model, frame)?;
                Ok(0)
            }
            CarGameInitEffect::ReleaseFrame { frame } => {
                let key = self.frame_key(frame)?;
                let references = self.arena.frame(key)?.references;
                self.arena.release(key)?;
                Ok(references.wrapping_sub(1))
            }
            CarGameInitEffect::InsertVehicleCollisionPointer {
                header,
                insertion,
                source,
            } => {
                let mut projection = GameProjection {
                    backend: self,
                    memory,
                };
                crate::vehicle_component_records::vehicle_component_pointers_insert(
                    &mut projection,
                    header,
                    insertion,
                    1,
                    source,
                )?;
                Ok(0)
            }
            CarGameInitEffect::FlagsCallback { .. }
            | CarGameInitEffect::DynamicCollision { .. } => self.game_host_init(memory, effect),
            CarGameInitEffect::InsertVehicleCollision {
                header,
                insertion,
                record,
            } => {
                let mut projection = GameProjection {
                    backend: self,
                    memory,
                };
                crate::vehicle_component_records::vehicle_collision_records_insert(
                    &mut projection,
                    header,
                    insertion,
                    1,
                    record,
                )?;
                Ok(0)
            }
            CarGameInitEffect::InitializeSkidModels {
                vehicle,
                position_pointer_slot,
            } => {
                self.game_initialize_skid_models(memory, vehicle, position_pointer_slot)?;
                Ok(0)
            }
            CarGameInitEffect::AddSectorLight { sector, light } => {
                let sector = crate::scene_ownership::SectorKey(self.frame_key(sector)?);
                let light = self.frame_key(light)?;
                self.arena.sector_add_light(sector, light)?;
                Ok(0)
            }
            CarGameInitEffect::AddUsingObject { manager, object } => {
                let parameter = self.game_stack(0x55dd60)?.wrapping_sub(4);
                if memory.read_word_with_backend(self, parameter)? != object {
                    return Err(VehicleFrameError::UnavailableFrameMethod(
                        "using-object caller parameter",
                    ));
                }
                using_object_add_object(self, memory, manager, object, parameter)
                    .map_err(game_failure)?;
                Ok(0)
            }
            CarGameInitEffect::CreateShadowModel { .. } => {
                let model = self.arena.create_model()?;
                self.frame_address(model.root())
            }
            CarGameInitEffect::OpenShadowModel { model } => {
                self.game_open_model(model, "cars low shadow.I3D")?;
                Ok(0)
            }
            CarGameInitEffect::FindShadowBox { model } => self.game_find_frame(model, b"Box01"),
            CarGameInitEffect::SetFrameSector {
                scene,
                frame,
                position,
            } => {
                let scene = crate::scene_ownership::SceneKey(self.frame_key(scene)?);
                let frame = self.frame_key(frame)?;
                let mut point = [0.; 3];
                for (i, value) in point.iter_mut().enumerate() {
                    *value = f32::from_bits(
                        memory.read_word_with_backend(self, position.wrapping_add(i as u32 * 4))?,
                    );
                }
                self.arena
                    .set_frame_sector_pos(scene, frame, point)
                    .map_err(VehicleFrameError::from)
            }
        }
    }
}
impl<B: VehicleComponentTailBackend> VehicleFrameBackend<'_, B> {
    fn game_update_axle_visuals(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        vehicle: u32,
    ) -> Result<(), VehicleFrameError<B::Error>>
    where
        B: VehicleComponentTailBackend,
    {
        update_vehicle_axes(self, memory, vehicle).map_err(|failure| match failure {
            VehicleWheelsUnavailable::Init(VehicleInitUnavailable::Effect(error)) => error,
            VehicleWheelsUnavailable::Init(VehicleInitUnavailable::Storage(address)) => {
                VehicleFrameError::Address(address)
            }
            VehicleWheelsUnavailable::NumericDomain => {
                VehicleFrameError::NumericDomain("Vehicle UpdateAxis")
            }
            _ => VehicleFrameError::UnavailableFrameMethod("Vehicle UpdateAxis projection"),
        })
    }
    fn game_clear_visual_shadows(&mut self, frame: u32) -> Result<(), VehicleFrameError<B::Error>> {
        if frame == 0 {
            return Ok(());
        }
        let key = self.frame_key(frame)?;
        if matches!(
            self.arena.state(key)?.kind,
            FrameKind::Mesh | FrameKind::Singlemesh | FrameKind::Projector
        ) {
            self.game_virtual(frame, 0x54, &[0])?;
        }
        let mut index = self.arena.links(key)?.child_count;
        while index != 0 {
            index -= 1;
            let child = self.game_virtual(frame, 0x30, &[index])?;
            self.game_clear_visual_shadows(child)?;
        }
        Ok(())
    }
    fn game_open_model(
        &mut self,
        address: u32,
        name: &str,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        let model = ModelKey(self.frame_key(address)?);
        let installation = self
            .installation
            .ok_or(VehicleFrameError::UnavailableFrameMethod(
                "GameInit model installation",
            ))?;
        // The Game model cache searches its original Models directory before
        // calling the library Open method, whose filename rewrite adds .4ds.
        let path = format!("Models\\{name}");
        let mut content = self
            .arena
            .open_original_static_model(model, installation, &path)
            .map_err(|failure| {
                VehicleFrameError::Arena(match failure.error {
                    ModelError::Asset(message) => ModelError::Asset(format!("{path}: {message}")),
                    error => error,
                })
            })?;
        self.arena.transfer_shared_model_content(&mut content)?;
        Ok(())
    }
    fn game_find_frame(&self, root: u32, name: &[u8]) -> Result<u32, VehicleFrameError<B::Error>> {
        let key = self.frame_key(root)?;
        let lookup = crate::frame_lookup::ModelFrameLookup::borrow(self.arena, true);
        let reference = lookup
            .reference(key)
            .map_err(|_| VehicleFrameError::UnavailableFrameMethod("GameInit model root lookup"))?;
        let found = crate::frame_lookup::get_child_frame(
            &lookup,
            reference,
            name,
            crate::frame_lookup::SCRIPT_FIND_FLAGS,
            crate::frame_lookup::MAX_LOOKUP_VISITS,
        )
        .map_err(|_| VehicleFrameError::UnavailableFrameMethod("GameInit model frame lookup"))?;
        found.map_or(Ok(0), |reference| {
            self.frame_address(FrameKey(reference.identity()))
        })
    }
    fn materialize_game_quaternion(
        &mut self,
        frame: u32,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        let key = self.frame_key(frame)?;
        let state = self
            .arena
            .graph
            .projected_state_mut(key)
            .map_err(ModelError::from)?;
        // Imported S_quat::Make consumes the already stored local matrix.
        state.rotation = SQuat::try_make_from_matrix(&state.local)
            .map_err(|_| VehicleFrameError::NumericDomain("S_quat::Make"))?;
        Ok(())
    }
}
impl<B: VehicleComponentTailBackend> CarGameInitSupportBackend for VehicleFrameBackend<'_, B> {
    fn car_game_init_support_effect(
        &mut self,
        mut memory: VehicleComponentTailMemory<'_>,
        effect: CarGameInitSupportEffect,
    ) -> Result<u32, Self::Error> {
        match effect {
            CarGameInitSupportEffect::EnumerateShadowReceivers {
                root,
                callback,
                context,
                mask,
                pattern,
            } => {
                if callback != 0x4d0eb0 {
                    return Err(VehicleFrameError::UnavailableGameDependency(callback));
                }
                let pattern = if pattern == 0x6323b4 {
                    b"*".to_vec()
                } else {
                    let mut bytes = Vec::new();
                    for offset in 0..32768u32 {
                        let byte =
                            memory.read_byte_with_backend(self, pattern.wrapping_add(offset))?;
                        if byte == 0 {
                            break;
                        }
                        bytes.push(byte);
                        if offset == 32767 {
                            return Err(VehicleFrameError::UnavailableFrameMethod(
                                "GameInit frame pattern",
                            ));
                        }
                    }
                    bytes
                };
                self.enumerate_frames(root, &pattern, mask, |backend, frame| {
                    backend.collect_frame_projected(memory.reborrow(), context, frame)
                })
                .map_err(|failure| match failure {
                    VehicleInitUnavailable::Effect(error) => error,
                    VehicleInitUnavailable::Storage(address) => VehicleFrameError::Address(address),
                    _ => VehicleFrameError::UnavailableFrameMethod("GameInit shadow enumeration"),
                })?;
                Ok(0)
            }
            CarGameInitSupportEffect::VehicleSoundDependency { entry, vehicle } => {
                self.game_vehicle_dependency(memory, vehicle, entry, [0, 0])
            }
            _ => self.game_host_support(memory, effect),
        }
    }
}
impl<B: VehicleComponentTailBackend> CarGameInitResourceBackend for VehicleFrameBackend<'_, B> {
    fn vehicle_resource_effect(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: VehicleResourceEffect,
    ) -> Result<u32, Self::Error> {
        match effect {
            VehicleResourceEffect::MeshVirtual {
                object,
                slot,
                arguments,
                count,
            } => {
                let arguments = arguments.get(..usize::from(count)).ok_or(
                    VehicleFrameError::UnavailableFrameMethod("GameInit resource argument count"),
                )?;
                self.game_virtual(object, slot, arguments)
            }
            VehicleResourceEffect::Dependency {
                entry,
                vehicle,
                arguments,
            } => self.game_vehicle_dependency(memory, vehicle, entry, arguments),
            VehicleResourceEffect::TextureOpen {
                name,
                extension,
                flags,
                output,
                ..
            } => {
                if flags != 4 {
                    return self.game_host_resource(memory, effect);
                }
                // IGraph::CreateITexture clears the caller slot before lookup.
                let mut memory = memory;
                memory.write_word_with_backend(self, output, 0)?;
                let mut names = Vec::new();
                for pointer in [name, extension] {
                    let mut bytes = Vec::new();
                    for i in 0..32768u32 {
                        let byte = memory.read_byte_with_backend(self, pointer.wrapping_add(i))?;
                        if byte == 0 {
                            break;
                        }
                        bytes.push(byte);
                        if i == 32767 {
                            return Err(VehicleFrameError::UnavailableFrameMethod(
                                "glass texture name length",
                            ));
                        }
                    }
                    names.push(String::from_utf8(bytes).map_err(|_| {
                        VehicleFrameError::UnavailableFrameMethod("glass texture filename encoding")
                    })?);
                }
                let installation =
                    self.installation
                        .ok_or(VehicleFrameError::UnavailableFrameMethod(
                            "glass texture installation",
                        ))?;
                let key = self
                    .arena
                    .visual_resource_owner_mut()?
                    .game_load_glass_texture(installation, &names[0], &names[1], flags)
                    .map_err(ModelError::LoadedVisual)?;
                let address = self.texture_address(key)?;
                memory.write_word_with_backend(self, output, address)?;
                Ok(0)
            }
            VehicleResourceEffect::CreateSound { .. }
            | VehicleResourceEffect::LoadSound { .. }
            | VehicleResourceEffect::SoundVirtual { .. }
            | VehicleResourceEffect::AddSectorSound { .. }
            | VehicleResourceEffect::DeleteSectorSound { .. } => {
                self.game_audio_resource(memory, effect)
            }
        }
    }
}

impl<B: VehicleComponentTailBackend> CarGameDoneBackend for VehicleFrameBackend<'_, B> {
    fn game_done_effect(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: CarGameDoneEffect,
    ) -> Result<(), Self::Error> {
        match effect {
            CarGameDoneEffect::VehicleGameDone { vehicle } => {
                // Outer GameDone's post-prologue ESP, CALL return, then the
                // Vehicle's SUB24 and four saved registers (PC0x4c55a0).
                let stack = self.game_stack(0x4c55a0)?.wrapping_sub(0x38);
                vehicle_game_done(self, memory, vehicle, stack).map_err(game_failure)
            }
            CarGameDoneEffect::ShadowCarOn { car, enabled } => {
                let stack = self.game_stack(0x429bb0)?.wrapping_sub(0xc);
                car_game_init_support(self, memory, car, stack, 0x429bb0, u32::from(enabled))
                    .map(|_| ())
                    .map_err(game_failure)
            }
            _ => self.game_host_done(memory, effect),
        }
    }
}

struct GameProjection<'a, 'memory, 'arena, B: VehicleInitBackend> {
    backend: &'a mut VehicleFrameBackend<'arena, B>,
    memory: VehicleComponentTailMemory<'memory>,
}
impl<B: VehicleComponentsBackend> VehicleInitBackend for GameProjection<'_, '_, '_, B> {
    type Error = VehicleFrameError<B::Error>;
    fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
        self.memory.read_byte_with_backend(self.backend, address)
    }
    fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
        self.memory.read_word_with_backend(self.backend, address)
    }
    fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
        self.memory
            .write_byte_with_backend(self.backend, address, value)
    }
    fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
        self.memory
            .write_word_with_backend(self.backend, address, value)
    }
    fn free(&mut self, allocation: u32) -> Result<(), Self::Error> {
        self.backend.free_projected(&mut self.memory, allocation)
    }
    fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
        self.backend.allocate_projected(&mut self.memory, bytes)
    }
    fn collect_frame(
        &mut self,
        vector: &mut VehicleInitFrameVector,
        frame: u32,
    ) -> Result<(), Self::Error> {
        self.backend.collect_frame(vector, frame)
    }
    fn enumerate(
        &mut self,
        model: u32,
        query: VehicleInitQuery,
        target: &mut VehicleInitEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.backend.enumerate(model, query, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CAR: u32 = 0x0010_0000;
    const STACK: u32 = 0x0084_0000;
    fn state() -> (VehicleInitState, VehicleComponentTailScratch) {
        let state = VehicleInitState::from_car_bytes(
            CAR,
            vec![0; 0x2370],
            CarInitScratch {
                address: 0x0080_0000,
                bytes: [0x33; CAR_INIT_STACK_BYTES],
            },
            CarInitRegisters {
                ebx: 1,
                ebp: 2,
                esi: 3,
                edi: 4,
            },
        )
        .unwrap();
        let scratch = VehicleComponentTailScratch {
            address: STACK - 0x120,
            bytes: [0x44; VEHICLE_INIT_STACK_BYTES],
        };
        (state, scratch)
    }
    fn actor(
        kind: u32,
    ) -> (
        ModelArena,
        VehicleFrameMemory,
        VehicleInitState,
        VehicleComponentTailScratch,
        FrameKey,
    ) {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let mut heap = VehicleFrameMemory::default();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut heap);
        let frame = backend.frame_address(model.root()).unwrap();
        let (mut state, mut scratch) = state();
        let mut memory = state.car_live_memory(&mut scratch).unwrap();
        memory
            .write_word_with_backend(&mut backend, CAR + 0x10, kind)
            .unwrap();
        memory
            .write_word_with_backend(&mut backend, CAR + 0x1c, 0xdeadbeef)
            .unwrap();
        memory
            .write_word_with_backend(&mut backend, CAR + 0x68, frame)
            .unwrap();
        (arena, heap, state, scratch, model.root())
    }
    #[test]
    fn actor_initialization_updates_the_same_car_frame_and_heap() {
        let (mut arena, mut heap, mut state, mut scratch, key) = actor(2);
        let mut backend =
            VehicleFrameBackend::new(&mut arena, &mut heap).with_game_init_stack(STACK);
        backend
            .game_init_effect(
                state.car_live_memory(&mut scratch).unwrap(),
                CarGameInitEffect::CarDependency {
                    entry: 0x405fa0,
                    car: CAR,
                    argument: 0,
                },
            )
            .unwrap();
        let frame = backend.frame_address(key).unwrap();
        let record = backend.read_word(frame + 8).unwrap();
        assert_ne!(record, 0);
        assert_eq!(backend.read_word(record), Ok(CAR));
        let memory = state.car_live_memory(&mut scratch).unwrap();
        assert_eq!(
            memory.read_word_with_backend(&mut backend, CAR + 0x1c),
            Ok(0)
        );
        assert_eq!(
            memory.read_word_with_backend(&mut backend, CAR + 0x68),
            Ok(frame)
        );
        assert_eq!(
            memory.read_byte_with_backend(&mut backend, CAR + 0x64),
            Ok(1)
        );
        assert_eq!(
            memory.read_word_with_backend(&mut backend, CAR + 0x38),
            Ok(1f32.to_bits())
        );
    }
    #[test]
    fn actor_no_shadow_traversal_preserves_the_same_owned_prefix() {
        let (mut arena, mut heap, mut state, mut scratch, key) = actor(1);
        let mut backend =
            VehicleFrameBackend::new(&mut arena, &mut heap).with_game_init_stack(STACK);
        let result = backend.game_init_effect(
            state.car_live_memory(&mut scratch).unwrap(),
            CarGameInitEffect::CarDependency {
                entry: 0x405fa0,
                car: CAR,
                argument: 0,
            },
        );
        assert_eq!(result, Ok(0));
        let frame = backend.frame_address(key).unwrap();
        let record = backend.read_word(frame + 8).unwrap();
        assert_eq!(backend.read_word(record), Ok(CAR));
        let memory = state.car_live_memory(&mut scratch).unwrap();
        assert_eq!(
            memory.read_word_with_backend(&mut backend, CAR + 0x1c),
            Ok(0)
        );
        assert_eq!(
            memory.read_byte_with_backend(&mut backend, CAR + 0x5d),
            Ok(1)
        );
        assert_eq!(
            memory.read_byte_with_backend(&mut backend, CAR + 0x65),
            Ok(1)
        );
    }
    #[test]
    fn brake_light_requests_update_the_same_live_emitter_without_detail_projectors() {
        for braking in [false, true] {
            let mut arena = ModelArena::default();
            let emitter = arena.create_model().unwrap().root();
            arena.set_frame_on(emitter, 0).unwrap();
            let mut heap = VehicleFrameMemory::default();
            let record = 0x700000;
            heap.map_bytes(record, &[0; 32]).unwrap();
            let (mut state, mut scratch) = state();
            let mut backend =
                VehicleFrameBackend::new(&mut arena, &mut heap).with_game_init_stack(STACK);
            let emitter_address = backend.frame_address(emitter).unwrap();
            let vehicle = CAR + 0x70;
            let mut memory = state.car_live_memory(&mut scratch).unwrap();
            for (offset, value) in [
                (0xc68, 0x80),
                (0xc6c, 0x80),
                (0x134, 1f32.to_bits()),
                (0x124, 1f32.to_bits()),
                (0x128, 1f32.to_bits()),
                (0x130, 1f32.to_bits()),
                (0x11c, 0x8808),
                (0x5b8, if braking { 1f32.to_bits() } else { 0 }),
                (0x100, record),
                (0x104, record + 32),
            ] {
                memory
                    .write_word_with_backend(&mut backend, vehicle + offset, value)
                    .unwrap();
            }
            memory
                .write_word_with_backend(&mut backend, record + 4, emitter_address)
                .unwrap();
            memory
                .write_word_with_backend(&mut backend, record + 0x10, 4)
                .unwrap();
            vehicle_game_init_resource_dependency(
                &mut backend,
                memory.reborrow(),
                vehicle,
                STACK,
                VehicleGameInitResourceDependency::DoLights {
                    seconds: 0.1f32.to_bits(),
                },
            )
            .unwrap();
            assert_eq!(
                backend.arena.state(emitter).unwrap().flags & 1 != 0,
                braking
            );
            let requests = memory
                .read_word_with_backend(&mut backend, vehicle + 0x11c)
                .unwrap();
            assert_eq!(requests & 0x100 != 0, braking);
            assert_eq!(
                memory.read_word_with_backend(&mut backend, vehicle + 0x120),
                Ok(requests)
            );
        }
    }
}
