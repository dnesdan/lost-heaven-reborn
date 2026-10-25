//! Data-driven original menu layout and atlas text; no game scripts are executed here.
use lhr_assets::{FontMetrics, Installation, MenuDefinition, Texture as Image};
use lhr_render::{Mesh, Texture, Vertex};
use std::sync::Arc;

pub const DESIGN: [f32; 2] = [800.0, 600.0];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Intro,
    NewGame,
    Quit,
    Unsupported(u32),
}

pub struct MenuUi {
    definition: MenuDefinition,
    labels: Vec<String>,
    font: FontMetrics,
    title_font: FontMetrics,
    atlas: Image,
    title_atlas: Image,
    language: [u16; 256],
    encoding: Vec<(char, u8)>,
    header: Image,
    paper: Image,
    selected: usize,
}
impl MenuUi {
    pub fn load(installation: &Installation) -> Result<Self, String> {
        let read = |name: &str| {
            installation
                .read(name)
                .map_err(|e| format!("Menu resource {name}: {e}"))
        };
        let definition = lhr_assets::parse_menu_definition(&read("tables/menu/mainmenu.mnu")?)
            .map_err(|e| e.to_string())?;
        validate_layout(&definition)?;
        let text =
            lhr_assets::parse_text_db(&read("tables/textdb_cz.def")?).map_err(|e| e.to_string())?;
        let labels = definition
            .items
            .iter()
            .map(|item| {
                text.get(item.text_id)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("Menu text ID {} is absent", item.text_id))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if labels.iter().any(|label| label.chars().count() > 128)
            || labels
                .iter()
                .map(|label| label.chars().count())
                .sum::<usize>()
                > 1024
        {
            return Err("Menu labels exceed supported text quota".into());
        }
        let lang = read("tables/lang_cz.def")?;
        if lang.len() != 568 || &lang[..6] != b"Langcz" {
            return Err("Unsupported menu language table".into());
        }
        let language =
            std::array::from_fn(|i| u16::from_le_bytes([lang[56 + i * 2], lang[57 + i * 2]]));
        let encoding = (0..=255u8)
            .filter_map(|b| {
                lhr_assets::decode_windows1250(&[b])
                    .ok()
                    .and_then(|s| s.chars().next())
                    .map(|c| (c, b))
            })
            .collect();
        let decode = |name: &str| lhr_assets::decode_tga(&read(name)?).map_err(|e| e.to_string());
        let selected = definition
            .items
            .iter()
            .filter(|item| item.kind == *b"ttub")
            .position(|item| enabled(item.id))
            .unwrap_or(0);
        let ui = Self {
            definition,
            labels,
            language,
            encoding,
            selected,
            font: lhr_assets::parse_font_metrics(&read("tables/font1_lat.gmf")?)
                .map_err(|e| e.to_string())?,
            title_font: lhr_assets::parse_font_metrics(&read("tables/font2_lat.gmf")?)
                .map_err(|e| e.to_string())?,
            atlas: decode("maps/font1_lat.tga")?,
            title_atlas: decode("maps/font2_lat.tga")?,
            header: decode("maps/papir5a.tga")?,
            paper: decode("maps/papir3.tga")?,
        };
        for font in [&ui.font, &ui.title_font] {
            if !(0.1..=2.0).contains(&font.height_scale)
                || !(0.1..=2.0).contains(&font.width_scale)
                || !(0.001..=1.0).contains(&font.uv_height)
                || !(0.0..=1.0).contains(&font.tracking)
                || font.baseline.abs() > 2.0
                || font.glyphs.iter().any(|g| {
                    !(0.0..=2.0).contains(&g.width)
                        || !(0.0..=1.0).contains(&g.u_left)
                        || !(0.0..=1.0).contains(&g.u_right)
                        || !(0.0..=1.0).contains(&g.v_top)
                })
            {
                return Err("Menu font metrics exceed supported layout quota".into());
            }
        }
        for image in [&ui.atlas, &ui.title_atlas, &ui.header, &ui.paper] {
            if image.width > 2048 || image.height > 2048 {
                return Err("Menu artwork exceeds supported quota".into());
            }
        }
        Ok(ui)
    }
    fn rows(&self) -> impl Iterator<Item = usize> + '_ {
        self.definition
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.kind == *b"ttub")
            .map(|(i, _)| i)
    }
    fn selected_index(&self) -> usize {
        self.rows().nth(self.selected).unwrap_or(0)
    }
    pub fn focus_id(&self) -> u32 {
        self.definition.items[self.selected_index()].id
    }
    pub fn move_selection(&mut self, direction: i32) {
        let rows: Vec<_> = self.rows().collect();
        if rows.is_empty() {
            return;
        }
        if direction == 0 {
            return;
        }
        for _ in 0..rows.len() {
            self.selected =
                (self.selected as i32 + direction).rem_euclid(rows.len() as i32) as usize;
            if enabled(self.definition.items[rows[self.selected]].id) {
                break;
            }
        }
    }
    pub fn point(&mut self, position: [f32; 2]) -> bool {
        let root = self.definition.items[0].rect;
        let rows: Vec<_> = self.rows().collect();
        for (selection, index) in rows.into_iter().enumerate() {
            let item = &self.definition.items[index];
            let [x, y, w, h] = item.rect;
            if enabled(item.id)
                && position[0] >= root[0] + x
                && position[0] < root[0] + x + w
                && position[1] >= root[1] + 40.0 + y
                && position[1] < root[1] + 40.0 + y + h
            {
                let changed = self.selected != selection;
                self.selected = selection;
                return changed;
            }
        }
        false
    }
    pub fn hit(&self, position: [f32; 2]) -> bool {
        let root = self.definition.items[0].rect;
        self.rows().any(|index| {
            let item = &self.definition.items[index];
            let [x, y, w, h] = item.rect;
            enabled(item.id)
                && position[0] >= root[0] + x
                && position[0] < root[0] + x + w
                && position[1] >= root[1] + 40.0 + y
                && position[1] < root[1] + 40.0 + y + h
        })
    }
    pub fn activate(&self) -> Action {
        match self.focus_id() {
            990101 => Action::Intro,
            990103 => Action::NewGame,
            990113 => Action::Quit,
            id => Action::Unsupported(id),
        }
    }
    pub fn pixels(&self) -> Texture {
        let mut canvas = Canvas::new();
        let [x, y, w, h] = self.definition.items[0].rect;
        canvas.image(&self.header, [x, y, w, 40.0], [0.0, 0.0, 1.0, 1.0], None);
        canvas.image(
            &self.paper,
            [x, y + 39.0, w, h - 40.0],
            [0.0, 0.0, 1.0, 1.0],
            None,
        );
        self.text(
            &mut canvas,
            &self.labels[0],
            [x, y + 4.0, w, 32.0],
            true,
            [255; 3],
        );
        for index in self.rows() {
            let item = &self.definition.items[index];
            let [rx, ry, rw, rh] = item.rect;
            let rect = [x + rx, y + 40.0 + ry, rw, rh];
            let width = self.text(
                &mut canvas,
                &self.labels[index],
                rect,
                false,
                if !enabled(item.id) { [127; 3] } else { [0; 3] },
            );
            if index == self.selected_index() {
                canvas.rectangle(
                    [rect[0] + (rw - width) * 0.5, rect[1] + rh - 3.0, width, 1.5],
                    [160, 0, 0, 255],
                );
            }
        }
        Texture {
            width: 800,
            height: 600,
            rgba: canvas.rgba,
            repeat: [false; 2],
        }
    }
    fn text(
        &self,
        canvas: &mut Canvas,
        text: &str,
        rect: [f32; 4],
        title: bool,
        color: [u8; 3],
    ) -> f32 {
        let (font, atlas) = if title {
            (&self.title_font, &self.title_atlas)
        } else {
            (&self.font, &self.atlas)
        };
        let header = |p: usize| f32::from_le_bytes(font.raw_header[p..p + 4].try_into().unwrap());
        let h = rect[3] * header(8);
        let tracking = h * font.tracking;
        let glyphs: Vec<_> = text
            .chars()
            .map(|c| {
                self.encoding
                    .iter()
                    .find(|(v, _)| *v == c)
                    .map_or(0, |(_, b)| self.language[*b as usize])
            })
            .collect();
        let advance = |g: u16| {
            let width = font.glyphs[(g & 255) as usize].width;
            if width < 0.001 {
                h * font.width_scale * 0.3
            } else {
                h * font.width_scale * width - tracking
            }
        };
        let width = glyphs.iter().map(|g| advance(*g)).sum::<f32>() + tracking;
        let mut x = rect[0] + (rect[2] - width) * 0.5;
        for g in glyphs {
            let base = font.glyphs[(g & 255) as usize];
            let draw = |canvas: &mut Canvas, glyph: lhr_assets::FontGlyph, gx: f32| {
                if glyph.width >= 0.001 {
                    canvas.image(
                        atlas,
                        [
                            gx,
                            rect[1] + header(24) * h,
                            h * font.width_scale * glyph.width,
                            h,
                        ],
                        [
                            glyph.u_left,
                            glyph.v_top,
                            glyph.u_right,
                            glyph.v_top + header(16),
                        ],
                        Some(color),
                    );
                }
            };
            draw(canvas, base, x);
            let accent = ((g >> 8) & 0x7f) as usize;
            if accent != 0 {
                let mark = font.glyphs[accent];
                draw(
                    canvas,
                    mark,
                    x + h * font.width_scale * (base.width - mark.width) * 0.5,
                );
            }
            x += advance(g);
        }
        width
    }
}
fn enabled(id: u32) -> bool {
    matches!(id, 990103 | 990113)
}
fn validate_layout(definition: &MenuDefinition) -> Result<(), String> {
    if definition
        .items
        .first()
        .is_none_or(|item| item.kind != *b"dniw")
    {
        return Err("Main menu requires an original window root".into());
    }
    if definition.items.len() > 64
        || definition.items.iter().any(|item| {
            item.rect.iter().any(|v| !v.is_finite() || v.abs() > 1600.0)
                || item.rect[2] <= 0.0
                || item.rect[3] <= 0.0
                || item.rect[2] > 800.0
                || item.rect[3] > 600.0
        })
    {
        return Err("Menu layout exceeds supported canvas quota".into());
    }
    let mut ids = std::collections::HashSet::new();
    if definition.items.iter().any(|item| !ids.insert(item.id)) {
        return Err("Duplicate menu component ID".into());
    }
    if !definition
        .items
        .iter()
        .any(|item| item.kind == *b"ttub" && enabled(item.id))
    {
        return Err("Main menu has no supported selectable action".into());
    }
    Ok(())
}
pub fn quad(texture: Texture) -> Mesh {
    Mesh {
        material: Default::default(),
        vertices: vec![
            Vertex {
                position: [0.0, 0.0, 0.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                position: [800.0, 0.0, 0.0],
                uv: [1.0, 0.0],
            },
            Vertex {
                position: [800.0, 600.0, 0.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                position: [0.0, 600.0, 0.0],
                uv: [0.0, 1.0],
            },
        ],
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: Some(Arc::new(texture)),
    }
}
pub fn design_position(position: [f32; 2], size: [u32; 2]) -> [f32; 2] {
    let scale = (size[0] as f32 / 800.0).min(size[1] as f32 / 600.0);
    [
        (position[0] - (size[0] as f32 - 800.0 * scale) * 0.5) / scale,
        (position[1] - (size[1] as f32 - 600.0 * scale) * 0.5) / scale,
    ]
}
struct Canvas {
    rgba: Vec<u8>,
}
impl Canvas {
    fn new() -> Self {
        Self {
            rgba: vec![0; 800 * 600 * 4],
        }
    }
    fn pixel(&mut self, x: usize, y: usize, p: [u8; 4]) {
        let dest = &mut self.rgba[(y * 800 + x) * 4..(y * 800 + x + 1) * 4];
        let alpha = u32::from(p[3]);
        let out = alpha + u32::from(dest[3]) * (255 - alpha) / 255;
        if out == 0 {
            return;
        }
        for c in 0..3 {
            dest[c] = ((u32::from(p[c]) * alpha
                + u32::from(dest[c]) * u32::from(dest[3]) * (255 - alpha) / 255)
                / out) as u8;
        }
        dest[3] = out as u8;
    }
    fn rectangle(&mut self, rect: [f32; 4], color: [u8; 4]) {
        for y in (rect[1].max(0.0) as usize)..((rect[1] + rect[3]).ceil().min(600.0) as usize) {
            for x in (rect[0].max(0.0) as usize)..((rect[0] + rect[2]).ceil().min(800.0) as usize) {
                self.pixel(x, y, color);
            }
        }
    }
    fn image(&mut self, image: &Image, rect: [f32; 4], uv: [f32; 4], tint: Option<[u8; 3]>) {
        for y in (rect[1].max(0.0) as usize)..((rect[1] + rect[3]).ceil().min(600.0) as usize) {
            let sy = ((uv[1] + (y as f32 - rect[1]) / rect[3] * (uv[3] - uv[1]))
                * image.height as f32)
                .clamp(0.0, image.height as f32 - 1.0) as usize;
            for x in (rect[0].max(0.0) as usize)..((rect[0] + rect[2]).ceil().min(800.0) as usize) {
                let sx = ((uv[0] + (x as f32 - rect[0]) / rect[2] * (uv[2] - uv[0]))
                    * image.width as f32)
                    .clamp(0.0, image.width as f32 - 1.0) as usize;
                let p = &image.rgba[(sy * image.width as usize + sx) * 4
                    ..(sy * image.width as usize + sx + 1) * 4];
                let rgb = tint.unwrap_or([p[0], p[1], p[2]]);
                self.pixel(x, y, [rgb[0], rgb[1], rgb[2], p[3]]);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> MenuUi {
        let mut bytes = b"GFnt".to_vec();
        bytes.extend([0; 24 + 256 * 16]);
        let font = lhr_assets::parse_font_metrics(&bytes).unwrap();
        let image = || Image {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        };
        let mut items = vec![lhr_assets::MenuItem {
            id: 1,
            kind: *b"dniw",
            rect: [40.0, 40.0, 217.0, 353.0],
            text_id: 1,
            style: 0,
        }];
        for (i, id) in [990101, 990103, 990106, 990113].into_iter().enumerate() {
            items.push(lhr_assets::MenuItem {
                id,
                kind: *b"ttub",
                rect: [10.0, 12.0 + i as f32 * 25.0, 200.0, 30.0],
                text_id: 1,
                style: 0,
            });
        }
        MenuUi {
            definition: MenuDefinition {
                version: 1,
                reserved: 0,
                items,
            },
            labels: vec!["Fixture".into(); 5],
            font: font.clone(),
            title_font: font,
            atlas: image(),
            title_atlas: image(),
            language: [0; 256],
            encoding: vec![],
            header: image(),
            paper: image(),
            selected: 1,
        }
    }
    #[test]
    fn keyboard_and_pointer_skip_unimplemented_actions_and_dispatch_original_ids() {
        let mut ui = fixture();
        assert_eq!(ui.activate(), Action::NewGame);
        ui.move_selection(1);
        assert_eq!(ui.activate(), Action::Quit);
        ui.move_selection(0);
        assert_eq!(ui.activate(), Action::Quit);
        ui.move_selection(1);
        assert_eq!(ui.activate(), Action::NewGame);
        assert!(!ui.hit([60.0, 95.0]));
        assert!(!ui.point([60.0, 95.0]));
        assert!(!ui.hit([-10.0, -10.0]));
        assert!(ui.point([60.0, 175.0]));
        assert_eq!(ui.activate(), Action::Quit);
        ui.move_selection(-1);
        assert_eq!(ui.activate(), Action::NewGame);
    }
    #[test]
    fn unsupported_empty_duplicate_and_all_disabled_layouts_are_rejected() {
        let definition = MenuDefinition {
            version: 1,
            reserved: 0,
            items: vec![],
        };
        assert!(validate_layout(&definition).is_err());
        let root = lhr_assets::MenuItem {
            id: 1,
            kind: *b"dniw",
            rect: [40.0, 40.0, 217.0, 353.0],
            text_id: 101,
            style: 0,
        };
        let mut definition = MenuDefinition {
            version: 1,
            reserved: 0,
            items: vec![root.clone()],
        };
        assert!(validate_layout(&definition).is_err());
        definition.items.push(lhr_assets::MenuItem {
            id: 990106,
            kind: *b"ttub",
            ..root.clone()
        });
        assert!(validate_layout(&definition).is_err());
        definition.items[1].id = 990103;
        assert!(validate_layout(&definition).is_ok());
        definition.items.push(root);
        assert!(validate_layout(&definition).is_err());
    }
    #[test]
    fn pointer_coordinates_account_for_letterboxing() {
        assert_eq!(design_position([320.0, 240.0], [640, 480]), [400.0, 300.0]);
        assert_eq!(design_position([480.0, 320.0], [960, 640]), [400.0, 300.0]);
        assert!(design_position([0.0, 0.0], [960, 640])[0] < 0.0);
    }
    #[test]
    fn alpha_composition_preserves_transparent_canvas() {
        let mut canvas = Canvas::new();
        canvas.pixel(1, 1, [255, 0, 0, 128]);
        assert_eq!(&canvas.rgba[3204..3208], &[255, 0, 0, 128]);
        canvas.pixel(1, 1, [0, 0, 255, 255]);
        assert_eq!(&canvas.rgba[3204..3208], &[0, 0, 255, 255]);
    }
}
