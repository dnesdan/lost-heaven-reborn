//! Saved-lightmap truecolor atlas ownership, following mafia-re 29b65b0.
//!
//! Source: I3D/mesh/lightmap.cpp (free lists, packing, m_CreateLM and dtor).
//! This host profile is A8R8G8B8-equivalent RGBA, maximum 512 in each axis.
//! It does not implement native ITexture::Create/Lock or device failure paths.
//! Each allocation is an actual owner, including a failed construction prefix;
//! callers must release it explicitly. No Drop callback rolls mutations back.
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "lightmap_resize.rs"]
mod resize;
pub use resize::LightmapResizeError;

static NEXT_REGISTRY: AtomicU64 = AtomicU64::new(1);
const MIN_ATLAS: u16 = 128;
const MAX_ATLAS: u16 = 512;
const HOST_INPUT_LIMIT: usize = 256 * 1024 * 1024;

macro_rules! key {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name {
            registry: u64,
            slot: u32,
        }
    };
}
key!(LightmapAtlasKey);
key!(LightmapAllocationKey);

#[derive(Debug, PartialEq, Eq)]
pub enum RectangleInput {
    SingleColor {
        argb: u32,
    },
    Rgb {
        width: u16,
        height: u16,
        pixels: Box<[u8]>,
    },
}
impl RectangleInput {
    fn size(&self) -> [u16; 2] {
        match self {
            Self::SingleColor { .. } => [3, 3],
            Self::Rgb { width, height, .. } => [*width, *height],
        }
    }
    fn bytes(&self) -> usize {
        match self {
            Self::SingleColor { .. } => 0,
            Self::Rgb { pixels, .. } => pixels.len(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RectanglePlacement {
    pub texture_size: [u16; 2],
    pub offset: [u16; 2],
    pub rotated: bool,
}
impl RectanglePlacement {
    /// Source RECTANGLE_INFO u/v, including u += 2 for a rotated RGB rectangle.
    /// Atlas dimensions are the verified powers of two of the borrowed image.
    pub fn source_uv_origin(self, atlas_size: [u32; 2]) -> [f32; 2] {
        [
            self.offset[0] as f32 / atlas_size[0] as f32 + if self.rotated { 2.0 } else { 0.0 },
            self.offset[1] as f32 / atlas_size[1] as f32,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightmapAtlasError {
    IdentityExhausted,
    HostAllocation,
    HostInputBudget,
    TooManyRectangles,
    UnsupportedDimensions { rectangle: u32, size: [u16; 2] },
    InvalidRgbLength { rectangle: u32 },
    ExtentOverflow,
    NoFit,
    ForeignKey,
    StaleAllocation,
    StaleAtlas,
    IncompleteAllocation,
    Invariant,
    Resize(LightmapResizeError),
}
impl From<LightmapResizeError> for LightmapAtlasError {
    fn from(value: LightmapResizeError) -> Self {
        Self::Resize(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocationPhase {
    Preparing,
    Ready,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightmapAllocationFailure {
    pub allocation: Option<LightmapAllocationKey>,
    pub error: LightmapAtlasError,
}
#[derive(Debug)]
pub struct LightmapAllocationView<'a> {
    pub atlas: LightmapAtlasKey,
    pub texture: &'a lhr_assets::Texture,
    pub placements: &'a [RectanglePlacement],
    pub source_memory_bytes: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationDiagnostics {
    pub phase: AllocationPhase,
    pub atlas: Option<LightmapAtlasKey>,
    pub rectangles: usize,
    pub source_memory_bytes: u32,
    pub area: i32,
    pub shrink_passes: u32,
    pub retained_rgb_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasDiagnostics {
    pub references: u32,
    pub free_area: i32,
    pub lost_area: i32,
    pub free_rectangles: usize,
    pub revision: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtlasStatistics {
    pub allocations: usize,
    pub failed_allocations: usize,
    pub atlases: usize,
    pub rgba_bytes: usize,
}

#[derive(Debug)]
struct Allocation {
    phase: AllocationPhase,
    atlas: Option<LightmapAtlasKey>,
    rectangles: Vec<RectangleInput>,
    sizes: Vec<[u16; 2]>,
    converted: Vec<Option<Box<[u8]>>>,
    placements: Vec<RectanglePlacement>,
    order: Vec<u16>,
    area: i32,
    memory: u32,
    shrink_passes: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FreeRect {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}
impl FreeRect {
    fn area(self) -> i32 {
        self.width * self.height
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Offset {
    x: u16,
    y: u16,
    rotated: bool,
}
#[derive(Debug)]
struct TexBlock {
    size: [u16; 2],
    free_area: i32,
    lost_area: i32,
    free: Vec<FreeRect>,
    offsets: Vec<Offset>,
}
#[derive(Debug)]
struct Atlas {
    texture: lhr_assets::Texture,
    block: TexBlock,
    references: u32,
    revision: u64,
}
#[derive(Debug)]
pub struct LightmapAtlasRegistry {
    incarnation: u64,
    allocations: Vec<Option<Allocation>>,
    atlases: Vec<Option<Atlas>>,
    remaining: Vec<LightmapAtlasKey>,
}

fn reserve<T>(v: &mut Vec<T>, additional: usize) -> Result<(), LightmapAtlasError> {
    v.try_reserve_exact(additional)
        .map_err(|_| LightmapAtlasError::HostAllocation)
}

impl TexBlock {
    fn new(size: [u16; 2]) -> Self {
        Self {
            size,
            free_area: 0,
            lost_area: 0,
            free: Vec::new(),
            offsets: Vec::new(),
        }
    }
    fn duplicate(&self) -> Result<Self, LightmapAtlasError> {
        let mut free = Vec::new();
        reserve(&mut free, self.free.len())?;
        free.extend_from_slice(&self.free);
        Ok(Self {
            size: self.size,
            free_area: self.free_area,
            lost_area: self.lost_area,
            free,
            offsets: Vec::new(),
        })
    }
    /// Source AddRect/ReplaceRect: dimensions under 3 are irretrievably lost;
    /// equal areas precede existing equal entries. Capacity is reserved by caller.
    fn add_rect(&mut self, rect: FreeRect) {
        if rect.width < 3 || rect.height < 3 {
            self.lost_area += rect.area();
        } else {
            self.free_area += rect.area();
            let position = self
                .free
                .iter()
                .position(|r| r.area() >= rect.area())
                .unwrap_or(self.free.len());
            self.free.insert(position, rect);
        }
    }
    /// Exact source m_AddTexture branch/order, with local trial ownership.
    fn add_texture(&mut self, size: [u16; 2]) -> Result<bool, LightmapAtlasError> {
        reserve(&mut self.free, 2)?;
        reserve(&mut self.offsets, 1)?;
        let [sx, sy] = size.map(i32::from);
        if self.free.is_empty() {
            self.add_rect(FreeRect {
                x: 0,
                y: 0,
                width: i32::from(self.size[0]),
                height: i32::from(self.size[1]),
            });
        }
        let found = self.free.iter().enumerate().find_map(|(i, r)| {
            if r.width >= sx && r.height >= sy {
                let mode = if (r.height >= sx && r.width == sy) || (r.height == sx && r.width >= sy)
                {
                    3
                } else if r.height >= sx && r.width >= sy {
                    2
                } else {
                    1
                };
                Some((i, mode))
            } else if r.height >= sx && r.width >= sy {
                Some((i, 3))
            } else {
                None
            }
        });
        let Some((position, mode)) = found else {
            return Ok(false);
        };
        let mut r = self.free.remove(position);
        self.free_area -= r.area();
        let offset_index = self.offsets.len();
        self.offsets.push(Offset {
            x: r.x as u16,
            y: r.y as u16,
            rotated: mode == 3,
        });
        if mode == 3 {
            if r.width == sy && r.height == sx {
                return Ok(true);
            }
            if r.width == sy {
                r.y += sx;
                r.height -= sx;
                self.add_rect(r);
                return Ok(true);
            }
            if r.height == sx {
                r.x += sy;
                r.width -= sy;
                self.add_rect(r);
                return Ok(true);
            }
            let b1 = (r.height - sx) * r.width;
            let b2 = (r.width - sy) * r.height;
            let mut nr = r;
            nr.y += sx;
            r.x += sy;
            if b1 >= b2 {
                nr.width = sy;
                nr.height -= sx;
                r.width -= sy;
            } else {
                nr.height -= sx;
                r.height = sx;
                r.width -= sy;
            }
            self.add_rect(r);
            self.add_rect(nr);
        } else {
            if r.width == sx && r.height == sy {
                return Ok(true);
            }
            if r.width == sx {
                r.y += sy;
                r.height -= sy;
                self.add_rect(r);
                return Ok(true);
            }
            if r.height == sy {
                r.x += sx;
                r.width -= sx;
                self.add_rect(r);
                return Ok(true);
            }
            let a1 = (r.width - sx) * r.height;
            let a2 = (r.height - sy) * r.width;
            let a3 = (r.height - sx) * r.width;
            let a4 = (r.width - sy) * r.height;
            let m = a1.max(a2.max(a3.max(a4)));
            let mut nr = r;
            if mode == 1 || m == a1 || m == a2 {
                nr.y += sy;
                r.x += sx;
                if m == a1 {
                    nr.width = sx;
                    nr.height -= sy;
                    r.width -= sx;
                } else {
                    nr.height -= sy;
                    r.height = sy;
                    r.width -= sx;
                }
            } else {
                nr.y += sx;
                r.x += sy;
                if m == a3 {
                    nr.width = sy;
                    nr.height -= sx;
                    r.width -= sy;
                } else {
                    nr.height -= sx;
                    r.height = sx;
                    r.width -= sy;
                }
                self.offsets[offset_index].rotated = true;
            }
            self.add_rect(r);
            self.add_rect(nr);
        }
        Ok(true)
    }
}

impl LightmapAtlasRegistry {
    pub fn new() -> Result<Self, LightmapAtlasError> {
        let incarnation = NEXT_REGISTRY
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| LightmapAtlasError::IdentityExhausted)?;
        Ok(Self {
            incarnation,
            allocations: Vec::new(),
            atlases: Vec::new(),
            remaining: Vec::new(),
        })
    }
    fn allocation_index(&self, key: LightmapAllocationKey) -> Result<usize, LightmapAtlasError> {
        if key.registry != self.incarnation {
            return Err(LightmapAtlasError::ForeignKey);
        }
        let i = key
            .slot
            .checked_sub(1)
            .ok_or(LightmapAtlasError::StaleAllocation)? as usize;
        if self.allocations.get(i).is_none_or(Option::is_none) {
            return Err(LightmapAtlasError::StaleAllocation);
        }
        Ok(i)
    }
    fn atlas_index(&self, key: LightmapAtlasKey) -> Result<usize, LightmapAtlasError> {
        if key.registry != self.incarnation {
            return Err(LightmapAtlasError::ForeignKey);
        }
        let i = key
            .slot
            .checked_sub(1)
            .ok_or(LightmapAtlasError::StaleAtlas)? as usize;
        if self.atlases.get(i).is_none_or(Option::is_none) {
            return Err(LightmapAtlasError::StaleAtlas);
        }
        Ok(i)
    }
    pub fn texture(
        &self,
        key: LightmapAtlasKey,
    ) -> Result<&lhr_assets::Texture, LightmapAtlasError> {
        Ok(&self.atlases[self.atlas_index(key)?]
            .as_ref()
            .unwrap()
            .texture)
    }
    pub fn allocation(
        &self,
        key: LightmapAllocationKey,
    ) -> Result<LightmapAllocationView<'_>, LightmapAtlasError> {
        let a = self.allocations[self.allocation_index(key)?]
            .as_ref()
            .unwrap();
        if a.phase != AllocationPhase::Ready {
            return Err(LightmapAtlasError::IncompleteAllocation);
        }
        let atlas = a.atlas.ok_or(LightmapAtlasError::Invariant)?;
        Ok(LightmapAllocationView {
            atlas,
            texture: self.texture(atlas)?,
            placements: &a.placements,
            source_memory_bytes: a.memory,
        })
    }
    pub fn diagnostics(
        &self,
        key: LightmapAllocationKey,
    ) -> Result<AllocationDiagnostics, LightmapAtlasError> {
        let a = self.allocations[self.allocation_index(key)?]
            .as_ref()
            .unwrap();
        Ok(AllocationDiagnostics {
            phase: a.phase,
            atlas: a.atlas,
            rectangles: a.rectangles.len(),
            source_memory_bytes: a.memory,
            area: a.area,
            shrink_passes: a.shrink_passes,
            retained_rgb_bytes: a
                .rectangles
                .iter()
                .map(RectangleInput::bytes)
                .sum::<usize>()
                + a.converted.iter().flatten().map(|p| p.len()).sum::<usize>(),
        })
    }
    pub fn atlas_diagnostics(
        &self,
        key: LightmapAtlasKey,
    ) -> Result<AtlasDiagnostics, LightmapAtlasError> {
        let a = self.atlases[self.atlas_index(key)?].as_ref().unwrap();
        Ok(AtlasDiagnostics {
            references: a.references,
            free_area: a.block.free_area,
            lost_area: a.block.lost_area,
            free_rectangles: a.block.free.len(),
            revision: a.revision,
        })
    }
    pub fn statistics(&self) -> AtlasStatistics {
        let mut s = AtlasStatistics::default();
        for a in self.allocations.iter().flatten() {
            s.allocations += 1;
            s.failed_allocations += usize::from(a.phase == AllocationPhase::Failed);
        }
        for a in self.atlases.iter().flatten() {
            s.atlases += 1;
            s.rgba_bytes += a.texture.rgba.len();
        }
        s
    }
    pub fn allocate(
        &mut self,
        rectangles: Vec<RectangleInput>,
    ) -> Result<LightmapAllocationKey, LightmapAllocationFailure> {
        let mut a = Allocation {
            phase: AllocationPhase::Preparing,
            atlas: None,
            rectangles,
            sizes: Vec::new(),
            converted: Vec::new(),
            placements: Vec::new(),
            order: Vec::new(),
            area: 0,
            memory: 0,
            shrink_passes: 0,
        };
        let slot = u32::try_from(self.allocations.len())
            .ok()
            .and_then(|v| v.checked_add(1))
            .ok_or(LightmapAllocationFailure {
                allocation: None,
                error: LightmapAtlasError::IdentityExhausted,
            })?;
        reserve(&mut self.allocations, 1).map_err(|error| LightmapAllocationFailure {
            allocation: None,
            error,
        })?;
        let key = LightmapAllocationKey {
            registry: self.incarnation,
            slot,
        };
        self.allocations.push(None);
        let result = self.construct(&mut a);
        a.phase = if result.is_ok() {
            AllocationPhase::Ready
        } else {
            AllocationPhase::Failed
        };
        self.allocations[(slot - 1) as usize] = Some(a);
        result
            .map(|()| key)
            .map_err(|error| LightmapAllocationFailure {
                allocation: Some(key),
                error,
            })
    }
    fn construct(&mut self, a: &mut Allocation) -> Result<(), LightmapAtlasError> {
        let count = a.rectangles.len();
        if count > u16::MAX as usize {
            return Err(LightmapAtlasError::TooManyRectangles);
        }
        reserve(&mut a.sizes, count)?;
        reserve(&mut a.converted, count)?;
        reserve(&mut a.placements, count)?;
        reserve(&mut a.order, count)?;
        let mut source_bytes = 0usize;
        for (index, rectangle) in a.rectangles.iter().enumerate() {
            let size = rectangle.size();
            if let RectangleInput::Rgb { pixels, .. } = rectangle {
                if size.into_iter().any(|v| v < 4 || v > i16::MAX as u16) {
                    return Err(LightmapAtlasError::UnsupportedDimensions {
                        rectangle: index as u32,
                        size,
                    });
                }
                if pixels.len() != usize::from(size[0]) * usize::from(size[1]) * 3 {
                    return Err(LightmapAtlasError::InvalidRgbLength {
                        rectangle: index as u32,
                    });
                }
                source_bytes = source_bytes
                    .checked_add(pixels.len())
                    .ok_or(LightmapAtlasError::ExtentOverflow)?;
                if source_bytes > HOST_INPUT_LIMIT {
                    return Err(LightmapAtlasError::HostInputBudget);
                }
            }
            a.area = a
                .area
                .checked_add(i32::from(size[0]) * i32::from(size[1]))
                .ok_or(LightmapAtlasError::ExtentOverflow)?;
            a.sizes.push(size);
            a.converted.push(None);
            a.placements.push(RectanglePlacement::default());
            a.order.push(index as u16);
        }
        // All admitted rectangle u values start at zero. This total ordering
        // equals the source repeated max scan with >= (last equal wins).
        a.order.sort_unstable_by_key(|&i| {
            let size = a.sizes[usize::from(i)];
            std::cmp::Reverse((i32::from(size[0]) * i32::from(size[1]), i))
        });
        let mut selected = None;
        for &atlas in &self.remaining {
            let existing = self.atlases[self.atlas_index(atlas)?].as_ref().unwrap();
            if a.area > existing.block.free_area {
                continue;
            }
            let mut trial = existing.block.duplicate()?;
            let mut fits = true;
            for &index in &a.order {
                let size = a.sizes[usize::from(index)];
                if trial.free_area < i32::from(size[0]) * i32::from(size[1])
                    || !trial.add_texture(size)?
                {
                    fits = false;
                    break;
                }
            }
            if fits {
                selected = Some((Some(atlas), trial));
                break;
            }
        }
        while selected.is_none() {
            let mut best: Option<TexBlock> = None;
            for height in [MIN_ATLAS, 256, MAX_ATLAS] {
                for width in [MIN_ATLAS, 256, MAX_ATLAS] {
                    if a.area > i32::from(width) * i32::from(height) {
                        continue;
                    }
                    let mut trial = TexBlock::new([width, height]);
                    let mut fits = true;
                    for &index in &a.order {
                        if !trial.add_texture(a.sizes[usize::from(index)])? {
                            fits = false;
                            break;
                        }
                    }
                    if fits && best.as_ref().is_none_or(|b| trial.free_area < b.free_area) {
                        best = Some(trial);
                    }
                }
            }
            if let Some(best) = best {
                selected = Some((None, best));
                break;
            }
            let old_area = a.area;
            a.shrink_passes += 1;
            for &index in &a.order {
                let index = usize::from(index);
                let [width, height] = a.sizes[index];
                if width >= 4 && height >= 4 {
                    a.area -= i32::from(width) * i32::from(height);
                    let size = [(width >> 1).max(4), (height >> 1).max(4)];
                    let RectangleInput::Rgb {
                        width,
                        height,
                        pixels,
                    } = &a.rectangles[index]
                    else {
                        return Err(LightmapAtlasError::Invariant);
                    };
                    let converted = resize::resize_rgb_source([*width, *height], pixels, size)?;
                    a.sizes[index] = size;
                    a.area += i32::from(size[0]) * i32::from(size[1]);
                    a.converted[index] = Some(converted);
                }
            }
            if old_area == a.area {
                return Err(LightmapAtlasError::NoFit);
            }
        }
        let (existing, block) = selected.unwrap();
        self.commit(a, existing, block)
    }
    fn insert_remaining(&mut self, key: LightmapAtlasKey) {
        let free_area = self.atlases[(key.slot - 1) as usize]
            .as_ref()
            .unwrap()
            .block
            .free_area;
        let position = self
            .remaining
            .iter()
            .position(|k| {
                self.atlases[(k.slot - 1) as usize]
                    .as_ref()
                    .unwrap()
                    .block
                    .free_area
                    >= free_area
            })
            .unwrap_or(self.remaining.len());
        self.remaining.insert(position, key);
    }
    fn commit(
        &mut self,
        a: &mut Allocation,
        existing: Option<LightmapAtlasKey>,
        mut block: TexBlock,
    ) -> Result<(), LightmapAtlasError> {
        if block.offsets.len() != a.order.len() {
            return Err(LightmapAtlasError::Invariant);
        }
        let memory = u32::try_from(a.area)
            .ok()
            .and_then(|v| v.checked_mul(4))
            .ok_or(LightmapAtlasError::ExtentOverflow)?;
        reserve(&mut self.remaining, 1)?;
        // CPU memory/capacity preparation is host admission. From the actual
        // atlas attachment onward all verified image writes are infallible;
        // native graphics Lock/Create failures are not simulated by this adapter.
        let key = if let Some(key) = existing {
            let index = self.atlas_index(key)?;
            let atlas = self.atlases[index].as_ref().unwrap();
            if atlas.references == u32::MAX || atlas.revision == u64::MAX {
                return Err(LightmapAtlasError::ExtentOverflow);
            }
            let position = self
                .remaining
                .iter()
                .position(|k| *k == key)
                .ok_or(LightmapAtlasError::Invariant)?;
            self.remaining.remove(position);
            self.atlases[index].as_mut().unwrap().references += 1;
            key
        } else {
            let slot = u32::try_from(self.atlases.len())
                .ok()
                .and_then(|v| v.checked_add(1))
                .ok_or(LightmapAtlasError::IdentityExhausted)?;
            reserve(&mut self.atlases, 1)?;
            let size = usize::from(block.size[0]) * usize::from(block.size[1]) * 4;
            let mut rgba = Vec::new();
            reserve(&mut rgba, size)?;
            rgba.resize(size, 255);
            let key = LightmapAtlasKey {
                registry: self.incarnation,
                slot,
            };
            self.atlases.push(Some(Atlas {
                texture: lhr_assets::Texture {
                    width: u32::from(block.size[0]),
                    height: u32::from(block.size[1]),
                    rgba,
                },
                block: TexBlock::new(block.size),
                references: 1,
                revision: 0,
            }));
            key
        };
        a.atlas = Some(key);
        a.memory = memory;
        let atlas = self.atlases[(key.slot - 1) as usize].as_mut().unwrap();
        for (&index, &offset) in a.order.iter().zip(&block.offsets) {
            let index = usize::from(index);
            let size = a.sizes[index];
            let single = matches!(a.rectangles[index], RectangleInput::SingleColor { .. });
            let placement = RectanglePlacement {
                texture_size: size,
                offset: [offset.x, offset.y],
                rotated: offset.rotated && !single,
            };
            a.placements[index] = placement;
            for y in 0..usize::from(size[1]) {
                for x in 0..usize::from(size[0]) {
                    let color = match &a.rectangles[index] {
                        RectangleInput::SingleColor { argb } => {
                            [(*argb >> 16) as u8, (*argb >> 8) as u8, *argb as u8]
                        }
                        RectangleInput::Rgb { pixels, .. } => {
                            let data = a.converted[index].as_deref().unwrap_or(pixels);
                            let at = (y * usize::from(size[0]) + x) * 3;
                            [data[at], data[at + 1], data[at + 2]]
                        }
                    };
                    let (tx, ty) = if placement.rotated { (y, x) } else { (x, y) };
                    let at = ((usize::from(offset.y) + ty) * atlas.texture.width as usize
                        + usize::from(offset.x)
                        + tx)
                        * 4;
                    atlas.texture.rgba[at..at + 3].copy_from_slice(&color);
                }
            }
        }
        block.offsets.clear();
        atlas.block = block;
        atlas.revision += 1;
        self.insert_remaining(key);
        // Source m_CreateLM:931..939 frees RGB and converted storage once
        // uploaded (all admitted nonsingle rectangle areas exceed 9).
        for rectangle in &mut a.rectangles {
            if let RectangleInput::Rgb { pixels, .. } = rectangle {
                *pixels = Box::default();
            }
        }
        a.converted.iter_mut().for_each(|p| *p = None);
        Ok(())
    }
    pub fn release(&mut self, key: LightmapAllocationKey) -> Result<(), LightmapAtlasError> {
        let index = self.allocation_index(key)?;
        let allocation = self.allocations[index].as_ref().unwrap();
        let Some(atlas_key) = allocation.atlas else {
            self.allocations[index] = None;
            return Ok(());
        };
        let atlas_index = self.atlas_index(atlas_key)?;
        let atlas = self.atlases[atlas_index].as_mut().unwrap();
        if atlas.references == 0 {
            return Err(LightmapAtlasError::Invariant);
        }
        let position = self
            .remaining
            .iter()
            .position(|k| *k == atlas_key)
            .ok_or(LightmapAtlasError::Invariant)?;
        if atlas.references > 1 {
            if atlas.revision == u64::MAX {
                return Err(LightmapAtlasError::ExtentOverflow);
            }
            reserve(&mut atlas.block.free, allocation.order.len())?;
        }
        // After preflight there are no fallible host operations. Effects follow
        // source Release, rectangle return/whitening, and free-area reinsertion.
        self.remaining.remove(position);
        atlas.references -= 1;
        if atlas.references == 0 {
            self.atlases[atlas_index] = None;
        } else {
            for &i in &allocation.order {
                let p = allocation.placements[usize::from(i)];
                let [width, height] = if p.rotated {
                    [p.texture_size[1], p.texture_size[0]]
                } else {
                    p.texture_size
                };
                atlas.block.add_rect(FreeRect {
                    x: i32::from(p.offset[0]),
                    y: i32::from(p.offset[1]),
                    width: i32::from(width),
                    height: i32::from(height),
                });
                for y in usize::from(p.offset[1])..usize::from(p.offset[1] + height) {
                    let start = (y * atlas.texture.width as usize + usize::from(p.offset[0])) * 4;
                    atlas.texture.rgba[start..start + usize::from(width) * 4].fill(255);
                }
            }
            atlas.revision += 1;
            self.insert_remaining(atlas_key);
        }
        self.allocations[index] = None;
        Ok(())
    }
}

#[cfg(test)]
#[path = "lightmap_atlas_tests.rs"]
mod tests;
