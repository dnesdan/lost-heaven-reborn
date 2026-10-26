//! Source IGraph::ResizeImage, RGB/filter=1 downscale branch.
//!
//! mafia-re 29b65b0, IGraph/IGraph.cpp:3349..3525. The source interior loop
//! advances the horizontal offset twice; crossing a row there is intentional.
//! This is a pinned-source PC24/RN arithmetic profile, not a claim of original
//! runtime or function parity: binary32 operations in source order, no
//! fused multiply/add, and FloatToInt's round-to-nearest/ties-even. Dimensions
//! and pixel sums stay in the normal finite binary32 range. This does not
//! represent the x87 status word or alternative rounding-control settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightmapResizeError {
    UnsupportedDimensions,
    InvalidRgbLength,
    HostAllocation,
    UnsupportedReadRange { pixel: usize },
    InvalidNumericResult,
}
const MRG_ZERO: f32 = 1.0e-8;

fn pixel(source: &[u8], at: usize) -> Result<[f32; 3], LightmapResizeError> {
    let start = at
        .checked_mul(3)
        .ok_or(LightmapResizeError::InvalidRgbLength)?;
    let p = source
        .get(start..start + 3)
        .ok_or(LightmapResizeError::UnsupportedReadRange { pixel: at })?;
    Ok([f32::from(p[0]), f32::from(p[1]), f32::from(p[2])])
}
fn add_one(sum: &mut [f32; 3], value: [f32; 3], weight: f32) {
    for channel in 0..3 {
        sum[channel] += value[channel] * weight;
    }
}
fn add_two(sum: &mut [f32; 3], value: [f32; 3], first: f32, second: f32) {
    for channel in 0..3 {
        sum[channel] += (value[channel] * first) * second;
    }
}

pub(crate) fn resize_rgb_source(
    source_size: [u16; 2],
    original_rgb: &[u8],
    destination_size: [u16; 2],
) -> Result<Box<[u8]>, LightmapResizeError> {
    let [sx, sy] = source_size.map(usize::from);
    let [dx, dy] = destination_size.map(usize::from);
    if [sx, sy, dx, dy]
        .into_iter()
        .any(|v| v == 0 || v > i16::MAX as usize)
        || dx > sx
        || dy > sy
    {
        return Err(LightmapResizeError::UnsupportedDimensions);
    }
    if original_rgb.len() != sx * sy * 3 {
        return Err(LightmapResizeError::InvalidRgbLength);
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(dx * dy * 3)
        .map_err(|_| LightmapResizeError::HostAllocation)?;
    output.resize(dx * dy * 3, 0);
    let x_ratio = (sx - 1) as f32 / dx as f32;
    let y_ratio = (sy - 1) as f32 / dy as f32;
    for y in 0..dy {
        for x in 0..dx {
            let fy0 = y as f32 * y_ratio;
            let fy1 = (y + 1) as f32 * y_ratio;
            let iy0 = fy0 as usize + 1;
            let iy1 = fy1 as usize;
            let (wy0, wy1) = if iy0 > iy1 {
                (fy1 - fy0, 0.0)
            } else {
                (iy0 as f32 - fy0, fy1 - iy1 as f32)
            };
            let rows = iy1.saturating_sub(iy0);
            let fx0 = x as f32 * x_ratio;
            let fx1 = (x + 1) as f32 * x_ratio;
            let ix0 = fx0 as usize + 1;
            let ix1 = fx1 as usize;
            let (wx0, wx1) = if ix0 > ix1 {
                (fx1 - fx0, 0.0)
            } else {
                (ix0 as f32 - fx0, fx1 - ix1 as f32)
            };
            let cols = ix1.saturating_sub(ix0);
            let mut sum = [0.0; 3];
            let mut weight = 0.0;
            if wy0 > MRG_ZERO {
                let pos = (iy0 - 1) * sx + ix0 - 1;
                if wx0 > MRG_ZERO {
                    let p = pixel(original_rgb, pos)?;
                    for c in 0..3 {
                        sum[c] = (p[c] * wx0) * wy0;
                    }
                    weight = wx0 * wy0;
                }
                if cols > 0 {
                    for i in 1..=cols {
                        add_one(&mut sum, pixel(original_rgb, pos + i)?, wy0);
                    }
                    weight += cols as f32 * wy0;
                }
                if wx1 > MRG_ZERO {
                    add_two(&mut sum, pixel(original_rgb, pos + cols + 1)?, wx1, wy0);
                    weight += wx1 * wy0;
                }
            }
            if rows > 0 {
                if wx0 > MRG_ZERO {
                    for j in 0..rows {
                        add_one(
                            &mut sum,
                            pixel(original_rgb, (iy0 + j) * sx + ix0 - 1)?,
                            wx0,
                        );
                    }
                    weight += rows as f32 * wx0;
                }
                if cols > 0 {
                    for j in 0..rows {
                        for i in 0..cols {
                            // Source pos includes i and the actual load includes
                            // i again. Do not clamp this to its nominal row.
                            let p = pixel(original_rgb, (iy0 + j) * sx + ix0 + i + i)?;
                            for c in 0..3 {
                                sum[c] += p[c];
                            }
                        }
                    }
                    weight += (cols * rows) as f32;
                }
                if wx1 > MRG_ZERO {
                    for j in 0..rows {
                        add_one(
                            &mut sum,
                            pixel(original_rgb, (iy0 + j) * sx + ix0 + cols)?,
                            wx1,
                        );
                    }
                    weight += rows as f32 * wx1;
                }
            }
            if wy1 > MRG_ZERO {
                let pos = (iy0 + rows) * sx + ix0 - 1;
                if wx0 > MRG_ZERO {
                    add_two(&mut sum, pixel(original_rgb, pos)?, wx0, wy1);
                    weight += wx0 * wy1;
                }
                if cols > 0 {
                    for i in 1..=cols {
                        add_one(&mut sum, pixel(original_rgb, pos + i)?, wy1);
                    }
                    weight += cols as f32 * wy1;
                }
                if wx1 > MRG_ZERO {
                    add_two(&mut sum, pixel(original_rgb, pos + cols + 1)?, wx1, wy1);
                    weight += wx1 * wy1;
                }
            }
            if weight <= MRG_ZERO {
                sum = [255.0, 0.0, 255.0];
            } else {
                let k = 1.0 / weight;
                for c in &mut sum {
                    *c *= k;
                }
            }
            for (c, value) in sum.into_iter().enumerate() {
                // FloatToInt uses FISTP, not truncation. Admitted RGB/filter
                // inputs must remain in the representable byte domain.
                let rounded = value.round_ties_even();
                if !rounded.is_finite() || !(0.0..=255.0).contains(&rounded) {
                    return Err(LightmapResizeError::InvalidNumericResult);
                }
                output[(y * dx + x) * 3 + c] = rounded as u8;
            }
        }
    }
    Ok(output.into_boxed_slice())
}

#[cfg(test)]
#[path = "lightmap_resize_tests.rs"]
mod tests;
