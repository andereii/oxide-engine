//! Deterministic `assets.pak` writer.
//!
//! Entry order is sorted by name, timestamps are the frozen 2000-01-01
//! 00:00:00, text is Deflated at a fixed level and media/fonts are Stored.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, System, ZipWriter};

use crate::MediaError;

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
