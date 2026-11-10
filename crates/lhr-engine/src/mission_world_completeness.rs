//! Mission world completeness diagnostics over caller-owned inputs.
//!
//! Reference: mafia-re `d0fbd6d`, game/mission/Mission.cpp,
//! `C_mission::Open` (PC 0x5409d0): `check.bin` (navigation) before
//! `road.bin` (roads) before `OpenBin` (scene2), with `cache.bin` probed
//! before `OpenBin` and opened after. This module does not load navigation
//! graphs, road networks, or base caches into live owners; it reports file
//! presence, bounded parse summaries, and authored actor association so the
//! MissionOpen promotion can be ordered by evidence.
//!
//! All inputs stay caller-owned. Mission names follow the probe boundary:
//! 1..=64 ASCII letters, digits, underscores or hyphens.
#![forbid(unsafe_code)]

/// Resolved world file paths for one mission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissionWorldFiles {
    pub check: String,
    pub road: String,
    pub cache: String,
}

/// Mission-name rejection at the command boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionNameError {
    Empty,
    TooLong,
    InvalidChar(u8),
}

/// Resolve the three world file paths for a mission without touching storage.
pub fn mission_world_files(mission: &str) -> Result<MissionWorldFiles, MissionNameError> {
    if mission.is_empty() {
        return Err(MissionNameError::Empty);
    }
    if mission.len() > 64 {
        return Err(MissionNameError::TooLong);
    }
    for byte in mission.bytes() {
        if !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')) {
            return Err(MissionNameError::InvalidChar(byte));
        }
    }
    Ok(MissionWorldFiles {
        check: format!("missions/{mission}/check.bin"),
        road: format!("missions/{mission}/road.bin"),
        cache: format!("missions/{mission}/cache.bin"),
    })
}

/// Presence of one optional world file. Absence is the original `ROpen`-failed
/// branch, not a missing backend; source errors stay distinct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldFileState {
    pub path: String,
    pub present: bool,
    pub bytes: usize,
}

/// Installation resolver failure; distinct from genuine absence.
#[derive(Debug)]
pub enum WorldFileReadError {
    Source(String),
}

/// Read presence + byte length for each optional world file in order.
/// Resolution completes before any parse enters; a resolver failure stops the
/// sequence without touching live owners.
pub fn read_world_file_sizes(
    installation: &lhr_assets::Installation,
    files: &MissionWorldFiles,
) -> Result<Vec<WorldFileState>, WorldFileReadError> {
    let mut out = Vec::with_capacity(3);
    for path in [&files.check, &files.road, &files.cache] {
        let bytes = installation
            .read_optional(path)
            .map_err(|error| WorldFileReadError::Source(error.to_string()))?;
        out.push(WorldFileState {
            path: path.clone(),
            present: bytes.is_some(),
            bytes: bytes.map_or(0, |b| b.len()),
        });
    }
    Ok(out)
}

/// Bounded city-cache summary from `cache.bin` bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CityCacheSummary {
    pub parts: usize,
    pub objects: usize,
    pub unknown_chunks: usize,
}

/// City-cache parse failure; the bytes stay caller-owned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CityCacheSummaryError {
    pub detail: String,
}

/// Parse `cache.bin` through the existing bounded parser without inventing
/// placements. Quotas and version checks remain authoritative.
pub fn summarize_city_cache(bytes: &[u8]) -> Result<CityCacheSummary, CityCacheSummaryError> {
    let cache = lhr_assets::parse_city_cache(bytes).map_err(|error| CityCacheSummaryError {
        detail: error.to_string(),
    })?;
    Ok(CityCacheSummary {
        parts: cache.parts.len(),
        objects: cache.parts.iter().map(|part| part.objects.len()).sum(),
        unknown_chunks: cache.unknown_chunks,
    })
}

/// Authored actor association for mission control: the player spawn
/// (`TommyTAXIDRIVER`, kind 2) and the mission taxi (`taxi`, kind 4).
/// Association requires exactly one matching actor record and one matching
/// object record each; anything else is reported, not defaulted.
#[derive(Clone, Debug, PartialEq)]
pub struct MissionActorPresence {
    pub player_found: bool,
    pub taxi_found: bool,
    pub player_world_position: Option<[f32; 3]>,
    pub taxi_world_position: Option<[f32; 3]>,
}

