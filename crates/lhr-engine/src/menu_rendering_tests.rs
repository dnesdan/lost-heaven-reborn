use super::*;
use crate::itexture::TextureResources;
use crate::menu::{MenuComponent, MenuComponentCreateContext, MenuComponentDrawVertex};
use crate::menu_draw_resources::{MenuComponentResources, MenuFontView};
use crate::menu_text_drawing::OUTTEXT_UNDERLINE;
use lhr_assets::{FontGlyph, FontMetrics, MenuDefinition, MenuItem};
use std::cell::{Cell, RefCell};

const PRECISION: MenuTextPrecision = MenuTextPrecision::Pc24Nearest;

#[derive(Debug, PartialEq, Eq)]
enum Access {
    Font(u32),
    Texture(TextureKey),
}

#[derive(Clone, Copy)]
enum FontSelection {
    Initial,
    Replacement,
    Unavailable,
}

struct ReplacementFont {
    key: TextureKey,
    surface: CpuTextureSurface,
    metrics: FontMetrics,
    glyph_map: [u16; 256],
}

struct Resources {
    textures: TextureResources,
    key: TextureKey,
    surface: CpuTextureSurface,
    metrics: FontMetrics,
    glyph_map: [u16; 256],
    atlas: bool,
    accesses: RefCell<Vec<Access>>,
    texture_reads: Cell<usize>,
    fail_texture_read: Option<usize>,
    font_selection: Cell<FontSelection>,
    select_font_on_first_texture: Option<FontSelection>,
    replacement_font: Option<ReplacementFont>,
}

impl Resources {
    fn new() -> Self {
        let mut textures = TextureResources::new().unwrap();
        let key = textures.create_texture().unwrap();
        Self {
            textures,
            key,
            surface: CpuTextureSurface {
                width: 1,
                height: 1,
                format: D3DFMT_A8R8G8B8,
                bgra: vec![0x12, 0x34, 0x56, 0x78],
            },
            metrics: FontMetrics {
                raw_header: [0; 28],
                height_scale: 1.0,
                width_scale: 1.0,
                uv_height: 0.125,
                tracking: 0.125,
                baseline: 0.25,
                glyphs: vec![
                    FontGlyph {
                        u_left: 0.25,
                        v_top: 0.375,
                        u_right: 0.5,
                        width: 0.5,
                    };
                    256
                ],
            },
            glyph_map: std::array::from_fn(|index| index as u16),
            atlas: true,
            accesses: RefCell::new(Vec::new()),
            texture_reads: Cell::new(0),
            fail_texture_read: None,
            font_selection: Cell::new(FontSelection::Initial),
            select_font_on_first_texture: None,
            replacement_font: None,
        }
    }

    fn install_replacement_font(&mut self) -> TextureKey {
        let key = self.textures.create_texture().unwrap();
        let mut glyphs = vec![
            FontGlyph {
                u_left: 0.0,
                v_top: 0.0,
                u_right: 0.125,
                width: 0.5,
            };
            256
        ];
        glyphs[usize::from(b'B')] = FontGlyph {
            u_left: 0.625,
            v_top: 0.5,
            u_right: 0.875,
            width: 0.75,
        };
        let mut glyph_map = std::array::from_fn(|index| index as u16);
        glyph_map[usize::from(b'A')] = u16::from(b'B');
        self.replacement_font = Some(ReplacementFont {
            key,
            surface: CpuTextureSurface {
                width: 1,
                height: 1,
                format: D3DFMT_A8R8G8B8,
                bgra: vec![0x9a, 0xbc, 0xde, 0xf0],
            },
            metrics: FontMetrics {
                raw_header: [0; 28],
                height_scale: 0.5,
                width_scale: 2.0,
                uv_height: 0.25,
                tracking: 0.25,
                baseline: 0.5,
                glyphs,
            },
            glyph_map,
        });
        key
    }

    fn sink(&self, precision: MenuTextPrecision) -> MenuSceneSink<'_> {
        MenuSceneSink {
            scene: Scene { meshes: Vec::new() },
            resources: Some(self),
            precision,
            submit: None,
        }
    }
}

impl MenuComponentResources for Resources {
    fn text(&mut self, text_id: u32) -> Result<Option<Arc<[u8]>>, String> {
        if text_id == 1 {
            Ok(Some(Arc::from(b"A".as_slice())))
        } else {
            Err("unexpected fixture text id".into())
        }
    }

    fn create_alpha_texture(&mut self, name: &[u8]) -> Result<Option<TextureKey>, String> {
        if name == b"fixture-menu.bmp" {
            Ok(Some(self.key))
        } else {
            Err("unexpected fixture texture name".into())
        }
    }

    fn release_texture(&mut self, _key: TextureKey) -> Result<(), String> {
        Err("unexpected texture release in draw-only fixture".into())
    }
}

