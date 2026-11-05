#![forbid(unsafe_code)]
//! Render LOD selection (`GetObjectLOD`).
//!
//! LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! RVA 0x61e30. Pinned mafia-re ab67b13 `ls3df/I3D/render/render.cpp:115-182`
//! (`GetObjectLOD`; PS2 name, evidenced original symbol).
//!
//! Contract tier this wave: reviewed upstream source plus Rust tests, no
//! Ghidra body claimed. Float policy: plain f32 `*` and `/` with single
//! rounding, no fused multiply-add, no f64 substitution. NaN/negative-zero
//! and division-by-zero flows follow IEEE f32 comparisons exactly as the
//! source spells them (`<=`, `>`, `==`).

/// Callback message for LOD changes (I3D.h:301).
pub const LS3D_CALLBACK_LOD: u32 = 0x400;
/// LOD follows loaded textures (I3D.h:2683).
pub const VISF_TEXTURE_LOD: u32 = 0x10;
/// Texture-LOD path changed the stored level (I3D.h:2705).
pub const VISF_LOD_CHANGED: u32 = 0x2000000;
/// Texture is resident in a manager slot (IGraph.h:650).
pub const ITC_LOADED: u32 = 0x80000000;

/// Host admission error. The native routine trusts `num_lods` and level
/// pointers; empty level storage would read out of bounds there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LodError {
    InvalidLevels,
}

/// One mesh level's CPU inputs for the LOD decision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LodLevel {
    /// `lod_dist` threshold. `None` texture means `GetTexture()` returned null.
    pub lod_dist: f32,
    /// Level texture state, or `None` when the material has no texture.
    pub texture: Option<LodTexture>,
}

/// One level's texture inputs: `Request()` outcome plus the `ITC_LOADED` bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LodTexture {
    /// `true` when `Request()` succeeds (texture ready).
    pub request_succeeds: bool,
    /// `true` when `m_Flags & ITC_LOADED != 0`.
    pub itc_loaded: bool,
}

/// Typed live view of one object's LOD inputs and mutable LOD words.
/// Borrows the actual state; the callback may mutate it and the texture
/// branch rereads it afterwards, exactly as the original does.
#[derive(Clone, Debug, PartialEq)]
pub struct LodObject {
    /// `m_WorldScale`.
    pub world_scale: f32,
    /// `cur_lod` (may hold the one-past-last cull sentinel).
    pub cur_lod: i32,
    /// `m_FrmFlags` (`LS3D_CALLBACK_LOD` selects the callback).
    pub frm_flags: u32,
    /// `vis_flags` (`VISF_TEXTURE_LOD` / `VISF_LOD_CHANGED`).
    pub vis_flags: u32,
    /// `mesh->levels[0..num_lods]`; `None` is a null level pointer.
    pub levels: Vec<Option<LodLevel>>,
}

fn levels_len(object: &LodObject) -> Result<i32, LodError> {
    i32::try_from(object.levels.len()).map_err(|_| LodError::InvalidLevels)
}

/// LOD-change callback (`old_lod`, `new_lod`, live object view).
/// Lifetime-generic alias for the `get_object_lod` callback slot; no `Send` /
/// `Sync` / `'static` bound beyond the borrowed `FnMut` itself.
pub type LodChangedCallback<'a> = dyn FnMut(i32, i32, &mut LodObject) + 'a;

