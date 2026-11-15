// Nene (Pixel) character script (ported from nene-pixel.hxc)
import "gameplay/characters/nene/ABotAtlasSprite" as abot_mod;
import "gameplay/characters/nene/ABotVisualizer" as viz_mod;

fn find_slot(id) {
    if char_id("gf") == id { return "gf"; }
    if char_id("dad") == id { return "dad"; }
    if char_id("bf") == id { return "bf"; }
    ""
}

fn onCreate() {
    this.nene_state = "DEFAULT";
    this.blink_countdown = 3;
    this.abot = abot_mod::new_abot_state("nene-pixel");
    this.viz = viz_mod::new_viz_state(true);
    let target = find_slot("nene-pixel");
    if target != "" {
        let pos = char_pos(target);
        spawn_sprite("abot_pixel_back", "gameplay/characters/nene-pixel/abot-pixel/abot-pixel-back.png", pos[0], pos[1], 38.0);
        prop_scale("abot_pixel_back", 6.1, 6.0);
    }
}

fn onSongStart() {
    this.viz = viz_mod::start_viz(this.viz);
}

fn onSongRetry() {
    this.nene_state = "DEFAULT";
    this.blink_countdown = 3;
    this.viz = viz_mod::reset_viz(this.viz);
    let target = find_slot("nene-pixel");
    if target != "" {
        set_dance_every(target, 1.0);
    }
}

fn onSongEnd() {
    this.viz = viz_mod::reset_viz(this.viz);
}

fn onBeatHit() {
    let target = find_slot("nene-pixel");
    if target == "" { return; }

    let h = hp();
    let v_thresh = 0.5;
    if this.nene_state == "DEFAULT" && h <= v_thresh {
        this.nene_state = "PRE_RAISE";
    }
    if this.nene_state == "PRE_RAISE" {
        if h > v_thresh {
            this.nene_state = "DEFAULT";
        } else {
            play_anim(target, "danceLeft");
        }
    } else if this.nene_state == "READY" {
        if h > v_thresh {
            this.nene_state = "LOWER";
            set_dance_every(target, 0.0);
            play_anim(target, "lowerKnife");
        } else if this.blink_countdown <= 0 {
            play_anim(target, "idleKnifeBlink");
            this.blink_countdown = rand_int(3, 7);
        } else {
            this.blink_countdown -= 1;
            set_dance_every(target, 0.0);
        }
    }
}

fn onUpdate() {
    let target = find_slot("nene-pixel");
    if target == "" { return; }
    let cur = char_anim(target);
    let finished = char_anim_finished(target);

    if this.nene_state == "PRE_RAISE" && cur == "danceLeft" && finished {
        this.nene_state = "RAISE";
        set_dance_every(target, 0.0);
        play_anim(target, "raiseKnife");
    } else if this.nene_state == "RAISE" && cur == "raiseKnife" && finished {
        this.nene_state = "READY";
    } else if this.nene_state == "LOWER" && cur == "lowerKnife" && finished {
        this.nene_state = "DEFAULT";
        set_dance_every(target, 1.0);
    }
}

fn onSongEvent(ev) {
    this.abot = abot_mod::on_FocusCamera(this.abot, ev);
}
