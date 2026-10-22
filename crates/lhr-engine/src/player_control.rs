//! Bounded player control and model-animation dependencies. Names describe behavior.
//!
//! Game SHA-256 `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`:
//! control lock RVA `0x195f00`, follow-up `0x17d310`, direction reset `0x17a560`,
//! embedded reset `0xe7a0`, model-animation caller `0xe8e0`, animation index `0x8760`.
//! LS3DF SHA-256 `e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6`:
//! virtual slot `0x68` targets RVA `0x33890` (upstream PS2 research name
//! `StopAnimation`; no original PC symbol).
//! This vtable-anchored, provider-decoded LS dependency is absent from the native
//! function-manager inventory and receives no canonical-entry credit.
//! Caller-owned raw storage already represents a constructed actor. These views do
//! not construct native actors or create a controllable world. Required backend
//! errors preserve writes up to the first unavailable native call.
use crate::interpolator::InterpolatorSet;

pub const PLAYER_CONTROL_STORAGE_BYTES: usize = 0xb00;

/// Owned projection of Game's existing sorted animation-code/index table.
/// Entries must come from the caller. The original game's table bootstrap and
/// resource catalog remain unresolved; no original executable or extracted
/// catalog is a runtime dependency. This does not load animation resources or
/// construct a human actor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationIndexTable {
    entries: Option<Vec<(u32, i32)>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimationIndexTableError {
    EmptyNonnullTable,
    IndexArithmeticOverflow,
    Unsorted { index: usize },
}

impl AnimationIndexTable {
    /// `None` represents the original null table pointer. A nonnull empty table
    /// is unsupported: the original lookup reads its first entry anyway.
    /// Duplicate codes are allowed and retain their original record order.
    pub fn from_existing_entries(
        entries: Option<Vec<(u32, i32)>>,
    ) -> Result<Self, AnimationIndexTableError> {
        if let Some(entries) = &entries {
            if entries.is_empty() {
                return Err(AnimationIndexTableError::EmptyNonnullTable);
            }
            // Host admission: eight-byte records must fit a native 32-bit extent.
            if entries.len() > u32::MAX as usize / 8 {
                return Err(AnimationIndexTableError::IndexArithmeticOverflow);
            }
            // The original adds its signed low/high bounds before dividing.
            if entries.len() - 1 > i32::MAX as usize / 2 {
                return Err(AnimationIndexTableError::IndexArithmeticOverflow);
            }
            if let Some(index) = entries.windows(2).position(|pair| pair[0].0 > pair[1].0) {
                return Err(AnimationIndexTableError::Unsorted { index: index + 1 });
            }
        }
        Ok(Self { entries })
    }

    /// Game RVA `0x8760`; original PS2 name `GetAnimIndex(unsigned int)` has
    /// matching table-search semantics. PC compares unsigned animation codes
    /// and returns the selected signed index, or `-1` when absent. Midpoint
    /// selection matters for duplicate codes; do not use an arbitrary-match
    /// standard-library binary search here.
    pub fn get_anim_index(&self, code: u32) -> i32 {
        let Some(entries) = &self.entries else {
            return -1;
        };
        let mut low = 0;
        let mut high = entries.len() - 1;
        loop {
            let middle = (low + high) / 2;
            let (candidate, index) = entries[middle];
            if candidate == code {
                return index;
            }
            if low >= high {
                return -1;
            }
            if candidate > code {
                // Original MOV EDI,EAX-1, not the usual upper-bound update.
                // middle==0 makes the native high bound -1; the next midpoint
                // truncates (-1+0)/2 to zero and returns a miss on that entry.
                if middle == 0 {
                    return -1;
                }
                high = middle - 1;
            } else {
                low = middle + 1;
            }
        }
    }
}

