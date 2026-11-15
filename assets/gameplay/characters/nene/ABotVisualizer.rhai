// A-Bot visualizer bar configuration and helper state (ported from ABotVisualizer.hxc)

fn viz_bar_count() {
    7
}

fn viz_asset_path() {
    "gameplay/characters/nene/abot-viz"
}

fn viz_offsets() {
    [50.0, 100.0, 150.0, 200.0, 250.0, 300.0, 350.0]
}

fn new_viz_state(is_pixel) {
    #{
        active: false,
        alpha: 0.0,
        is_pixel: is_pixel,
        asset: if is_pixel { "gameplay/characters/nene-pixel/abot-pixel/abot-pixel-viz" } else { "gameplay/characters/nene/abot-viz" }
    }
}

fn start_viz(state) {
    state.active = true;
    state.alpha = 1.0;
    state
}

fn reset_viz(state) {
    state.active = false;
    state.alpha = 0.0;
    state
}
