//! Scoped original startup-effect callers from Game.exe SHA256
//! `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`.
//! Native helper names are unknown; the Rust names describe their observed work.
//! The script commands are `game_nightmission`, `enablemap` and `use_lightcache`.
//! Resource lookup and scene traversal are required dependencies, not no-ops.

/// The raw byte touched at mission+0x2d44. The caller, rather than this setter,
/// determines whether an argument is normalized to a boolean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NightMissionState {
    pub enabled_raw: u8,
}

impl NightMissionState {
    /// Original RVA0x7b590 takes the low byte, including non-boolean values.
    pub fn set_night_mission_raw(&mut self, argument: u32) {
        self.enabled_raw = argument as u8;
    }
}

/// A projection of the UI object, not the mission object. The fields correspond
/// to UI+0x46ec and UI+0x46f0; the resource value is an opaque backend token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapState {
    pub enabled_raw: u8,
    pub resource_value: u32,
}

pub trait MapResourceBackend {
    type Error;
    /// Bind the resource owner corresponding to the original global object.
    /// The state is live: callback changes and synchronous reentry are visible.
    fn lookup(&mut self, state: &mut MapState, key: u32) -> Result<u32, Self::Error>;
}

/// One physical eight-byte resource-table record. Values are opaque tokens;
/// the original loader and the lifetime of its pointed-to data are separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceRecord {
    pub key: u32,
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceLookupError {
    UnsupportedCount(u32),
    MissingRecord(u32),
}

/// Original lookup at RVA0x20fb40, including its capped initial search step,
/// unsigned comparisons and duplicate-key selection. `raw_count` is the word
/// at owner+4, not a Rust slice length: the routine can read record[raw_count].
/// Supply physical backing records, including any sentinel, and the token for
/// original default data at VA0x638390. This is not a loader or sorting pass.
/// Missing storage and unsupported 32-bit spans are explicit host errors.
pub fn lookup_resource_value(
    raw_count: u32,
    records: &[ResourceRecord],
    key: u32,
    default_value: u32,
) -> Result<u32, ResourceLookupError> {
    if raw_count > 0x1fff_ffff {
        return Err(ResourceLookupError::UnsupportedCount(raw_count));
    }
    let record = |index: u32| {
        records
            .get(index as usize)
            .ok_or(ResourceLookupError::MissingRecord(index))
    };
    let mut step = 0x1000;
    while step > raw_count {
        step >>= 1;
    }
    if step == 0 {
        return Ok(default_value);
    }
    let mut index = if record(step)?.key < key {
        raw_count.wrapping_sub(step).wrapping_sub(1)
    } else {
        0
    };
    step >>= 1;
    while step != 0 {
        let candidate = index.wrapping_add(step);
        step >>= 1;
        if record(candidate)?.key <= key {
            index = candidate;
        }
    }
    let selected = record(index)?;
    Ok(if selected.key == key {
        selected.value
    } else {
        default_value
    })
}

impl MapState {
    /// Original RVA0x1f9d10 stores the byte before looking up key0x5f, including
    /// when disabling the map or setting an unchanged value. The lookup result
    /// overwrites the resource field after callback completion. Adapter failure
    /// does not roll back the already-visible byte or callback mutations.
    pub fn set_map_enabled_raw<B: MapResourceBackend>(
        &mut self,
        backend: &mut B,
        argument: u32,
    ) -> Result<(), B::Error> {
        self.enabled_raw = argument as u8;
        let resource = backend.lookup(self, 0x5f)?;
        self.resource_value = resource;
        Ok(())
    }
}

/// Only the vector header and raw field touched by the light-cache caller.
/// Tokens identify retained storage; resetting the end does not copy, release
/// or read elements. It is not a reconstruction of the native allocator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightCacheHeader {
    pub begin_token: u32,
    pub end_token: u32,
    pub field_178: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneId(pub u32);

/// A retained frame identity. This is not an index into a copied traversal list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightFrameId(pub u32);

/// Raw fields read by the original light collector. `user_data_word` is the
/// first word of non-null frame user data; its meaning is not yet recovered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightFrameView {
    pub user_data_word: Option<u32>,
    pub frame_type: i32,
    pub light_kind: i32,
    pub first_child: Option<LightFrameId>,
    pub next_sibling: Option<LightFrameId>,
}