/// Borrows existing player storage; this is not a native-constructor replacement.
pub struct RawPlayerControlState<'a> {
    storage: &'a mut [u8; PLAYER_CONTROL_STORAGE_BYTES],
}
impl<'a> RawPlayerControlState<'a> {
    pub fn from_existing_storage(storage: &'a mut [u8; PLAYER_CONTROL_STORAGE_BYTES]) -> Self {
        Self { storage }
    }
    pub fn as_bytes(&self) -> &[u8; PLAYER_CONTROL_STORAGE_BYTES] {
        self.storage
    }
    pub fn set_lock(
        &mut self,
        input_mask: &mut u32,
        argument: u8,
    ) -> Result<(), RequiredControlBackend> {
        set_control_lock(self.storage, input_mask, argument)
    }
    pub fn set_lock_from_script(
        &mut self,
        input_mask: &mut u32,
        argument: u32,
    ) -> Result<(), RequiredControlBackend> {
        set_control_lock_from_script(self.storage, input_mask, argument)
    }
    pub fn reset_selected_state(&mut self) {
        reset_selected_control_state(self.storage);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlReceiver {
    PlayerOffset(usize),
    Mission,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequiredControlBackend {
    Direct {
        entry_rva: u32,
        receiver: ControlReceiver,
        arguments: Vec<u32>,
    },
    Virtual {
        object: u32,
        slot: u32,
        arguments: Vec<u32>,
    },
}
fn word(player: &[u8; 0xb00], offset: usize) -> u32 {
    u32::from_le_bytes(player[offset..offset + 4].try_into().unwrap())
}
fn store(player: &mut [u8; 0xb00], offset: usize, value: u32) {
    player[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
/// Game RVA `0x195f00`: exact byte parameter including noncanonical nonzero byte values.
pub fn set_control_lock(
    player: &mut [u8; 0xb00],
    input_mask: &mut u32,
    argument: u8,
) -> Result<(), RequiredControlBackend> {
    player[0xada] = argument;
    if argument == 0 {
        return Ok(());
    }
    *input_mask &= !0x2000;
    let state = word(player, 0x40c) as i32;
    if state <= 0 || (state > 6 && state != 10) {
        return Ok(());
    }
    if state == 10 {
        *input_mask &= !0x1000;
        let object_564 = word(player, 0x564);
        store(player, 0x478, 0);
        store(player, 0x44c, u32::MAX);
        if object_564 != 0 {
            return Err(RequiredControlBackend::Virtual {
                object: object_564,
                slot: 0x24,
                arguments: vec![object_564, 1],
            });
        }
        let object_568 = word(player, 0x568);
        if object_568 != 0 {
            return Err(RequiredControlBackend::Virtual {
                object: object_568,
                slot: 0x24,
                arguments: vec![object_568, 1],
            });
        }
        return Err(RequiredControlBackend::Direct {
            entry_rva: 0x1acc00,
            receiver: ControlReceiver::Mission,
            arguments: vec![word(player, 0x99c), 0, 0x3a83126f],
        });
    }
    match word(player, 0x410) {
        1 => {
            return Err(RequiredControlBackend::Direct {
                entry_rva: 0x1716d0,
                receiver: ControlReceiver::PlayerOffset(0),
                arguments: vec![1],
            });
        }
        2 | 5 => {
            return Err(RequiredControlBackend::Direct {
                entry_rva: 0x1716d0,
                receiver: ControlReceiver::PlayerOffset(0),
                arguments: vec![0],
            });
        }
        3 => {
            return Err(RequiredControlBackend::Direct {
                entry_rva: 0x171b90,
                receiver: ControlReceiver::PlayerOffset(0),
                arguments: vec![1],
            });
        }
        4 => {
            return Err(RequiredControlBackend::Direct {
                entry_rva: 0x171b90,
                receiver: ControlReceiver::PlayerOffset(0),
                arguments: vec![0],
            });
        }
        _ => {}
    }
    store(player, 0x410, 0);
    store(player, 0x9c, 0);
    store(player, 0x40c, 1);
    Err(RequiredControlBackend::Direct {
        entry_rva: 0xe8e0,
        receiver: ControlReceiver::PlayerOffset(0xbc),
        arguments: vec![],
    })
}
/// Game RVA `0x17d310`: conditional reset called after the control setter by the original script arm.
pub fn reset_selected_control_state(player: &mut [u8; 0xb00]) {
    if player[0x5d] != 0 && matches!(word(player, 0x40c), 1..=3) {
        store(player, 0x40c, 1);
        store(player, 0x42c, 0);
        store(player, 0x420, 0);
        store(player, 0x424, 0);
        store(player, 0x428, 0);
        store(player, 0x74, 1);
    }
}
/// Bounded original opcode `0x5f` arm effects, including script DWORD-to-boolean normalization.
pub fn set_control_lock_from_script(
    player: &mut [u8; 0xb00],
    input_mask: &mut u32,
    raw_argument: u32,
) -> Result<(), RequiredControlBackend> {
    set_control_lock(player, input_mask, u8::from(raw_argument != 0))?;
    reset_selected_control_state(player);
    Ok(())
}
/// Game RVA `0x17a560`. Descriptive name; exact raw bit stores, with no frame or numeric helper.
pub fn reset_direction_vectors(player: &mut [u8; 0xb00]) {
    store(player, 0x2cc, 0);
    store(player, 0x2d8, 0);
    store(player, 0x2d0, 0);
    player[0x2b0] = 1;
    store(player, 0x2d4, 0x3f800000);
    store(player, 0x2c0, 0);
    store(player, 0x2c4, 0);
    store(player, 0x2c8, 0x3f800000);
    store(player, 0x2b4, 0);
    store(player, 0x2b8, 0);
    store(player, 0x2bc, 0x3f800000);
    store(player, 0xa88, (word(player, 0xa88) & !2) | 5);
}
/// Game RVA `0xe7a0` on the embedded player `+0xbc` object. Descriptive ownership projection.
pub fn reset_embedded_animation_state(player: &mut [u8; 0xb00]) {
    const O: usize = 0xbc;
    for offset in [0xf, 0xc, 0x10, 0xd, 0x11, 0xe, 0x12] {
        player[O + offset] = 0;
    }
    store(player, O + 0x14, 0);
    store(player, O + 0x18, 0);
    store(player, O + 0x1c, 0);
    store(player, O + 0x2c, u32::MAX);
    store(player, O + 0x80, u32::MAX);
    store(player, O + 0x20, 0xbf800000);
    player[O + 0xf4] = 0;
    player[O + 0x110] = 0;
    player[O + 0x10e..O + 0x110].copy_from_slice(&u16::MAX.to_le_bytes());
}

/// Nonowning model identity. `None` represents native null; identity zero is valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModelAnimationKey(pub u32);

/// Projection of the live model field at the original animation owner `+0x8`.
#[derive(Debug, PartialEq, Eq)]
pub struct ModelAnimationOwner {
    pub model: Option<ModelAnimationKey>,
}
impl ModelAnimationOwner {
    pub fn from_existing_model(model: Option<ModelAnimationKey>) -> Self {
        Self { model }
    }
    /// Game RVA `0xe8e0`: initial null guard, then eight calls using the live model field.
    /// Prior slot effects remain visible when a backend call fails. A callback
    /// clearing the owner would fault on the next native call; that unsupported
    /// storage state returns an explicit host error before the next call.
    pub fn clear_animations<B: ModelAnimationStopBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ClearAnimationsError<B::Error>> {
        if self.model.is_none() {
            return Ok(());
        }
        for slot in 0..8 {
            let model = self
                .model
                .ok_or(ClearAnimationsError::MissingLiveModel { slot })?;
            backend
                .stop_animation(self, model, slot)
                .map_err(|source| ClearAnimationsError::Backend {
                    model,
                    slot,
                    source,
                })?;
        }
        Ok(())
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum ClearAnimationsError<E> {
    MissingLiveModel {
        slot: u32,
    },
    Backend {
        model: ModelAnimationKey,
        slot: u32,
        source: E,
    },
}
/// Resolve the actual model and its interpolator ownership. Missing nonnull
/// storage must be an error. Callback borrows end before the next live read.
pub trait ModelAnimationStopBackend {
    type Error;
    fn stop_animation(
        &mut self,
        owner: &mut ModelAnimationOwner,
        model: ModelAnimationKey,
        slot: u32,
    ) -> Result<(), Self::Error>;
}
/// LS3DF model virtual slot `0x68` at RVA `0x33890`. Only the selected enabled byte changes.
/// `None` means an actual null model interpolator pointer; lookup failure for a
/// nonnull interpolator must be reported by the caller instead of using `None`.
/// Invalid unsigned indices return before inspecting the interpolator.
pub fn stop_animation(interpolator: Option<&mut InterpolatorSet>, index: u32) {
    if index >= 8 {
        return;
    }
    if let Some(set) = interpolator {
        set.slots[index as usize].enabled = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_index_preserves_unsigned_order_and_original_duplicate_choice() {
        let table = AnimationIndexTable::from_existing_entries(Some(vec![
            (0, 9),
            (7, 10),
            (7, 11),
            (7, 12),
            (0x8000_0000, -2),
            (u32::MAX, i32::MIN),
        ]))
        .unwrap();
        assert_eq!(table.get_anim_index(7), 11);
        assert_eq!(table.get_anim_index(0x8000_0000), -2);
        assert_eq!(table.get_anim_index(u32::MAX), i32::MIN);
        assert_eq!(table.get_anim_index(0x7fff_ffff), -1);
        assert_eq!(table.get_anim_index(0), 9);
    }

    #[test]
    fn animation_index_distinguishes_null_from_invalid_nonnull_empty_table() {
        let null = AnimationIndexTable::from_existing_entries(None).unwrap();
        assert_eq!(null.get_anim_index(u32::MAX), -1);
        assert_eq!(
            AnimationIndexTable::from_existing_entries(Some(vec![])),
            Err(AnimationIndexTableError::EmptyNonnullTable)
        );
        assert_eq!(
            AnimationIndexTable::from_existing_entries(Some(vec![(1, 0), (0, 1)])),
            Err(AnimationIndexTableError::Unsorted { index: 1 })
        );
        let single = AnimationIndexTable::from_existing_entries(Some(vec![(1, 42)])).unwrap();
        assert_eq!(single.get_anim_index(0), -1);
        assert_eq!(single.get_anim_index(1), 42);
        assert_eq!(single.get_anim_index(2), -1);
    }

    #[test]
    fn unavailable_animation_call_retains_only_prior_native_writes() {
        let mut storage = [0xa5; PLAYER_CONTROL_STORAGE_BYTES];
        store(&mut storage, 0x40c, 1);
        store(&mut storage, 0x410, 0);
        let mut mask = u32::MAX;
        let mut state = RawPlayerControlState::from_existing_storage(&mut storage);
        assert_eq!(
            state.set_lock(&mut mask, 1),
            Err(RequiredControlBackend::Direct {
                entry_rva: 0xe8e0,
                receiver: ControlReceiver::PlayerOffset(0xbc),
                arguments: vec![],
            })
        );
        assert_eq!(state.as_bytes()[0xada], 1);
        assert_eq!(word(state.as_bytes(), 0x410), 0);
        assert_eq!(word(state.as_bytes(), 0x9c), 0);
        assert_eq!(word(state.as_bytes(), 0x40c), 1);
        assert_eq!(word(state.as_bytes(), 0x420), 0xa5a5a5a5);
        assert_eq!(mask, 0xffffdfff);
    }

    #[test]
    fn state_ten_stops_before_first_virtual_call_after_native_prefix() {
        let mut storage = [0xa5; PLAYER_CONTROL_STORAGE_BYTES];
        store(&mut storage, 0x40c, 10);
        store(&mut storage, 0x564, 0x10203040);
        let mut mask = u32::MAX;
        assert_eq!(
            set_control_lock(&mut storage, &mut mask, 255),
            Err(RequiredControlBackend::Virtual {
                object: 0x10203040,
                slot: 0x24,
                arguments: vec![0x10203040, 1],
            })
        );
        assert_eq!(storage[0xada], 255);
        assert_eq!(mask, 0xffffcfff);
        assert_eq!(word(&storage, 0x478), 0);
        assert_eq!(word(&storage, 0x44c), u32::MAX);
        assert_eq!(word(&storage, 0x410), 0xa5a5a5a5);
    }

    #[test]
    fn clear_animations_reads_live_owner_and_preserves_completed_calls_on_error() {
        struct Backend {
            calls: Vec<(ModelAnimationKey, u32)>,
        }
        impl ModelAnimationStopBackend for Backend {
            type Error = &'static str;
            fn stop_animation(
                &mut self,
                owner: &mut ModelAnimationOwner,
                model: ModelAnimationKey,
                slot: u32,
            ) -> Result<(), Self::Error> {
                if slot == 3 {
                    return Err("unavailable interpolator");
                }
                self.calls.push((model, slot));
                owner.model = Some(ModelAnimationKey(2));
                Ok(())
            }
        }
        let mut owner = ModelAnimationOwner::from_existing_model(Some(ModelAnimationKey(1)));
        let mut backend = Backend { calls: Vec::new() };
        assert_eq!(
            owner.clear_animations(&mut backend),
            Err(ClearAnimationsError::Backend {
                model: ModelAnimationKey(2),
                slot: 3,
                source: "unavailable interpolator",
            })
        );
        assert_eq!(
            backend.calls,
            vec![
                (ModelAnimationKey(1), 0),
                (ModelAnimationKey(2), 1),
                (ModelAnimationKey(2), 2)
            ]
        );
        assert_eq!(owner.model, Some(ModelAnimationKey(2)));
    }

    #[test]
    fn invalidated_live_model_has_explicit_error_instead_of_native_null_read() {
        struct Backend;
        impl ModelAnimationStopBackend for Backend {
            type Error = ();
            fn stop_animation(
                &mut self,
                owner: &mut ModelAnimationOwner,
                _: ModelAnimationKey,
                _: u32,
            ) -> Result<(), Self::Error> {
                owner.model = None;
                Ok(())
            }
        }
        let mut owner = ModelAnimationOwner::from_existing_model(Some(ModelAnimationKey(0)));
        assert_eq!(
            owner.clear_animations(&mut Backend),
            Err(ClearAnimationsError::MissingLiveModel { slot: 1 })
        );
    }
}