impl MenuDrawResourceAccess for Resources {
    fn font(&self, font: u32) -> Result<MenuFontView<'_>, String> {
        self.accesses.borrow_mut().push(Access::Font(font));
        match self.font_selection.get() {
            FontSelection::Initial => Ok(MenuFontView {
                metrics: &self.metrics,
                glyph_map: &self.glyph_map,
                atlas: self.atlas.then_some(self.key),
            }),
            FontSelection::Replacement => {
                let replacement = self
                    .replacement_font
                    .as_ref()
                    .ok_or("replacement fixture font is not installed")?;
                Ok(MenuFontView {
                    metrics: &replacement.metrics,
                    glyph_map: &replacement.glyph_map,
                    atlas: Some(replacement.key),
                })
            }
            FontSelection::Unavailable => Err("reached font lookup failure".into()),
        }
    }

    fn texture_surface(&self, key: TextureKey) -> Result<&CpuTextureSurface, String> {
        self.accesses.borrow_mut().push(Access::Texture(key));
        self.textures
            .texture(key)
            .map_err(|error| format!("{error:?}"))?;
        let surface = if key == self.key {
            &self.surface
        } else {
            &self
                .replacement_font
                .as_ref()
                .filter(|font| font.key == key)
                .ok_or("fixture does not own this surface")?
                .surface
        };
        let read = self.texture_reads.get() + 1;
        self.texture_reads.set(read);
        if self.fail_texture_read == Some(read) {
            return Err("reached texture lookup failure".into());
        }
        if read == 1 {
            if let Some(selection) = self.select_font_on_first_texture {
                self.font_selection.set(selection);
            }
        }
        Ok(surface)
    }
}

struct ProgressCallbackResources<'a> {
    resources: &'a Resources,
    menu: MenuNativeMenu,
    observed_progress: RefCell<Vec<f32>>,
}

impl ProgressCallbackResources<'_> {
    fn observe_progress(&self) -> Result<(), String> {
        let value = self
            .menu
            .with_components(|components| {
                components
                    .progress_value(4)
                    .ok_or_else(|| "fixture progress component is unavailable".to_owned())
            })
            .map_err(|error| error.to_string())?;
        self.observed_progress.borrow_mut().push(value);
        Ok(())
    }
}

impl MenuComponentResources for ProgressCallbackResources<'_> {
    fn text(&mut self, _text_id: u32) -> Result<Option<Arc<[u8]>>, String> {
        Err("unexpected text lookup in draw-only callback fixture".into())
    }

    fn create_alpha_texture(&mut self, _name: &[u8]) -> Result<Option<TextureKey>, String> {
        Err("unexpected texture creation in draw-only callback fixture".into())
    }

    fn release_texture(&mut self, _key: TextureKey) -> Result<(), String> {
        Err("unexpected texture release in draw-only callback fixture".into())
    }
}

impl MenuDrawResourceAccess for ProgressCallbackResources<'_> {
    fn font(&self, font: u32) -> Result<MenuFontView<'_>, String> {
        self.resources.font(font)
    }

    fn texture_surface(&self, key: TextureKey) -> Result<&CpuTextureSurface, String> {
        // The fixture emits its textured rectangle, then the gray and red
        // progress draws. Reenter the same owner from the gray lookup.
        match self.resources.texture_reads.get() + 1 {
            2 => {
                self.observe_progress()?;
                self.menu
                    .with_components_mut(|components| components.set_float(4, 0.75))
                    .map_err(|error| error.to_string())?;
                self.observe_progress()?;
            }
            3 => self.observe_progress()?,
            _ => {}
        }
        self.resources.texture_surface(key)
    }
}

fn strip(argb: u32) -> [MenuComponentDrawVertex; 4] {
    [
        ([12.25, 30.5, 0.75, 1.0], [-0.125, 0.25]),
        ([42.75, 30.5, 0.75, 1.0], [1.125, 0.25]),
        ([12.25, 71.5, 0.75, 1.0], [-0.125, 1.25]),
        ([42.75, 71.5, 0.75, 1.0], [1.125, 1.25]),
    ]
    .map(|(position, uv)| MenuComponentDrawVertex { position, uv, argb })
}

fn primitive<'a>(
    component: &MenuComponent,
    vertices: &'a [MenuComponentDrawVertex],
    texture: Option<&'a TextureKey>,
) -> MenuComponentPrimitive<'a> {
    MenuComponentPrimitive {
        component: component.native_key().unwrap(),
        component_id: component.id,
        draw_flags: 0,
        topology: MenuComponentPrimitiveTopology::TriangleStrip,
        vertices,
        texture,
    }
}

fn text<'a>(component: &MenuComponent, bytes: &'a [u8], flags: u32) -> MenuComponentText<'a> {
    MenuComponentText {
        component: component.native_key().unwrap(),
        component_id: component.id,
        draw_flags: 0,
        bytes,
        rect: [12.25, 40.5, 120.0, 16.0],
        argb: 0x81234567,
        text_flags: flags,
        font: 0,
    }
}

fn packet_components() -> MenuStack {
    let mut components = MenuStack::new();
    components
        .create_components_native(
            &MenuDefinition {
                version: 1,
                reserved: 0,
                items: vec![native_item(5, *b"cniw", [0.0, 0.0, 1.0, 1.0], 0)],
            },
            MenuComponentCreateContext {
                screen_scale: [1.0, 1.0],
                left_margin_x: 0.0,
                precision: crate::menu::MenuComponentPrecision::Pc24Nearest,
            },
            &mut Resources::new(),
        )
        .unwrap();
    components
}

fn color(mesh: &Mesh) -> [f32; 4] {
    let RenderMaterial::OriginalNull {
        color,
        sort_center,
        visual_z_bias,
    } = mesh.material
    else {
        panic!("menu batch does not have uniform OriginalNull diffuse");
    };
    assert_eq!(sort_center, [0.0; 3]);
    assert!(!visual_z_bias);
    color
}

