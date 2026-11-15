use std::collections::BTreeMap;

use rhai::Dynamic;
use serde::{Deserialize, Serialize};

use crate::utils::constants::{SCRIPT_EXT_LUA, SCRIPT_EXT_RHAI, SCRIPT_EXT_WASM};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScriptLanguage {
    Rhai,
    Lua,
    Wasm,
    Custom(String),
}

impl ScriptLanguage {
    pub fn from_extension(ext: &str) -> Option<Self> {
        let lower = ext.trim().trim_start_matches('.').to_ascii_lowercase();
        match lower.as_str() {
            SCRIPT_EXT_RHAI => Some(Self::Rhai),
            SCRIPT_EXT_LUA => Some(Self::Lua),
            SCRIPT_EXT_WASM => Some(Self::Wasm),
            "" => None,
            other => Some(Self::Custom(other.to_string())),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Rhai => SCRIPT_EXT_RHAI,
            Self::Lua => SCRIPT_EXT_LUA,
            Self::Wasm => SCRIPT_EXT_WASM,
            Self::Custom(name) => name.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(untagged)]
pub enum ScriptValue {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<ScriptValue>),
    Map(BTreeMap<String, ScriptValue>),
}

impl ScriptValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(v) => Some(*v),
            Self::Float(v) => Some(*v as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(v) => Some(*v),
            Self::Int(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Null => serde_json::Value::Null,
            Self::Bool(v) => serde_json::Value::Bool(*v),
            Self::Int(v) => serde_json::json!(*v),
            Self::Float(v) => serde_json::Number::from_f64(*v)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Self::String(s) => serde_json::Value::String(s.clone()),
            Self::List(items) => serde_json::Value::Array(items.iter().map(Self::to_json).collect()),
            Self::Map(map) => {
                let mut obj = serde_json::Map::new();
                for (k, v) in map {
                    obj.insert(k.clone(), v.to_json());
                }
                serde_json::Value::Object(obj)
            }
        }
    }

    pub fn from_json(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(b) => Self::Bool(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Self::Int(i)
                } else {
                    Self::Float(n.as_f64().unwrap_or(0.0))
                }
            }
            serde_json::Value::String(s) => Self::String(s.clone()),
            serde_json::Value::Array(arr) => Self::List(arr.iter().map(Self::from_json).collect()),
            serde_json::Value::Object(obj) => Self::Map(
                obj.iter()
                    .map(|(k, v)| (k.clone(), Self::from_json(v)))
                    .collect(),
            ),
        }
    }

    pub fn from_rhai(dynamic: &Dynamic) -> Self {
        if dynamic.is_unit() {
            return Self::Null;
        }
        if let Ok(v) = dynamic.as_bool() {
            return Self::Bool(v);
        }
        if let Ok(v) = dynamic.as_int() {
            return Self::Int(v);
        }
        if let Ok(v) = dynamic.as_float() {
            return Self::Float(v);
        }
        if let Ok(s) = dynamic.clone().into_string() {
            return Self::String(s);
        }
        if let Some(arr) = dynamic.read_lock::<rhai::Array>() {
            return Self::List(arr.iter().map(Self::from_rhai).collect());
        }
        if let Some(map) = dynamic.read_lock::<rhai::Map>() {
            return Self::Map(
                map.iter()
                    .map(|(k, v)| (k.to_string(), Self::from_rhai(v)))
                    .collect(),
            );
        }
        Self::String(dynamic.to_string())
    }

    pub fn into_rhai(self) -> Dynamic {
        match self {
            Self::Null => Dynamic::UNIT,
            Self::Bool(v) => Dynamic::from(v),
            Self::Int(v) => Dynamic::from(v),
            Self::Float(v) => Dynamic::from(v),
            Self::String(s) => Dynamic::from(s),
            Self::List(items) => {
                let arr: rhai::Array = items.into_iter().map(Self::into_rhai).collect();
                Dynamic::from_array(arr)
            }
            Self::Map(map) => {
                let mut rhai_map = rhai::Map::new();
                for (k, v) in map {
                    rhai_map.insert(k.into(), v.into_rhai());
                }
                Dynamic::from_map(rhai_map)
            }
        }
    }
}

impl From<bool> for ScriptValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for ScriptValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<i32> for ScriptValue {
    fn from(value: i32) -> Self {
        Self::Int(i64::from(value))
    }
}

impl From<f64> for ScriptValue {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<String> for ScriptValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for ScriptValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}
