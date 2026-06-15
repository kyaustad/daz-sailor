use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::archive::{
    ArchiveFormat, extract_reader_to_dir, extract_zip_to_dir, is_nested_archive_name,
    list_container_paths, open_archive, open_archive_from_bytes, read_zip_entry,
};
use crate::config::AppConfig;
use crate::paths::{is_installable_library_path, strip_archive_prefix_fuzzy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DimInstallSource {
    /// Zip file stored directly inside the outer archive.
    Direct { archive_entry: String },
    /// DIM zip nested inside another zip within the outer archive.
    NestedZip {
        container_entry: String,
        nested_zip_path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallAction {
    Dim {
        source: DimInstallSource,
        destination: PathBuf,
    },
    Manual {
        source_description: String,
        archive_entry: Option<String>,
        nested_zip_path: Option<String>,
        strip_prefix: String,
        destination: PathBuf,
        file_count_estimate: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessPlan {
    pub archive_path: PathBuf,
    pub action: InstallAction,
}

pub fn plan_install(archive_path: &Path, config: &AppConfig) -> Result<ProcessPlan> {
    let reader = open_archive(archive_path)?;
    let entries = reader.list_entries()?;

    let inner_archives: Vec<_> = entries
        .iter()
        .filter(|entry| !entry.is_dir && is_nested_archive_name(&entry.name))
        .collect();

    if !inner_archives.is_empty() {
        for archive_entry in &inner_archives {
            let data = reader
                .read_file(&archive_entry.name)
                .with_context(|| format!("failed to read inner archive {}", archive_entry.name))?;

            if let Some(source) = find_dim_source(&data, &archive_entry.name) {
                let output_name = dim_output_filename(&source);
                return Ok(ProcessPlan {
                    archive_path: archive_path.to_path_buf(),
                    action: InstallAction::Dim {
                        source,
                        destination: config.dim_downloads_dir.join(output_name),
                    },
                });
            }
        }

        for archive_entry in &inner_archives {
            let data = reader
                .read_file(&archive_entry.name)
                .with_context(|| format!("failed to read inner archive {}", archive_entry.name))?;

            if let Some(manual) = find_manual_in_container(&data, &archive_entry.name)? {
                return Ok(ProcessPlan {
                    archive_path: archive_path.to_path_buf(),
                    action: InstallAction::Manual {
                        source_description: manual.description,
                        archive_entry: Some(archive_entry.name.clone()),
                        nested_zip_path: manual.nested_zip_path,
                        strip_prefix: manual.strip_prefix,
                        destination: config.daz_library_dir.clone(),
                        file_count_estimate: manual.file_count_estimate,
                    },
                });
            }
        }

        bail!("no inner archive contains recognizable DAZ library folders or DIM package");
    }

    let outer_paths: Vec<String> = entries.iter().map(|entry| entry.name.clone()).collect();
    if has_library_content(&outer_paths) {
        let strip_prefix = detect_strip_prefix(&outer_paths)?;
        return Ok(ProcessPlan {
            archive_path: archive_path.to_path_buf(),
            action: InstallAction::Manual {
                source_description: "outer archive".to_string(),
                archive_entry: None,
                nested_zip_path: None,
                strip_prefix: strip_prefix.clone(),
                destination: config.daz_library_dir.clone(),
                file_count_estimate: count_installable_paths(&outer_paths, &strip_prefix),
            },
        });
    }

    bail!(
        "could not classify archive: no DIM package or recognizable DAZ library structure found"
    );
}

use crate::log::Logger;

pub fn execute_plan(plan: &ProcessPlan, config: &AppConfig, log: &Logger) -> Result<InstallResult> {
    match &plan.action {
        InstallAction::Dim { source, destination } => {
            install_dim(&plan.archive_path, source, destination, config, log)
        }
        InstallAction::Manual {
            source_description,
            archive_entry,
            nested_zip_path,
            strip_prefix,
            destination,
            file_count_estimate,
        } => install_manual(
            &plan.archive_path,
            source_description,
            archive_entry.as_deref(),
            nested_zip_path.as_deref(),
            strip_prefix,
            destination,
            *file_count_estimate,
            config,
            log,
        ),
    }
}

#[derive(Debug, Clone)]
pub struct InstallResult {
    pub kind: &'static str,
    pub files_installed: u64,
    pub destination: PathBuf,
    pub details: String,
}

fn install_dim(
    archive_path: &Path,
    source: &DimInstallSource,
    destination: &Path,
    config: &AppConfig,
    log: &Logger,
) -> Result<InstallResult> {
    let (label, output_name) = match source {
        DimInstallSource::Direct { archive_entry } => (archive_entry.as_str(), archive_entry.as_str()),
        DimInstallSource::NestedZip {
            nested_zip_path,
            ..
        } => (nested_zip_path.as_str(), nested_zip_path.as_str()),
    };

    if config.dry_run {
        log.step(&format!(
            "would copy DIM package {output_name} to {}",
            destination.display()
        ));
        return Ok(InstallResult {
            kind: "dim",
            files_installed: 0,
            destination: destination.to_path_buf(),
            details: format!("would copy DIM package {output_name} to DIM downloads"),
        });
    }

    if destination.exists() {
        log.warn(&format!(
            "DIM package already queued at {}; skipping copy",
            destination.display()
        ));
        return Ok(InstallResult {
            kind: "dim",
            files_installed: 0,
            destination: destination.to_path_buf(),
            details: format!("DIM package already exists at {}", destination.display()),
        });
    }

    log.step(&format!("extracting {label} to DIM downloads"));
    let reader = open_archive(archive_path)?;
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let dim_bytes = match source {
        DimInstallSource::Direct { archive_entry } => reader.read_file(archive_entry)?,
        DimInstallSource::NestedZip {
            container_entry,
            nested_zip_path,
        } => {
            let container = reader.read_file(container_entry)?;
            read_zip_entry(&container, nested_zip_path)?
        }
    };

    std::fs::write(destination, dim_bytes)
        .with_context(|| format!("failed to write {}", destination.display()))?;
    log.success(&format!("DIM package ready at {}", destination.display()));

    Ok(InstallResult {
        kind: "dim",
        files_installed: 1,
        destination: destination.to_path_buf(),
        details: format!("copied {output_name} to DIM downloads"),
    })
}

fn install_manual(
    archive_path: &Path,
    source_description: &str,
    archive_entry: Option<&str>,
    nested_zip_path: Option<&str>,
    strip_prefix: &str,
    destination: &Path,
    file_count_estimate: usize,
    config: &AppConfig,
    log: &Logger,
) -> Result<InstallResult> {
    let reader = open_archive(archive_path)?;

    if let Some(entry_name) = archive_entry {
        let container = reader.read_file(entry_name)?;
        let format = ArchiveFormat::from_path(Path::new(entry_name))
            .context("inner archive has unsupported format")?;

        if format == ArchiveFormat::Zip {
            let zip_data = if let Some(nested) = nested_zip_path {
                read_zip_entry(&container, nested)?
            } else {
                container
            };

            if config.dry_run {
                let inner_paths = list_container_paths(&zip_data, "container.zip")?;
                let file_count = count_installable_paths(&inner_paths, strip_prefix);
                log.step(&format!(
                    "would install {file_count} files from {source_description} (strip prefix: {})",
                    if strip_prefix.is_empty() {
                        "(none)".to_string()
                    } else {
                        strip_prefix.to_string()
                    }
                ));
                return Ok(InstallResult {
                    kind: "manual",
                    files_installed: 0,
                    destination: destination.to_path_buf(),
                    details: format!(
                        "would install {file_count} files from {source_description} with strip prefix {:?}",
                        strip_prefix
                    ),
                });
            }

            log.step(&format!(
                "installing ~{file_count_estimate} files from {source_description} into library"
            ));
            let files_installed =
                extract_zip_to_dir(&zip_data, destination, strip_prefix, |current, total| {
                    if current == 1 || current == total || current % 50 == 0 {
                        log.progress(current, total, "extracting library files");
                    }
                })?;
            log.success(&format!("installed {files_installed} files into library"));
            return Ok(InstallResult {
                kind: "manual",
                files_installed,
                destination: destination.to_path_buf(),
                details: format!("installed {files_installed} files from {source_description}"),
            });
        }

        let inner_reader = open_archive_from_bytes(&container, entry_name)?;

        if config.dry_run {
            let inner_paths: Vec<String> = inner_reader
                .list_entries()?
                .into_iter()
                .map(|entry| entry.name)
                .collect();
            let file_count = count_installable_paths(&inner_paths, strip_prefix);
            log.step(&format!(
                "would install {file_count} files from {source_description} (strip prefix: {})",
                if strip_prefix.is_empty() {
                    "(none)".to_string()
                } else {
                    strip_prefix.to_string()
                }
            ));
            return Ok(InstallResult {
                kind: "manual",
                files_installed: 0,
                destination: destination.to_path_buf(),
                details: format!(
                    "would install {file_count} files from {source_description} with strip prefix {:?}",
                    strip_prefix
                ),
            });
        }

        log.step(&format!(
            "installing ~{file_count_estimate} files from {source_description} into library"
        ));
        let files_installed =
            extract_reader_to_dir(&*inner_reader, destination, strip_prefix, |current, total| {
                if current == 1 || current == total || current % 50 == 0 {
                    log.progress(current, total, "extracting library files");
                }
            })?;
        log.success(&format!("installed {files_installed} files into library"));
        return Ok(InstallResult {
            kind: "manual",
            files_installed,
            destination: destination.to_path_buf(),
            details: format!("installed {files_installed} files from {source_description}"),
        });
    }

    let entries = reader.list_entries()?;

    if config.dry_run {
        let file_count = entries
            .iter()
            .filter(|entry| {
                !entry.is_dir && is_installable_library_path(&entry.name, strip_prefix)
            })
            .count();
        log.step(&format!(
            "would install {file_count} files from outer archive (strip prefix: {strip_prefix:?})"
        ));
        return Ok(InstallResult {
            kind: "manual",
            files_installed: 0,
            destination: destination.to_path_buf(),
            details: format!(
                "would install {file_count} files from outer archive with strip prefix {:?}",
                strip_prefix
            ),
        });
    }

    let file_entries: Vec<_> = entries
        .iter()
        .filter(|entry| {
            !entry.is_dir && is_installable_library_path(&entry.name, strip_prefix)
        })
        .collect();

    if file_entries.is_empty() {
        bail!("no installable library files found in outer archive");
    }

    let total = file_entries.len();
    log.step(&format!("installing {total} files from outer archive into library"));

    let mut files_installed = 0u64;
    for (index, entry) in file_entries.iter().enumerate() {
        let relative = strip_archive_prefix_fuzzy(&entry.name, strip_prefix)
            .with_context(|| format!("failed to map {}", entry.name))?;
        let out_path = destination.join(relative);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        reader.extract_file(&entry.name, &out_path)?;
        files_installed += 1;

        let current = index + 1;
        if current == 1 || current == total || current % 50 == 0 {
            log.progress(current, total, &entry.name);
        }
    }

    log.success(&format!("installed {files_installed} files into library"));
    Ok(InstallResult {
        kind: "manual",
        files_installed,
        destination: destination.to_path_buf(),
        details: format!("installed {files_installed} files from outer archive"),
    })
}

fn is_dim_package(paths: &[String]) -> bool {
    let has_manifest = paths
        .iter()
        .any(|path| path.to_ascii_lowercase().ends_with("manifest.dsx"));
    let has_supplement = paths
        .iter()
        .any(|path| path.to_ascii_lowercase().ends_with("supplement.dsx"));
    has_manifest && has_supplement
}

fn has_library_content(paths: &[String]) -> bool {
    paths.iter().any(|path| path_contains_library_folder(path))
}

pub fn detect_strip_prefix(paths: &[String]) -> Result<String> {
    let mut prefix_scores: HashMap<String, usize> = HashMap::new();
    let mut has_root_library_folder = false;

    for path in paths {
        let parts: Vec<&str> = path
            .trim_start_matches("./")
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();

        for (index, part) in parts.iter().enumerate() {
            if !is_library_folder(part) {
                continue;
            }

            if index == 0 {
                has_root_library_folder = true;
                break;
            }

            let prefix = parts[..index].join("/");
            *prefix_scores.entry(prefix).or_default() += 1;
        }
    }

    if has_root_library_folder {
        return Ok(String::new());
    }

    let best = prefix_scores
        .into_iter()
        .max_by(|(prefix_a, score_a), (prefix_b, score_b)| {
            score_a
                .cmp(score_b)
                .then_with(|| prefix_b.len().cmp(&prefix_a.len()))
        })
        .map(|(prefix, _)| prefix);

    best.ok_or_else(|| {
        anyhow::anyhow!("could not detect library folder structure in archive paths")
    })
}

fn find_dim_source(data: &[u8], container_name: &str) -> Option<DimInstallSource> {
    let paths = list_container_paths(data, container_name).ok()?;
    if is_dim_package(&paths) {
        return Some(DimInstallSource::Direct {
            archive_entry: container_name.to_string(),
        });
    }

    if ArchiveFormat::from_path(Path::new(container_name)) != Some(ArchiveFormat::Zip) {
        return None;
    }

    for path in paths
        .iter()
        .filter(|path| path.to_ascii_lowercase().ends_with(".zip"))
    {
        let Ok(nested) = read_zip_entry(data, path) else {
            continue;
        };
        let Ok(nested_paths) = list_container_paths(&nested, path) else {
            continue;
        };
        if is_dim_package(&nested_paths) {
            return Some(DimInstallSource::NestedZip {
                container_entry: container_name.to_string(),
                nested_zip_path: path.clone(),
            });
        }
    }

    None
}

struct ManualSource {
    description: String,
    nested_zip_path: Option<String>,
    strip_prefix: String,
    file_count_estimate: usize,
}

fn find_manual_in_container(data: &[u8], container_name: &str) -> Result<Option<ManualSource>> {
    let paths = list_container_paths(data, container_name)?;
    if let Some(source) = manual_source_from_paths(&paths, container_name, None)? {
        return Ok(Some(source));
    }

    if ArchiveFormat::from_path(Path::new(container_name)) == Some(ArchiveFormat::Zip) {
        for path in paths
            .iter()
            .filter(|path| is_nested_archive_name(path))
        {
            let nested = read_zip_entry(data, path)
                .with_context(|| format!("failed to read nested archive {path} in {container_name}"))?;
            if let Some(source) = find_manual_in_container(&nested, path)? {
                return Ok(Some(ManualSource {
                    description: format!("{} in {container_name}", source.description),
                    nested_zip_path: Some(path.clone()),
                    strip_prefix: source.strip_prefix,
                    file_count_estimate: source.file_count_estimate,
                }));
            }
        }
    }

    Ok(None)
}

fn manual_source_from_paths(
    paths: &[String],
    container_name: &str,
    nested_zip_path: Option<String>,
) -> Result<Option<ManualSource>> {
    if !has_library_content(paths) {
        return Ok(None);
    }

    let strip_prefix = detect_strip_prefix(paths)?;
    let file_count_estimate = count_installable_paths(paths, &strip_prefix);
    if file_count_estimate == 0 {
        return Ok(None);
    }

    let description = match nested_zip_path.as_deref() {
        Some(nested) => format!("nested archive {nested} in {container_name}"),
        None => match ArchiveFormat::from_path(Path::new(container_name)) {
            Some(ArchiveFormat::Zip) => format!("inner zip {container_name}"),
            _ => format!("inner archive {container_name}"),
        },
    };

    Ok(Some(ManualSource {
        description,
        nested_zip_path,
        strip_prefix,
        file_count_estimate,
    }))
}

fn dim_output_filename(source: &DimInstallSource) -> PathBuf {
    let name = match source {
        DimInstallSource::Direct { archive_entry } => archive_entry,
        DimInstallSource::NestedZip { nested_zip_path, .. } => nested_zip_path,
    };
    Path::new(name).file_name().unwrap().into()
}

fn count_installable_paths(paths: &[String], strip_prefix: &str) -> usize {
    paths
        .iter()
        .filter(|path| !path.ends_with('/') && is_installable_library_path(path, strip_prefix))
        .count()
}

fn path_contains_library_folder(path: &str) -> bool {
    let parts: Vec<&str> = path
        .trim_start_matches("./")
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();

    parts.iter().any(|part| is_library_folder(part))
}

fn is_library_folder(name: &str) -> bool {
    crate::config::LIBRARY_FOLDERS
        .iter()
        .any(|folder| folder.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_empty_prefix_for_root_library_folders() {
        let paths = vec![
            "data/foo/bar.dsf".to_string(),
            "People/Genesis 9/Poses/test.duf".to_string(),
        ];
        assert_eq!(detect_strip_prefix(&paths).unwrap(), "");
    }

    #[test]
    fn detects_my_library_wrapper() {
        let paths = vec![
            "My Library/People/Genesis 8 Female/Poses/test.duf".to_string(),
            "My Library/People/Genesis 9/Poses/test.duf".to_string(),
        ];
        assert_eq!(detect_strip_prefix(&paths).unwrap(), "My Library");
    }

    #[test]
    fn detects_product_folder_wrapper() {
        let paths = vec![
            "AC Cumshots for Dicktator for G9/data/foo/bar.dsf".to_string(),
            "AC Cumshots for Dicktator for G9/People/test.duf".to_string(),
        ];
        assert_eq!(
            detect_strip_prefix(&paths).unwrap(),
            "AC Cumshots for Dicktator for G9"
        );
    }

    #[test]
    fn recognizes_dim_package() {
        let paths = vec![
            "Content/data/foo.dsf".to_string(),
            "Manifest.dsx".to_string(),
            "Supplement.dsx".to_string(),
        ];
        assert!(is_dim_package(&paths));
    }

    #[test]
    fn recognizes_nested_dim_package() {
        let paths = vec![
            "IM80069892-01_StormingThePalaceforG9GoldenPalace.dsx".to_string(),
            "IM80069892-01_StormingThePalaceforG9GoldenPalace.zip".to_string(),
        ];
        assert!(!is_dim_package(&paths));
    }
}
