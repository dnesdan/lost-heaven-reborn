//! Scoped original startup-effect callers from Game.exe SHA256
//! `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`.
//! Native helper names are unknown; the Rust names describe their observed work.
//! The script commands are `game_nightmission`, `enablemap` and `use_lightcache`.
//! Resource lookup and scene traversal are required dependencies, not no-ops.

/// The raw byte touched at mission+0x2d44. The caller, rather than this setter,
/// determines whether an argument is normalized to a boolean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NightMissionState {
    pub enabled_raw: u8,
}

impl NightMissionState {
    /// Original RVA0x7b590 takes the low byte, including non-boolean values.
    pub fn set_night_mission_raw(&mut self, argument: u32) {
        self.enabled_raw = argument as u8;
    }
}

/// A projection of the UI object, not the mission object. The fields correspond
/// to UI+0x46ec and UI+0x46f0; the resource value is an opaque backend token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapState {
    pub enabled_raw: u8,
    pub resource_value: u32,
}

/// Effect item kind codes (mafia-re `game/mission/C_effects.h:39-41`).
pub const EFFECT_ITEM_EFFECT: i32 = 1;
pub const EFFECT_ITEM_PART: i32 = 2;
pub const EFFECT_ITEM_SUBPART: i32 = 3;
/// Table index that scripts emit even with the particles option at Low
/// (mafia-re `game/mission/C_effects.h:46`).
pub const EFFECT_DYMOVNICE: i32 = 0x5a;

/// Opaque effect handle. Native `C_effect*` identity is the caller's concern.
pub type EffectHandle = u32;
/// Opaque effect-part handle. Native `C_effect_part*` identity is the caller's.
pub type EffectPartHandle = u32;
/// Opaque effect-subpart handle. Native `C_effect_subpart*` identity is the caller's.
pub type EffectSubpartHandle = u32;
/// Opaque emitter handle. Native `C_ParticleEmitter*` identity is the caller's.
pub type EmitterHandle = u32;
/// Opaque particle handle. Native `C_Particle*` identity is the caller's.
pub type ParticleHandle = u32;
/// Opaque particle-type handle. Native `C_ParticleType*` identity is the caller.
pub type ParticleTypeHandle = u32;
/// Opaque particle-subpart handle. Native `S_particle_subpart*` identity is the caller's.
pub type ParticleSubpartHandle = u32;

/// Shared base of every effects-table item (mafia-re
/// `game/mission/C_effects.h:48-64`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectItemBase {
    /// `type` at +0x4; zero means no `EFFECT_ITEM_*` kind yet.
    pub kind: i32,
}

impl EffectItemBase {
    /// FUNCTION: GAME RVA 0x44be70, VA 0x84be70 (mafia-re pin 113b5b6,
    /// `game/mission/C_effects.h:56-60`, `C_effect_item::C_effect_item`).
    /// Stores kind zero; the name stays default-empty.
    pub fn new() -> Self {
        Self { kind: 0 }
    }
}

impl Default for EffectItemBase {
    fn default() -> Self {
        Self::new()
    }
}

/// Scalar/vector defaults of one effect part (mafia-re
/// `game/mission/C_effects.h:147-173`). The name, frame and subpart list are
/// owned by the table reader and start empty; only the stored words live here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectPartParams {
    pub kind: i32,
    pub blend: i32,
    pub offset: [f32; 3],
    pub offset_spread: [f32; 3],
    pub dir: [f32; 3],
    pub dir_spread: [f32; 3],
    pub speed: f32,
    pub speed_spread: f32,
    pub scale: [f32; 3],
    pub scale_spread: [f32; 3],
    pub transparency: f32,
    pub transparency_spread: f32,
    pub rate: f32,
    pub burst: i32,
    pub rate_fall: f32,
    pub fall_delay: f32,
    pub repeats: f32,
    pub restart_delay: f32,
    pub unknown_94: f32,
    pub flags: i32,
}

impl EffectPartParams {
    /// FUNCTION: GAME RVA 0x44b820, VA 0x84b820 (mafia-re pin 113b5b6,
    /// `game/mission/C_effects.h:175-198`, `C_effect_part::C_effect_part`).
    /// Kind part; zero vectors; unit scale, unit speed and full transparency;
    /// zero spreads, rates, delays, counts and flags. `burst` is stored last,
    /// after `flags`, as in the original store order.
    pub fn new() -> Self {
        Self {
            kind: EFFECT_ITEM_PART,
            blend: 0,
            offset: [0.0; 3],
            offset_spread: [0.0; 3],
            dir: [0.0; 3],
            dir_spread: [0.0; 3],
            speed: 1.0,
            speed_spread: 0.0,
            scale: [1.0; 3],
            scale_spread: [0.0; 3],
            transparency: 1.0,
            transparency_spread: 0.0,
            rate: 0.0,
            burst: 0,
            rate_fall: 0.0,
            fall_delay: 0.0,
            repeats: 0.0,
            restart_delay: 0.0,
            unknown_94: 0.0,
            flags: 0,
        }
    }
}

impl Default for EffectPartParams {
    fn default() -> Self {
        Self::new()
    }
}

/// FUNCTION: GAME RVA 0x44bef0, VA 0x84bef0 (mafia-re pin 113b5b6,
/// `game/mission/C_effects.h:200-208`, `C_effect_part::~C_effect_part`).
/// Releases subparts from the last one; the relative order of the returned
/// handles is the native deletion order. Actual storage release stays with
/// the caller.
pub fn free_effect_part_children(
    subparts: &mut Vec<EffectSubpartHandle>,
) -> Vec<EffectSubpartHandle> {
    let mut freed = Vec::with_capacity(subparts.len());
    while let Some(child) = subparts.pop() {
        freed.push(child);
    }
    freed
}

