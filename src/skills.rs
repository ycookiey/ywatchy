use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

use tracing::{error, info, warn};

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub skill_dir: PathBuf,
    pub symlink_path: PathBuf,
}

pub fn discover_skills(
    scan_dirs: &[PathBuf],
    patterns: &[String],
    target_dir: &Path,
) -> Vec<SkillInfo> {
    let mut all_skills = Vec::new();
    let mut seen_names = HashSet::new();

    for scan_dir in scan_dirs {
        if !scan_dir.is_dir() {
            warn!(
                scan_dir = %scan_dir.display(),
                "skill scan directory does not exist; skipping"
            );
            continue;
        }

        let entries = match fs::read_dir(scan_dir) {
            Ok(entries) => entries,
            Err(err) => {
                warn!(
                    scan_dir = %scan_dir.display(),
                    error = %err,
                    "failed to read skill scan directory; skipping"
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

            let project_dir = entry.path();
            for (name, skill_dir) in find_skills_in_project(&project_dir, patterns) {
                let normalized = name.to_ascii_lowercase();
                if !seen_names.insert(normalized) {
                    warn!(
                        skill = %name,
                        "duplicate skill name discovered; keeping first match"
                    );
                    continue;
                }

                all_skills.push(SkillInfo {
                    name: name.clone(),
                    skill_dir,
                    symlink_path: target_dir.join(name),
                });
            }
        }
    }

    all_skills.sort_by(|left, right| left.name.cmp(&right.name));
    all_skills
}

pub fn find_skills_in_project(project_dir: &Path, patterns: &[String]) -> Vec<(String, PathBuf)> {
    let mut discovered: HashMap<String, PathBuf> = HashMap::new();

    for pattern in patterns {
        let Some(spec) = PatternSpec::parse(pattern) else {
            warn!(pattern = %pattern, "unsupported skill pattern; skipping");
            continue;
        };

        let base_dir = project_dir.join(spec.prefix_path());
        if !base_dir.is_dir() {
            continue;
        }

        let Ok(entries) = fs::read_dir(&base_dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }

            let skill_name = entry.file_name().to_string_lossy().to_string();
            let skill_dir = entry.path();
            let candidate = spec.skill_file_path(&skill_dir);
            if candidate.is_file() {
                discovered.entry(skill_name).or_insert(skill_dir);
            }
        }
    }

    let mut skills = discovered.into_iter().collect::<Vec<_>>();
    skills.sort_by(|left, right| left.0.cmp(&right.0));
    skills
}

pub fn ensure_symlink(skill: &SkillInfo) -> io::Result<()> {
    if !skill.skill_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("skill target does not exist: {}", skill.skill_dir.display()),
        ));
    }

    if let Some(parent) = skill.symlink_path.parent() {
        fs::create_dir_all(parent)?;
    }

    if skill.symlink_path.exists() {
        if same_destination(&skill.symlink_path, &skill.skill_dir) {
            return Ok(());
        }

        if is_symlink_like(&skill.symlink_path)? {
            remove_link_path(&skill.symlink_path)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "path exists and is not a symlink/junction: {}",
                    skill.symlink_path.display()
                ),
            ));
        }
    }

    create_dir_symlink(&skill.skill_dir, &skill.symlink_path)?;
    info!(
        skill = %skill.name,
        target = %skill.skill_dir.display(),
        link = %skill.symlink_path.display(),
        "created skill symlink"
    );
    Ok(())
}

pub fn remove_symlink(skill: &SkillInfo) -> io::Result<()> {
    if !skill.symlink_path.exists() {
        return Ok(());
    }

    if !is_symlink_like(&skill.symlink_path)? {
        warn!(
            path = %skill.symlink_path.display(),
            "skip removing skill path because it is a normal directory/file"
        );
        return Ok(());
    }

    remove_link_path(&skill.symlink_path)?;
    info!(
        skill = %skill.name,
        link = %skill.symlink_path.display(),
        "removed skill symlink"
    );
    Ok(())
}

pub fn initial_skill_sync(skills: &[SkillInfo]) {
    for skill in skills {
        if let Err(err) = ensure_symlink(skill) {
            error!(
                skill = %skill.name,
                error = %err,
                "failed to ensure skill symlink"
            );
        }
    }
}