#[test]
fn strip_triangle_sequence_preserves_winding_without_sorting() {
    assert_eq!(triangle_strip_indices(3).unwrap(), [0, 1, 2]);
    assert_eq!(triangle_strip_indices(4).unwrap(), [0, 1, 2, 1, 3, 2]);
    assert_eq!(
        triangle_strip_indices(6).unwrap(),
        [0, 1, 2, 1, 3, 2, 2, 3, 4, 3, 5, 4]
    );
    assert!(triangle_strip_indices(2).is_err());
    assert!(triangle_strip_indices(u32::MAX as usize).is_err());
}

#[test]
fn primitive_preserves_pixel_and_uv_bits_and_rgba_diffuse() {
    let resources = Resources::new();
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut vertices = strip(0x81234567);
    vertices[0].position[0] = -0.0;
    let mut sink = resources.sink(PRECISION);
    sink.primitive(primitive(component, &vertices, None))
        .unwrap();
    let mesh = &sink.scene.meshes[0];
    for (actual, expected) in mesh.vertices.iter().zip(vertices) {
        assert_eq!(
            actual.position.map(f32::to_bits),
            [
                expected.position[0],
                expected.position[1],
                expected.position[2]
            ]
            .map(f32::to_bits)
        );
        assert_eq!(actual.uv.map(f32::to_bits), expected.uv.map(f32::to_bits));
    }
    assert_eq!(
        color(mesh),
        [0x23, 0x45, 0x67, 0x81].map(|value| value as f32 / 255.0)
    );
    assert!(mesh.texture.is_none());
    assert!(resources.accesses.borrow().is_empty());
}

#[test]
fn every_batch_stays_in_supplied_order_despite_alpha_and_depth() {
    let resources = Resources::new();
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut sink = resources.sink(PRECISION);
    for (argb, depth) in [(0x20ff0000, 0.8), (0xff00ff00, 0.2), (0x800000ff, 0.5)] {
        let mut vertices = strip(argb);
        for vertex in &mut vertices {
            vertex.position[2] = depth;
        }
        sink.primitive(primitive(component, &vertices, None))
            .unwrap();
    }
    assert_eq!(sink.scene.meshes.len(), 3);
    assert_eq!(color(&sink.scene.meshes[0]), [1.0, 0.0, 0.0, 32.0 / 255.0]);
    assert_eq!(color(&sink.scene.meshes[1]), [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(color(&sink.scene.meshes[2]), [0.0, 0.0, 1.0, 128.0 / 255.0]);
    assert_eq!(sink.scene.meshes[0].vertices[0].position[2], 0.8);
}

#[test]
fn varying_vertex_color_stops_without_replacing_the_completed_prefix() {
    let resources = Resources::new();
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut vertices = strip(0xff123456);
    let mut sink = resources.sink(PRECISION);
    sink.primitive(primitive(component, &vertices, None))
        .unwrap();
    vertices[3].argb ^= 0x01000000;
    let error = sink
        .primitive(primitive(component, &vertices, Some(&resources.key)))
        .unwrap_err();
    assert!(error.starts_with("UnsupportedMenuPerVertexColor:"));
    assert_eq!(sink.scene.meshes.len(), 1);
    assert!(resources.accesses.borrow().is_empty());
}

#[test]
fn line_strip_stops_without_inventing_a_quad() {
    let resources = Resources::new();
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let vertices = strip(0xff123456);
    let mut sink = resources.sink(PRECISION);
    sink.primitive(primitive(component, &vertices, None))
        .unwrap();
    let mut line = primitive(component, &vertices[..2], None);
    line.topology = MenuComponentPrimitiveTopology::LineStrip;
    assert!(sink
        .primitive(line)
        .unwrap_err()
        .starts_with("UnsupportedMenuLineStrip:"));
    assert_eq!(sink.scene.meshes.len(), 1);
}

#[test]
fn unsupported_vertex_domains_do_not_append_partial_batches() {
    let resources = Resources::new();
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    for invalid in [f32::NAN, f32::INFINITY, 0.0, 0.5] {
        let mut vertices = strip(0xff123456);
        vertices[3].position[3] = invalid;
        let mut sink = resources.sink(PRECISION);
        assert!(sink
            .primitive(primitive(component, &vertices, None))
            .unwrap_err()
            .starts_with("UnsupportedMenuRhw:"));
        assert!(sink.scene.meshes.is_empty());
    }
    let mut vertices = strip(0xff123456);
    vertices[3].uv[1] = f32::NAN;
    let mut sink = resources.sink(PRECISION);
    assert!(sink
        .primitive(primitive(component, &vertices, None))
        .is_err());
    assert!(sink.scene.meshes.is_empty());
    assert!(sink.primitive(primitive(component, &[], None)).is_err());
}

#[test]
fn borrowed_bgra_surface_is_converted_once_at_upload_without_alpha_changes() {
    let surface = CpuTextureSurface {
        width: 2,
        height: 1,
        format: D3DFMT_A8R8G8B8,
        bgra: vec![1, 2, 3, 4, 0xff, 0x80, 0x40, 0],
    };
    let texture = texture_from_surface(&surface).unwrap();
    assert_eq!([texture.width, texture.height], [2, 1]);
    assert_eq!(texture.rgba, [3, 2, 1, 4, 0x40, 0x80, 0xff, 0]);
    assert_eq!(surface.bgra, [1, 2, 3, 4, 0xff, 0x80, 0x40, 0]);
    assert_eq!(texture.repeat, [false; 2]);
}

#[test]
fn unsupported_formats_and_incomplete_surfaces_stop_explicitly() {
    for (width, height, format, bytes) in [
        (0, 1, 21, 0),
        (1, 0, 21, 0),
        (8193, 1, 21, 0),
        (1, 1, 21, 3),
        (1, 1, 21, 5),
        (1, 1, 22, 4),
    ] {
        let surface = CpuTextureSurface {
            width,
            height,
            format,
            bgra: vec![0; bytes],
        };
        assert!(texture_from_surface(&surface).is_err());
    }
}

#[test]
fn texture_owner_is_read_again_and_later_error_preserves_mesh_prefix() {
    let mut resources = Resources::new();
    resources.fail_texture_read = Some(2);
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let vertices = strip(0xff123456);
    let mut sink = resources.sink(PRECISION);
    sink.primitive(primitive(component, &vertices, Some(&resources.key)))
        .unwrap();
    assert_eq!(
        sink.primitive(primitive(component, &vertices, Some(&resources.key)))
            .unwrap_err(),
        "reached texture lookup failure"
    );
    assert_eq!(sink.scene.meshes.len(), 1);
    assert_eq!(
        sink.scene.meshes[0].texture.as_ref().unwrap().rgba,
        [0x56, 0x34, 0x12, 0x78]
    );
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Texture(resources.key),
            Access::Texture(resources.key)
        ]
    );
}

