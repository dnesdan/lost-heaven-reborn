//! Host adapter from live native component draws to ordered screen meshes.
//!
//! Positions are already viewport pixels; this adapter neither fits a canvas nor
//! adjusts pixel centers or UVs. Native owners supply fonts and texture surfaces
//! at each reached draw. Only GPU-upload bytes are copied. Empty glyph draws
//! still resolve their font binding but contribute no raster primitive to Scene.
//! This is not GM_Menu::Draw, sound/menu-count lifecycle or a parity claim.

#![forbid(unsafe_code)]

use crate::itexture::{CpuTextureSurface, TextureKey, D3DFMT_A8R8G8B8};
use crate::menu::{
    MenuComponentDrawSink, MenuComponentPrimitive, MenuComponentPrimitiveTopology,
    MenuComponentText, MenuStack,
};
use crate::menu_draw_resources::MenuDrawResourceAccess;
use crate::menu_native_drawing::MenuNativeMenu;
use crate::menu_text_drawing::{
    advance_out_text_multi_line_y, out_text, MenuFont, MenuGlyphQuad, MenuTextDrawOp,
    MenuTextLayout, MenuTextPrecision, MENU_GLYPH_INDICES,
};
use lhr_render::{Mesh, RenderMaterial, Scene, Texture, Vertex};
use std::sync::Arc;

pub struct MenuSceneDraw {
    pub scene: Scene,
    pub result: Result<(), String>,
}

/// Retains the completed mesh prefix when native traversal or a draw cannot
/// resolve. The caller must explicitly select the text precision policy and
/// present this scene through the renderer's ordered screen path.
pub fn draw_menu_components(
    menu: &mut MenuStack,
    draw_flags: u32,
    progress_texture: Option<&TextureKey>,
    resources: &dyn MenuDrawResourceAccess,
    precision: MenuTextPrecision,
) -> MenuSceneDraw {
    let mut sink = MenuSceneSink {
        scene: Scene { meshes: Vec::new() },
        resources: Some(resources),
        precision,
        submit: None,
    };
    let result = menu.draw_components_native(draw_flags, progress_texture, &mut sink);
    MenuSceneDraw {
        scene: sink.scene,
        result,
    }
}

/// Draws the canonical aliased menu owner, releasing its component borrow
/// before each resource callback. Errors retain the completed mesh prefix.
/// Text precision remains an explicit caller-selected policy.
pub fn draw_menu_native_components(
    menu: &MenuNativeMenu,
    draw_flags: u32,
    progress_texture: Option<&TextureKey>,
    resources: &dyn MenuDrawResourceAccess,
    precision: MenuTextPrecision,
) -> MenuSceneDraw {
    draw_menu_native_components_with_progress_texture(
        menu,
        draw_flags,
        &|| Ok(progress_texture.copied()),
        resources,
        precision,
    )
}

pub(crate) fn draw_menu_native_components_with_progress_texture(
    menu: &MenuNativeMenu,
    draw_flags: u32,
    resolve_progress_texture: &dyn Fn() -> Result<Option<TextureKey>, String>,
    resources: &dyn MenuDrawResourceAccess,
    precision: MenuTextPrecision,
) -> MenuSceneDraw {
    let mut sink = MenuSceneSink {
        scene: Scene { meshes: Vec::new() },
        resources: Some(resources),
        precision,
        submit: None,
    };
    let result = menu.draw_components_native_with_progress_texture(
        draw_flags,
        resolve_progress_texture,
        &mut sink,
    );
    MenuSceneDraw {
        scene: sink.scene,
        result,
    }
}

pub(crate) fn submit_menu_native_components(
    menu: &MenuNativeMenu,
    draw_flags: u32,
    resolve_progress_texture: &dyn Fn() -> Result<Option<TextureKey>, String>,
    resources: Option<&dyn MenuDrawResourceAccess>,
    precision: MenuTextPrecision,
    submit: &mut dyn FnMut(Mesh) -> Result<(), String>,
) -> Result<(), String> {
    menu.draw_components_native_with_progress_texture(
        draw_flags,
        resolve_progress_texture,
        &mut MenuSceneSink {
            scene: Scene { meshes: Vec::new() },
            resources,
            precision,
            submit: Some(submit),
        },
    )
}