pub fn cleanup_stale_symlinks(target_dir: &Path, active_skills: &[SkillInfo], scan_dirs: &[PathBuf]) {
    if !target_dir.is_dir() {
        return;
    }

    let active: HashSet<String> = active_skills
        .iter()
        .map(|skill| normalize_for_compare(&skill.symlink_path))
        .collect();

    let scan_roots = scan_dirs
        .iter()
        .map(|path| canonical_or_original(path))
        .collect::<Vec<_>>();

    let entries = match fs::read_dir(target_dir) {
        Ok(entries) => entries,
        Err(err) => {
            warn!(
                path = %target_dir.display(),
                error = %err,
                "failed to read skill target directory for cleanup"
            );
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let normalized = normalize_for_compare(&path);
        if active.contains(&normalized) {
            continue;
        }

        let Ok(is_link) = is_symlink_like(&path) else {
            continue;
        };
        if !is_link {
            continue;
        }

        let target = fs::canonicalize(&path).ok();
        let points_inside_scan_dirs = target
            .as_ref()
            .map(|resolved| scan_roots.iter().any(|scan_root| is_within(resolved, scan_root)))
            .unwrap_or(false);

        if points_inside_scan_dirs {
            if let Err(err) = remove_link_path(&path) {
                error!(
                    path = %path.display(),
                    error = %err,
                    "failed to remove stale skill symlink"
                );
            } else {
                info!(path = %path.display(), "removed stale skill symlink");
            }
        }
    }
}

pub fn resolve_skill_event(
    path: &Path,
    scan_dirs: &[PathBuf],
    patterns: &[String],
    target_dir: &Path,
) -> Option<SkillInfo> {
    if !file_name_eq(path, "SKILL.md") {
        return None;
    }

    let specs = patterns
        .iter()
        .filter_map(|pattern| PatternSpec::parse(pattern))
        .collect::<Vec<_>>();

    for scan_dir in scan_dirs {
        let Ok(relative_to_scan) = path.strip_prefix(scan_dir) else {
            continue;
        };

        let rel_scan_segments = to_segments(relative_to_scan);
        if rel_scan_segments.len() < 2 {
            continue;
        }

        let project_name = &rel_scan_segments[0];
        let project_dir = scan_dir.join(project_name);
        let rel_to_project = &rel_scan_segments[1..];

        for spec in &specs {
            if let Some(skill_name) = spec.match_skill_name(rel_to_project) {
                let skill_dir = project_dir.join(spec.prefix_path()).join(&skill_name);
                return Some(SkillInfo {
                    name: skill_name.clone(),
                    skill_dir,
                    symlink_path: target_dir.join(skill_name),
                });
            }
        }
    }

    None
}

#[cfg(windows)]
fn create_dir_symlink(target: &Path, link: &Path) -> io::Result<()> {
    use std::os::windows::fs::symlink_dir;

    match symlink_dir(target, link) {
        Ok(()) => Ok(()),
        Err(symlink_err) => {
            let link_str = strip_extended_prefix(&link.to_string_lossy().replace('/', "\\"));
            let target_str = strip_extended_prefix(&target.to_string_lossy().replace('/', "\\"));
            tracing::debug!(link = %link_str, target = %target_str, "attempting junction fallback");
            let status = Command::new("cmd.exe")
                .args(["/C", "mklink", "/J", &link_str, &target_str])
                .status();

            match status {
                Ok(status) if status.success() => Ok(()),
                Ok(status) => Err(io::Error::other(
                    format!(
                        "failed to create symlink ({}) and junction fallback exited with status {}",
                        symlink_err, status
                    ),
                )),
                Err(cmd_err) => Err(io::Error::other(
                    format!(
                        "failed to create symlink ({}) and junction fallback failed: {}",
                        symlink_err, cmd_err
                    ),
                )),
            }
        }
    }
}

#[cfg(not(windows))]
fn create_dir_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

fn remove_link_path(path: &Path) -> io::Result<()> {
    if fs::remove_file(path).is_ok() {
        return Ok(());
    }
    fs::remove_dir(path)
}

fn same_destination(link: &Path, target: &Path) -> bool {
    let link_canonical = fs::canonicalize(link).ok();
    let target_canonical = fs::canonicalize(target).ok();
    match (link_canonical, target_canonical) {
        (Some(link_path), Some(target_path)) => normalize_for_compare(&link_path) == normalize_for_compare(&target_path),
        _ => false,
    }
}

fn is_symlink_like(path: &Path) -> io::Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        Ok(metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
    }

    #[cfg(not(windows))]
    {
        Ok(false)
    }
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn normalize_for_compare(path: &Path) -> String {
    canonical_or_original(path)
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn is_within(path: &Path, root: &Path) -> bool {
    let path_normalized = normalize_for_compare(path);
    let root_normalized = normalize_for_compare(root);
    path_normalized == root_normalized || path_normalized.starts_with(&(root_normalized + "/"))
}

fn file_name_eq(path: &Path, expected: &str) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn to_segments(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect()
}

#[derive(Debug, Clone)]
struct PatternSpec {
    segments: Vec<String>,
    wildcard_index: usize,
}

impl PatternSpec {
    fn parse(pattern: &str) -> Option<Self> {
        let normalized = pattern.replace('\\', "/");
        let segments = normalized
            .split('/')
            .filter(|segment| !segment.trim().is_empty())
            .map(|segment| segment.to_string())
            .collect::<Vec<_>>();
        if segments.is_empty() {
            return None;
        }

        let wildcard_positions = segments
            .iter()
            .enumerate()
            .filter_map(|(index, segment)| if segment == "*" { Some(index) } else { None })
            .collect::<Vec<_>>();
        if wildcard_positions.len() != 1 {
            return None;
        }

        Some(Self {
            segments,
            wildcard_index: wildcard_positions[0],
        })
    }

    fn prefix_path(&self) -> PathBuf {
        let mut path = PathBuf::new();
        for segment in &self.segments[..self.wildcard_index] {
            path.push(segment);
        }
        path
    }

    fn skill_file_path(&self, skill_dir: &Path) -> PathBuf {
        let mut path = skill_dir.to_path_buf();
        for segment in &self.segments[self.wildcard_index + 1..] {
            path.push(segment);
        }
        path
    }

    fn match_skill_name(&self, rel_to_project: &[String]) -> Option<String> {
        if rel_to_project.len() != self.segments.len() {
            return None;
        }

        let mut skill_name: Option<String> = None;
        for (pattern_segment, actual_segment) in self.segments.iter().zip(rel_to_project.iter()) {
            if pattern_segment == "*" {
                skill_name = Some(actual_segment.clone());
                continue;
            }

            if !pattern_segment.eq_ignore_ascii_case(actual_segment) {
                return None;
            }
        }

        skill_name
    }
}

fn strip_extended_prefix(s: &str) -> String {
    s.strip_prefix(r"\\?\").unwrap_or(s).to_string()
}
