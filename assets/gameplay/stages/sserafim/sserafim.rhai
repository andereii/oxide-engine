// Sserafim stage script (ported from sserafim.hxc / StageShaderRuntime to oxide-engine Rhai)

fn sserafim_shader() { "gameplay/stages/sserafim/sserafim" }
fn clear_duration_s() { 24.0 }
fn dust_hue() { 6.0 }
fn dust_sat() { -74.0 }
fn dust_bright() { -24.0 }
fn dust_contrast() { -26.0 }

fn char_targets() {
    ["bf", "gf", "dad"]
}

fn prop_targets() {
    [
        "solid",
        "bg",
        "fucker",
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
    for t in ["sserafim_cutscene", "sserafim_gf", "sserafim_bf"] {
        if prop_exists(t) {
            shader_vec(t, "vec0", state.hue, state.sat, state.bright, state.contrast);
        }
    }
}

fn sync_params_all(state) {
    for t in char_targets() {
        shader_vec(t, "vec1", state.darken_amount, state.pulse_strength, state.truck_strength, 1.0);
    }
    for t in prop_targets() {
        shader_vec(t, "vec1", state.darken_amount, state.pulse_strength, state.truck_strength, 0.0);
    }
    for t in ["sserafim_cutscene", "sserafim_gf", "sserafim_bf"] {
        if prop_exists(t) {
            shader_vec(t, "vec1", state.darken_amount, state.pulse_strength, state.truck_strength, 1.0);
        }
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

fn create_reset_state() {
    let state = #{
        hue: dust_hue(),
        sat: dust_sat(),
        bright: dust_bright(),
        contrast: dust_contrast(),
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
    };

    prop_alpha("solidCover", 0.0);
    prop_alpha("truckLight1", 0.0);
    prop_alpha("truckLight2", 0.0);
    prop_tint("backLightColor", "#FFFFFF");
    prop_alpha("backLightColor", 0.0);
    prop_alpha("backLightWhite", 0.0);

    attach_sserafim_shaders(state);
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

fn handle_update(state, elapsed) {
    if elapsed <= 0.0 {
        return state;
    }

    let hsbc_dirty = false;
    let params_dirty = false;

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

    if hsbc_dirty {
        sync_hsbc_all(state);
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

    if event_name == "sserafimDark" {
        let amount = to_f64_or(find_param(payload, "amount"), 0.0);
        let duration = to_f64_or(find_param(payload, "duration"), 0.0);
        if duration <= 0.0 {
            state.darken_amount = amount;
            state.darken_tween = ();
            sync_params_all(state);
        } else {
            state.darken_tween = make_tween(state.darken_amount, amount, duration, "sineInOut");
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
            state.truck_tween = make_tween(amount, 0.0, duration, "cubeInOut");
            state.truck2_tween = make_tween(amount, 0.0, duration, "cubeInOut");
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
    } else if event_name == "sserafimKick" {
        if to_bool_or(find_param(payload, "final"), false) {
            let dur = clear_duration_s();
            state.clear_hue_tween = make_tween(state.hue, 0.0, dur, "sineOut");
            state.clear_sat_tween = make_tween(state.sat, 0.0, dur, "sineOut");
            state.clear_bright_tween = make_tween(state.bright, 0.0, dur, "sineOut");
            state.clear_contrast_tween = make_tween(state.contrast, 0.0, dur, "sineOut");
        }
    } else if event_name == "sserafimCover" {
        let visible = to_bool_or(find_param(payload, "visible"), false);
        prop_alpha("solidCover", if visible { 1.0 } else { 0.0 });
    } else if event_name == "sserafimFlash" {
        let dur = to_f64_or(find_param(payload, "duration"), 1.6);
        game_flash("#FFFFFF", dur);
    } else if event_name == "sserafimGuitarVibration" {
        let dur = to_f64_or(find_param(payload, "duration"), 0.55);
        shake(0.015, dur);
    } else if event_name == "sserafimEnd" {
        end_stuff();
    } else if event_name == "sserafimShow" || event_name == "sserafimSing" || event_name == "sserafimBeautiful" {
        // Recognized chart events: `sserafimBeautiful` is consumed by `sserafim-gf`;
        // multi-singer show/sing cues remain no-ops in the 3-character stage setup.
    }
    state
}

fn sserafim_set_cutscene_vis(in_cutscene) {
    for id in ["truck", "backTables", "backStools", "frontStool", "dust1", "dust2", "dust3", "dust4"] {
        if prop_exists(id) {
            prop_vis(id, !in_cutscene);
        }
    }
    if prop_exists("truckDoor") {
        prop_vis("truckDoor", false);
    }
    for id in ["backTablesCutscene", "burgerCutscene"] {
        if prop_exists(id) {
            prop_vis(id, in_cutscene);
        }
    }
    if char_exists("gf") {
        char_vis("gf", false);
    }
    if char_exists("bf") {
        char_vis("bf", !in_cutscene);
    }
    if char_exists("dad") {
        char_vis("dad", !in_cutscene);
    }
    if prop_exists("sserafim_cutscene") {
        prop_vis("sserafim_cutscene", in_cutscene);
    }
}

fn sserafim_attach_shader_to(id, state) {
    if !prop_exists(id) || state == () { return; }
    shader(id, sserafim_shader());
    shader_vec(id, "vec0", state.hue, state.sat, state.bright, state.contrast);
    shader_vec(id, "vec1", state.darken_amount, state.pulse_strength, state.truck_strength, 1.0);
    shader_vec(id, "vec2", state.light_color[0], state.light_color[1], state.light_color[2], 0.0);
}

fn sserafim_create_cutscene_sprites() {
    if !prop_exists("sserafim_cutscene") {
        spawn_atlas(
            "sserafim_cutscene",
            "gameplay/stages/sserafim/cutscene/cutscene-main",
            -395.0,
            10.0,
            -497.5,
            []
        );
        prop_scroll("sserafim_cutscene", 0.94, 0.94);
        sserafim_attach_shader_to("sserafim_cutscene", this.s);
    }
    if !prop_exists("sserafim_gf") {
        spawn_atlas(
            "sserafim_gf",
            "gameplay/stages/sserafim/cutscene/gf-get-up",
            655.0,
            -104.0,
            -497.5,
            [
                #{ name: "static", prefix: "static", fps: 24.0, looped: false },
                #{ name: "getup", prefix: "getup", fps: 24.0, looped: false }
            ]
        );
        prop_scroll("sserafim_gf", 0.95, 0.95);
        prop_alpha("sserafim_gf", 0.5);
        prop_vis("sserafim_gf", false);
        sserafim_attach_shader_to("sserafim_gf", this.s);
    }
    if !prop_exists("sserafim_bf") {
        spawn_atlas(
            "sserafim_bf",
            "gameplay/stages/sserafim/cutscene/bf-get-up",
            1220.0,
            531.0,
            -469.5,
            [
                #{ name: "static", prefix: "static", fps: 24.0, looped: false },
                #{ name: "getup", prefix: "getup", fps: 24.0, looped: false }
            ]
        );
        prop_scroll("sserafim_bf", 0.99, 0.99);
        prop_vis("sserafim_bf", false);
        sserafim_attach_shader_to("sserafim_bf", this.s);
    }
    if !prop_exists("sserafim_end1") {
        spawn_hud_sprite("sserafim_end1", "gameplay/stages/sserafim/end/end-1", 640.0, 360.0, 90.0);
        prop_scale("sserafim_end1", 0.67, 0.67);
        prop_vis("sserafim_end1", false);
    }
    if !prop_exists("sserafim_end2") {
        spawn_hud_sprite("sserafim_end2", "gameplay/stages/sserafim/end/end-2", 900.0, 500.0, 90.0);
        prop_scale("sserafim_end2", 0.67, 0.67);
        prop_vis("sserafim_end2", false);
    }
}

fn sserafim_cancel_cutscene_timers() {
    for fn_name in [
        "sserafim_cs_sound",
        "sserafim_cs_tap1_on",
        "sserafim_cs_tap_off",
        "sserafim_cs_tap2_on",
        "sserafim_cs_truck_approach",
        "sserafim_cs_truck_close",
        "sserafim_cs_crash",
        "sserafim_cs_fade_out",
        "sserafim_cs_getup",
        "sserafim_cs_finish",
        "sserafim_enable_skip"
    ] {
        cancel_timer(fn_name);
    }
}

fn sserafim_cleanup_cutscene() {
    sserafim_cancel_cutscene_timers();
    for id in ["sserafim_cutscene", "sserafim_gf", "sserafim_bf", "sserafim_end1", "sserafim_end2", "sserafim_skip_txt"] {
        if prop_exists(id) {
            despawn(id);
        }
    }
    stop_snd("sserafim_start_sfx");
    stop_snd("sserafim_end1_sfx");
    stop_snd("sserafim_end2_sfx");
}

fn intro_cutscene() {
    sserafim_create_cutscene_sprites();
    sserafim_set_cutscene_vis(true);
    set_cutscene(true);
    hud_visible(false);
    set_can_pause(false);
    this.cutscene_skipped = false;
    this.can_skip = false;
    this.skip_cutoff = false;

    spawn_hud_text("sserafim_skip_txt", "Skip [Enter]", 1248.0, 696.0, 90.0, 28.0, "#FFFFFFFF", "br");
    text_font("sserafim_skip_txt", "VCR OSD Mono.ttf");
    set_prop("sserafim_skip_txt", "alpha", 0.0);

    play_anim("sserafim_cutscene", "idle");
    zoom(0.5, 0.0, "linear");
    pan_cam(660.0, -200.0, 0.0, "linear");
    fade("#FF000000", 3.0, true, false);

    after(14.5 / 24.0, "sserafim_cs_sound");
    zoom(0.7, 3.0, "circOut");
    pan_cam(660.0, 300.0, 3.0, "circOut");

    after(245.0 / 24.0, "sserafim_cs_tap1_on");
    after(251.0 / 24.0, "sserafim_cs_tap_off");
    after(406.0 / 24.0, "sserafim_cs_tap2_on");
    after(411.0 / 24.0, "sserafim_cs_tap_off");
    after(499.0 / 24.0, "sserafim_cs_truck_approach");
    after(548.0 / 24.0, "sserafim_cs_truck_close");
    after(563.0 / 24.0, "sserafim_cs_crash");
    after(650.0 / 24.0, "sserafim_cs_fade_out");
    after(710.0 / 24.0, "sserafim_cs_getup");
    after(730.0 / 24.0, "sserafim_cs_finish");
}

fn sserafim_cs_sound() {
    if this.cutscene_skipped { return; }
    play_snd("sserafim_start_sfx", "gameplay/stages/sserafim/sounds/cutscene/start-cutscene.ogg", 1.0);
}

fn sserafim_cs_tap1_on() {
    if this.cutscene_skipped || this.s == () { return; }
    this.s.darken_amount = 0.2;
    sync_params_all(this.s);
}

fn sserafim_cs_tap2_on() {
    if this.cutscene_skipped || this.s == () { return; }
    this.s.darken_amount = 0.2;
    sync_params_all(this.s);
}

fn sserafim_cs_tap_off() {
    if this.cutscene_skipped || this.s == () { return; }
    this.s.darken_tween = make_tween(this.s.darken_amount, 0.0, 0.8, "linear");
}

fn sserafim_cs_truck_approach() {
    if this.cutscene_skipped || this.s == () { return; }
    this.skip_cutoff = true;
    this.can_skip = false;
    if prop_exists("sserafim_skip_txt") {
        tween("sserafim_skip_txt", "alpha", 0.0, 0.5, "quadIn");
    }
    let dur = 49.0 / 24.0;
    this.s.clear_bright_tween = make_tween(this.s.bright, 55.0, dur, "sineOut");
    this.s.clear_hue_tween = make_tween(this.s.hue, 0.0, dur, "sineOut");
    this.s.clear_contrast_tween = make_tween(this.s.contrast, -30.0, dur, "sineOut");
    this.s.clear_sat_tween = make_tween(this.s.sat, 0.0, dur, "sineOut");
}

fn sserafim_cs_truck_close() {
    if this.cutscene_skipped || this.s == () { return; }
    let dur = 15.0 / 24.0;
    this.s.clear_bright_tween = make_tween(this.s.bright, 66.0, dur, "cubeInOut");
    this.s.clear_hue_tween = make_tween(this.s.hue, 10.0, dur, "cubeInOut");
    this.s.clear_contrast_tween = make_tween(this.s.contrast, -17.0, dur, "cubeInOut");
    this.s.clear_sat_tween = make_tween(this.s.sat, 0.0, dur, "cubeInOut");
}

fn sserafim_cs_crash() {
    if this.cutscene_skipped || this.s == () { return; }
    this.s.clear_bright_tween = ();
    this.s.clear_hue_tween = ();
    this.s.clear_contrast_tween = ();
    this.s.clear_sat_tween = ();
    this.s.bright = 0.0;
    this.s.hue = 0.0;
    this.s.contrast = 0.0;
    this.s.sat = 0.0;
    sync_hsbc_all(this.s);
    fade("#FFFFFFFF", 30.0 / 24.0, true, false);
    prop_alpha("solidCover", 1.0);
    sserafim_set_cutscene_vis(false);
}

fn sserafim_cs_fade_out() {
    if this.cutscene_skipped || this.s == () { return; }
    zoom(0.7, 0.0, "linear");
    zoom(0.55, 3.0, "circOut");
    pan_cam(1070.0, 470.0, 0.0, "linear");
    this.s.hue = dust_hue();
    this.s.sat = dust_sat();
    this.s.bright = dust_bright();
    this.s.contrast = dust_contrast();
    sync_hsbc_all(this.s);
    tween_prop("solidCover", "alpha", 0.0, 3.0, "sineOut");
    if prop_exists("sserafim_gf") {
        prop_vis("sserafim_gf", true);
        play_anim("sserafim_gf", "static");
    }
    if prop_exists("sserafim_bf") {
        prop_vis("sserafim_bf", true);
        play_anim("sserafim_bf", "static");
    }
}

fn sserafim_cs_getup() {
    if this.cutscene_skipped { return; }
    if prop_exists("sserafim_gf") { play_anim("sserafim_gf", "getup"); }
    if prop_exists("sserafim_bf") { play_anim("sserafim_bf", "getup"); }
}

fn sserafim_cs_finish() {
    if this.cutscene_skipped { return; }
    if prop_exists("sserafim_skip_txt") { despawn("sserafim_skip_txt"); }
    set_can_pause(true);
    set_cutscene(false);
    hud_visible(true);
    countdown("start");
}

fn sserafim_enable_skip() {
    if !this.cutscene_skipped && !this.skip_cutoff {
        this.can_skip = true;
    }
}

fn sserafim_do_skip() {
    sserafim_cancel_cutscene_timers();
    stop_snd("sserafim_start_sfx");
    fade("#FF000000", 0.5, true, true);
    if prop_exists("sserafim_skip_txt") { despawn("sserafim_skip_txt"); }
    sserafim_set_cutscene_vis(false);
    zoom(0.55, 1.0, "circOut");
    pan_cam(1070.0, 470.0, 1.0, "circOut");
    if prop_exists("sserafim_gf") {
        prop_vis("sserafim_gf", true);
        play_anim_frame("sserafim_gf", "getup", 23);
    }
    if prop_exists("sserafim_bf") {
        prop_vis("sserafim_bf", true);
        play_anim_frame("sserafim_bf", "getup", 23);
    }
    set_can_pause(true);
    set_cutscene(false);
    hud_visible(true);
    countdown("start");
}

fn end_stuff() {
    sserafim_create_cutscene_sprites();
    play_snd("sserafim_end1_sfx", "gameplay/stages/sserafim/sounds/cutscene/end-1.ogg", 1.0);
    after(0.05, "sserafim_end_step1");
    after(4.0, "sserafim_end_step2");
    after(8.0, "sserafim_end_step3");
    after(9.0, "sserafim_end_finish");
}

fn sserafim_end_step1() {
    if prop_exists("sserafim_end1") { prop_vis("sserafim_end1", true); }
    prop_alpha("solidCover", 1.0);
    hud_visible(false);
    set_cutscene(true);
    set_can_pause(false);
}

fn sserafim_end_step2() {
    if prop_exists("sserafim_end1") { prop_vis("sserafim_end1", false); }
    if prop_exists("sserafim_end2") { prop_vis("sserafim_end2", true); }
    play_snd("sserafim_end2_sfx", "gameplay/stages/sserafim/sounds/cutscene/end-2.ogg", 1.0);
}

fn sserafim_end_step3() {
    if prop_exists("sserafim_end1") { prop_vis("sserafim_end1", false); }
    if prop_exists("sserafim_end2") { prop_vis("sserafim_end2", false); }
}

fn sserafim_end_finish() {
    set_can_pause(true);
    set_cutscene(false);
    end_song();
}

fn onCreate() {
    this.has_played_cutscene = false;
    this.cutscene_skipped = false;
    this.can_skip = false;
    this.skip_cutoff = false;
    sserafim_cleanup_cutscene();
    this.s = create_reset_state();
}

fn onSongRetry() {
    this.has_played_cutscene = true;
    this.cutscene_skipped = false;
    this.can_skip = false;
    this.skip_cutoff = false;
    sserafim_cleanup_cutscene();
    this.s = create_reset_state();
}

fn onCountdownStart(ev) {
    if !this.has_played_cutscene {
        this.has_played_cutscene = true;
        ev.cancel();
        intro_cutscene();
    }
}

fn onBeatHit() {
    if this.s == () || in_cutscene() {
        return;
    }
    this.s = handle_beat_hit(this.s, beat());
}

fn onUpdate() {
    if in_cutscene() && !this.cutscene_skipped && !this.skip_cutoff {
        if ctrl_hit("ACCEPT") || key_hit("Enter") || key_hit("Space") {
            if !this.can_skip {
                if prop_exists("sserafim_skip_txt") {
                    tween("sserafim_skip_txt", "alpha", 1.0, 0.5, "quadOut");
                }
                after(0.5, "sserafim_enable_skip");
            } else {
                this.cutscene_skipped = true;
                fade("#FF000000", 0.5, false, true);
                after(0.5, "sserafim_do_skip");
            }
        }
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