/// FUNCTION: GAME RVA 0x44bff0, VA 0x84bff0 (mafia-re pin 113b5b6,
/// `game/mission/C_effects.h:298-306`, `C_effect::~C_effect`).
/// Releases parts from the last one; the relative order of the returned
/// handles is the native deletion order. Actual storage release stays with
/// the caller.
pub fn free_effect_children(parts: &mut Vec<EffectPartHandle>) -> Vec<EffectPartHandle> {
    let mut freed = Vec::with_capacity(parts.len());
    while let Some(child) = parts.pop() {
        freed.push(child);
    }
    freed
}

/// Owned PC effects state (mafia-re `game/mission/C_effects.h:348-365`,
/// `C_effects`). The `free_emitters` list aliases the file-scope
/// `particle_emitters` free pool (GLOBAL GAME 0x63d6e8). The three remaining
/// file-scope free pools keep their own handles in [`EffectsFreePools`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectsState {
    /// Emitters in use with their numbers (`in_use` at +0x0).
    pub in_use: Vec<EmitterHandle>,
    /// Loaded table effects (`effects` at +0x10).
    pub effects: Vec<EffectHandle>,
    /// Emitters started for the static scene (`static_emitters` at +0x20).
    pub static_emitters: Vec<EmitterHandle>,
    /// Recycled emitters (the file-scope `particle_emitters` free pool).
    pub free_emitters: Vec<EmitterHandle>,
    /// Last emitted particle number (`last_guid` at +0x30); -1 means none.
    pub last_guid: i32,
    /// A mission is started (`started` at +0x34).
    pub started: bool,
}

/// File-scope free pools drained by `Mission_Done` (mafia-re
/// `game/mission/C_effects.cpp:27-35`): `particle_pool` (GLOBAL GAME 0x63d718),
/// `particle_parts` (GLOBAL GAME 0x63d708) and `particle_subparts` (GLOBAL
/// GAME 0x63d6f8). Handles are opaque; the particle owners interpret them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EffectsFreePools {
    pub particles: Vec<ParticleHandle>,
    pub parts: Vec<ParticleTypeHandle>,
    pub subparts: Vec<ParticleSubpartHandle>,
}

/// Host table source for construction and mission start. The original reads
/// `tables\effects.tbl` through `C_chunk`; the backend owns that file path.
pub trait EffectsTableBackend {
    /// Append the loaded table effects to `state.effects`, in file order.
    fn load_effects_table(&mut self, state: &mut EffectsState);
}

/// Host emitter effects for the client tick. Emitters stay behind the backend;
/// this module only moves their handles between the owned lists.
pub trait EffectsEmitBackend {
    /// Run one emitter tick; false means the emitter has finished.
    fn tick_emitter(&mut self, emitter: EmitterHandle, time: i32) -> bool;
    /// Return a finished emitter to its idle state.
    fn reset_emitter(&mut self, emitter: EmitterHandle);
}

/// FUNCTION: GAME RVA 0x449080, VA 0x849080 (mafia-re pin 113b5b6,
/// `game/mission/C_effects.cpp:37-42`, `C_effects::C_effects`).
/// Loads the table, then marks the particle number none (-1). The `started`
/// flag is left false here; the native constructor leaves it indeterminate
/// until `Mission_Start` writes it, and false is the host seed.
pub fn effects_create(backend: &mut impl EffectsTableBackend) -> EffectsState {
    let mut state = EffectsState::default();
    backend.load_effects_table(&mut state);
    state.last_guid = -1;
    state
}

/// FUNCTION: GAME RVA 0x449100, VA 0x849100 (mafia-re pin 113b5b6,
/// `game/mission/C_effects.cpp:44-48`, `C_effects::~C_effects`).
/// Runs `Clear` over the loaded effects and reports them in deletion order.
pub fn effects_free(state: &mut EffectsState) -> Vec<EffectHandle> {
    effects_clear(&mut state.effects)
}

/// FUNCTION: GAME RVA 0x449180, VA 0x849180 (mafia-re pin 113b5b6,
/// `game/mission/C_effects.cpp:53-67`, `C_effects::Mission_Start`).
/// Marks unstarted, clears the particle number, loads the table when nothing
/// is loaded yet, marks started and reports true. The native performance
/// counter read has no observable effect and is not modeled.
pub fn effects_mission_start(
    state: &mut EffectsState,
    backend: &mut impl EffectsTableBackend,
) -> bool {
    state.started = false;
    state.last_guid = -1;
    if state.effects.is_empty() {
        backend.load_effects_table(state);
    }
    state.started = true;
    true
}

/// FUNCTION: GAME RVA 0x4491d0, VA 0x8491d0 (mafia-re pin 113b5b6,
/// `game/mission/C_effects.cpp:70-97`, `C_effects::Mission_Done`).
/// Does nothing without a started mission. Otherwise releases the emitters in
/// use first (as `ClearEmitters` does), clears the particle number, releases
/// the static emitters and every free pool entry, and marks unstarted.
/// Returns whether the teardown ran.
pub fn effects_mission_done(
    state: &mut EffectsState,
    pools: &mut EffectsFreePools,
) -> bool {
    if !state.started {
        return false;
    }
    effects_clear_emitters(&mut state.in_use);
    state.last_guid = -1;
    state.static_emitters.clear();
    pools.particles.clear();
    pools.parts.clear();
    pools.subparts.clear();
    state.free_emitters.clear();
    state.started = false;
    true
}

/// FUNCTION: GAME RVA 0x44a350, VA 0x84a350 (mafia-re pin 113b5b6,
/// `game/mission/C_effects.cpp:175-181`, `C_effects::Clear`).
/// Releases every loaded effect from the first one and reports them in that
/// order. Actual storage release stays with the caller.
pub fn effects_clear(effects: &mut Vec<EffectHandle>) -> Vec<EffectHandle> {
    let mut freed = Vec::with_capacity(effects.len());
    for effect in effects.drain(..) {
        freed.push(effect);
    }
    freed
}

