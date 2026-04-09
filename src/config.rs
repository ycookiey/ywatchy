use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub general: GeneralConfig,
    pub sync: SyncConfig,
    pub skills: SkillsConfig,
    pub watcher: WatcherConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub log_dir: String,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub scan_dirs: Vec<String>,
    pub claude_md_store_dir: String,
    #[serde(default)]
    pub exclude_projects: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    pub scan_dirs: Vec<String>,
    #[serde(default)]
    pub target_dir: String,
    pub skill_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherConfig {
    pub debounce_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                log_dir: String::from("logs"),
                log_level: String::from("info"),
            },
            sync: SyncConfig {
                scan_dirs: Vec::new(),
                claude_md_store_dir: String::from(""),
                exclude_projects: vec![String::from("ywatchy")],
            },
            skills: SkillsConfig {
                scan_dirs: Vec::new(),
                target_dir: String::from(""),
                skill_patterns: vec![
                    String::from("skills/*/SKILL.md"),
                    String::from(".claude/skills/*/SKILL.md"),
                ],
            },
            watcher: WatcherConfig {
                debounce_ms: 500,
            },
        }
    }
}

impl Config {
    pub fn write_default(path: &Path) -> io::Result<()> {
        let content = r#"[general]
# ログ出力先 (設定ファイルからの相対パス or 絶対パス)
log_dir = "logs"
# ログレベル: trace, debug, info, warn, error
log_level = "info"

[sync]
# CLAUDE.md を検索するプロジェクト親ディレクトリ
# 例: ["C:/Users/you/Projects", "D:/work"]
scan_dirs = []
# CLAUDE.md バックアップ先 (空なら設定ファイルと同階層の store/)
claude_md_store_dir = ""
# 同期対象から除外するプロジェクト名
exclude_projects = ["ywatchy"]

[skills]
# スキルを検索するプロジェクト親ディレクトリ (通常 sync.scan_dirs と同じ)
scan_dirs = []
# スキルのシンボリックリンク先 (空なら ~/.claude/skills/)
target_dir = ""
# スキル検出パターン (* がスキル名に展開される)
skill_patterns = [
    "skills/*/SKILL.md",
    ".claude/skills/*/SKILL.md",
]

[watcher]
# ファイル変更検出のデバウンス時間 (ms)
debounce_ms = 500
"#;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(())
    }

    /// scan_dirs が未設定かどうか
    pub fn has_scan_dirs(&self) -> bool {
        !self.sync.scan_dirs.is_empty() || !self.skills.scan_dirs.is_empty()
    }

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
