mod config;
mod print;
mod skills;
mod sync;
mod watcher;

use std::{env, fs, io, path::{Path, PathBuf}, process};

use config::Config;
use tracing::{error, info};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{prelude::*, EnvFilter};

fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ywatchy")
        .join("ywatchy.toml")
}

const USAGE: &str = "\
usage: ywatchy [--config <path>]

  --config <path>  設定ファイルを指定 (既定: <config_dir>/ywatchy/ywatchy.toml)
  -V, --version    バージョンを表示して終了
  -h, --help       この説明を表示して終了

引数なしで起動すると常駐し、skill の symlink と CLAUDE.md の sync を行う。";

#[derive(Debug, PartialEq, Eq)]
enum CliAction {
    Run { config: Option<PathBuf> },
    Version,
    Help,
}

/// 引数を解釈する。知らない引数はエラーにし、常駐を始めない
fn parse_args<I: IntoIterator<Item = String>>(args: I) -> Result<CliAction, String> {
    let mut config = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-V" | "--version" => return Ok(CliAction::Version),
            "-h" | "--help" => return Ok(CliAction::Help),
            "--config" => match iter.next() {
                Some(path) => config = Some(PathBuf::from(path)),
                None => return Err(String::from("--config requires a path argument")),
            },
            other => return Err(format!("unknown argument: {}", other)),
        }
    }
    Ok(CliAction::Run { config })
}

fn find_config(configured: Option<PathBuf>) -> PathBuf {
    if let Some(path) = configured {
        return path;
    }

    let path = default_config_path();
    if !path.is_file() {
        if let Err(err) = Config::write_default(&path) {
            eprintln!("failed to create default config at {}: {}", path.display(), err);
            process::exit(1);
        }
        eprintln!("設定ファイルを生成しました: {}", path.display());
        eprintln!("scan_dirs を編集してから再度実行してください。");
        process::exit(0);
    }
    path
}

fn setup_logging(config: &Config, ywatchy_root: &Path) -> io::Result<WorkerGuard> {
    let log_dir = config.resolve_log_dir(ywatchy_root);
    fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::daily(log_dir, "ywatchy.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(config.general.log_level.clone()))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(non_blocking),
        )
        .init();

    Ok(guard)
}

fn main() {
    let configured = match parse_args(env::args().skip(1)) {
        Ok(CliAction::Version) => {
            println!("ywatchy {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Ok(CliAction::Help) => {
            println!("{}", USAGE);
            return;
        }
        Ok(CliAction::Run { config }) => config,
        Err(err) => {
            eprintln!("{}", err);
            eprintln!();
            eprintln!("{}", USAGE);
            process::exit(2);
        }
    };
    let config_path = find_config(configured);
    let config_dir = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let config = match Config::load(&config_path) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("failed to load config from {}: {}", config_path.display(), err);
            process::exit(1);
        }
    };

    let _log_guard = match setup_logging(&config, &config_dir) {
        Ok(guard) => guard,
        Err(err) => {
            eprintln!("failed to initialize logging: {}", err);
            process::exit(1);
        }
    };

    info!(config = %config_path.display(), "ywatchy starting");

    if !config.has_scan_dirs() {
        eprintln!("scan_dirs が未設定です。");
        eprintln!();
        eprintln!("設定ファイルを編集してください:");
        eprintln!("  {}", config_path.display());
        eprintln!();
        eprintln!("最低限、scan_dirs にプロジェクト親ディレクトリを指定:");
        eprintln!();
        eprintln!("  [sync]");
        eprintln!("  scan_dirs = [\"C:/path/to/your/projects\"]");
        eprintln!();
        eprintln!("  [skills]");
        eprintln!("  scan_dirs = [\"C:/path/to/your/projects\"]");
        process::exit(1);
    }

    if let Err(err) = ctrlc::set_handler(|| {
        info!("received Ctrl+C, exiting");
        process::exit(0);
    }) {
        error!(error = %err, "failed to set Ctrl+C handler");
        process::exit(1);
    }

    if let Err(err) = watcher::run(config, config_dir) {
        error!(error = %err, "watcher exited with error");
        eprintln!("watcher exited with error: {}", err);
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<CliAction, String> {
        parse_args(args.iter().map(|a| a.to_string()))
    }

    #[test]
    fn no_args_runs_with_default_config() {
        assert_eq!(parse(&[]), Ok(CliAction::Run { config: None }));
    }

    #[test]
    fn config_arg_sets_path() {
        assert_eq!(
            parse(&["--config", "a.toml"]),
            Ok(CliAction::Run { config: Some(PathBuf::from("a.toml")) })
        );
    }

    #[test]
    fn version_and_help_do_not_run() {
        assert_eq!(parse(&["--version"]), Ok(CliAction::Version));
        assert_eq!(parse(&["-V"]), Ok(CliAction::Version));
        assert_eq!(parse(&["--help"]), Ok(CliAction::Help));
        assert_eq!(parse(&["-h"]), Ok(CliAction::Help));
    }

    #[test]
    fn unknown_or_incomplete_args_are_errors() {
        assert!(parse(&["--verbose"]).is_err());
        assert!(parse(&["status"]).is_err());
        assert!(parse(&["--config"]).is_err());
    }
}