/// FUNCTION: GAME RVA 0x44a2f0, VA 0x84a2f0 (mafia-re pin 113b5b6,
/// `game/mission/C_effects.cpp:167-173`, `C_effects::ClearEmitters`).
/// Releases every emitter in use from the first one and reports them in that
/// order. Actual storage release stays with the caller.
pub fn effects_clear_emitters(emitters: &mut Vec<EmitterHandle>) -> Vec<EmitterHandle> {
    let mut freed = Vec::with_capacity(emitters.len());
    for emitter in emitters.drain(..) {
        freed.push(emitter);
    }
    freed
}

/// FUNCTION: GAME RVA 0x44a0d0, VA 0x84a0d0 (mafia-re pin 113b5b6,
/// `game/mission/C_effects.cpp:129-146`, `C_effects::ClientTick`).
/// Ticks every static emitter in order. With no particle started yet
/// (`last_guid == -1`) nothing else runs. Otherwise each finished in-use
/// emitter is reset through the backend and recycled into `free_emitters`;
/// the ordered `erase` re-examines the entry shifted into its place.
pub fn effects_client_tick(
    state: &mut EffectsState,
    time: i32,
    backend: &mut impl EffectsEmitBackend,
) {
    for index in 0..state.static_emitters.len() {
        backend.tick_emitter(state.static_emitters[index], time);
    }
    if state.last_guid == -1 {
        return;
    }
    let mut index = 0;
    while index < state.in_use.len() {
        let emitter = state.in_use[index];
        if !backend.tick_emitter(emitter, time) {
            backend.reset_emitter(emitter);
            state.free_emitters.push(emitter);
            state.in_use.remove(index);
        } else {
            index += 1;
        }
    }
}

pub trait MapResourceBackend {
    type Error;
    /// Bind the resource owner corresponding to the original global object.
    /// The state is live: callback changes and synchronous reentry are visible.
    fn lookup(&mut self, state: &mut MapState, key: u32) -> Result<u32, Self::Error>;
}

/// One physical eight-byte resource-table record. Values are opaque tokens;
/// the original loader and the lifetime of its pointed-to data are separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceRecord {
    pub key: u32,
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceLookupError {
    UnsupportedCount(u32),
    MissingRecord(u32),
}

/// Original lookup at RVA0x20fb40, including its capped initial search step,
/// unsigned comparisons and duplicate-key selection. `raw_count` is the word
/// at owner+4, not a Rust slice length: the routine can read record[raw_count].
/// Supply physical backing records, including any sentinel, and the token for
/// original default data at VA0x638390. This is not a loader or sorting pass.
/// Missing storage and unsupported 32-bit spans are explicit host errors.
pub fn lookup_resource_value(
    raw_count: u32,
    records: &[ResourceRecord],
    key: u32,
    default_value: u32,
) -> Result<u32, ResourceLookupError> {
    if raw_count > 0x1fff_ffff {
        return Err(ResourceLookupError::UnsupportedCount(raw_count));
    }
    let record = |index: u32| {
        records
            .get(index as usize)
            .ok_or(ResourceLookupError::MissingRecord(index))
    };
    let mut step = 0x1000;
    while step > raw_count {
        step >>= 1;
    }
    if step == 0 {
        return Ok(default_value);
    }
    let mut index = if record(step)?.key < key {
        raw_count.wrapping_sub(step).wrapping_sub(1)
    } else {
        0
    };
    step >>= 1;
    while step != 0 {
        let candidate = index.wrapping_add(step);
        step >>= 1;
        if record(candidate)?.key <= key {
            index = candidate;
        }
    }
    let selected = record(index)?;
    Ok(if selected.key == key {
        selected.value
    } else {
        default_value
    })
}

impl MapState {
    /// Original RVA0x1f9d10 stores the byte before looking up key0x5f, including
    /// when disabling the map or setting an unchanged value. The lookup result
    /// overwrites the resource field after callback completion. Adapter failure
    /// does not roll back the already-visible byte or callback mutations.
    pub fn set_map_enabled_raw<B: MapResourceBackend>(
        &mut self,
        backend: &mut B,
        argument: u32,
    ) -> Result<(), B::Error> {
        self.enabled_raw = argument as u8;
        let resource = backend.lookup(self, 0x5f)?;
        self.resource_value = resource;
        Ok(())
    }
}

/// Only the vector header and raw field touched by the light-cache caller.
/// Tokens identify retained storage; resetting the end does not copy, release
/// or read elements. It is not a reconstruction of the native allocator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightCacheHeader {
    pub begin_token: u32,
    pub end_token: u32,
    pub field_178: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneId(pub u32);

/// A retained frame identity. This is not an index into a copied traversal list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightFrameId(pub u32);

/// Raw fields read by the original light collector. `user_data_word` is the
/// first word of non-null frame user data; its meaning is not yet recovered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightFrameView {
    pub user_data_word: Option<u32>,
    pub frame_type: i32,
    pub light_kind: i32,
    pub first_child: Option<LightFrameId>,
    pub next_sibling: Option<LightFrameId>,
}

