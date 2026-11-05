//! Zip archive access for mod layers and [`crate::Source::Zip`] reads.

use std::collections::HashSet;
use std::fs::File;
use std::path::Path;

use zip::ZipArchive;

use crate::manifest::{self, MANIFEST_FILE};
use crate::{VfsError, read_limited};

pub(crate) fn load_entries(zip_path: &Path) -> Result<HashSet<String>, VfsError> {
    let file = File::open(zip_path)?;
    let archive = ZipArchive::new(file).map_err(|err| zip_error(zip_path, err))?;

    let mut entries = HashSet::new();
    for name in archive.file_names() {
        validate_entry(zip_path, name)?;
        entries.insert(name.to_owned());
    }
    Ok(entries)
}

pub(crate) fn read_manifest(zip_path: &Path) -> Result<manifest::Manifest, VfsError> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file).map_err(|err| zip_error(zip_path, err))?;

    let entry = match archive.by_name(MANIFEST_FILE) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => {
            return Err(VfsError::MissingManifest {
                path: zip_path.to_path_buf(),
            });
        }
        Err(err) => return Err(zip_error(zip_path, err)),
    };

    manifest::read_manifest(zip_path, entry)
}

pub(crate) fn read_entry(
    zip_path: &Path,
    entry_name: &str,
    size_limit: u64,
) -> Result<Vec<u8>, VfsError> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file).map_err(|err| zip_error(zip_path, err))?;
    let entry = archive
        .by_name(entry_name)
        .map_err(|err| zip_error(zip_path, err))?;

    if entry.size() > size_limit {
        return Err(crate::too_large());
    }
    read_limited(entry, size_limit)
}

fn validate_entry(zip_path: &Path, name: &str) -> Result<(), VfsError> {
    let has_parent = name.split(['/', '\\']).any(|component| component == "..");
    if has_parent || is_absolute(name) {
        return Err(VfsError::UnsafeZipEntry {
            path: zip_path.to_path_buf(),
            entry: name.to_owned(),
        });
    }
    Ok(())
}

fn is_absolute(name: &str) -> bool {
    if name.starts_with('/') || name.starts_with('\\') {
        return true;
    }
    let bytes = name.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

fn zip_error(zip_path: &Path, err: zip::result::ZipError) -> VfsError {
    VfsError::Zip(format!("{}: {err}", zip_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_entry_detection_covers_parent_and_absolute_names() {
        for name in [
            "../evil.txt",
            "assets/../../evil.txt",
            "/absolute.txt",
            "C:\\evil.txt",
        ] {
            assert!(
                validate_entry(Path::new("mod.zip"), name).is_err(),
                "{name} must be rejected"
            );
        }
        for name in ["images/bf.png", "..hidden", "assets/foo..bar.txt"] {
            assert!(
                validate_entry(Path::new("mod.zip"), name).is_ok(),
                "{name} must be accepted"
            );
        }
    }
}
