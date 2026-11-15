// Sserafim stage script (ported from sserafim.hxc to oxide-engine Rhai)
// Note: Sserafim shader uniforms and custom events are handled natively by StageShaderRuntime.

fn reset_sserafim_props() {
    prop_alpha("solidCover", 0.0);
    prop_alpha("truckLight1", 0.0);
    prop_alpha("truckLight2", 0.0);
    prop_alpha("backLightColor", 0.0);
    prop_alpha("backLightWhite", 0.0);
}

fn onCreate() {
    reset_sserafim_props();
}

fn onSongRetry() {
    reset_sserafim_props();
}
