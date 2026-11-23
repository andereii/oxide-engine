// Eggnog (Erect) song script (ported from eggnog-erect.hxc to oxide-engine Rhai)

fn cleanup_eggnog_cutscene() {
    for id in ["eggnog_santa_dead", "eggnog_parents_shoot", "eggnog_skip_txt"] {
        if prop_exists(id) {
            despawn(id);
        }
    }
    stop_snd("eggnog_santa_snd");
    stop_snd("eggnog_shoot_snd");
    subtitle_clear();
}

fn onCreate() {
    this.has_played_cutscene = false;
    this.cutscene_skipped = false;
    this.can_skip = false;
    cleanup_eggnog_cutscene();
}

fn onSongRetry() {
    this.cutscene_skipped = false;
    this.can_skip = false;
    char_vis("dad", true);
    cleanup_eggnog_cutscene();
}

fn onSongEnd(ev) {
    if !this.has_played_cutscene {
        this.has_played_cutscene = true;
        ev.cancel();
        start_eggnog_cutscene();
    } else {
        this.has_played_cutscene = false;
    }
}

fn start_eggnog_cutscene() {
    set_cutscene(true);
    hud_visible(false);
    this.cutscene_skipped = false;
    this.can_skip = false;

    spawn_hud_text("eggnog_skip_txt", "Skip [Enter]", 1248.0, 696.0, 90.0, 28.0, "#FFFFFFFF", "br");
    text_font("eggnog_skip_txt", "VCR OSD Mono.ttf");
    set_prop("eggnog_skip_txt", "alpha", 0.0);

    let santa_z = if prop_exists("santa") {
        prop_vis("santa", false);
        get_prop_z("santa")
    } else {
        10.0
    };

    spawn_atlas(
        "eggnog_santa_dead",
        "gameplay/stages/mallXmasErect/cutscene/santa-speaks",
        -1300.0,
        100.0,
        santa_z - 0.05,
        []
    );
    char_vis("dad", false);
    spawn_atlas(
        "eggnog_parents_shoot",
        "gameplay/stages/mallXmasErect/cutscene/parents-shoot",
        -602.0,
        -3.5,
        santa_z - 0.1,
        []
    );

    pan_cam(-100.0, 400.0, 2.8, "expoOut");
    zoom(0.73, 2.0, "quadInOut");

    play_snd("eggnog_santa_snd", "gameplay/stages/mallXmasErect/cutscene/santa-speaks.ogg", 1.0);
    subtitle_srt("gameplay/songs/eggnog/subtitles/erect/santa-emotions.srt");

    after(2.8, "eggnog_step_pan2");
    after(11.375, "eggnog_step_shoot");
    after(12.83, "eggnog_step_impact");
    after(15.0, "eggnog_step_fade_out");
    after(16.0, "eggnog_step_finish");
}

fn eggnog_step_pan2() {
    if this.cutscene_skipped { return; }
    pan_cam(-250.0, 400.0, 9.0, "quartInOut");
    zoom(0.79, 9.0, "quadInOut");
}

fn eggnog_step_shoot() {
    if this.cutscene_skipped { return; }
    play_snd("eggnog_shoot_snd", "gameplay/stages/mallXmasErect/cutscene/parents-shoot.ogg", 1.0);
}

fn eggnog_step_impact() {
    if this.cutscene_skipped { return; }
    shake(0.005, 0.2);
    pan_cam(-240.0, 480.0, 5.0, "expoOut");
}

fn eggnog_step_fade_out() {
    if this.cutscene_skipped { return; }
    fade("#FF000000", 1.0, false, true);
}

fn eggnog_step_finish() {
    if this.cutscene_skipped { return; }
    fade("#FF000000", 0.5, true, true);
    cleanup_eggnog_cutscene();
    set_cutscene(false);
    end_song();
}

fn eggnog_enable_skip() {
    this.can_skip = true;
}

fn eggnog_do_skip_end() {
    cleanup_eggnog_cutscene();
    set_cutscene(false);
    end_song();
}

fn onUpdate() {
    if !in_cutscene() || this.cutscene_skipped { return; }
    if ctrl_hit("ACCEPT") || key_hit("Enter") || key_hit("Space") {
        if !this.can_skip {
            tween("eggnog_skip_txt", "alpha", 1.0, 0.5, "quadOut");
            after(0.5, "eggnog_enable_skip");
        } else {
            this.cutscene_skipped = true;
            stop_snd("eggnog_santa_snd");
            stop_snd("eggnog_shoot_snd");
            fade("#FF000000", 0.5, false, true);
            tween("eggnog_skip_txt", "alpha", 0.0, 0.5, "quadOut");
            after(0.5, "eggnog_do_skip_end");
        }
    }
}
