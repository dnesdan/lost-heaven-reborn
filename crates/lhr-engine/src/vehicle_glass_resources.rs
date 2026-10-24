//! CPU color/alpha resources used by the reached IGraph texture-cache call.
//! This admits its flags4/default Maps-directory domain and keeps material
//! ownership in the existing visual resource owner.
use super::*;

impl LoadedVisualResources {
    pub(crate) fn game_load_glass_texture(
        &mut self,
        installation: &lhr_assets::Installation,
        color_name: &str,
        alpha_name: &str,
        flags: u32,
    ) -> Result<TextureKey> {
        if flags != 4 || color_name.is_empty() || alpha_name.is_empty() {
            return Err(LoadedVisualError::Backend(
                "glass texture cache flags/name domain",
            ));
        }
        let decode = |name: &str| -> Result<lhr_assets::Texture> {
            let path = format!("maps/{name}");
            let bytes = installation
                .read_original_path(&path)
                .map_err(|error| LoadedVisualError::Asset(format!("{path}: {error}")))?;
            if name.to_ascii_lowercase().ends_with(".tga") {
                lhr_assets::decode_tga(&bytes)
            } else {
                lhr_assets::decode_bmp_original(&bytes)
            }
            .map_err(|error| LoadedVisualError::Asset(format!("{path}: {error}")))
        };
        let color = decode(color_name)?;
        let alpha = decode(alpha_name)?;
        self.backend.admit_texture_with_flags(
            &format!("maps/{color_name}|maps/{alpha_name}"),
            color,
            Some(alpha),
            flags,
            flags,
        )
    }
    pub(crate) fn game_set_material_texture(
        &mut self,
        material: MaterialKey,
        texture: Option<TextureKey>,
    ) -> Result<()> {
        self.objects
            .set_texture(material, texture, &mut self.backend)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bmp(value: u8) -> Vec<u8> {
        let mut bytes = vec![0u8; 58];
        bytes[..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&58u32.to_le_bytes());
        bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&1u32.to_le_bytes());
        bytes[22..26].copy_from_slice(&1u32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
        bytes[54..57].fill(value);
        bytes
    }
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn original_file_color_alpha_cache_and_material_keep_last_reference() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = Directory(std::env::temp_dir().join(format!(
            "lhr-glass-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::create_dir(&directory.0).unwrap();
        std::fs::create_dir(directory.0.join("maps")).unwrap();
        std::fs::write(directory.0.join("maps/glass.bmp"), bmp(21)).unwrap();
        std::fs::write(directory.0.join("maps/glass+.bmp"), bmp(171)).unwrap();
        let installation = lhr_assets::Installation::open(&directory.0).unwrap();
        let mut owner = LoadedVisualResources::new().unwrap();
        let first = owner
            .game_load_glass_texture(&installation, "glass.bmp", "glass+.bmp", 4)
            .unwrap();
        let shared = owner
            .game_load_glass_texture(&installation, "glass.bmp", "glass+.bmp", 4)
            .unwrap();
        assert_eq!(first, shared);
        assert_eq!(owner.backend.decoded_texture_bytes(), 8);
        let material = owner.objects.create_material().unwrap();
        owner
            .game_set_material_texture(material, Some(first))
            .unwrap();
        assert_ne!(
            owner
                .objects
                .managed()
                .resources()
                .material(material)
                .unwrap()
                .flags
                & 0x40000000,
            0
        );
        owner.release_cpu_texture(first).unwrap();
        owner.release_cpu_texture(shared).unwrap();
        assert_eq!(owner.backend.texture_references(first), Some(1));
        owner
            .objects
            .release_material(material, &mut owner.backend)
            .unwrap();
        assert_eq!(owner.backend.live_textures(), 0);
    }
}
