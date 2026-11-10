use super::*;

#[derive(Default)]
struct RecordingBackend {
    allocations: Vec<usize>,
    conversions: Vec<(usize, Vec<Option<i32>>)>,
    fail_at: Option<usize>,
    fail_allocation: bool,
    null_allocation: bool,
}

impl RemainingParameterBackend for RecordingBackend {
    type Error = &'static str;

    fn allocate_remaining(
        &mut self,
        count: usize,
    ) -> Result<Option<Box<RemainingParameter>>, Self::Error> {
        self.allocations.push(count);
        if self.fail_allocation {
            return Err("allocation failure");
        }
        if self.null_allocation {
            return Ok(None);
        }
        Ok(Some(Box::new(RemainingParameter {
            values: vec![None; count],
        })))
    }

    fn convert_remaining(
        &mut self,
        index: usize,
        token: &[u8],
        published: &RemainingParameter,
    ) -> Result<i32, Self::Error> {
        self.conversions.push((index, published.values.clone()));
        if self.fail_at == Some(index) {
            return Err("conversion failure");
        }
        ascii_integer_prefix(token).map_err(|_| "integer conversion")
    }
}

#[test]
fn optional_second_defaults_use_two_owned_words() {
    for opcode in [129, 130, 234] {
        for (source, second) in [
            (&b"7\0"[..], 500),
            (&b"7,0\0"[..], 0),
            (&b"7,-9\0"[..], -9),
            (&b"7,123\0"[..], 123),
            (&b"7,500\0"[..], 500),
        ] {
            let count = if source.contains(&b',') { 2 } else { 1 };
            let mut backend = RecordingBackend::default();
            let mut slot = RemainingParameterSlot::default();
            let mut source = source.to_vec();
            assert_eq!(slot.decode(opcode, &mut source, &mut backend), Ok(true));
            assert_eq!(backend.allocations, vec![2]);
            assert_eq!(slot.parameter().unwrap().values(), &[Some(7), Some(second)]);
            assert_eq!(backend.conversions[0], (0, vec![None, None]));
            assert_eq!(backend.conversions.len(), count);
            if count == 2 {
                assert_eq!(backend.conversions[1], (1, vec![Some(7), None]));
            }
        }
    }
}

#[test]
fn optional_second_rejects_wrong_arity_before_allocation() {
    for opcode in [129, 130, 234] {
        for source in [&b"\0"[..], &b"1,2,3\0"[..], &b"1,2,3,4\0"[..]] {
            let mut backend = RecordingBackend::default();
            let mut slot = RemainingParameterSlot::default();
            let mut source = source.to_vec();
            assert_eq!(slot.decode(opcode, &mut source, &mut backend), Ok(false));
            assert!(slot.parameter().is_none());
            assert!(backend.allocations.is_empty());
            assert!(backend.conversions.is_empty());
        }
    }
}

#[test]
fn optional_second_conversion_failure_retains_only_reached_writes() {
    for opcode in [129, 130, 234] {
        for (source, index) in [(&b"7\0"[..], 0), (&b"7,9\0"[..], 1)] {
            let mut backend = RecordingBackend {
                fail_at: Some(index),
                ..Default::default()
            };
            let mut slot = RemainingParameterSlot::default();
            let mut source = source.to_vec();
            assert_eq!(
                slot.decode(opcode, &mut source, &mut backend),
                Err(RemainingDecodeError::Conversion {
                    index,
                    error: "conversion failure",
                })
            );
            let expected = if index == 0 { None } else { Some(7) };
            assert_eq!(slot.parameter().unwrap().values(), &[expected, None]);
            let calls = backend.conversions.len();
            assert_eq!(
                slot.decode(opcode, &mut source, &mut backend),
                Err(RemainingDecodeError::AlreadyEntered)
            );
            assert_eq!(backend.conversions.len(), calls);
            assert_eq!(backend.allocations, vec![2]);
        }
    }
}

#[test]
fn optional_second_allocation_failure_never_converts() {
    for opcode in [129, 130, 234] {
        for null_allocation in [false, true] {
            let mut backend = RecordingBackend {
                fail_allocation: !null_allocation,
                null_allocation,
                ..Default::default()
            };
            let mut slot = RemainingParameterSlot::default();
            let mut source = b"7\0".to_vec();
            let expected = if null_allocation {
                RemainingDecodeError::NullAllocation
            } else {
                RemainingDecodeError::Allocation("allocation failure")
            };
            assert_eq!(slot.decode(opcode, &mut source, &mut backend), Err(expected));
            assert!(slot.parameter().is_none());
            assert_eq!(backend.allocations, vec![2]);
            assert!(backend.conversions.is_empty());
        }
    }
}
