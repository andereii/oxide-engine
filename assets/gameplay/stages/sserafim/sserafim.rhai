// Sserafim stage script (ported from sserafim.hxc / StageShaderRuntime to oxide-engine Rhai)

fn sserafim_shader() { "gameplay/stages/sserafim/sserafim" }
fn clear_duration_s() { 24.0 }
fn dust_hue() { 6.0 }
fn dust_sat() { -74.0 }
fn dust_bright() { -24.0 }
fn dust_contrast() { -26.0 }

fn char_targets() {
    [
        "bf",
        "gf",
        "dad",
        "sserafim-yunjin",
        "sserafim-chaewon",
        "sserafim-eunchae",
        "sserafimCutscene",
        "sserafimBf",
        "sserafimGf",
    ]
}

fn prop_targets() {
    [
        "solid",
        "bg",
        "fucker",
        "perspectiveFloor",
        "backTables",
        "backTablesCutscene",
        "burgerCutscene",
        "backStools",
        "truck",
        "truckDoor",
        "frontStool",
    ]
}

fn find_param(payload, key) {
    if payload == () {
        return ();
    }
    let list = payload["params"];
    if type_of(list) == "array" {
        for entry in list {
            if entry["name"] == key {
                return entry["value"];
            }
        }
    }
    payload[key]
}

fn to_f64_or(val, fallback) {
    if val == () {
        return fallback;
    }
    let ty = type_of(val);
    if ty == "f64" || ty == "float" {
        return val;
    }
    if ty == "i64" || ty == "int" {
        return val.to_float();
    }
    if ty == "string" {
        let s = val;
        s.trim();
        if s == "" {
            return fallback;
        }
        return parse_float(s);
    }
    fallback
}

fn to_bool_or(val, fallback) {
    if val == () {
        return fallback;
    }
    let ty = type_of(val);
    if ty == "bool" {
        return val;
    }
    if ty == "string" {
        let s = val;
        s.trim();
        s = s.to_lower();
        if s == "true" || s == "1" {
            return true;
        }
        if s == "false" || s == "0" {
            return false;
        }
    }
    fallback
}

fn hex_nibble(ch) {
    let idx = "0123456789abcdef".index_of(ch.to_lower());
    if idx < 0 { 0 } else { idx }
}

fn hex_byte(s, offset) {
    hex_nibble(s.sub_string(offset, 1)) * 16 + hex_nibble(s.sub_string(offset + 1, 1))
}

fn byte_to_hex(v) {
    let digits = "0123456789ABCDEF";
    let iv = clamp(v, 0.0, 255.0).to_int();
    let hi = iv / 16;
    let lo = iv % 16;
    digits.sub_string(hi, 1) + digits.sub_string(lo, 1)
}

fn parse_color_entry(raw) {
    let s = raw;
    s.trim();
    if s.starts_with("#") {
        s = s.sub_string(1, s.len() - 1);
    } else if s.starts_with("0x") || s.starts_with("0X") {
        s = s.sub_string(2, s.len() - 2);
    }
    if s.len() == 8 {
        s = s.sub_string(2, 6);
    }
    if s.len() != 6 {
        return #{ rgb: [1.0, 1.0, 1.0], hex: "#FFFFFF" };
    }
    let r = hex_byte(s, 0).to_float() / 255.0;
    let g = hex_byte(s, 2).to_float() / 255.0;
    let b = hex_byte(s, 4).to_float() / 255.0;
    #{ rgb: [r, g, b], hex: "#" + s.to_upper() }
}

fn parse_color_array(val) {
    if val == () {
        return ();
    }
    let items = if type_of(val) == "array" {
        val
    } else if type_of(val) == "string" {
        val.split(",")
    } else {
        return ();
    };
    let out = [];
    for item in items {
        let s = item.to_string();
        s.trim();
        if s != "" {
            out.push(parse_color_entry(s));
        }
    }
    out
}

fn parse_float_array(val) {
    if val == () {
        return ();
    }
    let items = if type_of(val) == "array" {
        val
    } else if type_of(val) == "string" {
        val.split(",")
    } else {
        return ();
    };
    let out = [];
    for item in items {
        let s = item.to_string();
        s.trim();
        if s != "" {
            out.push(parse_float(s));
        }
    }
    out
}

fn parse_bool_array(val) {
    if val == () {
        return ();
    }
    let items = if type_of(val) == "array" {
        val
    } else if type_of(val) == "string" {
        val.split(",")
    } else {
        return ();
    };
    let out = [];
    for item in items {
        out.push(to_bool_or(item, false));
    }
    out
}

fn make_tween(start, end, duration, ease) {
    #{ start: start, end: end, elapsed: 0.0, duration: duration, ease: ease }
}

fn eval_ease(t_in, ease) {
    let t = clamp(t_in, 0.0, 1.0);
    if ease == "sineInOut" {
        return -((pi() * t).cos() - 1.0) * 0.5;
    }
    if ease == "sineOut" {
        return (t * pi() * 0.5).sin();
    }
    if ease == "cubeInOut" {
        if t < 0.5 {
            return 4.0 * t * t * t;
        }
        let k = 1.0 - (t * 2.0 - 1.0);
        return 0.5 + 0.5 * (1.0 - k * k * k);
    }
    if ease == "quadOut" {
        let inv = 1.0 - t;
        return 1.0 - inv * inv;
    }
    if ease == "quadIn" {
        return t * t;
    }
    if ease == "expoIn" {
        if t <= 0.0 {
            return 0.0;
        }
        return (10.0 * (t - 1.0) * 0.6931471805599453).exp();
    }
    t
}

fn step_tween(tw, dt_s) {
    tw.elapsed += dt_s;
    let t = if tw.duration <= 0.0 { 1.0 } else { clamp(tw.elapsed / tw.duration, 0.0, 1.0) };
    let val = tw.start + (tw.end - tw.start) * eval_ease(t, tw.ease);
    let done = tw.duration <= 0.0 || tw.elapsed >= tw.duration;
    #{ value: val, tween: if done { () } else { tw } }
}

