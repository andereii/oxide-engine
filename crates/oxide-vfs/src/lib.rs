//! Layered mod filesystem with case-exact asset resolution.
//!
//! A [`Vfs`] is a base assets directory plus any number of mod layers. Mods are
//! resolved from last-added to first, then the base directory (Polymod
//! precedence, ADR-0008). Directory layers never rely on the platform's
//! case-insensitive path lookup: every component is matched against names read
//! from `read_dir`, so resolution behaves the same on Linux, macOS and Windows.
//!
//! Assets are addressed with `/`-separated relative paths. [`Vfs::resolve`]
//! resolves an exact path; [`Vfs::resolve_kind`] applies the game's kind
//! mapping and extension fallbacks (for example sound `ogg` before `mp3`).

mod kinds;
mod layers;
mod manifest;
mod watch;
mod zip;

use std::io::Read;
use std::path::{Path, PathBuf};

use thiserror::Error;

pub use manifest::{Contributor, Manifest};
pub use watch::VfsWatcher;

/// Errors returned by VFS operations.
#[derive(Debug, Error)]
pub enum VfsError {
    #[error("manifest {path:?} is invalid: {detail}")]
    Manifest { path: PathBuf, detail: String },
    #[error("zip {path:?} has an unsafe entry {entry:?} (absolute or contains '..')")]
    UnsafeZipEntry { path: PathBuf, entry: String },
    #[error("duplicate mod id '{id}'")]
    DuplicateMod { id: String },
    #[error("mod {path:?} has no _polymod_meta.json at its root")]
    MissingManifest { path: PathBuf },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(String),
}

/// Asset kinds with the path mapping used by the game (`funkin.Paths`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResKind {
    Image,
    Sound,
    Music,
    Inst,
    Voices,
    Chart,
    Json,
    Frag,
}

/// Where a resolved asset lives.
#[derive(Debug, Clone)]
pub enum Source {
    /// Absolute concrete file path inside a directory layer.
    Dir(PathBuf),
    /// Entry inside a zip layer.
    Zip { path: PathBuf, entry: String },
}

/// A layered view over a base asset directory and its mods.
pub struct Vfs {
    base_dir: PathBuf,
    base_index: layers::DirIndex,
    mods: Vec<layers::Layer>,
    mod_ids: Vec<String>,
}

impl Vfs {
    /// Creates a VFS over `base_dir`. The directory does not need to exist yet.
    ///
    /// An existing base is canonicalized; a missing one is made absolute
    /// without requiring existence, since this constructor cannot report I/O
    /// errors.
    pub fn new(base_dir: PathBuf) -> Self {
        let base_dir = match std::fs::canonicalize(&base_dir) {
            Ok(path) => path,
            Err(_) => std::path::absolute(&base_dir).unwrap_or(base_dir),
        };
        Self {
            base_dir,
            base_index: layers::DirIndex::default(),
            mods: Vec::new(),
            mod_ids: Vec::new(),
        }
    }

    /// Adds a directory mod. `dir` must contain `_polymod_meta.json` at its root.
    pub fn add_mod_dir(&mut self, dir: PathBuf, id: String) -> Result<(), VfsError> {
        self.reject_duplicate(&id)?;
        let dir = std::fs::canonicalize(&dir)?;
        manifest::read_dir_manifest(&dir)?;
        self.mod_ids.push(id);
        self.mods.push(layers::Layer::new_dir(dir));
        Ok(())
    }

    /// Adds a zip mod. The archive must contain `_polymod_meta.json` at its root.
    ///
    /// All entries are checked eagerly; absolute names and names with a `..`
    /// component are rejected with [`VfsError::UnsafeZipEntry`].
    pub fn add_mod_zip(&mut self, zip_path: PathBuf, id: String) -> Result<(), VfsError> {
        self.reject_duplicate(&id)?;
        let zip_path = std::fs::canonicalize(&zip_path)?;
        let entries = zip::load_entries(&zip_path)?;
        zip::read_manifest(&zip_path)?;
        self.mod_ids.push(id);
        self.mods.push(layers::Layer::new_zip(zip_path, entries));
        Ok(())
    }

    /// Ids in the order the mods were added.
    pub fn mod_ids(&self) -> &[String] {
        &self.mod_ids
    }

    /// Resolves an exact relative path, last-added mod first, then the base.
    pub fn resolve(&self, rel: &str) -> Option<Source> {
        for layer in self.mods.iter().rev() {
            if let Some(source) = layer.resolve(rel) {
                return Some(source);
            }
        }
        self.base_index
            .resolve(&self.base_dir, rel)
            .map(Source::Dir)
    }

    /// Maps a kind key to a relative path and resolves it case-exactly.
    ///
    /// Extension candidates are tried in order across every layer: the first
    /// extension that exists anywhere wins, and fallbacks are only attempted on
    /// a miss.
    pub fn resolve_kind(&self, kind: ResKind, key: &str) -> Option<Source> {
        kind.candidates(key)
            .into_iter()
            .find_map(|candidate| self.resolve(&candidate))
    }

    /// Reads a resolved source. Returns an I/O error when content exceeds
    /// `size_limit` bytes (the message is `resource too large`).
    pub fn read(&self, src: &Source, size_limit: u64) -> Result<Vec<u8>, VfsError> {
        match src {
            Source::Dir(path) => {
                let file = std::fs::File::open(path)?;
                if file.metadata()?.len() > size_limit {
                    return Err(too_large());
                }
                read_limited(file, size_limit)
            }
            Source::Zip { path, entry } => zip::read_entry(path, entry, size_limit),
        }
    }

    /// Drops the cached directory state affected by this path.
    ///
    /// The path is grounded against the filesystem first (canonicalized, or
    /// resolved to its nearest existing ancestor), so symlinked and
    /// platform-prefixed event paths hit the same canonical cache keys. File
    /// events purge the grounded parent listing; directory events purge the
    /// parent listing plus the directory and its cached descendants; missing
    /// paths purge the grounded parent's cached subtree. File contents are
    /// re-read on every [`read`](Self::read); names are always re-derived by
    /// `read_dir` and event casing is never inserted into the index.
    pub fn invalidate(&self, path: &Path) {
        let grounded = layers::GroundedPath::new(path);
        self.base_index.invalidate(&grounded);
        for layer in &self.mods {
            layer.invalidate(path, &grounded);
        }
    }

    fn reject_duplicate(&self, id: &str) -> Result<(), VfsError> {
        if self.mod_ids.iter().any(|known| known == id) {
            Err(VfsError::DuplicateMod { id: id.to_owned() })
        } else {
            Ok(())
        }
    }
}

const TOO_LARGE_MESSAGE: &str = "resource too large";

fn too_large() -> VfsError {
    VfsError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        TOO_LARGE_MESSAGE,
    ))
}

pub(crate) fn is_too_large(err: &VfsError) -> bool {
    matches!(
        err,
        VfsError::Io(io)
            if io.kind() == std::io::ErrorKind::InvalidData
                && io.to_string() == TOO_LARGE_MESSAGE
    )
}

pub(crate) fn read_limited(mut reader: impl Read, size_limit: u64) -> Result<Vec<u8>, VfsError> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(size_limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > size_limit {
        return Err(too_large());
    }
    Ok(bytes)
}