pub trait LightTraversalBackend {
    type Error;
    /// Resolve valid, live storage without side effects. A snapshot of all
    /// descendants is insufficient: append may change child/sibling links.
    fn frame(&self, id: LightFrameId) -> Result<LightFrameView, Self::Error>;
    /// Append this exact identity to retained cache storage, in call order.
    /// Allocation is a host dependency, not a reconstruction of RVA0x48cc0.
    fn append(&mut self, id: LightFrameId) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LightTraversalError<E> {
    Backend(E),
    /// Added host safety boundary, absent from the original function. Already
    /// appended entries remain visible, including when the graph has a cycle.
    BudgetExhausted,
}

/// Original recursive collector at RVA0x1b5cd0, represented with an explicit
/// host stack. The root is excluded. Nonzero user-data words suppress the
/// entire branch; native type 2 and signed kinds 1..=2 are appended before
/// descendants. Child and sibling links are read after synchronous effects.
/// This does not interpret 4DS file type tags as native frame type values.
pub fn collect_scene_lights<B: LightTraversalBackend>(
    backend: &mut B,
    root: LightFrameId,
    remaining_visits: &mut usize,
) -> Result<(), LightTraversalError<B::Error>> {
    let mut current = backend
        .frame(root)
        .map_err(LightTraversalError::Backend)?
        .first_child;
    let mut ancestors = Vec::new();
    while let Some(id) = current {
        if *remaining_visits == 0 {
            return Err(LightTraversalError::BudgetExhausted);
        }
        *remaining_visits -= 1;
        let view = backend.frame(id).map_err(LightTraversalError::Backend)?;
        if view.user_data_word.unwrap_or(0) == 0 {
            if view.frame_type == 2 && (1..=2).contains(&view.light_kind) {
                backend.append(id).map_err(LightTraversalError::Backend)?;
            }
            if let Some(child) = backend
                .frame(id)
                .map_err(LightTraversalError::Backend)?
                .first_child
            {
                ancestors.push(id);
                current = Some(child);
                continue;
            }
        }
        let mut completed = id;
        loop {
            current = backend
                .frame(completed)
                .map_err(LightTraversalError::Backend)?
                .next_sibling;
            if current.is_some() {
                break;
            }
            match ancestors.pop() {
                Some(parent) => completed = parent,
                None => break,
            }
        }
    }
    Ok(())
}

pub trait LightCacheBackend {
    type Error;
    /// Read the active scene through the live game/context ownership chain.
    /// This accessor must reject unavailable storage and have no side effects.
    fn active_scene(&self) -> Result<SceneId, Self::Error>;
    /// Required synchronous recursive helper at RVA0x1b5cd0. Implementations
    /// must collect real scene data; a recording backend proves ordering only.
    fn collect(&mut self, state: &mut LightCacheHeader, scene: SceneId) -> Result<(), Self::Error>;
}

impl LightCacheHeader {
    /// Original RVA0x1b5c80. Header reset precedes active-scene lookup and the
    /// tail-called traversal; callback writes survive. The apparent native
    /// element-copy loop is unreachable because its initial compare is equal.
    pub fn use_lightcache<B: LightCacheBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), B::Error> {
        self.end_token = self.begin_token;
        self.field_178 = u32::MAX;
        let scene = backend.active_scene()?;
        backend.collect(self, scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_lookup_preserves_physical_sentinel_and_duplicate_selection() {
        assert_eq!(lookup_resource_value(0, &[], 95, 7), Ok(7));
        let records = [
            ResourceRecord { key: 0, value: 100 },
            ResourceRecord { key: 5, value: 101 },
            ResourceRecord { key: 5, value: 102 },
            ResourceRecord { key: 5, value: 103 },
            ResourceRecord { key: 5, value: 104 },
            ResourceRecord {
                key: u32::MAX,
                value: 105,
            },
        ];
        // The original search need not choose the first or last equal record.
        assert_eq!(lookup_resource_value(5, &records, 5, 7), Ok(103));
        assert_eq!(
            lookup_resource_value(1, &records[..1], 0, 7),
            Err(ResourceLookupError::MissingRecord(1))
        );
    }

    #[test]
    fn resource_lookup_retains_the_original_capped_search_gap() {
        let mut records: Vec<_> = (0..10_000)
            .map(|key| ResourceRecord {
                key,
                value: key + 1,
            })
            .collect();
        records.push(ResourceRecord {
            key: u32::MAX,
            value: 10_001,
        });
        assert_eq!(lookup_resource_value(10_000, &records, 5_000, 17), Ok(17));
        assert_eq!(
            lookup_resource_value(10_000, &records, 9_998, 17),
            Ok(9_999)
        );
        assert_eq!(
            lookup_resource_value(u32::MAX, &[], 0, 17),
            Err(ResourceLookupError::UnsupportedCount(u32::MAX))
        );
    }

    struct Tree {
        frames: Vec<LightFrameView>,
        selected: Vec<LightFrameId>,
        mutate_links: bool,
    }
    impl LightTraversalBackend for Tree {
        type Error = &'static str;
        fn frame(&self, id: LightFrameId) -> Result<LightFrameView, Self::Error> {
            self.frames
                .get(id.0 as usize)
                .copied()
                .ok_or("missing frame")
        }
        fn append(&mut self, id: LightFrameId) -> Result<(), Self::Error> {
            self.selected.push(id);
            if self.mutate_links {
                match id.0 {
                    1 => {
                        self.frames[1].first_child = Some(LightFrameId(3));
                        self.frames[1].next_sibling = Some(LightFrameId(4));
                    }
                    3 => self.frames[1].next_sibling = Some(LightFrameId(5)),
                    _ => {}
                }
            }
            Ok(())
        }
    }
    fn light() -> LightFrameView {
        LightFrameView {
            frame_type: 2,
            light_kind: 1,
            ..LightFrameView::default()
        }
    }

    #[test]
    fn light_traversal_excludes_root_and_skips_whole_tagged_branches() {
        let mut tree = Tree {
            frames: vec![light(); 6],
            selected: Vec::new(),
            mutate_links: false,
        };
        tree.frames[0].user_data_word = Some(123); // Root tag is not tested.
        tree.frames[0].first_child = Some(LightFrameId(1));
        tree.frames[1].user_data_word = Some(1);
        tree.frames[1].first_child = Some(LightFrameId(2));
        tree.frames[1].next_sibling = Some(LightFrameId(3));
        tree.frames[3].user_data_word = Some(0);
        tree.frames[3].light_kind = 2;
        tree.frames[3].first_child = Some(LightFrameId(4));
        tree.frames[4].light_kind = 3;
        tree.frames[4].first_child = Some(LightFrameId(5));
        collect_scene_lights(&mut tree, LightFrameId(0), &mut 16).unwrap();
        assert_eq!(tree.selected, [LightFrameId(3), LightFrameId(5)]);
    }

    #[test]
    fn light_traversal_rereads_child_and_sibling_after_synchronous_append() {
        let mut tree = Tree {
            frames: vec![light(); 6],
            selected: Vec::new(),
            mutate_links: true,
        };
        tree.frames[0].first_child = Some(LightFrameId(1));
        tree.frames[1].first_child = Some(LightFrameId(2));
        collect_scene_lights(&mut tree, LightFrameId(0), &mut 16).unwrap();
        assert_eq!(
            tree.selected,
            [LightFrameId(1), LightFrameId(3), LightFrameId(5)]
        );
    }

    #[test]
    fn cyclic_light_storage_exhausts_host_budget_without_rolling_back() {
        let mut tree = Tree {
            frames: vec![light(); 2],
            selected: Vec::new(),
            mutate_links: false,
        };
        tree.frames[0].first_child = Some(LightFrameId(1));
        tree.frames[1].next_sibling = Some(LightFrameId(1));
        assert_eq!(
            collect_scene_lights(&mut tree, LightFrameId(0), &mut 2),
            Err(LightTraversalError::BudgetExhausted)
        );
        assert_eq!(tree.selected, [LightFrameId(1), LightFrameId(1)]);
    }

    /// Synthetic arguments from the unchanged native-body corpus. High bytes
    /// must not be treated as a nonzero boolean by the helper itself.
    #[test]
    fn night_setter_keeps_the_original_low_byte_domain() {
        let cases = [
            (0, 0),
            (1, 1),
            (255, 255),
            (256, 0),
            (257, 1),
            (0x8000_0000, 0),
            (0x8000_00ff, 255),
            (u32::MAX, 255),
        ];
        let mut state = NightMissionState { enabled_raw: 165 };
        for (argument, expected) in cases {
            state.set_night_mission_raw(argument);
            assert_eq!(state.enabled_raw, expected);
        }
    }

    #[derive(Default)]
    struct Resources {
        events: Vec<(u8, u32, u32, bool)>,
        mode: u8,
        nested: bool,
    }
    impl MapResourceBackend for Resources {
        type Error = &'static str;
        fn lookup(&mut self, state: &mut MapState, key: u32) -> Result<u32, Self::Error> {
            self.events
                .push((state.enabled_raw, state.resource_value, key, self.nested));
            if self.mode == 1 {
                state.enabled_raw = 128;
                state.resource_value = 0xaabb_ccdd;
            }
            if self.mode == 2 {
                state.enabled_raw = 128;
                return Err("resource unavailable");
            }
            let answer = if self.nested {
                0x6000_0001
            } else {
                0x6000_0000
            };
            if self.mode == 3 && !self.nested {
                self.nested = true;
                state.set_map_enabled_raw(self, 0x1234)?;
                self.nested = false;
            }
            Ok(answer)
        }
    }

    #[test]
    fn map_refresh_is_unconditional_and_observes_the_new_byte() {
        let mut state = MapState {
            enabled_raw: 0,
            resource_value: 0x1234_5678,
        };
        let mut resources = Resources::default();
        state.set_map_enabled_raw(&mut resources, 256).unwrap();
        state.set_map_enabled_raw(&mut resources, 0).unwrap();
        assert_eq!(
            resources.events,
            [(0, 0x1234_5678, 0x5f, false), (0, 0x6000_0000, 0x5f, false)]
        );
        assert_eq!(state.resource_value, 0x6000_0000);
    }

    #[test]
    fn map_callback_mutation_and_reentry_survive_the_outer_resource_store() {
        for (mode, byte, events) in [(1, 128, 1), (3, 0x34, 2)] {
            let mut state = MapState {
                enabled_raw: 165,
                resource_value: 0x1234_5678,
            };
            let mut resources = Resources {
                mode,
                ..Resources::default()
            };
            state.set_map_enabled_raw(&mut resources, 1).unwrap();
            assert_eq!(state.enabled_raw, byte);
            assert_eq!(state.resource_value, 0x6000_0000);
            assert_eq!(resources.events.len(), events);
            if mode == 3 {
                assert_eq!(resources.events[1], (0x34, 0x1234_5678, 0x5f, true));
            }
        }
    }

    #[test]
    fn resource_error_does_not_restore_the_old_ui_state() {
        let mut state = MapState {
            enabled_raw: 0,
            resource_value: 17,
        };
        let mut resources = Resources {
            mode: 2,
            ..Resources::default()
        };
        assert_eq!(
            state.set_map_enabled_raw(&mut resources, 1),
            Err("resource unavailable")
        );
        assert_eq!(state.enabled_raw, 128);
        assert_eq!(state.resource_value, 17);
    }

    struct Collector {
        missing_scene: bool,
        seen: Option<LightCacheHeader>,
    }
    impl LightCacheBackend for Collector {
        type Error = &'static str;
        fn active_scene(&self) -> Result<SceneId, Self::Error> {
            if self.missing_scene {
                Err("scene unavailable")
            } else {
                Ok(SceneId(7))
            }
        }
        fn collect(
            &mut self,
            state: &mut LightCacheHeader,
            scene: SceneId,
        ) -> Result<(), Self::Error> {
            assert_eq!(scene, SceneId(7));
            self.seen = Some(*state);
            state.end_token = state.end_token.wrapping_add(4);
            state.field_178 = 7;
            Ok(())
        }
    }

    #[test]
    fn light_cache_reset_precedes_traversal_and_preserves_its_changes() {
        let mut state = LightCacheHeader {
            begin_token: u32::MAX,
            end_token: 31,
            field_178: 99,
        };
        let mut backend = Collector {
            missing_scene: false,
            seen: None,
        };
        state.use_lightcache(&mut backend).unwrap();
        assert_eq!(
            backend.seen,
            Some(LightCacheHeader {
                begin_token: u32::MAX,
                end_token: u32::MAX,
                field_178: u32::MAX,
            })
        );
        assert_eq!(state.end_token, 3);
        assert_eq!(state.field_178, 7);
        backend.missing_scene = true;
        assert_eq!(state.use_lightcache(&mut backend), Err("scene unavailable"));
        assert_eq!(state.end_token, u32::MAX);
        assert_eq!(state.field_178, u32::MAX);
    }
}