fn sync_hsbc_all(state) {
    for t in char_targets() {
        shader_vec(t, "vec0", state.hue, state.sat, state.bright, state.contrast);
    }
    for t in prop_targets() {
        shader_vec(t, "vec0", state.hue, state.sat, state.bright, state.contrast);
    }
}

fn sync_params_all(state) {
    for t in char_targets() {
        shader_vec(t, "vec1", state.darken_amount, state.pulse_strength, state.truck_strength, 1.0);
    }
    for t in prop_targets() {
        shader_vec(t, "vec1", state.darken_amount, state.pulse_strength, state.truck_strength, 0.0);
    }
}

fn sync_light_all(state) {
    let r = state.light_color[0];
    let g = state.light_color[1];
    let b = state.light_color[2];
    for t in char_targets() {
        shader_vec(t, "vec2", r, g, b, 0.0);
    }
    for t in prop_targets() {
        shader_vec(t, "vec2", r, g, b, 0.0);
    }
}

fn attach_sserafim_shaders(state) {
    let sh = sserafim_shader();
    for t in char_targets() {
        shader(t, sh);
        shader_vec(t, "vec0", state.hue, state.sat, state.bright, state.contrast);
        shader_vec(t, "vec1", state.darken_amount, state.pulse_strength, state.truck_strength, 1.0);
        shader_vec(t, "vec2", state.light_color[0], state.light_color[1], state.light_color[2], 0.0);
    }
    for t in prop_targets() {
        shader(t, sh);
        shader_vec(t, "vec0", state.hue, state.sat, state.bright, state.contrast);
        shader_vec(t, "vec1", state.darken_amount, state.pulse_strength, state.truck_strength, 0.0);
        shader_vec(t, "vec2", state.light_color[0], state.light_color[1], state.light_color[2], 0.0);
    }
}

fn set_girls_visible(vis_arr) {
    if vis_arr.len() < 5 {
        return;
    }
    char_visible("sserafim-yunjin", vis_arr[0]);
    char_visible("dad", vis_arr[1]);
    char_visible("sserafim-chaewon", vis_arr[2]);
    char_visible("sserafim-eunchae", vis_arr[3]);
    char_visible("bf", vis_arr[4]);
}

fn set_girls_singing(sing_arr) {
    if sing_arr.len() < 5 {
        return;
    }
    char_sing_type("sserafim-yunjin", if sing_arr[0] { "bf" } else { "dad" });
    char_sing_type("dad", if sing_arr[1] { "bf" } else { "dad" });
    char_sing_type("sserafim-chaewon", if sing_arr[2] { "bf" } else { "dad" });
    char_sing_type("sserafim-eunchae", if sing_arr[3] { "bf" } else { "dad" });
    char_sing_type("bf", if sing_arr[4] { "bf" } else { "dad" });
    if sing_arr.len() >= 6 {
        char_sing_type("gf", if sing_arr[5] { "bf" } else { "dad" });
    }
}

fn hide_dust(visible) {
    prop_visible("dust1", visible);
    prop_visible("dust2", visible);
    prop_visible("dust3", visible);
    prop_visible("dust4", visible);
}

fn update_perspective_floor() {
    let cp = cam_pos();
    let scroll_x = cp[0] - screen_w() * 0.5;
    let scroll_y = cp[1] - screen_h() * 0.5;
    let bot_x = 760.0 + (1.0 - 1.05) * scroll_x;
    let bot_y = 1375.0 + (1.0 - 1.05) * scroll_y;
    let top_x = 790.0 + (1.0 - 0.93) * scroll_x;
    let top_y = 625.0 + (1.0 - 0.93) * scroll_y;
    let h = 754.0;
    let w = 5362.0;
    let dx = top_x - bot_x;
    let dy = top_y - (bot_y - h);
    prop_pos("perspectiveFloor", bot_x - w * 0.5, bot_y - h);
    prop_matrix("perspectiveFloor", 1.0, 0.0, -(dx / h), 1.0 - (dy / h), dx * 0.5, dy * 0.5);
}

// Flixel's `FlxBackdrop` calls `scale.set(s, s)` without `updateHitbox()`, so scaling
// expands from the 2048x1280 texture center `(1024, 640)` rather than top-left `(x, y)`.
fn dust_tl_x(x, sc) { x - 1024.0 * (sc - 1.0) }
fn dust_tl_y(y, sc) { y - 640.0 * (sc - 1.0) }

fn spawn_dust_prop(id, path, x, y, scroll, sc, alpha, vel_x, tint) {
    spawn_sprite_tl(id, path, dust_tl_x(x, sc), dust_tl_y(y, sc), -300.0);
    prop_scroll(id, scroll, scroll);
    prop_scale(id, sc, sc);
    prop_vel(id, vel_x, 0.0);
    prop_tint(id, tint);
    prop_alpha(id, alpha);
    prop_repeat(id, true, false);
}

