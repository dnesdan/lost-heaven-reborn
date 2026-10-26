//! CPU true-color projection of LightMap packing. The free-rectangle ordering,
//! rotation and candidate selection follow pinned mafia-re 29b65b0
//! lightmap.cpp. Native 16-bit conversion/dither, compression and ResizeImage
//! are separate dependencies; exhaustion reports a real frontier.
use std::sync::{Arc, Mutex, RwLock, Weak};

#[derive(Clone, PartialEq, Eq)]
pub struct LightmapAtlas {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Changes whenever another lightmap fills this shared texture.
    pub revision: u64,
}
impl std::fmt::Debug for LightmapAtlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LightmapAtlas")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("rgba_bytes", &self.rgba.len())
            .field("revision", &self.revision)
            .finish()
    }
}
pub type LightmapAtlasOwner = Arc<RwLock<LightmapAtlas>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LightmapTexels {
    Single(u32),
    Rgb(Vec<u8>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightmapRectangle {
    pub width: u16,
    pub height: u16,
    pub texels: LightmapTexels,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightmapPlacement {
    pub x: u16,
    pub y: u16,
    pub rotated: bool,
    pub width: u16,
    pub height: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtlasError {
    HostAllocation,
    InvalidRectangle,
    ResizeRequired,
    Poisoned,
    Quota,
}
#[derive(Clone, Copy, Debug)]
struct FreeRect {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}
impl FreeRect {
    fn area(self) -> u32 {
        u32::from(self.w) * u32::from(self.h)
    }
}
#[derive(Clone, Debug)]
struct Page {
    width: u16,
    height: u16,
    free: Vec<FreeRect>,
    texture: Weak<RwLock<LightmapAtlas>>,
}
impl Page {
    fn free_area(&self) -> u32 {
        self.free.iter().map(|r| r.area()).sum()
    }
}
/// One LS3DF ownership domain. The remaining-room list holds weak texture
/// identities, as the native raw pointers do. Every live litLod owns its map.
#[derive(Debug, Default)]
pub struct LightmapAtlasPool {
    state: Arc<Mutex<PoolState>>,
}
#[derive(Debug, Default)]
struct PoolState {
    pages: Vec<Page>,
    poisoned: bool,
}
#[derive(Clone, Debug)]
pub struct LightmapAllocationLease {
    _release: Arc<AllocationRelease>,
}
#[derive(Debug)]
struct AllocationRelease {
    pool: Weak<Mutex<PoolState>>,
    texture: Weak<RwLock<LightmapAtlas>>,
    placements: Vec<LightmapPlacement>,
}
impl Drop for AllocationRelease {
    fn drop(&mut self) {
        let Some(pool) = self.pool.upgrade() else {
            return;
        };
        let Ok(mut state) = pool.lock() else { return };
        let Some(index) = state
            .pages
            .iter()
            .position(|p| Weak::ptr_eq(&p.texture, &self.texture))
        else {
            return;
        };
        let Some(texture) = self.texture.upgrade() else {
            state.pages.remove(index);
            return;
        };
        let mut page = state.pages.remove(index);
        let Ok(mut atlas) = texture.write() else {
            state.poisoned = true;
            return;
        };
        for p in &self.placements {
            let (w, h) = if p.rotated {
                (p.height, p.width)
            } else {
                (p.width, p.height)
            };
            add_free(
                &mut page.free,
                FreeRect {
                    x: p.x,
                    y: p.y,
                    w,
                    h,
                },
            );
            for y in p.y..p.y + h {
                for x in p.x..p.x + w {
                    let at = (usize::from(y) * usize::from(page.width) + usize::from(x)) * 4;
                    atlas.rgba[at..at + 4].fill(255);
                }
            }
        }
        if let Some(next) = atlas.revision.checked_add(1) {
            atlas.revision = next
        } else {
            state.poisoned = true;
        }
        drop(atlas);
        let at = state
            .pages
            .iter()
            .position(|p| p.free_area() >= page.free_area())
            .unwrap_or(state.pages.len());
        state.pages.insert(at, page);
    }
}

fn add_free(free: &mut Vec<FreeRect>, rect: FreeRect) {
    if rect.w < 3 || rect.h < 3 {
        return;
    }
    let at = free
        .iter()
        .position(|r| r.area() >= rect.area())
        .unwrap_or(free.len());
    free.insert(at, rect);
}
fn place(free: &mut Vec<FreeRect>, sx: u16, sy: u16) -> Option<LightmapPlacement> {
    let (index, mode) = free.iter().enumerate().find_map(|(i, r)| {
        let mode = if r.w >= sx && r.h >= sy {
            if (r.h >= sx && r.w == sy) || (r.h == sx && r.w >= sy) {
                3
            } else if r.h >= sx && r.w >= sy {
                2
            } else {
                1
            }
        } else if r.h >= sx && r.w >= sy {
            3
        } else {
            return None;
        };
        Some((i, mode))
    })?;
    let mut r = free.remove(index);
    let mut rotated = mode == 3;
    let out = LightmapPlacement {
        x: r.x,
        y: r.y,
        rotated,
        width: sx,
        height: sy,
    };
    let (w, h) = if rotated { (sy, sx) } else { (sx, sy) };
    if r.w == w && r.h == h {
        return Some(out);
    }
    if r.w == w {
        r.y += h;
        r.h -= h;
        add_free(free, r);
        return Some(out);
    }
    if r.h == h {
        r.x += w;
        r.w -= w;
        add_free(free, r);
        return Some(out);
    }
    if rotated {
        let b1 = u32::from(r.h - sx) * u32::from(r.w);
        let b2 = u32::from(r.w - sy) * u32::from(r.h);
        let mut nr = r;
        nr.y += sx;
        r.x += sy;
        if b1 >= b2 {
            nr.w = sy;
            nr.h -= sx;
            r.w -= sy;
        } else {
            nr.h -= sx;
            r.h = sx;
            r.w -= sy;
        }
        add_free(free, r);
        add_free(free, nr);
    } else {
        let a1 = u32::from(r.w - sx) * u32::from(r.h);
        let a2 = u32::from(r.h - sy) * u32::from(r.w);
        // In mode one the rotated dimensions need not fit; native arithmetic
        // computes signed intermediates before choosing this branch.
        let a3 = (i32::from(r.h) - i32::from(sx)) * i32::from(r.w);
        let a4 = (i32::from(r.w) - i32::from(sy)) * i32::from(r.h);
        let m = (a1 as i32).max(a2 as i32).max(a3).max(a4);
        let mut nr = r;
        if mode == 1 || m == a1 as i32 || m == a2 as i32 {
            nr.y += sy;
            r.x += sx;
            if m == a1 as i32 {
                nr.w = sx;
                nr.h -= sy;
                r.w -= sx;
            } else {
                nr.h -= sy;
                r.h = sy;
                r.w -= sx;
            }
        } else {
            rotated = true;
            nr.y += sx;
            r.x += sy;
            if m == a3 {
                nr.w = sy;
                nr.h -= sx;
                r.w -= sy;
            } else {
                nr.h -= sx;
                r.h = sx;
                r.w -= sy;
            }
        }
        add_free(free, r);
        add_free(free, nr);
    }
    Some(LightmapPlacement { rotated, ..out })
}
fn try_page(
    width: u16,
    height: u16,
    initial: &[FreeRect],
    rects: &[LightmapRectangle],
    order: &[usize],
) -> Option<(Vec<FreeRect>, Vec<LightmapPlacement>)> {
    let mut free = initial.to_vec();
    let mut offsets = vec![
        LightmapPlacement {
            x: 0,
            y: 0,
            rotated: false,
            width: 0,
            height: 0
        };
        rects.len()
    ];
    // An empty free list on a new candidate initially means the whole texture.
    // Full shared pages are skipped by their zero free-area check.
    if free.is_empty() {
        free.push(FreeRect {
            x: 0,
            y: 0,
            w: width,
            h: height,
        });
    }
    for &i in order {
        if free.is_empty() {
            return None;
        }
        offsets[i] = place(&mut free, rects[i].width, rects[i].height)?;
    }
    Some((free, offsets))
}
impl LightmapAtlasPool {
    pub fn pack(
        &mut self,
        rects: &[LightmapRectangle],
    ) -> Result<
        (
            LightmapAtlasOwner,
            Vec<LightmapPlacement>,
            LightmapAllocationLease,
        ),
        AtlasError,
    > {
        let mut state = self.state.lock().map_err(|_| AtlasError::Poisoned)?;
        if state.poisoned {
            return Err(AtlasError::Poisoned);
        }
        state.pages.retain(|p| p.texture.strong_count() != 0);
        if rects.is_empty() || rects.len() > u16::MAX as usize {
            return Err(AtlasError::InvalidRectangle);
        }
        let mut area = 0u32;
        for r in rects {
            let n = usize::from(r.width) * usize::from(r.height);
            if r.width < 3 || r.height < 3 || r.width > 32767 || r.height > 32767 {
                return Err(AtlasError::InvalidRectangle);
            }
            match &r.texels {
                LightmapTexels::Single(_) if r.width == 3 && r.height == 3 => (),
                LightmapTexels::Rgb(v) if v.len() == n * 3 && !(r.width == 3 && r.height == 3) => {}
                _ => return Err(AtlasError::InvalidRectangle),
            }
            area = area.checked_add(n as u32).ok_or(AtlasError::Quota)?;
        }
        // Native ties select the last still-unmarked input rectangle.
        let mut order: Vec<_> = (0..rects.len()).collect();
        order.sort_by_key(|&i| {
            (
                std::cmp::Reverse(u32::from(rects[i].width) * u32::from(rects[i].height)),
                std::cmp::Reverse(i),
            )
        });
        let mut selected = None;
        for (i, p) in state.pages.iter().enumerate() {
            if area > p.free_area() {
                continue;
            }
            if let Some((free, offsets)) = try_page(p.width, p.height, &p.free, rects, &order) {
                selected = Some((Some(i), p.width, p.height, free, offsets));
                break;
            }
        }
        if selected.is_none() {
            let mut best_area = u32::MAX;
            for height in [128u16, 256, 512] {
                for width in [128u16, 256, 512] {
                    let size = u32::from(width) * u32::from(height);
                    if size < area {
                        continue;
                    }
                    if let Some((free, offsets)) = try_page(width, height, &[], rects, &order) {
                        let free_area: u32 = free.iter().map(|r| r.area()).sum();
                        if free_area < best_area {
                            best_area = free_area;
                            selected = Some((None, width, height, free, offsets));
                        }
                    }
                }
            }
        }
        let (existing, width, height, free, offsets) =
            selected.ok_or(AtlasError::ResizeRequired)?;
        let texture = if let Some(i) = existing {
            state
                .pages
                .remove(i)
                .texture
                .upgrade()
                .ok_or(AtlasError::Poisoned)?
        } else {
            if state.pages.len() >= 4096 {
                return Err(AtlasError::Quota);
            }
            let mut rgba = Vec::new();
            rgba.try_reserve_exact(usize::from(width) * usize::from(height) * 4)
                .map_err(|_| AtlasError::HostAllocation)?;
            rgba.resize(usize::from(width) * usize::from(height) * 4, 255);
            Arc::new(RwLock::new(LightmapAtlas {
                width: width.into(),
                height: height.into(),
                rgba,
                revision: 0,
            }))
        };
        {
            let mut atlas = texture.write().map_err(|_| AtlasError::Poisoned)?;
            for &i in &order {
                let r = &rects[i];
                let p = offsets[i];
                for y in 0..r.height {
                    for x in 0..r.width {
                        let (dx, dy) = if p.rotated {
                            (p.x + y, p.y + x)
                        } else {
                            (p.x + x, p.y + y)
                        };
                        let at = (usize::from(dy) * usize::from(width) + usize::from(dx)) * 4;
                        let rgb = match &r.texels {
                            LightmapTexels::Single(c) => {
                                [(c >> 16) as u8, (c >> 8) as u8, *c as u8]
                            }
                            LightmapTexels::Rgb(v) => {
                                let j =
                                    (usize::from(y) * usize::from(r.width) + usize::from(x)) * 3;
                                [v[j], v[j + 1], v[j + 2]]
                            }
                        };
                        atlas.rgba[at..at + 3].copy_from_slice(&rgb);
                    }
                }
            }
            atlas.revision = atlas.revision.checked_add(1).ok_or(AtlasError::Quota)?;
        }
        let page = Page {
            width,
            height,
            free,
            texture: Arc::downgrade(&texture),
        };
        let at = state
            .pages
            .iter()
            .position(|p| p.free_area() >= page.free_area())
            .unwrap_or(state.pages.len());
        state.pages.insert(at, page);
        let release = LightmapAllocationLease {
            _release: Arc::new(AllocationRelease {
                pool: Arc::downgrade(&self.state),
                texture: Arc::downgrade(&texture),
                placements: order.iter().map(|&i| offsets[i]).collect(),
            }),
        };
        Ok((texture, offsets, release))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_share_texture_ownership_and_do_not_overwrite_prior_rectangles() {
        let mut pool = LightmapAtlasPool::default();
        let one = |color| {
            vec![LightmapRectangle {
                width: 3,
                height: 3,
                texels: LightmapTexels::Single(color),
            }]
        };
        let (first, a, first_lease) = pool.pack(&one(0xff123456)).unwrap();
        let (second, b, second_lease) = pool.pack(&one(0xff789abc)).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_ne!((a[0].x, a[0].y), (b[0].x, b[0].y));
        let atlas = first.read().unwrap();
        assert_eq!(&atlas.rgba[0..4], &[0x12, 0x34, 0x56, 255]);
        assert_eq!(atlas.revision, 2);
        drop(atlas);
        drop(pool);
        assert_eq!(Arc::strong_count(&first), 2);
        drop((first_lease, second_lease));
    }
    #[test]
    fn resize_is_an_explicit_unimplemented_dependency() {
        let mut pool = LightmapAtlasPool::default();
        let r = LightmapRectangle {
            width: 513,
            height: 3,
            texels: LightmapTexels::Rgb(vec![0; 513 * 3 * 3]),
        };
        assert!(matches!(pool.pack(&[r]), Err(AtlasError::ResizeRequired)));
    }
    #[test]
    fn destroyed_lightmap_returns_rectangles_and_last_texture_drops_without_cache_owner() {
        let mut pool = LightmapAtlasPool::default();
        let rect = |color| {
            [LightmapRectangle {
                width: 3,
                height: 3,
                texels: LightmapTexels::Single(color),
            }]
        };
        let (first, a, first_lease) = pool.pack(&rect(0xff123456)).unwrap();
        let (second, _, second_lease) = pool.pack(&rect(0xffabcdef)).unwrap();
        let weak = Arc::downgrade(&first);
        drop(first);
        drop(first_lease);
        assert_eq!(&second.read().unwrap().rgba[..4], &[255; 4]);
        let (third, c, third_lease) = pool.pack(&rect(0xff789abc)).unwrap();
        assert_eq!((a[0].x, a[0].y), (c[0].x, c[0].y));
        drop((second, second_lease, third, third_lease));
        assert_eq!(weak.strong_count(), 0);
        assert!(pool.state.lock().unwrap().pages.is_empty());
    }
}
