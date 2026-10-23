//! The link-resolution continuation of Game `g_collision::LoadCollision`
//! (RVA 0x1c2b70, instructions 0x5c2c92..0x5c2d0f).
//!
//! Slots are visited backwards. Lookup overwrites the source-kind word before
//! type testing; rejected results overwrite it again with null. Only links with
//! `kind & 1 != 0` and type-1 survivors receive SetOn(true). The original names remain
//! byte strings. Constructors, scene ownership and LOD0 storage are external
//! boundaries; this continuation cannot materialize absent frames.

use crate::{
    frame_fast_lookup::{FastFrameError, I3dFastFrameTree},
    frame_transform::FrameKey,
};

/// Typed projection of the original link's first word. FrameKey is an opaque
/// caller identity, so a native frame pointer is never mistaken for a kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionLinkWord {
    SourceKind(u32),
    Frame(Option<FrameKey>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CollisionLinkSlot<'name> {
    pub name: &'name [u8],
    pub word: CollisionLinkWord,
}

impl<'name> CollisionLinkSlot<'name> {
    pub fn unresolved(kind: u32, name: &'name [u8]) -> Self {
        Self {
            name,
            word: CollisionLinkWord::SourceKind(kind),
        }
    }

    pub fn frame(&self) -> Option<FrameKey> {
        match self.word {
            CollisionLinkWord::Frame(frame) => frame,
            CollisionLinkWord::SourceKind(_) => None,
        }
    }
}

/// Mutable loader storage. The scene field is the caller's actual context+0x10
/// value, not a mission identity. Virtual overrides can change this state;
/// every iteration reloads it just as the original instructions do.
pub struct CollisionLinkState<'state, 'name> {
    pub scene: Option<u32>,
    pub links: &'state mut [CollisionLinkSlot<'name>],
    pub frames: &'state mut I3dFastFrameTree,
}

/// Original virtual call boundary. Overrides receive the live storage, so
/// reached effects can alter subsequent context, names, types and lookup.
pub trait CollisionLinkCalls {
    type Error;
    fn find_frame_fast(
        &mut self,
        state: &mut CollisionLinkState<'_, '_>,
        scene: Option<u32>,
        name: &[u8],
    ) -> Result<Option<FrameKey>, Self::Error>;
    fn frame_type(
        &mut self,
        state: &mut CollisionLinkState<'_, '_>,
        frame: FrameKey,
    ) -> Result<u32, Self::Error>;
    fn set_on(
        &mut self,
        state: &mut CollisionLinkState<'_, '_>,
        frame: FrameKey,
        on: bool,
    ) -> Result<(), Self::Error>;
}

/// The composed original fast-tree lookup, frame type field and base SetOn.
pub struct FastTreeCollisionLinkCalls;

impl CollisionLinkCalls for FastTreeCollisionLinkCalls {
    type Error = FastFrameError;

    fn find_frame_fast(
        &mut self,
        state: &mut CollisionLinkState<'_, '_>,
        scene: Option<u32>,
        name: &[u8],
    ) -> Result<Option<FrameKey>, Self::Error> {
        state.frames.find_frame_fast(scene, name)
    }

    fn frame_type(
        &mut self,
        state: &mut CollisionLinkState<'_, '_>,
        frame: FrameKey,
    ) -> Result<u32, Self::Error> {
        Ok(state.frames.frame(frame)?.frame_type)
    }

