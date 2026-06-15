use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;

use super::{ArchiveEntry, ArchiveReader};

pub struct ExternalArchiveReader {
    path: PathBuf,
    tool: ExternalTool,
}

pub struct TempArchiveReader {
    _temp: NamedTempFile,
    inner: ExternalArchiveReader,
}

enum ExternalTool {
    Unrar,
    SevenZ,
}

impl ExternalArchiveReader {
    pub fn rar(path: &Path) -> Result<Self> {
        ensure_tool("unrar")?;
        Ok(Self {
            path: path.to_path_buf(),
            tool: ExternalTool::Unrar,
        })
    }

    pub fn seven_z(path: &Path) -> Result<Self> {
        ensure_tool("7z")?;
        Ok(Self {
            path: path.to_path_buf(),
            tool: ExternalTool::SevenZ,
        })
    }
}

impl TempArchiveReader {
    pub fn rar_from_bytes(data: &[u8]) -> Result<Self> {
        Self::from_bytes(data, ExternalTool::Unrar)
    }

    pub fn seven_z_from_bytes(data: &[u8]) -> Result<Self> {
        Self::from_bytes(data, ExternalTool::SevenZ)
    }

    fn from_bytes(data: &[u8], tool: ExternalTool) -> Result<Self> {
        let mut temp = NamedTempFile::new().context("failed to create temp archive file")?;
        temp.write_all(data)
            .context("failed to write temp archive file")?;

        let inner = match tool {
            ExternalTool::Unrar => ExternalArchiveReader::rar(temp.path())?,
            ExternalTool::SevenZ => ExternalArchiveReader::seven_z(temp.path())?,
        };

        Ok(Self { _temp: temp, inner })
    }
}

impl ArchiveReader for TempArchiveReader {
    fn list_entries(&self) -> Result<Vec<ArchiveEntry>> {
        self.inner.list_entries()
    }

    fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        self.inner.read_file(name)
    }

    fn extract_file(&self, name: &str, dest: &Path) -> Result<()> {
        self.inner.extract_file(name, dest)
    }
}

impl ArchiveReader for ExternalArchiveReader {
    fn list_entries(&self) -> Result<Vec<ArchiveEntry>> {
        match self.tool {
            ExternalTool::Unrar => list_with_unrar(&self.path),
            ExternalTool::SevenZ => list_with_7z(&self.path),
        }
    }

    fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        match self.tool {
            ExternalTool::Unrar => read_with_unrar(&self.path, name),
            ExternalTool::SevenZ => read_with_7z(&self.path, name),
        }
    }

    fn extract_file(&self, name: &str, dest: &Path) -> Result<()> {
        let data = self.read_file(name)?;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(dest)
            .with_context(|| format!("failed to create {}", dest.display()))?;
        file.write_all(&data)
            .with_context(|| format!("failed to write {}", dest.display()))?;
        Ok(())
    }
}

fn ensure_tool(tool: &str) -> Result<()> {
    let output = Command::new("which")
        .arg(tool)
        .output()
        .with_context(|| format!("failed to check for {tool}"))?;

    if output.status.success() {
        Ok(())
    } else {
        bail!("required tool not found in PATH: {tool}");
    }
}

fn list_with_unrar(path: &Path) -> Result<Vec<ArchiveEntry>> {
    // Use 7z for listing: unrar's table output splits on whitespace and corrupts
    // filenames that contain multiple consecutive spaces.
    list_with_7z(path)
}

fn parse_unrar_list(output: &str) -> Result<Vec<ArchiveEntry>> {
    let mut entries = Vec::new();
    let mut in_listing = false;

    for line in output.lines() {
        if line.starts_with("-----------") {
            in_listing = !in_listing;
            continue;
        }

        if !in_listing || line.trim().is_empty() {
            continue;
        }

        // Attributes Size Date Time Name
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }

        let size = parts[1].parse::<u64>().unwrap_or(0);
        let name = parts[4..].join(" ");
        let is_dir = is_unrar_directory_attribute(parts[0]);

        entries.push(ArchiveEntry {
            name: name.replace('\\', "/"),
            is_dir,
            size,
        });
    }

    Ok(entries)
}

/// UnRAR marks directories with `D` in the attributes column (`...D...`), not by
/// searching the filename (which breaks on paths like `dForce Dark Side`).
fn is_unrar_directory_attribute(attributes: &str) -> bool {
    attributes
        .chars()
        .nth(3)
        .is_some_and(|ch| ch == 'D')
}

