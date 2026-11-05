//! CPU true-color projection of LightMap packing. The free-rectangle ordering,
//! rotation and candidate selection follow pinned mafia-re 29b65b0
//! lightmap.cpp. RGB/filter resize uses the separately bounded pinned-source
//! arithmetic profile. Native 16-bit conversion/dither and compression remain
//! separate dependencies; unshrinkable exhaustion reports a real frontier.
#[path = "lightmap_resize.rs"]
mod resize;
pub use resize::LightmapResizeError;
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
    Resize(LightmapResizeError),
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
    live_allocations: usize,
    poisoned: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtlasStatistics {
    pub live_pages: usize,
    pub live_allocations: usize,
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
        state.live_allocations -= 1;
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
    sizes: &[[u16; 2]],
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
        sizes.len()
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
        offsets[i] = place(&mut free, sizes[i][0], sizes[i][1])?;
    }
    Some((free, offsets))
}

/// 0x5d090 admission step (lightmap.cpp:72-154, LM_SINGLE_COLOR_SIZE=3).
/// 3x3 → Ok(None) (keep single color, native `return true`). No
/// minimum-dimension refusal exists natively — 1x1/2x2 convert through the
/// format switch like any other size. The RGB copy below is the 24/32-bit
/// true-color path; 16-bit/dither/compress stay device backends (separate
/// boundary, never silently substituted). Length mismatch reports
/// InvalidRectangle (disclosed hardening — native memcpys `sx*sy*3` bytes
/// unchecked). Contract tier this wave.
pub fn convert_lightmap_texels(w: u16, h: u16, rgb: &[u8]) -> Result<Option<Vec<u8>>, AtlasError> {
    if w == 3 && h == 3 {
        return Ok(None);
    }
    if rgb.len() != usize::from(w) * usize::from(h) * 3 {
        return Err(AtlasError::InvalidRectangle);
    }
    Ok(Some(rgb.to_vec()))
}

/// 0x5e330 RGB→BGR line, pitch bytes apart. Widths u8 from byte loads/stores.
/// Short slices panic (disclosed hardening — native memcpys out of bounds).
/// Panic contract: callers pass `num*3 <= src.len()` and
/// `(num-1)*pitch+3 <= dst.len()`. Contract tier this wave.
pub fn fill_line_rgb_swap(src: &[u8], dst: &mut [u8], num: usize, pitch: usize) {
    for i in 0..num {
        let (r, g, b) = (src[i * 3], src[i * 3 + 1], src[i * 3 + 2]);
        let o = i * pitch;
        dst[o] = b;
        dst[o + 1] = g;
        dst[o + 2] = r;
    }
}

/// Portable free-list rows [x,y,w,h] so overlay symbols stay on pub items
/// (the pack-internal FreeRect path is separate and untouched).
/// 0x5da90: `<3` on either axis → lost (`true`), else area-sorted insert.
/// The native lost_area/free_area accounting stays with the pack-internal
/// path (pool selection there uses the dynamic free_area() sum); this
/// portable row list only orders. Contract tier this wave.
pub fn insert_sorted_free_rect(free: &mut Vec<[u16; 4]>, rect: [u16; 4]) -> bool {
    let [_, _, w, h] = rect;
    if w < 3 || h < 3 {
        return true;
    }
    let area = u32::from(w) * u32::from(h);
    let at = free
        .iter()
        .position(|r| u32::from(r[2]) * u32::from(r[3]) >= area)
        .unwrap_or(free.len());
    free.insert(at, rect);
    true
}

/// 0x5dbb0: unlink done by the caller (Vec::remove); this re-sorts or drops
/// sub-3 leftovers exactly like the native replace arm. Contract tier this wave.
pub fn reinsert_shrunk_free_rect(free: &mut Vec<[u16; 4]>, rect: [u16; 4]) -> bool {
    insert_sorted_free_rect(free, rect)
}

/// 0x5dca0: append copies preserving order. Contract tier this wave.
pub fn duplicate_free_list(src: &[[u16; 4]]) -> Vec<[u16; 4]> {
    src.to_vec()
}

