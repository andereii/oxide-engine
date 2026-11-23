use std::collections::BTreeMap;

use crate::scripting::value::ScriptValue;

use super::context::ScriptContext;
use super::{camera_hud, core, gameplay, notes, shaders, stage, text_tweens};

/// Language-agnostic signature for every scripting API function.
pub type ScriptApiFn = fn(&mut ScriptContext, &[ScriptValue]) -> Result<ScriptValue, String>;

/// Authoritative runtime binding descriptor shared by all scripting backends.
#[derive(Clone, Copy)]
pub struct ScriptApiEntry {
    pub names: &'static [&'static str],
    pub arities: &'static [usize],
    pub handler: ScriptApiFn,
}

impl ScriptApiEntry {
    pub const fn new(
        names: &'static [&'static str],
        arities: &'static [usize],
        handler: ScriptApiFn,
    ) -> Self {
        Self {
            names,
            arities,
            handler,
        }
    }
}

/// Collects every scripting API binding across all engine domains.
/// Backends iterate this single list instead of duplicating function implementations.
pub fn collect_api_entries() -> Vec<ScriptApiEntry> {
    let mut entries = Vec::with_capacity(280);
    entries.extend(core::api_entries());
    entries.extend(gameplay::api_entries());
    entries.extend(camera_hud::api_entries());
    entries.extend(stage::api_entries());
    entries.extend(notes::api_entries());
    entries.extend(text_tweens::api_entries());
    entries.extend(shaders::api_entries());
    entries
}

pub(crate) fn arg(args: &[ScriptValue], idx: usize) -> Result<&ScriptValue, String> {
    args.get(idx)
        .ok_or_else(|| format!("missing argument {}", idx + 1))
}

pub(crate) fn arg_str<'a>(args: &'a [ScriptValue], idx: usize) -> Result<&'a str, String> {
    let val = arg(args, idx)?;
    val.as_str().ok_or_else(|| {
        format!(
            "expected string for argument {}, got {}",
            idx + 1,
            val.type_name()
        )
    })
}

pub(crate) fn arg_bool(args: &[ScriptValue], idx: usize) -> Result<bool, String> {
    let val = arg(args, idx)?;
    val.as_bool().ok_or_else(|| {
        format!(
            "expected bool for argument {}, got {}",
            idx + 1,
            val.type_name()
        )
    })
}

pub(crate) fn val_f64(val: &ScriptValue) -> Result<f64, String> {
    val.as_f64()
        .ok_or_else(|| format!("expected numeric value, got {}", val.type_name()))
}

pub(crate) fn arg_f64(args: &[ScriptValue], idx: usize) -> Result<f64, String> {
    val_f64(arg(args, idx)?)
}

pub(crate) fn arg_f32(args: &[ScriptValue], idx: usize) -> Result<f32, String> {
    Ok(arg_f64(args, idx)? as f32)
}

pub(crate) fn val_i64(val: &ScriptValue) -> Result<i64, String> {
    val.as_i64()
        .ok_or_else(|| format!("expected integer value, got {}", val.type_name()))
}

pub(crate) fn arg_i64(args: &[ScriptValue], idx: usize) -> Result<i64, String> {
    val_i64(arg(args, idx)?)
}

pub(crate) fn val_usize(val: &ScriptValue) -> Result<usize, String> {
    let i = val_i64(val)?;
    usize::try_from(i).map_err(|_| format!("expected non-negative index, got {i}"))
}

pub(crate) fn arg_usize(args: &[ScriptValue], idx: usize) -> Result<usize, String> {
    val_usize(arg(args, idx)?)
}

pub(crate) fn arg_str_or_i64(args: &[ScriptValue], idx: usize) -> Result<String, String> {
    let val = arg(args, idx)?;
    if let Some(s) = val.as_str() {
        Ok(s.to_string())
    } else {
        Ok(val_i64(val)?.to_string())
    }
}

pub(crate) fn arg_list<'a>(
    args: &'a [ScriptValue],
    idx: usize,
) -> Result<&'a [ScriptValue], String> {
    let val = arg(args, idx)?;
    val.as_list().ok_or_else(|| {
        format!(
            "expected array for argument {}, got {}",
            idx + 1,
            val.type_name()
        )
    })
}

pub(crate) fn arg_map<'a>(
    args: &'a [ScriptValue],
    idx: usize,
) -> Result<&'a BTreeMap<String, ScriptValue>, String> {
    let val = arg(args, idx)?;
    val.as_map().ok_or_else(|| {
        format!(
            "expected object map for argument {}, got {}",
            idx + 1,
            val.type_name()
        )
    })
}

pub(crate) fn vec2_f32_list(pair: [f32; 2]) -> ScriptValue {
    ScriptValue::List(vec![
        ScriptValue::Float(f64::from(pair[0])),
        ScriptValue::Float(f64::from(pair[1])),
    ])
}

pub(crate) fn vec2_f64_list(pair: [f64; 2]) -> ScriptValue {
    ScriptValue::List(vec![
        ScriptValue::Float(pair[0]),
        ScriptValue::Float(pair[1]),
    ])
}

pub(crate) fn vec2_bool_list(pair: [bool; 2]) -> ScriptValue {
    ScriptValue::List(vec![
        ScriptValue::Bool(pair[0]),
        ScriptValue::Bool(pair[1]),
    ])
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use super::{collect_api_entries, val_f64, val_i64, val_usize};
    use crate::scripting::value::ScriptValue;

    #[test]
    fn api_entries_have_unique_name_arity_pairs_and_supported_arities() {
        let entries = collect_api_entries();
        assert!(!entries.is_empty());

        let mut seen: HashSet<(&'static str, usize)> = HashSet::new();
        for entry in &entries {
            assert!(!entry.names.is_empty());
            assert!(!entry.arities.is_empty());
            for &name in entry.names {
                for &arity in entry.arities {
                    assert!(
                        arity <= 12,
                        "arity {arity} for '{name}' exceeds Rhai binder limit 12"
                    );
                    assert!(
                        seen.insert((name, arity)),
                        "duplicate script API binding ({name}, {arity})"
                    );
                }
            }
        }
        assert_eq!(seen.len(), 409);
    }

    #[test]
    fn script_value_coercion_and_display_match_backend_contract() {
        assert_eq!(ScriptValue::Null.to_display_string(), "");
        assert_eq!(ScriptValue::Bool(true).to_display_string(), "true");
        assert_eq!(ScriptValue::Int(42).to_display_string(), "42");
        assert_eq!(ScriptValue::String("ok".into()).to_display_string(), "ok");

        assert_eq!(val_f64(&ScriptValue::Int(7)).unwrap(), 7.0);
        assert_eq!(val_i64(&ScriptValue::Float(3.9)).unwrap(), 3);
        assert_eq!(val_usize(&ScriptValue::Int(2)).unwrap(), 2);
        assert!(val_usize(&ScriptValue::Int(-1)).is_err());

        let mut map = BTreeMap::new();
        map.insert("k".to_string(), ScriptValue::List(vec![ScriptValue::Int(1)]));
        let json = ScriptValue::Map(map).to_json();
        assert_eq!(ScriptValue::from_json(&json).to_json(), json);
    }
}
