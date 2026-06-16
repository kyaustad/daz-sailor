use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tempfile::NamedTempFile;
use unrar_ng::Archive;

use super::{ArchiveEntry, ArchiveReader};

pub struct RarReader {
    path: PathBuf,
}

pub struct TempRarReader {
    _temp: NamedTempFile,
    inner: RarReader,
}

impl RarReader {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl TempRarReader {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut temp = NamedTempFile::new().context("failed to create temp archive file")?;
        temp.write_all(data)
            .context("failed to write temp archive file")?;

        let inner = RarReader::open(temp.path())?;
        Ok(Self { _temp: temp, inner })
    }
}

impl ArchiveReader for TempRarReader {
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

impl ArchiveReader for RarReader {
    fn list_entries(&self) -> Result<Vec<ArchiveEntry>> {
        list_rar_entries(&self.path)
    }

    fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        read_rar_file(&self.path, name)
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

fn normalize_entry_name(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn list_rar_entries(path: &Path) -> Result<Vec<ArchiveEntry>> {
    let archive = Archive::new(path)
        .open_for_listing()
        .with_context(|| format!("failed to open RAR archive {}", path.display()))?;

    let mut entries = Vec::new();
    for result in archive {
        let header = result.with_context(|| {
            format!("failed to read RAR entry listing for {}", path.display())
        })?;
        entries.push(ArchiveEntry {
            name: normalize_entry_name(&header.filename),
            is_dir: header.is_directory(),
            size: header.unpacked_size,
        });
    }

    Ok(entries)
}

fn read_rar_file(path: &Path, name: &str) -> Result<Vec<u8>> {
    let mut archive = Archive::new(path)
        .open_for_processing()
        .with_context(|| format!("failed to open RAR archive {}", path.display()))?;

    loop {
        let Some(archive) = archive
            .read_header()
            .with_context(|| format!("failed to read RAR header in {}", path.display()))?
        else {
            bail!("RAR entry not found: {name}");
        };

        let entry_name = normalize_entry_name(&archive.entry().filename);
        if entry_name == name {
            if archive.entry().is_directory() {
                bail!("RAR entry is a directory: {name}");
            }
            let (data, _) = archive
                .read()
                .with_context(|| format!("failed to read RAR entry {name}"))?;
            return Ok(data);
        }

        archive = archive
            .skip()
            .with_context(|| format!("failed to skip RAR entry while searching for {name}"))?;
    }
}
