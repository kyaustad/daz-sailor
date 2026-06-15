use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Default paths for local development. Override via CLI flags or environment variables.
pub const DEFAULT_DOWNLOADS: &str = "/home/kyaustad/Downloads/Daz Downloads/";
pub const DEFAULT_DIM_DOWNLOADS: &str =
    "/home/kyaustad/.wine-daz/drive_c/users/Public/Documents/DAZ 3D/InstallManager/Downloads/";
pub const DEFAULT_DAZ_LIBRARY: &str =
    "/home/kyaustad/.wine-daz/drive_c/users/Public/Documents/My DAZ 3D Library/";

pub const DEMO_FILE_1_PATH: &str =
    "/home/kyaustad/Downloads/Daz Downloads/A3S Comfort Bed.rar";
pub const DEMO_FILE_2_PATH: &str =
    "/home/kyaustad/Downloads/Daz Downloads/10 My Ass Poses 2 for G9 and G8F.rar";
pub const DEMO_FILE_3_PATH: &str =
    "/home/kyaustad/Downloads/Daz Downloads/X-Fashion Dark Punk Leather Set Genesis 9.rar";

/// Top-level folders that appear in a DAZ 3D content library.
pub const LIBRARY_FOLDERS: &[&str] = &[
    "data",
    "People",
    "Props",
    "Environments",
    "Runtime",
    "Scenes",
    "Scripts",
    "Light Presets",
    "Render Presets",
    "Render Settings",
    "Shader Presets",
    "ReadMe's",
    "Animals",
    "Documents",
    "Figures",
    "Hair",
    "Lights",
    "Materials",
    "Morphs",
    "Poses",
    "Templates",
    "Textures",
    "Vehicles",
];

/// Return the canonical DAZ library folder name for `name`, if it matches one.
pub fn canonical_library_folder(name: &str) -> Option<&'static str> {
    LIBRARY_FOLDERS
        .iter()
        .find(|folder| folder.eq_ignore_ascii_case(name))
        .copied()
}

/// Normalize the first path segment to DAZ's expected folder casing (e.g. `Data` -> `data`).
pub fn normalize_library_relative_path(relative: &Path) -> PathBuf {
    let parts: Vec<String> = relative
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            std::path::Component::CurDir => None,
            _ => None,
        })
        .collect();

    if parts.is_empty() {
        return relative.to_path_buf();
    }

    let mut normalized = parts;
    if let Some(canonical) = canonical_library_folder(&normalized[0]) {
        normalized[0] = canonical.to_string();
    }

    normalized.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_data_folder_casing() {
        let path = normalize_library_relative_path(Path::new("Data/Daz 3D/foo.dsf"));
        assert_eq!(path, Path::new("data/Daz 3D/foo.dsf"));
    }

    #[test]
    fn normalizes_people_folder_casing() {
        let path = normalize_library_relative_path(Path::new("people/Genesis 9/test.duf"));
        assert_eq!(path, Path::new("People/Genesis 9/test.duf"));
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub downloads_dir: PathBuf,
    pub dim_downloads_dir: PathBuf,
    pub daz_library_dir: PathBuf,
    pub done_dir: PathBuf,
    pub dry_run: bool,
    pub verbose: bool,
}

impl AppConfig {
    pub fn from_cli(
        downloads_dir: Option<PathBuf>,
        dim_downloads_dir: Option<PathBuf>,
        daz_library_dir: Option<PathBuf>,
        done_dir: Option<PathBuf>,
        dry_run: bool,
        verbose: bool,
    ) -> Result<Self> {
        let downloads_dir = resolve_path(
            downloads_dir,
            env::var("DAZ_SAILOR_DOWNLOADS").ok(),
            DEFAULT_DOWNLOADS,
        )?;
        let dim_downloads_dir = resolve_path(
            dim_downloads_dir,
            env::var("DAZ_SAILOR_DIM").ok(),
            DEFAULT_DIM_DOWNLOADS,
        )?;
        let daz_library_dir = resolve_path(
            daz_library_dir,
            env::var("DAZ_SAILOR_LIBRARY").ok(),
            DEFAULT_DAZ_LIBRARY,
        )?;
        let done_dir = match done_dir.or_else(|| env::var("DAZ_SAILOR_DONE").ok().map(PathBuf::from))
        {
            Some(path) => path,
            None => downloads_dir.join("done"),
        };

        Ok(Self {
            downloads_dir,
            dim_downloads_dir,
            daz_library_dir,
            done_dir,
            dry_run,
            verbose,
        })
    }

    pub fn validate(&self) -> Result<()> {
        if !self.downloads_dir.is_dir() {
            bail!(
                "downloads directory does not exist: {}",
                self.downloads_dir.display()
            );
        }
        Ok(())
    }

    pub fn ensure_output_dirs(&self) -> Result<()> {
        if self.dry_run {
            return Ok(());
        }
        std::fs::create_dir_all(&self.dim_downloads_dir).with_context(|| {
            format!(
                "failed to create DIM downloads dir: {}",
                self.dim_downloads_dir.display()
            )
        })?;
        std::fs::create_dir_all(&self.daz_library_dir).with_context(|| {
            format!(
                "failed to create library dir: {}",
                self.daz_library_dir.display()
            )
        })?;
        std::fs::create_dir_all(&self.done_dir).with_context(|| {
            format!("failed to create done dir: {}", self.done_dir.display())
        })?;
        Ok(())
    }
}

fn resolve_path(
    cli: Option<PathBuf>,
    env_var: Option<String>,
    default: &str,
) -> Result<PathBuf> {
    if let Some(path) = cli {
        return Ok(path);
    }
    if let Some(value) = env_var {
        return Ok(PathBuf::from(value));
    }
    Ok(PathBuf::from(default))
}

pub fn is_inside_done_dir(path: &Path, done_dir: &Path) -> bool {
    path.starts_with(done_dir)
}
