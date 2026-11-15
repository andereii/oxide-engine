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
    this.abot = abot_mod::new_abot_state("nene-dark");
    this.viz = viz_mod::new_viz_state(false);
    let target = find_slot("nene-dark");
    if target != "" {
        let pos = char_pos(target);
        spawn_sprite("abot_stereo_bg", "gameplay/characters/nene/stereo-bg.png", pos[0] + 55.0, pos[1] + 414.0, 40.0);
    prop_tint("abot_stereo_bg", "#FF616785");
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
    if char_id("bf") == "pico-blazin" { return; }
    let target = find_slot("nene-dark");
    if target == "" { return; }

    if this.nene_state == "PRE_RAISE" {
        play_anim(target, "danceLeft");
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

fn onUpdate() {
    if char_id("bf") == "pico-blazin" { return; }
    let target = find_slot("nene-dark");
    if target == "" { return; }
    this.nene_state = transition_nene_state(this.nene_state, target);
}

fn onSongEvent(ev) {
    import "gameplay/characters/nene/ABotAtlasSprite" as abot_mod;
    this.abot = abot_mod::on_FocusCamera(this.abot, ev);
}

fn onNoteHit(ev) {
    let kind = ev.get("kind");
    if kind == "weekend-1-lightcan" {
        this.abot.pupil_state = "left";
    } else if kind == "weekend-1-cockgun" {
        this.abot.pupil_state = "right";
    }
}
