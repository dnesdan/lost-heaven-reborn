//! Material references owned by loaded lensflare glow slots. Source-index
//! identity is scoped to one immutable parsed model Arc, matching the image's
//! shared material table while keeping every glow's individual AddRef/Release.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct LensMaterialEntry {
    pub key: MaterialKey,
    pub glow_references: u32,
}

impl LoadedVisualResources {
    fn prepare_lensflare_material(
        &self,
        material: &lhr_assets::Material,
        installation: &lhr_assets::Installation,
    ) -> Result<PreparedMaterial> {
        if material.animation.is_some() {
            return Err(LoadedVisualError::Backend(
                "animated lensflare material ownership unavailable",
            ));
        }
        self.material_load_context.base_texture_flags(material)?;
        let decode = |name: &str| -> Result<lhr_assets::Texture> {
            let path = format!("maps/{name}");
            let bytes = installation
                .read(&path)
                .map_err(|e| LoadedVisualError::Asset(format!("{path}: {e}")))?;
            if name.to_ascii_lowercase().ends_with(".tga") {
                lhr_assets::decode_tga(&bytes)
            } else {
                lhr_assets::decode_bmp_original(&bytes)
            }
            .map_err(|e| LoadedVisualError::Asset(format!("{path}: {e}")))
        };
        let mut inspection_rgba8_color_key = false;
        let color = if material.texture.is_empty() {
            None
        } else if self.inspection_rgba8_color_key
            && self.material_load_context.base_texture_flags(material)? & 0xe == 8
            && material
                .alpha_texture
                .as_ref()
                .is_none_or(|name| name.is_empty())
            && !material.texture.to_ascii_lowercase().ends_with(".tga")
        {
            let path = format!("maps/{}", material.texture);
            let raw = installation
                .read(&path)
                .map_err(|e| LoadedVisualError::Asset(format!("{path}: {e}")))?;
            let (image, keyed) = car_inspection_texture::decode_rgba8_color_key(&raw)
                .map_err(|e| LoadedVisualError::Asset(format!("{path}: {e}")))?;
            inspection_rgba8_color_key = keyed;
            Some(image)
        } else {
            Some(decode(&material.texture)?)
        };
        let alpha = material
            .alpha_texture
            .as_deref()
            .filter(|name| !name.is_empty())
            .map(decode)
            .transpose()?;
        if color.is_none() && alpha.is_some() {
            return Err(LoadedVisualError::Backend("alpha without color"));
        }
        let alpha = match (&color, alpha) {
            (Some(c), Some(a)) => Some(
                crate::itexture::resize_loaded_texture(a, c.width, c.height)
                    .map_err(|_| LoadedVisualError::Backend("alpha image resize"))?,
            ),
            (_, alpha) => alpha,
        };
        let environment = material
            .environment_texture
            .as_deref()
            .filter(|name| !name.is_empty())
            .map(decode)
            .transpose()?;
        Ok(PreparedMaterial {
            source: material.clone(),
            color,
            alpha,
            environment,
            inspection_rgba8_color_key,
            animation: None,
        })
    }

