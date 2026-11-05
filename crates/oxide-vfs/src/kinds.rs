//! Kind-to-relative-path mapping with the extension fallbacks from `funkin.Paths`.

use crate::ResKind;

impl ResKind {
    /// Ordered candidate relative paths for `key`, with extension fallbacks.
    ///
    /// `Inst`, `Voices` and `Chart` lowercase only the song id (the segment
    /// before the first `/`), keeping the remainder verbatim.
    pub fn candidates(&self, key: &str) -> Vec<String> {
        match self {
            Self::Image => with_extensions("images/", key, &["png"]),
            Self::Sound => with_extensions("sounds/", key, &["ogg", "mp3"]),
            Self::Music => with_extensions("music/", key, &["ogg", "mp3"]),
            Self::Inst => {
                let song = lowercase_song(key);
                with_extensions(&format!("songs/{song}/"), "Inst", &["ogg", "mp3"])
            }
            Self::Voices => {
                let song = lowercase_song(key);
                with_extensions(&format!("songs/{song}/"), "Voices", &["ogg", "mp3"])
            }
            Self::Chart => {
                let key = lowercase_song(key);
                with_extensions("data/songs/", &key, &["json"])
            }
            Self::Json => with_extensions("data/", key, &["json"]),
            Self::Frag => with_extensions("shaders/", key, &["frag"]),
        }
    }
}

fn with_extensions(prefix: &str, key: &str, extensions: &[&str]) -> Vec<String> {
    extensions
        .iter()
        .map(|extension| format!("{prefix}{key}.{extension}"))
        .collect()
}

/// Lowercases only the song id, i.e. the segment before the first `/`.
fn lowercase_song(key: &str) -> String {
    match key.split_once('/') {
        Some((song, remainder)) => format!("{}/{remainder}", song.to_lowercase()),
        None => key.to_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_apply_the_contract_table() {
        assert_eq!(ResKind::Image.candidates("bf"), ["images/bf.png"]);
        assert_eq!(
            ResKind::Sound.candidates("click"),
            ["sounds/click.ogg", "sounds/click.mp3"]
        );
        assert_eq!(
            ResKind::Music.candidates("bopeebo"),
            ["music/bopeebo.ogg", "music/bopeebo.mp3"]
        );
        assert_eq!(
            ResKind::Inst.candidates("Bopeebo"),
            ["songs/bopeebo/Inst.ogg", "songs/bopeebo/Inst.mp3"]
        );
        assert_eq!(
            ResKind::Voices.candidates("Bopeebo"),
            ["songs/bopeebo/Voices.ogg", "songs/bopeebo/Voices.mp3"]
        );
        assert_eq!(
            ResKind::Chart.candidates("Bopeebo/Bopeebo-chart"),
            ["data/songs/bopeebo/Bopeebo-chart.json"]
        );
        assert_eq!(
            ResKind::Chart.candidates("Bopeebo/Bopeebo-chart-Erect"),
            ["data/songs/bopeebo/Bopeebo-chart-Erect.json"]
        );
        assert_eq!(
            ResKind::Chart.candidates("Bopeebo/Bopeebo-chart-erect"),
            ["data/songs/bopeebo/Bopeebo-chart-erect.json"]
        );
        assert_eq!(ResKind::Json.candidates("ui/foo"), ["data/ui/foo.json"]);
        assert_eq!(
            ResKind::Frag.candidates("grayscale"),
            ["shaders/grayscale.frag"]
        );
    }
}
