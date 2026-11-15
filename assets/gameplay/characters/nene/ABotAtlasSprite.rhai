// A-Bot atlas state, spawning, and animation helpers (ported from ABotAtlasSprite.hxc)

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

fn spawn_abot(target, offset_x, offset_y) {
    let tl = char_top_left(target);
    let gz = char_z(target);
    let ax = tl[0] + offset_x;
    let ay = tl[1] + offset_y;

    spawn_rect_tl("abot_eye_whites", 160.0, 60.0, "#FFFFFF", ax + 40.0, ay + 250.0, gz - 0.10);
    spawn_sprite_tl("abot_stereo_bg", "gameplay/characters/nene/stereo-bg.png", ax + 150.0, ay + 30.0, gz - 0.08);
    spawn_atlas(
        "abot_pupil",
        "gameplay/characters/nene/abot-eyes",
        ax + 50.0,
        ay + 238.0,
        gz - 0.06,
        [
            #{ name: "lookLeft", start: 0, end: 16, fps: 24.0, looped: false },
            #{ name: "lookRight", start: 17, end: 31, fps: 24.0, looped: false }
        ]
    );
    play_anim_frame("abot_pupil", "lookRight", 14);
    spawn_atlas("abot_body", "gameplay/characters/nene/abot-system", ax, ay, gz - 0.02, []);
    play_anim_frame("abot_body", "idle", 1);

    [ax, ay, gz]
}

fn dance_abot() {
    play_anim_frame("abot_body", "idle", 1);
}

fn move_pupils(state, dir) {
    if dir == "left" && state.pupil_state != "left" {
        state.pupil_state = "left";
        play_anim("abot_pupil", "lookLeft");
    } else if dir == "right" && state.pupil_state != "right" {
        state.pupil_state = "right";
        play_anim("abot_pupil", "lookRight");
    }
    state
}

fn on_FocusCamera(state, ev) {
    if ev == () { return state; }
    let kind = ev.get("kind");
    if kind != "FocusCamera" { return state; }
    let val = ev.get("value");
    if val == () { return state; }
    let ch = val["char"];
    if ch == 0 {
        state = move_pupils(state, "right");
    } else if ch == 1 {
        state = move_pupils(state, "left");
    }
    state
}

fn set_abot_visible(vis) {
    prop_visible("abot_eye_whites", vis);
    prop_visible("abot_stereo_bg", vis);
    prop_visible("abot_pupil", vis);
    prop_visible("abot_body", vis);
}