fn read_with_unrar(path: &Path, name: &str) -> Result<Vec<u8>> {
    let output = Command::new("unrar")
        .args(["p", "-inul", "-p-", path.to_str().context("non-utf8 path")?, name])
        .output()
        .context("failed to run unrar")?;

    if !output.status.success() {
        bail!(
            "unrar extract failed for {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(output.stdout)
}

fn list_with_7z(path: &Path) -> Result<Vec<ArchiveEntry>> {
    let output = Command::new("7z")
        .args(["l", "-slt", path.to_str().context("non-utf8 path")?])
        .output()
        .context("failed to run 7z")?;

    if !output.status.success() {
        bail!(
            "7z list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    parse_7z_slt_list(&String::from_utf8_lossy(&output.stdout))
        .map(|entries| filter_7z_archive_entries(entries))
}

fn filter_7z_archive_entries(entries: Vec<ArchiveEntry>) -> Vec<ArchiveEntry> {
    entries
        .into_iter()
        .filter(|entry| !Path::new(&entry.name).is_absolute())
        .collect()
}

fn parse_7z_slt_list(output: &str) -> Result<Vec<ArchiveEntry>> {
    let mut entries = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_folder: Option<bool> = None;
    let mut current_size: u64 = 0;

    let flush_entry = |entries: &mut Vec<ArchiveEntry>,
                           path: Option<String>,
                           folder: Option<bool>,
                           size: u64| {
        if let Some(path) = path {
            entries.push(ArchiveEntry {
                name: path.replace('\\', "/"),
                is_dir: folder.unwrap_or(false),
                size,
            });
        }
    };

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("Path = ") {
            flush_entry(
                &mut entries,
                current_path.take(),
                current_folder.take(),
                current_size,
            );
            current_path = Some(rest.to_string());
            current_folder = None;
            current_size = 0;
            continue;
        }

        if let Some(rest) = line.strip_prefix("Folder = ") {
            current_folder = Some(rest == "+");
            continue;
        }

        if let Some(rest) = line.strip_prefix("Size = ") {
            current_size = rest.parse().unwrap_or(0);
        }
    }

    flush_entry(
        &mut entries,
        current_path,
        current_folder,
        current_size,
    );

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn does_not_mark_dforce_paths_as_directories() {
        assert!(!is_unrar_directory_attribute("..A...."));
        assert!(is_unrar_directory_attribute("...D..."));
    }

    #[test]
    fn parses_unrar_list_with_spaces_in_path() {
        let output = r#"
 Attributes       Size     Date    Time   Name
----------- ----------  ---------- -----  ----
    ..A....  111683024  2024-03-14 10:43  Daz3D 93490 - dForce Dark Side Outfit for Genesis 9/IM00093490-01_dForceDarkSideOutfitForGenesis9.zip
    ...D...          0  2025-04-30 16:44  Daz3D 93490 - dForce Dark Side Outfit for Genesis 9
----------- ----------  ---------- -----  ----
"#;
        let entries = parse_unrar_list(output).unwrap();
        let zip = entries
            .iter()
            .find(|entry| entry.name.ends_with(".zip"))
            .expect("zip entry");
        assert!(!zip.is_dir);
    }

    #[test]
    fn parses_7z_slt_file_entries_without_plus_folder() {
        let output = r#"
Path = My Library/TIP  2x Hair.dsa
Folder = -
Size = 1166
"#;
        let entries = parse_7z_slt_list(output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "My Library/TIP  2x Hair.dsa");
        assert!(!entries[0].is_dir);
    }

    #[test]
    fn parses_7z_slt_without_folder_field() {
        let output = r#"
Path = IM00091071-01_example.zip
Size = 174846448
Path = preview.jpg
Size = 650065
"#;
        let entries = parse_7z_slt_list(output).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "IM00091071-01_example.zip");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].size, 174846448);
        assert_eq!(entries[1].name, "preview.jpg");
    }
}

fn read_with_7z(path: &Path, name: &str) -> Result<Vec<u8>> {
    let temp_dir = tempfile::tempdir().context("failed to create temp dir")?;
    let output = Command::new("7z")
        .args([
            "e",
            "-y",
            &format!("-o{}", temp_dir.path().display()),
            path.to_str().context("non-utf8 path")?,
            name,
        ])
        .output()
        .context("failed to run 7z")?;

    if !output.status.success() {
        bail!(
            "7z extract failed for {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let file_name = Path::new(name)
        .file_name()
        .context("invalid entry name")?
        .to_string_lossy();
    let extracted = temp_dir.path().join(file_name.as_ref());
    let data = std::fs::read(&extracted)
        .with_context(|| format!("failed to read extracted file {}", extracted.display()))?;
    Ok(data)
}