/// 0x5dd40 m_AddTexture, ported arm-for-arm from the reference: first fit in
/// list order; mode 1 = as-is, mode 3 = rotated, mode 2 = as-is unless the
/// max-leftover comparison picks the rotated split (offsets then report
/// rotated). Exact-fit removes the rect; strip fits shrink + re-sort; split
/// fits shrink + re-sort + add (sub-3 leftovers dropped via the insert).
/// Returns [x, y, rotated]. Areas in i32 like the native int arithmetic
/// (and the pack-internal path), with wrapping ops: native 32-bit products
/// wrap identically, and `as u16` packs truncate exactly like the native
/// ushort stores. Branch guards make every subtraction non-negative, so no
/// wrap is reachable on either side at atlas sizes. Contract tier this wave.
pub fn place_lightmap_rect(free: &mut Vec<[u16; 4]>, sx: u16, sy: u16) -> Option<[u16; 3]> {
    let (idx, mode) = free.iter().enumerate().find_map(|(i, r)| {
        let (w, h) = (u32::from(r[2]), u32::from(r[3]));
        let (sx, sy) = (u32::from(sx), u32::from(sy));
        if w >= sx && h >= sy {
            if (h >= sx && w == sy) || (h == sx && w >= sy) {
                Some((i, 3))
            } else if h >= sx && w >= sy {
                Some((i, 2))
            } else {
                Some((i, 1))
            }
        } else if h >= sx && w >= sy {
            Some((i, 3))
        } else {
            None
        }
    })?;
    let r = free.remove(idx);
    let (x, y, w, h) = (
        u32::from(r[0]),
        u32::from(r[1]),
        u32::from(r[2]),
        u32::from(r[3]),
    );
    let (sx, sy) = (u32::from(sx), u32::from(sy));
    let pack = |v: u32| v as u16;
    let pack4 = |r: [u32; 4]| [r[0] as u16, r[1] as u16, r[2] as u16, r[3] as u16];
    if mode == 3 {
        if w == sy && h == sx {
            return Some([pack(x), pack(y), 1]);
        }
        if w == sy {
            // Remaining bottom strip: full width, height h-sx.
            insert_sorted_free_rect(free, [pack(x), pack(y + sx), pack(w), pack(h - sx)]);
            return Some([pack(x), pack(y), 1]);
        }
        if h == sx {
            // Remaining right strip.
            insert_sorted_free_rect(free, [pack(x + sy), pack(y), pack(w - sy), pack(h)]);
            return Some([pack(x), pack(y), 1]);
        }
        let b1 = (h as i32).wrapping_sub(sx as i32).wrapping_mul(w as i32);
        let b2 = (w as i32).wrapping_sub(sy as i32).wrapping_mul(h as i32);
        let (mut kept, mut nr) = ([x, y, w, h], [x, y, w, h]);
        nr[1] += sx;
        kept[0] += sy;
        if b1 >= b2 {
            nr[2] = sy;
            nr[3] -= sx;
            kept[2] -= sy;
        } else {
            nr[3] -= sx;
            kept[3] = sx;
            kept[2] -= sy;
        }
        insert_sorted_free_rect(free, pack4(kept));
        insert_sorted_free_rect(free, pack4(nr));
        return Some([pack(x), pack(y), 1]);
    }
    // Modes 1/2 place as-is; mode 2 may still rotate on max-leftover wins.
    if w == sx && h == sy {
        return Some([pack(x), pack(y), 0]);
    }
    if w == sx {
        insert_sorted_free_rect(free, [pack(x), pack(y + sy), pack(w), pack(h - sy)]);
        return Some([pack(x), pack(y), 0]);
    }
    if h == sy {
        insert_sorted_free_rect(free, [pack(x + sx), pack(y), pack(w - sx), pack(h)]);
        return Some([pack(x), pack(y), 0]);
    }
    let a1 = (w as i32).wrapping_sub(sx as i32).wrapping_mul(h as i32);
    let a2 = (h as i32).wrapping_sub(sy as i32).wrapping_mul(w as i32);
    let a3 = (h as i32).wrapping_sub(sx as i32).wrapping_mul(w as i32);
    let a4 = (w as i32).wrapping_sub(sy as i32).wrapping_mul(h as i32);
    let m = a1.max(a2).max(a3).max(a4);
    let (mut kept, mut nr) = ([x, y, w, h], [x, y, w, h]);
    if mode == 1 || m == a1 || m == a2 {
        nr[1] += sy;
        kept[0] += sx;
        if m == a1 {
            nr[2] = sx;
            nr[3] -= sy;
            kept[2] -= sx;
        } else {
            nr[3] -= sy;
            kept[3] = sy;
            kept[2] -= sx;
        }
        insert_sorted_free_rect(free, pack4(kept));
        insert_sorted_free_rect(free, pack4(nr));
        Some([pack(x), pack(y), 0])
    } else {
        // Mode 2 rotated split: offsets report rotated (native pop/push).
        nr[1] += sx;
        kept[0] += sy;
        if m == a3 {
            nr[2] = sy;
            nr[3] -= sx;
            kept[2] -= sy;
        } else {
            nr[3] -= sx;
            kept[3] = sx;
            kept[2] -= sy;
        }
        insert_sorted_free_rect(free, pack4(kept));
        insert_sorted_free_rect(free, pack4(nr));
        Some([pack(x), pack(y), 1])
    }
}