/// Resolve player/taxi association from already-parsed scene2 metadata.
/// Metadata stays borrowed; no owner is constructed here.
pub fn resolve_mission_actors(scene: &lhr_assets::Scene) -> MissionActorPresence {
    MissionActorPresence {
        player_found: actor_object(scene, "TommyTAXIDRIVER", 2).is_ok(),
        taxi_found: actor_object(scene, "taxi", 4).is_ok(),
        player_world_position: actor_world_position(scene, "TommyTAXIDRIVER", 2),
        taxi_world_position: actor_world_position(scene, "taxi", 4),
    }
}

/// Named detector records (kind 5) from scene2 metadata with their object
/// world positions where exactly one matching object exists. No owner is
/// constructed here; ambiguous or missing objects report as a missing
/// position, never as invented coordinates.
pub fn mission_detector_standings(scene: &lhr_assets::Scene) -> Vec<(String, Option<[f32; 3]>)> {
    let mut standings = Vec::new();
    for actor in &scene.actors {
        if actor.kind != Some(5) {
            continue;
        }
        let position = if scene
            .objects
            .iter()
            .filter(|object| object.name.eq_ignore_ascii_case(&actor.name))
            .count()
            == 1
        {
            scene
                .objects
                .iter()
                .find(|object| object.name.eq_ignore_ascii_case(&actor.name))
                .and_then(|object| object.world_position)
        } else {
            None
        };
        standings.push((actor.name.clone(), position));
    }
    standings
}

fn actor_object(scene: &lhr_assets::Scene, name: &str, kind: u32) -> Result<(), ()> {
    let actors = scene
        .actors
        .iter()
        .filter(|actor| actor.name.eq_ignore_ascii_case(name) && actor.kind == Some(kind))
        .count();
    let objects = scene
        .objects
        .iter()
        .filter(|object| object.name.eq_ignore_ascii_case(name))
        .count();
    if actors == 1 && objects == 1 {
        Ok(())
    } else {
        Err(())
    }
}

