//! Asset classification and deterministic plan construction.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{AudioLayer, Disposition, EntryKind, MediaError, Plan, PlanEntry};

const TEXT_EXTENSIONS: [&str; 9] = [
    "json", "xml", "txt", "rb", "frag", "fnt", "md", "yml", "srt",
];

impl Plan {
    /// Builds the plan for `root`: walks every regular file, classifies it by
    /// its relative path and sorts by that path. The walk order never leaks
    /// into the result.
    pub fn build(root: &Path) -> Result<Plan, MediaError> {
        if !root.is_dir() {
            return Err(MediaError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("assets directory not found: {}", root.display()),
            )));
        }

        let mut files = Vec::new();
        collect_files(root, root, &mut files)?;
        if files.is_empty() {
            return Err(MediaError::Verify(format!(
                "empty source corpus: {}",
                root.display()
            )));
        }
        files.sort_by(|left, right| left.rel.cmp(&right.rel));

        let mut entries = Vec::with_capacity(files.len());
        for collected in files {
            let (kind, disposition) = if collected.skip {
                (EntryKind::Skip, Disposition::Skip)
            } else {
                classify(&collected.rel).map_err(|ext| MediaError::Unknown {
                    path: collected.path.clone(),
                    ext,
                })?
            };
            entries.push(PlanEntry {
                rel: collected.rel,
                kind,
                disposition,
            });
        }
        Ok(Plan { entries })
    }
}

/// A planned Voices stem and its matching Inst, if present. Paths are relative
/// to the assets root; an unpaired voice is valid in an override-only mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoicePairing<'a> {
    pub voices: &'a str,
    pub instrumental: Option<&'a str>,
}

/// Resolves classified song stems without reading or converting any files.
/// Selects the longest matching Inst variant suffix within the same song,
/// falling back to the unsuffixed Inst. Every Voices stem gets a result,
/// including those with no compatible instrumental.
pub fn voice_pairings<'a>(
    entries: impl IntoIterator<Item = &'a PlanEntry>,
) -> Vec<VoicePairing<'a>> {
    #[derive(Default)]
    struct SongStems<'a> {
        inst: Vec<(&'a str, &'a str)>,
        voices: Vec<(&'a str, &'a str)>,
    }

    let mut songs: BTreeMap<&str, SongStems<'a>> = BTreeMap::new();
    for entry in entries {
        let prefix = match entry.kind {
            EntryKind::Audio {
                layer: AudioLayer::Inst,
            } => "Inst",
            EntryKind::Audio {
                layer: AudioLayer::Voices,
            } => "Voices",
            _ => continue,
        };
        let parts: Vec<&str> = entry.rel.split('/').collect();
        if parts.len() != 3 {
            continue;
        }
        let stem = parts[2]
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(parts[2]);
        let Some(variant) = stem.strip_prefix(prefix) else {
            continue;
        };
        let slot = songs.entry(parts[1]).or_default();
        let stem = (entry.rel.as_str(), variant);
        if prefix == "Inst" {
            slot.inst.push(stem);
        } else {
            slot.voices.push(stem);
        }
    }

    let mut pairings = Vec::new();
    for stems in songs.into_values() {
        for (voices, variant) in stems.voices {
            let instrumental = stems
                .inst
                .iter()
                .filter(|(_, inst_variant)| {
                    inst_variant.is_empty()
                        || variant == *inst_variant
                        || variant.ends_with(*inst_variant)
                })
                .max_by_key(|(_, inst_variant)| inst_variant.len())
                .map(|(rel, _)| *rel);
            pairings.push(VoicePairing {
                voices,
                instrumental,
            });
        }
    }
    pairings
}

struct Collected {
    path: PathBuf,
    rel: String,
    skip: bool,
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<Collected>) -> std::io::Result<()> {
    let mut children: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let path = child.path();
        let file_type = child.file_type()?;
        let relative = path.strip_prefix(root).map_err(std::io::Error::other)?;
        let rel = relative_path(relative);
        if file_type.is_symlink() {
            out.push(Collected {
                path,
                rel,
                skip: true,
            });
        } else if file_type.is_dir() {
            if child.file_name() == ".git" {
                out.push(Collected {
                    path,
                    rel,
                    skip: true,
                });
            } else {
                collect_files(root, &path, out)?;
            }
        } else if file_type.is_file() {
            out.push(Collected {
                path,
                rel,
                skip: false,
            });
        } else {
            out.push(Collected {
                path,
                rel,
                skip: true,
            });
        }
    }
    Ok(())
}

