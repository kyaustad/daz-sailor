use std::fs::File;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sevenz_rust2::{ArchiveReader, Password};

use super::{ArchiveEntry, ArchiveReader};

pub struct SevenZReader {
    path: PathBuf,
}

pub struct TempSevenZReader {
    data: Vec<u8>,
}

impl SevenZReader {
    pub fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl TempSevenZReader {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        Ok(Self {
            data: data.to_vec(),
        })
    }
}

impl ArchiveReader for TempSevenZReader {
    fn list_entries(&self) -> Result<Vec<ArchiveEntry>> {
        let cursor = Cursor::new(&self.data);
        list_7z_entries(cursor)
    }

    fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        let cursor = Cursor::new(&self.data);
        read_7z_file(cursor, name)
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

impl ArchiveReader for SevenZReader {
    fn list_entries(&self) -> Result<Vec<ArchiveEntry>> {
        let reader = ArchiveReader::open(&self.path, Password::empty())
            .with_context(|| format!("failed to open 7z archive {}", self.path.display()))?;
        Ok(entries_from_reader(&reader))
    }

    fn read_file(&self, name: &str) -> Result<Vec<u8>> {
        let mut reader = ArchiveReader::open(&self.path, Password::empty())
            .with_context(|| format!("failed to open 7z archive {}", self.path.display()))?;
        reader
            .read_file(name)
            .with_context(|| format!("failed to read 7z entry {name}"))
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

fn entries_from_reader(reader: &ArchiveReader<impl std::io::Read + std::io::Seek>) -> Vec<ArchiveEntry> {
    reader
        .archive()
        .files
        .iter()
        .filter(|entry| !Path::new(&entry.name).is_absolute())
        .map(|entry| ArchiveEntry {
            name: entry.name().replace('\\', "/"),
            is_dir: entry.is_directory(),
            size: entry.size(),
        })
        .collect()
}

fn list_7z_entries<R: std::io::Read + std::io::Seek>(source: R) -> Result<Vec<ArchiveEntry>> {
    let reader = ArchiveReader::new(source, Password::empty())
        .context("failed to parse 7z archive")?;
    Ok(entries_from_reader(&reader))
}

fn read_7z_file<R: std::io::Read + std::io::Seek>(source: R, name: &str) -> Result<Vec<u8>> {
    let mut reader = ArchiveReader::new(source, Password::empty())
        .context("failed to parse 7z archive")?;
    reader
        .read_file(name)
        .with_context(|| format!("failed to read 7z entry {name}"))
}
