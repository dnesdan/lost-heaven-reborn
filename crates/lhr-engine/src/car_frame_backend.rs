//! C_car frame and initial C-locale CRT effects over the live model projection.
//! Other CRT locales and their OS/allocation paths are outside this adapter.
use super::*;
use crate::native_transform::SMatrix;

fn matrix_fields<B: VehicleInitBackend>(
    backend: &mut B,
    memory: &VehicleComponentTailMemory<'_>,
    address: u32,
    fields: impl IntoIterator<Item = usize>,
) -> Result<SMatrix, B::Error> {
    let mut elements = [0.; 16];
    // Only requested fields participate in the operation. In particular, native
    // point transforms and GetScale never read the fourth column.
    for index in fields {
        elements[index] = f32::from_bits(
            memory.read_word_with_backend(backend, address.wrapping_add(index as u32 * 4))?,
        );
    }
    Ok(SMatrix { elements })
}

impl<B: VehicleInitBackend> CarInitBackend for VehicleFrameBackend<'_, B> {
    fn enumerate_car(
        &mut self,
        mut memory: VehicleComponentTailMemory<'_>,
        model: u32,
        query: CarInitQuery,
        output: u32,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.enumerate_frames(model, query.pattern(), query.mask(), |backend, frame| {
            // Original 0x4d0ea0 writes every visited pointer and returns zero.
            memory.write_word_with_backend(backend, output, frame)?;
            Ok::<_, VehicleInitUnavailable<Self::Error>>(0)
        })?;
        Ok(())
    }

    fn find_car_frame(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        model: u32,
        pattern: u32,
        mask: u32,
    ) -> Result<u32, Self::Error> {
        let root = self.frame_key(model)?;
        if self.arena.state(root)?.kind == FrameKind::Scene || mask & !0x3ffff != 0 {
            return Err(VehicleFrameError::UnsupportedEnumeration(mask));
        }
        let name = if pattern == 0 {
            None
        } else {
            let mut name = Vec::new();
            let mut address = pattern;
            loop {
                let byte = memory.read_byte_with_backend(self, address)?;
                if byte == 0 {
                    break;
                }
                name.push(byte);
                address = address.wrapping_add(1);
            }
            if !name.is_ascii() {
                return Err(VehicleFrameError::UnsupportedName(root));
            }
            Some(name)
        };
        // The original Model constructor installs vtable1009c160, whose +0x38
        // slot is FindChildFrame1001c330. Its callback1001c310 stores the first
        // result and returns CANCEL. It enumerates children by original name;
        // the self-first full-name GetChildFrame1001c360 is nonvirtual.
        let mut found = 0;
        self.enumerate_frames_with_optional_pattern(
            model,
            name.as_deref(),
            mask,
            |_, frame, _| {
                found = frame;
                Ok::<_, Self::Error>(2)
            },
        )?;
        Ok(found)
    }

    fn frame_local_matrix(
        &mut self,
        _: VehicleComponentTailMemory<'_>,
        frame: u32,
    ) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        self.arena
            .graph
            .update_matrix(key)
            .map_err(ModelError::from)?;
        Ok(())
    }

    fn transform_point(
        &mut self,
        mut memory: VehicleComponentTailMemory<'_>,
        point: u32,
        source_matrix: u32,
        destination: u32,
    ) -> Result<u32, Self::Error> {
        let mut vector = [0.; 3];
        for (index, value) in vector.iter_mut().enumerate() {
            *value = f32::from_bits(
                memory.read_word_with_backend(self, point.wrapping_add(index as u32 * 4))?,
            );
        }
        let transformed = matrix_fields(
            self,
            &memory,
            source_matrix,
            [0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14],
        )?
        .try_transform_point(vector)
        .map_err(|_| VehicleFrameError::NumericDomain("C_car point transform"))?;
        for (index, value) in transformed.into_iter().enumerate() {
            memory.write_word_with_backend(
                self,
                destination.wrapping_add(index as u32 * 4),
                value.to_bits(),
            )?;
        }
        Ok(destination)
    }

    fn detach_car_frame(
        &mut self,
        _: VehicleComponentTailMemory<'_>,
        frame: u32,
    ) -> Result<(), Self::Error> {
        self.bodycol_link_frame(frame, 0, 0)
    }

    fn remove_car_frame(
        &mut self,
        _: VehicleComponentTailMemory<'_>,
        model: u32,
        frame: u32,
    ) -> Result<(), Self::Error> {
        self.bodycol_remove_model_child(model, frame)
    }

    fn update_car_world_matrix(
        &mut self,
        _: VehicleComponentTailMemory<'_>,
        frame: u32,
    ) -> Result<(), Self::Error> {
        self.bodycol_update_world(frame)
    }

    fn car_matrix_scale(
        &mut self,
        mut memory: VehicleComponentTailMemory<'_>,
        source_matrix: u32,
        destination: u32,
    ) -> Result<(), Self::Error> {
        // Native computes Z/Y/X into callee temporaries before storing X/Y/Z.
        let scale = matrix_fields(self, &memory, source_matrix, [10, 9, 8, 6, 5, 4, 2, 1, 0])?
            .try_get_scale()
            .map_err(|_| VehicleFrameError::NumericDomain("C_car matrix scale"))?;
        for (row, value) in scale.into_iter().enumerate() {
            memory.write_word_with_backend(
                self,
                destination.wrapping_add(row as u32 * 4),
                value.to_bits(),
            )?;
        }
        Ok(())
    }

    fn car_atoi(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        mut source: u32,
    ) -> Result<u32, Self::Error> {
        // Admit the PE's initial C locale and its selected C ctype table only.
        // Other codepages/classification tables need the original locale path.
        let locale = memory.read_word_with_backend(self, 0x6d87c4)?;
        if locale != 0 {
            return Err(VehicleFrameError::UnsupportedCarLocale(locale));
        }
        let character_width = memory.read_word_with_backend(self, 0x63a4cc)?;
        if character_width != 1 {
            return Err(VehicleFrameError::UnsupportedCarLocale(character_width));
        }
        let mut byte = memory.read_byte_with_backend(self, source)?;
        while matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
            source = source.wrapping_add(1);
            byte = memory.read_byte_with_backend(self, source)?;
        }
        // The native ctype whitespace loop and sign load read separately.
        byte = memory.read_byte_with_backend(self, source)?;
        let negative = byte == b'-';
        if matches!(byte, b'-' | b'+') {
            source = source.wrapping_add(1);
            byte = memory.read_byte_with_backend(self, source)?;
        }
        let mut value = 0u32;
        while byte.is_ascii_digit() {
            value = value.wrapping_mul(10).wrapping_add(u32::from(byte - b'0'));
            source = source.wrapping_add(1);
            byte = memory.read_byte_with_backend(self, source)?;
        }
        Ok(if negative {
            0u32.wrapping_sub(value)
        } else {
            value
        })
    }

    fn car_uppercase(
        &mut self,
        mut memory: VehicleComponentTailMemory<'_>,
        mut source: u32,
    ) -> Result<(), Self::Error> {
        let locale = memory.read_word_with_backend(self, 0x6d87c4)?;
        if locale != 0 {
            return Err(VehicleFrameError::UnsupportedCarLocale(locale));
        }
        if memory.read_byte_with_backend(self, source)? == 0 {
            return Ok(());
        }
        loop {
            let byte = memory.read_byte_with_backend(self, source)?;
            if byte.is_ascii_lowercase() {
                memory.write_byte_with_backend(self, source, byte.to_ascii_uppercase())?;
            }
            source = source.wrapping_add(1);
            if memory.read_byte_with_backend(self, source)? == 0 {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_transform::I3dFrameState;

    const CAR: u32 = 0x0010_0000;
    const STACK: u32 = 0x8000;

    fn state() -> (VehicleInitState, VehicleComponentTailScratch) {
        (
            VehicleInitState::from_car_bytes(
                CAR,
                vec![0xa5; 0x2370],
                CarInitScratch {
                    address: STACK,
                    bytes: [0x33; CAR_INIT_STACK_BYTES],
                },
                CarInitRegisters {
                    ebx: 1,
                    ebp: 2,
                    esi: 3,
                    edi: 4,
                },
            )
            .unwrap(),
            VehicleComponentTailScratch {
                address: 0x9000,
                bytes: [0x44; VEHICLE_INIT_STACK_BYTES],
            },
        )
    }

    fn dummy(
        arena: &mut ModelArena,
        model: ModelKey,
        parent: FrameKey,
        name: &[u8],
        position: [f32; 3],
    ) -> FrameKey {
        let mut state = I3dFrameState::dummy();
        state.local.elements[12..15].copy_from_slice(&position);
        state.flags &= !0x20;
        let key = arena
            .insert(
                state,
                Some(0),
                name.to_vec(),
                Vec::new(),
                Some([[-1.; 3], [1.; 3]]),
            )
            .unwrap();
        arena.add_frame(model, key).unwrap();
        arena.release(key).unwrap();
        arena.link_frame(key, Some(parent), 0).unwrap();
        key
    }

    #[test]
    fn car_enumeration_transform_and_removal_use_one_live_arena() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let older = dummy(&mut arena, model, model.root(), b"AI00", [1., 2., 3.]);
        let newer = dummy(&mut arena, model, model.root(), b"AI00", [4., 5., 6.]);
        let parent = dummy(&mut arena, model, model.root(), b"BODY", [10., 20., 30.]);
        let child = dummy(&mut arena, model, parent, b"SM0a", [2., 3., 4.]);
        arena.set_frame_name(model.root(), Some(b"FUEL")).unwrap();
        arena.propagate_model_name(model).unwrap();
        // Removal must release model ownership while retaining this real lease.
        arena.retain(child).unwrap();
        let mut heap = VehicleFrameMemory::default();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut heap);
        let root = backend.frame_address(model.root()).unwrap();
        let older_address = backend.frame_address(older).unwrap();
        let newer_address = backend.frame_address(newer).unwrap();
        let parent_address = backend.frame_address(parent).unwrap();
        let child_address = backend.frame_address(child).unwrap();
        let (mut state, mut scratch) = state();
        backend
            .enumerate_car(
                state.car_live_memory(&mut scratch).unwrap(),
                root,
                CarInitQuery::Ai00,
                CAR + 0x40,
            )
            .unwrap();
        let view = state.car_live_memory(&mut scratch).unwrap();
        assert_eq!(view.read_word(CAR + 0x40), Some(older_address));
        assert_ne!(older_address, newer_address);
        assert_eq!(backend.arena().frame(older).unwrap().references, 1);
        assert_eq!(
            backend
                .find_car_frame(
                    state.car_live_memory(&mut scratch).unwrap(),
                    root,
                    0x6322f8,
                    0x80
                )
                .unwrap(),
            0
        );
        let mut view = state.car_live_memory(&mut scratch).unwrap();
        for (index, byte) in b"FUEL.SM0*\0".iter().copied().enumerate() {
            view.write_byte(STACK + 0xe4 + index as u32, byte).unwrap();
        }
        assert_eq!(
            backend
                .find_car_frame(
                    state.car_live_memory(&mut scratch).unwrap(),
                    root,
                    STACK + 0xe4,
                    0x10020
                )
                .unwrap(),
            0
        );
        let mut view = state.car_live_memory(&mut scratch).unwrap();
        for (index, byte) in b"SM0*\0".iter().copied().enumerate() {
            view.write_byte(STACK + 0xe4 + index as u32, byte).unwrap();
        }
        assert_eq!(
            backend
                .find_car_frame(
                    state.car_live_memory(&mut scratch).unwrap(),
                    root,
                    STACK + 0xe4,
                    0x10020
                )
                .unwrap(),
            child_address
        );
        // Actual vtable+38 FindChildFrame compares original names and cancels
        // on the first match; the AI collector above retained the last match.
        assert_eq!(
            backend
                .find_car_frame(
                    state.car_live_memory(&mut scratch).unwrap(),
                    root,
                    0x632340,
                    0x20
                )
                .unwrap(),
            newer_address
        );
        assert_eq!(
            backend
                .find_car_frame(state.car_live_memory(&mut scratch).unwrap(), root, 0, 0x20)
                .unwrap(),
            parent_address
        );
        backend
            .frame_local_matrix(state.car_live_memory(&mut scratch).unwrap(), parent_address)
            .unwrap();
        backend
            .transform_point(
                state.car_live_memory(&mut scratch).unwrap(),
                child_address + 0x80,
                parent_address + 0x50,
                CAR + 0x2080,
            )
            .unwrap();
        let view = state.car_live_memory(&mut scratch).unwrap();
        assert_eq!(
            (0..3)
                .map(|i| f32::from_bits(view.read_word(CAR + 0x2080 + i * 4).unwrap()))
                .collect::<Vec<_>>(),
            [12., 23., 34.]
        );
        backend
            .update_car_world_matrix(state.car_live_memory(&mut scratch).unwrap(), child_address)
            .unwrap();
        assert_eq!(
            &backend.arena().state(child).unwrap().world.elements[12..15],
            &[12., 23., 34.]
        );
        backend
            .detach_car_frame(state.car_live_memory(&mut scratch).unwrap(), child_address)
            .unwrap();
        backend
            .remove_car_frame(
                state.car_live_memory(&mut scratch).unwrap(),
                root,
                child_address,
            )
            .unwrap();
        assert_eq!(backend.arena().links(child).unwrap().parent, None);
        assert_eq!(backend.arena().frame(child).unwrap().owner, None);
        assert_eq!(backend.arena().frame(child).unwrap().references, 1);
        backend.arena_mut().release(child).unwrap();
        assert!(backend.arena().frame(child).is_err());
    }

    #[test]
    fn scale_commits_after_all_rows_and_never_reads_unused_fields() {
        let mut arena = ModelArena::default();
        let mut heap = VehicleFrameMemory::default();
        let source = heap.allocate(64).unwrap();
        // Fourth columns and translation remain uninitialized deliberately.
        for (index, value) in [
            (0, f32::MAX),
            (1, 0.),
            (2, 0.),
            (4, 0.),
            (5, -2.),
            (6, 0.),
            (8, 0.),
            (9, 0.),
            (10, 3.),
        ] {
            heap.write_word(source + index * 4, value.to_bits())
                .unwrap();
        }
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut heap);
        let (mut state, mut scratch) = state();
        assert_eq!(
            backend.car_matrix_scale(
                state.car_live_memory(&mut scratch).unwrap(),
                source,
                CAR + 0x2080
            ),
            Err(VehicleFrameError::NumericDomain("C_car matrix scale"))
        );
        let view = state.car_live_memory(&mut scratch).unwrap();
        assert_eq!(view.read_word(CAR + 0x2080), Some(0xa5a5a5a5));
        assert_eq!(view.read_word(CAR + 0x2084), Some(0xa5a5a5a5));
        assert_eq!(view.read_word(CAR + 0x2088), Some(0xa5a5a5a5));
        backend.write_word(source, 4f32.to_bits()).unwrap();
        backend
            .car_matrix_scale(
                state.car_live_memory(&mut scratch).unwrap(),
                source,
                CAR + 0x2080,
            )
            .unwrap();
        let view = state.car_live_memory(&mut scratch).unwrap();
        assert_eq!(view.read_word(CAR + 0x2080), Some(4f32.to_bits()));
        assert_eq!(view.read_word(CAR + 0x2084), Some(2f32.to_bits()));
        assert_eq!(view.read_word(CAR + 0x2088), Some(3f32.to_bits()));
    }

    #[test]
    fn crt_operations_observe_canonical_prefix_vehicle_and_car_stack_bytes() {
        let mut arena = ModelArena::default();
        let mut heap = VehicleFrameMemory::default();
        heap.map_bytes(0x6d87c4, &0u32.to_le_bytes()).unwrap();
        heap.map_bytes(0x63a4cc, &1u32.to_le_bytes()).unwrap();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut heap);
        let (mut state, mut scratch) = state();
        let mut view = state.car_live_memory(&mut scratch).unwrap();
        for (index, byte) in b"abcxyzab\xe1cd\0".iter().copied().enumerate() {
            view.write_byte(CAR + 0x68 + index as u32, byte).unwrap();
        }
        backend
            .car_uppercase(state.car_live_memory(&mut scratch).unwrap(), CAR + 0x68)
            .unwrap();
        assert_eq!(&state.car_bytes().unwrap()[0x68..0x74], b"ABCXYZAB\xe1CD\0");
        let mut view = state.car_live_memory(&mut scratch).unwrap();
        for (index, byte) in b"\t -4294967297tail\0".iter().copied().enumerate() {
            view.write_byte(STACK + 0xe4 + index as u32, byte).unwrap();
        }
        assert_eq!(
            backend
                .car_atoi(state.car_live_memory(&mut scratch).unwrap(), STACK + 0xe4)
                .unwrap(),
            u32::MAX
        );
        assert_eq!(
            backend.find_car_frame(
                state.car_live_memory(&mut scratch).unwrap(),
                0,
                0x6322f8,
                0x20
            ),
            Err(VehicleFrameError::Address(0))
        );
        state
            .car_live_memory(&mut scratch)
            .unwrap()
            .write_byte(CAR + 0x68, b'q')
            .unwrap();
        backend.write_word(0x6d87c4, 0x409).unwrap();
        assert_eq!(
            backend.car_uppercase(state.car_live_memory(&mut scratch).unwrap(), CAR + 0x68),
            Err(VehicleFrameError::UnsupportedCarLocale(0x409))
        );
        assert_eq!(
            state
                .car_live_memory(&mut scratch)
                .unwrap()
                .read_byte(CAR + 0x68),
            Some(b'q')
        );
    }

    #[test]
    fn atoi_rejects_noninitial_locale_before_reading_source() {
        let mut arena = ModelArena::default();
        let mut heap = VehicleFrameMemory::default();
        heap.map_bytes(0x6d87c4, &0x409u32.to_le_bytes()).unwrap();
        heap.map_bytes(0x63a4cc, &1u32.to_le_bytes()).unwrap();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut heap);
        let (mut state, mut scratch) = state();
        // The source is deliberately unmapped: rejection must precede parsing.
        let source = 0x00ee_0000;
        assert_eq!(
            backend.car_atoi(state.car_live_memory(&mut scratch).unwrap(), source),
            Err(VehicleFrameError::UnsupportedCarLocale(0x409))
        );
        backend.write_word(0x6d87c4, 0).unwrap();
        backend.write_word(0x63a4cc, 2).unwrap();
        assert_eq!(
            backend.car_atoi(state.car_live_memory(&mut scratch).unwrap(), source),
            Err(VehicleFrameError::UnsupportedCarLocale(2))
        );
        backend.write_word(0x63a4cc, 1).unwrap();
        assert_eq!(
            backend.car_atoi(state.car_live_memory(&mut scratch).unwrap(), source),
            Err(VehicleFrameError::Host(VehicleMemoryError::Address(source)))
        );
    }
    #[test]
    fn car_model_category_literals_are_readable_and_immutable_without_host_mapping() {
        let mut arena = ModelArena::default();
        let mut heap = VehicleFrameMemory::default();
        assert_eq!(
            heap.map_bytes(FILENAME_BASE + 4, b"host filename\0"),
            Err(VehicleMemoryError::Overlap(FILENAME_BASE + 4))
        );
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut heap);
        for (base, name) in [
            (0x632320, b"POLCAD\0".as_slice()),
            (0x632318, b"POLI\0"),
            (0x63230c, b"POLIMFOR\0"),
            (0x632300, b"POLIMTUD\0"),
        ] {
            for (offset, byte) in name.iter().copied().enumerate() {
                assert_eq!(backend.read_byte(base + offset as u32).unwrap(), byte);
            }
            assert_eq!(
                backend.read_word(base).unwrap(),
                u32::from_le_bytes(name[..4].try_into().unwrap())
            );
            assert_eq!(
                backend.write_byte(base, b'x'),
                Err(VehicleFrameError::Address(base))
            );
            assert_eq!(
                backend.write_word(base, 0),
                Err(VehicleFrameError::Address(base))
            );
            assert_eq!(backend.free(base), Err(VehicleFrameError::Address(base)));
        }
    }
}
