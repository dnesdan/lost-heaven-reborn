//! Explicit source-derived material decisions for the bounded presentation bridge.
//! Legacy host draws keep their existing menu/inspection policy.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum RenderMaterial {
    #[default]
    Legacy,
    Original(OriginalMaterial),
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
    /// Explicit source branch; texture lighting requires a real second texture.
    pub lighting: LightingMode,
    pub alpha: AlphaMode,
    /// Queue classification is independent of the draw's blend state.
    pub queue: MaterialQueue,
    pub color_key_reference: u8,
    pub two_sided: bool,
    pub front_face: FrontFace,
    /// Presentation-space world bounds center, used for view-depth ordering.
    pub sort_center: [f32; 3],
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightingMode {
    #[default]
    Unlit,
    /// HWTL vertex branch: MODULATE2X RGB, texture alpha.
    SavedVertex,
    /// Single-pass lightmap branch: ADD lightmap/diffuse, then MODULATE2X base.
    SavedTexture,
}
impl RenderMaterial {
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::Legacy => true,
            Self::Original(m) => {
                m.color
                    .into_iter()
                    .all(|v| v.is_finite() && (0. ..=1.).contains(&v))
                    && m.sort_center.into_iter().all(f32::is_finite)
            }
        }
    }
    pub(crate) fn parameters(self, target_srgb: bool) -> [f32; 8] {
        match self {
            Self::Legacy => [1., 1., 1., 1., -1., 0., 0., 0.],
            Self::Original(m) => {
                let cutoff = match m.alpha {
                    AlphaMode::Opaque => -1.,
                    AlphaMode::ColorKey => f32::from(m.color_key_reference) / 255.,
                    AlphaMode::Blend | AlphaMode::Additive => 1. / 255.,
                };
                [
                    m.color[0],
                    m.color[1],
                    m.color[2],
                    m.color[3],
                    cutoff,
                    match m.lighting {
                        LightingMode::Unlit => 0.,
                        LightingMode::SavedVertex => 1.,
                        LightingMode::SavedTexture => 2.,
                    },
                    if target_srgb { 1. } else { 0. },
                    0.,
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
