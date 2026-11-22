// Darnell song script (ported from darnell.hxc to oxide-engine Rhai)

fn cleanup_darnell_cutscene() {
    for id in ["darnell_bg", "darnell_cutscene_can", "darnell_new_can"] {
        if prop_exists(id) {
            despawn(id);
        }
    }
    stop_snd("darnell_cutscene_mus");
}

fn onCreate() {
    this.has_played_video = false;
    this.has_played_in_game = false;
    cleanup_darnell_cutscene();
}

fn onSongRetry() {
    this.has_played_video = true;
    this.has_played_in_game = true;
    cleanup_darnell_cutscene();
}

fn onCountdownStart(ev) {
    if !this.has_played_video {
        this.has_played_video = true;
        ev.cancel();
        set_zoom(1.3);
        spawn_hud_rect("darnell_bg", 2000.0, 2500.0, "#FF000000", 640.0, 360.0, 80.0);
        play_video("gameplay/songs/darnell/story/cutscene.mp4", "starting");
        return;
    }
    if !this.has_played_in_game {
        this.has_played_in_game = true;
        ev.cancel();
        darnell_intro_cutscene();
    }
}

fn darnell_intro_cutscene() {
    set_cutscene(true);
    hud_visible(false);
    set_can_pause(false);

    if !prop_exists("darnell_bg") {
        spawn_hud_rect("darnell_bg", 2000.0, 2500.0, "#FF000000", 640.0, 360.0, 80.0);
    }

    let pile_pos = if prop_exists("spraycanPile") { get_prop_pos("spraycanPile") } else { [920.0, 1045.0] };
    let pile_z = if prop_exists("spraycanPile") { get_prop_z("spraycanPile") } else { 880.0 };

    spawn_sparrow(
        "darnell_cutscene_can",
        "gameplay/songs/darnell/cutscene/cutscene-can",
        pile_pos[0] + 30.0,
        pile_pos[1] - 320.0,
        pile_z - 1.0,
        [
            #{ name: "forward", prefix: "can kick quick0", fps: 24.0, looped: false },
            #{ name: "up", prefix: "can kicked up0", fps: 24.0, looped: false }
        ]
    );
    prop_vis("darnell_cutscene_can", false);

    spawn_atlas(
        "darnell_new_can",
        "gameplay/songs/2hot/spraycan",
        pile_pos[0] - 10.0,
        pile_pos[1] - 550.0,
        pile_z + 1.0,
        [#{ name: "shot", prefix: "can shot", fps: 24.0, looped: false }]
    );
    prop_vis("darnell_new_can", false);

    play_anim("bf", "intro1");

    let delay = 2.0;
    after(0.1, "darnell_step_cam_pico");
    after(0.7, "darnell_step_music");
    after(delay, "darnell_step_cam_all");
    after(delay + 3.0, "darnell_step_light_can");
    after(delay + 4.0, "darnell_step_cock");
    after(delay + 4.4, "darnell_step_kick_up");
    after(delay + 4.9, "darnell_step_knee_forward");
    after(delay + 5.1, "darnell_step_shoot");
    after(delay + 5.9, "darnell_step_dad_laugh");
    after(delay + 6.2, "darnell_step_gf_laugh");
    after(delay + 8.0, "darnell_step_finish");
}

fn darnell_step_cam_pico() {
    let pico_pos = char_focus("bf");
    pan_cam(pico_pos[0] + 250.0, pico_pos[1], 0.0, "linear");
    zoom(1.3, 0.0, "linear");
}

fn darnell_step_music() {
    play_snd("darnell_cutscene_mus", "gameplay/songs/darnell/cutscene/darnell-can-cutscene/darnell-can-cutscene.ogg", 1.0, true);
    if prop_exists("darnell_bg") {
        after(0.3, "darnell_fade_bg");
    }
}

fn darnell_fade_bg() {
    if prop_exists("darnell_bg") {
        tween_prop("darnell_bg", "alpha", 0.0, 2.0, "linear", "darnell_hide_bg");
    }
}

fn darnell_hide_bg() {
    if prop_exists("darnell_bg") {
        despawn("darnell_bg");
    }
}

fn darnell_step_cam_all() {
    let darnell_pos = char_focus("dad");
    pan_cam(darnell_pos[0] + 100.0, darnell_pos[1], 2.5, "quadInOut");
    zoom(0.66, 2.5, "quadInOut");
}

fn darnell_step_light_can() {
    play_anim("dad", "lightCan");
    sfx("gameplay/songs/2hot/sounds/darnell-lighter.ogg", 1.0);
}

fn darnell_step_cock() {
    let darnell_pos = char_focus("dad");
    play_anim("bf", "cock");
    pan_cam(darnell_pos[0] + 180.0, darnell_pos[1], 0.4, "backOut");
    sfx("gameplay/characters/pico-playable/sounds/gun-prep.ogg", 1.0);
}

fn darnell_step_kick_up() {
    play_anim("dad", "kickCan");
    sfx("gameplay/songs/2hot/sounds/kick-can-up.ogg", 1.0);
    if prop_exists("darnell_cutscene_can") {
        prop_vis("darnell_cutscene_can", true);
        play_anim("darnell_cutscene_can", "up");
    }
}

fn darnell_step_knee_forward() {
    play_anim("dad", "kneeCan");
    sfx("gameplay/songs/2hot/sounds/kick-can-forward.ogg", 1.0);
    if prop_exists("darnell_cutscene_can") {
        play_anim("darnell_cutscene_can", "forward");
    }
}

fn darnell_step_shoot() {
    let darnell_pos = char_focus("dad");
    play_anim("bf", "intro2");
    let shot_idx = rand_int(1, 4);
    sfx("gameplay/characters/pico-playable/sounds/shot-" + shot_idx + ".ogg", 1.0);
    pan_cam(darnell_pos[0] + 100.0, darnell_pos[1], 1.0, "quadInOut");
    if prop_exists("darnell_new_can") {
        prop_vis("darnell_new_can", true);
        play_anim("darnell_new_can", "shot");
    }
    if prop_exists("darnell_cutscene_can") {
        prop_vis("darnell_cutscene_can", false);
    }
}

fn darnell_step_dad_laugh() {
    play_anim("dad", "laughCutscene");
    sfx("gameplay/songs/darnell/cutscene/darnell-laugh.ogg", 0.6);
}

fn darnell_step_gf_laugh() {
    play_anim("gf", "laughCutscene");
    sfx("gameplay/songs/darnell/cutscene/nene-laugh.ogg", 0.6);
}

fn darnell_step_finish() {
    let darnell_pos = char_focus("dad");
    zoom(0.77, 2.0, "sineInOut");
    pan_cam(darnell_pos[0] + 180.0, darnell_pos[1], 2.0, "sineInOut");
    stop_snd("darnell_cutscene_mus");
    cleanup_darnell_cutscene();
    set_can_pause(true);
    set_cutscene(false);
    hud_visible(true);
    countdown("start");
}
