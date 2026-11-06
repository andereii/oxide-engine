//! Deterministic `assets.pak` writer.
//!
//! Entry order is sorted by name, timestamps are the frozen 2000-01-01
//! 00:00:00, text is Deflated at a fixed level and media/fonts are Stored.

use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, System, ZipWriter};

use crate::MediaError;

pub const DIST_MARKER: &str = "assets-dist.marker";
pub(crate) const PAK_NAME: &str = "assets.pak";
const MARKER_FORMAT: &str = "oxide-assets-dist";
const MARKER_VERSION: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PakMethod {
    Stored,
    Deflated,
}

#[derive(Debug, Clone)]
pub(crate) struct PakInput {
    pub name: String,
    pub source: PathBuf,
    pub method: PakMethod,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PakOutput {
    pub bytes: u64,
    pub stored_bytes: u64,
    pub entries: u64,
}

pub(crate) fn write_pak(
    destination: &Path,
    mut inputs: Vec<PakInput>,
) -> Result<(PakOutput, String), MediaError> {
    inputs.sort_by(|left, right| left.name.cmp(&right.name));
    for pair in inputs.windows(2) {
        if pair[0].name == pair[1].name {
            return Err(MediaError::Zip(format!(
                "duplicate pak entry: {}",
                pair[0].name
            )));
        }
    }

    let fixed_time = DateTime::from_date_and_time(2000, 1, 1, 0, 0, 0)
        .map_err(|error| MediaError::Zip(format!("fixed timestamp: {error}")))?;
    let file = File::create(destination)?;
    let mut writer = ZipWriter::new(file);
    let mut stored_bytes = 0u64;
    let mut entries = 0u64;

    for input in &inputs {
        let method = match input.method {
            PakMethod::Stored => CompressionMethod::Stored,
            PakMethod::Deflated => CompressionMethod::Deflated,
        };
        // Pin the "made by" system byte so the pak is identical on every
        // host (the zip crate otherwise defaults to Dos on Windows and Unix
        // elsewhere).
        let mut options = SimpleFileOptions::default()
            .compression_method(method)
            .last_modified_time(fixed_time)
            .system(System::Unix)
            .unix_permissions(0o644);
        if input.method == PakMethod::Deflated {
            options = options.compression_level(Some(9));
        }
        writer
            .start_file(input.name.clone(), options)
            .map_err(|error| MediaError::Zip(error.to_string()))?;
        let mut source = File::open(&input.source)?;
        let copied = std::io::copy(&mut source, &mut writer)?;
        if input.method == PakMethod::Stored {
            stored_bytes += copied;
        }
        entries += 1;
    }

    writer
        .finish()
        .map_err(|error| MediaError::Zip(error.to_string()))?;

    let bytes = std::fs::metadata(destination)?.len();
    let digest = sha256_file(destination)?;
    Ok((
        PakOutput {
            bytes,
            stored_bytes,
            entries,
        },
        digest,
    ))
}

pub(crate) fn write_marker(
    out_dir: &Path,
    owned: &[String],
    pak_sha256: &str,
) -> Result<(), MediaError> {
    let mut owned = owned.to_vec();
    owned.sort();
    let value = json!({
        "format": MARKER_FORMAT,
        "version": MARKER_VERSION,
        "owned": owned,
        "pak_sha256": pak_sha256,
    });
    let mut text = serde_json::to_string(&value)
        .map_err(|error| MediaError::Verify(format!("marker JSON: {error}")))?;
    text.push('\n');
    std::fs::write(out_dir.join(DIST_MARKER), text)?;
    Ok(())
}

/// Reads the owned paths from a marker. `None` means missing, unreadable or a
/// foreign marker version; callers must refuse to touch the directory then.
pub(crate) fn read_marker_owned(out_dir: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(out_dir.join(DIST_MARKER)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    if value.get("format")?.as_str()? != MARKER_FORMAT {
        return None;
    }
    if value.get("version")?.as_u64()? != MARKER_VERSION {
        return None;
    }
    value.get("pak_sha256")?.as_str()?;
    let owned = value.get("owned")?.as_array()?;
    let mut paths = Vec::with_capacity(owned.len());
    for item in owned {
        paths.push(item.as_str()?.to_string());
    }
    Some(paths)
}

/// Deletes only the files named by a valid marker, leaving foreign content and
/// the directory itself intact. `assets.pak` is also removed defensively.
pub(crate) fn remove_owned(out_dir: &Path, owned: &[String]) -> Result<(), MediaError> {
    for rel in owned {
        let path = owned_path(out_dir, rel).ok_or_else(|| {
            MediaError::Verify(format!("marker contains an unsafe owned path: {rel}"))
        })?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => std::fs::remove_file(&path)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let pak = out_dir.join(PAK_NAME);
    match std::fs::symlink_metadata(&pak) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => std::fs::remove_file(&pak)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn owned_path(out_dir: &Path, rel: &str) -> Option<PathBuf> {
    let path = Path::new(rel);
    if path.is_absolute() {
        return None;
    }
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if relative.as_os_str().is_empty() {
        return None;
    }
    Some(out_dir.join(relative))
}

fn sha256_file(path: &Path) -> Result<String, MediaError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}
