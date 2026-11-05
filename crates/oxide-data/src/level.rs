use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A V-Slice story-mode level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LevelData {
    #[serde(rename = "version")]
    pub version: String,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "titleAsset", skip_serializing_if = "Option::is_none")]
    pub title_asset: Option<String>,
    #[serde(rename = "background", skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(rename = "capsule", skip_serializing_if = "Option::is_none")]
    pub capsule: Option<serde_json::Value>,
    #[serde(rename = "props", default)]
    pub props: Vec<LevelProp>,
    #[serde(rename = "songs", default)]
    pub songs: Vec<LevelSong>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LevelProp {
    #[serde(rename = "assetPath")]
    pub asset_path: String,
    #[serde(rename = "scale", skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(rename = "offsets", skip_serializing_if = "Option::is_none")]
    pub offsets: Option<(f64, f64)>,
    #[serde(rename = "animations", default)]
    pub animations: Vec<crate::character::AnimationData>,
    #[serde(rename = "danceEvery", skip_serializing_if = "Option::is_none")]
    pub dance_every: Option<f64>,
    #[serde(rename = "startingAnimation", skip_serializing_if = "Option::is_none")]
    pub starting_animation: Option<String>,
}

/// A song entry in a level.
///
/// The frozen contract models an object (`songId`, optional `difficulties`),
/// but the shipped V-Slice level files store a bare song id string. Both forms
/// are accepted, and the original form is preserved on serialization so real
/// level files round-trip unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelSong {
    pub song_id: String,
    pub difficulties: Option<Vec<String>>,
}

impl Serialize for LevelSong {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.difficulties {
            Some(difficulties) => {
                let mut object = serializer.serialize_struct("LevelSong", 2)?;
                object.serialize_field("songId", &self.song_id)?;
                object.serialize_field("difficulties", difficulties)?;
                object.end()
            }
            None => serializer.serialize_str(&self.song_id),
        }
    }
}

impl<'de> Deserialize<'de> for LevelSong {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ObjectForm {
            #[serde(rename = "songId")]
            song_id: String,
            #[serde(rename = "difficulties", default)]
            difficulties: Option<Vec<String>>,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Form {
            SongId(String),
            Object(ObjectForm),
        }

        Ok(match Form::deserialize(deserializer)? {
            Form::SongId(song_id) => Self {
                song_id,
                difficulties: None,
            },
            Form::Object(object) => Self {
                song_id: object.song_id,
                difficulties: object.difficulties,
            },
        })
    }
}