pub trait LightTraversalBackend {
    type Error;
    /// Resolve valid, live storage without side effects. A snapshot of all
    /// descendants is insufficient: append may change child/sibling links.
    fn frame(&self, id: LightFrameId) -> Result<LightFrameView, Self::Error>;
    /// Append this exact identity to retained cache storage, in call order.
    /// Allocation is a host dependency, not a reconstruction of RVA0x48cc0.
    fn append(&mut self, id: LightFrameId) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LightTraversalError<E> {
    Backend(E),
    /// Added host safety boundary, absent from the original function. Already
    /// appended entries remain visible, including when the graph has a cycle.
    BudgetExhausted,
}

/// Original recursive collector at RVA0x1b5cd0, represented with an explicit
/// host stack. The root is excluded. Nonzero user-data words suppress the
/// entire branch; native type 2 and signed kinds 1..=2 are appended before
/// descendants. Child and sibling links are read after synchronous effects.
/// This does not interpret 4DS file type tags as native frame type values.
pub fn collect_scene_lights<B: LightTraversalBackend>(
    backend: &mut B,
    root: LightFrameId,
    remaining_visits: &mut usize,
) -> Result<(), LightTraversalError<B::Error>> {
    let mut current = backend
        .frame(root)
        .map_err(LightTraversalError::Backend)?
        .first_child;
    let mut ancestors = Vec::new();
    while let Some(id) = current {
        if *remaining_visits == 0 {
            return Err(LightTraversalError::BudgetExhausted);
        }
        *remaining_visits -= 1;
        let view = backend.frame(id).map_err(LightTraversalError::Backend)?;
        if view.user_data_word.unwrap_or(0) == 0 {
            if view.frame_type == 2 && (1..=2).contains(&view.light_kind) {
                backend.append(id).map_err(LightTraversalError::Backend)?;
            }
            if let Some(child) = backend
                .frame(id)
                .map_err(LightTraversalError::Backend)?
                .first_child
            {
                ancestors.push(id);
                current = Some(child);
                continue;
            }
        }
        let mut completed = id;
        loop {
            current = backend
                .frame(completed)
                .map_err(LightTraversalError::Backend)?
                .next_sibling;
            if current.is_some() {
                break;
            }
            match ancestors.pop() {
                Some(parent) => completed = parent,
                None => break,
            }
        }
    }
    Ok(())
}

pub trait LightCacheBackend {
    type Error;
    /// Read the active scene through the live game/context ownership chain.
    /// This accessor must reject unavailable storage and have no side effects.
    fn active_scene(&self) -> Result<SceneId, Self::Error>;
    /// Required synchronous recursive helper at RVA0x1b5cd0. Implementations
    /// must collect real scene data; a recording backend proves ordering only.
    fn collect(&mut self, state: &mut LightCacheHeader, scene: SceneId) -> Result<(), Self::Error>;
}

impl LightCacheHeader {
    /// Original RVA0x1b5c80. Header reset precedes active-scene lookup and the
    /// tail-called traversal; callback writes survive. The apparent native
    /// element-copy loop is unreachable because its initial compare is equal.
    pub fn use_lightcache<B: LightCacheBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), B::Error> {
        self.end_token = self.begin_token;
        self.field_178 = u32::MAX;
        let scene = backend.active_scene()?;
        backend.collect(self, scene)
    }
}

/// Owned result of one light-cache collection: the reset header words plus
/// the collected frame identities in native call order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLightCache {
    pub header: LightCacheHeader,
    pub lights: Vec<LightFrameId>,
}

/// Session-owned snapshot of one scene's light-relevant frame words. The
/// native collector reads the live graph; the snapshot is taken with a
/// shared arena borrow before dispatch so the effect backend needs no arena
/// owner. Graph mutations between the snapshot and the effect are not
/// observed; GameInit dispatch does not mutate it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionLightGraph {
    /// Opaque numeric scene identity, threaded never interpreted.
    pub scene: Option<SceneId>,
    /// Primary sector traversal root, if the scene has a linked sector.
    pub root: Option<LightFrameId>,
    /// Frame views keyed by native frame number.
    pub views: std::collections::HashMap<u32, LightFrameView>,
}

/// Snapshot one scene's light-relevant words: kind-mapped frame types (only
/// kinds 2 and 5 are projected, the collector's only gates), user-data
/// words, point/spot light types and child/sibling links. `None` scene
/// yields an empty graph whose active scene is unavailable.
pub fn snapshot_session_lights(
    arena: &crate::model_loading::ModelArena,
    scene: Option<crate::scene_ownership::SceneKey>,
) -> SessionLightGraph {
    use crate::frame_transform::{FrameKey, FrameKind};
    let mut graph = SessionLightGraph::default();
    let Some(key) = scene else {
        return graph;
    };
    graph.scene = Some(SceneId(key.0 .0));
    let Some(root) = arena.primary_sector_frame(key) else {
        return graph;
    };
    graph.root = Some(LightFrameId(root.0));
    // Depth-first intake over the reachable subtree; the visited set is a
    // host termination guard, the native walk has none.
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![root];
    while let Some(frame) = stack.pop() {
        if !visited.insert(frame.0) {
            continue;
        }
        let Ok(state) = arena.state(frame) else {
            continue;
        };
        let frame_type = match state.kind {
            FrameKind::Light => 2,
            FrameKind::Sector => 5,
            _ => 0,
        };
        let user_data_word = arena.frame_user_data_word(frame).unwrap_or(None);
        let light_kind = match state.kind {
            FrameKind::Light => arena.light(frame).map(|l| l.light_type as i32).unwrap_or(0),
            _ => 0,
        };
        let (first_child, next_sibling) = match arena.links(frame) {
            Ok(links) => (
                links.child.map(|k| LightFrameId(k.0)),
                links.next.map(|k| LightFrameId(k.0)),
            ),
            Err(_) => (None, None),
        };
        graph.views.insert(
            frame.0,
            LightFrameView {
                user_data_word,
                frame_type,
                light_kind,
                first_child,
                next_sibling,
            },
        );
        if let Some(child) = first_child {
            stack.push(FrameKey(child.0));
        }
        if let Some(next) = next_sibling {
            stack.push(FrameKey(next.0));
        }
    }
    graph
}

/// Snapshot-backed traversal for the session light-cache collector.
/// Collection runs the exact `collect_scene_lights` leaf; the visit budget
/// is a host termination bound proportional to the snapshot size.
pub struct GraphLightBackend<'a> {
    graph: &'a SessionLightGraph,
    collected: Vec<LightFrameId>,
}

impl<'a> GraphLightBackend<'a> {
    pub fn new(graph: &'a SessionLightGraph) -> Self {
        Self {
            graph,
            collected: Vec::new(),
        }
    }

    pub fn into_collected(self) -> Vec<LightFrameId> {
        self.collected
    }
}