fn actor_world_position(scene: &lhr_assets::Scene, name: &str, kind: u32) -> Option<[f32; 3]> {
    if actor_object(scene, name, kind).is_err() {
        return None;
    }
    scene
        .objects
        .iter()
        .find(|object| object.name.eq_ignore_ascii_case(name))
        .and_then(|object| object.world_position)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene_chunk(kind: u16, bytes: &[u8]) -> Vec<u8> {
        [
            kind.to_le_bytes().as_slice(),
            &((bytes.len() + 6) as u32).to_le_bytes(),
            bytes,
        ]
        .concat()
    }

    #[test]
    fn world_paths_follow_mission_open_layout() {
        let files = mission_world_files("mise01").unwrap();
        assert_eq!(files.check, "missions/mise01/check.bin");
        assert_eq!(files.road, "missions/mise01/road.bin");
        assert_eq!(files.cache, "missions/mise01/cache.bin");
        assert_eq!(mission_world_files(""), Err(MissionNameError::Empty));
        assert_eq!(
            mission_world_files(&"a".repeat(65)),
            Err(MissionNameError::TooLong)
        );
        assert_eq!(
            mission_world_files("mise/01"),
            Err(MissionNameError::InvalidChar(b'/'))
        );
        assert!(mission_world_files("mise-01_x").is_ok());
    }

    #[test]
    fn city_cache_summary_counts_parts_and_objects() {
        // Minimal version-1 cache: root(500) -> one part(1000) -> one frame(2000).
        fn frame(model: &[u8]) -> Vec<u8> {
            let body = [
                (model.len() as u32).to_le_bytes().as_slice(),
                model,
                1f32.to_le_bytes().as_slice(),
                2f32.to_le_bytes().as_slice(),
                3f32.to_le_bytes().as_slice(),
                0f32.to_le_bytes().as_slice(),
                0f32.to_le_bytes().as_slice(),
                0f32.to_le_bytes().as_slice(),
                1f32.to_le_bytes().as_slice(),
                1f32.to_le_bytes().as_slice(),
                1f32.to_le_bytes().as_slice(),
                1f32.to_le_bytes().as_slice(),
                0u32.to_le_bytes().as_slice(),
                1f32.to_le_bytes().as_slice(),
                1f32.to_le_bytes().as_slice(),
                1f32.to_le_bytes().as_slice(),
                0u32.to_le_bytes().as_slice(),
            ]
            .concat();
            scene_chunk(2000, &body)
        }
        let part_body = [
            (4u32).to_le_bytes().as_slice(),
            b"part",
            0f32.to_le_bytes().as_slice(),
            0f32.to_le_bytes().as_slice(),
            10f32.to_le_bytes().as_slice(),
            [0f32; 16]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>()
                .as_slice(),
            frame(b"car.4ds").as_slice(),
            frame(b"man.4ds").as_slice(),
        ]
        .concat();
        let root_body = [
            1u32.to_le_bytes().as_slice(),
            scene_chunk(1000, &part_body).as_slice(),
        ]
        .concat();
        let bytes = scene_chunk(500, &root_body);
        let summary = summarize_city_cache(&bytes).unwrap();
        assert_eq!(
            summary,
            CityCacheSummary {
                parts: 1,
                objects: 2,
                unknown_chunks: 0,
            }
        );
        assert!(summarize_city_cache(&[]).is_err());
        assert!(summarize_city_cache(&bytes[..10]).is_err());
    }

    #[test]
    fn actor_resolution_requires_exact_association() {
        let mut scene = lhr_assets::Scene::default();
        assert_eq!(
            resolve_mission_actors(&scene),
            MissionActorPresence {
                player_found: false,
                taxi_found: false,
                player_world_position: None,
                taxi_world_position: None,
            }
        );
        scene.actors.push(lhr_assets::SceneActor {
            name: "TommyTAXIDRIVER".into(),
            kind: Some(2),
        });
        scene.objects.push(lhr_assets::SceneObject {
            name: "TommyTAXIDRIVER".into(),
            world_position: Some([1.0, 2.0, 3.0]),
            ..Default::default()
        });
        scene.actors.push(lhr_assets::SceneActor {
            name: "taxi".into(),
            kind: Some(4),
        });
        scene.objects.push(lhr_assets::SceneObject {
            name: "taxi".into(),
            world_position: Some([4.0, 5.0, 6.0]),
            ..Default::default()
        });
        assert_eq!(
            resolve_mission_actors(&scene),
            MissionActorPresence {
                player_found: true,
                taxi_found: true,
                player_world_position: Some([1.0, 2.0, 3.0]),
                taxi_world_position: Some([4.0, 5.0, 6.0]),
            }
        );
        // Ambiguous duplicate breaks association without a default.
        scene.objects.push(lhr_assets::SceneObject {
            name: "taxi".into(),
            ..Default::default()
        });
        let resolved = resolve_mission_actors(&scene);
        assert!(resolved.player_found);
        assert!(!resolved.taxi_found);
        assert_eq!(resolved.taxi_world_position, None);
    }

    #[test]
    fn detector_standings_list_kind_five_with_object_positions() {
        let mut scene = lhr_assets::Scene::default();
        assert!(mission_detector_standings(&scene).is_empty());
        scene.actors.push(lhr_assets::SceneActor {
            name: "TommyTAXIDRIVER".into(),
            kind: Some(2),
        });
        scene.actors.push(lhr_assets::SceneActor {
            name: "gate_a".into(),
            kind: Some(5),
        });
        scene.actors.push(lhr_assets::SceneActor {
            name: "gate_b".into(),
            kind: Some(5),
        });
        scene.objects.push(lhr_assets::SceneObject {
            name: "gate_a".into(),
            world_position: Some([1.0, 2.0, 3.0]),
            ..Default::default()
        });
        // gate_b has no object: position reports as missing, never invented.
        assert_eq!(
            mission_detector_standings(&scene),
            vec![
                ("gate_a".to_string(), Some([1.0, 2.0, 3.0])),
                ("gate_b".to_string(), None),
            ]
        );
        // Ambiguous duplicates report as missing positions as well.
        scene.objects.push(lhr_assets::SceneObject {
            name: "gate_a".into(),
            world_position: Some([9.0, 9.0, 9.0]),
            ..Default::default()
        });
        assert_eq!(
            mission_detector_standings(&scene),
            vec![("gate_a".to_string(), None), ("gate_b".to_string(), None),]
        );
    }
}
