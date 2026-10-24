//! Original CreateCrashActors430c50 and its recursive piece worker4311d0.
//! World source identities are resolved by the world owner; only the newly
//! constructed model and its pieces are addressable canonical arena frames.
use crate::crash_actor::{self, CrashActorBackend, CrashActorError};
use crate::native_precision::{add, div, mul, sqrt, sub};
use crate::native_transform::{SMatrix, SQuat};
pub use crate::vehicle_car_contacts::VehicleCreateCrashActorsCall as CrashActorsCall;
use crate::vehicle_init::VehicleInitBackend;
#[derive(Clone, Copy, Debug)]
pub struct CrashSourcePose {
    pub matrix: SMatrix,
    pub local_scale: [f32; 3],
    pub has_parent: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrashPendingResource {
    Frame(u32),
    Allocation(u32),
    ActorAllocation(u32),
    ActorInitializing(u32),
    ActorGameInitialized(u32),
    Actor { address: u32, constructed: bool },
}
/// Read-only evidence from the real initialized piece; never used as simulation input.
#[derive(Clone, Debug, PartialEq)]
pub struct CrashPieceObservation {
    pub actor: u32,
    pub frame: u32,
    pub source_index: Option<usize>,
    pub name: Vec<u8>,
    pub model_resource: Option<Vec<u8>>,
    pub parameters: [u32; 5],
    pub local_bbox: [[f32; 3]; 2],
    pub inertia: [u32; 3],
}
pub trait CrashActorsBackend: CrashActorBackend {
    fn crash_track(&mut self, resource: CrashPendingResource);
    fn crash_untrack(&mut self, resource: CrashPendingResource) -> Result<(), Self::Error>;
    fn crash_pending_last(&self) -> Option<CrashPendingResource>;
    fn crash_observe_piece(
        &mut self,
        model: u32,
        actor: u32,
        frame: u32,
        parameters: [u32; 5],
    ) -> Result<(), Self::Error>;