fn relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Pure path/extension classification. `Err` carries the offending extension
/// so `Plan::build` can name the absolute path in the diagnostic.
fn classify(rel: &str) -> Result<(EntryKind, Disposition), String> {
    let file_name = rel.rsplit('/').next().unwrap_or(rel);
    if matches!(file_name, ".git" | ".gitignore" | ".gitattributes") {
        return Ok((EntryKind::Skip, Disposition::Skip));
    }

    let ext = file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "fla" => Ok((EntryKind::Skip, Disposition::Skip)),
        "mp3" => Ok((
            EntryKind::Audio {
                layer: AudioLayer::Sfx,
            },
            Disposition::DropMp3,
        )),
        "ogg" => Ok((
            EntryKind::Audio {
                layer: audio_layer(rel),
            },
            Disposition::ConvertAudio,
        )),
        "png" | "jpg" | "jpeg" => Ok((EntryKind::Image, Disposition::ConvertImage)),
        "mp4" | "mkv" | "webm" => {
            if !rel.starts_with("videos/") {
                return Err(ext);
            }
            let stem = file_name
                .rsplit_once('.')
                .map(|(stem, _)| stem)
                .unwrap_or(file_name);
            if crate::EXCLUDED_VIDEOS.contains(&stem) {
                Ok((EntryKind::Video, Disposition::ExcludeVideo))
            } else {
                Ok((EntryKind::Video, Disposition::ConvertVideo))
            }
        }
        "hxc" => Ok((EntryKind::Text, Disposition::ExcludeHxc)),
        "ttf" | "otf" => Ok((EntryKind::Font, Disposition::CopyFont)),
        other if TEXT_EXTENSIONS.contains(&other) => Ok((EntryKind::Text, Disposition::TextOnly)),
        _ => Err(ext),
    }
}

