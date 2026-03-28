mod config;
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

fn find_config() -> PathBuf {
    let args: Vec<String> = env::args().collect();

    // --config <path> で上書き可能
    if let Some(pos) = args.iter().position(|a| a == "--config") {
        if let Some(path) = args.get(pos + 1) {
            return PathBuf::from(path);
        }
        eprintln!("--config requires a path argument");
        process::exit(1);
    }

    let path = default_config_path();
    if !path.is_file() {
        if let Err(err) = Config::write_default(&path) {
            eprintln!("failed to create default config at {}: {}", path.display(), err);
            process::exit(1);
        }
        eprintln!("created default config at: {}", path.display());
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
    let config_path = find_config();
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