/// 0x5e3a0 frontend: IGraph::ResizeImage is a backend contract; this
/// validates dims and filters ORIGINAL pixels (never cumulative), like pack.
/// Contract tier this wave.
pub fn resize_lightmap_rgb(
    src_size: [u16; 2],
    rgb: &[u8],
    dst_size: [u16; 2],
) -> Result<Box<[u8]>, AtlasError> {
    resize::resize_rgb_source(src_size, rgb, dst_size).map_err(AtlasError::Resize)
}
impl LightmapAtlasPool {
    /// 0x5d520 ctor: empty texture + sized offset/order lists (host: empty
    /// pages, zero allocations). Contract tier this wave.
    pub fn new() -> Self {
        Self::default()
    }
    /// 0x5d550 destructor remainder: drop dead textures (Release≤0 path),
    /// keep live pages sorted (refill-white + resort path). D3D Release /
    /// Fill / texture-list ops are backend contracts; ownership/order modeled.
    /// Contract tier this wave.
    pub fn retain_live_pages(&mut self) {
        if let Ok(mut s) = self.state.lock() {
            s.pages.retain(|p| p.texture.strong_count() != 0);
            s.pages.sort_by_key(|p| p.free_area());
        }
    }
    /// Counts actual live pages and distinct allocation leases. Weak stale
    /// page slots do not count as owned atlas textures.
    pub fn statistics(&self) -> AtlasStatistics {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        AtlasStatistics {
            live_pages: state
                .pages
                .iter()
                .filter(|page| page.texture.strong_count() != 0)
                .count(),
            live_allocations: state.live_allocations,
        }
    }

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
        let mut sizes: Vec<_> = rects.iter().map(|r| [r.width, r.height]).collect();
        let mut converted: Vec<Option<Box<[u8]>>> = vec![None; rects.len()];
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
            if let Some((free, offsets)) = try_page(p.width, p.height, &p.free, &sizes, &order) {
                selected = Some((Some(i), p.width, p.height, free, offsets));
                break;
            }
        }
        while selected.is_none() {
            let mut best_area = u32::MAX;
            for height in [128u16, 256, 512] {
                for width in [128u16, 256, 512] {
                    let size = u32::from(width) * u32::from(height);
                    if size < area {
                        continue;
                    }
                    if let Some((free, offsets)) = try_page(width, height, &[], &sizes, &order) {
                        let free_area: u32 = free.iter().map(|r| r.area()).sum();
                        if free_area < best_area {
                            best_area = free_area;
                            selected = Some((None, width, height, free, offsets));
                        }
                    }
                }
            }
            if selected.is_some() {
                break;
            }
            let old_area = area;
            // m_OrderList is sorted once, before any shrink. Source dimensions
            // and pixels remain immutable: retries filter the ORIGINAL image.
            for &i in &order {
                let [width, height] = sizes[i];
                if width >= 4 && height >= 4 {
                    let size = [(width >> 1).max(4), (height >> 1).max(4)];
                    let LightmapTexels::Rgb(original) = &rects[i].texels else {
                        return Err(AtlasError::InvalidRectangle);
                    };
                    let pixels = resize::resize_rgb_source(
                        [rects[i].width, rects[i].height],
                        original,
                        size,
                    )
                    .map_err(AtlasError::Resize)?;
                    area -= u32::from(width) * u32::from(height);
                    area += u32::from(size[0]) * u32::from(size[1]);
                    sizes[i] = size;
                    converted[i] = Some(pixels);
                }
            }
            if area == old_area {
                return Err(AtlasError::ResizeRequired);
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
                for y in 0..p.height {
                    for x in 0..p.width {
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
                                let pixels = converted[i].as_deref().unwrap_or(v);
                                let j =
                                    (usize::from(y) * usize::from(p.width) + usize::from(x)) * 3;
                                [pixels[j], pixels[j + 1], pixels[j + 2]]
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
        state.live_allocations += 1;
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
    fn unshrinkable_three_pixel_axis_retains_explicit_exhaustion() {
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
    #[test]
    fn resized_allocations_keep_weak_page_leases_and_filter_original_pixels() {
        let mut pool = LightmapAtlasPool::default();
        let mut original = Vec::new();
        for y in 0..8 {
            for x in 0..1536 {
                original.extend_from_slice(&[
                    ((x * 5 + y * 9) % 256) as u8,
                    ((x * 13 + y * 3) % 256) as u8,
                    ((x * 7 + y * 19) % 256) as u8,
                ]);
            }
        }
        let expected = resize::resize_rgb_source([1536, 8], &original, [384, 4]).unwrap();
        let first = resize::resize_rgb_source([1536, 8], &original, [768, 4]).unwrap();
        let cumulative = resize::resize_rgb_source([768, 4], &first, [384, 4]).unwrap();
        assert_ne!(expected, cumulative);
        let rect = LightmapRectangle {
            width: 1536,
            height: 8,
            texels: LightmapTexels::Rgb(original),
        };
        let (atlas, placements, lease) = pool.pack(&[rect]).unwrap();
        let p = placements[0];
        assert_eq!([p.width, p.height], [384, 4]);
        assert_eq!((p.x, p.y, p.rotated), (0, 0, false));
        let texture = atlas.read().unwrap();
        assert_eq!([texture.width, texture.height], [512, 128]);
        // PR19's independent pinned-C++ source-profile pixel oracle.
        for (x, y, rgb) in [
            (0, 0, [11, 21, 19]),
            (1, 0, [31, 73, 47]),
            (383, 0, [242, 212, 212]),
            (0, 1, [27, 28, 52]),
            (0, 3, [58, 38, 117]),
            (383, 3, [36, 197, 87]),
        ] {
            let at = (y * texture.width as usize + x) * 4;
            assert_eq!(&texture.rgba[at..at + 4], &[rgb[0], rgb[1], rgb[2], 255]);
        }
        drop(texture);
        let weak = Arc::downgrade(&atlas);
        drop((atlas, lease));
        assert_eq!(weak.strong_count(), 0);
        assert!(pool.state.lock().unwrap().pages.is_empty());
    }
    #[test]
    fn original_order_equal_area_last_and_rotated_rgb_are_retained() {
        let mut pool = LightmapAtlasPool::default();
        let mut ramp = Vec::new();
        for y in 0..4 {
            for x in 0..4 {
                ramp.extend_from_slice(&[x * 40, y * 50, 37]);
            }
        }
        let rects = [
            LightmapRectangle {
                width: 4,
                height: 4,
                texels: LightmapTexels::Rgb(ramp),
            },
            LightmapRectangle {
                width: 4,
                height: 4,
                texels: LightmapTexels::Rgb([7, 8, 9].repeat(16)),
            },
        ];
        let (atlas, placements, lease) = pool.pack(&rects).unwrap();
        assert_eq!(
            (placements[0].x, placements[0].y, placements[0].rotated),
            (0, 4, true)
        );
        assert_eq!(
            (placements[1].x, placements[1].y, placements[1].rotated),
            (0, 0, false)
        );
        let texture = atlas.read().unwrap();
        assert_eq!(&texture.rgba[..4], &[7, 8, 9, 255]);
        let at = (6 * texture.width as usize + 1) * 4;
        assert_eq!(&texture.rgba[at..at + 4], &[80, 50, 37, 255]);
        drop(texture);
        drop((atlas, lease));
        assert!(pool.state.lock().unwrap().pages.is_empty());
    }
    #[test]
    fn rgb_three_by_n_keeps_its_existing_admission() {
        let mut pool = LightmapAtlasPool::default();
        let rect = LightmapRectangle {
            width: 3,
            height: 127,
            texels: LightmapTexels::Rgb(vec![91; 3 * 127 * 3]),
        };
        let (atlas, placements, lease) = pool.pack(&[rect]).unwrap();
        assert_eq!([placements[0].width, placements[0].height], [3, 127]);
        drop((atlas, lease));
        assert!(pool.state.lock().unwrap().pages.is_empty());
    }

    #[test]
    fn pin_lightmap_rect_order_and_swap() {
        // RVAs 0x5da90/0x5dbb0/0x5dca0/0x5e330/0x5d090/0x5e3a0: sorted
        // insert, lost-area rule, order-preserving duplicate, BGR swap,
        // 3x3 admission, identity resize.
        let mut free = vec![[0u16, 0, 128, 128]];
        assert!(insert_sorted_free_rect(&mut vec![], [0, 0, 2, 4]));
        insert_sorted_free_rect(&mut free, [10, 10, 4, 4]);
        assert_eq!(free[0], [10, 10, 4, 4]);
        assert_eq!(duplicate_free_list(&free), free);
        let src = vec![1u8, 2, 3, 4, 5, 6];
        let mut dst = vec![0u8; 6];
        fill_line_rgb_swap(&src, &mut dst, 2, 3);
        assert_eq!(dst, vec![3, 2, 1, 6, 5, 4]);
        assert!(convert_lightmap_texels(3, 3, &[0; 27]).unwrap().is_none());
        // No minimum-dimension refusal natively: 1x1/2x2 convert like the rest.
        assert_eq!(
            convert_lightmap_texels(2, 2, &[9; 12]).unwrap(),
            Some(vec![9; 12])
        );
        assert_eq!(
            convert_lightmap_texels(1, 1, &[7; 3]).unwrap(),
            Some(vec![7; 3])
        );
        assert!(convert_lightmap_texels(2, 2, &[9; 11]).is_err());
        let r = resize_lightmap_rgb([4, 4], &[7; 48], [4, 4]).unwrap();
        assert_eq!(&*r, &[7; 48]);
    }
    #[test]
    fn pin_place_modes_and_splits() {
        // RVA 0x5dd40: exact-fit removal, strip shrink + re-sort, max-split
        // choice, rotated reporting.
        let mut free = vec![[0u16, 0, 128, 128]];
        // Exact fit picks the rotated arm (mode 3: w==sy && h==sx).
        assert_eq!(place_lightmap_rect(&mut free, 128, 128), Some([0, 0, 1]));
        assert!(free.is_empty());
        let mut free = vec![[0u16, 0, 128, 128]];
        // 64x128 in 128x128: rotated exact-width strip, bottom remainder.
        assert_eq!(place_lightmap_rect(&mut free, 64, 128), Some([0, 0, 1]));
        assert_eq!(free, vec![[0, 64, 128, 64]]);
        let mut free = vec![[0u16, 0, 64, 128]];
        // 128x64 in 64x128: rotated exact whole-rect consumption.
        assert_eq!(place_lightmap_rect(&mut free, 128, 64), Some([0, 0, 1]));
        assert!(free.is_empty());
        // As-is split (mode 2, max-leftover a2 arm): bottom strip + right piece.
        let mut free = vec![[0u16, 0, 100, 100]];
        assert_eq!(place_lightmap_rect(&mut free, 60, 40), Some([0, 0, 0]));
        assert_eq!(free, vec![[60, 0, 40, 40], [0, 40, 100, 60]]);
        assert_eq!(
            place_lightmap_rect(&mut vec![[0u16, 0, 8, 8]], 16, 16),
            None
        );
    }
    #[test]
    fn pin_pool_ctor_and_retain_order() {
        // RVAs 0x5d520/0x5d550/0x5e3f0(confirm-only): ctor empty, pack
        // tightest page, retain keeps live sorted.
        let mut pool = LightmapAtlasPool::new();
        assert_eq!(pool.statistics().live_pages, 0);
        let rects = [LightmapRectangle {
            width: 4,
            height: 4,
            texels: LightmapTexels::Rgb(vec![9; 48]),
        }];
        let (_owner, pl, _lease) = pool.pack(&rects).unwrap();
        assert_eq!((pl[0].x, pl[0].y, pl[0].rotated), (0, 0, false));
        pool.retain_live_pages();
        // Owner + lease keep the texture alive: exactly one live page.
        assert_eq!(pool.statistics().live_pages, 1);
    }
}