impl LightTraversalBackend for GraphLightBackend<'_> {
    type Error = String;

    fn frame(&self, id: LightFrameId) -> Result<LightFrameView, Self::Error> {
        self.graph
            .views
            .get(&id.0)
            .copied()
            .ok_or_else(|| format!("session light frame {id:?} outside the snapshot"))
    }

    fn append(&mut self, id: LightFrameId) -> Result<(), Self::Error> {
        self.collected.push(id);
        Ok(())
    }
}

impl LightCacheBackend for GraphLightBackend<'_> {
    type Error = String;

    fn active_scene(&self) -> Result<SceneId, Self::Error> {
        self.graph
            .scene
            .ok_or_else(|| "session scene unavailable".into())
    }

    fn collect(
        &mut self,
        _state: &mut LightCacheHeader,
        _scene: SceneId,
    ) -> Result<(), Self::Error> {
        let Some(root) = self.graph.root else {
            return Ok(());
        };
        // The traversal root is the stored primary sector, not the threaded
        // scene identity; the identity is opaque plumbing for the leaf. The
        // header keeps its reset words; the collected vector carries the
        // collection, mirroring native vector growth without inventing
        // token arithmetic.
        let mut remaining = self.graph.views.len().saturating_mul(4).saturating_add(64);
        collect_scene_lights(self, root, &mut remaining)
            .map_err(|error| format!("session light collect stopped: {error:?}"))
    }
}

