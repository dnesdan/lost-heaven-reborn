//! Real object owners used by the admitted CreateVisual/Duplicate composition.
use super::*;
use crate::i3d_object::ObjectVisualType;

impl LoadedVisualResources {
    pub(crate) fn create_empty_object_visual(
        &mut self,
        class: ObjectVisualType,
    ) -> Result<ObjectResourceKey> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        let object = match class {
            ObjectVisualType::Plain => self.objects.create_object_resources(None)?,
            ObjectVisualType::LitUnconstructed => self
                .objects
                .create_unconstructed_lit_object_resources(None)?,
            ObjectVisualType::Billboard => self.objects.create_billboard_resources(None)?,
            ObjectVisualType::Singlemesh | ObjectVisualType::Singlemorph => {
                return Err(LoadedVisualError::Backend(
                    "derived visual factory unavailable",
                ));
            }
        };
        self.sources
            .insert((object.arena(), object.slot()), Vec::new());
        Ok(object)
    }

    pub(crate) fn duplicate_object_visual(
        &mut self,
        destination: ObjectResourceKey,
        source: ObjectResourceKey,
    ) -> Result<()> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        if destination == source {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let class = self.objects.object(destination)?.visual_type();
        if self.objects.object(source)?.phase != crate::i3d_object::ObjectCleanupPhase::Alive
            || self.objects.object(destination)?.phase
                != crate::i3d_object::ObjectCleanupPhase::Alive
        {
            return Err(LoadedVisualError::Backend("incomplete visual cleanup"));
        }
        let mesh = self.objects.object(source)?.mesh;
        let materials = self.source_materials(source)?.to_vec();
        // The factory destination is pristine: native Destruct(0) has no lit
        // payload to release. Each destination owns its own empty LOD slots.
        let target = self.objects.object(destination)?;
        if target.mesh.is_some()
            || target.lods.iter().any(Option::is_some)
            || target.lit_object().is_some_and(|lit| {
                lit.levels
                    .iter()
                    .any(|level| level.constructed || level.lods.iter().any(Option::is_some))
            })
        {
            return Err(LoadedVisualError::Backend(
                "non-pristine visual duplicate destination",
            ));
        }
        if class == ObjectVisualType::LitUnconstructed {
            self.objects
                .copy_object_duplicate_state(destination, source)?;
        }
        match class {
            ObjectVisualType::Plain => {
                self.objects
                    .set_object_mesh(destination, mesh, false, &mut self.backend)?
            }
            ObjectVisualType::LitUnconstructed => {
                self.objects
                    .lit_set_object_mesh(destination, mesh, true, &mut self.backend)?
            }
            ObjectVisualType::Billboard => {
                self.objects
                    .set_object_mesh(destination, mesh, false, &mut self.backend)?
            }
            ObjectVisualType::Singlemesh | ObjectVisualType::Singlemorph => {
                return Err(LoadedVisualError::Backend(
                    "derived visual duplicate unavailable",
                ));
            }
        }
        if matches!(class, ObjectVisualType::Plain | ObjectVisualType::Billboard) {
            self.objects
                .copy_object_duplicate_state(destination, source)?;
        }
        self.sources
            .insert((destination.arena(), destination.slot()), materials);
        Ok(())
    }
}
