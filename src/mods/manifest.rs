use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::utils::constants::{
    DEFAULT_MOD_API_VERSION, DEFAULT_MOD_DESCRIPTION, DEFAULT_MOD_PRIORITY, DEFAULT_MOD_VERSION,
    MOD_MANIFEST_JSON, MOD_MANIFEST_TOML, POLYMOD_META_JSON, SCRIPT_EXT_DISABLED,
};

#[derive(Debug, Error)]
pub enum ModError {
    #[error("failed to read mod path {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse TOML manifest {path}: {source}")]
    InvalidToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("failed to parse JSON manifest {path}: {source}")]
    InvalidJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid relative VFS path '{0}'")]
    InvalidPath(String),
    #[error("asset not found in VFS: '{0}'")]
    NotFound(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModAuthor {
    Name(String),
    Detailed {
        name: String,
        #[serde(default)]
        role: Option<String>,
        #[serde(default)]
        url: Option<String>,
    },
}

impl ModAuthor {
    pub fn display_name(&self) -> &str {
        match self {
            Self::Name(n) => n.as_str(),
            Self::Detailed { name, .. } => name.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModDependencyEntry {
    Simple(String),
    Detailed {
        id: String,
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        optional: bool,
    },
}

impl ModDependencyEntry {
    pub fn mod_id(&self) -> &str {
        match self {
            Self::Simple(id) => id.as_str(),
            Self::Detailed { id, .. } => id.as_str(),
        }
    }

    pub fn is_optional(&self) -> bool {
        match self {
            Self::Simple(_) => false,
            Self::Detailed { optional, .. } => *optional,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct RawModManifest {
    id: Option<String>,
    title: Option<String>,
    name: Option<String>,
    description: String,
    author: Option<String>,
    authors: Vec<ModAuthor>,
    contributors: Vec<ModAuthor>,
    version: Option<String>,
    mod_version: Option<String>,
    api_version: String,
    license: Option<String>,
    homepage: Option<String>,
    enabled: bool,
    priority: i32,
    dependencies: RawDependenciesField,
    optional_dependencies: Vec<String>,
    config: HashMap<String, serde_json::Value>,
}

impl Default for RawModManifest {
    fn default() -> Self {
        Self {
            id: None,
            title: None,
            name: None,
            description: DEFAULT_MOD_DESCRIPTION.to_string(),
            author: None,
            authors: Vec::new(),
            contributors: Vec::new(),
            version: None,
            mod_version: None,
            api_version: DEFAULT_MOD_API_VERSION.to_string(),
            license: None,
            homepage: None,
            enabled: true,
            priority: DEFAULT_MOD_PRIORITY,
            dependencies: RawDependenciesField::List(Vec::new()),
            optional_dependencies: Vec::new(),
            config: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RawDependenciesField {
    List(Vec<ModDependencyEntry>),
    Map(HashMap<String, String>),
}

impl Default for RawDependenciesField {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ModManifest {
    pub id: String,
    pub title: String,
    pub description: String,
    pub authors: Vec<String>,
    pub version: String,
    pub api_version: String,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub enabled: bool,
    pub priority: i32,
    pub dependencies: Vec<ModDependencyEntry>,
    pub config: HashMap<String, serde_json::Value>,
}

impl ModManifest {
    pub fn fallback_for_dir(dir_name: &str) -> Self {
        let id = sanitize_mod_id(dir_name);
        Self {
            id: id.clone(),
            title: dir_name.to_string(),
            description: DEFAULT_MOD_DESCRIPTION.to_string(),
            authors: Vec::new(),
            version: DEFAULT_MOD_VERSION.to_string(),
            api_version: DEFAULT_MOD_API_VERSION.to_string(),
            license: None,
            homepage: None,
            enabled: true,
            priority: DEFAULT_MOD_PRIORITY,
            dependencies: Vec::new(),
            config: HashMap::new(),
        }
    }
}

pub fn sanitize_mod_id(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() || ch == '.' {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "unnamed-mod".to_string()
    } else {
        trimmed
    }
}

fn finalize_raw_manifest(raw: RawModManifest, fallback_dir_name: &str, disabled_marker: bool) -> ModManifest {
    let fallback_id = sanitize_mod_id(fallback_dir_name);
    let id = raw
        .id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(sanitize_mod_id)
        .unwrap_or_else(|| fallback_id.clone());

    let title = raw
        .title
        .or(raw.name)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback_dir_name.to_string());

    let mut authors = Vec::new();
    if let Some(single) = raw.author
        && !single.trim().is_empty()
    {
        authors.push(single.trim().to_string());
    }
    for author in raw.authors.into_iter().chain(raw.contributors.into_iter()) {
        let name = author.display_name().trim();
        if !name.is_empty() && !authors.iter().any(|a| a == name) {
            authors.push(name.to_string());
        }
    }

    let version = raw
        .version
        .or(raw.mod_version)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_MOD_VERSION.to_string());

    let mut dependencies = match raw.dependencies {
        RawDependenciesField::List(list) => list,
        RawDependenciesField::Map(map) => {
            let mut entries: Vec<ModDependencyEntry> = map
                .into_iter()
                .map(|(dep_id, ver)| ModDependencyEntry::Detailed {
                    id: sanitize_mod_id(&dep_id),
                    version: if ver.trim().is_empty() { None } else { Some(ver) },
                    optional: false,
                })
                .collect();
            entries.sort_by(|a, b| a.mod_id().cmp(b.mod_id()));
            entries
        }
    };
    for opt in raw.optional_dependencies {
        let opt_id = sanitize_mod_id(&opt);
        if !dependencies.iter().any(|d| d.mod_id() == opt_id) {
            dependencies.push(ModDependencyEntry::Detailed {
                id: opt_id,
                version: None,
                optional: true,
            });
        }
    }

    ModManifest {
        id,
        title,
        description: raw.description,
        authors,
        version,
        api_version: raw.api_version,
        license: raw.license,
        homepage: raw.homepage,
        enabled: raw.enabled && !disabled_marker,
        priority: raw.priority,
        dependencies,
        config: raw.config,
    }
}

pub fn load_manifest_for_dir(mod_dir: &Path) -> Result<(ModManifest, Option<PathBuf>), ModError> {
    let fallback_name = mod_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed-mod");
    let disabled_marker = mod_dir.join(format!(".{SCRIPT_EXT_DISABLED}")).exists()
        || fallback_name.ends_with(&format!(".{SCRIPT_EXT_DISABLED}"));

    let toml_path = mod_dir.join(MOD_MANIFEST_TOML);
    if toml_path.is_file() {
        let raw_text = fs::read_to_string(&toml_path).map_err(|source| ModError::Io {
            path: toml_path.clone(),
            source,
        })?;
        let parsed: RawModManifest = toml::from_str(&raw_text).map_err(|source| ModError::InvalidToml {
            path: toml_path.clone(),
            source,
        })?;
        return Ok((
            finalize_raw_manifest(parsed, fallback_name, disabled_marker),
            Some(toml_path),
        ));
    }

    for json_name in [MOD_MANIFEST_JSON, POLYMOD_META_JSON] {
        let json_path = mod_dir.join(json_name);
        if json_path.is_file() {
            let raw_text = fs::read_to_string(&json_path).map_err(|source| ModError::Io {
                path: json_path.clone(),
                source,
            })?;
            let parsed: RawModManifest =
                serde_json::from_str(&raw_text).map_err(|source| ModError::InvalidJson {
                    path: json_path.clone(),
                    source,
                })?;
            return Ok((
                finalize_raw_manifest(parsed, fallback_name, disabled_marker),
                Some(json_path),
            ));
        }
    }

    let mut manifest = ModManifest::fallback_for_dir(fallback_name);
    if disabled_marker {
        manifest.enabled = false;
    }
    Ok((manifest, None))
}
