//! Borrowed-font reconstruction of the PC indicator text drawing boundary.
//!
//! Static source correspondence: `game/ui/g_indicators.cpp`, reference fe7e746.
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27:
//! TextSize RVA 0x2036d0, OutText RVA 0x203880, OutTextMultiLine RVA 0x204700.
//! REA evidence: ev_ff61eaa9824a94fa056557495aadaa94da3b9b1c60fdfe6a82cc9e4efc9ab3ce,
//! ev_db5122d3f8fb5eac262fa423e7dd0b7c6cc69f2cde49367db79badf46c709b22,
//! ev_b665c5a566d728db23d3edd8e6fc9a2e191058ec731855b63be36a3350d9caf1.
//!
//! Coordinates are native screen units after caller scaling; UVs are normalized.
//! Font indices identify the owner's existing atlas, not a second resource owner.
//! Text is unsigned input bytes, never a host-font or Unicode layout. Slice end is
//! an implicit NUL; `None` is an actual native null, not a missing text resource.
//!
//! Arithmetic is conditional on finite PC24/round-to-nearest-even inputs. These
//! functions do not establish their incoming x87 control word. In particular,
//! the loading-menu caller's mode has not been validated against the original
//! runtime. Unknown modes, nonfinite values and unsupported stores stay explicit;
//! FPU status flags/exceptions and GPU/runtime equivalence are not implemented.

#![forbid(unsafe_code)]

use lhr_assets::{FontGlyph, FontMetrics};

pub const OUTTEXT_UNDERLINE: u32 = 0x01;
pub const OUTTEXT_CENTER: u32 = 0x02;
pub const OUTTEXT_RIGHT: u32 = 0x04;
pub const OUTTEXT_INDENT: u32 = 0x08;
pub const OUTTEXT_FIT: u32 = 0x10;

/// The native buffer holds 100 quads, including accents, not 100 input bytes.
pub const OUTTEXT_MAX_CHARS: usize = 100;

/// TL/TR/BL/BR quad indices, from Game.exe VA 0x638cc0 (RVA 0x238cc0),
/// REA ev_7b195d4b3fc45fa8ecddfa05eed9fe6cfa0ab92ff2640a132fa15b91e131a094.
pub const MENU_GLYPH_INDICES: [u16; 6] = [0, 1, 2, 1, 3, 2];

