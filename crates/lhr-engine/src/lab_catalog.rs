//! Bounded host discovery for Lab selection, using the actual asset namespace.
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LabCatalog {
    pub missions: Vec<String>,
    pub models: Vec<String>,
}

impl LabCatalog {
    pub fn discover(root: &Path) -> Result<Self, String> {
        let installation = lhr_assets::Installation::open(root).map_err(|e| e.to_string())?;
        let mission_paths = installation
            .asset_paths("missions")
            .map_err(|e| e.to_string())?;
        let model_paths = installation
            .asset_paths("models")
            .map_err(|e| e.to_string())?;
        Ok(Self::from_paths(mission_paths, model_paths))
    }

    fn from_paths(mission_paths: Vec<String>, model_paths: Vec<String>) -> Self {
        let mut missions: Vec<_> = mission_paths
            .into_iter()
            .filter_map(|path| {
                let path = path.to_lowercase();
                let name = path
                    .strip_prefix("missions/")?
                    .strip_suffix("/scene2.bin")?;
                (!name.is_empty() && !name.contains('/') && !name.starts_with('.'))
                    .then(|| name.to_owned())
            })
            .collect();
        let mut models: Vec<_> = model_paths
            .into_iter()
            .filter_map(|path| {
                let path = path.to_lowercase();
                (path.starts_with("models/") && path.ends_with(".4ds")).then_some(path)
            })
            .collect();
        missions.sort_unstable();
        missions.dedup();
        models.sort_unstable();
        models.dedup();
        Self { missions, models }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_selects_complete_mission_names_and_model_extensions() {
        let catalog = LabCatalog::from_paths(
            [
                "Missions/MISE01/scene2.bin",
                "missions/mise01/SCENE2.BIN",
                "missions/intro/scene.4ds",
                "missions/nested/deeper/scene2.bin",
                "missions/00menu/scene2.bin",
            ]
            .map(str::to_owned)
            .to_vec(),
            [
                "models/Car.4DS",
                "models/car.4ds",
                "models/nested/chair.4ds",
                "models/ignore.4ds.bak",
                "models/readme.txt",
            ]
            .map(str::to_owned)
            .to_vec(),
        );
        assert_eq!(catalog.missions, ["00menu", "mise01"]);
        assert_eq!(
            catalog.models,
            ["models/car.4ds", "models/nested/chair.4ds"]
        );
    }
}
