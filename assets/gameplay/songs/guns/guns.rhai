// Guns song script (ported from guns.hxc to oxide-engine Rhai)

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
        play_video("gameplay/songs/guns/cutscene.mkv", "starting");
    }
}
