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
    /// Presentation-space world bounds center, used for view-depth ordering.
    pub sort_center: [f32; 3],
}
impl RenderMaterial {
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::Legacy => true,
            Self::OriginalNull { color, sort_center } => {
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
}
impl From<OriginalMaterial> for PipelineKey {
    fn from(m: OriginalMaterial) -> Self {
        Self {
            alpha: m.alpha,
            two_sided: m.two_sided,
            front_face: m.front_face,
        }
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
