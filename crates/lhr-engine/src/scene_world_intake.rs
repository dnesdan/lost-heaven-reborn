//! Canonical CPU world-source adapter, NOT original SceneOpen/LoadHierarchy.
//! Actual static source frames/resources are staged in a real temporary model's
//! member list. Source roots link to the genuine Primary sector only after all
//! image records load, in file order; the temporary model is NEVER their parent.
//! Every frame then transfers through canonical SceneAddFrame into the actual
//! scene, followed by Primary Update and loaded-sector preparation. Source names,
//! earlier-parent links and real resources survive. Original flags6/7, loadingUI
//! callback540fd0→5bcc10, native allocation/error timing and GPU upload remain
//! outside this CPU adapter. No original MissionOpen success is implied.
use super::*;
use crate::scene_ownership::SceneKey;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuSceneWorldPhase {
    Admission,
    Input,
    Geometry,
    Resources,
    Transfer,
    Prepare,
    Complete,
}
#[derive(Debug)]
pub enum CpuSceneWorldError {
    Model(ModelError),
    DriverDomain(String),
}
#[derive(Debug)]
pub struct CpuSceneWorldProgress {
    scene: SceneKey,
    domain: crate::script_scheduler::FrameReferenceId,
    staging_model: Option<ModelKey>,
    frames: Vec<FrameKey>,
    transferred: usize,
    prepared_sectors: usize,
    phase: CpuSceneWorldPhase,
}
impl CpuSceneWorldProgress {
    pub fn scene(&self) -> SceneKey {
        self.scene
    }
    pub fn staging_model(&self) -> Option<ModelKey> {
        self.staging_model
    }
    pub fn frames(&self) -> &[FrameKey] {
        &self.frames
    }
    pub fn transferred_frames(&self) -> usize {
        self.transferred
    }
    pub fn prepared_sectors(&self) -> usize {
        self.prepared_sectors
    }
    pub fn phase(&self) -> CpuSceneWorldPhase {
        self.phase
    }
    pub(crate) fn empty(arena: &ModelArena, scene: SceneKey) -> Self {
        Self {
            scene,
            domain: arena.lookup_reference(FrameKey(0)),
            staging_model: None,
            frames: Vec::new(),
            transferred: 0,
            prepared_sectors: 0,
            phase: CpuSceneWorldPhase::Admission,
        }
    }
}
#[derive(Debug)]
pub struct CpuSceneWorldFailure {
    pub error: CpuSceneWorldError,
    pub progress: CpuSceneWorldProgress,
}
impl ModelArena {
    pub(crate) fn load_scene_world_cpu(
        &mut self,
        scene: SceneKey,
        installation: &lhr_assets::Installation,
        name: &str,
    ) -> Result<CpuSceneWorldProgress, CpuSceneWorldFailure> {
        let mut progress = CpuSceneWorldProgress::empty(self, scene);
        let result = (|| -> Result<(), ModelError> {
            let si = self.scene_index(scene)?;
            let primary = self.scenes[si].sectors[0];
            if self.scenes[si].cpu_world_intake_active
                || !self.scenes[si].frames.is_empty()
                || self.links(primary.frame())?.child.is_some()
            {
                return Err(ModelError::OriginalModelOpenUnavailable(
                    "CPU world intake requires an empty scene and no interrupted intake",
                ));
            }
            progress.phase = CpuSceneWorldPhase::Input;
            let path = original_static_geometry_path(name.as_bytes())?;
            let bytes = installation
                .read_original_path(&path)
                .map_err(|e| ModelError::Asset(e.to_string()))?;
            let source = Arc::new(
                lhr_assets::parse_four_ds_scene(&bytes)
                    .map_err(|e| ModelError::Asset(e.to_string()))?,
            );
            self.load_scene_world_parsed_cpu(scene, installation, source, &mut progress)
        })();
        match result {
            Ok(()) => Ok(progress),
            Err(error) => Err(CpuSceneWorldFailure {
                error: CpuSceneWorldError::Model(error),
                progress,
            }),
        }
    }
    fn load_scene_world_parsed_cpu(
        &mut self,
        scene: SceneKey,
        installation: &lhr_assets::Installation,
        source: Arc<Model>,
        progress: &mut CpuSceneWorldProgress,
    ) -> Result<(), ModelError> {
        let si = self.scene_index(scene)?;
        let primary = self.scenes[si].sectors[0];
        for (index, frame) in source.frames.iter().enumerate() {
            if !matches!(
                (frame.kind, frame.visual),
                (1, Some(0)) | (6, None) | (5, None)
            ) {
                return Err(ModelError::UnsupportedFrame {
                    index,
                    kind: frame.kind,
                    visual: frame.visual,
                });
            }
        }
        self.scenes[si].cpu_world_intake_active = true;
        let staging = self.create_model()?;
        progress.staging_model = Some(staging);
        progress.phase = CpuSceneWorldPhase::Geometry;
        let content =
            match self.load_parsed_into_model_for_root(staging, source, Some(primary.frame())) {
                Ok(content) => content,
                Err(failure) => {
                    progress.frames = failure.partial.frames;
                    return Err(failure.error);
                }
            };
        progress.frames = content.frames.clone();
        progress.phase = CpuSceneWorldPhase::Resources;
        self.load_plain_visual_resources(staging, installation)?;
        progress.phase = CpuSceneWorldPhase::Transfer;
        for &frame in &progress.frames {
            // Both AddFrame effects are real: retain, delete prior model owner,
            // assign Scene owner, grow/append its own actual member array.
            self.scene_add_frame(scene, frame)?;
            progress.transferred += 1;
        }
        self.release(staging.root())?;
        progress.staging_model = None;
        progress.phase = CpuSceneWorldPhase::Prepare;
        self.mark_loaded_model_root(primary.frame())?;
        // Traverse actual primary child/next identities; never manufacture map
        // entries or sector registration from parsed metadata alone.
        let mut stack = Vec::new();
        if let Some(child) = self.links(primary.frame())?.child {
            stack.push(child);
        }
        let mut visited = std::collections::HashSet::new();
        while let Some(frame) = stack.pop() {
            if !visited.insert(frame.0) || visited.len() > self.frames.len() {
                return Err(ModelError::OwnershipCycle);
            }
            let links = self.links(frame)?;
            if self.state(frame)?.kind == FrameKind::Sector {
                let key = self
                    .sectors
                    .iter()
                    .find(|s| s.key.frame() == frame)
                    .ok_or(crate::frame_sector::SectorError::Buffer)?
                    .key;
                self.sector_make_ready(key)?;
                progress.prepared_sectors += 1;
            }
            if let Some(next) = links.next {
                stack.push(next);
            }
            if let Some(child) = links.child {
                stack.push(child);
            }
        }
        self.scenes[si].cpu_world_intake_active = false;
        progress.phase = CpuSceneWorldPhase::Complete;
        Ok(())
    }
    /// Explicit terminal disposal of still-staged CPU owners, NOT native
    /// exception unwinding or rollback. Already-transferred Scene members remain
    /// owned by their Scene and must close through the canonical scene owner.
    pub(crate) fn discard_scene_world_staging_cpu(
        &mut self,
        progress: &mut CpuSceneWorldProgress,
    ) -> Result<(), ModelError> {
        if progress.domain != self.lookup_reference(FrameKey(0)) {
            return Err(ModelError::ForeignArena);
        }
        let si = self.scene_index(progress.scene)?;
        self.check_owner_available(progress.scene.into())?;
        if let Some(staging) = progress.staging_model {
            self.release(staging.root())?;
            progress.staging_model = None;
        }
        if self.scenes[si].frames.is_empty()
            && self
                .links(self.scenes[si].sectors[0].frame())?
                .child
                .is_none()
        {
            self.scenes[si].cpu_world_intake_active = false;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_world_intake_retains_real_staging_owner_until_explicit_terminal_disposal() {
        let directory = std::env::temp_dir().join(format!(
            "lhr-world-failure-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let installation = lhr_assets::Installation::open(&directory).unwrap();
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let mut frame = super::super::tests::frame(5, None, [0.; 3]);
        frame.sector = Some(lhr_assets::SectorRecord {
            flags: 0x801,
            group: 0,
            vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            faces: vec![[0, 1, 2]],
            bounds: [[0., 0., 0.], [1., 1., 0.]],
            portals: Vec::new(),
        });
        let source = Arc::new(lhr_assets::Model {
            materials: Vec::new(),
            frames: vec![frame],
        });
        arena.sector_storage_mut().fail_on_allocation(Some(1));
        let mut progress = CpuSceneWorldProgress::empty(&arena, scene);
        let error = arena
            .load_scene_world_parsed_cpu(scene, &installation, source, &mut progress)
            .unwrap_err();
        assert_eq!(error, ModelError::NativeModelOpenResult(0x80000002));
        assert_eq!(progress.phase(), CpuSceneWorldPhase::Geometry);
        let staging = progress.staging_model().unwrap();
        assert_eq!(arena.model(staging).unwrap().frames.len(), 1);
        assert_eq!(progress.frames().len(), 1);
        assert_eq!(progress.transferred_frames(), 0);
        assert!(arena.scene(scene).unwrap().frames().is_empty());
        arena
            .discard_scene_world_staging_cpu(&mut progress)
            .unwrap();
        assert_eq!(progress.staging_model(), None);
        assert_eq!(arena.sector_storage().live_allocations(), 0);
        assert_eq!(arena.live_portal_allocations(), 0);
        assert_eq!(arena.scene_loaded_sector_count(scene).unwrap(), 0);
        assert_eq!(arena.scene_portal_count(scene).unwrap(), 0);
        let si = arena.scene_index(scene).unwrap();
        assert!(!arena.scenes[si].cpu_world_intake_active);
        for key in arena.scene(scene).unwrap().sectors {
            arena.release(key.frame()).unwrap();
        }
        arena.destroy_empty_scene_base(scene.root()).unwrap();
        assert_eq!(arena.live_frame_count(), 0);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