fn build_stage_extras() {
    strum_visible("opponent", false);
    popups_vis(false);
    icon_visible("dad", false);

    prop_visible("backTablesCutscene", false);
    prop_visible("burgerCutscene", false);
    prop_visible("truckDoor", false);

    spawn_sprite_tl("perspectiveFloor", "gameplay/stages/sserafim/floor.png", 760.0 - 5362.0 * 0.5, 1375.0 - 754.0, -498.9);
    prop_scroll("perspectiveFloor", 1.0, 1.0);
    update_perspective_floor();

    spawn_dust_prop("dust1", "gameplay/stages/sserafim/dust/dust-mid.png", -650.0, -200.0, 1.1, 1.5, 0.8, 350.0, "#98847D");
    spawn_dust_prop("dust2", "gameplay/stages/sserafim/dust/dust-back.png", -650.0, -250.0, 1.15, 1.5, 0.9, -300.0, "#8B6C63");
    spawn_dust_prop("dust3", "gameplay/stages/sserafim/dust/dust-mid.png", -650.0, -400.0, 1.2, 2.0, 0.8, -200.0, "#6E645C");
    spawn_dust_prop("dust4", "gameplay/stages/sserafim/dust/dust-back.png", -650.0, -1300.0, 1.25, 3.5, 0.9, -150.0, "#886A60");

    char_spawn("sserafim-yunjin", "sserafim-yunjin", -623.0, -193.0, 1000.0, 0.95, 0.95);
    play_anim("sserafim-yunjin", "doorclosed");
    set_dance_every("sserafim-yunjin", 0.0);
    char_spawn("sserafim-chaewon", "sserafim-chaewon", 687.0, -232.0, 1000.0, 0.95, 0.95);
    char_spawn("sserafim-eunchae", "sserafim-eunchae", 770.0, 415.0, 1000.0, 0.97, 0.97);

    set_girls_visible([true, false, false, false, false]);
    set_girls_singing([false, false, false, false, false, false]);
    char_visible("gf", false);

    spawn_atlas("sserafimCutscene", "gameplay/stages/sserafim/cutscene/cutscene-main", -395.0, 10.0, -497.5, []);
    prop_scroll("sserafimCutscene", 0.94, 0.94);
    prop_visible("sserafimCutscene", false);

    let getup_anims = [
        #{ name: "static", prefix: "static", fps: 24.0, looped: false },
        #{ name: "getup", prefix: "getup", fps: 24.0, looped: false },
    ];
    spawn_atlas("sserafimGf", "gameplay/stages/sserafim/cutscene/gf-get-up", 655.0, -104.0, -497.4, getup_anims);
    prop_scroll("sserafimGf", 0.95, 0.95);
    prop_alpha("sserafimGf", 1.0);
    prop_visible("sserafimGf", false);

    spawn_atlas("sserafimBf", "gameplay/stages/sserafim/cutscene/bf-get-up", 1220.0, 531.0, -469.5, getup_anims);
    prop_scroll("sserafimBf", 0.99, 0.99);
    prop_visible("sserafimBf", false);

    spawn_sprite_tl("SEEYOU1", "gameplay/stages/sserafim/end/end-1.png", 387.745, 108.08, 10000.0);
    prop_scale("SEEYOU1", 0.67, 0.67);
    prop_overlay("SEEYOU1", true);
    prop_visible("SEEYOU1", false);

    spawn_sprite_tl("SEEYOU2", "gameplay/stages/sserafim/end/end-2.png", 516.4, 629.75, 10000.0);
    prop_scale("SEEYOU2", 0.67, 0.67);
    prop_overlay("SEEYOU2", true);
    prop_visible("SEEYOU2", false);

    preload_snd("gameplay/stages/sserafim/sounds/cutscene/start-cutscene.ogg");
    preload_snd("gameplay/stages/sserafim/sounds/door-kick-1.ogg");
    preload_snd("gameplay/stages/sserafim/sounds/door-kick-2.ogg");
    preload_snd("gameplay/stages/sserafim/sounds/cutscene/end-1.ogg");
    preload_snd("gameplay/stages/sserafim/sounds/cutscene/end-2.ogg");
}

fn create_reset_state() {
    let state = #{
        hue: 0.0,
        sat: 0.0,
        bright: 0.0,
        contrast: 0.0,
        clear_hue_tween: (),
        clear_sat_tween: (),
        clear_bright_tween: (),
        clear_contrast_tween: (),
        darken_amount: 0.0,
        darken_tween: (),
        truck_strength: 0.0,
        truck_tween: (),
        truck2_alpha: 0.0,
        truck2_tween: (),
        pulse_strength: 0.0,
        pulse_tween: (),
        white_pulse_alpha: 0.0,
        white_pulse_tween: (),
        light_color: [1.0, 1.0, 1.0],
        light_hex: "#FFFFFF",
        lights_enabled: false,
        pulse_colors: [],
        pulse_durations: [],
        pulse_intensities: [],
        last_beat: -999999,
        yunjin_kicking: false,
        dust1_y_tween: (),
        dust2_y_tween: (),
        dust3_y_tween: (),
        dust4_y_tween: (),
        dust1_vx_tween: (),
        dust2_vx_tween: (),
        dust3_vx_tween: (),
        dust4_vx_tween: (),
        dust1_a_tween: (),
        dust2_a_tween: (),
        dust3_a_tween: (),
        dust4_a_tween: (),
        solid_cover_tween: (),
        cutscene_playing: false,
        cutscene_t: 0.0,
        cutscene_stage: 0,
        cutscene_skipped: false,
        can_skip_cutscene: false,
        skip_prompt_shown: false,
        skip_unlock_timer: 0.0,
        skip_text_tween: (),
        skip_fade_snd_tween: (),
        skip_finish_timer: -1.0,
        end_active: false,
        end_t: 0.0,
        end_stage: 0,
        end_ready: false,
    };

    prop_alpha("solidCover", 0.0);
    prop_alpha("truckLight1", 0.0);
    prop_alpha("truckLight2", 0.0);
    prop_tint("backLightColor", "#CC3300");
    prop_alpha("backLightColor", 0.0);
    prop_alpha("backLightWhite", 0.0);

    attach_sserafim_shaders(state);
    state
}

