//! GameInit effects on the existing frame arena and canonical car projection.
//! Unbound world/audio/model-resource operations retain their reached failure.
use super::*;

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

impl<B: VehicleComponentsBackend> VehicleFrameBackend<'_, B> {
    fn game_stack(&self, entry: u32) -> Result<u32, VehicleFrameError<B::Error>> {
        self.game_init_stack
            .ok_or(VehicleFrameError::UnavailableGameDependency(entry))
    }
    fn game_virtual(
        &mut self,
        object: u32,
        slot: u32,
        arguments: &[u32],
    ) -> Result<u32, VehicleFrameError<B::Error>> {
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
    fn game_vehicle_dependency(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        vehicle: u32,
        entry: u32,
        arguments: [u32; 2],
    ) -> Result<u32, VehicleFrameError<B::Error>> {
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
        let dependency = match entry {
            0x4cc6e0 => Resource::SetSeatMass {
                index: arguments[0] as i32,
                mass: arguments[1],
            },
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
            _ => return Err(VehicleFrameError::UnavailableGameDependency(entry)),
        };
        vehicle_game_init_resource_dependency(self, memory, vehicle, stack, dependency)
            .map_err(game_failure)?;
        Ok(0)
    }
}

impl<B: VehicleComponentsBackend> CarGameInitBackend for VehicleFrameBackend<'_, B> {
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
            CarGameInitEffect::CarDependency { entry, .. } => {
                Err(VehicleFrameError::UnavailableGameDependency(entry))
            }
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
            CarGameInitEffect::FlagsCallback { function, .. } => {
                Err(VehicleFrameError::UnavailableGameDependency(function))
            }
            CarGameInitEffect::DynamicCollision { entry, .. } => {
                Err(VehicleFrameError::UnavailableGameDependency(entry))
            }
            CarGameInitEffect::InsertVehicleCollision { .. } => Err(
                VehicleFrameError::UnavailableFrameMethod("GameInit collision record insertion"),
            ),
            CarGameInitEffect::InitializeSkidModels { .. } => Err(
                VehicleFrameError::UnavailableFrameMethod("GameInit skid models"),
            ),
            CarGameInitEffect::AddSectorLight { .. } => Err(
                VehicleFrameError::UnavailableFrameMethod("GameInit sector light"),
            ),
            CarGameInitEffect::AddUsingObject { .. } => Err(
                VehicleFrameError::UnavailableFrameMethod("GameInit using-object call context"),
            ),
            CarGameInitEffect::CreateShadowModel { .. } => Err(
                VehicleFrameError::UnavailableFrameMethod("GameInit shadow model factory"),
            ),
            CarGameInitEffect::OpenShadowModel { .. } => Err(
                VehicleFrameError::UnavailableFrameMethod("GameInit shadow model open"),
            ),
            CarGameInitEffect::FindShadowBox { .. } => Err(
                VehicleFrameError::UnavailableFrameMethod("GameInit shadow box lookup"),
            ),
            CarGameInitEffect::SetFrameSector { .. } => Err(
                VehicleFrameError::UnavailableFrameMethod("GameInit scene sector assignment"),
            ),
        }
    }
}
impl<B: VehicleComponentsBackend> VehicleFrameBackend<'_, B> {
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
impl<B: VehicleComponentsBackend> CarGameInitSupportBackend for VehicleFrameBackend<'_, B> {
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
            CarGameInitSupportEffect::VehicleSoundDependency { entry, .. } => {
                Err(VehicleFrameError::UnavailableGameDependency(entry))
            }
            _ => Err(VehicleFrameError::UnavailableFrameMethod(
                "GameInit support resource/world effect",
            )),
        }
    }
}
impl<B: VehicleComponentsBackend> CarGameInitResourceBackend for VehicleFrameBackend<'_, B> {
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
            VehicleResourceEffect::CreateSound { .. }
            | VehicleResourceEffect::LoadSound { .. }
            | VehicleResourceEffect::SoundVirtual { .. } => Err(
                VehicleFrameError::UnavailableFrameMethod("GameInit audio resource"),
            ),
            VehicleResourceEffect::TextureOpen { .. } => Err(
                VehicleFrameError::UnavailableFrameMethod("GameInit glass texture open"),
            ),
            VehicleResourceEffect::AddSectorSound { .. }
            | VehicleResourceEffect::DeleteSectorSound { .. } => Err(
                VehicleFrameError::UnavailableFrameMethod("GameInit sector sound"),
            ),
        }
    }
}

struct GameProjection<'a, 'memory, 'arena, B> {
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
    fn unrecovered_actor_leaf_retains_completed_owned_prefix() {
        let (mut arena, mut heap, mut state, mut scratch, key) = actor(1);
        let mut backend =
            VehicleFrameBackend::new(&mut arena, &mut heap).with_game_init_stack(STACK);
        let failure = backend.game_init_effect(
            state.car_live_memory(&mut scratch).unwrap(),
            CarGameInitEffect::CarDependency {
                entry: 0x405fa0,
                car: CAR,
                argument: 0,
            },
        );
        assert_eq!(
            failure,
            Err(VehicleFrameError::UnavailableGameDependency(0x407d10))
        );
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
}
