use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::archive::is_archive_file;
use crate::config::{AppConfig, is_inside_done_dir};
use crate::detect::{InstallResult, ProcessPlan, execute_plan, plan_install};
use crate::log::{Logger, format_bytes};

#[derive(Debug, Default)]
pub struct BatchSummary {
    pub processed: Vec<(PathBuf, InstallResult)>,
    pub skipped: Vec<(PathBuf, String)>,
    pub failed: Vec<(PathBuf, String)>,
    pub moved_to_done: Vec<PathBuf>,
}

pub fn process_downloads_dir(config: &AppConfig, log: &Logger) -> Result<BatchSummary> {
    config.validate()?;
    config.ensure_output_dirs()?;

    let archives = list_pending_archives(&config.downloads_dir, &config.done_dir)?;
    let done_count = count_done_archives(&config.done_dir)?;

    log.info(&format!(
        "found {} pending archive(s) in {}",
        archives.len(),
        config.downloads_dir.display()
    ));
    if done_count > 0 {
        log.info(&format!(
            "{} archive(s) already completed in {}",
            done_count,
            config.done_dir.display()
        ));
    }

    if archives.is_empty() {
        log.info("nothing to do");
        return Ok(BatchSummary::default());
    }

    let total_bytes: u64 = archives
        .iter()
        .filter_map(|path| fs::metadata(path).ok().map(|meta| meta.len()))
        .sum();
    log.info(&format!(
        "total pending size: {}",
        format_bytes(total_bytes)
    ));

    if config.dry_run {
        log.warn("dry-run mode: no files will be written or moved");
    }

    let mut summary = BatchSummary::default();
    let total = archives.len();

    for (index, archive_path) in archives.iter().enumerate() {
        let file_name = archive_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| archive_path.display().to_string());
        let size = fs::metadata(archive_path)
            .map(|meta| format_bytes(meta.len()))
            .unwrap_or_else(|_| "unknown size".to_string());

        log.progress(
            index + 1,
            total,
            &format!("starting {file_name} ({size})"),
        );

        match process_single_archive(archive_path, config, log) {
            Ok(result) => {
                summary
                    .processed
                    .push((archive_path.to_path_buf(), result.clone()));

                if !config.dry_run {
                    match move_to_done(archive_path, &config.done_dir, log) {
                        Ok(dest) => summary.moved_to_done.push(dest),
                        Err(error) => {
                            let message = format!("{error:#}");
                            log.error(&format!(
                                "install succeeded but failed to move to done: {message}"
                            ));
                            summary
                                .failed
                                .push((archive_path.to_path_buf(), message));
                        }
                    }
                }
            }
            Err(error) => {
                let message = format!("{error:#}");
                log.error(&format!("{file_name}: {message}"));
                summary
                    .failed
                    .push((archive_path.to_path_buf(), message));
            }
        }
    }

    print_batch_summary(&summary, log);
    Ok(summary)
}

pub fn process_single_archive(
    archive_path: &Path,
    config: &AppConfig,
    log: &Logger,
) -> Result<InstallResult> {
    let file_name = archive_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| archive_path.display().to_string());

    log.step(&format!("analyzing {file_name}"));
    let plan = plan_install(archive_path, config)
        .with_context(|| format!("failed to plan install for {}", archive_path.display()))?;
    log_plan(&plan, log);

    let result = execute_plan(&plan, config, log)?;
    log.success(&format!(
        "{file_name}: {} install complete — {} file(s) -> {}",
        result.kind,
        result.files_installed,
        result.destination.display()
    ));
    Ok(result)
}

pub fn process_demo_files(config: &AppConfig, log: &Logger) -> BatchSummary {
    let demos: Vec<PathBuf> = crate::config::DEMO_FILE_NAMES
        .iter()
        .map(|name| config.downloads_dir.join(name))
        .collect();

    if config.dry_run {
        log.warn("dry-run mode: no files will be written or moved");
    }

    let mut summary = BatchSummary::default();
    let total = demos.len();

    for (index, path) in demos.iter().enumerate() {
        log.progress(index + 1, total, &format!("demo {}", path.display()));

        match process_single_archive(path, config, log) {
            Ok(result) => summary.processed.push((path.clone(), result)),
            Err(error) => {
                let message = format!("{error:#}");
                log.error(&message);
                summary.failed.push((path.clone(), message));
            }
        }
    }

    print_batch_summary(&summary, log);
    summary
}