#[test]
fn text_uses_shared_glyph_indices_and_native_pixel_layout_in_line_order() {
    let resources = Resources::new();
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut sink = resources.sink(PRECISION);
    sink.text(text(component, b"AB~A", 0)).unwrap();
    assert_eq!(sink.scene.meshes.len(), 2);
    assert_eq!(
        sink.scene.meshes[0].indices,
        [0, 1, 2, 1, 3, 2, 4, 5, 6, 5, 7, 6]
    );
    assert_eq!(
        sink.scene.meshes[1].indices,
        MENU_GLYPH_INDICES.map(u32::from)
    );
    assert_eq!(
        sink.scene.meshes[0].vertices[0].position,
        [12.25, 36.5, 0.0]
    );
    assert_eq!(
        sink.scene.meshes[1].vertices[0].position,
        [12.25, 52.5, 0.0]
    );
    assert_eq!(sink.scene.meshes[0].vertices[0].uv, [0.25, 0.375]);
    assert_eq!(
        color(&sink.scene.meshes[0]),
        [0x23, 0x45, 0x67, 0x81].map(|value| value as f32 / 255.0)
    );
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Font(0),
            Access::Texture(resources.key),
            Access::Font(0),
            Access::Texture(resources.key)
        ]
    );
}

#[test]
fn texture_callback_changes_the_next_lines_font_metrics_map_and_atlas() {
    let mut resources = Resources::new();
    let replacement_key = resources.install_replacement_font();
    resources.select_font_on_first_texture = Some(FontSelection::Replacement);
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut sink = resources.sink(PRECISION);
    sink.text(text(component, b"A~AA", 0)).unwrap();
    assert_eq!(sink.scene.meshes.len(), 2);
    let first = &sink.scene.meshes[0];
    assert_eq!(first.vertices[0].position, [12.25, 36.5, 0.0]);
    assert_eq!(first.vertices[0].uv, [0.25, 0.375]);
    assert_eq!(
        first.texture.as_ref().unwrap().rgba,
        [0x56, 0x34, 0x12, 0x78]
    );
    let second = &sink.scene.meshes[1];
    assert_eq!(second.vertices.len(), 8);
    assert_eq!(second.vertices[0].position, [12.25, 48.5, 0.0]);
    assert_eq!(second.vertices[1].position, [24.25, 48.5, 0.0]);
    assert_eq!(second.vertices[2].position, [12.25, 56.5, 0.0]);
    assert_eq!(second.vertices[4].position, [20.25, 48.5, 0.0]);
    assert_eq!(second.vertices[0].uv, [0.625, 0.5]);
    assert_eq!(second.vertices[3].uv, [0.875, 0.75]);
    assert_eq!(
        second.texture.as_ref().unwrap().rgba,
        [0xde, 0xbc, 0x9a, 0xf0]
    );
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Font(0),
            Access::Texture(resources.key),
            Access::Font(0),
            Access::Texture(replacement_key)
        ]
    );
}

#[test]
fn texture_callback_can_make_the_next_font_lookup_fail_after_the_first_mesh() {
    let mut resources = Resources::new();
    resources.select_font_on_first_texture = Some(FontSelection::Unavailable);
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut sink = resources.sink(PRECISION);
    assert_eq!(
        sink.text(text(component, b"A~B~A", 0)),
        Err("reached font lookup failure".to_owned())
    );
    assert_eq!(sink.scene.meshes.len(), 1);
    assert_eq!(
        sink.scene.meshes[0].indices,
        MENU_GLYPH_INDICES.map(u32::from)
    );
    assert_eq!(
        sink.scene.meshes[0].vertices[0].position,
        [12.25, 36.5, 0.0]
    );
    assert_eq!(
        sink.scene.meshes[0].texture.as_ref().unwrap().rgba,
        [0x56, 0x34, 0x12, 0x78]
    );
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Font(0),
            Access::Texture(resources.key),
            Access::Font(0)
        ]
    );
}

