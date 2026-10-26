//! Retained original-data model graph and fixed-topology renderer bindings.
//! Geometry is shared through Arc; every placement has separate frame identities
//! and local pose state. Native actor/mission initialization is not inferred.
use crate::{
    frame_dispatch::ObjectId,
    model_tick::{ModelBackend, ModelTickState, ModelVTableId},
    scene_tick::{FrameId, InterpolatorId},
};
use glam::{Mat4, Quat, Vec3};
use lhr_assets::Model;
use lhr_render::Vertex;
use std::sync::Arc;

/// Geometry consumes either the inspection placement or the recovered frame
/// matrix. Original point arithmetic must not pass through glam's association.
#[derive(Clone, Copy, Debug)]
pub enum GeometryTransform {
    Inspection(Mat4),
    Original(crate::native_transform::SMatrix),
}
impl From<Mat4> for GeometryTransform {
    fn from(value: Mat4) -> Self {
        Self::Inspection(value)
    }
}
impl GeometryTransform {
    pub fn source_point(self, point: [f32; 3]) -> Result<[f32; 3], String> {
        let result = match self {
            Self::Inspection(matrix) => matrix.transform_point3(Vec3::from_array(point)).to_array(),
            Self::Original(matrix) => matrix.try_transform_point(point).map_err(|_| {
                "Original point transform outside verified numeric domain".to_owned()
            })?,
        };
        if !result.into_iter().all(f32::is_finite) {
            return Err("Geometry transform overflow".into());
        }
        Ok(result)
    }
    pub fn render_point(self, point: [f32; 3]) -> Result<[f32; 3], String> {
        let [x, y, z] = self.source_point(point)?;
        Ok([x, y, -z])
    }
}

