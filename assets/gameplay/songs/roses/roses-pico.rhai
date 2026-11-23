// Roses (Pico mix) song script (ported from roses-pico.hxc to oxide-engine Rhai)

fn onCreate() {
    this.has_played_cutscene = false;
}

fn onSongRetry() {
    this.has_played_cutscene = true;
}

fn onCountdownStart(ev) {
    if !this.has_played_cutscene {
        this.has_played_cutscene = true;
        ev.cancel();
        set_cutscene(true);
        hud_visible(false);
        sfx("gameplay/dialogue/boxes/roses/angry-appear.ogg", 1.0);
        start_dialogue("roses-pico");
    }
}