fn reset_clear(state) {
    play_anim("sserafim-yunjin", "doorclosed");
    set_dance_every("sserafim-yunjin", 0.0);
    prop_visible("truckDoor", false);
    state.yunjin_kicking = false;

    state.clear_hue_tween = ();
    state.clear_sat_tween = ();
    state.clear_bright_tween = ();
    state.clear_contrast_tween = ();
    state.dust1_y_tween = ();
    state.dust2_y_tween = ();
    state.dust3_y_tween = ();
    state.dust4_y_tween = ();
    state.dust1_vx_tween = ();
    state.dust2_vx_tween = ();
    state.dust3_vx_tween = ();
    state.dust4_vx_tween = ();
    state.dust1_a_tween = ();
    state.dust2_a_tween = ();
    state.dust3_a_tween = ();
    state.dust4_a_tween = ();

    state.hue = dust_hue();
    state.sat = dust_sat();
    state.bright = dust_bright();
    state.contrast = dust_contrast();
    sync_hsbc_all(state);

    prop_pos("dust1", dust_tl_x(-650.0, 1.5), dust_tl_y(-400.0, 1.5));
    prop_pos("dust2", dust_tl_x(-650.0, 1.5), dust_tl_y(-450.0, 1.5));
    prop_pos("dust3", dust_tl_x(-650.0, 2.0), dust_tl_y(-600.0, 2.0));
    prop_pos("dust4", dust_tl_x(-650.0, 3.5), dust_tl_y(-1500.0, 3.5));
    prop_vel("dust1", 350.0, 0.0);
    prop_vel("dust2", -300.0, 0.0);
    prop_vel("dust3", -200.0, 0.0);
    prop_vel("dust4", -150.0, 0.0);
    prop_alpha("dust1", 1.0);
    prop_alpha("dust2", 1.0);
    prop_alpha("dust3", 1.0);
    prop_alpha("dust4", 1.0);
    state
}

fn start_clear_tweens(state) {
    let dur = clear_duration_s();
    state.clear_hue_tween = make_tween(state.hue, 0.0, dur, "sineOut");
    state.clear_sat_tween = make_tween(state.sat, 0.0, dur, "sineOut");
    state.clear_bright_tween = make_tween(state.bright, 0.0, dur, "sineOut");
    state.clear_contrast_tween = make_tween(state.contrast, 0.0, dur, "sineOut");

    let p1 = get_prop_pos("dust1");
    let p2 = get_prop_pos("dust2");
    let p3 = get_prop_pos("dust3");
    let p4 = get_prop_pos("dust4");
    state.dust1_y_tween = make_tween(p1[1], p1[1] + 100.0, 20.0, "sineOut");
    state.dust2_y_tween = make_tween(p2[1], p2[1] + 200.0, 16.0, "sineOut");
    state.dust3_y_tween = make_tween(p3[1], p3[1] + 150.0, 24.0, "sineOut");
    state.dust4_y_tween = make_tween(p4[1], p4[1] + 100.0, 16.0, "sineOut");

    state.dust1_a_tween = make_tween(get_prop_alpha("dust1"), 0.0, 20.0, "sineOut");
    state.dust2_a_tween = make_tween(get_prop_alpha("dust2"), 0.0, 16.0, "sineOut");
    state.dust3_a_tween = make_tween(get_prop_alpha("dust3"), 0.0, 24.0, "sineOut");
    state.dust4_a_tween = make_tween(get_prop_alpha("dust4"), 0.0, 16.0, "sineOut");

    state.dust1_vx_tween = make_tween(350.0, 0.0, 20.0, "sineOut");
    state.dust2_vx_tween = make_tween(-300.0, 0.0, 16.0, "sineOut");
    state.dust3_vx_tween = make_tween(-200.0, 0.0, 24.0, "sineOut");
    state.dust4_vx_tween = make_tween(-150.0, 0.0, 16.0, "sineOut");
    state
}

fn set_cutscene_visibility(state, in_cutscene) {
    if in_cutscene {
        set_girls_visible([false, false, false, false, false]);
    } else {
        set_girls_visible([true, false, false, false, false]);
    }

    prop_visible("truck", !in_cutscene);
    prop_visible("truckDoor", false);
    prop_visible("backTables", !in_cutscene);
    prop_visible("backStools", !in_cutscene);
    prop_visible("frontStool", !in_cutscene);
    hide_dust(!in_cutscene);

    prop_tex(
        "perspectiveFloor",
        if in_cutscene {
            "gameplay/stages/sserafim/cutscene/floor-cutscene.png"
        } else {
            "gameplay/stages/sserafim/floor.png"
        }
    );

    prop_visible("backTablesCutscene", in_cutscene);
    prop_visible("burgerCutscene", in_cutscene);

    char_visible("gf", false);
    prop_visible("sserafimCutscene", in_cutscene);
    update_perspective_floor();
    state
}

fn run_cutscene_from_restart(state) {
    state = set_cutscene_visibility(state, false);
    state = reset_clear(state);

    zoom(0.55, 1.0, "circOut");
    pan_cam(1070.0, 470.0, 1.0, "circOut");

    prop_visible("sserafimGf", true);
    prop_visible("sserafimBf", true);
    play_anim_frame("sserafimGf", "getup", 23);
    play_anim_frame("sserafimBf", "getup", 23);
    state
}

fn exit_cutscene_for_playback(state) {
    state.cutscene_playing = false;
    state.cutscene_skipped = true;
    state.can_skip_cutscene = false;
    state.skip_text_tween = ();
    state.skip_fade_snd_tween = ();
    state.skip_finish_timer = -1.0;
    stop_snd("cutscene");
    text_style("skipText", 40.0, "#00FFFFFF", "top_left");
    set_cutscene(false);
    set_can_pause(true);
    hud_visible(true, 0.0);
    hud_fade(1.0, 0.0);
    run_cutscene_from_restart(state)
}

