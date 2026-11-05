//! Polymod `_polymod_meta.json` parsing.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::VfsError;

pub(crate) const MANIFEST_FILE: &str = "_polymod_meta.json";
pub(crate) const MANIFEST_SIZE_LIMIT: u64 = 1024 * 1024;

/// One entry of the manifest's `contributors` array.
#[derive(Debug, Clone, Deserialize)]
pub struct Contributor {
    /// Display name of the contributor.
    pub name: String,
}

/// Parsed Polymod manifest. Field names mirror the JSON keys verbatim.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub title: String,
    pub description: String,
    pub contributors: Vec<Contributor>,
    pub api_version: String,
    pub mod_version: String,
    pub license: String,
}

pub(crate) fn parse(path: &Path, bytes: &[u8]) -> Result<Manifest, VfsError> {
    serde_json::from_slice(bytes).map_err(|err| VfsError::Manifest {
        path: path.to_path_buf(),
        detail: err.to_string(),
    })
}

pub(crate) fn read_manifest(path: &Path, reader: impl Read) -> Result<Manifest, VfsError> {
    let bytes = crate::read_limited(reader, MANIFEST_SIZE_LIMIT).map_err(|err| {
        if crate::is_too_large(&err) {
            VfsError::Manifest {
                path: path.to_path_buf(),
                detail: format!("manifest exceeds {MANIFEST_SIZE_LIMIT} bytes"),
            }
        } else {
            err
        }
    })?;
    parse(path, &bytes)
}

pub(crate) fn read_dir_manifest(dir: &Path) -> Result<Manifest, VfsError> {
    let path = find_dir_manifest(dir)?;
    let file = File::open(&path)?;
    read_manifest(&path, file)
}

fn find_dir_manifest(dir: &Path) -> Result<PathBuf, VfsError> {
    let entries = std::fs::read_dir(dir)?;
    let mut found = None;
    for entry in entries {
        let entry = entry?;
        if entry.file_name().to_str() != Some(MANIFEST_FILE) {
            continue;
        }
        if found.is_some() {
            return Err(VfsError::Manifest {
                path: dir.to_path_buf(),
                detail: format!("multiple {MANIFEST_FILE} entries at the mod root"),
            });
        }
        found = Some(entry.path());
    }
    found.ok_or_else(|| VfsError::MissingManifest {
        path: dir.to_path_buf(),
    })
}