#[derive(Debug, Clone, Copy)]
pub struct MenuFont<'a> {
    pub index: u32,
    pub metrics: &'a FontMetrics,
    pub translation: &'a [u16; 256],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuTextLayout {
    pub x: f32,
    pub y: f32,
    pub sx: f32,
    pub sy: f32,
    pub color: u32,
    pub flags: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuTextPrecision {
    /// A caller-supplied condition, not an established loading-menu default.
    Pc24Nearest,
    Unestablished,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuTextVertex {
    pub position: [f32; 3],
    pub rhw: f32,
    pub color: u32,
    pub uv: [f32; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuGlyphQuad {
    pub glyph: u8,
    pub vertices: [MenuTextVertex; 4],
}

/// Ordered draw effects. Empty native text still binds and draws an empty batch.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuTextDrawOp {
    BindFont { font_index: u32 },
    GlyphQuads { quads: Vec<MenuGlyphQuad> },
    ClearTexture,
    Underline { vertices: [MenuTextVertex; 2] },
}

/// Operations already reached are retained even if a later line cannot resolve.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuTextResult<T> {
    pub operations: Vec<MenuTextDrawOp>,
    pub result: Result<T, MenuTextUnresolved>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuTextUnresolved {
    pub reason: MenuTextUnresolvedReason,
    /// Input-byte offset when the failure belongs to a particular character.
    pub byte_offset: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuTextUnresolvedReason {
    MissingFont,
    PrecisionUnestablished,
    MissingGlyph(u8),
    NonFiniteValue,
    ArithmeticDomain,
    Binary32Overflow,
}

impl MenuTextUnresolved {
    fn new(reason: MenuTextUnresolvedReason) -> Self {
        Self {
            reason,
            byte_offset: None,
        }
    }

    fn at(mut self, offset: usize) -> Self {
        self.byte_offset = Some(offset);
        self
    }

    fn after(mut self, prefix: usize) -> Self {
        self.byte_offset = self.byte_offset.map(|offset| prefix + offset);
        self
    }
}

/// Retains the native return value until its caller actually performs a store.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuTextWidth(f64);

impl MenuTextWidth {
    pub fn to_f32(self) -> Result<f32, MenuTextUnresolved> {
        store(self.0)
    }
}

/// `stop` is an exclusive byte offset, independent of NUL/slice termination.
pub fn text_size(
    font: Option<MenuFont<'_>>,
    text: Option<&[u8]>,
    size: f32,
    stop: Option<usize>,
    precision: MenuTextPrecision,
) -> Result<MenuTextWidth, MenuTextUnresolved> {
    let Some(text) = text else {
        return Ok(MenuTextWidth(0.0));
    };
    measure(require_font(font, precision)?, text, size, stop)
}

pub fn out_text(
    font: Option<MenuFont<'_>>,
    text: Option<&[u8]>,
    layout: MenuTextLayout,
    stop: Option<usize>,
    precision: MenuTextPrecision,
) -> MenuTextResult<f32> {
    let mut operations = Vec::new();
    let result = out_text_into(&mut operations, font, text, layout, stop, precision);
    MenuTextResult { operations, result }
}

/// Splits only at `~`, including a trailing empty line. This is not word wrap.
pub fn out_text_multi_line(
    font: Option<MenuFont<'_>>,
    text: Option<&[u8]>,
    mut layout: MenuTextLayout,
    precision: MenuTextPrecision,
) -> MenuTextResult<()> {
    let mut operations = Vec::new();
    let result = (|| {
        let Some(text) = text else {
            return Ok(());
        };
        let mut start = 0;
        loop {
            let tail = &text[start..];
            let split = tail
                .iter()
                .copied()
                .take_while(|byte| *byte != 0)
                .position(|byte| byte == b'~');
            out_text_into(&mut operations, font, Some(tail), layout, split, precision)
                .map_err(|error| error.after(start))?;
            layout.y = advance_out_text_multi_line_y(layout.y, layout.sy)?;
            let Some(split) = split else {
                return Ok(());
            };
            start += split + 1;
        }
    })();
    MenuTextResult { operations, result }
}

pub fn advance_out_text_multi_line_y(y: f32, sy: f32) -> Result<f32, MenuTextUnresolved> {
    store(add(load(y)?, load(sy)?)?)
}

fn require_font(
    font: Option<MenuFont<'_>>,
    precision: MenuTextPrecision,
) -> Result<MenuFont<'_>, MenuTextUnresolved> {
    let font =
        font.ok_or_else(|| MenuTextUnresolved::new(MenuTextUnresolvedReason::MissingFont))?;
    if precision != MenuTextPrecision::Pc24Nearest {
        return Err(MenuTextUnresolved::new(
            MenuTextUnresolvedReason::PrecisionUnestablished,
        ));
    }
    Ok(font)
}

fn glyph(font: MenuFont<'_>, index: u8) -> Result<&FontGlyph, MenuTextUnresolved> {
    font.metrics
        .glyphs
        .get(usize::from(index))
        .ok_or_else(|| MenuTextUnresolved::new(MenuTextUnresolvedReason::MissingGlyph(index)))
}

fn measure(
    font: MenuFont<'_>,
    text: &[u8],
    size: f32,
    stop: Option<usize>,
) -> Result<MenuTextWidth, MenuTextUnresolved> {
    let spacing = mul(load(size)?, load(font.metrics.tracking)?)?;
    let size = store(mul(load(size)?, load(font.metrics.width_scale)?)?)?;
    let mut width = 0.0;
    for (offset, byte) in text.iter().copied().enumerate() {
        if byte == 0 || stop.is_some_and(|stop| offset >= stop) {
            break;
        }
        let next = (|| {
            let index = font.translation[usize::from(byte)] as u8;
            let glyph_width = load(glyph(font, index)?.width)?;
            let advance = if glyph_width < 0.001_f64 {
                mul(load(size)?, load(0.3_f32)?)?
            } else {
                sub(mul(load(size)?, glyph_width)?, spacing)?
            };
            add(width, advance)
        })();
        width = next.map_err(|error: MenuTextUnresolved| error.at(offset))?;
    }
    Ok(MenuTextWidth(add(width, spacing)?))
}

fn out_text_into(
    operations: &mut Vec<MenuTextDrawOp>,
    font: Option<MenuFont<'_>>,
    text: Option<&[u8]>,
    layout: MenuTextLayout,
    stop: Option<usize>,
    precision: MenuTextPrecision,
) -> Result<f32, MenuTextUnresolved> {
    let Some(text) = text else {
        return Ok(layout.x);
    };
    let font = require_font(font, precision)?;
    let top_y = store(sub(
        load(layout.y)?,
        mul(load(layout.sy)?, load(font.metrics.baseline)?)?,
    )?)?;
    let size = store(mul(load(layout.sy)?, load(font.metrics.height_scale)?)?)?;
    let mut spacing = store(mul(load(layout.sy)?, load(font.metrics.tracking)?)?)?;
    let mut char_width = store(mul(load(size)?, load(font.metrics.width_scale)?)?)?;
    let mut start_x = layout.x;
    if layout.flags & (OUTTEXT_CENTER | OUTTEXT_RIGHT) != 0 {
        let measured = measure(font, text, size, stop)?.0;
        start_x = if layout.flags & OUTTEXT_CENTER != 0 {
            store(sub(
                add(mul(load(layout.sx)?, 0.5)?, load(start_x)?)?,
                mul(measured, 0.5)?,
            )?)?
        } else {
            store(sub(add(load(start_x)?, load(layout.sx)?)?, measured)?)?
        };
    } else {
        if layout.flags & OUTTEXT_INDENT != 0 {
            start_x = store(add(mul(load(layout.sy)?, 0.5)?, load(start_x)?)?)?;
        }
        if layout.flags & OUTTEXT_FIT != 0 {
            let measured = measure(font, text, size, stop)?.to_f32()?;
            let available = if layout.flags & OUTTEXT_INDENT != 0 {
                sub(load(layout.sx)?, load(layout.sy)?)?
            } else {
                load(layout.sx)?
            };
            if load(measured)? > available {
                let ratio = div(available, load(measured)?)?;
                char_width = store(mul(load(char_width)?, ratio)?)?;
                spacing = store(mul(ratio, load(spacing)?)?)?;
            }
        }
    }
    let line = GlyphLine {
        font,
        top_y,
        size,
        spacing,
        char_width,
        color: layout.color,
    };
    let mut cursor = load(start_x)?;
    let mut quads = Vec::new();
    for (offset, byte) in text.iter().copied().enumerate() {
        if byte == 0 || stop.is_some_and(|stop| offset >= stop) {
            break;
        }
        if line
            .append(byte, &mut cursor, &mut quads)
            .map_err(|error| error.at(offset))?
        {
            break;
        }
    }
    let cursor = store(cursor)?;
    operations.push(MenuTextDrawOp::BindFont {
        font_index: font.index,
    });
    operations.push(MenuTextDrawOp::GlyphQuads { quads });
    if layout.flags & OUTTEXT_UNDERLINE != 0 {
        let factor = if font.index == 0 { 0.9_f32 } else { 0.8_f32 };
        let first_y = store(add(mul(load(size)?, load(factor)?)?, load(top_y)?)?)?;
        let end_x = store(add(load(cursor)?, load(spacing)?)?)?;
        operations.push(MenuTextDrawOp::ClearTexture);
        operations.push(MenuTextDrawOp::Underline {
            vertices: [
                underline_vertex(start_x, first_y),
                underline_vertex(end_x, first_y),
            ],
        });
        let second_y = store(sub(load(first_y)?, 1.0)?)?;
        operations.push(MenuTextDrawOp::Underline {
            vertices: [
                underline_vertex(start_x, second_y),
                underline_vertex(end_x, second_y),
            ],
        });
    }
    Ok(cursor)
}

struct GlyphLine<'a> {
    font: MenuFont<'a>,
    top_y: f32,
    size: f32,
    spacing: f32,
    char_width: f32,
    color: u32,
}

impl GlyphLine<'_> {
    fn append(
        &self,
        byte: u8,
        cursor: &mut f64,
        quads: &mut Vec<MenuGlyphQuad>,
    ) -> Result<bool, MenuTextUnresolved> {
        let code = self.font.translation[usize::from(byte)];
        let base_index = code as u8;
        if code > 0xff {
            let accent_index = ((code >> 8) & 0x7f) as u8;
            let accent = glyph(self.font, accent_index)?;
            let accent_width = store(mul(load(self.char_width)?, load(accent.width)?)?)?;
            let base = glyph(self.font, base_index)?;
            let base_width = load(base.width)?;
            let mut accent_x = *cursor;
            let mut accent_y = self.top_y;
            let mut char_width = None;
            if base_width >= 0.001_f64 {
                let product = mul(load(self.char_width)?, base_width)?;
                char_width = Some(store(product)?);
                accent_x = add(*cursor, mul(sub(product, load(accent_width)?)?, 0.5)?)?;
                if code & 0x8000 != 0 {
                    accent_y = store(sub(
                        load(self.top_y)?,
                        mul(load(self.size)?, load(0.12_f32)?)?,
                    )?)?;
                }
            }
            quads.push(self.quad(accent_index, accent, accent_x, accent_y, accent_width)?);
            if quads.len() == OUTTEXT_MAX_CHARS {
                return Ok(true);
            }
            let Some(char_width) = char_width else {
                let advance = if base_index != 0 {
                    mul(load(self.size)?, load(0.3_f32)?)?
                } else {
                    sub(load(accent_width)?, load(self.spacing)?)?
                };
                *cursor = add(*cursor, advance)?;
                return Ok(false);
            };
            quads.push(self.quad(base_index, base, *cursor, self.top_y, char_width)?);
            if quads.len() == OUTTEXT_MAX_CHARS {
                return Ok(true);
            }
            *cursor = add(*cursor, sub(load(char_width)?, load(self.spacing)?)?)?;
        } else {
            let base = glyph(self.font, base_index)?;
            let base_width = load(base.width)?;
            if base_width < 0.001_f64 {
                *cursor = add(*cursor, mul(load(self.size)?, load(0.3_f32)?)?)?;
                return Ok(false);
            }
            let char_width = store(mul(load(self.char_width)?, base_width)?)?;
            quads.push(self.quad(base_index, base, *cursor, self.top_y, char_width)?);
            *cursor = add(*cursor, sub(load(char_width)?, load(self.spacing)?)?)?;
            if quads.len() == OUTTEXT_MAX_CHARS {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn quad(
        &self,
        index: u8,
        glyph: &FontGlyph,
        left_x: f64,
        top_y: f32,
        width: f32,
    ) -> Result<MenuGlyphQuad, MenuTextUnresolved> {
        let left = store(left_x)?;
        let right = store(add(left_x, load(width)?)?)?;
        let bottom = store(add(load(top_y)?, load(self.size)?)?)?;
        let bottom_v = store(add(load(self.font.metrics.uv_height)?, load(glyph.v_top)?)?)?;
        load(glyph.u_left)?;
        load(glyph.u_right)?;
        let vertex = |x_position, y_position, u_position, v_position| MenuTextVertex {
            position: [x_position, y_position, 0.0],
            rhw: 1.0,
            color: self.color,
            uv: [u_position, v_position],
        };
        Ok(MenuGlyphQuad {
            glyph: index,
            vertices: [
                vertex(left, top_y, glyph.u_left, glyph.v_top),
                vertex(right, top_y, glyph.u_right, glyph.v_top),
                vertex(left, bottom, glyph.u_left, bottom_v),
                vertex(right, bottom, glyph.u_right, bottom_v),
            ],
        })
    }
}

fn underline_vertex(x_position: f32, y_position: f32) -> MenuTextVertex {
    MenuTextVertex {
        position: [x_position, y_position, 0.0],
        rhw: 1.0,
        color: 0x7fff0000,
        uv: [0.0, 0.0],
    }
}

/// Each operation rounds its significand to PC24, retaining a wide exponent.
/// Binary64 staging is sufficient here: its 53 bits exceed 2*24+2 for the
/// finite +, -, *, / operand domain. No binary32 spill or fusion is implicit.
fn pc24(value: f64) -> Result<f64, MenuTextUnresolved> {
    if !value.is_finite() || value != 0.0 && !value.is_normal() {
        return Err(MenuTextUnresolved::new(
            MenuTextUnresolvedReason::ArithmeticDomain,
        ));
    }
    if value == 0.0 {
        return Ok(value);
    }
    let bits = value.to_bits();
    let discarded = bits & ((1_u64 << 29) - 1);
    let mut retained = bits >> 29;
    if discarded > 1 << 28 || discarded == 1 << 28 && retained & 1 != 0 {
        retained += 1;
    }
    let rounded = f64::from_bits(retained << 29);
    if !rounded.is_finite() {
        return Err(MenuTextUnresolved::new(
            MenuTextUnresolvedReason::ArithmeticDomain,
        ));
    }
    Ok(rounded)
}

fn load(value: f32) -> Result<f64, MenuTextUnresolved> {
    if !value.is_finite() {
        return Err(MenuTextUnresolved::new(
            MenuTextUnresolvedReason::NonFiniteValue,
        ));
    }
    Ok(f64::from(value))
}

/// An explicit binary32/RN store, including subnormals, independent of the
/// host float-conversion rounding mode. The input already has a PC24 significand.
fn store(value: f64) -> Result<f32, MenuTextUnresolved> {
    if !value.is_finite() {
        return Err(MenuTextUnresolved::new(
            MenuTextUnresolvedReason::NonFiniteValue,
        ));
    }
    let bits = value.to_bits();
    let sign = ((bits >> 32) as u32) & 0x80000000;
    if value == 0.0 {
        return Ok(f32::from_bits(sign));
    }
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023;
    if exponent > 127 {
        return Err(MenuTextUnresolved::new(
            MenuTextUnresolvedReason::Binary32Overflow,
        ));
    }
    let significand = (((bits & ((1_u64 << 52) - 1)) | 1_u64 << 52) >> 29) as u32;
    if exponent >= -126 {
        return Ok(f32::from_bits(
            sign | ((exponent + 127) as u32) << 23 | significand & 0x7fffff,
        ));
    }
    let shift = (-126 - exponent) as u32;
    if shift > 24 {
        return Ok(f32::from_bits(sign));
    }
    let mut retained = significand >> shift;
    let discarded = significand & ((1_u32 << shift) - 1);
    let halfway = 1_u32 << (shift - 1);
    if discarded > halfway || discarded == halfway && retained & 1 != 0 {
        retained += 1;
    }
    Ok(f32::from_bits(sign | retained))
}

fn add(left: f64, right: f64) -> Result<f64, MenuTextUnresolved> {
    pc24(left + right)
}

fn sub(left: f64, right: f64) -> Result<f64, MenuTextUnresolved> {
    pc24(left - right)
}

fn mul(left: f64, right: f64) -> Result<f64, MenuTextUnresolved> {
    pc24(left * right)
}

fn div(left: f64, right: f64) -> Result<f64, MenuTextUnresolved> {
    if right == 0.0 {
        return Err(MenuTextUnresolved::new(
            MenuTextUnresolvedReason::ArithmeticDomain,
        ));
    }
    pc24(left / right)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRECISION: MenuTextPrecision = MenuTextPrecision::Pc24Nearest;

    struct Fixture {
        metrics: FontMetrics,
        translation: [u16; 256],
    }

    impl Fixture {
        fn new() -> Self {
            let mut glyphs = vec![
                FontGlyph {
                    u_left: 0.0,
                    v_top: 0.0,
                    u_right: 0.0,
                    width: 0.0,
                };
                256
            ];
            glyphs[usize::from(b'A')] = FontGlyph {
                u_left: 0.25,
                v_top: 0.375,
                u_right: 0.375,
                width: 0.5,
            };
            glyphs[usize::from(b'B')] = FontGlyph {
                u_left: 0.5,
                v_top: 0.25,
                u_right: 0.75,
                width: 0.75,
            };
            glyphs[1] = FontGlyph {
                u_left: 0.125,
                v_top: 0.125,
                u_right: 0.1875,
                width: 0.25,
            };
            Self {
                metrics: FontMetrics {
                    raw_header: [0; 28],
                    height_scale: 1.0,
                    width_scale: 1.0,
                    uv_height: 0.125,
                    tracking: 0.125,
                    baseline: 0.25,
                    glyphs,
                },
                translation: std::array::from_fn(|index| index as u16),
            }
        }

        fn font(&self) -> MenuFont<'_> {
            MenuFont {
                index: 0,
                metrics: &self.metrics,
                translation: &self.translation,
            }
        }

        fn draw(&self, text: &[u8], flags: u32) -> MenuTextResult<f32> {
            out_text(
                Some(self.font()),
                Some(text),
                MenuTextLayout { flags, ..layout() },
                None,
                PRECISION,
            )
        }

        fn width(&self, text: &[u8], stop: Option<usize>) -> f32 {
            text_size(Some(self.font()), Some(text), 8.0, stop, PRECISION)
                .unwrap()
                .to_f32()
                .unwrap()
        }
    }

    fn layout() -> MenuTextLayout {
        MenuTextLayout {
            x: 10.0,
            y: 20.0,
            sx: 100.0,
            sy: 8.0,
            color: 0x81234567,
            flags: 0,
        }
    }

    fn batches<T>(result: &MenuTextResult<T>) -> Vec<&[MenuGlyphQuad]> {
        result
            .operations
            .iter()
            .filter_map(|operation| match operation {
                MenuTextDrawOp::GlyphQuads { quads } => Some(quads.as_slice()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn null_bypasses_resources_precision_and_numeric_inputs() {
        let invalid = MenuTextLayout {
            x: f32::from_bits(0xffc01234),
            y: f32::NAN,
            sx: f32::INFINITY,
            sy: f32::NEG_INFINITY,
            ..layout()
        };
        assert_eq!(
            text_size(None, None, f32::NAN, None, MenuTextPrecision::Unestablished)
                .unwrap()
                .to_f32()
                .unwrap()
                .to_bits(),
            0
        );
        let result = out_text(
            None,
            None,
            invalid,
            Some(0),
            MenuTextPrecision::Unestablished,
        );
        assert!(result.operations.is_empty());
        assert_eq!(result.result.unwrap().to_bits(), invalid.x.to_bits());
        let result = out_text_multi_line(None, None, invalid, MenuTextPrecision::Unestablished);
        assert!(result.operations.is_empty());
        assert_eq!(result.result, Ok(()));
    }

    #[test]
    fn empty_and_stopped_text_are_not_null() {
        let fixture = Fixture::new();
        assert_eq!(fixture.width(b"", None), 1.0);
        assert_eq!(fixture.width(b"AB", Some(0)), 1.0);
        for (text, stop) in [(b"".as_slice(), None), (b"AB".as_slice(), Some(0))] {
            let result = out_text(Some(fixture.font()), Some(text), layout(), stop, PRECISION);
            assert_eq!(result.result, Ok(10.0));
            assert_eq!(
                result.operations,
                vec![
                    MenuTextDrawOp::BindFont { font_index: 0 },
                    MenuTextDrawOp::GlyphQuads { quads: vec![] },
                ]
            );
        }
    }

    #[test]
    fn glyph_geometry_uses_original_metrics_units_uvs_and_argb() {
        let fixture = Fixture::new();
        let result = fixture.draw(b"AB", 0);
        assert_eq!(fixture.width(b"AB", None), 9.0);
        assert_eq!(result.result, Ok(18.0));
        let quads = batches(&result)[0];
        assert_eq!(quads.len(), 2);
        assert_eq!(quads[0].glyph, b'A');
        assert_eq!(
            quads[0].vertices.map(|vertex| vertex.position),
            [
                [10.0, 18.0, 0.0],
                [14.0, 18.0, 0.0],
                [10.0, 26.0, 0.0],
                [14.0, 26.0, 0.0],
            ]
        );
        assert_eq!(
            quads[0].vertices.map(|vertex| vertex.uv),
            [[0.25, 0.375], [0.375, 0.375], [0.25, 0.5], [0.375, 0.5],]
        );
        assert!(quads
            .iter()
            .flat_map(|quad| quad.vertices)
            .all(|vertex| vertex.rhw == 1.0 && vertex.color == 0x81234567));
        assert_eq!(quads[1].vertices[0].position, [13.0, 18.0, 0.0]);
        assert_eq!(MENU_GLYPH_INDICES, [0, 1, 2, 1, 3, 2]);
    }

    #[test]
    fn byte_inputs_use_unsigned_translation_and_c_string_termination() {
        let mut fixture = Fixture::new();
        fixture.translation[255] = u16::from(b'A');
        let result = fixture.draw(&[0xff, 0, b'B', b'~', b'A'], 0);
        assert_eq!(result.result, Ok(13.0));
        assert_eq!(batches(&result)[0].len(), 1);
        assert_eq!(batches(&result)[0][0].glyph, b'A');
        assert_eq!(fixture.width(&[0xff, 0, b'B'], None), 4.0);
        assert_eq!(fixture.width(b"AB", Some(1)), 4.0);
        assert_eq!(fixture.width(b"AB", Some(50)), 9.0);
        assert_eq!(fixture.width(b"AB\0B", None), 9.0);
        let stopped = out_text(
            Some(fixture.font()),
            Some(b"AB"),
            layout(),
            Some(1),
            PRECISION,
        );
        assert_eq!(stopped.result, Ok(13.0));
    }

    #[test]
    fn font_view_borrows_resources_and_binding_keeps_owner_identity() {
        let mut fixture = Fixture::new();
        assert!(std::ptr::eq(fixture.font().metrics, &fixture.metrics));
        assert!(std::ptr::eq(
            fixture.font().translation,
            &fixture.translation
        ));
        let font = MenuFont {
            index: 7,
            ..fixture.font()
        };
        let first = out_text(Some(font), Some(b"A"), layout(), None, PRECISION);
        assert_eq!(
            first.operations[0],
            MenuTextDrawOp::BindFont { font_index: 7 }
        );
        fixture.metrics.glyphs[usize::from(b'A')].width = 0.75;
        assert_eq!(fixture.draw(b"A", 0).result, Ok(15.0));
        assert_eq!(first.result, Ok(13.0));
    }

    #[test]
    fn accented_translation_draws_accent_before_base_but_measures_only_base() {
        let mut fixture = Fixture::new();
        fixture.translation[0xc0] = 0x8141;
        let result = fixture.draw(&[0xc0], 0);
        assert_eq!(fixture.width(&[0xc0], None), 4.0);
        assert_eq!(result.result, Ok(13.0));
        let quads = batches(&result)[0];
        assert_eq!(
            quads.iter().map(|quad| quad.glyph).collect::<Vec<_>>(),
            [1, b'A']
        );
        assert_eq!(quads[0].vertices[0].position, [11.0, 17.04, 0.0]);
        assert_eq!(quads[0].vertices[1].position, [13.0, 17.04, 0.0]);
        assert_eq!(quads[1].vertices[0].position, [10.0, 18.0, 0.0]);
        assert_eq!(quads[1].vertices[1].position, [14.0, 18.0, 0.0]);
        fixture.translation[0xc0] = 0x0141;
        assert_eq!(
            batches(&fixture.draw(&[0xc0], 0))[0][0].vertices[0].position[1],
            18.0
        );
    }

    #[test]
    fn zero_width_accent_is_still_a_quad() {
        let mut fixture = Fixture::new();
        fixture.translation[0xc0] = 0x8141;
        fixture.metrics.glyphs[1].width = 0.0;
        let result = fixture.draw(&[0xc0], 0);
        let quads = batches(&result)[0];
        assert_eq!(quads.len(), 2);
        assert_eq!(quads[0].vertices[0].position, [12.0, 17.04, 0.0]);
        assert_eq!(quads[0].vertices[1].position, quads[0].vertices[0].position);
    }

    #[test]
    fn accent_without_base_has_distinct_zero_and_nonzero_base_advances() {
        let mut fixture = Fixture::new();
        fixture.translation[0xc0] = 0x8100;
        let isolated = fixture.draw(&[0xc0], 0);
        assert_eq!(isolated.result, Ok(11.0));
        assert_eq!(batches(&isolated)[0].len(), 1);
        assert_eq!(
            batches(&isolated)[0][0].vertices[0].position,
            [10.0, 18.0, 0.0]
        );
        fixture.translation[0xc0] = 0x8142;
        fixture.metrics.glyphs[usize::from(b'B')].width = 0.0;
        let missing = fixture.draw(&[0xc0], 0);
        assert_eq!(missing.result, Ok(12.4));
        assert_eq!(batches(&missing)[0].len(), 1);
        assert_eq!(batches(&missing)[0][0].vertices[0].position[1], 18.0);
    }

    #[test]
    fn missing_glyph_width_uses_different_measurement_and_draw_space_scales() {
        let mut fixture = Fixture::new();
        fixture.metrics.width_scale = 2.0;
        assert_eq!(fixture.width(b" ", None), 5.8);
        let result = fixture.draw(b" ", 0);
        assert_eq!(result.result, Ok(12.4));
        assert!(batches(&result)[0].is_empty());
    }

    #[test]
    fn normal_and_accented_caps_preserve_native_advance_asymmetry() {
        let mut fixture = Fixture::new();
        let ordinary = fixture.draw(&[b'A'; 120], 0);
        assert_eq!(ordinary.result, Ok(310.0));
        assert_eq!(batches(&ordinary)[0].len(), 100);
        assert_eq!(fixture.width(&[b'A'; 120], None), 361.0);
        fixture.translation[0xc0] = 0x8141;
        let accented = fixture.draw(&[0xc0; 60], 0);
        assert_eq!(accented.result, Ok(157.0));
        assert_eq!(batches(&accented)[0].len(), 100);
        let mut text = vec![b'A'; 99];
        text.extend_from_slice(&[0xc0, b'A']);
        let partial_pair = fixture.draw(&text, 0);
        assert_eq!(partial_pair.result, Ok(307.0));
        assert_eq!(batches(&partial_pair)[0].last().unwrap().glyph, 1);
    }

    #[test]
    fn alignment_measures_full_uncapped_text() {
        let fixture = Fixture::new();
        let result = fixture.draw(&[b'A'; 120], OUTTEXT_RIGHT);
        assert_eq!(batches(&result)[0][0].vertices[0].position[0], -251.0);
        assert_eq!(result.result, Ok(49.0));
    }

    #[test]
    fn alignment_precedence_ignores_indent_fit_and_unrecognized_bits() {
        let fixture = Fixture::new();
        let center = fixture.draw(b"AB", OUTTEXT_CENTER);
        assert_eq!(batches(&center)[0][0].vertices[0].position[0], 55.5);
        assert_eq!(center.result, Ok(63.5));
        assert_eq!(
            fixture.draw(
                b"AB",
                OUTTEXT_CENTER | OUTTEXT_RIGHT | OUTTEXT_INDENT | OUTTEXT_FIT
            ),
            center
        );
        let right = fixture.draw(b"AB", OUTTEXT_RIGHT);
        assert_eq!(batches(&right)[0][0].vertices[0].position[0], 101.0);
        assert_eq!(
            fixture.draw(b"AB", OUTTEXT_RIGHT | OUTTEXT_INDENT | OUTTEXT_FIT),
            right
        );
        assert_eq!(fixture.draw(b"AB", 0x80000020), fixture.draw(b"AB", 0));
    }

    #[test]
    fn alignment_measurement_receives_height_scaled_size() {
        let mut fixture = Fixture::new();
        fixture.metrics.height_scale = 2.0;
        let result = fixture.draw(b"AA", OUTTEXT_CENTER);
        assert_eq!(
            batches(&result)[0][0].vertices[0].position,
            [53.0, 18.0, 0.0]
        );
        assert_eq!(result.result, Ok(67.0));
    }

    #[test]
    fn fit_scales_width_and_spacing_and_accounts_for_indent() {
        let fixture = Fixture::new();
        let result = out_text(
            Some(fixture.font()),
            Some(b"AA"),
            MenuTextLayout {
                sx: 3.5,
                flags: OUTTEXT_FIT,
                ..layout()
            },
            None,
            PRECISION,
        );
        assert_eq!(result.result, Ok(13.0));
        assert_eq!(batches(&result)[0][0].vertices[1].position[0], 12.0);
        assert_eq!(batches(&result)[0][1].vertices[0].position[0], 11.5);
        let result = out_text(
            Some(fixture.font()),
            Some(b"AA"),
            MenuTextLayout {
                sx: 11.5,
                flags: OUTTEXT_FIT | OUTTEXT_INDENT,
                ..layout()
            },
            None,
            PRECISION,
        );
        assert_eq!(result.result, Ok(17.0));
        assert_eq!(batches(&result)[0][0].vertices[0].position[0], 14.0);
        assert_eq!(batches(&result)[0][0].vertices[1].position[0], 16.0);
        assert_eq!(fixture.draw(b"AA", OUTTEXT_FIT), fixture.draw(b"AA", 0));
    }

    #[test]
    fn fit_does_not_clamp_negative_available_width() {
        let fixture = Fixture::new();
        let result = out_text(
            Some(fixture.font()),
            Some(b"A"),
            MenuTextLayout {
                sx: -4.0,
                flags: OUTTEXT_FIT,
                ..layout()
            },
            None,
            PRECISION,
        );
        assert_eq!(result.result, Ok(7.0));
        assert_eq!(batches(&result)[0][0].vertices[1].position[0], 6.0);
    }

    #[test]
    fn underline_is_two_ordered_red_untextured_lines_even_for_empty_text() {
        let fixture = Fixture::new();
        let result = fixture.draw(b"", OUTTEXT_UNDERLINE);
        assert_eq!(result.operations.len(), 5);
        assert_eq!(result.operations[2], MenuTextDrawOp::ClearTexture);
        for (offset, expected_y) in [(3, 25.2), (4, 24.2)] {
            let MenuTextDrawOp::Underline { vertices } = result.operations[offset] else {
                panic!("expected ordered underline");
            };
            assert_eq!(
                vertices.map(|vertex| vertex.position),
                [[10.0, expected_y, 0.0], [11.0, expected_y, 0.0]]
            );
            assert!(vertices.iter().all(|vertex| vertex.color == 0x7fff0000
                && vertex.uv == [0.0, 0.0]
                && vertex.rhw == 1.0));
        }
        let result = out_text(
            Some(MenuFont {
                index: 2,
                ..fixture.font()
            }),
            Some(b"AB"),
            MenuTextLayout {
                flags: OUTTEXT_UNDERLINE,
                ..layout()
            },
            None,
            PRECISION,
        );
        assert_eq!(result.result, Ok(18.0));
        assert_eq!(
            result.operations[3],
            MenuTextDrawOp::Underline {
                vertices: [underline_vertex(10.0, 24.4), underline_vertex(19.0, 24.4)],
            }
        );
    }

    #[test]
    fn multiline_splits_only_tilde_and_keeps_empty_and_trailing_lines() {
        let fixture = Fixture::new();
        let result = out_text_multi_line(Some(fixture.font()), Some(b"A~~B~"), layout(), PRECISION);
        assert_eq!(result.result, Ok(()));
        assert_eq!(result.operations.len(), 8);
        let lines = batches(&result);
        assert_eq!(
            lines.iter().map(|line| line.len()).collect::<Vec<_>>(),
            [1, 0, 1, 0]
        );
        assert_eq!(lines[0][0].vertices[0].position, [10.0, 18.0, 0.0]);
        assert_eq!(lines[2][0].vertices[0].position, [10.0, 34.0, 0.0]);
        let newline = out_text_multi_line(
            Some(fixture.font()),
            Some(b"A\nB"),
            MenuTextLayout {
                sx: 0.0,
                ..layout()
            },
            PRECISION,
        );
        assert_eq!(newline.result, Ok(()));
        assert_eq!(batches(&newline).len(), 1);
        assert_eq!(batches(&newline)[0].len(), 2);
        assert_eq!(
            batches(&newline)[0][1].vertices[0].position,
            [15.4, 18.0, 0.0]
        );
        let nul = out_text_multi_line(Some(fixture.font()), Some(b"A\0~B"), layout(), PRECISION);
        assert_eq!(nul.result, Ok(()));
        assert_eq!(batches(&nul).len(), 1);
    }

    #[test]
    fn multiline_resets_quad_cap_at_every_tilde() {
        let fixture = Fixture::new();
        let mut text = vec![b'A'; 101];
        text.push(b'~');
        text.extend_from_slice(&[b'A'; 101]);
        let result = out_text_multi_line(Some(fixture.font()), Some(&text), layout(), PRECISION);
        assert_eq!(result.result, Ok(()));
        assert_eq!(
            batches(&result)
                .iter()
                .map(|line| line.len())
                .collect::<Vec<_>>(),
            [100, 100]
        );
        assert_eq!(
            batches(&result)[1][0].vertices[0].position,
            [10.0, 26.0, 0.0]
        );
    }

    #[test]
    fn multiline_updates_y_even_after_the_final_line_and_keeps_reached_draws() {
        let mut fixture = Fixture::new();
        fixture.metrics.baseline = 0.0;
        fixture.metrics.height_scale = 0.0;
        fixture.metrics.tracking = 0.0;
        let result = out_text_multi_line(
            Some(fixture.font()),
            Some(b""),
            MenuTextLayout {
                y: f32::MAX,
                sy: f32::MAX,
                ..layout()
            },
            PRECISION,
        );
        assert_eq!(
            result.result.unwrap_err().reason,
            MenuTextUnresolvedReason::Binary32Overflow
        );
        assert_eq!(result.operations.len(), 2);
        assert!(batches(&result)[0].is_empty());
    }

    #[test]
    fn missing_font_and_unestablished_precision_do_not_become_empty_success() {
        let fixture = Fixture::new();
        assert_eq!(
            text_size(None, Some(b""), 8.0, None, PRECISION)
                .unwrap_err()
                .reason,
            MenuTextUnresolvedReason::MissingFont
        );
        let result = out_text(
            Some(fixture.font()),
            Some(b""),
            layout(),
            None,
            MenuTextPrecision::Unestablished,
        );
        assert!(result.operations.is_empty());
        assert_eq!(
            result.result.unwrap_err().reason,
            MenuTextUnresolvedReason::PrecisionUnestablished
        );
        assert_eq!(
            text_size(
                Some(fixture.font()),
                Some(b"A"),
                8.0,
                None,
                MenuTextPrecision::Unestablished
            )
            .unwrap_err()
            .reason,
            MenuTextUnresolvedReason::PrecisionUnestablished
        );
    }

    #[test]
    fn missing_glyph_reports_character_offset_and_preserves_only_drawn_lines() {
        let mut fixture = Fixture::new();
        fixture.metrics.glyphs.truncate(66);
        assert_eq!(fixture.width(b"AB", Some(1)), 4.0);
        let result = fixture.draw(b"AB", 0);
        assert!(result.operations.is_empty());
        assert_eq!(
            result.result.unwrap_err(),
            MenuTextUnresolved {
                reason: MenuTextUnresolvedReason::MissingGlyph(b'B'),
                byte_offset: Some(1),
            }
        );
        let result = out_text_multi_line(Some(fixture.font()), Some(b"A~B"), layout(), PRECISION);
        assert_eq!(result.operations.len(), 2);
        assert_eq!(batches(&result)[0][0].glyph, b'A');
        assert_eq!(
            result.result.unwrap_err(),
            MenuTextUnresolved {
                reason: MenuTextUnresolvedReason::MissingGlyph(b'B'),
                byte_offset: Some(2),
            }
        );
        let mut text = vec![b'A'; 100];
        text.push(b'B');
        assert_eq!(fixture.draw(&text, 0).result, Ok(310.0));
        assert_eq!(
            fixture
                .draw(&text, OUTTEXT_RIGHT)
                .result
                .unwrap_err()
                .byte_offset,
            Some(100)
        );
    }

    #[test]
    fn accent_is_read_before_the_base_and_measurement_never_reads_the_accent() {
        let mut fixture = Fixture::new();
        fixture.translation[0xc0] = 0x8241;
        fixture.metrics.glyphs[2].width = f32::NAN;
        assert_eq!(fixture.width(&[0xc0], None), 4.0);
        fixture.metrics.glyphs.truncate(3);
        let result = fixture.draw(&[0xc0], 0);
        assert_eq!(
            result.result.unwrap_err().reason,
            MenuTextUnresolvedReason::NonFiniteValue
        );
        assert!(result.operations.is_empty());
    }

    #[test]
    fn nonfinite_and_unused_values_preserve_lazy_failure_boundaries() {
        let mut fixture = Fixture::new();
        fixture.metrics.uv_height = f32::NAN;
        let empty = fixture.draw(b"", 0);
        assert_eq!(empty.result, Ok(10.0));
        let visible = fixture.draw(b"A", 0);
        assert!(visible.operations.is_empty());
        assert_eq!(
            visible.result.unwrap_err(),
            MenuTextUnresolved {
                reason: MenuTextUnresolvedReason::NonFiniteValue,
                byte_offset: Some(0),
            }
        );
        let empty = out_text(
            Some(fixture.font()),
            Some(b""),
            MenuTextLayout {
                sx: f32::NAN,
                ..layout()
            },
            None,
            PRECISION,
        );
        assert_eq!(empty.result, Ok(10.0));
        fixture.metrics.uv_height = 0.125;
        let underline_overflow = out_text(
            Some(fixture.font()),
            Some(b""),
            MenuTextLayout {
                y: f32::MAX,
                sy: f32::MAX,
                flags: OUTTEXT_UNDERLINE,
                ..layout()
            },
            None,
            PRECISION,
        );
        assert_eq!(underline_overflow.operations.len(), 2);
        assert_eq!(
            underline_overflow.result.unwrap_err().reason,
            MenuTextUnresolvedReason::Binary32Overflow
        );
    }

    #[test]
    fn translation_to_zero_is_space_not_input_termination() {
        let mut fixture = Fixture::new();
        fixture.translation[255] = 0;
        let result = fixture.draw(&[0xff, b'A'], 0);
        assert_eq!(result.result, Ok(15.4));
        assert_eq!(batches(&result)[0].len(), 1);
        assert_eq!(batches(&result)[0][0].vertices[0].position[0], 12.4);
    }

    #[test]
    fn threshold_compares_loaded_glyph_width_against_binary64_point_zero_zero_one() {
        let mut fixture = Fixture::new();
        let upper = 0.001_f32;
        let lower = f32::from_bits(upper.to_bits() - 1);
        assert!(f64::from(lower) < 0.001_f64 && f64::from(upper) > 0.001_f64);
        fixture.metrics.glyphs[usize::from(b'A')].width = lower;
        assert!(batches(&fixture.draw(b"A", 0))[0].is_empty());
        fixture.metrics.glyphs[usize::from(b'A')].width = upper;
        assert_eq!(batches(&fixture.draw(b"A", 0))[0].len(), 1);
    }

    #[test]
    fn pc24_rounds_each_operation_ties_even_without_a_binary32_exponent_limit() {
        let half_ulp = 2.0_f64.powi(-24);
        assert_eq!(add(1.0, half_ulp).unwrap(), 1.0);
        assert_eq!(
            add(1.0 + 2.0_f64.powi(-23), half_ulp).unwrap(),
            1.0 + 2.0_f64.powi(-22)
        );
        let wide = mul(f64::from(f32::MAX), 2.0).unwrap();
        assert!(wide.is_finite());
        assert_eq!(
            store(wide).unwrap_err().reason,
            MenuTextUnresolvedReason::Binary32Overflow
        );
        assert_eq!(
            div(1.0, 0.0).unwrap_err().reason,
            MenuTextUnresolvedReason::ArithmeticDomain
        );
    }

    #[test]
    fn explicit_binary32_stores_round_subnormals_and_preserve_signed_zero() {
        let minimum = f64::from(f32::from_bits(1));
        assert_eq!(store(mul(minimum, 0.5).unwrap()).unwrap().to_bits(), 0);
        assert_eq!(store(mul(minimum, 0.75).unwrap()).unwrap().to_bits(), 1);
        assert_eq!(
            store(mul(minimum, -0.5).unwrap()).unwrap().to_bits(),
            0x80000000
        );
        assert_eq!(store(-0.0).unwrap().to_bits(), 0x80000000);
        let normal_boundary = f64::from(f32::MIN_POSITIVE) - minimum * 0.5;
        assert_eq!(
            store(pc24(normal_boundary).unwrap()).unwrap(),
            f32::MIN_POSITIVE
        );
    }

    #[test]
    fn alignment_does_not_prematurely_spill_text_size_return() {
        let mut fixture = Fixture::new();
        fixture.metrics.tracking = 0.5;
        fixture.metrics.baseline = 0.0;
        let minimum = f32::from_bits(1);
        let measured =
            text_size(Some(fixture.font()), Some(b""), minimum, None, PRECISION).unwrap();
        assert_eq!(measured.to_f32().unwrap().to_bits(), 0);
        let result = out_text(
            Some(fixture.font()),
            Some(b""),
            MenuTextLayout {
                x: 0.0,
                y: 0.0,
                sx: f32::from_bits(3),
                sy: minimum,
                flags: OUTTEXT_CENTER,
                ..layout()
            },
            None,
            PRECISION,
        );
        assert_eq!(result.result.unwrap().to_bits(), 1);
    }

    #[test]
    fn fit_retains_scale_exponent_until_multiplying_and_storing_width() {
        let mut fixture = Fixture::new();
        fixture.metrics.tracking = 0.0;
        fixture.metrics.baseline = 0.0;
        fixture.metrics.glyphs[usize::from(b'A')].width = 1.0;
        let result = out_text(
            Some(fixture.font()),
            Some(b"A"),
            MenuTextLayout {
                x: 0.0,
                y: 0.0,
                sx: f32::from_bits(1),
                sy: f32::MAX,
                flags: OUTTEXT_FIT,
                ..layout()
            },
            None,
            PRECISION,
        );
        assert_eq!(result.result.unwrap().to_bits(), 1);
        assert_eq!(batches(&result)[0][0].vertices[1].position[0].to_bits(), 1);
        assert_eq!(batches(&result)[0][0].vertices[2].position[1], f32::MAX);
    }

    #[test]
    fn space_accumulator_stays_retained_between_input_bytes() {
        let mut fixture = Fixture::new();
        fixture.metrics.tracking = 0.0;
        fixture.metrics.baseline = 0.0;
        let minimum = f32::from_bits(1);
        assert_eq!(
            text_size(Some(fixture.font()), Some(b"  "), minimum, None, PRECISION)
                .unwrap()
                .to_f32()
                .unwrap()
                .to_bits(),
            1
        );
        let result = out_text(
            Some(fixture.font()),
            Some(b"  "),
            MenuTextLayout {
                x: 0.0,
                y: 0.0,
                sy: minimum,
                ..layout()
            },
            None,
            PRECISION,
        );
        assert_eq!(result.result.unwrap().to_bits(), 1);
        assert!(batches(&result)[0].is_empty());
    }
}