#[test]
fn later_line_texture_error_keeps_the_first_mesh_and_stops_later_font_lookups() {
    let mut resources = Resources::new();
    resources.fail_texture_read = Some(2);
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut sink = resources.sink(PRECISION);
    assert_eq!(
        sink.text(text(component, b"A~B~A", 0)),
        Err("reached texture lookup failure".to_owned())
    );
    assert_eq!(sink.scene.meshes.len(), 1);
    assert_eq!(
        sink.scene.meshes[0].vertices[0].position,
        [12.25, 36.5, 0.0]
    );
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Font(0),
            Access::Texture(resources.key),
            Access::Font(0),
            Access::Texture(resources.key)
        ]
    );
}

#[test]
fn multiline_preserves_raw_bytes_and_newline_and_stops_before_tildes_after_nul() {
    let mut resources = Resources::new();
    resources.metrics.glyphs[usize::from(b'\n')].u_left = 0.0625;
    resources.metrics.glyphs[0x80].u_left = 0.125;
    resources.metrics.glyphs[0xff].u_left = 0.875;
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut sink = resources.sink(PRECISION);
    sink.text(text(component, b"A\n\x80~\xff\0~B", 0)).unwrap();
    assert_eq!(sink.scene.meshes.len(), 2);
    assert_eq!(sink.scene.meshes[0].vertices.len(), 12);
    assert_eq!(sink.scene.meshes[0].vertices[4].uv, [0.0625, 0.375]);
    assert_eq!(sink.scene.meshes[0].vertices[8].uv, [0.125, 0.375]);
    assert_eq!(sink.scene.meshes[1].vertices.len(), 4);
    assert_eq!(sink.scene.meshes[1].vertices[0].uv, [0.875, 0.375]);
    assert_eq!(
        sink.scene.meshes[1].vertices[0].position,
        [12.25, 52.5, 0.0]
    );
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Font(0),
            Access::Texture(resources.key),
            Access::Font(0),
            Access::Texture(resources.key)
        ]
    );
}

#[test]
fn incremental_lines_keep_the_100_quad_cap_including_accents() {
    let mut resources = Resources::new();
    resources.glyph_map[usize::from(b'A')] = (u16::from(b'C') << 8) | u16::from(b'B');
    let mut bytes = vec![b'A'; 51];
    bytes.push(b'~');
    bytes.extend_from_slice(&[b'B'; 101]);
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut sink = resources.sink(PRECISION);
    sink.text(text(component, &bytes, 0)).unwrap();
    assert_eq!(sink.scene.meshes.len(), 2);
    for mesh in &sink.scene.meshes {
        assert_eq!(mesh.vertices.len(), 400);
        assert_eq!(mesh.indices.len(), 600);
        assert_eq!(&mesh.indices[594..], &[396, 397, 398, 397, 399, 398]);
    }
    assert_eq!(
        sink.scene.meshes[1].vertices[0].position,
        [12.25, 52.5, 0.0]
    );
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Font(0),
            Access::Texture(resources.key),
            Access::Font(0),
            Access::Texture(resources.key)
        ]
    );
}

#[test]
fn missing_font_atlas_stops_at_first_nonempty_glyph_batch() {
    let mut resources = Resources::new();
    resources.atlas = false;
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut sink = resources.sink(PRECISION);
    sink.text(text(component, b"", 0)).unwrap();
    assert_eq!(
        sink.text(text(component, b"A~B", 0)).unwrap_err(),
        "missing atlas texture for menu font 0"
    );
    assert!(sink.scene.meshes.is_empty());
    assert_eq!(
        *resources.accesses.borrow(),
        [Access::Font(0), Access::Font(0)]
    );
}

#[test]
fn unestablished_precision_is_not_guessed_and_retains_prior_meshes() {
    let resources = Resources::new();
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let vertices = strip(0xff123456);
    let mut sink = resources.sink(MenuTextPrecision::Unestablished);
    sink.primitive(primitive(component, &vertices, None))
        .unwrap();
    assert!(sink
        .text(text(component, b"A", 0))
        .unwrap_err()
        .contains("PrecisionUnestablished"));
    assert_eq!(sink.scene.meshes.len(), 1);
    assert_eq!(*resources.accesses.borrow(), [Access::Font(0)]);
}

#[test]
fn later_text_layout_error_keeps_already_drawn_line() {
    let mut resources = Resources::new();
    resources.metrics.glyphs.truncate(usize::from(b'B'));
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut sink = resources.sink(PRECISION);
    let error = sink.text(text(component, b"A~B", 0)).unwrap_err();
    assert!(error.contains("MissingGlyph(66)"));
    assert!(error.contains("byte_offset: Some(2)"));
    assert_eq!(sink.scene.meshes.len(), 1);
    assert_eq!(sink.scene.meshes[0].vertices.len(), 4);
    assert_eq!(resources.texture_reads.get(), 1);
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Font(0),
            Access::Texture(resources.key),
            Access::Font(0)
        ]
    );
}

