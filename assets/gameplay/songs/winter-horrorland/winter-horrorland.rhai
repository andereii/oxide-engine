// Winter Horrorland song script (ported from winter-horrorland.hxc to oxide-engine Rhai)

fn onCreate() {
    this.has_played_cutscene = false;
    this.waiting_lights = false;
}

fn onSongRetry() {
    this.has_played_cutscene = true;
    this.waiting_lights = false;
    if prop_exists("horror_black") {
        despawn("horror_black");
    }
}

fn onCountdownStart(ev) {
    if !this.has_played_cutscene {
        this.has_played_cutscene = true;
        ev.cancel();
        set_cutscene(true);
        hud_visible(false);
        spawn_hud_rect("horror_black", 2560.0, 1440.0, "#FF000000", 640.0, 360.0, 90.0);
        after(0.1, "play_horror_reveal");
    }
}

fn play_horror_reveal() {
    if prop_exists("horror_black") {
        despawn("horror_black");
    }
    set_cam(400.0, -2050.0);
    reset_cam();
    set_zoom(2.5);
    play_snd("horror_lights_on", "gameplay/stages/mallEvil/sounds/lights-on.ogg", 1.0);
    this.waiting_lights = true;
}

fn onUpdate() {
    if this.waiting_lights && !snd_playing("horror_lights_on") {
        this.waiting_lights = false;
        hud_visible(true, 2.0);
        set_cutscene(false);
        countdown("start");
    }
}