/// LS3DF RVA 0x61e30 `GetObjectLOD` (mafia-re `ls3df/I3D/render/render.cpp:115-182`).
///
/// Order-exact contract: 1-lod vs N-lod distance select with
/// `s = RSLodQuality * m_WorldScale` (f32) and `d = dist_2 / (s * s)` (f32),
/// then the LOD-change callback BEFORE the texture branch, then the
/// `RS_TEXTURELOWDETAIL` guard, the `Request()` / `ITC_LOADED` scan that
/// skips `lod` itself, and the `cur_lod` / `VISF_LOD_CHANGED` stores.
/// State is reread live after the mutating callback; nothing is cached
/// across it. Null level/texture links are skipped (disclosed host
/// hardening — native dereferences the levels[i] chain unconditionally and
/// faults). The texture-LOD culled case (`lod > last`) stores nothing.
/// The texture-LOD resident case always returns 0.
pub fn get_object_lod(
    object: &mut LodObject,
    dist_2: f32,
    rs_lod_quality: f32,
    texture_low_detail: bool,
    on_lod: Option<&mut LodChangedCallback<'_>>,
) -> Result<i32, LodError> {
    if object.levels.is_empty() {
        return Err(LodError::InvalidLevels);
    }
    let num = levels_len(object)?;
    let mut lod: i32;
    if num == 1 {
        let level = object.levels[0].as_ref();
        let Some(level) = level else {
            return Ok(0);
        };
        if level.lod_dist <= 0.0 {
            return Ok(0);
        }
        let s = rs_lod_quality * object.world_scale;
        let d = dist_2 / (s * s);
        lod = if d > level.lod_dist { 1 } else { 0 };
    } else {
        let s = rs_lod_quality * object.world_scale;
        let d = dist_2 / (s * s);
        let num_dec = num - 1;
        let mut index: i32 = 0;
        let mut found = false;
        while index < num_dec {
            if let Some(level) = object.levels[index as usize].as_ref() {
                if d <= level.lod_dist {
                    found = true;
                    break;
                }
            }
            index += 1;
        }
        lod = index;
        if !found {
            if let Some(level) = object.levels[lod as usize].as_ref() {
                if level.lod_dist > 0.0 && d > level.lod_dist {
                    lod = num;
                }
            }
        }
    }
    if lod != object.cur_lod && (object.frm_flags & LS3D_CALLBACK_LOD) != 0 {
        if let Some(callback) = on_lod {
            let old = object.cur_lod;
            callback(old, lod, &mut *object);
        }
    }
    if (object.vis_flags & VISF_TEXTURE_LOD) != 0 {
        if object.levels.is_empty() {
            return Ok(lod);
        }
        let last = levels_len(object)? - 1;
        if lod <= last {
            if texture_low_detail && lod < last {
                lod += 1;
            }
            let need_scan = match object.levels[lod as usize].as_ref() {
                None => false,
                Some(level) => match level.texture.as_ref() {
                    None => false,
                    Some(texture) => !texture.request_succeeds,
                },
            };
            if need_scan {
                let mut chosen: Option<i32> = None;
                let mut i: i32 = 0;
                while i < last {
                    if i != lod {
                        if let Some(level) = object.levels[i as usize].as_ref() {
                            if let Some(texture) = level.texture.as_ref() {
                                if texture.itc_loaded {
                                    chosen = Some(i);
                                    break;
                                }
                            }
                        }
                    }
                    i += 1;
                }
                if let Some(found) = chosen {
                    lod = found;
                } else {
                    lod = last;
                }
            }
            if object.cur_lod != lod {
                object.cur_lod = lod;
                object.vis_flags |= VISF_LOD_CHANGED;
            }
            return Ok(0);
        }
        return Ok(lod);
    }
    object.cur_lod = lod;
    Ok(lod)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(dist: f32) -> LodLevel {
        LodLevel {
            lod_dist: dist,
            texture: None,
        }
    }

    fn textured(dist: f32, request_succeeds: bool, itc_loaded: bool) -> LodLevel {
        LodLevel {
            lod_dist: dist,
            texture: Some(LodTexture {
                request_succeeds,
                itc_loaded,
            }),
        }
    }

    #[test]
    fn single_lod_selects_one_past_on_far_distance() {
        let mut object = LodObject {
            world_scale: 2.0,
            cur_lod: 0,
            frm_flags: 0,
            vis_flags: 0,
            levels: vec![Some(plain(1.0))],
        };
        // s = 1.0 * 2.0 = 2.0; s*s = 4.0; d = 8.0/4.0 = 2.0 > 1.0 -> lod 1.
        assert_eq!(get_object_lod(&mut object, 8.0, 1.0, false, None), Ok(1));
        assert_eq!(object.cur_lod, 1);
        // d = 4.0/4.0 = 1.0, not > 1.0 -> lod 0.
        assert_eq!(get_object_lod(&mut object, 4.0, 1.0, false, None), Ok(0));
        assert_eq!(object.cur_lod, 0);
    }

    #[test]
    fn single_lod_null_or_nonpositive_returns_zero_without_effects() {
        let mut null = LodObject {
            world_scale: 1.0,
            cur_lod: 5,
            frm_flags: LS3D_CALLBACK_LOD,
            vis_flags: 0,
            levels: vec![None],
        };
        let mut callback_ran = false;
        {
            let mut cb = |_: i32, _: i32, _: &mut LodObject| {
                callback_ran = true;
            };
            assert_eq!(
                get_object_lod(&mut null, 100.0, 1.0, false, Some(&mut cb)),
                Ok(0)
            );
        }
        assert!(!callback_ran);
        assert_eq!(null.cur_lod, 5);
        let mut zero = LodObject {
            world_scale: 1.0,
            cur_lod: 5,
            frm_flags: LS3D_CALLBACK_LOD,
            vis_flags: 0,
            levels: vec![Some(plain(0.0))],
        };
        assert_eq!(get_object_lod(&mut zero, 100.0, 1.0, false, None), Ok(0));
        assert_eq!(zero.cur_lod, 5);
    }

    #[test]
    fn multi_lod_picks_first_covering_or_one_past_last() {
        let mut object = LodObject {
            world_scale: 1.0,
            cur_lod: 0,
            frm_flags: 0,
            vis_flags: 0,
            levels: vec![Some(plain(10.0)), Some(plain(20.0)), Some(plain(30.0))],
        };
        // s = 1.0; d = 15.0 -> first covering index 1.
        assert_eq!(get_object_lod(&mut object, 15.0, 1.0, false, None), Ok(1));
        assert_eq!(object.cur_lod, 1);
        // d = 5.0 -> index 0.
        assert_eq!(get_object_lod(&mut object, 5.0, 1.0, false, None), Ok(0));
        // d = 35.0 > last 30.0 (positive) -> one past last (3).
        assert_eq!(get_object_lod(&mut object, 35.0, 1.0, false, None), Ok(3));
        assert_eq!(object.cur_lod, 3);
    }

    #[test]
    fn multi_lod_skips_null_levels_and_keeps_last_on_nonpositive_tail() {
        let mut object = LodObject {
            world_scale: 1.0,
            cur_lod: 0,
            frm_flags: 0,
            vis_flags: 0,
            levels: vec![None, Some(plain(20.0)), Some(plain(0.0))],
        };
        // d = 5.0: index 0 is null (skip), index 1 covers -> lod 1.
        assert_eq!(get_object_lod(&mut object, 5.0, 1.0, false, None), Ok(1));
        // d = 50.0: no covering level; tail is 0.0 (not > 0) so lod stays 2.
        assert_eq!(get_object_lod(&mut object, 50.0, 1.0, false, None), Ok(2));
        assert_eq!(object.cur_lod, 2);
    }

    #[test]
    fn lod_callback_runs_before_texture_branch_and_is_reread() {
        let mut object = LodObject {
            world_scale: 1.0,
            cur_lod: 0,
            frm_flags: LS3D_CALLBACK_LOD,
            vis_flags: VISF_TEXTURE_LOD,
            levels: vec![
                Some(textured(100.0, true, true)),
                Some(textured(200.0, true, true)),
            ],
        };
        // Computed lod is 0 (d = 5.0 <= 100.0), so without interference the
        // texture path would return 0 and keep cur_lod 0. The callback clears
        // VISF_TEXTURE_LOD; a reread must then take the plain store path and
        // return lod itself (0) with a plain store instead of the texture
        // early-0 path. Distinguish by forcing lod 1 first.
        object.cur_lod = 0;
        // d = 150.0 -> N-lod select gives 1 (100 < 150 <= 200).
        let mut seen: Vec<(i32, i32)> = Vec::new();
        {
            let mut cb = |old: i32, new: i32, obj: &mut LodObject| {
                seen.push((old, new));
                obj.vis_flags &= !VISF_TEXTURE_LOD;
            };
            let result = get_object_lod(&mut object, 150.0, 1.0, false, Some(&mut cb));
            assert_eq!(result, Ok(1));
        }
        assert_eq!(seen, vec![(0, 1)]);
        assert_eq!(object.vis_flags & VISF_TEXTURE_LOD, 0);
        assert_eq!(object.cur_lod, 1);
    }

    #[test]
    fn texture_low_detail_steps_to_next_coarser_level() {
        let mut object = LodObject {
            world_scale: 1.0,
            cur_lod: 0,
            frm_flags: 0,
            vis_flags: VISF_TEXTURE_LOD,
            levels: vec![
                Some(textured(10.0, true, true)),
                Some(textured(20.0, true, true)),
                Some(textured(30.0, true, true)),
            ],
        };
        // d = 5.0 selects 0; low-detail guard steps to 1; resident so no scan.
        assert_eq!(get_object_lod(&mut object, 5.0, 1.0, true, None), Ok(0));
        assert_eq!(object.cur_lod, 1);
        assert_ne!(object.vis_flags & VISF_LOD_CHANGED, 0);
    }

    #[test]
    fn texture_scan_skips_lod_itself_and_falls_back_to_last() {
        let mut object = LodObject {
            world_scale: 1.0,
            cur_lod: 7,
            frm_flags: 0,
            vis_flags: VISF_TEXTURE_LOD,
            levels: vec![
                Some(textured(10.0, false, false)),
                Some(textured(20.0, true, true)),
                Some(textured(30.0, true, true)),
            ],
        };
        // d = 5.0 selects 0; level 0 requests and fails; scan skips i == 0
        // and picks 1 (first loaded). Returns 0, stores 1.
        assert_eq!(get_object_lod(&mut object, 5.0, 1.0, false, None), Ok(0));
        assert_eq!(object.cur_lod, 1);
        // Now nothing but the failing lod itself is loaded: levels 1 has no
        // loaded texture either, so the scan exhausts and falls back to last.
        object.cur_lod = 7;
        object.vis_flags &= !VISF_LOD_CHANGED;
        object.levels = vec![
            Some(textured(10.0, false, false)),
            Some(textured(20.0, true, false)),
            Some(textured(30.0, true, false)),
        ];
        assert_eq!(get_object_lod(&mut object, 5.0, 1.0, false, None), Ok(0));
        assert_eq!(object.cur_lod, 2);
    }

    #[test]
    fn texture_culled_level_stores_nothing() {
        let mut object = LodObject {
            world_scale: 1.0,
            cur_lod: 1,
            frm_flags: 0,
            vis_flags: VISF_TEXTURE_LOD,
            levels: vec![Some(plain(10.0)), Some(plain(20.0))],
        };
        // d = 50.0 selects one past last (2); texture branch with lod > last
        // stores nothing and returns lod.
        assert_eq!(get_object_lod(&mut object, 50.0, 1.0, false, None), Ok(2));
        assert_eq!(object.cur_lod, 1);
        assert_eq!(object.vis_flags & VISF_LOD_CHANGED, 0);
    }
}
