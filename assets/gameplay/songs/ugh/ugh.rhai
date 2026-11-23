// Ugh song script (ported from ugh.hxc to oxide-engine Rhai)

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
        play_video("gameplay/songs/ugh/cutscene.mkv", "starting");
    }
}