#[test]
fn later_layout_error_after_an_empty_line_keeps_the_original_byte_offset() {
    let mut resources = Resources::new();
    resources.metrics.glyphs.truncate(usize::from(b'B'));
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut sink = resources.sink(PRECISION);
    assert_eq!(
        sink.text(text(component, b"AA~~AB~A", 0)),
        Err("MenuTextUnresolved: MenuTextUnresolved { reason: MissingGlyph(66), byte_offset: Some(5) }".to_owned())
    );
    assert_eq!(sink.scene.meshes.len(), 1);
    assert_eq!(sink.scene.meshes[0].indices.len(), 12);
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Font(0),
            Access::Texture(resources.key),
            Access::Font(0),
            Access::Texture(resources.key),
            Access::Font(0)
        ]
    );
}

#[test]
fn y_store_error_occurs_after_the_reached_line_even_when_it_is_empty_or_final() {
    let mut resources = Resources::new();
    resources.metrics.baseline = 0.0;
    resources.metrics.height_scale = 0.0;
    resources.metrics.tracking = 0.0;
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let vertices = strip(0xff123456);
    for (bytes, mesh_count) in [
        (b"A".as_slice(), 2),
        (b"".as_slice(), 1),
        (b"~A".as_slice(), 1),
    ] {
        resources.accesses.borrow_mut().clear();
        let mut sink = resources.sink(PRECISION);
        sink.primitive(primitive(component, &vertices, None))
            .unwrap();
        let mut operation = text(component, bytes, 0);
        operation.rect[1] = f32::MAX;
        operation.rect[3] = f32::MAX;
        assert_eq!(
            sink.text(operation),
            Err("MenuTextUnresolved: MenuTextUnresolved { reason: Binary32Overflow, byte_offset: None }".to_owned())
        );
        assert_eq!(sink.scene.meshes.len(), mesh_count);
        assert_eq!(
            sink.scene.meshes[0].vertices[0].position,
            [12.25, 30.5, 0.75]
        );
        assert!(sink
            .scene
            .meshes
            .iter()
            .all(|mesh| !mesh.indices.is_empty()));
        if mesh_count == 2 {
            assert_eq!(
                sink.scene.meshes[1].indices,
                MENU_GLYPH_INDICES.map(u32::from)
            );
            assert_eq!(
                sink.scene.meshes[1].vertices[0].position,
                [12.25, f32::MAX, 0.0]
            );
        }
        assert_eq!(
            *resources.accesses.borrow(),
            [Access::Font(0), Access::Texture(resources.key)]
        );
    }
}

#[test]
fn underline_stops_after_its_glyph_batch_before_later_lines() {
    let resources = Resources::new();
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut sink = resources.sink(PRECISION);
    assert!(sink
        .text(text(component, b"A~B", OUTTEXT_UNDERLINE))
        .unwrap_err()
        .starts_with("UnsupportedMenuUnderline:"));
    assert_eq!(sink.scene.meshes.len(), 1);
    assert_eq!(sink.scene.meshes[0].indices.len(), 6);
    assert_eq!(resources.texture_reads.get(), 1);
    assert_eq!(
        *resources.accesses.borrow(),
        [Access::Font(0), Access::Texture(resources.key)]
    );
}

#[test]
fn empty_glyph_draws_resolve_bindings_without_invalid_meshes() {
    let resources = Resources::new();
    let components = packet_components();
    let component = components.find_component_by_id(5).unwrap();
    let mut sink = resources.sink(PRECISION);
    sink.text(text(component, b"A~~B~", 0)).unwrap();
    assert_eq!(sink.scene.meshes.len(), 2);
    assert_eq!(
        sink.scene.meshes[0].vertices[0].position,
        [12.25, 36.5, 0.0]
    );
    assert_eq!(
        sink.scene.meshes[1].vertices[0].position,
        [12.25, 68.5, 0.0]
    );
    assert!(sink
        .scene
        .meshes
        .iter()
        .all(|mesh| !mesh.indices.is_empty()));
    assert_eq!(resources.texture_reads.get(), 4);
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Font(0),
            Access::Texture(resources.key),
            Access::Font(0),
            Access::Texture(resources.key),
            Access::Font(0),
            Access::Texture(resources.key),
            Access::Font(0),
            Access::Texture(resources.key)
        ]
    );
    resources.accesses.borrow_mut().clear();
    let mut sink = resources.sink(PRECISION);
    sink.text(text(component, b"", 0)).unwrap();
    assert!(sink.scene.meshes.is_empty());
    assert_eq!(resources.texture_reads.get(), 5);
    assert_eq!(
        *resources.accesses.borrow(),
        [Access::Font(0), Access::Texture(resources.key)]
    );
}

#[test]
fn public_draw_does_not_fall_back_to_uncomposed_menu() {
    let resources = Resources::new();
    let mut menu = MenuStack::new();
    let draw = draw_menu_components(&mut menu, 0, None, &resources, PRECISION);
    assert!(draw.result.is_err());
    assert!(draw.scene.meshes.is_empty());
    assert!(resources.accesses.borrow().is_empty());
}

fn native_item(id: u32, kind: [u8; 4], rect: [f32; 4], style: u32) -> MenuItem {
    MenuItem {
        id,
        kind,
        rect,
        text_id: if kind == *b"txts" { 1 } else { 0 },
        style: crate::main_menu::COMPONENT_VISIBLE | style,
    }
}

