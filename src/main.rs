mod archive;
mod config;
mod detect;
mod log;
mod paths;
mod processor;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, ValueEnum};

use config::AppConfig;
use log::Logger;
use processor::{batch_had_failures, process_demo_files, process_downloads_dir, process_single_archive};

/// Load `.env` when present; otherwise fall back to `directories.env` at the project root.
fn load_env() {
    if dotenvy::dotenv().is_err() {
        let _ = dotenvy::from_filename("directories.env");
    }
}

#[derive(Debug, Clone, ValueEnum)]
enum Mode {
    /// Process a single archive file.
    SingleFile,
    /// Process every archive in the downloads folder.
    EntireFolder,
    /// Run against the three built-in demo archives.
    Demo,
}

#[derive(Parser, Debug)]
#[command(
    name = "daz-sailor",
    about = "Route DAZ Studio downloads to Install Manager or the content library",
    after_help = "Environment variables (set in .env or directories.env, or override via CLI):\n  \
                  DAZ_SAILOR_DOWNLOADS   Downloads folder to scan\n  \
                  DAZ_SAILOR_DIM         DAZ Install Manager downloads folder\n  \
                  DAZ_SAILOR_LIBRARY     DAZ content library folder\n  \
                  DAZ_SAILOR_DONE        Completed archives folder (default: <downloads>/done)"
)]
struct Cli {
    #[arg(long, value_enum, default_value = "entire-folder")]
    mode: Mode,

    /// Archive file to process when mode is single-file (`--file` or positional).
    #[arg(long, value_name = "ARCHIVE")]
    file: Option<PathBuf>,

    /// Archive file (positional shorthand for `--mode single-file`).
    #[arg(value_name = "ARCHIVE")]
    archive: Option<PathBuf>,

    /// Downloads directory for entire-folder mode.
    #[arg(long, env = "DAZ_SAILOR_DOWNLOADS")]
    downloads_dir: Option<PathBuf>,

    /// DAZ Install Manager downloads directory.
    #[arg(long, env = "DAZ_SAILOR_DIM")]
    dim_downloads_dir: Option<PathBuf>,

    /// DAZ content library directory.
    #[arg(long, env = "DAZ_SAILOR_LIBRARY")]
    daz_library_dir: Option<PathBuf>,

    /// Folder to move completed archives into (default: <downloads>/done).
    #[arg(long, env = "DAZ_SAILOR_DONE")]
    done_dir: Option<PathBuf>,

    /// Show what would happen without writing files or moving archives.
    #[arg(long)]
    dry_run: bool,

    /// Log per-file details during extraction.
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> Result<()> {
    load_env();
    let cli = Cli::parse();
    let config = AppConfig::from_cli(
        cli.downloads_dir,
        cli.dim_downloads_dir,
        cli.daz_library_dir,
        cli.done_dir,
        cli.dry_run,
        cli.verbose,
    )?;

    let log = Logger::new(config.verbose);
    log.banner("daz-sailor");
    log.config_line("Downloads", &config.downloads_dir);
    log.config_line("DIM queue", &config.dim_downloads_dir);
    log.config_line("Library", &config.daz_library_dir);
    log.config_line("Done", &config.done_dir);
    if config.dry_run {
        log.warn("dry-run enabled");
    }

    if let Some(file) = cli.file.or(cli.archive) {
        config.ensure_output_dirs()?;
        process_single_archive(&file, &config, &log)?;
        return Ok(());
    }

    let had_failures = match cli.mode {
        Mode::SingleFile => {
            anyhow::bail!("--file or a positional ARCHIVE path is required when --mode single-file is used");
        }
        Mode::EntireFolder => {
            let summary = process_downloads_dir(&config, &log)?;
            batch_had_failures(&summary)
        }
        Mode::Demo => {
            let summary = process_demo_files(&config, &log);
            batch_had_failures(&summary)
        }
    };

    if had_failures {
        std::process::exit(1);
    }

    Ok(())
}
