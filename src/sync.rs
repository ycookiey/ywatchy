use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

use tracing::{error, info, warn};

#[derive(Debug, Clone)]
pub struct ProjectClaudeMd {
    pub project_name: String,
    pub project_dir: PathBuf,
    pub source_path: PathBuf,
    pub store_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDirection {
    SourceToStore,
    StoreToSource,
}

#[derive(Debug)]
pub struct RecentWrites {
    entries: HashMap<PathBuf, Instant>,
    cooldown: Duration,
}

impl RecentWrites {
    pub fn new(cooldown: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            cooldown,
        }
    }

    pub fn mark(&mut self, path: &Path) {
        self.prune();
        self.entries.insert(normalize_path(path), Instant::now());
    }

    pub fn should_ignore(&mut self, path: &Path) -> bool {
        self.prune();
        let normalized = normalize_path(path);
        if let Some(written_at) = self.entries.get(&normalized) {
            return written_at.elapsed() <= self.cooldown;
        }
        false
    }

    fn prune(&mut self) {
        let cooldown = self.cooldown;
        self.entries.retain(|_, timestamp| timestamp.elapsed() <= cooldown);
    }
}

pub fn discover_projects(
    scan_dirs: &[PathBuf],
    store_dir: &Path,
    exclude: &[String],
) -> Vec<ProjectClaudeMd> {
    let excluded: HashSet<String> = exclude.iter().map(|name| name.to_ascii_lowercase()).collect();
    let mut seen_names = HashSet::new();
    let mut projects = Vec::new();

    for scan_dir in scan_dirs {
        if !scan_dir.is_dir() {
            warn!(
                scan_dir = %scan_dir.display(),
                "sync scan directory does not exist; skipping"
            );
            continue;
        }

        let entries = match fs::read_dir(scan_dir) {
            Ok(entries) => entries,
            Err(err) => {
                warn!(
                    scan_dir = %scan_dir.display(),
                    error = %err,
                    "failed to read sync scan directory; skipping"
                );
                continue;
            }
        };

        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }

            let project_name = entry.file_name().to_string_lossy().to_string();
            let normalized_name = project_name.to_ascii_lowercase();
            if excluded.contains(&normalized_name) {
                continue;
            }

            if !seen_names.insert(normalized_name) {
                warn!(
                    project = %project_name,
                    "duplicate project name discovered in scan directories; first match wins"
                );
                continue;
            }

            let project_dir = entry.path();
            let Some(source_path) = find_claude_md(&project_dir) else {
                continue;
            };

            let store_path = store_dir.join(&project_name).join("CLAUDE.md");
            projects.push(ProjectClaudeMd {
                project_name,
                project_dir,
                source_path,
                store_path,
            });
        }
    }

    projects.sort_by(|left, right| left.project_name.cmp(&right.project_name));
    projects
}

pub fn find_claude_md(project_dir: &Path) -> Option<PathBuf> {
    let root_candidate = project_dir.join("CLAUDE.md");
    if root_candidate.is_file() {
        return Some(root_candidate);
    }

    let nested_candidate = project_dir.join(".claude").join("CLAUDE.md");
    if nested_candidate.is_file() {
        return Some(nested_candidate);
    }

    None
}

pub fn sync_source_to_store(
    project: &ProjectClaudeMd,
    recent_writes: &mut RecentWrites,
) -> io::Result<()> {
    if !project.source_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("source file missing: {}", project.source_path.display()),
        ));
    }

    copy_with_tracking(&project.source_path, &project.store_path, recent_writes)?;
    info!(
        project = %project.project_name,
        source = %project.source_path.display(),
        dest = %project.store_path.display(),
        "synced CLAUDE.md source -> store"
    );
    Ok(())
}

pub fn sync_store_to_source(
    project: &ProjectClaudeMd,
    recent_writes: &mut RecentWrites,
) -> io::Result<()> {
    if !project.store_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("store file missing: {}", project.store_path.display()),
        ));
    }

    copy_with_tracking(&project.store_path, &project.source_path, recent_writes)?;
    info!(
        project = %project.project_name,
        source = %project.store_path.display(),
        dest = %project.source_path.display(),
        "synced CLAUDE.md store -> source"
    );
    Ok(())
}

pub fn initial_sync(projects: &[ProjectClaudeMd], recent_writes: &mut RecentWrites) {
    for project in projects {
        let source_exists = project.source_path.is_file();
        let store_exists = project.store_path.is_file();

        match (source_exists, store_exists) {
            (true, true) => {
                let source_modified = modified_or_epoch(&project.source_path);
                let store_modified = modified_or_epoch(&project.store_path);
                let result = if source_modified >= store_modified {
                    sync_source_to_store(project, recent_writes)
                } else {
                    sync_store_to_source(project, recent_writes)
                };
                if let Err(err) = result {
                    error!(
                        project = %project.project_name,
                        error = %err,
                        "initial sync failed for project"
                    );
                }
            }
            (true, false) => {
                if let Err(err) = sync_source_to_store(project, recent_writes) {
                    error!(
                        project = %project.project_name,
                        error = %err,
                        "initial sync failed for source-only project"
                    );
                }
            }
            (false, true) => {
                warn!(
                    project = %project.project_name,
                    store = %project.store_path.display(),
                    "store CLAUDE.md exists without source; leaving untouched"
                );
            }
            (false, false) => {}
        }
    }
}

pub fn resolve_event<'a>(
    path: &Path,
    projects: &'a [ProjectClaudeMd],
) -> Option<(&'a ProjectClaudeMd, SyncDirection)> {
    for project in projects {
        if path_eq(path, &project.source_path) {
            return Some((project, SyncDirection::SourceToStore));
        }
        if path_eq(path, &project.store_path) {
            return Some((project, SyncDirection::StoreToSource));
        }
    }
    None
}

fn copy_with_tracking(source: &Path, dest: &Path, recent_writes: &mut RecentWrites) -> io::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }

    recent_writes.mark(dest);
    fs::copy(source, dest).map(|_| ())
}

fn modified_or_epoch(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn normalize_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    std::env::current_dir()
        .map(|current| current.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn path_eq(left: &Path, right: &Path) -> bool {
    normalize_for_compare(left) == normalize_for_compare(right)
}

fn normalize_for_compare(path: &Path) -> String {
    normalize_path(path)
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}
