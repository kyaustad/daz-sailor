use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::{ArchiveEntry, ArchiveReader};

pub struct ZipReader {
    path: PathBuf,
}

pub struct ZipBytesReader {
    data: Vec<u8>,
}

impl ZipReader {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl ZipBytesReader {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl ArchiveReader for ZipReader {
    fn list_entries(&self) -> Result<Vec<ArchiveEntry>> {
        let file = std::fs::File::open(&self.path)
            .with_context(|| format!("failed to open {}", self.path.display()))?;
        list_zip_entries_from_reader(file)
    }

    fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        let file = std::fs::File::open(&self.path)
            .with_context(|| format!("failed to open {}", self.path.display()))?;
        read_zip_entry_from_reader(file, name)
    }

    fn extract_file(&self, name: &str, dest: &Path) -> Result<()> {
        let data = self.read_file(name)?;
        write_bytes_to_path(&data, dest)
    }
}

impl ArchiveReader for ZipBytesReader {
    fn list_entries(&self) -> Result<Vec<ArchiveEntry>> {
        list_zip_entries_from_reader(Cursor::new(&self.data))
    }

    fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        read_zip_entry_from_reader(Cursor::new(&self.data), name)
    }

    fn extract_file(&self, name: &str, dest: &Path) -> Result<()> {
        let data = self.read_file(name)?;
        write_bytes_to_path(&data, dest)
    }
}

fn list_zip_entries_from_reader<R: Read + std::io::Seek>(
    reader: R,
) -> Result<Vec<ArchiveEntry>> {
    let mut archive = zip::ZipArchive::new(reader).context("failed to read zip archive")?;
    let mut entries = Vec::with_capacity(archive.len());

    for index in 0..archive.len() {
        let entry = archive.by_index(index).context("failed to read zip entry")?;
        entries.push(ArchiveEntry {
            name: entry.name().replace('\\', "/"),
            is_dir: entry.is_dir() || entry.name().ends_with('/'),
            size: entry.size(),
        });
    }

    Ok(entries)
}

fn read_zip_entry_from_reader<R: Read + std::io::Seek>(
    reader: R,
    name: &str,
) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(reader).context("failed to read zip archive")?;
    let target = normalize_entry_name(name);

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).context("failed to read zip entry")?;
        if normalize_entry_name(entry.name()) == target {
            let mut data = Vec::new();
            entry
                .read_to_end(&mut data)
                .with_context(|| format!("failed to read zip entry {name}"))?;
            return Ok(data);
        }
    }

    anyhow::bail!("zip entry not found: {name}");
}

fn write_bytes_to_path(data: &[u8], dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(dest)
        .with_context(|| format!("failed to create {}", dest.display()))?;
    file.write_all(data)
        .with_context(|| format!("failed to write {}", dest.display()))?;
    Ok(())
}

fn normalize_entry_name(name: &str) -> String {
    name.replace('\\', "/")
}
