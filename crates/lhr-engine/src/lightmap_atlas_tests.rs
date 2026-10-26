use super::*;

fn rgb(size: [u16; 2], color: [u8; 3]) -> RectangleInput {
    RectangleInput::Rgb {
        width: size[0],
        height: size[1],
        pixels: color
            .repeat(usize::from(size[0]) * usize::from(size[1]))
            .into_boxed_slice(),
    }
}
fn pixel(texture: &lhr_assets::Texture, x: usize, y: usize) -> [u8; 4] {
    let at = (y * texture.width as usize + x) * 4;
    texture.rgba[at..at + 4].try_into().unwrap()
}

#[test]
fn equal_area_last_wins_original_order_is_retained_and_rotation_is_real() {
    let mut registry = LightmapAtlasRegistry::new().unwrap();
    let mut ramp = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            ramp.extend_from_slice(&[x * 40, y * 50, 37]);
        }
    }
    let key = registry
        .allocate(vec![
            RectangleInput::Rgb {
                width: 4,
                height: 4,
                pixels: ramp.into_boxed_slice(),
            },
            rgb([4, 4], [7, 8, 9]),
        ])
        .unwrap();
    let view = registry.allocation(key).unwrap();
    assert_eq!([view.texture.width, view.texture.height], [128, 128]);
    assert_eq!(
        view.placements,
        &[
            RectanglePlacement {
                texture_size: [4, 4],
                offset: [0, 4],
                rotated: true
            },
            RectanglePlacement {
                texture_size: [4, 4],
                offset: [0, 0],
                rotated: false
            },
        ]
    );
    assert_eq!(pixel(view.texture, 0, 0), [7, 8, 9, 255]);
    assert_eq!(pixel(view.texture, 1, 6), [80, 50, 37, 255]);
    assert_eq!(
        view.placements[0].source_uv_origin([128, 128]),
        [2.0, 4.0 / 128.0]
    );
    assert_eq!(view.source_memory_bytes, 128);
    assert_eq!(registry.diagnostics(key).unwrap().retained_rgb_bytes, 0);
    registry.release(key).unwrap();
    assert_eq!(registry.statistics(), AtlasStatistics::default());
}

#[test]
fn source_candidate_order_lost_strips_and_rotated_free_slot_are_preserved() {
    let mut r = LightmapAtlasRegistry::new().unwrap();
    let key = r.allocate(vec![rgb([128, 256], [1, 2, 3])]).unwrap();
    let view = r.allocation(key).unwrap();
    assert_eq!([view.texture.width, view.texture.height], [256, 128]);
    assert!(view.placements[0].rotated);
    r.release(key).unwrap();
    let key = r.allocate(vec![rgb([126, 126], [4, 5, 6])]).unwrap();
    let atlas = r.allocation(key).unwrap().atlas;
    let d = r.atlas_diagnostics(atlas).unwrap();
    assert_eq!((d.free_area, d.lost_area, d.free_rectangles), (0, 508, 0));
    let another = r.allocate(vec![rgb([4, 4], [7, 8, 9])]).unwrap();
    assert_ne!(atlas, r.allocation(another).unwrap().atlas);
    r.release(key).unwrap();
    r.release(another).unwrap();
    let key = r
        .allocate(vec![rgb([100, 20], [2, 3, 4]), rgb([10, 24], [8, 9, 10])])
        .unwrap();
    let view = r.allocation(key).unwrap();
    assert_eq!(view.placements[0].offset, [0, 0]);
    assert_eq!(
        view.placements[1],
        RectanglePlacement {
            texture_size: [10, 24],
            offset: [100, 0],
            rotated: true
        }
    );
    assert_eq!(r.atlas_diagnostics(view.atlas).unwrap().free_area, 14144);
    r.release(key).unwrap();
}

#[test]
fn shared_atlas_release_whitens_reclaims_and_keeps_other_owners_alive() {
    let mut r = LightmapAtlasRegistry::new().unwrap();
    let a = r.allocate(vec![rgb([4, 4], [20, 40, 60])]).unwrap();
    let b = r.allocate(vec![rgb([4, 4], [80, 100, 120])]).unwrap();
    let atlas = r.allocation(a).unwrap().atlas;
    assert_eq!(r.allocation(b).unwrap().atlas, atlas);
    assert_eq!(r.atlas_diagnostics(atlas).unwrap().references, 2);
    r.release(a).unwrap();
    assert_eq!(pixel(r.texture(atlas).unwrap(), 0, 0), [255; 4]);
    assert_eq!(pixel(r.texture(atlas).unwrap(), 0, 4), [80, 100, 120, 255]);
    assert_eq!(
        r.atlas_diagnostics(atlas).unwrap(),
        AtlasDiagnostics {
            references: 1,
            free_area: 16368,
            lost_area: 0,
            free_rectangles: 3,
            revision: 3
        }
    );
    let c = r.allocate(vec![rgb([4, 4], [130, 150, 170])]).unwrap();
    let view = r.allocation(c).unwrap();
    assert_eq!(view.atlas, atlas);
    assert_eq!(
        view.placements[0],
        RectanglePlacement {
            texture_size: [4, 4],
            offset: [0, 0],
            rotated: true
        }
    );
    assert_eq!(pixel(view.texture, 0, 0), [130, 150, 170, 255]);
    r.release(b).unwrap();
    assert_eq!(pixel(r.texture(atlas).unwrap(), 0, 4), [255; 4]);
    r.release(c).unwrap();
    assert_eq!(
        r.texture(atlas).unwrap_err(),
        LightmapAtlasError::StaleAtlas
    );
    assert_eq!(r.release(a), Err(LightmapAtlasError::StaleAllocation));
    assert_eq!(r.statistics(), AtlasStatistics::default());
}

