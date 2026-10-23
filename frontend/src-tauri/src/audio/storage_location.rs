//! Validate a calls directory and commit preferences before changing live state.
use anyhow::{Context, Result};
use std::{io::Write, path::Path};
pub fn validate_directory(path: &Path) -> Result<()> {
    anyhow::ensure!(path.is_absolute(), "Choose an absolute folder path");
    std::fs::create_dir_all(path).context("Cannot create the calls folder")?;
    anyhow::ensure!(path.is_dir(), "The calls location must be a folder");
    let probe = path.join(format!(".meetily-write-test-{}", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)?;
        file.write_all(b"Meetily storage check")?;
        file.sync_all()?;
        drop(file);
        std::fs::remove_file(&probe)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&probe);
    }
    result
        .context("The calls folder is not writable; choose another folder or check its permissions")
}
pub fn persist_settings(
    path: &Path,
    value: &serde_json::Value,
    commit_live: impl FnOnce(),
) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let parent = path
        .parent()
        .context("Preferences file has no parent folder")?;
    std::fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let result = (|| -> Result<()> {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
        .context("Could not save the calls folder preference; the previous setting is unchanged")?;
    commit_live();
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_absolute_writable_folder_and_removes_probe() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("calls");
        validate_directory(&folder).unwrap();
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        assert!(validate_directory(Path::new("relative/calls")).is_err());
        let file = root.path().join("not-a-directory");
        std::fs::write(&file, b"file").unwrap();
        assert!(validate_directory(&file).is_err());
    }
    #[test]
    fn failed_persistence_keeps_disk_and_live_preferences_then_retries() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("recording_preferences.json");
        let old = serde_json::json!({"preferences":{"save_folder":"/old"}});
        let new = serde_json::json!({"preferences":{"save_folder":"/new"}});
        let live = std::cell::RefCell::new(old.clone());
        persist_settings(&path, &old, || {}).unwrap();
        let obstruction = path.with_extension("json.tmp");
        std::fs::create_dir(&obstruction).unwrap();
        assert!(persist_settings(&path, &new, || {
            *live.borrow_mut() = new.clone();
        })
        .is_err());
        assert_eq!(*live.borrow(), old);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap(),
            old
        );
        std::fs::remove_dir(obstruction).unwrap();
        persist_settings(&path, &new, || {
            *live.borrow_mut() = new.clone();
        })
        .unwrap();
        assert_eq!(*live.borrow(), new);
    }
}
