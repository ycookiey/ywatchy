use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};

use notify::{RecursiveMode, Watcher};
use notify_debouncer_mini::{new_debouncer, DebouncedEvent};
use tracing::{debug, error, info, warn};

use crate::{
    config::Config,
    skills::{
        cleanup_stale_symlinks, discover_skills, ensure_symlink, initial_skill_sync, remove_symlink,
        resolve_skill_event, SkillInfo,
    },
    sync::{
        discover_projects, find_claude_md, initial_sync, resolve_event, sync_source_to_store,
        sync_store_to_source, ProjectClaudeMd, RecentWrites, SyncDirection,
    },
};

pub struct WatcherState {
    pub projects: Vec<ProjectClaudeMd>,
    pub skills: Vec<SkillInfo>,
    pub recent_writes: RecentWrites,
    pub config: Config,
    pub ywatchy_root: PathBuf,
}

pub fn run(
    config: Config,
    ywatchy_root: PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let store_dir = config.resolve_store_dir(&ywatchy_root);
    let sync_scan_dirs = config.resolve_sync_scan_dirs(&ywatchy_root);
    let skills_scan_dirs = config.resolve_skills_scan_dirs(&ywatchy_root);
    let skills_target_dir = config.resolve_skills_target();

    fs::create_dir_all(&store_dir)?;
    fs::create_dir_all(&skills_target_dir)?;

    let projects = discover_projects(&sync_scan_dirs, &store_dir, &config.sync.exclude_projects);
    let skills = discover_skills(
        &skills_scan_dirs,
        &config.skills.skill_patterns,
        &skills_target_dir,
    );

    let mut state = WatcherState {
        projects,
        skills,
        recent_writes: RecentWrites::new(Duration::from_millis(config.cooldown_ms())),
        config,
        ywatchy_root,
    };

    initial_sync(&state.projects, &mut state.recent_writes);
    let mut skill_items = initial_skill_sync(&state.skills);
    let stale_items = cleanup_stale_symlinks(&skills_target_dir, &state.skills, &skills_scan_dirs);
    skill_items.extend(stale_items);
    crate::print::print_section("skills", skill_items.len(), &skill_items);

    let (tx, rx) = mpsc::channel();
    let debounce = Duration::from_millis(state.config.watcher.debounce_ms);
    let mut debouncer = new_debouncer(debounce, tx)?;

    {
        let watcher = debouncer.watcher();
        add_initial_watches(
            watcher,
            &sync_scan_dirs,
            &skills_scan_dirs,
            &store_dir,
            &state.projects,
        );
    }

    info!(
        project_count = state.projects.len(),
        skill_count = state.skills.len(),
        store_dir = %store_dir.display(),
        skills_target = %skills_target_dir.display(),
        root = %state.ywatchy_root.display(),
        "watcher started"
    );
    crate::print::print_watching();

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                let watcher = debouncer.watcher();
                handle_debounced_events(
                    events,
                    &mut state,
                    watcher,
                    &sync_scan_dirs,
                    &skills_scan_dirs,
                    &store_dir,
                    &skills_target_dir,
                );
            }
            Ok(Err(err)) => warn!(error = %err, "watch error"),
            Err(_) => {
                info!("watch channel disconnected, exiting watcher loop");
                break;
            }
        }
    }

    Ok(())
}

fn add_initial_watches<W: Watcher + ?Sized>(
    watcher: &mut W,
    sync_scan_dirs: &[PathBuf],
    skills_scan_dirs: &[PathBuf],
    store_dir: &Path,
    projects: &[ProjectClaudeMd],
) {
    for scan_dir in sync_scan_dirs {
        watch_path_if_exists(watcher, scan_dir, RecursiveMode::NonRecursive);
    }

    for project in projects {
        if let Some(parent) = project.source_path.parent() {
            watch_path_if_exists(watcher, parent, RecursiveMode::NonRecursive);
        }
    }

    watch_path_if_exists(watcher, store_dir, RecursiveMode::Recursive);
    watch_existing_skill_roots(watcher, skills_scan_dirs);
}

fn watch_existing_skill_roots<W: Watcher + ?Sized>(watcher: &mut W, skills_scan_dirs: &[PathBuf]) {
    for scan_dir in skills_scan_dirs {
        let Ok(entries) = fs::read_dir(scan_dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }

            watch_project_skill_dirs(watcher, &entry.path());
        }
    }
}

fn watch_project_skill_dirs<W: Watcher + ?Sized>(watcher: &mut W, project_dir: &Path) {
    let root_skills = project_dir.join("skills");
    watch_path_if_exists(watcher, &root_skills, RecursiveMode::Recursive);

    let nested_skills = project_dir.join(".claude").join("skills");
    watch_path_if_exists(watcher, &nested_skills, RecursiveMode::Recursive);
}

fn watch_path_if_exists<W: Watcher + ?Sized>(watcher: &mut W, path: &Path, mode: RecursiveMode) {
    if !path.exists() {
        debug!(path = %path.display(), "watch path does not exist; skipping");
        return;
    }

    if let Err(err) = watcher.watch(path, mode) {
        debug!(
            path = %path.display(),
            error = %err,
            "failed to register watch (may already be watched)"
        );
    }
}