fn list_pending_archives(downloads_dir: &Path, done_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut archives = Vec::new();

    for entry in fs::read_dir(downloads_dir)
        .with_context(|| format!("failed to read {}", downloads_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        if path == done_dir || is_inside_done_dir(&path, done_dir) {
            continue;
        }

        if entry.file_type()?.is_file() && is_archive_file(&path) {
            archives.push(path);
        }
    }

    archives.sort();
    Ok(archives)
}

fn count_done_archives(done_dir: &Path) -> Result<usize> {
    if !done_dir.is_dir() {
        return Ok(0);
    }

    let count = fs::read_dir(done_dir)
        .with_context(|| format!("failed to read {}", done_dir.display()))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file() && is_archive_file(&entry.path()))
        .count();

    Ok(count)
}

fn move_to_done(archive_path: &Path, done_dir: &Path, log: &Logger) -> Result<PathBuf> {
    fs::create_dir_all(done_dir)?;

    let file_name = archive_path
        .file_name()
        .context("archive has no file name")?;
    let mut dest = done_dir.join(file_name);

    if dest.exists() {
        let stem = archive_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "archive".to_string());
        let extension = archive_path
            .extension()
            .map(|s| s.to_string_lossy().to_string());

        let mut counter = 1u32;
        loop {
            let candidate = match extension.as_deref() {
                Some(ext) => done_dir.join(format!("{stem} ({counter}).{ext}")),
                None => done_dir.join(format!("{stem} ({counter})")),
            };
            if !candidate.exists() {
                dest = candidate;
                break;
            }
            counter += 1;
        }
        log.warn(&format!(
            "done folder already contained {}; saving as {}",
            file_name.to_string_lossy(),
            dest.file_name().unwrap().to_string_lossy()
        ));
    }

    log.step(&format!(
        "moving {} -> {}",
        archive_path.display(),
        dest.display()
    ));
    if let Err(error) = fs::rename(archive_path, &dest) {
        fs::copy(archive_path, &dest).with_context(|| {
            format!(
                "failed to move or copy {} to {}: {error:#}",
                archive_path.display(),
                dest.display()
            )
        })?;
        fs::remove_file(archive_path).with_context(|| {
            format!(
                "copied to {} but failed to remove source {}",
                dest.display(),
                archive_path.display()
            )
        })?;
    }
    log.success(&format!("archived to {}", dest.display()));

    Ok(dest)
}

fn log_plan(plan: &ProcessPlan, log: &Logger) {
    match &plan.action {
        crate::detect::InstallAction::Dim { source, destination } => {
            log.info("classified as DAZ Install Manager package");
            log.verbose(&format!("source: {source:?}"));
            log.verbose(&format!("destination: {}", destination.display()));
        }
        crate::detect::InstallAction::Manual {
            source_description,
            strip_prefix,
            destination,
            file_count_estimate,
            ..
        } => {
            log.info("classified as manual library install");
            log.verbose(&format!("source: {source_description}"));
            log.verbose(&format!(
                "strip prefix: {}",
                if strip_prefix.is_empty() {
                    "(none)".to_string()
                } else {
                    strip_prefix.clone()
                }
            ));
            log.verbose(&format!("estimated files: {file_count_estimate}"));
            log.verbose(&format!("destination: {}", destination.display()));
        }
    }
}

fn print_batch_summary(summary: &BatchSummary, log: &Logger) {
    let success_count = summary.processed.len();
    let failure_count = summary.failed.len();

    log.banner("Summary");
    log.success(&format!("success: {success_count}, failure: {failure_count}"));

    if !summary.moved_to_done.is_empty() {
        log.info(&format!("moved to done: {}", summary.moved_to_done.len()));
    }

    if !summary.skipped.is_empty() {
        log.info(&format!("skipped: {}", summary.skipped.len()));
    }

    if success_count > 0 {
        log.info("successful packages:");
        for (path, result) in &summary.processed {
            log.success(&format!(
                "  {} -> {} ({})",
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string()),
                result.kind,
                result.details
            ));
        }
    }

    if failure_count > 0 {
        log.info("failed packages:");
        for (path, reason) in &summary.failed {
            log.error(&format!(
                "  {} — {reason}",
                path.file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string())
            ));
        }
    }
}

pub fn batch_had_failures(summary: &BatchSummary) -> bool {
    !summary.failed.is_empty()
}
