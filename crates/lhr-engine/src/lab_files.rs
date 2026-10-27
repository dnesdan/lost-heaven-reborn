//! Explicit, bounded Lab preset I/O, separate from original game saves.
use crate::{lab_preset, lab_state::Settings};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};

const MAX_BYTES: u64 = 64 * 1024;

fn validate_path(path: &Path) -> Result<(), String> {
    if path
        .extension()
        .is_none_or(|extension| extension != "lhr-lab")
    {
        return Err("Use a .lhr-lab filename for Lab presets".into());
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() || metadata.is_symlink() => {
            Err("Lab preset must be a regular file".into())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Cannot inspect Lab preset: {error}")),
    }
}

pub fn load(path: &Path) -> Result<Settings, String> {
    validate_path(path)?;
    let file = fs::File::open(path).map_err(|error| format!("Cannot open Lab preset: {error}"))?;
    let mut text = String::new();
    file.take(MAX_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| format!("Cannot read Lab preset: {error}"))?;
    if text.len() as u64 > MAX_BYTES {
        return Err("Lab preset exceeds 64 KiB".into());
    }
    lab_preset::decode(&text)
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    validate_path(path)?;
    let text = lab_preset::encode(settings)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let temporary = parent.join(format!(".lhr-preset-{}-{suffix}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("Cannot prepare Lab preset: {error}"))?;
    let result = (|| {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(format!("Cannot save Lab preset: {error}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_io_refuses_unrelated_files_and_bounds_input() {
        let dir = std::env::temp_dir().join(format!(
            "lhr-preset-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let unrelated = dir.join("source.rs");
        fs::write(&unrelated, "unchanged").unwrap();
        assert!(save(&unrelated, &Settings::default()).is_err());
        assert_eq!(fs::read_to_string(&unrelated).unwrap(), "unchanged");
        let path = dir.join("test.lhr-lab");
        save(&path, &Settings::default()).unwrap();
        assert_eq!(load(&path).unwrap(), Settings::default());
        let changed = Settings {
            fov_degrees: 65.,
            ..Settings::default()
        };
        save(&path, &changed).unwrap();
        assert_eq!(load(&path).unwrap(), changed);
        fs::write(&path, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
        assert!(load(&path).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