fn start_intro_cutscene(state) {
    set_cutscene(true);
    set_can_pause(false);

    spawn_hud_text("skipText", "Skip [ Z ]", 936.0, 618.0, 950.0, 40.0, "#00FFFFFF", "top_left");
    text_overlay("skipText", true);

    play_anim("sserafimCutscene", "idle");

    zoom(0.5, 0.0, "linear");
    pan_cam(660.0, -200.0, 0.0, "linear");
    overlay_fade("#FF000000", 3.0, true);
    zoom(0.7, 3.0, "circOut");
    pan_cam(660.0, 300.0, 3.0, "circOut");

    state.cutscene_playing = true;
    state.cutscene_t = 0.0;
    state.cutscene_stage = 0;
    state.cutscene_skipped = false;
    state.can_skip_cutscene = false;
    state.skip_prompt_shown = false;
    state.skip_unlock_timer = 0.0;
    state.skip_text_tween = ();
    state.skip_fade_snd_tween = ();
    state.skip_finish_timer = -1.0;
    state
}

fn step_cutscene_timeline(state, elapsed) {
    if state.skip_finish_timer >= 0.0 {
        if state.skip_fade_snd_tween != () {
            let res = step_tween(state.skip_fade_snd_tween, elapsed);
            snd_vol("cutscene", res.value);
            state.skip_fade_snd_tween = res.tween;
        }
        if state.skip_text_tween != () {
            let res = step_tween(state.skip_text_tween, elapsed);
            let hex = "#" + byte_to_hex(res.value * 255.0) + "FFFFFF";
            text_style("skipText", 40.0, hex, "top_left");
            state.skip_text_tween = res.tween;
        }
        state.skip_finish_timer -= elapsed;
        if state.skip_finish_timer <= 0.0 {
            state.skip_finish_timer = -1.0;
            state.cutscene_playing = false;
            stop_snd("cutscene");
            overlay_fade("#FF000000", 0.5, true);
            text_style("skipText", 40.0, "#00FFFFFF", "top_left");
            set_cutscene(false);
            set_can_pause(true);
            hud_visible(true, 0.25);
            countdown("start");
            state = run_cutscene_from_restart(state);
        }
        return state;
    }

    if state.skip_text_tween != () {
        let res = step_tween(state.skip_text_tween, elapsed);
        let hex = "#" + byte_to_hex(res.value * 255.0) + "FFFFFF";
        text_style("skipText", 40.0, hex, "top_left");
        state.skip_text_tween = res.tween;
    }
    if state.skip_unlock_timer > 0.0 {
        state.skip_unlock_timer -= elapsed;
        if state.skip_unlock_timer <= 0.0 {
            state.can_skip_cutscene = true;
        }
    }

    let advance_pressed = ctrl_hit("CUTSCENE_ADVANCE") || ctrl_hit("ACCEPT") || ctrl_hit("PAUSE");
    if advance_pressed && !state.cutscene_skipped {
        if !state.can_skip_cutscene {
            if !state.skip_prompt_shown {
                state.skip_prompt_shown = true;
                state.skip_text_tween = make_tween(0.0, 1.0, 0.5, "quadOut");
                state.skip_unlock_timer = 0.5;
            }
        } else {
            state.cutscene_skipped = true;
            state.can_skip_cutscene = false;
            state.skip_text_tween = make_tween(1.0, 0.0, 0.5, "linear");
            overlay_fade("#FF000000", 0.5, false);
            state.skip_fade_snd_tween = make_tween(1.0, 0.0, 0.5, "linear");
            state.skip_finish_timer = 0.5;
            return state;
        }
    }

    state.cutscene_t += elapsed;
    let t = state.cutscene_t;

    if state.cutscene_stage == 0 && t >= 20.0 / 24.0 {
        state.cutscene_stage = 1;
        play_snd("cutscene", "gameplay/stages/sserafim/sounds/cutscene/start-cutscene.ogg", 1.0, false);
    }
    if state.cutscene_stage == 1 && t >= 245.0 / 24.0 {
        state.cutscene_stage = 2;
        state.darken_tween = make_tween(state.darken_amount, 0.2, 0.01, "sineInOut");
    }
    if state.cutscene_stage == 2 && t >= 251.0 / 24.0 {
        state.cutscene_stage = 3;
        state.darken_tween = make_tween(state.darken_amount, 0.0, 0.8, "sineInOut");
    }
    if state.cutscene_stage == 3 && t >= 406.0 / 24.0 {
        state.cutscene_stage = 4;
        state.darken_tween = make_tween(state.darken_amount, 0.2, 0.01, "sineInOut");
    }
    if state.cutscene_stage == 4 && t >= 411.0 / 24.0 {
        state.cutscene_stage = 5;
        state.darken_tween = make_tween(state.darken_amount, 0.0, 0.8, "sineInOut");
    }
    if state.cutscene_stage == 5 && t >= 499.0 / 24.0 {
        state.cutscene_stage = 6;
        state.cutscene_skipped = true;
        state.can_skip_cutscene = false;
        state.skip_text_tween = make_tween(if state.skip_prompt_shown { 1.0 } else { 0.0 }, 0.0, 0.5, "quadIn");
        let d = 49.0 / 24.0;
        state.clear_hue_tween = make_tween(state.hue, 0.0, d, "sineOut");
        state.clear_sat_tween = make_tween(state.sat, 0.0, d, "sineOut");
        state.clear_bright_tween = make_tween(state.bright, 55.0, d, "sineOut");
        state.clear_contrast_tween = make_tween(state.contrast, -30.0, d, "sineOut");
    }
    if state.cutscene_stage == 6 && t >= 548.0 / 24.0 {
        state.cutscene_stage = 7;
        let d = 15.0 / 24.0;
        state.clear_hue_tween = make_tween(state.hue, 10.0, d, "expoIn");
        state.clear_sat_tween = make_tween(state.sat, 0.0, d, "expoIn");
        state.clear_bright_tween = make_tween(state.bright, 66.0, d, "expoIn");
        state.clear_contrast_tween = make_tween(state.contrast, -17.0, d, "expoIn");
    }
    if state.cutscene_stage == 7 && t >= 563.0 / 24.0 {
        state.cutscene_stage = 8;
        state.clear_hue_tween = ();
        state.clear_sat_tween = ();
        state.clear_bright_tween = ();
        state.clear_contrast_tween = ();
        state.hue = 0.0;
        state.sat = 0.0;
        state.bright = 0.0;
        state.contrast = 0.0;
        sync_hsbc_all(state);
        overlay_fade("#FFFFFFFF", 30.0 / 24.0, true);
        prop_alpha("solidCover", 1.0);
        play_anim("sserafim-yunjin", "doorclosed");
        set_dance_every("sserafim-yunjin", 0.0);
        state = set_cutscene_visibility(state, false);
    }
    if state.cutscene_stage == 8 && t >= 650.0 / 24.0 {
        state.cutscene_stage = 9;
        zoom(0.7, 0.0, "linear");
        zoom(0.55, 3.0, "circOut");
        pan_cam(1070.0, 470.0, 0.0, "linear");
        state = reset_clear(state);
        state.solid_cover_tween = make_tween(1.0, 0.0, 3.0, "sineOut");
        prop_visible("sserafimGf", true);
        prop_visible("sserafimBf", true);
        play_anim("sserafimGf", "static");
        play_anim("sserafimBf", "static");
    }
    if state.cutscene_stage == 9 && t >= 710.0 / 24.0 {
        state.cutscene_stage = 10;
        play_anim("sserafimGf", "getup");
        play_anim("sserafimBf", "getup");
    }
    if state.cutscene_stage == 10 && t >= 730.0 / 24.0 {
        state.cutscene_stage = 11;
        state.cutscene_playing = false;
        set_cutscene(false);
        set_can_pause(true);
        hud_visible(true, 0.25);
        countdown("start");
    }
    state
}