struct MenuSceneSink<'a> {
    scene: Scene,
    resources: Option<&'a dyn MenuDrawResourceAccess>,
    precision: MenuTextPrecision,
    submit: Option<&'a mut dyn FnMut(Mesh) -> Result<(), String>>,
}

impl<'a> MenuSceneSink<'a> {
    fn resources(&self) -> Result<&'a dyn MenuDrawResourceAccess, String> {
        self.resources
            .ok_or_else(|| "loading-menu draw resources unavailable".to_owned())
    }

    fn emit(&mut self, mesh: Mesh) -> Result<(), String> {
        if let Some(submit) = self.submit.as_mut() {
            return submit(mesh);
        }
        self.scene.meshes.push(mesh);
        Ok(())
    }

    fn texture(&self, key: Option<&TextureKey>) -> Result<Option<Arc<Texture>>, String> {
        key.map(|key| {
            let surface = self.resources()?.texture_surface(*key)?;
            texture_from_surface(surface).map(Arc::new)
        })
        .transpose()
    }

    fn glyphs(
        &mut self,
        quads: &[MenuGlyphQuad],
        font: u32,
        texture: Option<Arc<Texture>>,
    ) -> Result<(), String> {
        let Some(first) = quads.first() else {
            return Ok(());
        };
        let argb = first.vertices[0].color;
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for quad in quads {
            let base = u32::try_from(vertices.len())
                .map_err(|_| "UnsupportedMenuMeshSize: glyph vertices exceed u32".to_owned())?;
            for vertex in quad.vertices {
                vertices.push(menu_vertex(
                    vertex.position,
                    vertex.rhw,
                    vertex.uv,
                    vertex.color,
                    argb,
                )?);
            }
            for index in MENU_GLYPH_INDICES {
                indices.push(base.checked_add(u32::from(index)).ok_or_else(|| {
                    "UnsupportedMenuMeshSize: glyph index exceeds u32".to_owned()
                })?);
            }
        }
        u32::try_from(indices.len())
            .map_err(|_| "UnsupportedMenuMeshSize: glyph indices exceed u32".to_owned())?;
        let atlas = texture.ok_or_else(|| format!("missing atlas texture for menu font {font}"))?;
        self.emit(Mesh {
            vertices,
            indices,
            texture: Some(atlas),
            material: menu_material(argb),
        })
    }
}

