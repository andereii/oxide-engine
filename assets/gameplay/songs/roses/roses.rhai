// Roses song script (ported from roses.hxc to oxide-engine Rhai)

fn cleanup_roses_outro_prop() {
    cancel_tweens("roses_outro_red");
    if prop_exists("roses_outro_red") {
        despawn("roses_outro_red");
    }
}

fn onCreate() {
    this.outro_started = false;
    this.has_played_cutscene = false;
    cleanup_roses_outro_prop();
}

fn onSongRetry() {
    this.outro_started = false;
    this.has_played_cutscene = true;
    cleanup_roses_outro_prop();
}

fn onCountdownStart(ev) {
    if variation() == "pico" { return; }
    if !this.has_played_cutscene {
        this.has_played_cutscene = true;
        ev.cancel();
        set_cutscene(true);
        hud_visible(false);
        sfx("gameplay/dialogue/boxes/roses/angry-appear.ogg", 1.0);
        start_dialogue("roses");
    }
}

fn onBeatHit() {
    if variation() == "pico" { return; }
    if beat() == 180 && story_mode() && !this.outro_started {
        this.outro_started = true;
        spawn_hud_rect("roses_outro_red", 2400.0, 1600.0, "#FFFF1B31", 640.0, 360.0, 100.0);
        prop_alpha("roses_outro_red", 0.0);
        tween_prop("roses_outro_red", "alpha", 1.0, 2.0, "linear");
    }
}