/// One retained draw's exact source vertex mapping, including spatial crops.
#[derive(Debug)]
pub struct DrawBinding {
    pub draw_index: usize,
    pub frame_index: usize,
    pub geometry_frame: usize,
    pub source_vertices: Vec<u32>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FramePose {
    pub identity: FrameId,
    pub position: [f32; 3],
    pub rotation_wxyz: [f32; 4],
    pub scale: [f32; 3],
}
#[derive(Debug)]
pub struct RetainedModel {
    pub identity: ObjectId,
    pub source_name: String,
    pub source: Arc<Model>,
    pub world: Mat4,
    pub visual_scale: Option<[f32; 3]>,
    pose: Vec<FramePose>,
    needs_upload: bool,
    pub draws: Vec<DrawBinding>,
    pub native: ModelTickState,
    materialized: Option<crate::model_loading::MaterializedModel>,
    skeletal_bound: bool,
}
#[derive(Debug, Default)]
pub struct RetainedScene {
    pub models: Vec<RetainedModel>,
    next_frame: u32,
    metadata: Option<lhr_assets::Scene>,
    budgeted_bytes: usize,
    metadata_bytes: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub enum RuntimeError {
    UnknownInterpolator(InterpolatorId),
    MissingAnimationEvaluator,
    MissingHierarchyUpdate,
    UnsupportedPhase(&'static str),
    UnknownObject(ObjectId),
    Storage(&'static str),
    Modifier(crate::frame_delta::ModifierError),
    Dispatch(String),
    Animation(String),
}
struct UnboundAnimation;
impl ModelBackend for UnboundAnimation {
    type Error = RuntimeError;
    fn evaluator_count(&self, id: InterpolatorId) -> Result<u32, Self::Error> {
        Err(RuntimeError::UnknownInterpolator(id))
    }
    fn tick_interpolator(
        &mut self,
        _: &mut ModelTickState,
        _: InterpolatorId,
        _: i32,
    ) -> Result<(), Self::Error> {
        Err(RuntimeError::MissingAnimationEvaluator)
    }
    fn update_model(
        &mut self,
        _: &mut ModelTickState,
        _: ModelVTableId,
    ) -> Result<(), Self::Error> {
        Err(RuntimeError::MissingHierarchyUpdate)
    }
}
impl RetainedScene {
    /// Install one explicit inspection controller on the retained owned hierarchy.
    /// Source storage is already charged by assembly; charge controller ownership
    /// and any extra hierarchy storage once, after the initial pose is prepared.
    pub fn bind_skeletal(
        &mut self,
        model_index: usize,
        animation: Arc<lhr_assets::animation::AnimationSet>,
    ) -> Result<crate::skeletal_playback::SkeletalPlayback, String> {
        let model = self
            .models
            .get_mut(model_index)
            .ok_or("Unknown animation placement")?;
        if model.skeletal_bound || model.native.interpolator.is_some() {
            return Err("Animation placement already bound".into());
        }
        if model.world.to_cols_array().map(f32::to_bits)
            != Mat4::IDENTITY.to_cols_array().map(f32::to_bits)
            || model.visual_scale.is_some()
        {
            return Err("Skeletal binding requires identity wrapper placement".into());
        }
        let loaded = model
            .materialized
            .as_mut()
            .ok_or("Skeletal binding requires owned materialization")?;
        if !Arc::ptr_eq(&model.source, loaded.source()) {
            return Err("Skeletal materialization source identity mismatch".into());
        }
        let before = loaded.storage_bytes().map_err(|error| error.to_string())?;
        // Construction may leave original hierarchy writes on failure. Keep the
        // placement bound so a failed install cannot fall back to static success.
        model.skeletal_bound = true;
        let playback = crate::skeletal_playback::SkeletalPlayback::new(loaded, animation)
            .map_err(|error| error.to_string())?;
        let after = loaded.storage_bytes().map_err(|error| error.to_string())?;
        let bytes = playback
            .storage_bytes()
            .map_err(|error| error.to_string())?
            .checked_add(after.saturating_sub(before))
            .ok_or("Animation allocation overflow")?;
        let total = self
            .budgeted_bytes
            .checked_add(bytes)
            .ok_or("Animation allocation overflow")?;
        if total > 256 * 1024 * 1024 {
            return Err("Retained animation exceeds scene memory budget".into());
        }
        self.budgeted_bytes = total;
        model.needs_upload = true;
        Ok(playback)
    }
    /// Charge an externally owned position binding once when installing it.
    /// Repeated charges are conservative; failed charges preserve the budget.
    pub fn charge_position_binding(
        &mut self,
        playback: &crate::animation_binding::PositionPlayback,
    ) -> Result<(), String> {
        let bytes = playback
            .storage_bytes()
            .map_err(|error| format!("{error:?}"))?;
        let total = self
            .budgeted_bytes
            .checked_add(bytes)
            .ok_or("Animation allocation overflow")?;
        if total > 256 * 1024 * 1024 {
            return Err("Retained animation exceeds scene memory budget".into());
        }
        self.budgeted_bytes = total;
        Ok(())
    }
    pub fn metadata(&self) -> Option<&lhr_assets::Scene> {
        self.metadata.as_ref()
    }
    pub fn set_assembly_budget(&mut self, bytes: usize) -> Result<(), String> {
        if bytes > 256 * 1024 * 1024 {
            return Err("Retained scene exceeds resource quota".into());
        }
        self.budgeted_bytes = bytes;
        Ok(())
    }
    pub fn retain_metadata(&mut self, metadata: lhr_assets::Scene) -> Result<(), String> {
        let mut bytes = std::mem::size_of::<lhr_assets::Scene>();
        let mut add = |count: usize, size: usize| -> Result<(), String> {
            bytes = bytes
                .checked_add(count.checked_mul(size).ok_or("Metadata size overflow")?)
                .ok_or("Metadata size overflow")?;
            Ok(())
        };
        add(
            metadata.objects.capacity(),
            std::mem::size_of::<lhr_assets::SceneObject>(),
        )?;
        add(
            metadata.actors.capacity(),
            std::mem::size_of::<lhr_assets::SceneActor>(),
        )?;
        add(
            metadata.scripts.capacity(),
            std::mem::size_of::<lhr_assets::SceneScript>(),
        )?;
        for object in &metadata.objects {
            add(object.name.capacity(), 1)?;
            for value in [&object.model, &object.parent].into_iter().flatten() {
                add(value.capacity(), 1)?;
            }
        }
        for actor in &metadata.actors {
            add(actor.name.capacity(), 1)?;
        }
        for script in &metadata.scripts {
            add(script.name.capacity(), 1)?;
            add(script.code.capacity(), 1)?;
        }
        let next = self
            .budgeted_bytes
            .checked_sub(self.metadata_bytes)
            .and_then(|n| n.checked_add(bytes))
            .ok_or("Metadata budget overflow")?;
        if next > 256 * 1024 * 1024 {
            return Err("Retained metadata exceeds scene resource quota".into());
        }
        self.budgeted_bytes = next;
        self.metadata_bytes = bytes;
        self.metadata = Some(metadata);
        Ok(())
    }
    /// Input models must come from the supported 4DS parser: it rejects nonzero
    /// trailing LoadAnimCount. This is not a constructor for authored Game actors.
    pub fn retain(
        &mut self,
        source_name: String,
        source: Arc<Model>,
        world: Mat4,
        visual_scale: Option<[f32; 3]>,
        draws: Vec<DrawBinding>,
    ) -> Result<(), String> {
        self.retain_inner(source_name, source, world, visual_scale, draws, None)
    }
    /// Retain the same owned hierarchy that supplied the initial render geometry.
    pub fn retain_materialized(
        &mut self,
        source_name: String,
        materialized: crate::model_loading::MaterializedModel,
        draws: Vec<DrawBinding>,
    ) -> Result<(), String> {
        let source = materialized.source().clone();
        if !materialized.is_complete()
            || materialized.source_frame_keys().len() != source.frames.len()
            || materialized.root().0 == 0
        {
            return Err("Cannot retain an incomplete materialized model".into());
        }
        self.retain_inner(
            source_name,
            source,
            Mat4::IDENTITY,
            None,
            draws,
            Some(materialized),
        )
    }
    fn retain_inner(
        &mut self,
        source_name: String,
        source: Arc<Model>,
        world: Mat4,
        visual_scale: Option<[f32; 3]>,
        draws: Vec<DrawBinding>,
        materialized: Option<crate::model_loading::MaterializedModel>,
    ) -> Result<(), String> {
        if source.frames.iter().enumerate().any(|(index, frame)| {
            if let Some(loaded) = &materialized {
                ![1, 6, 7, 10].contains(&frame.kind)
                    || (frame.kind == 1 && !matches!(frame.visual, Some(0..=3)))
                    || (matches!(frame.visual, Some(2 | 3))
                        && loaded
                            .skin_pose()
                            .and_then(|pose| pose.vertices(index))
                            .is_none())
            } else {
                ![1, 2, 5, 6, 7, 12].contains(&frame.kind)
                    || (frame.kind == 1 && !matches!(frame.visual, Some(0 | 1)))
                    || frame.skin.is_some()
                    || frame.morph.is_some()
            }
        }) {
            return Err("Unsupported source frame or missing checked skin pose".into());
        }
        let identity = ObjectId(
            u32::try_from(self.models.len())
                .map_err(|_| "Model identity overflow")?
                .checked_add(1)
                .ok_or("Model identity overflow")?,
        );
        let mut pose = Vec::with_capacity(source.frames.len());
        for frame in &source.frames {
            self.next_frame = self
                .next_frame
                .checked_add(1)
                .ok_or("Frame identity overflow")?;
            pose.push(FramePose {
                identity: FrameId(self.next_frame),
                position: frame.position,
                rotation_wxyz: frame.rotation_wxyz,
                scale: frame.scale,
            });
        }
        let model = RetainedModel {
            identity,
            source_name,
            source,
            world,
            visual_scale,
            pose,
            needs_upload: false,
            draws,
            native: ModelTickState::new(ModelVTableId(0x1009c160)),
            materialized,
            skeletal_bound: false,
        };
        if model.materialized.is_none() {
            model.transforms()?;
        }
        self.models.push(model);
        Ok(())
    }
}
impl RetainedModel {
    fn validate_materialized(&self) -> Result<(), String> {
        let loaded = self
            .materialized
            .as_ref()
            .ok_or("Missing owned materialization")?;
        if !loaded.is_complete()
            || !Arc::ptr_eq(&self.source, loaded.source())
            || self.world.to_cols_array().map(f32::to_bits)
                != Mat4::IDENTITY.to_cols_array().map(f32::to_bits)
            || self.visual_scale.is_some()
        {
            return Err("Owned materialization diverged from retained placement".into());
        }
        Ok(())
    }
    fn materialized_world_matrices(
        &mut self,
    ) -> Result<Vec<crate::native_transform::SMatrix>, String> {
        // Validate public source/placement fields before the lazy getter can
        // mutate the original hierarchy's matrix cache.
        self.validate_materialized()?;
        self.materialized
            .as_mut()
            .ok_or("Missing owned materialization")?
            .world_matrices()
            .map_err(|error| error.to_string())
    }
    fn validate_skeletal(
        &self,
        playback: &crate::skeletal_playback::SkeletalPlayback,
    ) -> Result<(), String> {
        self.validate_materialized()?;
        if !self.skeletal_bound || !Arc::ptr_eq(&self.source, playback.source()) {
            return Err("Skeletal animation diverged from retained placement".into());
        }
        playback
            .validate_model(
                self.materialized
                    .as_ref()
                    .ok_or("Missing skeletal materialization")?,
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    fn vertices_materialized(
        &self,
        binding: &DrawBinding,
        matrices: &[crate::native_transform::SMatrix],
    ) -> Result<Vec<Vertex>, String> {
        self.validate_materialized()?;
        let loaded = self
            .materialized
            .as_ref()
            .ok_or("Missing owned materialization")?;
        let matrix = *matrices
            .get(binding.frame_index)
            .ok_or("Unknown retained frame binding")?;
        let geometry = self
            .source
            .frames
            .get(binding.geometry_frame)
            .ok_or("Unknown retained geometry binding")?;
        let lod = geometry.lods.first().ok_or("Missing retained first LOD")?;
        let pose = loaded
            .skin_pose()
            .and_then(|pose| pose.vertices(binding.frame_index));
        if matches!(geometry.visual, Some(2 | 3)) && pose.is_none()
            || pose.is_some_and(|pose| pose.len() != lod.vertices.len())
        {
            return Err("Missing checked deformation for materialized upload".into());
        }
        binding
            .source_vertices
            .iter()
            .map(|index| {
                let index = *index as usize;
                let vertex = lod
                    .vertices
                    .get(index)
                    .ok_or("Invalid retained vertex binding")?;
                let position = pose.map_or(vertex.position, |pose| pose[index].position);
                Ok(Vertex {
                    position: GeometryTransform::Original(matrix).render_point(position)?,
                    uv: vertex.uv,
                })
            })
            .collect()
    }
    pub fn pose(&self) -> &[FramePose] {
        &self.pose
    }
    /// Change the host inspection placement of an unbound static model.
    /// This is not an original frame/BODY operation. Validate the whole hierarchy
    /// and its bound geometry before committing; owned original hierarchies and
    /// animation bindings must use their respective recovered update paths.
    pub fn set_world_transform(&mut self, world: Mat4) -> Result<(), String> {
        if self.materialized.is_some() || self.skeletal_bound || self.native.interpolator.is_some()
        {
            return Err("Bound model requires its original transform update path".into());
        }
        if world.row(3).to_array() != [0.0, 0.0, 0.0, 1.0] {
            return Err("Inspection placement must be affine".into());
        }
        let transforms = self.transforms_at(world)?;
        for binding in &self.draws {
            self.vertices(binding, &transforms)?;
        }
        if self.world.to_cols_array().map(f32::to_bits) != world.to_cols_array().map(f32::to_bits) {
            self.world = world;
            self.needs_upload = true;
        }
        Ok(())
    }
    pub fn set_frame_position(&mut self, index: usize, position: [f32; 3]) -> Result<(), String> {
        if self.materialized.is_some() {
            return Err("Materialized model has no position-animation binding".into());
        }
        if position.iter().any(|v| !v.is_finite()) {
            return Err("Nonfinite bound position".into());
        }
        let pose = self.pose.get_mut(index).ok_or("Unknown bound frame")?;
        if pose.position.map(f32::to_bits) != position.map(f32::to_bits) {
            pose.position = position;
            self.needs_upload = true;
        }
        Ok(())
    }
    /// Execute the actual recovered model guard for a parsed, unbound static model.
    /// A later animation binding is never silently replaced with a no-op backend.
    pub fn tick_static(&mut self, delta: i32) -> Result<(), RuntimeError> {
        if self.skeletal_bound {
            return Err(RuntimeError::MissingAnimationEvaluator);
        }
        self.native.tick(&mut UnboundAnimation, delta)
    }

    pub fn transforms(&self) -> Result<Vec<Mat4>, String> {
        self.transforms_at(self.world)
    }

    fn transforms_at(&self, world: Mat4) -> Result<Vec<Mat4>, String> {
        if self.materialized.is_some() {
            return Err("Materialized model requires original world matrices".into());
        }
        if self.pose.len() != self.source.frames.len() || !world.is_finite() {
            return Err("Retained model graph mismatch".into());
        }
        if self.visual_scale.is_some_and(|s| {
            s.iter()
                .any(|v| !v.is_finite() || *v <= 0.0 || *v > 10000.0)
        }) {
            return Err("Invalid retained visual scale".into());
        }
        let mut transforms = Vec::with_capacity(self.pose.len());
        for (index, (source, pose)) in self.source.frames.iter().zip(&self.pose).enumerate() {
            let [w, x, y, z] = pose.rotation_wxyz;
            let q = Quat::from_xyzw(-x, -y, -z, w);
            let norm = q.length_squared();
            if !norm.is_finite() || (norm - 1.0).abs() > 0.00001 {
                return Err("Invalid retained frame quaternion".into());
            }
            let scale = if source.kind == 1 {
                self.visual_scale.unwrap_or(pose.scale)
            } else {
                pose.scale
            };
            if pose
                .position
                .iter()
                .chain(scale.iter())
                .any(|v| !v.is_finite())
            {
                return Err("Nonfinite retained pose".into());
            }
            let local = Mat4::from_scale_rotation_translation(
                Vec3::from_array(scale),
                q,
                Vec3::from_array(pose.position),
            );
            let transform = if let Some(parent) = source.parent {
                if parent >= index {
                    return Err("Retained parent ordering is unsupported".into());
                }
                transforms[parent] * local
            } else {
                world * local
            };
            if !transform.is_finite() {
                return Err("Retained transform overflow".into());
            }
            transforms.push(transform);
        }
        Ok(transforms)
    }

    /// Native frame world-matrix and point-transform path for the admitted
    /// single-root position model. Handedness conversion remains renderer policy.
    pub fn vertices_native(
        &self,
        binding: &DrawBinding,
        world: &crate::native_transform::SMatrix,
    ) -> Result<Vec<Vertex>, String> {
        if self.materialized.is_some() {
            return Err("Position-only vertex upload cannot replace a materialized pose".into());
        }
        if binding.frame_index != 0 || binding.geometry_frame != 0 || !world.is_verified_input() {
            return Err("Unsupported native vertex transform domain".into());
        }
        let lod = self
            .source
            .frames
            .first()
            .and_then(|f| f.lods.first())
            .ok_or("Missing native bound geometry")?;
        binding
            .source_vertices
            .iter()
            .map(|index| {
                let vertex = lod
                    .vertices
                    .get(*index as usize)
                    .ok_or("Invalid retained vertex binding")?;
                if !vertex
                    .position
                    .into_iter()
                    .all(crate::native_transform::is_verified_scalar)
                {
                    return Err("Native vertex outside verified numeric domain".into());
                }
                let [x, y, z] = world.transform_point(vertex.position);
                if ![x, y, z].into_iter().all(f32::is_finite) {
                    return Err("Native vertex transform overflow".into());
                }
                Ok(Vertex {
                    position: [x, y, -z],
                    uv: vertex.uv,
                })
            })
            .collect()
    }

    /// Rebuild only vertex values for an existing draw; topology and source geometry
    /// remain fixed. A crop's mapping continues to refer to the selected vertices.
    pub fn vertices(
        &self,
        binding: &DrawBinding,
        transforms: &[Mat4],
    ) -> Result<Vec<Vertex>, String> {
        if self.materialized.is_some() {
            return Err(
                "Materialized geometry requires its original matrix and checked pose".into(),
            );
        }
        let transform = *transforms
            .get(binding.frame_index)
            .ok_or("Unknown retained frame binding")?;
        let lod = self
            .source
            .frames
            .get(binding.geometry_frame)
            .and_then(|f| f.lods.first())
            .ok_or("Unknown retained geometry binding")?;
        binding
            .source_vertices
            .iter()
            .map(|index| {
                let vertex = lod
                    .vertices
                    .get(*index as usize)
                    .ok_or("Invalid retained vertex binding")?;
                let p = transform.transform_point3(Vec3::from_array(vertex.position));
                if !p.is_finite() {
                    return Err("Retained vertex transform overflow".into());
                }
                Ok(Vertex {
                    position: [p.x, p.y, -p.z],
                    uv: vertex.uv,
                })
            })
            .collect()
    }
}

/// Real I3D driver slot+8 (`PC-DRIVER-TICK-20261021`): shared wrapping time.
/// Constructor preserves this value; create/reset once from the proven Init-state,
/// not once per model or frame.
#[derive(Debug, PartialEq, Eq)]
pub struct DriverTickState {
    pub elapsed: u32,
}
impl DriverTickState {
    pub const fn from_existing(elapsed: u32) -> Self {
        Self { elapsed }
    }
    pub fn reset_for_init(&mut self) {
        self.elapsed = 0;
    }
    pub fn tick(&mut self, delta: i32) {
        self.elapsed = self.elapsed.wrapping_add(delta as u32);
    }
}

/// Host monotonic millisecond adapter. Its epoch is explicit host policy; it is
/// not an implementation of the original Win32 absolute timeGetTime epoch.
pub struct PlatformClock {
    origin: std::time::Instant,
}
impl Default for PlatformClock {
    fn default() -> Self {
        Self {
            origin: std::time::Instant::now(),
        }
    }
}
impl crate::clock::ClockBackend for PlatformClock {
    fn sample_ms(&mut self) -> u32 {
        self.origin.elapsed().as_millis() as u32
    }
    fn sleep_ms(&mut self, milliseconds: u32) {
        std::thread::sleep(std::time::Duration::from_millis(u64::from(milliseconds)));
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct RuntimeFrame {
    pub raw_delta: u32,
    pub modified_delta: u32,
    pub driver_time: u32,
    pub models_dispatched: usize,
}

/// Supported original-data static binding. Placement population/order is the
/// host's bounded inspection selection, not reconstructed Game mission state.
/// Required unknown actor/pose/mission callbacks remain errors. This does not
/// substitute successful no-op implementations for original dependencies.
pub struct RuntimeEngine {
    clock: crate::clock::EngineClock,
    modifier: crate::frame_delta::FrameDeltaState,
    driver: DriverTickState,
    scene: crate::scene_tick::LocalScene,
}
impl RuntimeEngine {
    pub fn from_sample(sample: u32) -> Self {
        let mut driver = DriverTickState::from_existing(0);
        driver.reset_for_init();
        Self {
            clock: crate::clock::EngineClock::from_sample(sample),
            modifier: crate::frame_delta::FrameDeltaState::default(),
            driver,
            scene: crate::scene_tick::LocalScene::default(),
        }
    }
    pub fn reset_clock<B: crate::clock::ClockBackend + ?Sized>(&mut self, backend: &mut B) {
        self.clock.null_timer(backend);
    }
    pub fn get_timer<B: crate::clock::ClockBackend + ?Sized>(
        &mut self,
        minimum: u32,
        maximum: u32,
        backend: &mut B,
    ) -> u32 {
        self.clock.get_timer(minimum, maximum, backend)
    }
    pub fn modify_delta(&mut self, delta: &mut u32, word: u16) -> Result<(), RuntimeError> {
        self.modifier
            .apply(Some(delta), &[], word)
            .map_err(RuntimeError::Modifier)
    }
    pub fn driver_time(&self) -> u32 {
        self.driver.elapsed
    }
    pub fn dispatch_static(
        &mut self,
        graph: &mut RetainedScene,
        delta: i32,
        word: u16,
    ) -> Result<usize, RuntimeError> {
        self.dispatch_bound(graph, delta, word, None)
    }
    /// Bind one explicitly selected original-data position model. Population and
    /// order remain the host inspection graph, not original mission construction.
    pub fn dispatch_position(
        &mut self,
        graph: &mut RetainedScene,
        model_index: usize,
        playback: &mut crate::animation_binding::PositionPlayback,
        delta: i32,
        word: u16,
    ) -> Result<usize, RuntimeError> {
        let model = graph
            .models
            .get(model_index)
            .ok_or(RuntimeError::Storage("Unknown animation placement"))?;
        if model.world != Mat4::IDENTITY || model.visual_scale.is_some() {
            return Err(RuntimeError::UnsupportedPhase(
                "Position binding requires identity wrapper placement",
            ));
        }
        if model.identity != playback.model_identity() {
            return Err(RuntimeError::UnsupportedPhase(
                "Animation placement identity mismatch",
            ));
        }
        if !Arc::ptr_eq(&model.source, playback.source_model()) {
            return Err(RuntimeError::UnsupportedPhase(
                "Animation source identity mismatch",
            ));
        }
        if word != 0x003f {
            return Err(RuntimeError::UnsupportedPhase(
                "Position evaluator requires PC24 nearest control word",
            ));
        }
        let object = model.identity;
        self.dispatch_bound(
            graph,
            delta,
            word,
            Some(ActivePlayback::Position(object, playback)),
        )
    }
    fn dispatch_bound(
        &mut self,
        graph: &mut RetainedScene,
        delta: i32,
        word: u16,
        playback: Option<ActivePlayback<'_>>,
    ) -> Result<usize, RuntimeError> {
        use crate::frame_dispatch::*;
        let records = graph
            .models
            .iter()
            .map(|model| ModelRecord {
                object: model.identity,
                opaque: 0,
            })
            .collect::<Vec<_>>();
        let mut state = FrameState {
            pending_flag: 0,
            pending_counter: 0,
            pending_target: None,
            pending_argument: 0,
            context_object: None,
            scene: Some(ObjectId(0xffff0001)),
            manager: Some(ObjectId(0xffff0002)),
            models: ModelView {
                allocation: ModelAllocationId(1),
                count: records.len(),
            },
            mission: None,
            timed: TimedView::default(),
            control_word: word,
        };
        let mut backend = StaticBackend {
            graph,
            records,
            driver: &mut self.driver,
            scene: &mut self.scene,
            dispatched: 0,
            playback,
        };
        state
            .tick(&mut backend, delta)
            .map_err(|error| RuntimeError::Dispatch(format!("{error:?}")))?;
        Ok(backend.dispatched)
    }
    /// Route one owned skeletal controller through the recovered frame dispatcher.
    pub fn dispatch_skeletal(
        &mut self,
        graph: &mut RetainedScene,
        model_index: usize,
        playback: &mut crate::skeletal_playback::SkeletalPlayback,
        delta: i32,
        word: u16,
    ) -> Result<usize, RuntimeError> {
        let model = graph
            .models
            .get(model_index)
            .ok_or(RuntimeError::Storage("Unknown animation placement"))?;
        model
            .validate_skeletal(playback)
            .map_err(RuntimeError::Animation)?;
        if word != 0x003f {
            return Err(RuntimeError::UnsupportedPhase(
                "Skeletal evaluator requires PC24 nearest control word",
            ));
        }
        let object = model.identity;
        self.dispatch_bound(
            graph,
            delta,
            word,
            Some(ActivePlayback::Skeletal(object, playback)),
        )
    }
    pub fn advance_skeletal<B: crate::clock::ClockBackend + ?Sized>(
        &mut self,
        graph: &mut RetainedScene,
        model_index: usize,
        playback: &mut crate::skeletal_playback::SkeletalPlayback,
        backend: &mut B,
    ) -> Result<RuntimeFrame, RuntimeError> {
        let raw_delta = self.get_timer(16, 83, backend);
        let mut delta = raw_delta;
        self.modify_delta(&mut delta, 0x003f)?;
        let models_dispatched =
            self.dispatch_skeletal(graph, model_index, playback, delta as i32, 0x003f)?;
        Ok(RuntimeFrame {
            raw_delta,
            modified_delta: delta,
            driver_time: self.driver.elapsed,
            models_dispatched,
        })
    }
    /// Advance the explicitly selected bounded position binding through the same
    /// clock, modifier and coordinator as static inspection.
    pub fn advance_position<B: crate::clock::ClockBackend + ?Sized>(
        &mut self,
        graph: &mut RetainedScene,
        model_index: usize,
        playback: &mut crate::animation_binding::PositionPlayback,
        backend: &mut B,
    ) -> Result<RuntimeFrame, RuntimeError> {
        let raw_delta = self.get_timer(16, 83, backend);
        let mut delta = raw_delta;
        self.modify_delta(&mut delta, 0x003f)?;
        let models_dispatched =
            self.dispatch_position(graph, model_index, playback, delta as i32, 0x003f)?;
        Ok(RuntimeFrame {
            raw_delta,
            modified_delta: delta,
            driver_time: self.driver.elapsed,
            models_dispatched,
        })
    }
    /// Host static-inspection profile, using the observed16/83 timer request and
    /// explicit empty-track modifier. It is not the entire original Game loop;
    /// original input, authored actors, camera, mission and presentation callers
    /// are not inferred from this local composition.
    pub fn advance_static<B: crate::clock::ClockBackend + ?Sized>(
        &mut self,
        graph: &mut RetainedScene,
        backend: &mut B,
    ) -> Result<RuntimeFrame, RuntimeError> {
        let raw_delta = self.get_timer(16, 83, backend);
        let mut delta = raw_delta;
        self.modify_delta(&mut delta, 0x003f)?;
        let models_dispatched = self.dispatch_static(graph, delta as i32, 0x003f)?;
        Ok(RuntimeFrame {
            raw_delta,
            modified_delta: delta,
            driver_time: self.driver.elapsed,
            models_dispatched,
        })
    }
}
enum ActivePlayback<'a> {
    Position(ObjectId, &'a mut crate::animation_binding::PositionPlayback),
    Skeletal(ObjectId, &'a mut crate::skeletal_playback::SkeletalPlayback),
}
struct StaticBackend<'a> {
    graph: &'a mut RetainedScene,
    records: Vec<crate::frame_dispatch::ModelRecord>,
    driver: &'a mut DriverTickState,
    scene: &'a mut crate::scene_tick::LocalScene,
    dispatched: usize,
    playback: Option<ActivePlayback<'a>>,
}
struct NoActiveFrames;
impl crate::scene_tick::SceneBackend for NoActiveFrames {
    fn tick_frame(&mut self, _: &mut crate::scene_tick::LocalScene, _: FrameId, _: i32) {
        unreachable!("Static-domain active frame guard");
    }
    fn tick_interpolator(
        &mut self,
        _: &mut crate::scene_tick::LocalScene,
        _: InterpolatorId,
        _: i32,
    ) {
        unreachable!("Static-domain scene interpolator guard");
    }
}
impl crate::frame_dispatch::FrameBackend for StaticBackend<'_> {
    type Error = RuntimeError;
    fn model_at(
        &self,
        allocation: crate::frame_dispatch::ModelAllocationId,
        index: usize,
    ) -> Result<crate::frame_dispatch::ModelRecord, Self::Error> {
        if allocation.0 != 1 {
            return Err(RuntimeError::Storage("Unknown model allocation"));
        }
        self.records
            .get(index)
            .copied()
            .ok_or(RuntimeError::Storage("Invalid model record"))
    }
    fn timed_at(
        &self,
        _: crate::frame_dispatch::TimedAllocationId,
        _: usize,
    ) -> Result<crate::frame_dispatch::TimedRecord, Self::Error> {
        Err(RuntimeError::UnsupportedPhase("Timed-object storage"))
    }
    fn write_timed(
        &mut self,
        _: crate::frame_dispatch::TimedAllocationId,
        _: usize,
        _: crate::frame_dispatch::TimedRecord,
    ) -> Result<(), Self::Error> {
        Err(RuntimeError::UnsupportedPhase("Timed-object storage"))
    }
    fn pending(
        &mut self,
        _: &mut crate::frame_dispatch::FrameState,
        _: ObjectId,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err(RuntimeError::UnsupportedPhase("Sound callback"))
    }
    fn context_tick(
        &mut self,
        _: &mut crate::frame_dispatch::FrameState,
        _: ObjectId,
        _: i32,
    ) -> Result<(), Self::Error> {
        Err(RuntimeError::UnsupportedPhase("Context560510"))
    }
    fn scene_tick(
        &mut self,
        _: &mut crate::frame_dispatch::FrameState,
        object: ObjectId,
        delta: i32,
    ) -> Result<(), Self::Error> {
        if object != ObjectId(0xffff0001) {
            return Err(RuntimeError::UnknownObject(object));
        }
        if !self.scene.frames().is_empty() || self.scene.interpolator().is_some() {
            return Err(RuntimeError::UnsupportedPhase("Active scene callbacks"));
        }
        self.scene.tick(&mut NoActiveFrames, delta);
        Ok(())
    }
    fn manager_tick(
        &mut self,
        _: &mut crate::frame_dispatch::FrameState,
        object: ObjectId,
        delta: i32,
    ) -> Result<(), Self::Error> {
        if object != ObjectId(0xffff0002) {
            return Err(RuntimeError::UnknownObject(object));
        }
        self.driver.tick(delta);
        Ok(())
    }
    fn model_tick(
        &mut self,
        _: &mut crate::frame_dispatch::FrameState,
        object: ObjectId,
        delta: i32,
    ) -> Result<(), Self::Error> {
        let model = self
            .graph
            .models
            .iter_mut()
            .find(|model| model.identity == object)
            .ok_or(RuntimeError::UnknownObject(object))?;
        match &mut self.playback {
            Some(ActivePlayback::Position(identity, playback)) if *identity == object => {
                let observation = playback
                    .tick(&mut model.native, delta)
                    .map_err(|error| RuntimeError::Animation(format!("{error:?}")))?;
                model
                    .set_frame_position(0, observation.position)
                    .map_err(RuntimeError::Animation)?;
            }
            Some(ActivePlayback::Skeletal(identity, playback)) if *identity == object => {
                model
                    .validate_skeletal(playback)
                    .map_err(RuntimeError::Animation)?;
                let loaded = model
                    .materialized
                    .as_mut()
                    .ok_or(RuntimeError::Storage("Missing skeletal materialization"))?;
                let before = loaded
                    .storage_bytes()
                    .map_err(|error| RuntimeError::Animation(error.to_string()))?;
                let changed = playback
                    .tick(loaded, delta)
                    .map_err(|error| RuntimeError::Animation(error.to_string()))?;
                let after = loaded
                    .storage_bytes()
                    .map_err(|error| RuntimeError::Animation(error.to_string()))?;
                let total = self
                    .graph
                    .budgeted_bytes
                    .checked_add(after.saturating_sub(before))
                    .ok_or(RuntimeError::Storage("Animation allocation overflow"))?;
                if total > 256 * 1024 * 1024 {
                    return Err(RuntimeError::Storage(
                        "Retained animation exceeds scene memory budget",
                    ));
                }
                self.graph.budgeted_bytes = total;
                model.needs_upload |= changed;
            }
            _ => model.tick_static(delta)?,
        }
        self.dispatched += 1;
        Ok(())
    }
    fn mission_enabled(&self, _: ObjectId) -> Result<bool, Self::Error> {
        Err(RuntimeError::UnsupportedPhase(
            "Authored mission initialization",
        ))
    }
    fn mission_tick(
        &mut self,
        _: &mut crate::frame_dispatch::FrameState,
        _: ObjectId,
        _: i32,
    ) -> Result<(), Self::Error> {
        Err(RuntimeError::UnsupportedPhase("Authored mission update"))
    }
    fn set_alpha(
        &mut self,
        _: &mut crate::frame_dispatch::FrameState,
        _: ObjectId,
        _: u32,
    ) -> Result<(), Self::Error> {
        Err(RuntimeError::UnsupportedPhase("Timed alpha update"))
    }
    fn release(
        &mut self,
        _: &mut crate::frame_dispatch::FrameState,
        _: ObjectId,
    ) -> Result<(), Self::Error> {
        Err(RuntimeError::UnsupportedPhase("Timed release"))
    }
}

impl RetainedScene {
    pub fn upload_skeletal_changed(
        &mut self,
        renderer: &lhr_render::Renderer,
        model_index: usize,
        playback: &crate::skeletal_playback::SkeletalPlayback,
    ) -> Result<usize, String> {
        self.models
            .get(model_index)
            .ok_or("Unknown animation placement")?
            .validate_skeletal(playback)?;
        self.upload_changed(renderer)
    }
    /// The lazy getter executes even for a clean upload: native cache flags are
    /// shared with animation finalization, independently of the GPU upload cache.
    pub fn upload_position_changed(
        &mut self,
        renderer: &lhr_render::Renderer,
        playback: &mut crate::animation_binding::PositionPlayback,
    ) -> Result<usize, String> {
        let model = self
            .models
            .iter_mut()
            .find(|m| m.identity == playback.model_identity())
            .ok_or("Unknown animation placement")?;
        if !Arc::ptr_eq(&model.source, playback.source_model())
            || model.world.to_cols_array().map(f32::to_bits)
                != Mat4::IDENTITY.to_cols_array().map(f32::to_bits)
            || model.visual_scale.is_some()
            || model.pose.len() != 1
            || model.pose[0].position.map(f32::to_bits)
                != playback.observation().position.map(f32::to_bits)
        {
            return Err("Native animation binding diverged from retained placement".into());
        }
        let world = playback
            .world_matrix()
            .map_err(|error| format!("{error:?}"))?;
        if !world.is_verified_input() {
            return Err("Native world matrix outside verified numeric domain".into());
        }
        if !model.needs_upload
            && playback.uploaded_world() == Some(world.elements.map(f32::to_bits))
        {
            return Ok(0);
        }
        let updates = model
            .draws
            .iter()
            .map(|binding| {
                model
                    .vertices_native(binding, &world)
                    .map(|vertices| (binding.draw_index, vertices))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let count = updates.len();
        for (draw, vertices) in updates {
            renderer.update_mesh_vertices(draw, &vertices)?;
        }
        model.needs_upload = false;
        playback.mark_uploaded(world);
        Ok(count)
    }

    /// Upload only explicitly changed, supported bound poses. Draw counts and
    /// topology are fixed by original-data assembly; renderer validation is kept.
    pub fn upload_changed(&mut self, renderer: &lhr_render::Renderer) -> Result<usize, String> {
        let mut count = 0;
        for model in &mut self.models {
            if !model.needs_upload {
                continue;
            }
            let updates = if model.materialized.is_some() {
                let matrices = model.materialized_world_matrices()?;
                model
                    .draws
                    .iter()
                    .map(|binding| {
                        model
                            .vertices_materialized(binding, &matrices)
                            .map(|vertices| (binding.draw_index, vertices))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                let transforms = model.transforms()?;
                model
                    .draws
                    .iter()
                    .map(|binding| {
                        model
                            .vertices(binding, &transforms)
                            .map(|vertices| (binding.draw_index, vertices))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
            for (draw, vertices) in updates {
                renderer.update_mesh_vertices(draw, &vertices)?;
                count += 1;
            }
            model.needs_upload = false;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_geometry_keeps_translation_association_and_handedness() {
        let mut matrix = crate::native_transform::SMatrix::identity();
        matrix.elements[0] = 16_777_216.;
        matrix.elements[4] = 1.;
        matrix.elements[8] = 1.;
        matrix.elements[12] = -16_777_216.;
        assert_eq!(
            GeometryTransform::Original(matrix)
                .render_point([1.; 3])
                .unwrap(),
            [2., 1., -1.]
        );
        assert!(GeometryTransform::Original(matrix)
            .render_point([f32::MAX, 1., 1.])
            .is_err());
    }
    #[test]
    fn original_geometry_admits_small_normal_values_and_keeps_guard_errors() {
        let mut matrix = crate::native_transform::SMatrix::identity();
        matrix.elements[12] = 1e-20;
        for i in [3, 7, 11, 15] {
            matrix.elements[i] = f32::NAN;
        }
        assert_eq!(
            GeometryTransform::Original(matrix)
                .source_point([1e-20, 0., 0.])
                .unwrap(),
            [2e-20, 0., 0.]
        );
        matrix.elements[0] = f32::MAX;
        assert_eq!(
            GeometryTransform::Original(matrix).source_point([2., 0., 0.]),
            Err("Original point transform outside verified numeric domain".to_owned())
        );
    }
    use lhr_assets::{Frame, Lod};
    fn source() -> Arc<Model> {
        Arc::new(Model {
            materials: vec![],
            frames: vec![Frame {
                kind: 1,
                visual: Some(0),
                render_flags: 0,
                logic_flags: 0,
                culling: 0,
                name: "source".into(),
                name_bytes: b"source".to_vec(),
                properties: Vec::new(),
                dummy_bounds: None,
                light: None,
                joint: None,
                skin: None,
                morph: None,
                lensflare: None,
                target: None,
                occluder: None,
                sector: None,
                parent: None,
                position: [0.; 3],
                scale: [1.; 3],
                rotation_wxyz: [1., 0., 0., 0.],
                instance: None,
                lods: vec![Lod {
                    distance: 0.,
                    uv_only: None,
                    vertices: vec![
                        lhr_assets::Vertex {
                            position: [1., 2., 3.],
                            normal: [0., 1., 0.],
                            uv: [0.25, 0.75],
                        },
                        lhr_assets::Vertex {
                            position: [4., 5., 6.],
                            normal: [0., 1., 0.],
                            uv: [0.5, 0.5],
                        },
                    ],
                    groups: vec![],
                }],
            }],
        })
    }
    fn graph() -> RetainedScene {
        let mut graph = RetainedScene::default();
        let source = source();
        for i in 0..2 {
            graph
                .retain(
                    format!("model{i}"),
                    source.clone(),
                    Mat4::IDENTITY,
                    None,
                    vec![DrawBinding {
                        draw_index: i,
                        frame_index: 0,
                        geometry_frame: 0,
                        source_vertices: vec![1, 0],
                    }],
                )
                .unwrap();
        }
        graph
    }
    #[test]
    fn materialized_retention_preserves_source_and_rejects_partial_or_legacy_pose_paths() {
        let source = source();
        let loaded = crate::model_loading::MaterializedModel::from_parsed(source.clone()).unwrap();
        let mut scene = RetainedScene::default();
        scene
            .retain_materialized("fixture".into(), loaded, Vec::new())
            .unwrap();
        let retained = &mut scene.models[0];
        assert!(Arc::ptr_eq(&retained.source, &source));
        assert!(retained.transforms().is_err());
        assert!(retained.set_frame_position(0, [1.; 3]).is_err());
        let binding = DrawBinding {
            draw_index: 0,
            frame_index: 0,
            geometry_frame: 0,
            source_vertices: vec![0],
        };
        assert!(retained
            .vertices_native(&binding, &crate::native_transform::SMatrix::identity())
            .is_err());
        assert!(retained.vertices(&binding, &[Mat4::IDENTITY]).is_err());
        retained.tick_static(16).unwrap();
        let mut incomplete = (*source).clone();
        let mut unsupported = incomplete.frames[0].clone();
        unsupported.kind = 2;
        incomplete.frames.push(unsupported);
        let failure =
            crate::model_loading::MaterializedModel::from_parsed(Arc::new(incomplete)).unwrap_err();
        assert_eq!(failure.partial.source_frame_keys().len(), 1);
        assert!(scene
            .retain_materialized("partial".into(), *failure.partial, Vec::new())
            .is_err());
        assert_eq!(scene.models.len(), 1);
    }
    fn skeletal_fixture_animation() -> Arc<lhr_assets::animation::AnimationSet> {
        let mut payload = vec![1, 0, 10, 0];
        payload.extend(12u32.to_le_bytes());
        payload.extend(19u32.to_le_bytes());
        payload.extend(b"source\0");
        payload.extend(2u16.to_le_bytes());
        payload.extend(0u16.to_le_bytes());
        payload.extend(2u16.to_le_bytes());
        payload.extend(0u16.to_le_bytes());
        payload.extend(10u16.to_le_bytes());
        payload.extend(0u16.to_le_bytes());
        for value in [0f32, 0., 0., 10., 0., 0.] {
            payload.extend(value.to_le_bytes());
        }
        let mut bytes = b"5DS\0".to_vec();
        bytes.extend(20u16.to_le_bytes());
        bytes.extend([0u8; 8]);
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(payload);
        Arc::new(lhr_assets::animation::parse_5ds(&bytes).unwrap())
    }
    fn skeletal_graph() -> RetainedScene {
        let source = source();
        let mut graph = RetainedScene::default();
        for i in 0..2 {
            let loaded =
                crate::model_loading::MaterializedModel::from_parsed(source.clone()).unwrap();
            graph
                .retain_materialized(
                    format!("skeletal{i}"),
                    loaded,
                    vec![DrawBinding {
                        draw_index: i,
                        frame_index: 0,
                        geometry_frame: 0,
                        source_vertices: vec![1, 0],
                    }],
                )
                .unwrap();
        }
        graph
    }
    #[test]
    fn materialized_geometry_rejects_mutated_public_source_and_placement_before_getters() {
        let mut graph = skeletal_graph();
        let model = &mut graph.models[0];
        let original = model.source.clone();
        let matrices = model.materialized_world_matrices().unwrap();
        assert!(model
            .vertices_materialized(&model.draws[0], &matrices)
            .is_ok());
        // A byte-identical clone still is not the owned hierarchy's source Arc.
        model.source = Arc::new((*original).clone());
        assert!(model.validate_materialized().is_err());
        assert!(model.materialized_world_matrices().is_err());
        assert!(model
            .vertices_materialized(&model.draws[0], &matrices)
            .is_err());
        model.source = original;
        model.world = Mat4::from_translation(Vec3::X);
        assert!(model.validate_materialized().is_err());
        assert!(model.materialized_world_matrices().is_err());
        assert!(model
            .vertices_materialized(&model.draws[0], &matrices)
            .is_err());
        model.world = Mat4::IDENTITY;
        model.visual_scale = Some([1.; 3]);
        assert!(model.validate_materialized().is_err());
        assert!(model.materialized_world_matrices().is_err());
        assert!(model
            .vertices_materialized(&model.draws[0], &matrices)
            .is_err());
        model.visual_scale = None;
        assert!(model.materialized_world_matrices().is_ok());
        assert!(model
            .vertices_materialized(&model.draws[0], &matrices)
            .is_ok());
    }
    #[test]
    fn sequential_skeletal_dispatch_refreshes_owned_vertices_and_keeps_placement_identity() {
        let mut graph = skeletal_graph();
        let mut playback = graph
            .bind_skeletal(0, skeletal_fixture_animation())
            .unwrap();
        let initial_budget = graph.budgeted_bytes;
        assert!(initial_budget > 0);
        assert!(graph
            .bind_skeletal(0, skeletal_fixture_animation())
            .is_err());
        assert!(graph.models[0].tick_static(16).is_err());
        let mut engine = RuntimeEngine::from_sample(0);
        let mut clock = Clock {
            samples: [40, 80].into(),
            sleeps: Vec::new(),
        };
        for _ in 0..2 {
            let frame = engine
                .advance_skeletal(&mut graph, 0, &mut playback, &mut clock)
                .unwrap();
            assert_eq!(frame.raw_delta, 40);
            assert_eq!(frame.models_dispatched, 2);
        }
        let model = &mut graph.models[0];
        assert!(model.needs_upload);
        let matrices = model
            .materialized
            .as_mut()
            .unwrap()
            .world_matrices()
            .unwrap();
        let vertices = model
            .vertices_materialized(&model.draws[0], &matrices)
            .unwrap();
        assert_eq!(vertices[0].position, [6., 5., -6.]);
        assert_eq!(vertices[0].uv, [0.5, 0.5]);
        assert_eq!(vertices[1].position, [3., 2., -3.]);
        assert_eq!(model.source.frames[0].position, [0.; 3]);
        assert_eq!(engine.driver_time(), 80);
        assert_eq!(graph.budgeted_bytes, initial_budget);
        assert!(engine
            .dispatch_skeletal(&mut graph, 0, &mut playback, 16, 0x37)
            .is_err());
        graph
            .bind_skeletal(1, skeletal_fixture_animation())
            .unwrap();
        assert!(engine
            .dispatch_skeletal(&mut graph, 1, &mut playback, 16, 0x3f)
            .is_err());
    }
    #[test]
    fn skeletal_binding_obeys_scene_budget_and_requires_owned_materialization() {
        assert!(graph()
            .bind_skeletal(0, skeletal_fixture_animation())
            .is_err());
        let mut graph = skeletal_graph();
        graph.set_assembly_budget(256 * 1024 * 1024).unwrap();
        assert!(graph
            .bind_skeletal(0, skeletal_fixture_animation())
            .is_err());
        assert_eq!(graph.budgeted_bytes, 256 * 1024 * 1024);
        assert!(graph.models[0].tick_static(16).is_err());
    }
    #[test]
    fn native_vertices_preserve_mapping_handedness_and_explicit_numeric_domain() {
        let mut graph = graph();
        let model = &mut graph.models[0];
        let mut world = crate::native_transform::SMatrix::identity();
        world.elements[12..15].copy_from_slice(&[10., 20., 30.]);
        let vertices = model.vertices_native(&model.draws[0], &world).unwrap();
        assert_eq!(vertices[0].position, [14., 25., -36.]);
        assert_eq!(vertices[1].position, [11., 22., -33.]);
        assert_eq!(vertices[0].uv, [0.5, 0.5]);
        world.elements[0] = f32::from_bits(1);
        assert!(model.vertices_native(&model.draws[0], &world).is_err());
        model.draws[0].frame_index = 1;
        assert!(model
            .vertices_native(
                &model.draws[0],
                &crate::native_transform::SMatrix::identity()
            )
            .is_err());
        model.set_frame_position(0, [-0., 0., 0.]).unwrap();
        assert_eq!(model.pose()[0].position[0].to_bits(), (-0.0f32).to_bits());
        assert_eq!(model.source.frames[0].position, [0.; 3]);
    }

    #[test]
    fn driver_tick_matches_six_native_wrapping_vectors_and_init_policy() {
        for (previous, delta, expected) in [
            (0, 83, 83),
            (83, 0, 83),
            (u32::MAX, 1, 0),
            (i32::MAX as u32, 1, 0x80000000),
            (10, -7, 3),
            (0x80000000, i32::MIN, 0),
        ] {
            let mut driver = DriverTickState::from_existing(previous);
            assert_eq!(driver.elapsed, previous);
            driver.tick(delta);
            assert_eq!(driver.elapsed, expected);
        }
        let mut driver = DriverTickState::from_existing(123);
        assert_eq!(driver.elapsed, 123);
        driver.reset_for_init();
        assert_eq!(driver.elapsed, 0);
        driver.tick(7);
        driver.tick(-1);
        assert_eq!(driver.elapsed, 6);
    }
    #[test]
    fn shared_source_has_distinct_placement_and_frame_identity() {
        let graph = graph();
        assert!(Arc::ptr_eq(
            &graph.models[0].source,
            &graph.models[1].source
        ));
        assert_ne!(graph.models[0].identity, graph.models[1].identity);
        assert_ne!(
            graph.models[0].pose()[0].identity,
            graph.models[1].pose()[0].identity
        );
        assert_eq!(graph.models[0].native.interpolator, None);
    }
    #[test]
    fn bound_pose_rebuild_preserves_source_vertex_mapping_and_other_placement() {
        let mut graph = graph();
        let model = &mut graph.models[0];
        let original = model
            .vertices(&model.draws[0], &model.transforms().unwrap())
            .unwrap();
        assert_eq!(original[0].position, [4., 5., -6.]);
        assert_eq!(original[0].uv, [0.5, 0.5]);
        model.set_frame_position(0, [10., 0., 0.]).unwrap();
        assert!(model.needs_upload);
        let moved = model
            .vertices(&model.draws[0], &model.transforms().unwrap())
            .unwrap();
        assert_eq!(moved[0].position, [14., 5., -6.]);
        assert_eq!(graph.models[1].pose()[0].position, [0.; 3]);
        assert_eq!(graph.models[0].source.frames[0].position, [0.; 3]);
        assert!(graph.models[0]
            .set_frame_position(0, [f32::NAN, 0., 0.])
            .is_err());
        assert_eq!(graph.models[0].pose()[0].position, [10., 0., 0.]);
    }
    #[test]
    fn inspection_placement_updates_all_roots_and_children_without_changing_source() {
        let mut source = (*source()).clone();
        let mut child = source.frames[0].clone();
        child.parent = Some(0);
        child.position = [2., 0., 0.];
        source.frames.push(child);
        let mut root = source.frames[0].clone();
        root.position = [-3., 0., 0.];
        source.frames.push(root);
        let source = Arc::new(source);
        let mut graph = RetainedScene::default();
        for index in 0..2 {
            graph
                .retain(
                    "placement".into(),
                    source.clone(),
                    Mat4::IDENTITY,
                    None,
                    (0..3)
                        .map(|frame| DrawBinding {
                            draw_index: index * 3 + frame,
                            frame_index: frame,
                            geometry_frame: frame,
                            source_vertices: vec![1, 0],
                        })
                        .collect(),
                )
                .unwrap();
        }
        let old_pose = graph.models[0].pose.clone();
        let placement = Mat4::from_cols_array(&[
            0., 0., -1., 0., 0., 1., 0., 0., 1., 0., 0., 0., 10., 0., 20., 1.,
        ]);
        graph.models[0].set_world_transform(placement).unwrap();
        let model = &graph.models[0];
        let transforms = model.transforms().unwrap();
        for (frame, x) in [4., 6., 1.].into_iter().enumerate() {
            let vertices = model.vertices(&model.draws[frame], &transforms).unwrap();
            assert_eq!(vertices[0].position, [16., 5., x - 20.]);
            assert_eq!(vertices[0].uv, [0.5, 0.5]);
        }
        assert!(model.needs_upload);
        assert_eq!(model.pose, old_pose);
        assert_eq!(model.source.frames[0].position, [0.; 3]);
        assert_eq!(graph.models[1].world, Mat4::IDENTITY);
        assert!(!graph.models[1].needs_upload);
        graph.models[0].needs_upload = false;
        graph.models[0].set_world_transform(placement).unwrap();
        assert!(!graph.models[0].needs_upload);
    }
    #[test]
    fn rejected_inspection_placement_preserves_world_and_pending_upload() {
        for pending in [false, true] {
            let mut graph = graph();
            let model = &mut graph.models[0];
            model.needs_upload = pending;
            let previous = model.world.to_cols_array().map(f32::to_bits);
            for world in [
                Mat4::from_translation(Vec3::new(f32::NAN, 0., 0.)),
                Mat4::ZERO,
                Mat4::from_scale(Vec3::splat(f32::MAX)),
            ] {
                assert!(model.set_world_transform(world).is_err());
                assert_eq!(model.world.to_cols_array().map(f32::to_bits), previous);
                assert_eq!(model.needs_upload, pending);
            }
            model.pose[0].position = [f32::MAX, 0., 0.];
            assert!(model
                .set_world_transform(Mat4::from_translation(Vec3::new(f32::MAX, 0., 0.)))
                .is_err());
            assert_eq!(model.world.to_cols_array().map(f32::to_bits), previous);
            assert_eq!(model.needs_upload, pending);
        }
    }
    #[test]
    fn inspection_placement_cannot_bypass_original_or_animation_bindings() {
        let loaded = crate::model_loading::MaterializedModel::from_parsed(source()).unwrap();
        let mut original = RetainedScene::default();
        original
            .retain_materialized("original".into(), loaded, vec![])
            .unwrap();
        let mut graph = graph();
        let moved = Mat4::from_translation(Vec3::X);
        assert!(original.models[0].set_world_transform(moved).is_err());
        assert_eq!(original.models[0].world, Mat4::IDENTITY);
        graph.models[0].native.interpolator = Some(InterpolatorId(1));
        assert!(graph.models[0].set_world_transform(moved).is_err());
        assert_eq!(graph.models[0].world, Mat4::IDENTITY);
        graph.models[0].native.interpolator = None;
        graph.models[0].skeletal_bound = true;
        assert!(graph.models[0].set_world_transform(moved).is_err());
        assert_eq!(graph.models[0].world, Mat4::IDENTITY);
        assert!(!graph.models[0].needs_upload);
    }
    struct Clock {
        samples: std::collections::VecDeque<u32>,
        sleeps: Vec<u32>,
    }
    impl crate::clock::ClockBackend for Clock {
        fn sample_ms(&mut self) -> u32 {
            self.samples.pop_front().unwrap()
        }
        fn sleep_ms(&mut self, delay: u32) {
            self.sleeps.push(delay);
        }
    }
    #[test]
    fn real_components_compose_with_one_shared_clock_and_driver() {
        let mut graph = graph();
        let mut engine = RuntimeEngine::from_sample(0);
        let mut clock = Clock {
            samples: [10, 40].into(),
            sleeps: vec![],
        };
        assert_eq!(
            engine.advance_static(&mut graph, &mut clock).unwrap(),
            RuntimeFrame {
                raw_delta: 16,
                modified_delta: 16,
                driver_time: 16,
                models_dispatched: 2
            }
        );
        assert_eq!(
            engine.advance_static(&mut graph, &mut clock).unwrap(),
            RuntimeFrame {
                raw_delta: 24,
                modified_delta: 24,
                driver_time: 40,
                models_dispatched: 2
            }
        );
        assert_eq!(clock.sleeps, vec![6]);
        assert!(clock.samples.is_empty());
        assert_eq!(engine.modifier.raw_accumulated, 40);
        assert_eq!(engine.modifier.derived_position, 40);
    }
    #[test]
    fn unbound_animation_fails_without_a_fake_evaluator_or_finalize() {
        let mut graph = graph();
        graph.models[1].native.interpolator = Some(InterpolatorId(9));
        let mut engine = RuntimeEngine::from_sample(0);
        let error = engine.dispatch_static(&mut graph, 7, 0x3f).unwrap_err();
        assert!(format!("{error:?}").contains("UnknownInterpolator"));
        assert_eq!(engine.driver_time(), 7); // preceding native phase already ran
    }
    #[test]
    fn active_scene_requires_binding_instead_of_silent_noop() {
        let mut engine = RuntimeEngine::from_sample(0);
        engine.scene.frames_mut().push(FrameId(1));
        assert!(engine.dispatch_static(&mut graph(), 7, 0x3f).is_err());
        assert_eq!(engine.driver_time(), 0);
    }
    #[test]
    fn retained_metadata_obeys_scene_memory_budget() {
        let mut graph = RetainedScene::default();
        graph.set_assembly_budget(256 * 1024 * 1024).unwrap();
        assert!(graph.retain_metadata(lhr_assets::Scene::default()).is_err());
        assert!(graph.metadata().is_none());
        graph.set_assembly_budget(0).unwrap();
        graph.retain_metadata(lhr_assets::Scene::default()).unwrap();
        let bytes = graph.budgeted_bytes;
        graph.retain_metadata(lhr_assets::Scene::default()).unwrap();
        assert_eq!(graph.budgeted_bytes, bytes);
    }
    #[test]
    fn runtime_retention_rejects_an_emitter_not_admitted_by_static_parser() {
        let mut source = (*source()).clone();
        source.frames[0].kind = 17;
        assert!(RetainedScene::default()
            .retain(
                "emitter".into(),
                Arc::new(source),
                Mat4::IDENTITY,
                None,
                vec![]
            )
            .is_err());
    }
}
