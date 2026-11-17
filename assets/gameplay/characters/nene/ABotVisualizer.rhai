// A-Bot visualizer bar spawning and animation helpers (ported from ABotVisualizer.hxc)

fn viz_bar_count() {
    7
}

fn viz_asset_path() {
    "gameplay/characters/nene/abot-viz"
}

fn viz_offsets_x(is_pixel) {
    if is_pixel {
        [0.0, 42.0, 48.0, 54.0, 60.0, 36.0, 42.0]
    } else {
        [0.0, 59.0, 56.0, 66.0, 54.0, 52.0, 51.0]
    }
}

fn viz_offsets_y(is_pixel) {
    if is_pixel {
        [0.0, -12.0, -6.0, 0.0, 0.0, 6.0, 12.0]
    } else {
        [0.0, -8.0, -3.5, -0.4, 0.5, 4.7, 7.0]
    }
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
    let base_x = if is_pixel { ax + 75.5 } else { ax + 207.0 };
    let base_y = if is_pixel { ay + 68.5 } else { ay + 84.0 };
    let dx = viz_offsets_x(is_pixel);
    let dy = viz_offsets_y(is_pixel);
    let acc_x = 0.0;
    let acc_y = 0.0;

    let idx = 0;
    while idx < 7 {
        acc_x += dx[idx];
        acc_y += dy[idx];
        let bar_num = idx + 1;
        let id = "abot_viz_" + bar_num;
        spawn_sparrow(
            id,
            asset,
            base_x + acc_x,
            base_y + acc_y,
            gz - 0.04,
            [#{ name: "idle", prefix: "viz" + bar_num, fps: 24.0, looped: false }]
        );
        if is_pixel {
            prop_scale(id, 6.0, 6.0);
        }
        play_anim_frame(id, "idle", 5);
        prop_visible(id, false);
        idx += 1;
    }
}

fn start_viz(state) {
    state.active = true;
    state.alpha = 1.0;
    let i = 1;
    while i <= 7 {
        prop_alpha("abot_viz_" + i, 1.0);
        prop_visible("abot_viz_" + i, false);
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
        prop_visible("abot_viz_" + i, false);
        i += 1;
    }
    state
}

fn pulse_viz(state) {
    if !state.active { return; }
    let i = 1;
    while i <= 7 {
        // Level in 0..=6 matching ABotVis.hx: 0 = hidden (silent), 1..=6 -> frame 5..=0.
        let level = if i <= 2 {
            rand_int(2, 5)
        } else if i <= 4 {
            rand_int(1, 4)
        } else {
            rand_int(0, 3)
        };
        let id = "abot_viz_" + i;
        if level <= 0 {
            prop_visible(id, false);
        } else {
            prop_visible(id, true);
            play_anim_frame(id, "idle", 6 - level);
        }
        i += 1;
    }
}
