//! Explicit source-derived material decisions for the unlit presentation bridge.
//! Legacy host draws keep their existing menu/inspection policy.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum RenderMaterial {
    #[default]
    Legacy,
    Original(OriginalMaterial),
    /// Native null face-group material: opaque, no texture, and the current
    /// driver cull state is inherited instead of being set by this draw.
    OriginalNull {
        color: [f32; 4],
        sort_center: [f32; 3],
        /// Effective VISF_ZBIAS belongs to the visual, including null groups.
        visual_z_bias: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AlphaMode {
    Opaque,
    ColorKey,
    Blend,
    Additive,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MaterialQueue {
    Opaque,
    ColorKey,
    Transparent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FrontFace {
    CounterClockwise,
    Clockwise,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OriginalMaterial {
    /// Unlit diffuse multiplier and already packed/normalized vertex opacity.
    pub color: [f32; 4],
    pub alpha: AlphaMode,
    /// Queue classification is independent of the draw's blend state.
    pub queue: MaterialQueue,
    /// Effective source texture pointer is nonnull. Native opaque groups
    /// without one share radix key0 with NULL-material groups.
    pub has_texture: bool,
    pub color_key_reference: u8,
    pub two_sided: bool,
    pub front_face: FrontFace,
    /// Native render.cpp selects SetZBias(2) for VISF_ZBIAS (0x100).
    /// The backend uses a documented portable D32 mapping, not native numeric parity.
    pub visual_z_bias: bool,
    /// Presentation-space world bounds center, used for view-depth ordering.
    pub sort_center: [f32; 3],
}
impl RenderMaterial {
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::Legacy => true,
            Self::OriginalNull {
                color, sort_center, ..
            } => {
                color
                    .into_iter()
                    .all(|v| v.is_finite() && (0. ..=1.).contains(&v))
                    && sort_center.into_iter().all(f32::is_finite)
            }
            Self::Original(m) => {
                m.color
                    .into_iter()
                    .all(|v| v.is_finite() && (0. ..=1.).contains(&v))
                    && m.sort_center.into_iter().all(f32::is_finite)
            }
        }
    }
    pub(crate) fn opaque_sort_zero(self) -> bool {
        match self {
            Self::OriginalNull { .. } => true,
            Self::Original(m) => m.queue == MaterialQueue::Opaque && !m.has_texture,
            Self::Legacy => false,
        }
    }
    pub(crate) fn parameters(self) -> [f32; 8] {
        match self {
            Self::Legacy => [1., 1., 1., 1., -1., 0., 0., 0.],
            Self::OriginalNull { color, .. } => {
                [color[0], color[1], color[2], color[3], -1., 0., 0., 0.]
            }
            Self::Original(m) => {
                let cutoff = match m.alpha {
                    AlphaMode::Opaque => -1.,
                    AlphaMode::ColorKey => f32::from(m.color_key_reference) / 255.,
                    AlphaMode::Blend | AlphaMode::Additive => 1. / 255.,
                };
                [
                    m.color[0], m.color[1], m.color[2], m.color[3], cutoff, 0., 0., 0.,
                ]
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PipelineKey {
    pub alpha: AlphaMode,
    pub two_sided: bool,
    pub front_face: FrontFace,
    pub visual_z_bias: bool,
}
impl From<OriginalMaterial> for PipelineKey {
    fn from(m: OriginalMaterial) -> Self {
        Self {
            alpha: m.alpha,
            two_sided: m.two_sided,
            front_face: m.front_face,
            visual_z_bias: m.visual_z_bias,
        }
    }
}
/// This coefficient is specific to the renderer's D32 float attachment.
pub(crate) const ORIGINAL_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Portable adapter policy, NOT an assertion of original-driver numeric parity.
/// DXVK cbd2206d72d3d9dd9941d5d868bb03a858e85f9b:
/// - src/d3d8/d3d8_caps.h: ZBIAS_SCALE = -1 / 65535 (D16 convention).
/// - d3d8_device.cpp SetRenderState maps native integer bias to that float.
/// - src/d3d9/d3d9_util.h GetDepthBufferRValue: D32 fallback scale is 2^23.
/// Native VISF_ZBIAS selects 2. wgpu's integer constant therefore rounds
/// (-2 / 65535) * 2^23 to -256; slope and clamp remain zero. DXVK may use
/// extension/vendor-specific representations that wgpu does not expose here.
/// A format change must choose and validate its own mapping explicitly.
pub(crate) fn visual_depth_bias(
    format: wgpu::TextureFormat,
    enabled: bool,
) -> wgpu::DepthBiasState {
    assert_eq!(
        format,
        wgpu::TextureFormat::Depth32Float,
        "VISF_ZBIAS mapping requires Depth32Float"
    );
    wgpu::DepthBiasState {
        constant: if enabled { -256 } else { 0 },
        slope_scale: 0.,
        clamp: 0.,
    }
}

pub(crate) fn blend(alpha: AlphaMode) -> Option<wgpu::BlendState> {
    match alpha {
        AlphaMode::Opaque | AlphaMode::ColorKey => None,
        AlphaMode::Blend | AlphaMode::Additive => {
            let component = wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: if alpha == AlphaMode::Blend {
                    wgpu::BlendFactor::OneMinusSrcAlpha
                } else {
                    wgpu::BlendFactor::One
                },
                operation: wgpu::BlendOperation::Add,
            };
            // D3D8 uses the same source/destination factors for color AND alpha.
            Some(wgpu::BlendState {
                color: component,
                alpha: component,
            })
        }
    }
}

#[cfg(test)]
mod depth_bias_tests {
    use super::*;
    #[test]
    fn portable_bias_uses_pinned_dxvk_d32_fallback() {
        let mapped = (-2_f64 / 65535. * f64::from(1 << 23)).round() as i32;
        assert_eq!(
            visual_depth_bias(ORIGINAL_DEPTH_FORMAT, true).constant,
            mapped
        );
        assert_eq!(visual_depth_bias(ORIGINAL_DEPTH_FORMAT, false).constant, 0);
    }
    #[test]
    #[should_panic(expected = "VISF_ZBIAS mapping requires Depth32Float")]
    fn another_depth_format_cannot_silently_reuse_the_d32_coefficient() {
        visual_depth_bias(wgpu::TextureFormat::Depth24Plus, true);
    }
}