fn step_end_sequence(state, elapsed) {
    state.end_t += elapsed;
    let t = state.end_t;
    if state.end_stage == 0 && t >= 0.05 {
        state.end_stage = 1;
        prop_visible("SEEYOU1", true);
        prop_alpha("solidCover", 1.0);
        hud_visible(false);
        hud_fade(0.0, 0.0);
        set_cutscene(true);
        set_can_pause(false);
    }
    if state.end_stage == 1 && t >= 4.0 {
        state.end_stage = 2;
        prop_visible("SEEYOU1", false);
        prop_visible("SEEYOU2", true);
        sfx("gameplay/stages/sserafim/sounds/cutscene/end-2.ogg", 1.0);
    }
    if state.end_stage == 2 && t >= 8.0 {
        state.end_stage = 3;
        prop_visible("SEEYOU1", false);
        prop_visible("SEEYOU2", false);
    }
    if state.end_stage == 3 && t >= 9.0 {
        state.end_stage = 4;
        state.end_active = false;
        state.end_ready = true;
        set_cutscene(false);
        set_can_pause(true);
        end_song();
    }
    state
}

fn flash_back_light(state, amount, duration_s, color_entry) {
    state.light_color = color_entry.rgb;
    state.light_hex = color_entry.hex;
    let pulse_alpha = amount * 0.8;
    let white_alpha = amount * 0.7;
    state.pulse_strength = pulse_alpha;
    state.white_pulse_alpha = white_alpha;
    if duration_s <= 0.0 {
        state.pulse_tween = ();
        state.white_pulse_tween = ();
    } else {
        state.pulse_tween = make_tween(pulse_alpha, 0.0, duration_s, "cubeInOut");
        state.white_pulse_tween = make_tween(white_alpha, 0.0, duration_s, "cubeInOut");
    }
    prop_tint("backLightColor", state.light_hex);
    prop_alpha("backLightColor", state.pulse_strength);
    prop_alpha("backLightWhite", state.white_pulse_alpha);
    sync_light_all(state);
    sync_params_all(state);
    state
}

fn handle_beat_hit(state, b) {
    if b == state.last_beat {
        return state;
    }
    state.last_beat = b;
    if state.lights_enabled
        && b >= 0
        && state.pulse_colors.len() > 0
        && state.pulse_durations.len() > 0
        && state.pulse_intensities.len() > 0
    {
        let c = state.pulse_colors[b % state.pulse_colors.len()];
        let dur = state.pulse_durations[b % state.pulse_durations.len()];
        let intensity = state.pulse_intensities[b % state.pulse_intensities.len()];
        state = flash_back_light(state, intensity, dur, c);
    }
    state
}

