// Stress (Pico mix) song script (ported from stress-pico.hxc to oxide-engine Rhai)

fn reset_stress_pico_actors() {
    if char_exists("dad") && char_anim("dad") == "redheadsAnim" {
        dance("dad");
    }
    if char_exists("bf") && char_anim("bf") == "knifeToss" {
        dance("bf");
    }
}

fn cleanup_stress_pico_end() {
    if prop_exists("stress_pico_bg") {
        despawn("stress_pico_bg");
    }
}

fn onCreate() {
    this.has_played_cutscene = false;
    this.has_played_end_cutscene = false;
    cleanup_stress_pico_end();
}

fn onSongRetry() {
    this.has_played_cutscene = true;
    this.has_played_end_cutscene = false;
    cleanup_stress_pico_end();
    reset_stress_pico_actors();
}

fn onCountdownStart(ev) {
    if !this.has_played_cutscene {
        this.has_played_cutscene = true;
        ev.cancel();
        play_video("gameplay/songs/stress/pico/cutscene.mkv", "starting");
    }
}

fn onSongEnd(ev) {
    if !this.has_played_end_cutscene {
        this.has_played_end_cutscene = true;
        ev.cancel();
        start_stress_pico_end_cutscene();
    } else {
        this.has_played_end_cutscene = false;
    }
}

fn start_stress_pico_end_cutscene() {
    set_cutscene(true);
    hud_visible(false);
    spawn_hud_rect("stress_pico_bg", 2000.0, 2500.0, "#00000000", 640.0, 360.0, 88.0);

    let tankman_pos = char_focus("dad");
    pan_cam(tankman_pos[0] + 320.0, tankman_pos[1] - 70.0, 2.8, "expoOut");
    zoom(0.65, 2.0, "expoOut");

    play_anim("dad", "stressPicoEnding");
    play_snd("stress_pico_end_sfx", "gameplay/songs/stress/pico/sounds/end-cutscene.ogg", 1.0);
    subtitle_srt("gameplay/songs/stress/subtitles/end-cutscene-pico.srt");

    after(176.0 / 24.0, "stress_pico_laugh_end");
    after(270.0 / 24.0, "stress_pico_fade_out");
    after(320.0 / 24.0, "stress_pico_finish_end");
}

fn stress_pico_laugh_end() {
    play_anim("bf", "laughEnd");
}

fn stress_pico_fade_out() {
    let tankman_pos = char_focus("dad");
    pan_cam(tankman_pos[0] + 320.0, tankman_pos[1] - 370.0, 2.0, "quadInOut");
    if prop_exists("stress_pico_bg") {
        tween_prop("stress_pico_bg", "alpha", 1.0, 2.0, "linear");
    }
}

fn stress_pico_finish_end() {
    set_cutscene(false);
    end_song();
}