fn native_menu(resources: &mut Resources) -> MenuStack {
    let mut menu = MenuStack::new();
    menu.create_components_native(
        &MenuDefinition {
            version: 1,
            reserved: 0,
            items: vec![
                native_item(1, *b"cniw", [2.0, 3.0, 100.0, 100.0], 0),
                native_item(2, *b"tcer", [4.0, 5.0, 6.0, 7.0], 0x300),
                native_item(3, *b"ttcr", [1.0, 2.0, 8.0, 6.0], 0),
                native_item(4, *b"rabp", [10.0, 11.0, 20.0, 4.0], 0),
                native_item(5, *b"txts", [30.0, 40.0, 80.0, 10.0], 0x100),
            ],
        },
        MenuComponentCreateContext {
            screen_scale: [1.5, 2.0],
            left_margin_x: 5.0,
            precision: crate::menu::MenuComponentPrecision::Pc24Nearest,
        },
        resources,
    )
    .unwrap();
    menu.set_string_native(3, Some(b"fixture-menu.bmp"), resources)
        .unwrap();
    menu.set_float(4, 0.25).unwrap();
    menu
}

#[test]
fn public_draw_reads_live_component_tree_and_keeps_native_batch_order() {
    let mut resources = Resources::new();
    let mut menu = native_menu(&mut resources);
    let draw = draw_menu_components(
        &mut menu,
        u32::MAX,
        Some(&resources.key),
        &resources,
        PRECISION,
    );
    assert_eq!(draw.result, Ok(()));
    assert_eq!(draw.scene.meshes.len(), 5);
    assert_eq!(draw.scene.meshes[0].vertices[0].position, [14.0, 16.0, 0.0]);
    assert_eq!(draw.scene.meshes[0].vertices[3].position, [23.0, 30.0, 0.0]);
    assert_eq!(color(&draw.scene.meshes[0]), [1.0; 4]);
    assert!(draw.scene.meshes[0].texture.is_none());
    assert_eq!(draw.scene.meshes[1].vertices[0].position, [9.5, 10.0, 0.0]);
    assert_eq!(draw.scene.meshes[1].vertices[3].uv, [1.0, 1.0]);
    assert_eq!(draw.scene.meshes[2].vertices[3].position, [53.0, 36.0, 0.0]);
    assert_eq!(
        color(&draw.scene.meshes[2]),
        [127.0 / 255.0, 127.0 / 255.0, 127.0 / 255.0, 95.0 / 255.0]
    );
    assert_eq!(draw.scene.meshes[3].vertices[3].position, [30.5, 36.0, 0.0]);
    assert_eq!(draw.scene.meshes[3].vertices[3].uv, [0.25, 1.0]);
    assert_eq!(color(&draw.scene.meshes[3]), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(draw.scene.meshes[4].vertices[0].position, [53.0, 81.0, 0.0]);
    assert_eq!(
        draw.scene.meshes[4].indices,
        MENU_GLYPH_INDICES.map(u32::from)
    );
    for mesh in &draw.scene.meshes[1..] {
        assert_eq!(
            mesh.texture.as_ref().unwrap().rgba,
            [0x56, 0x34, 0x12, 0x78]
        );
    }
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Texture(resources.key),
            Access::Texture(resources.key),
            Access::Texture(resources.key),
            Access::Font(0),
            Access::Texture(resources.key)
        ]
    );
    menu.set_float(4, 0.5).unwrap();
    let next = draw_menu_components(
        &mut menu,
        u32::MAX,
        Some(&resources.key),
        &resources,
        PRECISION,
    );
    assert_eq!(next.result, Ok(()));
    assert_eq!(next.scene.meshes[3].vertices[3].position, [38.0, 36.0, 0.0]);
    assert_eq!(next.scene.meshes[3].vertices[3].uv, [0.5, 1.0]);
    assert_eq!(draw.scene.meshes[3].vertices[3].position, [30.5, 36.0, 0.0]);
}

#[test]
fn aliased_native_draw_reads_progress_after_the_gray_resource_callback() {
    let mut resources = Resources::new();
    let menu = MenuNativeMenu::new(native_menu(&mut resources), None, 0);
    let callback_resources = ProgressCallbackResources {
        resources: &resources,
        menu: menu.clone(),
        observed_progress: RefCell::new(Vec::new()),
    };
    let draw = draw_menu_native_components(
        &menu,
        u32::MAX,
        Some(&resources.key),
        &callback_resources,
        PRECISION,
    );
    assert_eq!(draw.result, Ok(()));
    assert_eq!(
        *callback_resources.observed_progress.borrow(),
        [0.25, 0.75, 0.75]
    );
    assert_eq!(
        menu.with_components(|components| Ok(components.progress_value(4)))
            .unwrap(),
        Some(0.75)
    );
    assert_eq!(draw.scene.meshes.len(), 5);
    assert_eq!(draw.scene.meshes[2].vertices[3].position, [53.0, 36.0, 0.0]);
    assert_eq!(draw.scene.meshes[2].vertices[3].uv, [1.0, 1.0]);
    assert_eq!(
        color(&draw.scene.meshes[2]),
        [127.0 / 255.0, 127.0 / 255.0, 127.0 / 255.0, 95.0 / 255.0]
    );
    assert_eq!(draw.scene.meshes[3].vertices[3].position, [45.5, 36.0, 0.0]);
    assert_eq!(draw.scene.meshes[3].vertices[3].uv, [0.75, 1.0]);
    assert_eq!(color(&draw.scene.meshes[3]), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(draw.scene.meshes[4].vertices[0].position, [53.0, 81.0, 0.0]);
    assert_eq!(resources.texture_reads.get(), 4);
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Texture(resources.key),
            Access::Texture(resources.key),
            Access::Texture(resources.key),
            Access::Font(0),
            Access::Texture(resources.key)
        ]
    );
}

