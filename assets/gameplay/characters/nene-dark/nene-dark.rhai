// nene-dark character script (ported from nene-dark.hxc)

fn find_slot(id) {
    if char_id("gf") == id { return "gf"; }
    if char_id("dad") == id { return "dad"; }
    if char_id("bf") == id { return "bf"; }
    ""
}

fn transition_nene_state(nene_state, target) {
    let v_thresh = 0.5;
    let h = hp();
    let cur = char_anim(target);
    let finished = char_anim_finished(target);
    let train = global_get("train_passing") == true;
    let next = nene_state;

    if next == "DEFAULT" {
        if h <= v_thresh {
            next = "PRE_RAISE";
        }
        if train {
            next = "HAIR_BLOWING";
            play_anim(target, "hairBlowNormal");
            set_dance_every(target, 0.0);
        }
    } else if next == "PRE_RAISE" {
        if h > v_thresh {
            next = "DEFAULT";
            set_dance_every(target, 1.0);
        } else if train {
            next = "HAIR_BLOWING";
            play_anim(target, "hairBlowNormal");
            set_dance_every(target, 0.0);
        } else if cur == "danceLeft" && finished {
            next = "RAISE";
            play_anim(target, "raiseKnife");
            set_dance_every(target, 0.0);
        }
    } else if next == "RAISE" {
        if train {
            next = "HAIR_BLOWING_RAISE";
            play_anim(target, "hairBlowKnife");
        } else if finished {
            next = "READY";
            set_dance_every(target, 1.0);
        }
    } else if next == "READY" {
        if train {
            next = "HAIR_BLOWING_RAISE";
            play_anim(target, "hairBlowKnife");
            set_dance_every(target, 0.0);
        } else if h > v_thresh {
            next = "LOWER";
        }
    } else if next == "LOWER" {
        if train {
            next = "HAIR_BLOWING";
            play_anim(target, "hairBlowNormal");
            set_dance_every(target, 0.0);
        } else if cur == "lowerKnife" && finished {
            next = "DEFAULT";
            set_dance_every(target, 1.0);
        }
    } else if next == "HAIR_BLOWING" {
        if !train {
            next = "HAIR_FALLING";
            play_anim(target, "hairFallNormal");
        } else if finished {
            play_anim(target, "hairBlowNormal");
        }
    } else if next == "HAIR_FALLING" {
        if finished {
            next = "DEFAULT";
            set_dance_every(target, 1.0);
        }
    } else if next == "HAIR_BLOWING_RAISE" {
        if !train {
            next = "HAIR_FALLING_RAISE";
            play_anim(target, "hairFallKnife");
        } else if finished {
            play_anim(target, "hairBlowKnife");
        }
    } else if next == "HAIR_FALLING_RAISE" {
        if finished {
            next = "READY";
            set_dance_every(target, 1.0);
        }
    }
    next
}

fn onCreate() {
    import "gameplay/characters/nene/ABotAtlasSprite" as abot_mod;
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    this.nene_state = "DEFAULT";
    this.blink_countdown = 3;
    this.has_danced = false;
    this.last_alpha = -1.0;
    this.abot = abot_mod::new_abot_state("nene-dark");
    this.viz = viz_mod::new_viz_state(false);
    let target = find_slot("nene-dark");
    if target != "" {
        char_underlay(target, "nene", 0.0, 0.0, -1);
        let ab = abot_mod::spawn_abot_with_body_z(
            target,
            -95.0,
            384.0,
            "gameplay/characters/nene-dark/abot-system-dark",
            char_z(target) - 0.15
        );
        viz_mod::spawn_viz(ab[0], ab[1], ab[2], false);
        set_shader("abot_body", "texture_swap");
        shader_tex("abot_body", "gameplay/characters/nene/abot-system/spritemap1.png");
        shader_val("abot_body", "amount", 0.0);
        prop_tint("abot_eye_whites", "#FF6F96CE");
        prop_tint("abot_stereo_bg", "#FF616785");
        let i = 1;
        while i <= 7 {
            set_adjust_color("abot_viz_" + i, -26.0, -45.0, -12.0, 0.0);
            i += 1;
        }
    }
}

fn onSongStart() {
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    this.viz = viz_mod::start_viz(this.viz);
}

fn onSongRetry() {
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    this.nene_state = "DEFAULT";
    this.blink_countdown = 3;
    this.has_danced = false;
    this.viz = viz_mod::reset_viz(this.viz);
    let target = find_slot("nene-dark");
    if target != "" {
        set_dance_every(target, 1.0);
    }
}

fn onSongEnd() {
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    this.viz = viz_mod::reset_viz(this.viz);
}

fn onBeatHit() {
    import "gameplay/characters/nene/ABotAtlasSprite" as abot_mod;
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    if char_id("bf") == "pico-blazin" { return; }
    abot_mod::dance_abot();
    viz_mod::pulse_viz(this.viz);
    let target = find_slot("nene-dark");
    if target == "" { return; }

    if this.nene_state == "PRE_RAISE" {
        set_dance_every(target, 0.0);
        if char_anim(target) != "danceLeft" {
            play_anim(target, "danceLeft");
        }
        this.has_danced = false;
    } else if this.nene_state == "READY" {
        if this.blink_countdown <= 0 {
            play_anim(target, "idleKnife");
            this.blink_countdown = rand_int(3, 7);
        } else {
            this.blink_countdown -= 1;
            set_dance_every(target, 0.0);
        }
    } else if this.nene_state == "LOWER" {
        set_dance_every(target, 0.0);
        if char_anim(target) != "lowerKnife" {
            play_anim(target, "lowerKnife");
        }
    } else if this.nene_state == "DEFAULT" {
        set_dance_every(target, 1.0);
    }
}

fn lerp_channel(a, b, t) {
    let v = (a + (b - a) * t).round();
    if v < 0.0 { 0 } else if v > 255.0 { 255 } else { v.to_int() }
}

fn hex_byte(n) {
    let hex_digits = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "A", "B", "C", "D", "E", "F"];
    let hi = (n / 16) % 16;
    let lo = n % 16;
    hex_digits[hi] + hex_digits[lo]
}

fn sync_abot_darkness(a) {
    shader_val("abot_body", "amount", 1.0 - a);
    let r = lerp_channel(255.0, 111.0, a);
    let g = lerp_channel(255.0, 150.0, a);
    let b = lerp_channel(255.0, 206.0, a);
    prop_tint("abot_eye_whites", "#FF" + hex_byte(r) + hex_byte(g) + hex_byte(b));
}

fn onUpdate() {
    if char_id("bf") == "pico-blazin" { return; }
    let target = find_slot("nene-dark");
    if target == "" { return; }
    let a = clamp(get_char_alpha(target), 0.0, 1.0);
    if a != this.last_alpha {
        this.last_alpha = a;
        sync_abot_darkness(a);
    }
    this.nene_state = transition_nene_state(this.nene_state, target);
}

fn onSongEvent(ev) {
    import "gameplay/characters/nene/ABotAtlasSprite" as abot_mod;
    this.abot = abot_mod::on_FocusCamera(this.abot, ev);
}

fn onNoteHit(ev) {
    import "gameplay/characters/nene/ABotAtlasSprite" as abot_mod;
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    viz_mod::pulse_viz(this.viz);
    let kind = ev.get("kind");
    if kind == "weekend-1-lightcan" {
        this.abot = abot_mod::move_pupils(this.abot, "left");
    } else if kind == "weekend-1-cockgun" {
        this.abot = abot_mod::move_pupils(this.abot, "right");
    }
}
