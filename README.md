# ywatchy

[日本語版 README](README.ja.md)

A Windows command-line tool that watches your project folders, links Claude Code skills, and syncs `CLAUDE.md` files.

- Links each skill folder (a folder with a `SKILL.md`) into `~/.claude/skills/`, so the skill is available in every project
- Copies each project's `CLAUDE.md` to a store folder and keeps the two copies in sync in both directions

## Install

With [Scoop](https://scoop.sh/):

```powershell
scoop bucket add yscoopy https://github.com/ycookiey/yscoopy
scoop install yscoopy/ywatchy
```

Or download `ywatchy.exe` from [Releases](https://github.com/ycookiey/ywatchy/releases).

## Usage

1. Run `ywatchy`. On the first run, it creates `%APPDATA%\ywatchy\ywatchy.toml` and exits.
2. In that file, set `scan_dirs` under both `[sync]` and `[skills]` to the folders that contain your projects, for example `scan_dirs = ["C:/path/to/projects"]`.
3. Run `ywatchy` again. It syncs once, prints the result, and keeps watching until you press Ctrl+C. It does not start automatically at sign-in.

Each folder directly under a scan dir is a project.

- Skills: `<project>/skills/<name>/SKILL.md` and `<project>/.claude/skills/<name>/SKILL.md` are linked as `~/.claude/skills/<name>`. ywatchy creates a directory symlink, or a junction when Windows does not allow the symlink. Deleting the `SKILL.md` removes the link. On start, links in `~/.claude/skills` that point into a scan dir and are not the link of any skill found are removed.
- CLAUDE.md: `<project>/CLAUDE.md`, or `<project>/.claude/CLAUDE.md` when the first is missing, is copied to `<store>/<project>/CLAUDE.md`. Editing either copy updates the other. On start, if the two copies differ, the one modified later overwrites the other.

Options: `--config <path>` uses another config file. `-V`, `--version` and `-h`, `--help` print the version and the usage.

## Configuration

| Key | In the generated file | Meaning |
|---|---|---|
| `general.log_dir` | `"logs"` | Log folder. Relative to the config file's folder |
| `general.log_level` | `"info"` | `trace`, `debug`, `info`, `warn` or `error`. The `RUST_LOG` environment variable overrides it |
| `sync.scan_dirs` | `[]` | Folders that contain the projects whose `CLAUDE.md` is synced |
| `sync.claude_md_store_dir` | `""` | Store folder. Empty means the config file's folder |
| `sync.exclude_projects` | `["ywatchy"]` | Project folder names not to sync, case-insensitive |
| `skills.scan_dirs` | `[]` | Folders that contain the projects searched for skills |
| `skills.target_dir` | `""` | Folder for the links. Empty means `~/.claude/skills` |
| `skills.skill_patterns` | the two paths above | Paths relative to a project, with one `*` for the skill name |
| `skills.exclude_projects` | `[]` | Project folder names whose skills are not linked, case-insensitive. Their links are removed on the next start |
| `watcher.debounce_ms` | `500` | Debounce time for file change events, in milliseconds |

## Known limitations

- Only Windows is tested.
- Messages, `--help` and the comments in the generated config file are in Japanese.
- On start, the newer `CLAUDE.md` overwrites the older one without a backup.
- While running, new skills are linked only inside a `skills/` or `.claude/skills/` folder that existed when ywatchy started. Skills in other folders, and skills matched only by patterns you added to `skills.skill_patterns`, are linked on the next start.
- A `CLAUDE.md` added to an existing project folder is synced from the next start.
- Deleting a whole skill folder leaves a broken link in `~/.claude/skills`. Delete the link by hand.
- When two skills have the same name, only the first one found is linked.

## Uninstall

1. Stop ywatchy with Ctrl+C.
2. Run `scoop uninstall ywatchy`, or delete `ywatchy.exe`.
3. The links in `~/.claude/skills` and the folder `%APPDATA%\ywatchy` remain. The folder holds the config file, the logs and, when `sync.claude_md_store_dir` is empty, the copies of each project's `CLAUDE.md`. Delete them if you no longer need them.

## Build

Requires a Rust toolchain. The executable is written to `target\release\ywatchy.exe`.

```powershell
cargo build --release
```

## License

[MIT](LICENSE)