fn step_clear_tweens(state, elapsed) {
    let hsbc_dirty = false;

    if state.clear_hue_tween != () {
        let res = step_tween(state.clear_hue_tween, elapsed);
        state.hue = res.value;
        state.clear_hue_tween = res.tween;
        hsbc_dirty = true;
    }
    if state.clear_sat_tween != () {
        let res = step_tween(state.clear_sat_tween, elapsed);
        state.sat = res.value;
        state.clear_sat_tween = res.tween;
        hsbc_dirty = true;
    }
    if state.clear_bright_tween != () {
        let res = step_tween(state.clear_bright_tween, elapsed);
        state.bright = res.value;
        state.clear_bright_tween = res.tween;
        hsbc_dirty = true;
    }
    if state.clear_contrast_tween != () {
        let res = step_tween(state.clear_contrast_tween, elapsed);
        state.contrast = res.value;
        state.clear_contrast_tween = res.tween;
        hsbc_dirty = true;
    }

    if state.dust1_y_tween != () {
        let res = step_tween(state.dust1_y_tween, elapsed);
        set_prop("dust1", "y", res.value);
        state.dust1_y_tween = res.tween;
    }
    if state.dust2_y_tween != () {
        let res = step_tween(state.dust2_y_tween, elapsed);
        set_prop("dust2", "y", res.value);
        state.dust2_y_tween = res.tween;
    }
    if state.dust3_y_tween != () {
        let res = step_tween(state.dust3_y_tween, elapsed);
        set_prop("dust3", "y", res.value);
        state.dust3_y_tween = res.tween;
    }
    if state.dust4_y_tween != () {
        let res = step_tween(state.dust4_y_tween, elapsed);
        set_prop("dust4", "y", res.value);
        state.dust4_y_tween = res.tween;
    }

    if state.dust1_a_tween != () {
        let res = step_tween(state.dust1_a_tween, elapsed);
        prop_alpha("dust1", res.value);
        state.dust1_a_tween = res.tween;
    }
    if state.dust2_a_tween != () {
        let res = step_tween(state.dust2_a_tween, elapsed);
        prop_alpha("dust2", res.value);
        state.dust2_a_tween = res.tween;
    }
    if state.dust3_a_tween != () {
        let res = step_tween(state.dust3_a_tween, elapsed);
        prop_alpha("dust3", res.value);
        state.dust3_a_tween = res.tween;
    }
    if state.dust4_a_tween != () {
        let res = step_tween(state.dust4_a_tween, elapsed);
        prop_alpha("dust4", res.value);
        state.dust4_a_tween = res.tween;
    }

    if state.dust1_vx_tween != () {
        let res = step_tween(state.dust1_vx_tween, elapsed);
        prop_vel("dust1", res.value, 0.0);
        state.dust1_vx_tween = res.tween;
    }
    if state.dust2_vx_tween != () {
        let res = step_tween(state.dust2_vx_tween, elapsed);
        prop_vel("dust2", res.value, 0.0);
        state.dust2_vx_tween = res.tween;
    }
    if state.dust3_vx_tween != () {
        let res = step_tween(state.dust3_vx_tween, elapsed);
        prop_vel("dust3", res.value, 0.0);
        state.dust3_vx_tween = res.tween;
    }
    if state.dust4_vx_tween != () {
        let res = step_tween(state.dust4_vx_tween, elapsed);
        prop_vel("dust4", res.value, 0.0);
        state.dust4_vx_tween = res.tween;
    }

    if hsbc_dirty {
        sync_hsbc_all(state);
    }
    state
}

fn handle_update(state, elapsed) {
    update_perspective_floor();

    if state.cutscene_playing && time() > 0.0 {
        state = exit_cutscene_for_playback(state);
    }

    if state.yunjin_kicking {
        let anim_name = char_anim("sserafim-yunjin");
        let cur_f = char_frame("sserafim-yunjin");
        if (anim_name == "kick2" && cur_f >= 23) || (anim_name != "kick2" && anim_name != "") {
            prop_visible("truckDoor", true);
            state.yunjin_kicking = false;
        }
    }

    if elapsed <= 0.0 {
        return state;
    }

    if state.cutscene_playing {
        state = step_cutscene_timeline(state, elapsed);
    }
    if state.end_active {
        state = step_end_sequence(state, elapsed);
    }

    state = step_clear_tweens(state, elapsed);

    let params_dirty = false;
    if state.darken_tween != () {
        let res = step_tween(state.darken_tween, elapsed);
        state.darken_amount = res.value;
        state.darken_tween = res.tween;
        params_dirty = true;
    }
    if state.truck_tween != () {
        let res = step_tween(state.truck_tween, elapsed);
        state.truck_strength = res.value;
        state.truck_tween = res.tween;
        prop_alpha("truckLight1", state.truck_strength);
        params_dirty = true;
    }
    if state.truck2_tween != () {
        let res = step_tween(state.truck2_tween, elapsed);
        state.truck2_alpha = res.value;
        state.truck2_tween = res.tween;
        prop_alpha("truckLight2", state.truck2_alpha);
    }
    if state.pulse_tween != () {
        let res = step_tween(state.pulse_tween, elapsed);
        state.pulse_strength = res.value;
        state.pulse_tween = res.tween;
        prop_alpha("backLightColor", state.pulse_strength);
        params_dirty = true;
    }
    if state.white_pulse_tween != () {
        let res = step_tween(state.white_pulse_tween, elapsed);
        state.white_pulse_alpha = res.value;
        state.white_pulse_tween = res.tween;
        prop_alpha("backLightWhite", state.white_pulse_alpha);
    }
    if state.solid_cover_tween != () {
        let res = step_tween(state.solid_cover_tween, elapsed);
        prop_alpha("solidCover", res.value);
        state.solid_cover_tween = res.tween;
    }

    if params_dirty {
        sync_params_all(state);
    }
    state
}