    fn set_on(
        &mut self,
        state: &mut CollisionLinkState<'_, '_>,
        frame: FrameKey,
        on: bool,
    ) -> Result<(), Self::Error> {
        state.frames.set_on(frame, on)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionLinkStage {
    FindFrameFast,
    FrameType,
    SetOn,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CollisionLinkResolutionError<E> {
    /// This typed host API admits one pass over source-kind words. A native
    /// pointer's bit pattern cannot be reconstructed from an opaque FrameKey.
    AlreadyResolved { link_index: usize },
    Backend {
        link_index: usize,
        stage: CollisionLinkStage,
        source: E,
    },
}

/// All writes and callbacks reached before failure remain visible. In
/// particular a type-read/SetOn failure retains the preceding frame write.
/// No preflight lookup or sorting changes the original call order.
pub fn resolve_collision_links(
    state: &mut CollisionLinkState<'_, '_>,
) -> Result<(), CollisionLinkResolutionError<FastFrameError>> {
    resolve_collision_links_with_calls(state, &mut FastTreeCollisionLinkCalls)
}

pub fn resolve_collision_links_with_calls<C: CollisionLinkCalls>(
    state: &mut CollisionLinkState<'_, '_>,
    calls: &mut C,
) -> Result<(), CollisionLinkResolutionError<C::Error>> {
    for link_index in (0..state.links.len()).rev() {
        let kind = match state.links[link_index].word {
            CollisionLinkWord::SourceKind(kind) => kind,
            CollisionLinkWord::Frame(_) => {
                return Err(CollisionLinkResolutionError::AlreadyResolved { link_index });
            }
        };
        let scene = state.scene;
        let name = state.links[link_index].name;
        let frame = calls
            .find_frame_fast(state, scene, name)
            .map_err(|source| CollisionLinkResolutionError::Backend {
                link_index,
                stage: CollisionLinkStage::FindFrameFast,
                source,
            })?;
        state.links[link_index].word = CollisionLinkWord::Frame(frame);
        if kind & 1 == 0 {
            continue;
        }
        // Native rereads the overwritten slot after the virtual lookup.
        if let CollisionLinkWord::Frame(Some(frame)) = state.links[link_index].word {
            let frame_type = calls.frame_type(state, frame).map_err(|source| {
                CollisionLinkResolutionError::Backend {
                    link_index,
                    stage: CollisionLinkStage::FrameType,
                    source,
                }
            })?;
            if frame_type != 1 {
                state.links[link_index].word = CollisionLinkWord::Frame(None);
            }
        }
        // Reload after the type-read boundary as well; a custom backend never
        // receives a cached frame that no longer occupies the original slot.
        if let CollisionLinkWord::Frame(Some(frame)) = state.links[link_index].word {
            calls.set_on(state, frame, true).map_err(|source| {
                CollisionLinkResolutionError::Backend {
                    link_index,
                    stage: CollisionLinkStage::SetOn,
                    source,
                }
            })?;
        }
    }
    Ok(())
}

/// Stable original slot indices after the continuation. Null and unresolved
/// slots remain distinguishable in `links`; neither creates a binding here.
pub fn resolved_collision_frames<'links>(
    links: &'links [CollisionLinkSlot<'_>],
) -> impl Iterator<Item = (usize, FrameKey)> + 'links {
    links
        .iter()
        .enumerate()
        .filter_map(|(index, link)| match link.word {
            CollisionLinkWord::Frame(Some(frame)) => Some((index, frame)),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_fast_lookup::{FastFrameStorage, OriginalFrameName};

    fn fixture() -> I3dFastFrameTree {
        let mut tree = I3dFastFrameTree::new(true);
        for (id, frame_type, parent, scene) in [
            (100, 5, None, Some(7)),
            (101, 5, None, Some(8)),
            (1, 1, Some(100), None),
            (2, 2, Some(100), None),
            (3, 1, Some(101), None),
        ] {
            tree.import_frame(FastFrameStorage {
                frame: FrameKey(id),
                name: None,
                original_name: OriginalFrameName::Null,
                frame_type,
                parent: parent.map(FrameKey),
                sector_scene: scene,
                flags: 0x4000_003c,
                left: None,
                right: None,
            })
            .unwrap();
        }
        tree.set_name(FrameKey(1), Some(b"mesh")).unwrap();
        tree.set_name(FrameKey(2), Some(b"volume")).unwrap();
        tree.set_name(FrameKey(3), Some(b"mesh")).unwrap();
        tree
    }

    #[derive(Default)]
    struct Calls {
        events: Vec<(CollisionLinkStage, u32)>,
        fail: Option<CollisionLinkStage>,
        change_scene_and_name: bool,
    }

    impl CollisionLinkCalls for Calls {
        type Error = &'static str;
        fn find_frame_fast(
            &mut self,
            state: &mut CollisionLinkState<'_, '_>,
            scene: Option<u32>,
            name: &[u8],
        ) -> Result<Option<FrameKey>, Self::Error> {
            self.events.push((
                CollisionLinkStage::FindFrameFast,
                name.first().copied().unwrap_or(0) as u32,
            ));
            if self.fail == Some(CollisionLinkStage::FindFrameFast) {
                return Err("lookup unavailable");
            }
            state
                .frames
                .find_frame_fast(scene, name)
                .map_err(|_| "invalid storage")
        }
        fn frame_type(
            &mut self,
            state: &mut CollisionLinkState<'_, '_>,
            frame: FrameKey,
        ) -> Result<u32, Self::Error> {
            self.events.push((CollisionLinkStage::FrameType, frame.0));
            if self.fail == Some(CollisionLinkStage::FrameType) {
                return Err("type unavailable");
            }
            Ok(state
                .frames
                .frame(frame)
                .map_err(|_| "invalid storage")?
                .frame_type)
        }
        fn set_on(
            &mut self,
            state: &mut CollisionLinkState<'_, '_>,
            frame: FrameKey,
            on: bool,
        ) -> Result<(), Self::Error> {
            self.events.push((CollisionLinkStage::SetOn, frame.0));
            state
                .frames
                .set_on(frame, on)
                .map_err(|_| "invalid storage")?;
            if self.change_scene_and_name {
                state.scene = Some(8);
                state.links[0].name = b"mesh";
            }
            if self.fail == Some(CollisionLinkStage::SetOn) {
                return Err("after reached effect");
            }
            Ok(())
        }
    }

    #[test]
    fn registered_frames_feed_reverse_slots_type_rejection_and_reached_set_on() {
        let mut tree = fixture();
        let mut links = [
            CollisionLinkSlot::unresolved(1, b"mesh"),
            CollisionLinkSlot::unresolved(3, b"volume"),
            CollisionLinkSlot::unresolved(2, b"volume"),
            CollisionLinkSlot::unresolved(1, b"missing"),
            CollisionLinkSlot::unresolved(1, b"Mesh"),
        ];
        let mut calls = Calls::default();
        resolve_collision_links_with_calls(
            &mut CollisionLinkState {
                scene: Some(7),
                links: &mut links,
                frames: &mut tree,
            },
            &mut calls,
        )
        .unwrap();
        assert_eq!(
            links.map(|slot| slot.word),
            [
                CollisionLinkWord::Frame(Some(FrameKey(1))),
                CollisionLinkWord::Frame(None),
                CollisionLinkWord::Frame(Some(FrameKey(2))),
                CollisionLinkWord::Frame(None),
                CollisionLinkWord::Frame(None),
            ]
        );
        assert_eq!(
            resolved_collision_frames(&links).collect::<Vec<_>>(),
            [(0, FrameKey(1)), (2, FrameKey(2))]
        );
        assert_eq!(
            calls.events,
            [
                (CollisionLinkStage::FindFrameFast, b'M' as u32),
                (CollisionLinkStage::FindFrameFast, b'm' as u32),
                (CollisionLinkStage::FindFrameFast, b'v' as u32),
                (CollisionLinkStage::FindFrameFast, b'v' as u32),
                (CollisionLinkStage::FrameType, 2),
                (CollisionLinkStage::FindFrameFast, b'm' as u32),
                (CollisionLinkStage::FrameType, 1),
                (CollisionLinkStage::SetOn, 1),
            ]
        );
        assert_eq!(tree.frame(FrameKey(1)).unwrap().flags & 1, 1);
        assert_eq!(tree.frame(FrameKey(2)).unwrap().flags & 1, 0);
    }

    #[test]
    fn virtual_effects_change_the_context_and_names_used_by_the_next_iteration() {
        let mut tree = fixture();
        let mut links = [
            CollisionLinkSlot::unresolved(1, b"missing"),
            CollisionLinkSlot::unresolved(1, b"mesh"),
        ];
        let mut calls = Calls {
            change_scene_and_name: true,
            ..Calls::default()
        };
        resolve_collision_links_with_calls(
            &mut CollisionLinkState {
                scene: Some(7),
                links: &mut links,
                frames: &mut tree,
            },
            &mut calls,
        )
        .unwrap();
        assert_eq!(links[1].frame(), Some(FrameKey(1)));
        assert_eq!(links[0].frame(), Some(FrameKey(3)));
        assert_eq!(tree.frame(FrameKey(3)).unwrap().flags & 1, 1);
    }

    #[test]
    fn unavailable_calls_preserve_exact_prefix_and_the_overwritten_word() {
        for stage in [
            CollisionLinkStage::FindFrameFast,
            CollisionLinkStage::FrameType,
            CollisionLinkStage::SetOn,
        ] {
            let mut tree = fixture();
            let mut links = [
                CollisionLinkSlot::unresolved(1, b"volume"),
                CollisionLinkSlot::unresolved(1, b"mesh"),
            ];
            let mut calls = Calls {
                fail: Some(stage),
                ..Calls::default()
            };
            let result = resolve_collision_links_with_calls(
                &mut CollisionLinkState {
                    scene: Some(7),
                    links: &mut links,
                    frames: &mut tree,
                },
                &mut calls,
            );
            assert!(
                matches!(result, Err(CollisionLinkResolutionError::Backend { link_index: 1, stage: observed, .. }) if observed == stage)
            );
            assert_eq!(links[0].word, CollisionLinkWord::SourceKind(1));
            assert_eq!(
                links[1].word,
                if stage == CollisionLinkStage::FindFrameFast {
                    CollisionLinkWord::SourceKind(1)
                } else {
                    CollisionLinkWord::Frame(Some(FrameKey(1)))
                }
            );
            assert_eq!(
                tree.frame(FrameKey(1)).unwrap().flags & 1,
                u32::from(stage == CollisionLinkStage::SetOn)
            );
            assert_eq!(
                calls.events.len(),
                match stage {
                    CollisionLinkStage::FindFrameFast => 1,
                    CollisionLinkStage::FrameType => 2,
                    CollisionLinkStage::SetOn => 3,
                }
            );
        }
    }

    #[test]
    fn empty_input_makes_no_calls_and_repeated_pass_stops_at_overwritten_kind() {
        let mut tree = fixture();
        let mut links = [];
        let mut calls = Calls::default();
        resolve_collision_links_with_calls(
            &mut CollisionLinkState {
                scene: None,
                links: &mut links,
                frames: &mut tree,
            },
            &mut calls,
        )
        .unwrap();
        assert!(calls.events.is_empty());
        let mut links = [
            CollisionLinkSlot::unresolved(0, b"mesh"),
            CollisionLinkSlot {
                name: b"mesh",
                word: CollisionLinkWord::Frame(None),
            },
        ];
        assert_eq!(
            resolve_collision_links_with_calls(
                &mut CollisionLinkState {
                    scene: Some(7),
                    links: &mut links,
                    frames: &mut tree
                },
                &mut calls
            ),
            Err(CollisionLinkResolutionError::AlreadyResolved { link_index: 1 })
        );
        assert!(calls.events.is_empty());
        assert_eq!(links[0].word, CollisionLinkWord::SourceKind(0));
    }
}
