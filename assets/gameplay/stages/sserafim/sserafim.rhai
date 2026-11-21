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
    } else if event_name == "sserafimShow" || event_name == "sserafimSing" || event_name == "sserafimBeautiful" || event_name == "sserafimEnd" {
        // Recognized chart events: `sserafimBeautiful` is consumed by `sserafim-gf`;
        // multi-singer show/sing/end cues remain no-ops in the 3-character stage setup.
    }
    state
}

fn onCreate() {
    this.s = create_reset_state();
}

fn onSongRetry() {
    this.s = create_reset_state();
}

fn onBeatHit() {
    if this.s == () {
        return;
    }
    this.s = handle_beat_hit(this.s, beat());
}

fn onUpdate() {
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