    fn crash_source_pose(&mut self, identity: u32) -> Result<CrashSourcePose, Self::Error>;
    fn crash_new_model(&mut self) -> Result<u32, Self::Error>;
    fn crash_model_open(&mut self, model: u32, path: &[u8]) -> Result<i32, Self::Error>;
    fn crash_model_duplicate(&mut self, destination: u32, source: u32) -> Result<(), Self::Error>;
    fn crash_frame_virtual(
        &mut self,
        frame: u32,
        slot: u32,
        args: &[u32],
    ) -> Result<u32, Self::Error>;
    fn crash_update_world(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn crash_place_frame_sector(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn crash_test_col_hierarchy(
        &mut self,
        point: [f32; 3],
        delta: [f32; 3],
        model: u32,
    ) -> Result<u32, Self::Error>;
    /// The caller factory journals the raw allocation before applying its declared malloc fill.
    fn crash_allocate_actor(&mut self) -> Result<u32, Self::Error>;
    fn crash_allocator_marker(&self) -> u8;
    fn crash_actor_kick(&mut self, actor: u32) -> Result<(), Self::Error>;
}
#[derive(Debug)]
pub enum CrashActorsError<E> {
    Backend(E),
    Actor(CrashActorError<E>),
    StorageRange,
    AllocationFailed,
    NumericDomain,
    IncompleteActorConstructor(u32),
    IncompleteActorGameInit(u32),
}
type Out<B, T> = Result<T, CrashActorsError<<B as VehicleInitBackend>::Error>>;
fn track<B: CrashActorsBackend>(b: &mut B, resource: CrashPendingResource) {
    b.crash_track(resource)
}
fn untrack<B: CrashActorsBackend>(b: &mut B, resource: CrashPendingResource) -> Out<B, ()> {
    b.crash_untrack(resource).map_err(CrashActorsError::Backend)
}
fn tracked_free<B: CrashActorsBackend>(b: &mut B, address: u32) -> Out<B, ()> {
    b.free(address).map_err(CrashActorsError::Backend)?;
    untrack(b, CrashPendingResource::Allocation(address))
}

fn word<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<B, u32> {
    b.read_word(a).map_err(CrashActorsError::Backend)
}
fn put<B: VehicleInitBackend>(b: &mut B, a: u32, x: u32) -> Out<B, ()> {
    b.write_word(a, x).map_err(CrashActorsError::Backend)
}
fn byte<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<B, u8> {
    b.read_byte(a).map_err(CrashActorsError::Backend)
}
fn putb<B: VehicleInitBackend>(b: &mut B, a: u32, x: u8) -> Out<B, ()> {
    b.write_byte(a, x).map_err(CrashActorsError::Backend)
}
fn float<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<B, f32> {
    Ok(f32::from_bits(word(b, a)?))
}
fn putf<B: VehicleInitBackend>(b: &mut B, a: u32, x: f32) -> Out<B, ()> {
    put(b, a, x.to_bits())
}
fn virtual_call<B: CrashActorsBackend>(b: &mut B, f: u32, s: u32, a: &[u32]) -> Out<B, u32> {
    b.crash_frame_virtual(f, s, a)
        .map_err(CrashActorsError::Backend)
}
fn count<B: VehicleInitBackend>(b: &mut B, h: u32, stride: u32) -> Out<B, u32> {
    let p = word(b, h + 4)?;
    if p == 0 {
        return Ok(0);
    }
    let end = word(b, h + 8)?;
    end.checked_sub(p)
        .filter(|n| n.is_multiple_of(stride) && *n <= 16 * 1024 * 1024)
        .map(|n| n / stride)
        .ok_or(CrashActorsError::StorageRange)
}
fn cstring<B: VehicleInitBackend>(b: &mut B, p: u32) -> Out<B, Vec<u8>> {
    let mut v = Vec::new();
    for i in 0..65536 {
        let x = byte(b, p + i)?;
        if x == 0 {
            return Ok(v);
        }
        v.push(x);
    }
    Err(CrashActorsError::StorageRange)
}
fn allocate_string<B: CrashActorsBackend>(b: &mut B, name: &[u8]) -> Out<B, u32> {
    let p = b
        .allocate(name.len() as u32 + 1)
        .map_err(CrashActorsError::Backend)?;
    if p == 0 {
        return Err(CrashActorsError::AllocationFailed);
    }
    track(b, CrashPendingResource::Allocation(p));
    for (i, x) in name.iter().copied().chain([0]).enumerate() {
        putb(b, p + i as u32, x)?;
    }
    Ok(p)
}
fn erase_cache<B: VehicleInitBackend>(b: &mut B, index: u32) -> Out<B, ()> {
    let h = 0x647dd4;
    let n = count(b, h, 8)?;
    let p = word(b, h + 4)?;
    let name = word(b, p + index * 8 + 4)?;
    let references = word(b, name)?;
    put(b, name, references.wrapping_sub(1))?;
    if references == 1 {
        b.free(name).map_err(CrashActorsError::Backend)?;
    }
    for i in index..n - 1 {
        for o in [0, 4] {
            let x = word(b, p + (i + 1) * 8 + o)?;
            put(b, p + i * 8 + o, x)?;
        }
    }
    put(b, h + 8, p + (n - 1) * 8)
}
fn append_cache<B: CrashActorsBackend>(b: &mut B, model: u32, name: &[u8]) -> Out<B, ()> {
    let h = 0x647dd4;
    let n = count(b, h, 8)?;
    let p = word(b, h + 4)?;
    let end = word(b, h + 8)?;
    let cap = word(b, h + 12)?;
    let target = if cap.wrapping_sub(end) >= 8 {
        end
    } else {
        let capacity = n
            .checked_add(n.max(1))
            .and_then(|x| x.checked_mul(8))
            .ok_or(CrashActorsError::StorageRange)?;
        let fresh = b.allocate(capacity).map_err(CrashActorsError::Backend)?;
        if fresh == 0 {
            return Err(CrashActorsError::AllocationFailed);
        }
        track(b, CrashPendingResource::Allocation(fresh));
        for i in 0..n * 2 {
            let x = word(b, p + i * 4)?;
            put(b, fresh + i * 4, x)?;
        }
        b.free(p).map_err(CrashActorsError::Backend)?;
        put(b, h + 4, fresh)?;
        put(b, h + 8, fresh + n * 8)?;
        put(b, h + 12, fresh + capacity)?;
        untrack(b, CrashPendingResource::Allocation(fresh))?;
        fresh + n * 8
    };
    let backing = b
        .allocate(name.len() as u32 + 9)
        .map_err(CrashActorsError::Backend)?;
    if backing == 0 {
        return Err(CrashActorsError::AllocationFailed);
    }
    track(b, CrashPendingResource::Allocation(backing));
    put(b, backing, 1)?;
    put(b, backing + 4, name.len() as u32)?;
    for (i, x) in name.iter().copied().chain([0]).enumerate() {
        putb(b, backing + 8 + i as u32, x)?;
    }
    let name = backing;
    put(b, target, model)?;
    put(b, target + 4, name)?;
    put(b, h + 8, target + 8)?;
    untrack(b, CrashPendingResource::Allocation(backing))
}
fn cache_name_matches(stored: &[u8], pattern: &[u8]) -> bool {
    if stored.is_empty() || pattern.is_empty() {
        return stored.is_empty() && pattern.is_empty();
    }
    for (i, &p) in pattern.iter().enumerate() {
        if p == b'*' {
            return true;
        }
        let Some(&s) = stored.get(i) else {
            return false;
        };
        if p != b'?' && !s.eq_ignore_ascii_case(&p) {
            return false;
        }
    }
    stored.len() == pattern.len()
}

fn acquire_model<B: CrashActorsBackend>(b: &mut B, model: u32, name: &[u8]) -> Out<B, bool> {
    let n = count(b, 0x647dd4, 8)?;
    for i in (0..n).rev() {
        let base = word(b, 0x647dd8)?;
        if word(b, base + i * 8)? == model {
            erase_cache(b, i)?;
            break;
        }
    }
    let n = count(b, 0x647dd4, 8)?;
    for i in 0..n {
        let base = word(b, 0x647dd8)?;
        let p = word(b, base + i * 8 + 4)?;
        if cache_name_matches(&cstring(b, p + 8)?, name) {
            let source = word(b, base + i * 8)?;
            b.crash_model_duplicate(model, source)
                .map_err(CrashActorsError::Backend)?;
            return Ok(true);
        }
    }
    let paths = count(b, 0x647de4, 4)?;
    for i in 0..paths {
        let base = word(b, 0x647de8)?;
        let p = word(b, base + i * 4)?;
        let mut path = cstring(b, p + 8)?;
        path.extend_from_slice(name);
        let result = b
            .crash_model_open(model, &path)
            .map_err(CrashActorsError::Backend)?;
        if matches!(result as u32, 0x80000002 | 0x8000000a) {
            return Ok(false);
        }
        if result < 0 {
            continue;
        }
        if count(b, 0x647dd4, 8)? >= word(b, 0x647df4)? {
            let n = count(b, 0x647dd4, 8)?;
            for index in 0..n {
                let base = word(b, 0x647dd8)?;
                let cached = word(b, base + index * 8)?;
                if word(b, cached + 4)? == 1 {
                    virtual_call(b, cached, 0, &[])?;
                    erase_cache(b, index)?;
                    break;
                }
            }
        }
        let cached = b.crash_new_model().map_err(CrashActorsError::Backend)?;
        if cached == 0 {
            return Err(CrashActorsError::AllocationFailed);
        }
        track(b, CrashPendingResource::Frame(cached));
        b.crash_model_duplicate(cached, model)
            .map_err(CrashActorsError::Backend)?;
        append_cache(b, cached, name)?;
        untrack(b, CrashPendingResource::Frame(cached))?;
        return Ok(true);
    }
    Ok(false)
}
pub fn crash_debustize<B: CrashActorsBackend>(b: &mut B, frame: u32) -> Out<B, ()> {
    if frame == 0 {
        return Ok(());
    }
    if word(b, frame + 0x110)? == 1 {
        virtual_call(b, frame, 0x5c, &[0])?;
    }
    let n = word(b, frame + 0x11c)?;
    for i in (0..n).rev() {
        let child = virtual_call(b, frame, 0x30, &[i])?;
        crash_debustize(b, child)?;
    }
    Ok(())
}
fn normalize(mut p: [f32; 3]) -> [f32; 3] {
    let square = add(add(mul(p[1], p[1]), mul(p[2], p[2])), mul(p[0], p[0]));
    if f64::from(sub(square, 1.).abs()) < 1e-8 {
        return p;
    }
    if square >= 1e-8 {
        let inverse = div(1., sqrt(square));
        p = p.map(|x| mul(x, inverse));
    } else {
        let i = if p[0] != 0. {
            0
        } else if p[2] != 0. {
            2
        } else {
            1
        };
        p[i] = if p[i] >= 0. { 1. } else { -1. };
    }
    p
}
struct Worker<'a> {
    target: u32,
    force: bool,
    name: u32,
    byte_argument: u8,
    strength: f32,
    mask: u8,
    created: &'a mut Vec<u32>,
    scratch: u32,
}
fn pieces<B: CrashActorsBackend>(
    b: &mut B,
    frame: u32,
    model: u32,
    incoming: [f32; 3],
    w: &mut Worker<'_>,
) -> Out<B, ()> {
    let force = w.force || frame == w.target;
    let local = if force {
        incoming.map(|x| mul(x, 0.6))
    } else {
        incoming
    };
    let children = word(b, frame + 0x11c)?;
    for i in (0..children).rev() {
        let child = virtual_call(b, frame, 0x30, &[i])?;
        let previous = w.force;
        w.force = force;
        pieces(b, child, model, local, w)?;
        w.force = previous;
    }
    if word(b, frame + 0x110)? != 1 {
        return Ok(());
    }
    let refs = word(b, frame + 4)?;
    put(b, frame + 4, refs.wrapping_add(1))?;
    track(b, CrashPendingResource::Frame(frame));
    virtual_call(b, model, 0x5c, &[frame])?;
    let app = word(b, 0x63788c)?;
    let scene = word(b, app + 0x10)?;
    let sector = word(b, scene + 0x210)?;
    virtual_call(b, frame, 0x2c, &[sector, 1])?;
    b.crash_place_frame_sector(frame)
        .map_err(CrashActorsError::Backend)?;
    virtual_call(b, frame, 0x18, &[])?;
    let actor = b
        .crash_allocate_actor()
        .map_err(CrashActorsError::Backend)?;
    if actor == 0 {
        return Err(CrashActorsError::AllocationFailed);
    }
    untrack(b, CrashPendingResource::ActorAllocation(actor))?;
    track(
        b,
        CrashPendingResource::Actor {
            address: actor,
            constructed: false,
        },
    );
    let marker = b.crash_allocator_marker();
    crash_actor::crash_actor_construct(b, actor, marker).map_err(CrashActorsError::Actor)?;
    untrack(
        b,
        CrashPendingResource::Actor {
            address: actor,
            constructed: false,
        },
    )?;
    track(
        b,
        CrashPendingResource::Actor {
            address: actor,
            constructed: true,
        },
    );
    if !crash_actor::crash_actor_initialize(b, actor, frame).map_err(CrashActorsError::Actor)? {
        crash_actor::crash_actor_release(b, actor).map_err(CrashActorsError::Actor)?;
        untrack(
            b,
            CrashPendingResource::Actor {
                address: actor,
                constructed: true,
            },
        )?;
        virtual_call(b, frame, 0x2c, &[0, 0])?;
        virtual_call(b, frame, 0, &[])?;
        untrack(b, CrashPendingResource::Frame(frame))?;
        return Ok(());
    }
    let mask = byte(b, actor + 0x3be)? & w.mask;
    putb(b, actor + 0x3be, mask)?;
    let mass = float(b, actor + 0x394)?;
    let velocity = if force && mask != 0 {
        let direction = normalize(incoming);
        let strength = mul(w.strength, 0.5);
        let inverse = div(1., mass);
        direction.map(|x| mul(mul(x, strength), inverse))
    } else {
        [0.; 3]
    };
    let parameters = [
        mass.to_bits(),
        velocity[0].to_bits(),
        velocity[1].to_bits(),
        velocity[2].to_bits(),
        0.45f32.to_bits(),
    ];
    crash_actor::crash_actor_init_box(b, actor, parameters).map_err(CrashActorsError::Actor)?;
    b.crash_observe_piece(model, actor, frame, parameters)
        .map_err(CrashActorsError::Backend)?;
    crash_actor::crash_actor_set_model_filename(b, actor, w.name)
        .map_err(CrashActorsError::Actor)?;
    putb(b, actor + 0x398, w.byte_argument)?;
    let app = word(b, 0x63788c)?;
    let game = word(b, app + 0x24)?;
    untrack(
        b,
        CrashPendingResource::Actor {
            address: actor,
            constructed: true,
        },
    )?;
    track(b, CrashPendingResource::ActorInitializing(actor));
    let mut initialized = false;
    let added = crash_actor::crash_add_temporary_actor_with_progress(
        b,
        game,
        actor,
        w.scratch,
        &mut initialized,
    );
    if initialized {
        untrack(b, CrashPendingResource::ActorInitializing(actor))?;
        track(b, CrashPendingResource::ActorGameInitialized(actor));
    }
    added.map_err(CrashActorsError::Actor)?;
    if initialized {
        untrack(b, CrashPendingResource::ActorGameInitialized(actor))?;
    } else {
        untrack(b, CrashPendingResource::ActorInitializing(actor))?;
    }
    untrack(b, CrashPendingResource::Frame(frame))?;
    let lifetime = float(b, actor + 0x390)?;
    putf(b, actor + 0x390, sub(lifetime, 2.))?;
    w.created.push(actor);
    Ok(())
}
/// Returns the original success byte. A missing model resource returns false
/// after releasing the real factory model; reached host failures remain errors.
pub fn create_crash_actors<B: CrashActorsBackend>(b: &mut B, c: &CrashActorsCall) -> Out<B, bool> {
    if !c.filename.is_ascii() || c.filename.contains(&0) || c.filename.len() >= 96 {
        return Err(CrashActorsError::StorageRange);
    }
    let model = b.crash_new_model().map_err(CrashActorsError::Backend)?;
    if model == 0 {
        return Ok(false);
    }
    track(b, CrashPendingResource::Frame(model));
    if !acquire_model(b, model, &c.filename)? {
        virtual_call(b, model, 0, &[])?;
        untrack(b, CrashPendingResource::Frame(model))?;
        return Ok(false);
    }
    let pose = b
        .crash_source_pose(c.source_frame)
        .map_err(CrashActorsError::Backend)?;
    if word(b, model + 0xac)? & 0x80 == 0 {
        for i in 0..3 {
            putf(
                b,
                model + 0x80 + i * 4,
                pose.matrix.elements[12 + i as usize],
            )?;
        }
        let flags = word(b, model + 0xac)?;
        put(b, model + 0xac, (flags & 0xfffffedf) | 0x40000000)?;
    }
    let mut rotation =
        SQuat::try_make_from_matrix(&pose.matrix).map_err(|_| CrashActorsError::NumericDomain)?;
    rotation.normalize();
    for (i, x) in rotation.wxyz.into_iter().enumerate() {
        putf(b, model + 0x9c + i as u32 * 4, x)?;
    }
    let flags = word(b, model + 0xac)?;
    put(b, model + 0xac, (flags & 0xfffffecb) | 0x40000008)?;
    let scale = if pose.has_parent {
        pose.matrix
            .try_get_scale()
            .map_err(|_| CrashActorsError::NumericDomain)?
    } else {
        pose.local_scale
    };
    for (i, x) in scale.into_iter().enumerate() {
        putf(b, model + 0x90 + i as u32 * 4, x)?;
    }
    let flags = word(b, model + 0xac)?;
    put(b, model + 0xac, (flags & 0xfffffecb) | 0x40000008)?;
    virtual_call(b, model, 0x18, &[])?;
    virtual_call(b, model, 0x2c, &[0, 0])?;
    b.crash_place_frame_sector(model)
        .map_err(CrashActorsError::Backend)?;
    let mut name = c.filename.clone();
    if let Some(dot) = name.iter().rposition(|x| *x == b'.') {
        if c.change_filename {
            if dot == 0 {
                return Err(CrashActorsError::StorageRange);
            }
            name.truncate(dot - 1);
        } else {
            name.truncate(dot);
            name.extend_from_slice(b"_c");
        }
    }
    let mut target = b
        .crash_test_col_hierarchy(c.point, c.velocity, model)
        .map_err(CrashActorsError::Backend)?;
    let mut depth = c.ancestor_depth;
    while target != 0 && depth > 0 {
        let kind = word(b, target + 0x110)?;
        if kind == 9 || kind == 5 {
            break;
        }
        target = word(b, target + 0x120)?;
        depth -= 1;
    }
    crash_debustize(b, model)?;
    let name = allocate_string(b, &name)?;
    let scratch = b.allocate(4).map_err(CrashActorsError::Backend)?;
    if scratch == 0 {
        return Err(CrashActorsError::AllocationFailed);
    }
    track(b, CrashPendingResource::Allocation(scratch));
    let mut created = Vec::new();
    let mut worker = Worker {
        target,
        force: c.recursive_flag,
        name,
        byte_argument: c.byte_argument,
        strength: c.strength,
        mask: c.actor_mask,
        created: &mut created,
        scratch,
    };
    pieces(b, model, model, c.velocity, &mut worker)?;
    virtual_call(b, model, 0, &[])?;
    untrack(b, CrashPendingResource::Frame(model))?;
    for actor in created.into_iter().rev() {
        b.crash_actor_kick(actor)
            .map_err(CrashActorsError::Backend)?;
    }
    tracked_free(b, scratch)?;
    tracked_free(b, name)?;
    Ok(true)
}
/// Driver/process cleanup of the owned template cache. Live crash actors and
/// their removed pieces are owned by the temporary-actor lifecycle separately.
pub fn cleanup_crash_model_cache<B: CrashActorsBackend>(b: &mut B) -> Out<B, ()> {
    while count(b, 0x647dd4, 8)? != 0 {
        let p = word(b, 0x647dd8)?;
        let model = word(b, p)?;
        virtual_call(b, model, 0, &[])?;
        erase_cache(b, 0)?;
    }
    let p = word(b, 0x647dd8)?;
    b.free(p).map_err(CrashActorsError::Backend)?;
    for o in [4, 8, 12] {
        put(b, 0x647dd4 + o, 0)?;
    }
    Ok(())
}

/// Caller-owned interruption recovery. Entries remain journalled until their
/// real destructor/free completes. An incomplete base constructor is retained
/// explicitly; its unknown suffix is never treated as an initialized actor.
pub fn cleanup_crash_pending<B: CrashActorsBackend>(b: &mut B) -> Out<B, ()> {
    while let Some(resource) = b.crash_pending_last() {
        match resource {
            CrashPendingResource::Frame(frame) => {
                virtual_call(b, frame, 0, &[])?;
            }
            CrashPendingResource::Allocation(address)
            | CrashPendingResource::ActorAllocation(address) => {
                b.free(address).map_err(CrashActorsError::Backend)?;
            }
            CrashPendingResource::ActorInitializing(address) => {
                return Err(CrashActorsError::IncompleteActorGameInit(address))
            }
            CrashPendingResource::ActorGameInitialized(address) => {
                let app = word(b, 0x63788c)?;
                let game = word(b, app + 0x24)?;
                let frame = word(b, address + 0x68)?;
                crash_actor::crash_unlink_temporary_actor(b, game, address)
                    .map_err(CrashActorsError::Actor)?;
                crash_actor::crash_actor_game_done(b, address).map_err(CrashActorsError::Actor)?;
                untrack(b, CrashPendingResource::Frame(frame))?;
                crash_actor::crash_actor_release(b, address).map_err(CrashActorsError::Actor)?;
            }
            CrashPendingResource::Actor {
                address,
                constructed: false,
            } => return Err(CrashActorsError::IncompleteActorConstructor(address)),
            CrashPendingResource::Actor {
                address,
                constructed: true,
            } => {
                // Before AddTemporaryActor the actor owns its derived filename,
                // while its visual's extra caller reference remains separate.
                let name = word(b, address + 0x39c)?;
                if name != 0 {
                    b.free(name).map_err(CrashActorsError::Backend)?;
                    put(b, address + 0x39c, 0)?;
                }
                crash_actor::crash_actor_release(b, address).map_err(CrashActorsError::Actor)?;
            }
        }
        untrack(b, resource)?;
    }
    Ok(())
}