fn handle_song_event(state, ev) {
    let kind = ev.get("kind");
    let raw_val = ev.get("value");
    let event_name = kind;
    let payload = raw_val;
    if kind == "CustomEvent" && raw_val != () {
        let inner = raw_val["name"];
        if inner != () {
            event_name = inner;
        }
    }
    let ev_time = to_f64_or(ev.get("time"), time());
    let late_ms = time() - ev_time;

    if state.cutscene_playing && time() > 0.0 {
        state = exit_cutscene_for_playback(state);
    }

    if event_name == "sserafimDark" {
        let amount = to_f64_or(find_param(payload, "amount"), 0.0);
        let duration = to_f64_or(find_param(payload, "duration"), 0.0);
        if duration <= 0.0 {
            state.darken_amount = amount;
            state.darken_tween = ();
            sync_params_all(state);
        } else {
            let tw = make_tween(state.darken_amount, amount, duration, "sineInOut");
            if late_ms > 500.0 {
                let res = step_tween(tw, late_ms / 1000.0);
                state.darken_amount = res.value;
                state.darken_tween = res.tween;
                sync_params_all(state);
            } else {
                state.darken_tween = tw;
            }
        }
    } else if event_name == "sserafimLights" {
        let amount = to_f64_or(find_param(payload, "amount"), 0.0);
        let duration = to_f64_or(find_param(payload, "duration"), 0.0);
        state.truck_strength = amount;
        state.truck2_alpha = amount;
        if duration <= 0.0 {
            state.truck_tween = ();
            state.truck2_tween = ();
        } else {
            let tw1 = make_tween(amount, 0.0, duration, "cubeInOut");
            let tw2 = make_tween(amount, 0.0, duration, "cubeInOut");
            if late_ms > 500.0 {
                let r1 = step_tween(tw1, late_ms / 1000.0);
                let r2 = step_tween(tw2, late_ms / 1000.0);
                state.truck_strength = r1.value;
                state.truck2_alpha = r2.value;
                state.truck_tween = r1.tween;
                state.truck2_tween = r2.tween;
            } else {
                state.truck_tween = tw1;
                state.truck2_tween = tw2;
            }
        }
        prop_alpha("truckLight1", state.truck_strength);
        prop_alpha("truckLight2", state.truck2_alpha);
        sync_params_all(state);
    } else if event_name == "sserafimPulseLights" {
        state.lights_enabled = to_bool_or(find_param(payload, "enabled"), false);
        let colors = parse_color_array(find_param(payload, "colors"));
        if colors != () {
            state.pulse_colors = colors;
        }
        let durs = parse_float_array(find_param(payload, "durations"));
        if durs != () {
            state.pulse_durations = durs;
        }
        let ints = parse_float_array(find_param(payload, "intensities"));
        if ints != () {
            state.pulse_intensities = ints;
        }
    } else if event_name == "sserafimShow" {
        let vis_arr = parse_bool_array(find_param(payload, "visible"));
        if vis_arr != () {
            set_girls_visible(vis_arr);
        }
    } else if event_name == "sserafimSing" {
        let sing_arr = parse_bool_array(find_param(payload, "singing"));
        if sing_arr != () {
            set_girls_singing(sing_arr);
        }
    } else if event_name == "sserafimKick" {
        if to_bool_or(find_param(payload, "final"), false) {
            play_anim("sserafim-yunjin", "kick2");
            if late_ms <= 500.0 {
                sfx("gameplay/stages/sserafim/sounds/door-kick-2.ogg", 1.0);
            }
            set_dance_every("sserafim-yunjin", 1.0);
            icon_visible("dad", true);
            prop_visible("sserafimGf", false);
            prop_visible("sserafimBf", false);
            char_visible("gf", true);
            state.yunjin_kicking = true;
            state = start_clear_tweens(state);
            if late_ms > 500.0 {
                state = step_clear_tweens(state, late_ms / 1000.0);
                prop_visible("truckDoor", true);
                state.yunjin_kicking = false;
            }
        } else {
            play_anim("sserafim-yunjin", "kick1");
            if late_ms <= 500.0 {
                sfx("gameplay/stages/sserafim/sounds/door-kick-1.ogg", 1.0);
            }
        }
    } else if event_name == "sserafimCover" {
        let visible = to_bool_or(find_param(payload, "visible"), false);
        prop_alpha("solidCover", if visible { 1.0 } else { 0.0 });
    } else if event_name == "sserafimFlash" {
        if late_ms <= 500.0 {
            let dur = to_f64_or(find_param(payload, "duration"), 1.6);
            game_flash("#FFFFFF", dur);
        }
    } else if event_name == "sserafimGuitarVibration" {
        if late_ms <= 500.0 {
            let dur = to_f64_or(find_param(payload, "duration"), 0.55);
            shake(0.015, dur);
        }
    } else if event_name == "sserafimEnd" {
        if !state.end_active {
            sfx("gameplay/stages/sserafim/sounds/cutscene/end-1.ogg", 1.0);
            state.end_active = true;
            state.end_t = 0.0;
            state.end_stage = 0;
            state.end_ready = false;
        }
    }
    state
}

fn onCreate() {
    this.has_hidden = false;
    this.has_played_cutscene = false;
    build_stage_extras();
    this.s = create_reset_state();
}

fn onDestroy() {
    this.has_hidden = false;
    this.has_played_cutscene = false;
    stop_snd("cutscene");
}

fn onCountdownStart(ev) {
    if !this.has_played_cutscene {
        this.has_played_cutscene = true;
        if start_ms() > 0.0 || time() > 0.0 {
            if this.s != () {
                this.s.cutscene_skipped = true;
                this.s = run_cutscene_from_restart(this.s);
            }
            return;
        }
        ev.cancel();
        hud_visible(false);
        hud_fade(0.0, 0.0);
        if this.s != () {
            this.s = set_cutscene_visibility(this.s, true);
            this.s = start_intro_cutscene(this.s);
        }
    }
}

fn onSongRetry() {
    stop_snd("cutscene");
    set_cutscene(false);
    set_can_pause(true);
    hud_visible(true);
    hud_fade(1.0, 0.0);
    prop_visible("SEEYOU1", false);
    prop_visible("SEEYOU2", false);
    set_girls_visible([true, false, false, false, false]);
    set_girls_singing([false, false, false, false, false, false]);
    icon_visible("dad", false);
    this.s = create_reset_state();
    this.s = run_cutscene_from_restart(this.s);
}

fn onSongEnd(ev) {
    if this.s != () && this.s.end_active && !this.s.end_ready {
        ev.cancel();
    }
}

fn onBeatHit() {
    if this.s == () {
        return;
    }
    this.s = handle_beat_hit(this.s, beat());
}

fn onUpdate() {
    if !this.has_hidden {
        this.has_hidden = true;
        strum_visible("opponent", false);
    }
    if this.s == () {
        return;
    }
    this.s = handle_update(this.s, dt());
}

fn onSongEvent(ev) {
    if this.s == () {
        return;
    }
    this.s = handle_song_event(this.s, ev);
}