impl MenuComponentDrawSink for MenuSceneSink<'_> {
    fn progress_primitive(&mut self, primitive: MenuComponentPrimitive<'_>) -> Result<(), String> {
        if primitive.texture.is_none() {
            return Err("missing loading progress texture: kulicky.tga".into());
        }
        self.primitive(primitive)
    }

    fn primitive(&mut self, primitive: MenuComponentPrimitive<'_>) -> Result<(), String> {
        if primitive.topology == MenuComponentPrimitiveTopology::LineStrip {
            return Err(
                "UnsupportedMenuLineStrip: faithful native line rasterization is not established"
                    .to_owned(),
            );
        }
        let indices = triangle_strip_indices(primitive.vertices.len())?;
        let argb = primitive.vertices[0].argb;
        let vertices = primitive
            .vertices
            .iter()
            .map(|vertex| {
                let [position_x, position_y, position_z, rhw] = vertex.position;
                menu_vertex(
                    [position_x, position_y, position_z],
                    rhw,
                    vertex.uv,
                    vertex.argb,
                    argb,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let texture = self.texture(primitive.texture)?;
        self.emit(Mesh {
            vertices,
            indices,
            texture,
            material: menu_material(argb),
        })
    }

    fn text(&mut self, text: MenuComponentText<'_>) -> Result<(), String> {
        let [position_x, position_y, size_x, size_y] = text.rect;
        let mut layout = MenuTextLayout {
            x: position_x,
            y: position_y,
            sx: size_x,
            sy: size_y,
            color: text.argb,
            flags: text.text_flags,
        };
        let mut start = 0;
        loop {
            let tail = &text.bytes[start..];
            let split = tail
                .iter()
                .copied()
                .take_while(|byte| *byte != 0)
                .position(|byte| byte == b'~');
            // OutTextMultiLine completes each OutText before the next font read
            // (g_indicators.cpp, fe7e746). Texture callbacks can change that read.
            let font = self.resources()?.font(text.font)?;
            let draw = out_text(
                Some(MenuFont {
                    index: text.font,
                    metrics: font.metrics,
                    translation: font.glyph_map,
                }),
                Some(tail),
                layout,
                split,
                self.precision,
            );
            let mut texture = None;
            for operation in draw.operations {
                match operation {
                    MenuTextDrawOp::BindFont { font_index } => {
                        if font_index != text.font {
                            return Err(
                                "UnsupportedMenuFontBinding: font changed within text draw".into(),
                            );
                        }
                        texture = self.texture(font.atlas.as_ref())?;
                    }
                    MenuTextDrawOp::GlyphQuads { quads } => {
                        self.glyphs(&quads, text.font, texture.clone())?
                    }
                    MenuTextDrawOp::ClearTexture => texture = None,
                    MenuTextDrawOp::Underline { .. } => {
                        return Err(
                            "UnsupportedMenuUnderline: faithful native line rasterization is not established"
                                .to_owned(),
                        );
                    }
                }
            }
            draw.result.map_err(|mut error| {
                error.byte_offset = error.byte_offset.map(|offset| start + offset);
                format!("MenuTextUnresolved: {error:?}")
            })?;
            // The native y store is reached after every completed line, even
            // the last one. Keep its numeric policy in the text owner.
            layout.y = advance_out_text_multi_line_y(layout.y, layout.sy)
                .map_err(|error| format!("MenuTextUnresolved: {error:?}"))?;
            let Some(split) = split else {
                return Ok(());
            };
            start += split + 1;
        }
    }
}

fn triangle_strip_indices(vertex_count: usize) -> Result<Vec<u32>, String> {
    if vertex_count < 3 {
        return Err("UnsupportedMenuTriangleStrip: fewer than three vertices".to_owned());
    }
    let count = u32::try_from(vertex_count)
        .map_err(|_| "UnsupportedMenuMeshSize: strip vertices exceed u32".to_owned())?;
    let index_count = (vertex_count - 2)
        .checked_mul(3)
        .and_then(|count| u32::try_from(count).ok())
        .ok_or_else(|| "UnsupportedMenuMeshSize: strip indices exceed u32".to_owned())?;
    let mut indices = Vec::with_capacity(index_count as usize);
    for index in 0..count - 2 {
        if index % 2 == 0 {
            indices.extend_from_slice(&[index, index + 1, index + 2]);
        } else {
            indices.extend_from_slice(&[index, index + 2, index + 1]);
        }
    }
    Ok(indices)
}

fn menu_vertex(
    position: [f32; 3],
    rhw: f32,
    uv: [f32; 2],
    argb: u32,
    batch_argb: u32,
) -> Result<Vertex, String> {
    if argb != batch_argb {
        return Err("UnsupportedMenuPerVertexColor: batch diffuse must be uniform".to_owned());
    }
    if rhw != 1.0 {
        return Err("UnsupportedMenuRhw: ordered mesh vertices require rhw=1".to_owned());
    }
    if position.iter().chain(&uv).any(|value| !value.is_finite()) {
        return Err("UnsupportedMenuVertex: nonfinite pixel position or UV".to_owned());
    }
    Ok(Vertex { position, uv })
}

fn menu_material(argb: u32) -> RenderMaterial {
    let [alpha, red, green, blue] = argb.to_be_bytes();
    RenderMaterial::OriginalNull {
        color: [red, green, blue, alpha].map(|channel| f32::from(channel) / 255.0),
        sort_center: [0.0; 3],
        visual_z_bias: false,
    }
}

fn texture_from_surface(surface: &CpuTextureSurface) -> Result<Texture, String> {
    if surface.format != D3DFMT_A8R8G8B8 {
        return Err(format!(
            "UnsupportedMenuTextureFormat: expected A8R8G8B8, got {}",
            surface.format
        ));
    }
    let width = u32::from(surface.width);
    let height = u32::from(surface.height);
    let expected = width as usize * height as usize * 4;
    if width == 0 || height == 0 || width > 8192 || height > 8192 || surface.bgra.len() != expected
    {
        return Err(
            "InvalidMenuTextureSurface: expected bounded, tightly packed BGRA pixels".into(),
        );
    }
    let mut rgba = Vec::with_capacity(expected);
    for pixel in surface.bgra.as_chunks::<4>().0 {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    Ok(Texture {
        width,
        height,
        rgba,
        repeat: [false; 2],
    })
}

#[cfg(test)]
#[path = "menu_rendering_tests.rs"]
mod tests;