#[test]
fn zero_rectangles_keep_the_source_zero_free_area_and_single_colors_ignore_argb_alpha() {
    let mut r = LightmapAtlasRegistry::new().unwrap();
    let a = r.allocate(Vec::new()).unwrap();
    let b = r.allocate(Vec::new()).unwrap();
    let atlas = r.allocation(a).unwrap().atlas;
    assert_eq!(r.allocation(b).unwrap().atlas, atlas);
    assert!(r.allocation(a).unwrap().placements.is_empty());
    assert_eq!(r.allocation(a).unwrap().source_memory_bytes, 0);
    assert_eq!(r.atlas_diagnostics(atlas).unwrap().free_area, 0);
    let c = r
        .allocate(vec![
            RectangleInput::SingleColor { argb: 0x00123456 },
            RectangleInput::SingleColor { argb: 0xffabcdef },
        ])
        .unwrap();
    let view = r.allocation(c).unwrap();
    assert_ne!(view.atlas, atlas);
    assert!(!view.placements[0].rotated && !view.placements[1].rotated);
    assert_eq!(pixel(view.texture, 0, 0), [0xab, 0xcd, 0xef, 255]);
    assert_eq!(pixel(view.texture, 0, 3), [0x12, 0x34, 0x56, 255]);
    r.release(a).unwrap();
    r.release(b).unwrap();
    r.release(c).unwrap();
    assert_eq!(r.statistics(), AtlasStatistics::default());
}

#[test]
fn shrink_retries_filter_the_original_pixels_and_drop_them_only_after_upload() {
    let mut r = LightmapAtlasRegistry::new().unwrap();
    let mut original = Vec::new();
    for y in 0..8 {
        for x in 0..1536 {
            original.extend_from_slice(&[
                ((x * 5 + y * 9) % 256) as u8,
                ((x * 13 + y * 3) % 256) as u8,
                ((x * 7 + y * 19) % 256) as u8,
            ]);
        }
    }
    let key = r
        .allocate(vec![RectangleInput::Rgb {
            width: 1536,
            height: 8,
            pixels: original.into_boxed_slice(),
        }])
        .unwrap();
    let view = r.allocation(key).unwrap();
    assert_eq!([view.texture.width, view.texture.height], [512, 128]);
    assert_eq!(
        view.placements[0],
        RectanglePlacement {
            texture_size: [384, 4],
            offset: [0, 0],
            rotated: false
        }
    );
    // Independent pinned C++ ResizeImage oracle, original 1536x8 directly to384x4.
    for (x, y, rgb) in [
        (0, 0, [11, 21, 19]),
        (1, 0, [31, 73, 47]),
        (383, 0, [242, 212, 212]),
        (0, 1, [27, 28, 52]),
        (0, 3, [58, 38, 117]),
        (383, 3, [36, 197, 87]),
    ] {
        assert_eq!(pixel(view.texture, x, y), [rgb[0], rgb[1], rgb[2], 255]);
    }
    assert_eq!(r.diagnostics(key).unwrap().shrink_passes, 2);
    assert_eq!(r.diagnostics(key).unwrap().retained_rgb_bytes, 0);
    r.release(key).unwrap();
}

#[test]
fn failures_keep_owned_prefix_and_foreign_or_retired_keys_cannot_release_it() {
    let mut r = LightmapAtlasRegistry::new().unwrap();
    let mut other = LightmapAtlasRegistry::new().unwrap();
    let foreign = other.allocate(Vec::new()).unwrap();
    assert_eq!(r.release(foreign), Err(LightmapAtlasError::ForeignKey));
    let failed = r.allocate(vec![rgb([2, 4], [1, 2, 3])]).unwrap_err();
    let key = failed.allocation.unwrap();
    assert_eq!(
        failed.error,
        LightmapAtlasError::UnsupportedDimensions {
            rectangle: 0,
            size: [2, 4]
        }
    );
    assert_eq!(r.diagnostics(key).unwrap().phase, AllocationPhase::Failed);
    assert_eq!(
        r.allocation(key).unwrap_err(),
        LightmapAtlasError::IncompleteAllocation
    );
    assert_eq!(r.statistics().failed_allocations, 1);
    r.release(key).unwrap();
    assert_eq!(r.diagnostics(key), Err(LightmapAtlasError::StaleAllocation));
    other.release(foreign).unwrap();
    let rectangles = (0..16385).map(|_| rgb([4, 4], [70, 80, 90])).collect();
    let failed = r.allocate(rectangles).unwrap_err();
    assert_eq!(failed.error, LightmapAtlasError::NoFit);
    let key = failed.allocation.unwrap();
    let d = r.diagnostics(key).unwrap();
    assert_eq!(
        (d.phase, d.area, d.shrink_passes, d.atlas),
        (AllocationPhase::Failed, 262160, 1, None)
    );
    assert_eq!(d.retained_rgb_bytes, 16385 * 4 * 4 * 3 * 2);
    r.release(key).unwrap();
    assert_eq!(r.statistics(), AtlasStatistics::default());
}