#[test]
fn aliased_native_draw_late_error_keeps_gray_mesh_and_callback_mutation() {
    let mut resources = Resources::new();
    let menu = MenuNativeMenu::new(native_menu(&mut resources), None, 0);
    resources.fail_texture_read = Some(3);
    let callback_resources = ProgressCallbackResources {
        resources: &resources,
        menu: menu.clone(),
        observed_progress: RefCell::new(Vec::new()),
    };
    let draw = draw_menu_native_components(
        &menu,
        u32::MAX,
        Some(&resources.key),
        &callback_resources,
        PRECISION,
    );
    assert_eq!(draw.result, Err("reached texture lookup failure".into()));
    assert_eq!(
        *callback_resources.observed_progress.borrow(),
        [0.25, 0.75, 0.75]
    );
    assert_eq!(
        menu.with_components(|components| Ok(components.progress_value(4)))
            .unwrap(),
        Some(0.75)
    );
    assert_eq!(draw.scene.meshes.len(), 3);
    assert_eq!(draw.scene.meshes[0].vertices[0].position, [14.0, 16.0, 0.0]);
    assert_eq!(draw.scene.meshes[1].vertices[0].position, [9.5, 10.0, 0.0]);
    assert_eq!(draw.scene.meshes[2].vertices[3].position, [53.0, 36.0, 0.0]);
    assert_eq!(draw.scene.meshes[2].vertices[3].uv, [1.0, 1.0]);
    assert_eq!(
        color(&draw.scene.meshes[2]),
        [127.0 / 255.0, 127.0 / 255.0, 127.0 / 255.0, 95.0 / 255.0]
    );
    assert_eq!(resources.texture_reads.get(), 3);
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Texture(resources.key),
            Access::Texture(resources.key),
            Access::Texture(resources.key)
        ]
    );
}

#[test]
fn public_draw_texture_failure_keeps_the_native_progress_batch_prefix() {
    let mut resources = Resources::new();
    let mut menu = native_menu(&mut resources);
    resources.fail_texture_read = Some(3);
    let draw = draw_menu_components(
        &mut menu,
        u32::MAX,
        Some(&resources.key),
        &resources,
        PRECISION,
    );
    assert_eq!(draw.result, Err("reached texture lookup failure".into()));
    assert_eq!(draw.scene.meshes.len(), 3);
    assert_eq!(draw.scene.meshes[2].vertices[3].position, [53.0, 36.0, 0.0]);
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Texture(resources.key),
            Access::Texture(resources.key),
            Access::Texture(resources.key)
        ]
    );
    assert_eq!(
        menu.find_component_by_id(5)
            .unwrap()
            .native_geometry()
            .unwrap()
            .position,
        [53.0, 86.0]
    );
}

#[test]
fn public_draw_unestablished_text_precision_returns_only_completed_geometry() {
    let mut resources = Resources::new();
    let mut menu = native_menu(&mut resources);
    let draw = draw_menu_components(
        &mut menu,
        u32::MAX,
        Some(&resources.key),
        &resources,
        MenuTextPrecision::Unestablished,
    );
    assert!(draw.result.unwrap_err().contains("PrecisionUnestablished"));
    assert_eq!(draw.scene.meshes.len(), 4);
    assert_eq!(
        *resources.accesses.borrow(),
        [
            Access::Texture(resources.key),
            Access::Texture(resources.key),
            Access::Texture(resources.key),
            Access::Font(0)
        ]
    );
}

#[test]
fn public_draw_outline_boundary_keeps_the_previous_native_rectangle() {
    let mut resources = Resources::new();
    let mut menu = MenuStack::new();
    menu.create_components_native(
        &MenuDefinition {
            version: 1,
            reserved: 0,
            items: vec![
                native_item(1, *b"cniw", [1.0, 2.0, 40.0, 50.0], 0),
                native_item(2, *b"tcer", [3.0, 4.0, 10.0, 12.0], 0x300),
                native_item(3, *b"tcer", [5.0, 6.0, 14.0, 16.0], 0),
            ],
        },
        MenuComponentCreateContext {
            screen_scale: [1.0, 1.0],
            left_margin_x: 0.0,
            precision: crate::menu::MenuComponentPrecision::Pc24Nearest,
        },
        &mut resources,
    )
    .unwrap();
    let draw = draw_menu_components(
        &mut menu,
        u32::MAX,
        None,
        &resources,
        MenuTextPrecision::Unestablished,
    );
    assert!(draw
        .result
        .unwrap_err()
        .starts_with("UnsupportedMenuLineStrip:"));
    assert_eq!(draw.scene.meshes.len(), 1);
    assert_eq!(draw.scene.meshes[0].vertices[0].position, [4.0, 6.0, 0.0]);
    assert_eq!(draw.scene.meshes[0].indices, [0, 1, 2, 1, 3, 2]);
    assert!(resources.accesses.borrow().is_empty());
}
