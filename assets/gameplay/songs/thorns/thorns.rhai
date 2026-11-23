// Thorns song script (ported from thorns.hxc to oxide-engine Rhai)

fn cleanup_thorns_cutscene() {
    for id in ["thorns_red", "senpai_crazy", "thorns_white", "thorns_black"] {
        if prop_exists(id) {
            despawn(id);
        }
    }
}

fn onCreate() {
    this.has_played_cutscene = false;
    this.crazy_alpha = 0.0;
    this.waiting_die_sfx = false;
    cleanup_thorns_cutscene();
}

fn onSongRetry() {
    this.has_played_cutscene = true;
    this.waiting_die_sfx = false;
    cleanup_thorns_cutscene();
}

fn onCountdownStart(ev) {
    if !this.has_played_cutscene {
        this.has_played_cutscene = true;
        ev.cancel();
        set_cutscene(true);
        hud_visible(false);
        lock_keys(true);
        spawn_hud_rect("thorns_red", 2000.0, 1500.0, "#FFFF1B31", 640.0, 360.0, 88.0);
        spawn_sparrow(
            "senpai_crazy",
            "gameplay/songs/thorns/cutscene/senpai-fucking-dies",
            720.0,
            360.0,
            89.0,
            [#{ name: "idle", prefix: "Senpai Pre Explosion instance 1", fps: 24.0, looped: false }],
            true
        );
        set_prop("senpai_crazy", "scale", 6.0);
        prop_alpha("senpai_crazy", 0.0);
        this.crazy_alpha = 0.0;
        every(0.3, "thorns_fade_in_crazy", 7);
    }
}

fn thorns_fade_in_crazy() {
    this.crazy_alpha += 0.15;
    if this.crazy_alpha < 1.0 {
        if prop_exists("senpai_crazy") {
            prop_alpha("senpai_crazy", this.crazy_alpha);
        }
    } else {
        cancel_timer("thorns_fade_in_crazy");
        if prop_exists("senpai_crazy") {
            prop_alpha("senpai_crazy", 1.0);
            play_anim("senpai_crazy", "idle");
        }
        play_snd("thorns_die", "gameplay/songs/thorns/cutscene/senpai-fucking-dies.ogg", 1.0);
        this.waiting_die_sfx = true;
        after(3.2, "thorns_white_flash");
    }
}

fn thorns_white_flash() {
    spawn_hud_rect("thorns_white", 2000.0, 1500.0, "#00FFFFFF", 640.0, 360.0, 91.0);
    tween_prop("thorns_white", "alpha", 1.0, 1.4, "linear", "thorns_to_black");
}

fn thorns_to_black() {
    if prop_exists("senpai_crazy") {
        despawn("senpai_crazy");
    }
    if prop_exists("thorns_red") {
        prop_tint("thorns_red", "#FFFFFFFF");
    }
    if prop_exists("thorns_white") {
        tween_prop("thorns_white", "alpha", 0.0, 0.8, "linear");
    }
}

fn onUpdate() {
    if this.waiting_die_sfx && !snd_playing("thorns_die") {
        this.waiting_die_sfx = false;
        if prop_exists("thorns_red") { despawn("thorns_red"); }
        if prop_exists("thorns_white") { despawn("thorns_white"); }
        spawn_hud_rect("thorns_black", 2000.0, 1500.0, "#FF000000", 640.0, 360.0, 92.0);
        tween_prop("thorns_black", "alpha", 0.0, 1.4, "linear");
        after(1.6, "thorns_start_dlg");
    }
}

fn thorns_start_dlg() {
    if prop_exists("thorns_black") {
        despawn("thorns_black");
    }
    lock_keys(false);
    start_dialogue("thorns");
}