fn audio_layer(rel: &str) -> AudioLayer {
    let parts: Vec<&str> = rel.split('/').collect();
    if parts.len() == 3 && parts[0] == "songs" {
        let stem = parts[2]
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(parts[2]);
        if stem.starts_with("Inst") {
            return AudioLayer::Inst;
        }
        if stem.starts_with("Voices") {
            return AudioLayer::Voices;
        }
    }
    AudioLayer::Sfx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_of(rel: &str) -> (EntryKind, Disposition) {
        classify(rel).expect("fixture path must classify")
    }

    #[test]
    fn audio_layers_follow_song_layout() {
        assert_eq!(
            kind_of("songs/bopeebo/Inst.ogg"),
            (
                EntryKind::Audio {
                    layer: AudioLayer::Inst
                },
                Disposition::ConvertAudio
            )
        );
        assert_eq!(
            kind_of("songs/bopeebo/Inst-erect.ogg"),
            (
                EntryKind::Audio {
                    layer: AudioLayer::Inst
                },
                Disposition::ConvertAudio
            )
        );
        assert_eq!(
            kind_of("songs/bopeebo/Voices-bf.ogg"),
            (
                EntryKind::Audio {
                    layer: AudioLayer::Voices
                },
                Disposition::ConvertAudio
            )
        );
        assert_eq!(
            kind_of("sounds/beep.ogg"),
            (
                EntryKind::Audio {
                    layer: AudioLayer::Sfx
                },
                Disposition::ConvertAudio
            )
        );
        // An ogg inside a song directory but not named Inst/Voices is SFX.
        assert_eq!(
            kind_of("songs/bopeebo/extra.ogg"),
            (
                EntryKind::Audio {
                    layer: AudioLayer::Sfx
                },
                Disposition::ConvertAudio
            )
        );
        // Deeper nesting does not match the frozen Inst/Voices rule.
        assert_eq!(
            kind_of("songs/bopeebo/sub/Inst.ogg").0,
            EntryKind::Audio {
                layer: AudioLayer::Sfx
            }
        );
    }

    #[test]
    fn voice_pairings_share_suffix_fallback_and_song_isolation_policy() {
        let entries: Vec<_> = [
            "songs/a/Inst.ogg",
            "songs/a/Inst-erect.ogg",
            "songs/a/Inst-bf-erect.ogg",
            "songs/a/Voices-bf-erect.ogg",
            "songs/a/Voices-pico-erect.ogg",
            "songs/a/Voices-bf.ogg",
            "songs/b/Inst-erect.ogg",
            "songs/b/Voices-bf.ogg",
            "songs/override/Voices-bf.ogg",
            "songs/instrumental-only/Inst.ogg",
            "sounds/Voices-sfx.ogg",
            "songs/a/Voices-dropped.mp3",
        ]
        .into_iter()
        .map(|rel| {
            let (kind, disposition) = kind_of(rel);
            PlanEntry {
                rel: rel.to_string(),
                kind,
                disposition,
            }
        })
        .collect();
        let pairings = voice_pairings(&entries);
        assert_eq!(
            pairings,
            vec![
                VoicePairing {
                    voices: "songs/a/Voices-bf-erect.ogg",
                    instrumental: Some("songs/a/Inst-bf-erect.ogg"),
                },
                VoicePairing {
                    voices: "songs/a/Voices-pico-erect.ogg",
                    instrumental: Some("songs/a/Inst-erect.ogg"),
                },
                VoicePairing {
                    voices: "songs/a/Voices-bf.ogg",
                    instrumental: Some("songs/a/Inst.ogg"),
                },
                VoicePairing {
                    voices: "songs/b/Voices-bf.ogg",
                    instrumental: None,
                },
                VoicePairing {
                    voices: "songs/override/Voices-bf.ogg",
                    instrumental: None,
                },
            ]
        );

        let available = entries
            .iter()
            .filter(|entry| entry.rel != "songs/a/Inst-bf-erect.ogg");
        let pairings = voice_pairings(available);
        assert_eq!(pairings[0].instrumental, Some("songs/a/Inst-erect.ogg"));
    }

    #[test]
    fn mp3_is_dropped_everywhere() {
        assert_eq!(
            kind_of("songs/2hot/Inst.mp3"),
            (
                EntryKind::Audio {
                    layer: AudioLayer::Sfx
                },
                Disposition::DropMp3
            )
        );
    }

    #[test]
    fn images_and_fonts() {
        assert_eq!(
            kind_of("images/logo.png"),
            (EntryKind::Image, Disposition::ConvertImage)
        );
        assert_eq!(
            kind_of("images/logo.jpg"),
            (EntryKind::Image, Disposition::ConvertImage)
        );
        assert_eq!(
            kind_of("images/logo.JPEG"),
            (EntryKind::Image, Disposition::ConvertImage)
        );
        assert_eq!(
            kind_of("fonts/micro.ttf"),
            (EntryKind::Font, Disposition::CopyFont)
        );
        assert_eq!(
            kind_of("fonts/micro.TTF"),
            (EntryKind::Font, Disposition::CopyFont)
        );
        assert_eq!(
            kind_of("fonts/micro.otf"),
            (EntryKind::Font, Disposition::CopyFont)
        );
    }

    #[test]
    fn videos_require_videos_root_and_exact_excluded_stems() {
        assert_eq!(
            kind_of("videos/videos/tiny.mp4"),
            (EntryKind::Video, Disposition::ConvertVideo)
        );
        assert_eq!(
            kind_of("videos/videos/other.mkv"),
            (EntryKind::Video, Disposition::ConvertVideo)
        );
        assert_eq!(
            kind_of("videos/videos/boyfriendEverywhere.mp4"),
            (EntryKind::Video, Disposition::ExcludeVideo)
        );
        assert_eq!(
            kind_of("videos/videos/boyfriendEverywhereElse.mp4"),
            (EntryKind::Video, Disposition::ConvertVideo)
        );
        assert!(classify("cutscenes/tiny.mp4").is_err());
    }

    #[test]
    fn every_text_extension_is_pak_only() {
        for ext in TEXT_EXTENSIONS {
            let rel = format!("data/file.{ext}");
            assert_eq!(
                kind_of(&rel),
                (EntryKind::Text, Disposition::TextOnly),
                "{rel}"
            );
        }
    }

    #[test]
    fn hxc_is_excluded_not_packed() {
        assert_eq!(
            kind_of("script/module.hxc"),
            (EntryKind::Text, Disposition::ExcludeHxc)
        );
    }

    #[test]
    fn skip_and_unknown() {
        for rel in ["bundle.fla", ".git", ".gitignore", ".gitattributes"] {
            assert_eq!(kind_of(rel).1, Disposition::Skip, "{rel}");
        }
        for rel in ["evil.exe", "bundle.bin", "no_extension"] {
            assert!(classify(rel).is_err(), "{rel}");
        }
        assert_eq!(classify("evil.exe"), Err("exe".to_string()));
        assert_eq!(classify("no_extension"), Err(String::new()));
    }

    fn temp_root(tag: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("oxide-media-plan-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp root");
        path
    }

    #[cfg(unix)]
    #[test]
    fn plan_skips_symlinks_and_git_directories() {
        use std::os::unix::fs::symlink;

        let root = temp_root("symlinks");
        std::fs::create_dir_all(root.join("real")).expect("create real dir");
        std::fs::write(root.join("real/keep.json"), "{}").expect("write kept file");
        std::fs::write(root.join("file.json"), "{}").expect("write file");
        std::fs::create_dir(root.join(".git")).expect("create git dir");
        std::fs::write(root.join(".git/config"), "ignored").expect("write git config");
        symlink(root.join("file.json"), root.join("link.json")).expect("file symlink");
        symlink(root.join("real"), root.join("link-dir")).expect("dir symlink");

        let plan = Plan::build(&root).expect("plan");
        let rels: Vec<&str> = plan
            .entries()
            .iter()
            .map(|entry| entry.rel.as_str())
            .collect();
        assert!(rels.contains(&"file.json"));
        assert!(rels.contains(&"real/keep.json"));
        for skipped in [".git", "link.json", "link-dir"] {
            let entry = plan
                .entries()
                .iter()
                .find(|entry| entry.rel == skipped)
                .unwrap_or_else(|| panic!("missing skip entry {skipped}"));
            assert_eq!(entry.disposition, Disposition::Skip, "{skipped}");
        }
        assert!(!rels.iter().any(|rel| rel.starts_with(".git/")));
        assert!(!rels.iter().any(|rel| rel.starts_with("link-dir/")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn plan_is_sorted_and_deterministic() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/fixtures/media/mini");
        let first = Plan::build(&root).expect("plan");
        let second = Plan::build(&root).expect("plan");
        assert_eq!(first, second);
        assert!(
            first
                .entries()
                .windows(2)
                .all(|pair| pair[0].rel < pair[1].rel)
        );
        assert!(first.entries().iter().all(|entry| !entry.rel.is_empty()));
    }
}
