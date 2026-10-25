//! Original scene2 loader's local name-map initialization, Game RVA 0x14120a.
//! PC SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! This borrows an existing scene. It neither loads objects nor advances to
//! actors: camera setup and ordered scene2 fields remain enclosing dependencies.
use crate::frame_transform::FrameKey;
use crate::mission_actor_stream::OriginalLoadMap;
use crate::model_loading::ModelArena;
use crate::scene_ownership::SceneKey;
use crate::script_scheduler::FrameReferenceId;
use std::collections::HashSet;

/// Publication order is retained separately from the logical map's key order.
#[derive(Debug, PartialEq, Eq)]
pub struct LoadMapPublication {
    pub name: Vec<u8>,
    pub frame: FrameReferenceId,
}
pub struct SceneLoadMapPrefix {
    pub map: OriginalLoadMap,
    pub publications: Vec<LoadMapPublication>,
}
pub struct SceneLoadMapError {
    pub prefix: SceneLoadMapPrefix,
    pub detail: String,
}

/// Executes the fixed no-mask EnumFrames(0x4ffff, 5455e0) projection, then
/// explicitly publishes primary, backdrop and omni sectors (541315..5414db).
/// Enumeration excludes sector roots and visits primary children followed by
/// backdrop children in native child/next order. All low sixteen type bits are
/// enabled; omni and near subtree flags are absent. Callback5455e0 copies the
/// current full name (+100), mapping a null name pointer to an empty key.
/// Native C_str allocation/refcounts and map balancing remain host projections.
/// The existing host map admits ASCII names; other name bytes return the
/// already-published prefix without continuing to later frames or sectors.
pub fn initialize_scene_load_map(
    arena: &ModelArena,
    scene: SceneKey,
) -> Result<SceneLoadMapPrefix, SceneLoadMapError> {
    let mut prefix = SceneLoadMapPrefix {
        map: OriginalLoadMap::for_preceding_loader(),
        publications: Vec::new(),
    };
    let result = (|| -> Result<(), String> {
        let sectors = arena.scene(scene).map_err(|e| format!("{e:?}"))?.sectors;
        for sector in &sectors[..2] {
            // A read-only callback never destroys or relinks frames, so the
            // original saved-next and lastDestroyedFrame branches cannot fire.
            let mut stack = Vec::new();
            if let Some(child) = arena
                .links(sector.frame())
                .map_err(|e| format!("{e:?}"))?
                .child
            {
                stack.push(child);
            }
            let mut visited = HashSet::new();
            while let Some(key) = stack.pop() {
                if !visited.insert(key.0) || visited.len() > 65_536 {
                    return Err("invalid or over-budget original enumeration hierarchy".into());
                }
                let links = arena.links(key).map_err(|e| format!("{e:?}"))?;
                publish(arena, key, &mut prefix)?;
                if let Some(next) = links.next {
                    stack.push(next);
                }
                if let Some(child) = links.child {
                    stack.push(child);
                }
            }
        }
        for sector in &sectors[..3] {
            publish(arena, sector.frame(), &mut prefix)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(prefix),
        Err(detail) => Err(SceneLoadMapError { prefix, detail }),
    }
}

fn publish(
    arena: &ModelArena,
    key: FrameKey,
    prefix: &mut SceneLoadMapPrefix,
) -> Result<(), String> {
    let loaded = arena.frame(key).map_err(|e| format!("{e:?}"))?;
    let name = if loaded.name_is_null {
        &[][..]
    } else {
        loaded.name_bytes.as_slice()
    };
    let frame = arena.lookup_reference(key);
    prefix.map.publish_loaded_frame(arena, name, frame)?;
    prefix.publications.push(LoadMapPublication {
        name: name.to_vec(),
        frame,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_initializer_and_callback_eight_native_schedules() {
        // Synthetic normalized identities from unchanged native block/callback
        // execution, including actual scene/frame/subtree enumeration at fixed
        // flags4ffff. Native allocation and tree insertion were controlled.
        let expected: [&[(usize, &[u8])]; 8] = [
            &[
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (3, b"duplicate"),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (4, b"duplicate"),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (3, b"duplicate"),
                (4, b"duplicate"),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (5, b""),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (3, b"Primary sector"),
                (5, b""),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (4, b"duplicate"),
                (5, b""),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (3, b"Primary sector"),
                (4, b"duplicate"),
                (5, b""),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
        ];
        for (case, expected) in expected.iter().enumerate() {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let sectors = arena.scene(scene).unwrap().sectors;
            let frames = [
                sectors[0].frame(),
                sectors[1].frame(),
                sectors[2].frame(),
                arena.create_model().unwrap().root(),
                arena.create_model().unwrap().root(),
                arena.create_model().unwrap().root(),
            ];
            arena
                .set_frame_name(
                    frames[3],
                    Some(if case >= 4 {
                        b"Primary sector"
                    } else {
                        b"duplicate"
                    }),
                )
                .unwrap();
            arena.set_frame_name(frames[4], Some(b"duplicate")).unwrap();
            arena
                .set_frame_name(frames[5], if case % 2 == 1 { None } else { Some(b"") })
                .unwrap();
            for index in (0..3).rev() {
                if case & (1 << index) != 0 {
                    arena
                        .link_frame(
                            frames[index + 3],
                            Some(sectors[usize::from(index == 2)].frame()),
                            0,
                        )
                        .unwrap();
                }
            }
            let result = match initialize_scene_load_map(&arena, scene) {
                Ok(x) => x,
                Err(e) => panic!("{}", e.detail),
            };
            let actual = result
                .publications
                .iter()
                .map(|p| (p.frame, p.name.as_slice()))
                .collect::<Vec<_>>();
            let expected = expected
                .iter()
                .map(|(id, name)| (arena.lookup_reference(frames[*id]), *name))
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "native schedule {case}");
            for (_, name) in expected.iter().rev() {
                let last = expected
                    .iter()
                    .rev()
                    .find(|(_, key)| key == name)
                    .unwrap()
                    .0;
                assert_eq!(result.map.slot(name), Some(Some(last)));
            }
        }
    }

    #[test]
    fn existing_scene_hierarchy_publication_and_sector_overwrite() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        let a = arena.create_model().unwrap().root();
        let b = arena.create_model().unwrap().root();
        let c = arena.create_model().unwrap().root();
        let omitted = arena.create_model().unwrap().root();
        arena.set_frame_name(a, Some(b"Primary sector")).unwrap();
        arena.set_frame_name(b, Some(b"duplicate")).unwrap();
        arena.set_frame_name(c, Some(b"duplicate")).unwrap();
        arena.set_frame_name(omitted, Some(b"omitted")).unwrap();
        arena.link_frame(a, Some(sectors[0].frame()), 0).unwrap();
        arena.link_frame(b, Some(a), 0).unwrap();
        arena.link_frame(c, Some(sectors[1].frame()), 0).unwrap();
        arena
            .link_frame(omitted, Some(sectors[2].frame()), 0)
            .unwrap();
        let result = match initialize_scene_load_map(&arena, scene) {
            Ok(result) => result,
            Err(error) => panic!("{}", error.detail),
        };
        assert_eq!(
            result
                .publications
                .iter()
                .map(|x| x.name.as_slice())
                .collect::<Vec<_>>(),
            [
                b"Primary sector".as_slice(),
                b"duplicate",
                b"duplicate",
                b"Primary sector",
                b"Backdrop sector",
                b"Omni sector"
            ]
        );
        assert_eq!(
            result.map.slot(b"duplicate"),
            Some(Some(arena.lookup_reference(c)))
        );
        assert_eq!(
            result.map.slot(b"Primary sector"),
            Some(Some(arena.lookup_reference(sectors[0].frame())))
        );
        assert_eq!(result.map.slot(b"omitted"), None);
        assert_eq!(result.map.slot(b"Near sector"), None);
    }

    #[test]
    fn null_and_allocated_empty_full_names_share_the_empty_key() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        let null = arena.create_model().unwrap().root();
        let empty = arena.create_model().unwrap().root();
        arena.set_frame_name(empty, Some(b"")).unwrap();
        arena.link_frame(null, Some(sectors[0].frame()), 0).unwrap();
        arena
            .link_frame(empty, Some(sectors[1].frame()), 0)
            .unwrap();
        let result = match initialize_scene_load_map(&arena, scene) {
            Ok(result) => result,
            Err(error) => panic!("{}", error.detail),
        };
        assert_eq!(result.publications[0].name, b"");
        assert_eq!(result.publications[1].name, b"");
        assert_eq!(
            result.map.slot(b""),
            Some(Some(arena.lookup_reference(empty)))
        );
    }

    #[test]
    fn unsupported_name_preserves_already_published_live_identity() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        let good = arena.create_model().unwrap().root();
        let unsupported = arena.create_model().unwrap().root();
        arena.set_frame_name(good, Some(b"retained")).unwrap();
        arena.set_frame_name(unsupported, Some(&[0xe8])).unwrap();
        arena.link_frame(good, Some(sectors[0].frame()), 0).unwrap();
        arena
            .link_frame(unsupported, Some(sectors[1].frame()), 0)
            .unwrap();
        let error = match initialize_scene_load_map(&arena, scene) {
            Ok(_) => panic!("unsupported host map name accepted"),
            Err(error) => error,
        };
        assert_eq!(error.prefix.publications.len(), 1);
        assert_eq!(
            error.prefix.map.slot(b"retained"),
            Some(Some(arena.lookup_reference(good)))
        );
        assert_eq!(error.prefix.map.slot(b"Primary sector"), None);
        assert!(error.detail.contains("name domain"));
    }
}
