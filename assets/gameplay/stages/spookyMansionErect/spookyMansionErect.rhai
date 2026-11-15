// Spooky Mansion (Erect) stage script (ported from spookyMansionErect.hxc to oxide-engine Rhai)

fn onCreate() {
    this.lightning_strike_beat = 0;
    this.lightning_strike_offset = 8;
    prop_alpha("bgLight", 0.0001);
    prop_alpha("stairsLight", 0.0001);
    set_rain(7.2, 0.4, "#6680CC");
}

fn lightning_off() {
    prop_alpha("bgLight", 0.0);
    prop_alpha("stairsLight", 0.0);
    char_tint("bf", "#FFFFFFFF");
    char_tint("dad", "#FFFFFFFF");
    char_tint("gf", "#FFFFFFFF");
}

fn lightning_fade() {
    prop_alpha("bgLight", 1.0);
    prop_alpha("stairsLight", 1.0);
    char_tint("bf", "#00FFFFFF");
    char_tint("dad", "#00FFFFFF");
    char_tint("gf", "#00FFFFFF");
    tween_prop("bgLight", "alpha", 0.0, 1.5, "linear");
    tween_prop("stairsLight", "alpha", 0.0, 1.5, "linear");
    after(0.15, "restore_chars");
}

fn restore_chars() {
    char_tint("bf", "#FFFFFFFF");
    char_tint("dad", "#FFFFFFFF");
    char_tint("gf", "#FFFFFFFF");
}

fn onBeatHit() {
    let b = beat();
    let strike = false;
    let play_sound = true;

    if b == 4 && song() == "spookeez" {
        strike = true;
        play_sound = false;
    } else if rand_int(0, 99) < 10 && b > (this.lightning_strike_beat + this.lightning_strike_offset) {
        strike = true;
        play_sound = true;
    }

    if !strike {
        return;
    }

    if play_sound {
        let idx = rand_int(1, 2);
        sfx("gameplay/stages/spookyMansion/sounds/thunder-" + idx + ".ogg", 1.0);
    }

    prop_alpha("bgLight", 1.0);
    prop_alpha("stairsLight", 1.0);
    char_tint("bf", "#00FFFFFF");
    char_tint("dad", "#00FFFFFF");
    char_tint("gf", "#00FFFFFF");

    after(0.06, "lightning_off");
    after(0.12, "lightning_fade");

    this.lightning_strike_beat = b;
    this.lightning_strike_offset = rand_int(8, 24);

    bf_anim("scared");
    gf_anim("scared");
}

fn onSongRetry() {
    this.lightning_strike_beat = 0;
    this.lightning_strike_offset = 8;
    prop_alpha("bgLight", 0.0001);
    prop_alpha("stairsLight", 0.0001);
    restore_chars();
}
