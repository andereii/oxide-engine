// A-Bot visualizer bar spawning and animation helpers (ported from ABotVisualizer.hxc)

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

fn spawn_viz(ax, ay, gz, is_pixel) {
    let asset = if is_pixel {
        "gameplay/characters/nene-pixel/abot-pixel/abot-pixel-viz"
    } else {
        "gameplay/characters/nene/abot-viz"
    };
    let base_x = if is_pixel { ax - 162.0 } else { ax + 207.0 };
    let base_y = if is_pixel { ay + 16.0 } else { ay + 84.0 };
    let step_x = if is_pixel { 36.0 } else { 50.0 };

    let i = 1;
    while i <= 7 {
        let id = "abot_viz_" + i;
        let x = base_x + (i - 1) * step_x;
        spawn_sparrow(
            id,
            asset,
            x,
            base_y,
            gz - 0.04,
            [#{ name: "idle", prefix: "viz" + i, fps: 24.0, looped: false }]
        );
        if is_pixel {
            prop_scale(id, 6.0, 6.0);
        }
        prop_alpha(id, 0.0);
        i += 1;
    }
}

fn start_viz(state) {
    state.active = true;
    state.alpha = 1.0;
    let i = 1;
    while i <= 7 {
        prop_alpha("abot_viz_" + i, 1.0);
        i += 1;
    }
    state
}

fn reset_viz(state) {
    state.active = false;
    state.alpha = 0.0;
    let i = 1;
    while i <= 7 {
        prop_alpha("abot_viz_" + i, 0.0);
        i += 1;
    }
    state
}

fn pulse_viz(state) {
    if !state.active { return; }
    let i = 1;
    while i <= 7 {
        play_anim_frame("abot_viz_" + i, "idle", rand_int(0, 2));
        i += 1;
    }
}