/// Run one light-cache collection over a session snapshot through the exact
/// `use_lightcache` leaf: header reset, active-scene gate, subtree walk.
pub fn collect_session_lights(graph: &SessionLightGraph) -> Result<SessionLightCache, String> {
    let mut backend = GraphLightBackend::new(graph);
    let mut header = LightCacheHeader {
        begin_token: 0,
        end_token: 0,
        field_178: u32::MAX,
    };
    header
        .use_lightcache(&mut backend)
        .map_err(|error| format!("session light collect stopped: {error}"))?;
    Ok(SessionLightCache {
        header,
        lights: backend.into_collected(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_lookup_preserves_physical_sentinel_and_duplicate_selection() {
        assert_eq!(lookup_resource_value(0, &[], 95, 7), Ok(7));
        let records = [
            ResourceRecord { key: 0, value: 100 },
            ResourceRecord { key: 5, value: 101 },
            ResourceRecord { key: 5, value: 102 },
            ResourceRecord { key: 5, value: 103 },
            ResourceRecord { key: 5, value: 104 },
            ResourceRecord {
                key: u32::MAX,
                value: 105,
            },
        ];
        // The original search need not choose the first or last equal record.
        assert_eq!(lookup_resource_value(5, &records, 5, 7), Ok(103));
        assert_eq!(
            lookup_resource_value(1, &records[..1], 0, 7),
            Err(ResourceLookupError::MissingRecord(1))
        );
    }

    #[test]
    fn resource_lookup_retains_the_original_capped_search_gap() {
        let mut records: Vec<_> = (0..10_000)
            .map(|key| ResourceRecord {
                key,
                value: key + 1,
            })
            .collect();
        records.push(ResourceRecord {
            key: u32::MAX,
            value: 10_001,
        });
        assert_eq!(lookup_resource_value(10_000, &records, 5_000, 17), Ok(17));
        assert_eq!(
            lookup_resource_value(10_000, &records, 9_998, 17),
            Ok(9_999)
        );
        assert_eq!(
            lookup_resource_value(u32::MAX, &[], 0, 17),
            Err(ResourceLookupError::UnsupportedCount(u32::MAX))
        );
    }

    struct Tree {
        frames: Vec<LightFrameView>,
        selected: Vec<LightFrameId>,
        mutate_links: bool,
    }
    impl LightTraversalBackend for Tree {
        type Error = &'static str;
        fn frame(&self, id: LightFrameId) -> Result<LightFrameView, Self::Error> {
            self.frames
                .get(id.0 as usize)
                .copied()
                .ok_or("missing frame")
        }
        fn append(&mut self, id: LightFrameId) -> Result<(), Self::Error> {
            self.selected.push(id);
            if self.mutate_links {
                match id.0 {
                    1 => {
                        self.frames[1].first_child = Some(LightFrameId(3));
                        self.frames[1].next_sibling = Some(LightFrameId(4));
                    }
                    3 => self.frames[1].next_sibling = Some(LightFrameId(5)),
                    _ => {}
                }
            }
            Ok(())
        }
    }
    fn light() -> LightFrameView {
        LightFrameView {
            frame_type: 2,
            light_kind: 1,
            ..LightFrameView::default()
        }
    }

    #[test]
    fn light_traversal_excludes_root_and_skips_whole_tagged_branches() {
        let mut tree = Tree {
            frames: vec![light(); 6],
            selected: Vec::new(),
            mutate_links: false,
        };
        tree.frames[0].user_data_word = Some(123); // Root tag is not tested.
        tree.frames[0].first_child = Some(LightFrameId(1));
        tree.frames[1].user_data_word = Some(1);
        tree.frames[1].first_child = Some(LightFrameId(2));
        tree.frames[1].next_sibling = Some(LightFrameId(3));
        tree.frames[3].user_data_word = Some(0);
        tree.frames[3].light_kind = 2;
        tree.frames[3].first_child = Some(LightFrameId(4));
        tree.frames[4].light_kind = 3;
        tree.frames[4].first_child = Some(LightFrameId(5));
        collect_scene_lights(&mut tree, LightFrameId(0), &mut 16).unwrap();
        assert_eq!(tree.selected, [LightFrameId(3), LightFrameId(5)]);
    }

    #[test]
    fn light_traversal_rereads_child_and_sibling_after_synchronous_append() {
        let mut tree = Tree {
            frames: vec![light(); 6],
            selected: Vec::new(),
            mutate_links: true,
        };
        tree.frames[0].first_child = Some(LightFrameId(1));
        tree.frames[1].first_child = Some(LightFrameId(2));
        collect_scene_lights(&mut tree, LightFrameId(0), &mut 16).unwrap();
        assert_eq!(
            tree.selected,
            [LightFrameId(1), LightFrameId(3), LightFrameId(5)]
        );
    }

    #[test]
    fn cyclic_light_storage_exhausts_host_budget_without_rolling_back() {
        let mut tree = Tree {
            frames: vec![light(); 2],
            selected: Vec::new(),
            mutate_links: false,
        };
        tree.frames[0].first_child = Some(LightFrameId(1));
        tree.frames[1].next_sibling = Some(LightFrameId(1));
        assert_eq!(
            collect_scene_lights(&mut tree, LightFrameId(0), &mut 2),
            Err(LightTraversalError::BudgetExhausted)
        );
        assert_eq!(tree.selected, [LightFrameId(1), LightFrameId(1)]);
    }

    /// Synthetic arguments from the unchanged native-body corpus. High bytes
    /// must not be treated as a nonzero boolean by the helper itself.
    #[test]
    fn night_setter_keeps_the_original_low_byte_domain() {
        let cases = [
            (0, 0),
            (1, 1),
            (255, 255),
            (256, 0),
            (257, 1),
            (0x8000_0000, 0),
            (0x8000_00ff, 255),
            (u32::MAX, 255),
        ];
        let mut state = NightMissionState { enabled_raw: 165 };
        for (argument, expected) in cases {
            state.set_night_mission_raw(argument);
            assert_eq!(state.enabled_raw, expected);
        }
    }

    #[derive(Default)]
    struct Resources {
        events: Vec<(u8, u32, u32, bool)>,
        mode: u8,
        nested: bool,
    }
    impl MapResourceBackend for Resources {
        type Error = &'static str;
        fn lookup(&mut self, state: &mut MapState, key: u32) -> Result<u32, Self::Error> {
            self.events
                .push((state.enabled_raw, state.resource_value, key, self.nested));
            if self.mode == 1 {
                state.enabled_raw = 128;
                state.resource_value = 0xaabb_ccdd;
            }
            if self.mode == 2 {
                state.enabled_raw = 128;
                return Err("resource unavailable");
            }
            let answer = if self.nested {
                0x6000_0001
            } else {
                0x6000_0000
            };
            if self.mode == 3 && !self.nested {
                self.nested = true;
                state.set_map_enabled_raw(self, 0x1234)?;
                self.nested = false;
            }
            Ok(answer)
        }
    }

    #[test]
    fn map_refresh_is_unconditional_and_observes_the_new_byte() {
        let mut state = MapState {
            enabled_raw: 0,
            resource_value: 0x1234_5678,
        };
        let mut resources = Resources::default();
        state.set_map_enabled_raw(&mut resources, 256).unwrap();
        state.set_map_enabled_raw(&mut resources, 0).unwrap();
        assert_eq!(
            resources.events,
            [(0, 0x1234_5678, 0x5f, false), (0, 0x6000_0000, 0x5f, false)]
        );
        assert_eq!(state.resource_value, 0x6000_0000);
    }

    #[test]
    fn map_callback_mutation_and_reentry_survive_the_outer_resource_store() {
        for (mode, byte, events) in [(1, 128, 1), (3, 0x34, 2)] {
            let mut state = MapState {
                enabled_raw: 165,
                resource_value: 0x1234_5678,
            };
            let mut resources = Resources {
                mode,
                ..Resources::default()
            };
            state.set_map_enabled_raw(&mut resources, 1).unwrap();
            assert_eq!(state.enabled_raw, byte);
            assert_eq!(state.resource_value, 0x6000_0000);
            assert_eq!(resources.events.len(), events);
            if mode == 3 {
                assert_eq!(resources.events[1], (0x34, 0x1234_5678, 0x5f, true));
            }
        }
    }

    #[test]
    fn resource_error_does_not_restore_the_old_ui_state() {
        let mut state = MapState {
            enabled_raw: 0,
            resource_value: 17,
        };
        let mut resources = Resources {
            mode: 2,
            ..Resources::default()
        };
        assert_eq!(
            state.set_map_enabled_raw(&mut resources, 1),
            Err("resource unavailable")
        );
        assert_eq!(state.enabled_raw, 128);
        assert_eq!(state.resource_value, 17);
    }

    struct Collector {
        missing_scene: bool,
        seen: Option<LightCacheHeader>,
    }
    impl LightCacheBackend for Collector {
        type Error = &'static str;
        fn active_scene(&self) -> Result<SceneId, Self::Error> {
            if self.missing_scene {
                Err("scene unavailable")
            } else {
                Ok(SceneId(7))
            }
        }
        fn collect(
            &mut self,
            state: &mut LightCacheHeader,
            scene: SceneId,
        ) -> Result<(), Self::Error> {
            assert_eq!(scene, SceneId(7));
            self.seen = Some(*state);
            state.end_token = state.end_token.wrapping_add(4);
            state.field_178 = 7;
            Ok(())
        }
    }

    #[test]
    fn light_cache_reset_precedes_traversal_and_preserves_its_changes() {
        let mut state = LightCacheHeader {
            begin_token: u32::MAX,
            end_token: 31,
            field_178: 99,
        };
        let mut backend = Collector {
            missing_scene: false,
            seen: None,
        };
        state.use_lightcache(&mut backend).unwrap();
        assert_eq!(
            backend.seen,
            Some(LightCacheHeader {
                begin_token: u32::MAX,
                end_token: u32::MAX,
                field_178: u32::MAX,
            })
        );
        assert_eq!(state.end_token, 3);
        assert_eq!(state.field_178, 7);
        backend.missing_scene = true;
        assert_eq!(state.use_lightcache(&mut backend), Err("scene unavailable"));
        assert_eq!(state.end_token, u32::MAX);
        assert_eq!(state.field_178, u32::MAX);
    }

    #[test]
    fn effect_item_ctor_rva_44be70_stores_kind_zero() {
        // GAME RVA 0x44be70, VA 0x84be70.
        assert_eq!(EffectItemBase::new(), EffectItemBase { kind: 0 });
        assert_eq!(EffectItemBase::default().kind, 0);
    }

    #[test]
    fn effect_part_ctor_rva_44b820_matches_table_defaults() {
        // GAME RVA 0x44b820, VA 0x84b820.
        let params = EffectPartParams::new();
        assert_eq!(params.kind, EFFECT_ITEM_PART);
        assert_eq!(params.blend, 0);
        assert_eq!(params.offset, [0.0, 0.0, 0.0]);
        assert_eq!(params.offset_spread, [0.0, 0.0, 0.0]);
        assert_eq!(params.dir, [0.0, 0.0, 0.0]);
        assert_eq!(params.dir_spread, [0.0, 0.0, 0.0]);
        assert_eq!(params.speed.to_bits(), 1.0f32.to_bits());
        assert_eq!(params.speed_spread.to_bits(), 0.0f32.to_bits());
        assert_eq!(params.scale, [1.0, 1.0, 1.0]);
        assert_eq!(params.scale_spread, [0.0, 0.0, 0.0]);
        assert_eq!(params.transparency.to_bits(), 1.0f32.to_bits());
        assert_eq!(params.transparency_spread.to_bits(), 0.0f32.to_bits());
        assert_eq!(params.rate, 0.0);
        assert_eq!(params.burst, 0);
        assert_eq!(params.rate_fall, 0.0);
        assert_eq!(params.fall_delay, 0.0);
        assert_eq!(params.repeats, 0.0);
        assert_eq!(params.restart_delay, 0.0);
        assert_eq!(params.unknown_94, 0.0);
        assert_eq!(params.flags, 0);
    }

    #[test]
    fn effect_part_dtor_rva_44bef0_releases_back_to_front() {
        // GAME RVA 0x44bef0, VA 0x84bef0.
        let mut subparts = vec![11, 12, 13];
        assert_eq!(free_effect_part_children(&mut subparts), vec![13, 12, 11]);
        assert!(subparts.is_empty());
        let mut empty: Vec<EffectSubpartHandle> = Vec::new();
        assert!(free_effect_part_children(&mut empty).is_empty());
    }

    #[test]
    fn effect_dtor_rva_44bff0_releases_back_to_front() {
        // GAME RVA 0x44bff0, VA 0x84bff0.
        let mut parts = vec![21, 22];
        assert_eq!(free_effect_children(&mut parts), vec![22, 21]);
        assert!(parts.is_empty());
    }

    struct Table {
        loads: usize,
        batch: Vec<EffectHandle>,
    }
    impl EffectsTableBackend for Table {
        fn load_effects_table(&mut self, state: &mut EffectsState) {
            self.loads += 1;
            state.effects.extend(self.batch.iter().copied());
        }
    }

    #[test]
    fn effects_ctor_rva_449080_loads_then_marks_guid_none() {
        // GAME RVA 0x449080, VA 0x849080.
        let mut backend = Table {
            loads: 0,
            batch: vec![5, 6],
        };
        let state = effects_create(&mut backend);
        assert_eq!(backend.loads, 1);
        assert_eq!(state.effects, vec![5, 6]);
        assert_eq!(state.last_guid, -1);
        assert!(!state.started);
    }

    #[test]
    fn effects_dtor_rva_449100_clears_loaded_effects() {
        // GAME RVA 0x449100, VA 0x849100.
        let mut state = EffectsState {
            effects: vec![1, 2, 3],
            ..EffectsState::default()
        };
        assert_eq!(effects_free(&mut state), vec![1, 2, 3]);
        assert!(state.effects.is_empty());
    }

    #[test]
    fn effects_mission_start_rva_449180_resets_and_loads_only_when_empty() {
        // GAME RVA 0x449180, VA 0x849180.
        let mut backend = Table {
            loads: 0,
            batch: vec![9],
        };
        let mut state = EffectsState {
            last_guid: 41,
            started: true,
            ..EffectsState::default()
        };
        assert!(effects_mission_start(&mut state, &mut backend));
        assert_eq!(backend.loads, 1);
        assert_eq!(state.effects, vec![9]);
        assert_eq!(state.last_guid, -1);
        assert!(state.started);
        // A loaded table is kept; the number and flag are still rewritten.
        state.last_guid = 7;
        assert!(effects_mission_start(&mut state, &mut backend));
        assert_eq!(backend.loads, 1);
        assert_eq!(state.last_guid, -1);
        assert!(state.started);
    }

    #[test]
    fn effects_mission_done_rva_4491d0_gates_on_started_and_drains_pools() {
        // GAME RVA 0x4491d0, VA 0x8491d0.
        let mut state = EffectsState {
            in_use: vec![1, 2],
            static_emitters: vec![3],
            free_emitters: vec![4],
            last_guid: 8,
            started: false,
            ..EffectsState::default()
        };
        let mut pools = EffectsFreePools {
            particles: vec![10],
            parts: vec![11],
            subparts: vec![12],
        };
        assert!(!effects_mission_done(&mut state, &mut pools));
        assert_eq!(state.in_use, vec![1, 2]);
        state.started = true;
        assert!(effects_mission_done(&mut state, &mut pools));
        assert!(state.in_use.is_empty());
        assert!(state.static_emitters.is_empty());
        assert!(state.free_emitters.is_empty());
        assert!(pools.particles.is_empty());
        assert!(pools.parts.is_empty());
        assert!(pools.subparts.is_empty());
        assert_eq!(state.last_guid, -1);
        assert!(!state.started);
        // A second call after the teardown is a no-op.
        assert!(!effects_mission_done(&mut state, &mut pools));
    }

    #[test]
    fn effects_clear_rva_44a350_releases_forward() {
        // GAME RVA 0x44a350, VA 0x84a350.
        let mut effects = vec![1, 2, 3];
        assert_eq!(effects_clear(&mut effects), vec![1, 2, 3]);
        assert!(effects.is_empty());
    }

    #[test]
    fn effects_clear_emitters_rva_44a2f0_releases_forward() {
        // GAME RVA 0x44a2f0, VA 0x84a2f0.
        let mut emitters = vec![4, 5];
        assert_eq!(effects_clear_emitters(&mut emitters), vec![4, 5]);
        assert!(emitters.is_empty());
    }

    struct Emitters {
        ticks: Vec<(EmitterHandle, i32)>,
        resets: Vec<EmitterHandle>,
        finished: Vec<EmitterHandle>,
    }
    impl EffectsEmitBackend for Emitters {
        fn tick_emitter(&mut self, emitter: EmitterHandle, time: i32) -> bool {
            self.ticks.push((emitter, time));
            !self.finished.contains(&emitter)
        }
        fn reset_emitter(&mut self, emitter: EmitterHandle) {
            self.resets.push(emitter);
        }
    }

    #[test]
    fn effects_client_tick_rva_44a0d0_recycles_finished_in_place() {
        // GAME RVA 0x44a0d0, VA 0x84a0d0.
        let mut state = EffectsState {
            in_use: vec![1, 2, 3],
            static_emitters: vec![7, 8],
            last_guid: 0,
            ..EffectsState::default()
        };
        let mut backend = Emitters {
            ticks: Vec::new(),
            resets: Vec::new(),
            finished: vec![2],
        };
        effects_client_tick(&mut state, 16, &mut backend);
        assert_eq!(
            backend.ticks,
            vec![(7, 16), (8, 16), (1, 16), (2, 16), (3, 16)]
        );
        assert_eq!(backend.resets, vec![2]);
        assert_eq!(state.in_use, vec![1, 3]);
        assert_eq!(state.free_emitters, vec![2]);
        // Without a started particle the in-use list is left alone.
        state.last_guid = -1;
        backend.ticks.clear();
        effects_client_tick(&mut state, 16, &mut backend);
        assert_eq!(backend.ticks, vec![(7, 16), (8, 16)]);
        assert_eq!(state.in_use, vec![1, 3]);
    }

    fn light_view(
        frame_type: i32,
        light_kind: i32,
        user_data_word: Option<u32>,
        first_child: Option<u32>,
        next_sibling: Option<u32>,
    ) -> LightFrameView {
        LightFrameView {
            user_data_word,
            frame_type,
            light_kind,
            first_child: first_child.map(LightFrameId),
            next_sibling: next_sibling.map(LightFrameId),
        }
    }

    fn light_graph(views: &[(u32, LightFrameView)]) -> SessionLightGraph {
        SessionLightGraph {
            scene: Some(SceneId(7)),
            root: Some(LightFrameId(1)),
            views: views.iter().map(|(id, view)| (*id, view.clone())).collect(),
        }
    }

    #[test]
    fn session_lights_collect_point_and_spot_in_call_order() {
        // Sector root 1: point light 2, spot light 3, fog light 4 (skipped:
        // kind 6 is not 1..=2), user-data-blocked branch 5 (skipped with its
        // point-light child 6).
        let graph = light_graph(&[
            (1, light_view(5, 0, Some(0), Some(2), None)),
            (2, light_view(2, 1, Some(0), Some(3), None)),
            (3, light_view(2, 2, Some(0), Some(4), None)),
            (4, light_view(2, 6, Some(0), Some(5), None)),
            (5, light_view(1, 0, Some(9), Some(6), None)),
            (6, light_view(2, 1, Some(0), None, None)),
        ]);
        let collected = collect_session_lights(&graph).unwrap();
        assert_eq!(collected.header.end_token, 0);
        assert_eq!(collected.header.field_178, u32::MAX);
        assert_eq!(collected.lights, vec![LightFrameId(2), LightFrameId(3)]);
    }

    #[test]
    fn session_lights_reject_missing_scene_and_exhausted_budget() {
        // No scene: active-scene gate fails before any traversal.
        let empty = SessionLightGraph::default();
        assert_eq!(
            collect_session_lights(&empty),
            Err("session light collect stopped: session scene unavailable".into())
        );
        // Root outside the snapshot: fail loudly, never invent.
        let mut partial = light_graph(&[(9, light_view(5, 0, Some(0), None, None))]);
        partial.root = Some(LightFrameId(2));
        assert!(collect_session_lights(&partial).is_err());
    }

    #[test]
    fn session_lights_snapshot_projects_arena_kinds() {
        use crate::frame_transform::FrameKey;
        use crate::model_loading::ModelArena;
        use crate::scene_ownership::SceneKey;
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.primary_sector_frame(scene), None);
        let sector = arena.create_sector_frame().unwrap();
        arena
            .link_sector_scene_ring(sector.frame(), Some(scene))
            .unwrap();
        assert_eq!(arena.primary_sector_frame(scene), Some(sector.frame()));
        assert_eq!(arena.primary_sector_frame(SceneKey(FrameKey(0x1234))), None);
        let point = arena.create_light().unwrap();
        let fog = arena.create_light().unwrap();
        link_light_under(&mut arena, point, sector.frame(), 1);
        link_light_under(&mut arena, fog, sector.frame(), 6);
        let graph = snapshot_session_lights(&arena, Some(scene));
        assert_eq!(graph.root.map(|id| id.0), Some(sector.frame().0));
        let collected = collect_session_lights(&graph).unwrap();
        assert_eq!(collected.lights, vec![LightFrameId(point.0)]);
        let _ = fog;
    }

    fn link_light_under(
        arena: &mut crate::model_loading::ModelArena,
        light: crate::frame_transform::FrameKey,
        parent: crate::frame_transform::FrameKey,
        light_type: u32,
    ) {
        arena
            .graph
            .apply_light_load_image_fields(
                light,
                &lhr_assets::LightRecord {
                    mode: 0,
                    light_type,
                    power: 1.0,
                    diffuse: [1.0; 3],
                    range: [1.0, 10.0],
                    cone: [0.2, 0.5],
                },
            )
            .unwrap();
        arena.link_frame(light, Some(parent), 0).unwrap();
    }
}
