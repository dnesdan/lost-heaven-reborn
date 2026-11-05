//! Source-derived unlit presentation decisions from pinned mafia-re 29b65b0:
//! I3d_object.cpp queue/draw branches and T_l.cpp KEEP_DIFFUSE/packed alpha.
//! Takes EFFECTIVE live flags, never assumes serialized asset flags are live.
use crate::visual_resources::I3dMaterial;
use lhr_render::{AlphaMode, FrontFace, MaterialQueue, OriginalMaterial, RenderMaterial};

/// Index order after source vertices are reflected across presentation Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceIndexOrder {
    Retained,
    Reversed,
}

/// Surviving GPU front for the ordinary native D3DCULL_CCW profile. The native
/// value names the culled winding; wgpu FrontFace names the surviving winding.
/// Source Z reflection reverses orientation. Assembly also reverses each source
/// triangle, whereas the canonical owned-buffer bridge retains its indices.
/// Native SetCullState aliases CCW through m_CullMode; mirror rendering's
/// flipped driver profile must supply its own resolved state when admitted.
pub const fn source_ccw_cull_front_face(indices: SourceIndexOrder) -> FrontFace {
    match indices {
        SourceIndexOrder::Retained => FrontFace::Clockwise,
        SourceIndexOrder::Reversed => FrontFace::CounterClockwise,
    }
}

/// Admitted material-only presentation: object fade/transparency/lightness are
/// explicitly one. Lighting/envmaps/fog and their vertex colors remain separate.
pub fn live_material(
    material: &I3dMaterial,
    has_texture: bool,
    sort_center: [f32; 3],
    front_face: FrontFace,
) -> Result<RenderMaterial, String> {
    live_material_with_fade(material, has_texture, sort_center, front_face, 1.)
}
/// The same unlit decisions with an actual source-selected object LOD fade.
/// Transparency/lightness remain one; source lighting is still separate.
pub fn live_material_with_fade(
    material: &I3dMaterial,
    has_texture: bool,
    sort_center: [f32; 3],
    front_face: FrontFace,
    fade: f32,
) -> Result<RenderMaterial, String> {
    if !material
        .diffuse
        .into_iter()
        .all(|v| v.is_finite() && (0. ..=1.).contains(&v))
        || !material.alpha.is_finite()
        || !(0. ..=1.).contains(&material.alpha)
        || !material.emissive.into_iter().all(f32::is_finite)
        || !sort_center.into_iter().all(f32::is_finite)
    {
        return Err("Original presentation material is outside its admitted scalar domain".into());
    }
    if !fade.is_finite() || !(0. ..=1.).contains(&fade) {
        return Err("Object LOD fade is outside the presentation domain".into());
    }
    let total_alpha = crate::native_matrix::checked_f32_mul(material.alpha, fade)
        .map_err(|_| "Object/material alpha arithmetic unavailable")?;
    let flags = material.flags;
    let transparent = flags & (0x40000000 | 0x80000000) != 0 || total_alpha < 1.;
    let queue = if !has_texture && total_alpha >= 1. {
        MaterialQueue::Opaque
    } else if transparent {
        MaterialQueue::Transparent
    } else if flags & 0x20000000 != 0 {
        MaterialQueue::ColorKey
    } else {
        MaterialQueue::Opaque
    };
    let alpha = if flags & 0x80000000 != 0 {
        AlphaMode::Additive
    } else if flags & 0x40000000 != 0 || total_alpha < 1. {
        if material.emissive.into_iter().any(|v| v.to_bits() != 0) {
            AlphaMode::Additive
        } else {
            AlphaMode::Blend
        }
    } else if flags & 0x20000000 != 0 {
        AlphaMode::ColorKey
    } else {
        AlphaMode::Opaque
    };
    let diffuse = if has_texture && flags & 0x08000000 == 0 {
        [1.; 3]
    } else {
        material.diffuse
    };
    // Source FloatToInt uses the active FPU rounding; this presentation path
    // admits the engine's round-to-nearest-even profile explicitly.
    let opacity = (total_alpha * 255.).round_ties_even() / 255.;
    Ok(RenderMaterial::Original(OriginalMaterial {
        color: [diffuse[0], diffuse[1], diffuse[2], opacity],
        alpha,
        queue,
        has_texture,
        color_key_reference: 200,
        two_sided: flags & 0x10000000 != 0,
        front_face,
        visual_z_bias: false,
        sort_center,
    }))
}