fn handle_debounced_events<W: Watcher + ?Sized>(
    events: Vec<DebouncedEvent>,
    state: &mut WatcherState,
    watcher: &mut W,
    sync_scan_dirs: &[PathBuf],
    skills_scan_dirs: &[PathBuf],
    store_dir: &Path,
    skills_target_dir: &Path,
) {
    for event in events {
        let path = event.path;

        if file_name_eq(&path, "CLAUDE.md") {
            if state.recent_writes.should_ignore(&path) {
                debug!(path = %path.display(), "ignored self-triggered CLAUDE.md event");
                continue;
            }

            if let Some((project, direction)) = resolve_event(&path, &state.projects) {
                let project = project.clone();
                let dir_label = match direction {
                    SyncDirection::SourceToStore => "source -> store",
                    SyncDirection::StoreToSource => "store -> source",
                };
                let result = match direction {
                    SyncDirection::SourceToStore => {
                        sync_source_to_store(&project, &mut state.recent_writes)
                    }
                    SyncDirection::StoreToSource => {
                        sync_store_to_source(&project, &mut state.recent_writes)
                    }
                };
                match result {
                    Ok(()) => crate::print::print_event("sync", &project.project_name, dir_label),
                    Err(err) => {
                        error!(
                            path = %path.display(),
                            project = %project.project_name,
                            error = %err,
                            "failed to sync CLAUDE.md after event"
                        );
                        crate::print::print_event_error("sync", &project.project_name, &err.to_string());
                    }
                }
            }
            continue;
        }

        if file_name_eq(&path, "SKILL.md") {
            if let Some(skill) = resolve_skill_event(
                &path,
                skills_scan_dirs,
                &state.config.skills.skill_patterns,
                skills_target_dir,
            ) {
                if path.exists() {
                    match ensure_symlink(&skill) {
                        Ok(true) => crate::print::print_event("skill", &skill.name, "linked"),
                        Ok(false) => {}
                        Err(err) => {
                            error!(
                                path = %path.display(),
                                skill = %skill.name,
                                error = %err,
                                "failed to ensure skill symlink after SKILL.md event"
                            );
                            crate::print::print_event_error("skill", &skill.name, &err.to_string());
                        }
                    }
                } else {
                    match remove_symlink(&skill) {
                        Ok(()) => crate::print::print_event("skill", &skill.name, "removed"),
                        Err(err) => {
                            error!(
                                path = %path.display(),
                                skill = %skill.name,
                                error = %err,
                                "failed to remove skill symlink after SKILL.md deletion"
                            );
                            crate::print::print_event_error("skill", &skill.name, &err.to_string());
                        }
                    }
                }

                refresh_skills_snapshot(state, skills_scan_dirs, skills_target_dir);
            }
            continue;
        }

        maybe_add_new_project(
            &path,
            state,
            watcher,
            sync_scan_dirs,
            skills_scan_dirs,
            store_dir,
            skills_target_dir,
        );
    }
}

fn maybe_add_new_project<W: Watcher + ?Sized>(
    path: &Path,
    state: &mut WatcherState,
    watcher: &mut W,
    sync_scan_dirs: &[PathBuf],
    skills_scan_dirs: &[PathBuf],
    store_dir: &Path,
    skills_target_dir: &Path,
) {
    if !path.is_dir() {
        return;
    }

    let Some(parent) = path.parent() else {
        return;
    };
    if !sync_scan_dirs.iter().any(|scan_dir| path_eq(parent, scan_dir)) {
        return;
    }

    let Some(project_name_os) = path.file_name() else {
        return;
    };
    let project_name = project_name_os.to_string_lossy().to_string();
    if state
        .config
        .sync
        .exclude_projects
        .iter()
        .any(|excluded| excluded.eq_ignore_ascii_case(&project_name))
    {
        return;
    }

    if state
        .projects
        .iter()
        .any(|project| path_eq(&project.project_dir, path))
    {
        return;
    }

    let Some(source_path) = find_claude_md(path) else {
        return;
    };

    let project = ProjectClaudeMd {
        project_name: project_name.clone(),
        project_dir: path.to_path_buf(),
        source_path: source_path.clone(),
        store_path: store_dir.join(&project_name).join("CLAUDE.md"),
    };

    state.projects.push(project.clone());
    state
        .projects
        .sort_by(|left, right| left.project_name.cmp(&right.project_name));

    if let Some(parent) = source_path.parent() {
        watch_path_if_exists(watcher, parent, RecursiveMode::NonRecursive);
    }
    watch_project_skill_dirs(watcher, path);

    match sync_source_to_store(&project, &mut state.recent_writes) {
        Ok(()) => {
            info!(
                project = %project.project_name,
                source = %project.source_path.display(),
                store = %project.store_path.display(),
                "added new project to watch list"
            );
            crate::print::print_event("watch", &project.project_name, "discovered");
        }
        Err(err) => {
            error!(
                project = %project.project_name,
                error = %err,
                "failed to sync newly discovered project"
            );
            crate::print::print_event_error("watch", &project.project_name, &err.to_string());
        }
    }

    refresh_skills_snapshot(state, skills_scan_dirs, skills_target_dir);
}

fn refresh_skills_snapshot(state: &mut WatcherState, skills_scan_dirs: &[PathBuf], skills_target_dir: &Path) {
    state.skills = discover_skills(
        skills_scan_dirs,
        &state.config.skills.skill_patterns,
        skills_target_dir,
    );
    let _ = initial_skill_sync(&state.skills);
    let _ = cleanup_stale_symlinks(skills_target_dir, &state.skills, skills_scan_dirs);
}

fn file_name_eq(path: &Path, expected: &str) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

fn path_eq(left: &Path, right: &Path) -> bool {
    normalize_for_compare(left) == normalize_for_compare(right)
}

fn normalize_for_compare(path: &Path) -> String {
    canonical_or_original(path)
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn canonical_or_original(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
