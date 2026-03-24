use serde::Deserialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub general: GeneralConfig,
    pub sync: SyncConfig,
    pub skills: SkillsConfig,
    pub watcher: WatcherConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GeneralConfig {
    pub log_dir: String,
    pub log_level: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SyncConfig {
    pub scan_dirs: Vec<String>,
    pub claude_md_store_dir: String,
    #[serde(default)]
    pub exclude_projects: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillsConfig {
    pub scan_dirs: Vec<String>,
    #[serde(default)]
    pub target_dir: String,
    pub skill_patterns: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WatcherConfig {
    pub debounce_ms: u64,
}

impl Config {
    pub fn load(config_path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(config_path).map_err(|err| {
            io::Error::new(
                err.kind(),
                format!("failed to read {}: {}", config_path.display(), err),
            )
        })?;

        toml::from_str(&content).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse {}: {}", config_path.display(), err),
            )
        })
    }

    pub fn resolve_store_dir(&self, root: &Path) -> PathBuf {
        resolve_path(root, &self.sync.claude_md_store_dir)
    }

    pub fn resolve_sync_scan_dirs(&self, root: &Path) -> Vec<PathBuf> {
        self.sync
            .scan_dirs
            .iter()
            .map(|path| resolve_path(root, path))
            .collect()
    }

    pub fn resolve_skills_scan_dirs(&self, root: &Path) -> Vec<PathBuf> {
        self.skills
            .scan_dirs
            .iter()
            .map(|path| resolve_path(root, path))
            .collect()
    }

    pub fn resolve_log_dir(&self, root: &Path) -> PathBuf {
        resolve_path(root, &self.general.log_dir)
    }

    pub fn resolve_skills_target(&self) -> PathBuf {
        let configured = self.skills.target_dir.trim();
        if configured.is_empty() {
            if let Some(home_dir) = dirs::home_dir() {
                return home_dir.join(".claude").join("skills");
            }
            return PathBuf::from(".").join(".claude").join("skills");
        }

        PathBuf::from(configured)
    }

    pub fn cooldown_ms(&self) -> u64 {
        self.watcher.debounce_ms.saturating_add(500)
    }
}

fn resolve_path(root: &Path, configured: &str) -> PathBuf {
    let path = PathBuf::from(configured);
    if path.is_absolute() {
        return path;
    }
    root.join(path)
}