    fn create_lensflare_material(&mut self, prepared: PreparedMaterial) -> Result<MaterialKey> {
        let key = match self.objects.create_material() {
            Ok(key) => key,
            Err(error) => {
                self.incomplete = true;
                return Err(error.into());
            }
        };
        let result = (|| {
            let source = &prepared.source;
            self.objects.install_loaded_material_scalars(
                key,
                source,
                self.material_load_context.loaded_material_flags(source),
            )?;
            if let Some(environment) = prepared.environment {
                let texture_key = format!(
                    "maps/{}|",
                    source.environment_texture.as_deref().unwrap_or("")
                );
                let flags = self.material_load_context.environment_texture_flags(source);
                let texture = self.backend.admit_texture_with_flags(
                    &texture_key,
                    environment,
                    None,
                    flags,
                    flags & 0x10e,
                )?;
                let adopted = self.objects.adopt_loaded_material_texture(
                    key,
                    Some(texture),
                    true,
                    &mut self.backend,
                );
                let released = self.backend.release_texture(texture);
                adopted?;
                released?;
            }
            if let Some(color) = prepared.color {
                let texture_key = format!(
                    "maps/{}|{}",
                    source.texture,
                    source.alpha_texture.as_deref().unwrap_or("")
                );
                let texture = self.backend.admit_texture_with_flags(
                    &texture_key,
                    color,
                    prepared.alpha,
                    self.material_load_context
                        .base_texture_request_flags(source),
                    self.material_load_context.base_texture_flags(source)?,
                )?;
                if prepared.inspection_rgba8_color_key {
                    self.backend
                        .textures
                        .get_mut(&texture.0)
                        .ok_or(LoadedVisualError::Backend(
                            "inspection color-key texture unavailable",
                        ))?
                        .inspection_rgba8_color_key = true;
                }
                let adopted = self.objects.adopt_loaded_material_texture(
                    key,
                    Some(texture),
                    false,
                    &mut self.backend,
                );
                let released = self.backend.release_texture(texture);
                adopted?;
                released?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = self.objects.release_material(key, &mut self.backend);
            self.incomplete = true;
            return Err(error);
        }
        Ok(key)
    }

    /// One glow slot gains one live material reference. Each source table entry
    /// is materialized once; repeated IDs and distinct lensflare frames retain
    /// the same MaterialKey and decoded texture owners.
    pub(crate) fn admit_lensflare_glow_material(
        &mut self,
        source: &Arc<lhr_assets::Model>,
        material_index: usize,
        installation: &lhr_assets::Installation,
    ) -> Result<MaterialKey> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        let material = source
            .materials
            .get(material_index)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let identity = (Arc::as_ptr(source) as usize, material_index);
        if let Some(entry) = self.lens_materials.get(&identity).copied() {
            let next = match entry.glow_references.checked_add(1) {
                Some(next) => next,
                None => {
                    self.incomplete = true;
                    return Err(LoadedVisualError::Backend("lensflare reference overflow"));
                }
            };
            if let Err(error) = self.objects.retain_material(entry.key) {
                self.incomplete = true;
                return Err(error.into());
            }
            self.lens_materials
                .get_mut(&identity)
                .unwrap()
                .glow_references = next;
            return Ok(entry.key);
        }
        let prepared = match self.prepare_lensflare_material(material, installation) {
            Ok(prepared) => prepared,
            Err(error) => {
                self.incomplete = true;
                return Err(error);
            }
        };
        let key = self.create_lensflare_material(prepared)?;
        self.lens_materials.insert(
            identity,
            LensMaterialEntry {
                key,
                glow_references: 1,
            },
        );
        Ok(key)
    }

    /// Native Duplicate preserves the same material object and adds one ref
    /// for each copied glow; it does not reload image bytes.
    pub(crate) fn duplicate_lensflare_glow_material(
        &mut self,
        source: &Arc<lhr_assets::Model>,
        material_index: usize,
        original: MaterialKey,
    ) -> Result<MaterialKey> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        let identity = (Arc::as_ptr(source) as usize, material_index);
        let entry = self
            .lens_materials
            .get(&identity)
            .copied()
            .ok_or(LoadedVisualError::InvalidGeometry);
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                self.incomplete = true;
                return Err(error);
            }
        };
        if entry.key != original {
            self.incomplete = true;
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let next = match entry.glow_references.checked_add(1) {
            Some(next) => next,
            None => {
                self.incomplete = true;
                return Err(LoadedVisualError::Backend("lensflare reference overflow"));
            }
        };
        if let Err(error) = self.objects.retain_material(entry.key) {
            self.incomplete = true;
            return Err(error.into());
        }
        self.lens_materials
            .get_mut(&identity)
            .unwrap()
            .glow_references = next;
        Ok(entry.key)
    }

    pub(crate) fn release_lensflare_glow_material(
        &mut self,
        source: &Arc<lhr_assets::Model>,
        material_index: usize,
        key: MaterialKey,
    ) -> Result<()> {
        let identity = (Arc::as_ptr(source) as usize, material_index);
        let entry = self
            .lens_materials
            .get(&identity)
            .copied()
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        if entry.key != key || entry.glow_references == 0 {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        self.objects.release_material(key, &mut self.backend)?;
        if entry.glow_references == 1 {
            self.lens_materials.remove(&identity);
        } else {
            self.lens_materials
                .get_mut(&identity)
                .unwrap()
                .glow_references -= 1;
        }
        Ok(())
    }

    pub(crate) fn lensflare_material_references(
        &self,
        source: &Arc<lhr_assets::Model>,
        material_index: usize,
    ) -> Option<(MaterialKey, u32)> {
        self.lens_materials
            .get(&(Arc::as_ptr(source) as usize, material_index))
            .map(|entry| (entry.key, entry.glow_references))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_glows_and_duplicate_retain_one_live_material_and_texture() {
        let directory = std::env::temp_dir().join(format!(
            "lhr-lens-material-{}-{}",
            std::process::id(),
            identity().unwrap()
        ));
        std::fs::create_dir(&directory).unwrap();
        std::fs::create_dir(directory.join("maps")).unwrap();
        let mut bmp = vec![0u8; 58];
        bmp[..2].copy_from_slice(b"BM");
        bmp[2..6].copy_from_slice(&58u32.to_le_bytes());
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&1u32.to_le_bytes());
        bmp[22..26].copy_from_slice(&1u32.to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        bmp[54..57].copy_from_slice(&[255, 240, 120]);
        std::fs::write(directory.join("maps/flare.bmp"), bmp).unwrap();
        let installation = lhr_assets::Installation::open(&directory).unwrap();
        let source = Arc::new(lhr_assets::Model {
            frames: vec![],
            materials: vec![lhr_assets::Material {
                flags: 0,
                ambient: [1.; 3],
                diffuse: [1.; 3],
                texture: "flare.bmp".into(),
                emission: [0.; 3],
                opacity: 1.,
                alpha_texture: None,
                reflection: None,
                environment_texture: None,
                animation: None,
            }],
        });
        let mut owner = LoadedVisualResources::new().unwrap();
        assert!(matches!(
            owner.admit_lensflare_glow_material(&source, 1, &installation),
            Err(LoadedVisualError::InvalidGeometry)
        ));
        assert_eq!(owner.backend().live_textures(), 0);
        let first = owner
            .admit_lensflare_glow_material(&source, 0, &installation)
            .unwrap();
        let second = owner
            .admit_lensflare_glow_material(&source, 0, &installation)
            .unwrap();
        let copied = owner
            .duplicate_lensflare_glow_material(&source, 0, first)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first, copied);
        assert_eq!(
            owner.lensflare_material_references(&source, 0),
            Some((first, 3))
        );
        assert_eq!(
            owner
                .objects
                .managed()
                .resources()
                .material(first)
                .unwrap()
                .references,
            3
        );
        assert_eq!(owner.backend().live_textures(), 1);
        owner
            .release_lensflare_glow_material(&source, 0, second)
            .unwrap();
        assert_eq!(
            owner.lensflare_material_references(&source, 0),
            Some((first, 2))
        );
        owner
            .release_lensflare_glow_material(&source, 0, copied)
            .unwrap();
        assert_eq!(owner.backend().live_textures(), 1);
        owner
            .release_lensflare_glow_material(&source, 0, first)
            .unwrap();
        assert_eq!(owner.lensflare_material_references(&source, 0), None);
        assert_eq!(owner.backend().live_textures(), 0);

        let mut absent = source.materials[0].clone();
        absent.texture = "absent.bmp".into();
        let interrupted = Arc::new(lhr_assets::Model {
            frames: vec![],
            materials: vec![source.materials[0].clone(), absent],
        });
        let retained = owner
            .admit_lensflare_glow_material(&interrupted, 0, &installation)
            .unwrap();
        assert!(matches!(
            owner.admit_lensflare_glow_material(&interrupted, 1, &installation),
            Err(LoadedVisualError::Asset(_))
        ));
        assert!(owner.has_incomplete_admission());
        assert_eq!(
            owner.admit_lensflare_glow_material(&interrupted, 0, &installation),
            Err(LoadedVisualError::IncompleteAdmission)
        );
        assert_eq!(
            owner.lensflare_material_references(&interrupted, 0),
            Some((retained, 1))
        );
        owner
            .release_lensflare_glow_material(&interrupted, 0, retained)
            .unwrap();
        assert_eq!(owner.backend().live_textures(), 0);
        drop(installation);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
