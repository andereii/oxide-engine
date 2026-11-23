// Senpai (Pico mix) song script (ported from senpai-pico.hxc + senpai-pico.hxc conversation to oxide-engine Rhai)

fn onCreate() {
    this.has_played_cutscene = false;
    this.fade_step = 0;
}

fn onSongRetry() {
    this.has_played_cutscene = true;
    if prop_exists("senpai_black") {
        despawn("senpai_black");
    }
}

fn onCountdownStart(ev) {
    if !this.has_played_cutscene {
        this.has_played_cutscene = true;
        ev.cancel();
        set_cutscene(true);
        hud_visible(false);
        lock_keys(true);
        spawn_hud_rect("senpai_black", 2000.0, 1500.0, "#FF000000", 640.0, 360.0, 90.0);
        this.fade_step = 0;
        after(0.25, "senpai_start_fade");
    }
}

fn senpai_start_fade() {
    every(2.0 / 12.0, "senpai_fade_tick", 12);
}

fn senpai_fade_tick() {
    this.fade_step += 1;
    let a = 1.0 - (this.fade_step / 12.0);
    if a < 0.0 { a = 0.0; }
    if prop_exists("senpai_black") {
        prop_alpha("senpai_black", a);
    }
    if this.fade_step >= 12 {
        if prop_exists("senpai_black") {
            despawn("senpai_black");
        }
        lock_keys(false);
        start_dialogue("senpai-pico");
    }
}

fn onDialogueLine(ev) {
    let spk = dialogue_speaker();
    if spk == "senpai-bwuh" {
        dialogue_music(false);
    } else if spk == "senpai" {
        dialogue_music(true);
    }
}
