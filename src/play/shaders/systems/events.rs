use crate::play::types::SongEventData;

use super::super::defs::WiggleEffectType;
use super::StageShaderRuntime;

pub(super) fn handle_song_shader_event(event: &SongEventData, runtime: &mut StageShaderRuntime) {
    let name = unwrap_event_name(event);
    if matches!(name, "EnableMask" | "enableMask") {
        runtime.tankman_alt_mask = true;
    }
}

fn unwrap_event_name(event: &SongEventData) -> &str {
    if event.kind.eq_ignore_ascii_case("CustomEvent")
        && let Some(inner_name) = event.raw_value.get("name").and_then(|v| v.as_str())
    {
        return inner_name;
    }
    &event.kind
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
