// A-Bot atlas state and metadata helpers (ported from ABotAtlasSprite.hxc)

fn abot_atlas_config() {
    #{
        asset_path: "gameplay/characters/nene/abot-system",
        default_anim: "idle",
        start_frame: 1
    }
}

fn new_abot_state(owner_id) {
    #{
        owner_id: owner_id,
        pupil_state: "right",
        visible: true
    }
}

fn on_FocusCamera(state, ev) {
    if ev == () { return state; }
    let kind = ev.get("kind");
    if kind != "FocusCamera" { return state; }
    let val = ev.get("value");
    if val == () { return state; }
    let ch = val["char"];
    if ch == 0 {
        state.pupil_state = "right";
    } else if ch == 1 {
        state.pupil_state = "left";
    }
    state
}
