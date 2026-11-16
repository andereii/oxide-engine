use crate::play::types::SongEventData;

use super::super::defs::WiggleEffectType;
use super::StageShaderRuntime;

pub(super) fn handle_song_shader_event(event: &SongEventData, runtime: &mut StageShaderRuntime) {
    let (name, params_root, params_list) = unwrap_custom_event(event);
    match name {
        "EnableMask" | "enableMask" => {
            runtime.tankman_alt_mask = true;
        }
        "sserafimDark" => {
            let amount = read_param_f32(params_root, params_list, "amount").unwrap_or(0.0);
            let duration = read_param_f64(params_root, params_list, "duration").unwrap_or(0.0);
            runtime.sserafim.start_darken(amount, duration);
        }
        "sserafimLights" => {
            let amount = read_param_f32(params_root, params_list, "amount").unwrap_or(0.0);
            let duration = read_param_f64(params_root, params_list, "duration").unwrap_or(0.0);
            runtime.sserafim.flash_truck_lights(amount, duration);
        }
        "sserafimPulseLights" => {
            runtime.sserafim.lights_enabled =
                read_param_bool(params_root, params_list, "enabled").unwrap_or(false);
            if let Some(colors) = read_param_string_array(params_root, params_list, "colors") {
                runtime.sserafim.pulse_colors =
                    colors.iter().map(|s| parse_shader_color_rgb(s)).collect();
            }
            if let Some(durs) = read_param_f32_array(params_root, params_list, "durations") {
                runtime.sserafim.pulse_durations = durs;
            }
            if let Some(ints) = read_param_f32_array(params_root, params_list, "intensities") {
                runtime.sserafim.pulse_intensities = ints;
            }
        }
        "sserafimKick" => {
            if read_param_bool(params_root, params_list, "final").unwrap_or(false) {
                runtime.sserafim.start_clear();
            }
        }
        _ => {}
    }
}

fn unwrap_custom_event(
    event: &SongEventData,
) -> (&str, &serde_json::Value, Option<&Vec<serde_json::Value>>) {
    if event.kind.eq_ignore_ascii_case("CustomEvent")
        && let Some(inner_name) = event.raw_value.get("name").and_then(|v| v.as_str())
    {
        let params = event.raw_value.get("params").and_then(|v| v.as_array());
        return (inner_name, &event.raw_value, params);
    }
    (&event.kind, &event.raw_value, None)
}

fn find_param<'a>(
    root: &'a serde_json::Value,
    params: Option<&'a Vec<serde_json::Value>>,
    key: &str,
) -> Option<&'a serde_json::Value> {
    if let Some(list) = params {
        for entry in list {
            if entry.get("name").and_then(|v| v.as_str()) == Some(key)
                && let Some(val) = entry.get("value")
            {
                return Some(val);
            }
        }
    }
    root.get(key)
}

fn read_param_f32(
    root: &serde_json::Value,
    params: Option<&Vec<serde_json::Value>>,
    key: &str,
) -> Option<f32> {
    let val = find_param(root, params, key)?;
    val.as_f64()
        .map(|n| n as f32)
        .or_else(|| val.as_str().and_then(|s| s.trim().parse::<f32>().ok()))
}

fn read_param_f64(
    root: &serde_json::Value,
    params: Option<&Vec<serde_json::Value>>,
    key: &str,
) -> Option<f64> {
    let val = find_param(root, params, key)?;
    val.as_f64()
        .or_else(|| val.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
}

fn read_param_bool(
    root: &serde_json::Value,
    params: Option<&Vec<serde_json::Value>>,
    key: &str,
) -> Option<bool> {
    let val = find_param(root, params, key)?;
    val.as_bool().or_else(|| {
        val.as_str()
            .and_then(|s| match s.trim().to_ascii_lowercase().as_str() {
                "true" | "1" => Some(true),
                "false" | "0" => Some(false),
                _ => None,
            })
    })
}

fn read_param_string_array(
    root: &serde_json::Value,
    params: Option<&Vec<serde_json::Value>>,
    key: &str,
) -> Option<Vec<String>> {
    let val = find_param(root, params, key)?;
    if let Some(arr) = val.as_array() {
        return Some(
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
        );
    }
    if let Some(s) = val.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Vec<String>>(s) {
            return Some(parsed);
        }
        return Some(
            s.split(',')
                .map(|part| part.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect(),
        );
    }
    None
}

fn read_param_f32_array(
    root: &serde_json::Value,
    params: Option<&Vec<serde_json::Value>>,
    key: &str,
) -> Option<Vec<f32>> {
    let val = find_param(root, params, key)?;
    if let Some(arr) = val.as_array() {
        return Some(
            arr.iter()
                .filter_map(|v| v.as_f64().map(|n| n as f32))
                .collect(),
        );
    }
    if let Some(s) = val.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Vec<f32>>(s) {
            return Some(parsed);
        }
        return Some(
            s.split(',')
                .filter_map(|part| part.trim().parse::<f32>().ok())
                .collect(),
        );
    }
    None
}

pub fn parse_wiggle_effect_type(name: &str) -> WiggleEffectType {
    match name.trim().to_ascii_lowercase().as_str() {
        "wavy" | "1" => WiggleEffectType::Wavy,
        "heat_wave_horizontal" | "heatwavehorizontal" | "heat_h" | "2" => {
            WiggleEffectType::HeatWaveHorizontal
        }
        "heat_wave_vertical" | "heatwavevertical" | "heat_v" | "3" => {
            WiggleEffectType::HeatWaveVertical
        }
        "flag" | "4" => WiggleEffectType::Flag,
        _ => WiggleEffectType::Dreamy,
    }
}

pub fn parse_shader_color_rgb(hex: &str) -> [f32; 3] {
    let [r, g, b, _] = crate::scripting::api::parse_hex_rgba(hex);
    [r, g, b]
}