/// T_l.cpp PrepareLights/SetVertexAlpha pack one alpha for the entire LOD
/// from its FIRST face-group material. Draw queue/blend decisions independently
/// read each current group. Object transparency is one in this admitted bridge.
pub fn packed_object_vertex_alpha(
    first_material: Option<&I3dMaterial>,
    fade: f32,
) -> Result<f32, String> {
    let alpha = first_material.map_or(1., |material| material.alpha);
    if !alpha.is_finite()
        || !(0. ..=1.).contains(&alpha)
        || !fade.is_finite()
        || !(0. ..=1.).contains(&fade)
    {
        return Err("First face-group alpha/fade is outside the presentation domain".into());
    }
    let total = crate::native_matrix::checked_f32_mul(fade, alpha)
        .map_err(|_| "Object/first-material alpha arithmetic unavailable")?;
    Ok((total * 255.).round_ties_even() / 255.)
}

/// I3d_lit_object.cpp m_RenderFGroup's unconstructed branch. Alpha/add/key
/// textures remain outside this bounded path. Diffuse, opacity and object fade
/// do not alter the source cyan texture factor; texture alpha SELECTARG1 is
/// preserved by the raw texture producer and opaque framebuffer write.
pub fn unconstructed_lit_material(
    material: &I3dMaterial,
    has_texture: bool,
    sort_center: [f32; 3],
    front_face: FrontFace,
) -> Result<RenderMaterial, String> {
    unconstructed_lit_material_flags(material.flags, has_texture, sort_center, front_face)
}
/// Constructor-state static presentation shares the same selector, supplying
/// explicitly mapped effective material flags without inventing a live owner.
pub fn unconstructed_lit_material_flags(
    flags: u32,
    has_texture: bool,
    sort_center: [f32; 3],
    front_face: FrontFace,
) -> Result<RenderMaterial, String> {
    if has_texture && flags & 0xe0000000 != 0 {
        return Err("Unconstructed lit texture alpha/add/color-key unavailable".into());
    }
    if !sort_center.into_iter().all(f32::is_finite) {
        return Err("Unconstructed lit sort center is nonfinite".into());
    }
    Ok(RenderMaterial::Original(OriginalMaterial {
        color: [0., 1., 1., 1.],
        alpha: AlphaMode::Opaque,
        queue: MaterialQueue::Opaque,
        has_texture,
        color_key_reference: 200,
        two_sided: flags & 0x10000000 != 0,
        front_face,
        visual_z_bias: false,
        sort_center,
    }))
}

