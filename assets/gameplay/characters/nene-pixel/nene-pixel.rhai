// Nene (Pixel) character script (ported from nene-pixel.hxc)

fn find_slot(id) {
    if char_id("gf") == id { return "gf"; }
    if char_id("dad") == id { return "dad"; }
    if char_id("bf") == id { return "bf"; }
    ""
}

fn onCreate() {
    import "gameplay/characters/nene/ABotAtlasSprite" as abot_mod;
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    this.nene_state = "DEFAULT";
    this.blink_countdown = 3;
    this.abot_danced = false;
    this.abot = abot_mod::new_abot_state("nene-pixel");
    this.viz = viz_mod::new_viz_state(true);
    let target = find_slot("nene-pixel");
    if target != "" {
        let tl = char_top_left(target);
        let gz = char_z(target);
        let ax = tl[0] + 296.0;
        let ay = tl[1] + 430.0;

        spawn_sparrow(
            "abot_pixel_head",
            "gameplay/characters/nene-pixel/abot-pixel/abot-pixel-head",
            ax - 325.0,
            ay + 72.0,
            gz - 0.10,
            [
                #{ name: "toleft", prefix: "toleft0", fps: 24.0, looped: false },
                #{ name: "toright", prefix: "toright0", fps: 24.0, looped: false }
            ]
        );
        prop_scale("abot_pixel_head", 6.0, 6.0);
        play_anim_frame("abot_pixel_head", "toright", 5);

        spawn_sparrow(
            "abot_pixel_speaker",
            "gameplay/characters/nene-pixel/abot-pixel/abot-pixel-speaker",
            ax - 78.0,
            ay + 9.0,
            gz - 0.08,
            [#{ name: "idle", prefix: "bop", fps: 24.0, looped: false }]
        );
        prop_scale("abot_pixel_speaker", 6.0, 6.0);

        spawn_sprite_tl(
            "abot_pixel_back",
            "gameplay/characters/nene-pixel/abot-pixel/abot-pixel-back.png",
            ax - 55.0,
            ay,
            gz - 0.06
        );
        prop_scale("abot_pixel_back", 6.1, 6.0);

        viz_mod::spawn_viz(ax, ay, gz, true);

        spawn_sparrow(
            "abot_pixel_body",
            "gameplay/characters/nene-pixel/abot-pixel/abot-pixel-body",
            ax,
            ay,
            gz - 0.02,
            [#{ name: "idle", prefix: "bop", fps: 24.0, looped: false }]
        );
        prop_scale("abot_pixel_body", 6.0, 6.0);
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
    this.viz = viz_mod::reset_viz(this.viz);
    let target = find_slot("nene-pixel");
    if target != "" {
        set_dance_every(target, 1.0);
    }
}

fn onSongEnd() {
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    this.viz = viz_mod::reset_viz(this.viz);
}

fn onBeatHit() {
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    play_anim("abot_pixel_speaker", "idle");
    play_anim("abot_pixel_body", "idle");
    viz_mod::pulse_viz(this.viz);
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
    if ev == () { return; }
    if ev.get("kind") != "FocusCamera" { return; }
    let val = ev.get("value");
    if val == () { return; }
    let ch = val["char"];
    if ch == 0 && this.abot.pupil_state != "right" {
        this.abot.pupil_state = "right";
        play_anim("abot_pixel_head", "toright");
    } else if ch == 1 && this.abot.pupil_state != "left" {
        this.abot.pupil_state = "left";
        play_anim("abot_pixel_head", "toleft");
    }
}

fn onNoteHit(ev) {
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    viz_mod::pulse_viz(this.viz);
}
