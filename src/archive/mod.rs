mod external;
mod zip_archive;

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::paths::{is_installable_library_path, strip_archive_prefix_fuzzy};

pub use zip_archive::{ZipBytesReader, ZipReader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    Rar,
    SevenZ,
}

impl ArchiveFormat {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("zip") => Some(Self::Zip),
            Some("rar") => Some(Self::Rar),
            Some("7z") => Some(Self::SevenZ),
            _ => None,
        }
    }
}

pub fn is_nested_archive_name(name: &str) -> bool {
    ArchiveFormat::from_path(Path::new(name)).is_some()
}

pub fn open_archive(path: &Path) -> Result<Box<dyn ArchiveReader>> {
    let format = ArchiveFormat::from_path(path)
        .with_context(|| format!("unsupported archive type: {}", path.display()))?;

    match format {
        ArchiveFormat::Zip => Ok(Box::new(ZipReader::open(path)?)),
        ArchiveFormat::Rar => Ok(Box::new(external::ExternalArchiveReader::rar(path)?)),
        ArchiveFormat::SevenZ => Ok(Box::new(external::ExternalArchiveReader::seven_z(path)?)),
    }
}

pub fn open_archive_from_bytes(data: &[u8], name: &str) -> Result<Box<dyn ArchiveReader>> {
    match ArchiveFormat::from_path(Path::new(name)) {
        Some(ArchiveFormat::Zip) => Ok(Box::new(ZipBytesReader::new(data.to_vec()))),
        Some(ArchiveFormat::Rar) => Ok(Box::new(
            external::TempArchiveReader::rar_from_bytes(data)?,
        )),
        Some(ArchiveFormat::SevenZ) => Ok(Box::new(
            external::TempArchiveReader::seven_z_from_bytes(data)?,
        )),
        None => bail!("unsupported nested archive type: {name}"),
    }
}

pub fn list_container_paths(data: &[u8], container_name: &str) -> Result<Vec<String>> {
    let reader = open_archive_from_bytes(data, container_name)?;
    Ok(reader
        .list_entries()?
        .into_iter()
        .map(|entry| entry.name)
        .collect())
}

#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

pub trait ArchiveReader {
    fn list_entries(&self) -> Result<Vec<ArchiveEntry>>;
    fn read_file(&self, name: &str) -> Result<Vec<u8>>;
    fn extract_file(&self, name: &str, dest: &Path) -> Result<()>;
}

pub fn list_zip_entries(data: &[u8]) -> Result<Vec<String>> {
    list_container_paths(data, "container.zip")
}

pub fn read_zip_entry(data: &[u8], name: &str) -> Result<Vec<u8>> {
    let reader = open_archive_from_bytes(data, "container.zip")?;
    reader.read_file(name)
}

pub fn extract_zip_to_dir(
    data: &[u8],
    dest: &Path,
    strip_prefix: &str,
    on_progress: impl FnMut(usize, usize),
) -> Result<u64> {
    let reader = open_archive_from_bytes(data, "container.zip")?;
    extract_reader_to_dir(&*reader, dest, strip_prefix, on_progress)
}

pub fn extract_reader_to_dir(
    reader: &dyn ArchiveReader,
    dest: &Path,
    strip_prefix: &str,
    mut on_progress: impl FnMut(usize, usize),
) -> Result<u64> {
    std::fs::create_dir_all(dest)?;

    let file_entries: Vec<_> = reader
        .list_entries()?
        .into_iter()
        .filter(|entry| {
            !entry.is_dir && is_installable_library_path(&entry.name, strip_prefix)
        })
        .collect();

    if file_entries.is_empty() {
        bail!("no installable library files found in archive");
    }

    let total = file_entries.len();
    let mut files_copied = 0u64;

    for (index, entry) in file_entries.iter().enumerate() {
        let relative = strip_archive_prefix_fuzzy(&entry.name, strip_prefix)
            .with_context(|| format!("could not map archive entry to library path: {}", entry.name))?;
        let out_path = dest.join(relative);
        reader.extract_file(&entry.name, &out_path)?;
        files_copied += 1;

        let current = index + 1;
        if current == 1 || current == total || current % 50 == 0 {
            on_progress(current, total);
        }
    }

    Ok(files_copied)
}

pub fn is_archive_file(path: &Path) -> bool {
    ArchiveFormat::from_path(path).is_some()
}