/// PC debug-draw color (`MakeColor`, mafia-re ls3df/I3D/I3d_driver.cpp:721, a
/// `static inline` helper with no RVA — CONTRACT helper, no body bar). All
/// PC-only driver draw names are upstream working guesses per I3D.h:4380.
/// `FloatToInt` is the FPU `fistp` round-to-nearest (ls3df.h:82-89), NOT the
/// `cvttss2si` truncation used by the shadow-vertex pack: `127.5` rounds to
/// `128`. `alpha` is a transparency byte, so `a = (255 - (alpha & 0xff)) *
/// (1/255)` before the `ia = FloatToInt(a * 255)` round-trip, then
/// `ia << 24 | r << 16 | g << 8 | b` with full-word shifts (overflow bleeds
/// across bytes, as natively). Nonfinite inputs and unrepresentable
/// `fistp` results are domain failures (the original folds `0x80000000`
/// through `fistp` — disclosed hardening, not modeled).
pub fn pack_debug_draw_color(alpha: u32, color: [f32; 3]) -> Result<u32, String> {
    fn fistp(value: f32) -> Result<i32, String> {
        if !value.is_finite() {
            return Err("Debug-draw FloatToInt input is nonfinite".into());
        }
        let rounded = value.round_ties_even();
        if !rounded.is_finite() || rounded < -2_147_483_648.0 || rounded > 2_147_483_647.0 {
            return Err("Debug-draw FloatToInt result is unrepresentable".into());
        }
        #[allow(clippy::cast_possible_truncation)]
        Ok(rounded as i32)
    }
    let transparency = 255u32.wrapping_sub(alpha & 0xFF);
    #[allow(clippy::cast_precision_loss)]
    let a = transparency as f32 * (1.0f32 / 255.0f32);
    let ia = fistp(a * 255.0)?;
    let r = fistp(color[0] * 255.0)?;
    let g = fistp(color[1] * 255.0)?;
    let b = fistp(color[2] * 255.0)?;
    Ok((ia as u32).wrapping_shl(24)
        | (r as u32).wrapping_shl(16)
        | (g as u32).wrapping_shl(8)
        | (b as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual_resources::VisualResources;
    fn source() -> lhr_assets::Material {
        lhr_assets::Material {
            flags: 0x40000,
            ambient: [0.; 3],
            diffuse: [0.25, 0.5, 1.],
            texture: "synthetic.bmp".into(),
            emission: [0.; 3],
            opacity: 1.,
            alpha_texture: None,
            reflection: None,
            environment_texture: None,
            animation: None,
        }
    }
    #[test]
    fn queue_classification_and_blend_decisions_are_independent() {
        let mut resources = VisualResources::new().unwrap();
        let key = resources.create_material().unwrap();
        let mut src = source();
        for (flags, has_texture, opacity, queue, alpha) in [
            (
                0x80000000,
                false,
                1.,
                MaterialQueue::Opaque,
                AlphaMode::Additive,
            ),
            (
                0x80040000,
                true,
                1.,
                MaterialQueue::Transparent,
                AlphaMode::Additive,
            ),
            (
                0x20040000,
                true,
                1.,
                MaterialQueue::ColorKey,
                AlphaMode::ColorKey,
            ),
            (
                0x60040000,
                true,
                1.,
                MaterialQueue::Transparent,
                AlphaMode::Blend,
            ),
            (
                0x40000,
                true,
                0.5,
                MaterialQueue::Transparent,
                AlphaMode::Blend,
            ),
        ] {
            src.opacity = opacity;
            resources
                .install_loaded_material_scalars(key, &src, flags)
                .unwrap();
            let RenderMaterial::Original(m) = live_material(
                resources.material(key).unwrap(),
                has_texture,
                [0.; 3],
                FrontFace::CounterClockwise,
            )
            .unwrap() else {
                panic!()
            };
            assert_eq!((m.queue, m.alpha), (queue, alpha));
        }
    }
    #[test]
    #[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
    fn keep_diffuse_and_packed_opacity_reach_actual_gpu_readback() {
        use lhr_render::{Mesh, Renderer, Scene, Texture, Vertex};
        use std::sync::Arc;
        let instance = lhr_render::instance();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let mut resources = VisualResources::new().unwrap();
        let key = resources.create_material().unwrap();
        let mut src = source();
        let mut policy = |flags: u32, opacity: f32| {
            src.opacity = opacity;
            resources
                .install_loaded_material_scalars(key, &src, flags | 0x10000000)
                .unwrap();
            live_material(
                resources.material(key).unwrap(),
                true,
                [1., 1., 0.],
                FrontFace::CounterClockwise,
            )
            .unwrap()
        };
        let mesh = Mesh {
            vertices: vec![
                Vertex {
                    position: [0., 0., 0.],
                    uv: [0.; 2],
                },
                Vertex {
                    position: [2., 0., 0.],
                    uv: [0.; 2],
                },
                Vertex {
                    position: [0., 2., 0.],
                    uv: [0.; 2],
                },
            ],
            indices: vec![0, 1, 2],
            texture: Some(Arc::new(Texture {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
                repeat: [false; 2],
            })),
            material: policy(0x40000, 1.),
        };
        let mut renderer = pollster::block_on(Renderer::with_adapter(
            &adapter,
            &Scene { meshes: vec![mesh] },
            wgpu::TextureFormat::Rgba8Unorm,
            false,
        ))
        .unwrap();
        let pixel = |renderer: &Renderer| -> [u8; 4] {
            let rgba = renderer.readback_screen(32, 32, [2., 2.]).unwrap();
            rgba[(8 * 32 + 8) * 4..(8 * 32 + 8) * 4 + 4]
                .try_into()
                .unwrap()
        };
        assert_eq!(pixel(&renderer), [255; 4]); // textured WITHOUT KEEP_DIFFUSE uses white
        renderer
            .update_render_material(0, policy(0x08040000, 1.))
            .unwrap();
        assert_eq!(pixel(&renderer), [64, 128, 255, 255]);
        renderer
            .update_render_material(0, policy(0x08040000, 0.5))
            .unwrap();
        let got = pixel(&renderer);
        // Quantized source vertex alpha 128/255; alpha uses SrcAlpha as well.
        assert!((i16::from(got[3]) - 191).abs() <= 1);
        assert!((i16::from(got[0]) - 35).abs() <= 1);
    }
    #[test]
    fn debug_draw_color_rounds_to_nearest_not_trunc() {
        // Opaque red: (255-0)/255 = 1.0 -> 255, channels 255,0,0.
        assert_eq!(
            pack_debug_draw_color(0, [1.0, 0.0, 0.0]).unwrap(),
            0xFFFF0000
        );
        // Fully transparent black: (255-255)/255 = 0.0 -> 0.
        assert_eq!(
            pack_debug_draw_color(0xFF, [0.0, 0.0, 0.0]).unwrap(),
            0x00000000
        );
        // Low alpha byte only: 0x1FF masks to 0xFF -> transparent.
        assert_eq!(
            pack_debug_draw_color(0x1FF, [0.0, 0.0, 0.0]).unwrap(),
            0x00000000
        );
        // 127.5 rounds to even 128, proving FPU round-to-nearest over trunc.
        assert_eq!(
            pack_debug_draw_color(0, [0.5, 0.0, 0.0]).unwrap(),
            0xFF800000
        );
        assert!(pack_debug_draw_color(0, [f32::NAN, 0.0, 0.0]).is_err());
        assert!(pack_debug_draw_color(0, [f32::INFINITY, 0.0, 0.0]).is_err());
    }
}
