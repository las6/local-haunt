//! User preferences live outside the project and survive app upgrades.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub project_roots: Vec<PathBuf>,
    pub zoom: i8,
    pub auto_refresh: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            project_roots: vec![],
            zoom: 0,
            auto_refresh: true,
        }
    }
}
impl Settings {
    pub fn normalize(&mut self) -> Result<(), String> {
        if self.project_roots.iter().any(|root| !root.is_absolute()) {
            return Err("Project folders must use absolute paths".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        self.project_roots.retain(|root| seen.insert(root.clone()));
        self.zoom = self.zoom.clamp(-2, 6);
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(format!("Could not read settings: {error}")),
        };
        let mut settings: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Could not parse settings: {error}"))?;
        settings.normalize()?;
        Ok(settings)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut normalized = self.clone();
        normalized.normalize()?;
        let parent = path.parent().ok_or("Settings path has no parent")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temporary = parent.join(format!(".settings-{}.tmp", std::process::id()));
        let data = serde_json::to_vec_pretty(&normalized).map_err(|e| e.to_string())?;
        std::fs::write(&temporary, data).map_err(|e| format!("Could not write settings: {e}"))?;
        std::fs::rename(&temporary, path).map_err(|e| format!("Could not save settings: {e}"))
    }
}

#[cfg(not(test))]
pub fn path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home).join("Library/Application Support/Local Haunt/settings.json")
    })
}

#[cfg(test)]
mod tests {
    use super::Settings;
    #[test]
    fn preferences_survive_save_and_load_and_invalid_json_is_preserved() {
        let directory =
            std::env::temp_dir().join(format!("local-haunt-settings-test-{}", std::process::id()));
        let path = directory.join("settings.json");
        let mut settings = Settings {
            project_roots: vec!["/projects/client".into(), "/projects/client".into()],
            zoom: 3,
            auto_refresh: false,
        };
        settings.normalize().unwrap();
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), settings);
        std::fs::write(&path, b"invalid json").unwrap();
        assert!(Settings::load(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"invalid json");
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn missing_settings_and_validation() {
        let path =
            std::env::temp_dir().join(format!("local-haunt-missing-{}.json", std::process::id()));
        assert_eq!(Settings::load(&path).unwrap(), Settings::default());
        let mut settings = Settings {
            project_roots: vec!["relative".into()],
            zoom: 127,
            auto_refresh: true,
        };
        assert!(settings.normalize().is_err());
        settings.project_roots.clear();
        settings.normalize().unwrap();
        assert_eq!(settings.zoom, 6);
    }
}
