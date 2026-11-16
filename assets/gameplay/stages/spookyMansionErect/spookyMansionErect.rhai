// Spooky Mansion (Erect) stage script (ported from spookyMansionErect.hxc to oxide-engine Rhai)

fn reset_lightning(alpha_val) {
    cancel_timer("lightning_off");
    cancel_timer("lightning_fade");
    cancel_tweens("bgLight");
    cancel_tweens("stairsLight");
    prop_alpha("bgLight", alpha_val);
    prop_alpha("stairsLight", alpha_val);
    char_alpha("bf", 1.0);
    char_alpha("dad", 1.0);
    char_alpha("gf", 1.0);
    prop_alpha("abot_body", 1.0);
}

fn onCreate() {
    this.lightning_strike_beat = 0;
    this.lightning_strike_offset = 8;
    this.char_fading = false;
    this.char_fade_elapsed = 0.0;
    reset_lightning(0.0001);
}

fn lightning_off() {
    cancel_tweens("bgLight");
    cancel_tweens("stairsLight");
    prop_alpha("bgLight", 0.0);
    prop_alpha("stairsLight", 0.0);
    this.char_fading = false;
    char_alpha("bf", 1.0);
    char_alpha("dad", 1.0);
    char_alpha("gf", 1.0);
    prop_alpha("abot_body", 1.0);
}

fn lightning_fade() {
    cancel_tweens("bgLight");
    cancel_tweens("stairsLight");
    prop_alpha("bgLight", 1.0);
    prop_alpha("stairsLight", 1.0);
    this.char_fading = true;
    this.char_fade_elapsed = 0.0;
    char_alpha("bf", 0.0);
    char_alpha("dad", 0.0);
    char_alpha("gf", 0.0);
    prop_alpha("abot_body", 0.0);
    tween_prop("bgLight", "alpha", 0.0, 1.5, "linear");
    tween_prop("stairsLight", "alpha", 0.0, 1.5, "linear");
}

fn onUpdate() {
    if this.char_fading {
        this.char_fade_elapsed += dt();
        let a = clamp(this.char_fade_elapsed / 1.5, 0.0, 1.0);
        char_alpha("bf", a);
        char_alpha("dad", a);
        char_alpha("gf", a);
        prop_alpha("abot_body", a);
        if a >= 1.0 {
            this.char_fading = false;
        }
    }
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

    cancel_timer("lightning_off");
    cancel_timer("lightning_fade");
    cancel_tweens("bgLight");
    cancel_tweens("stairsLight");
    this.char_fading = false;

    prop_alpha("bgLight", 1.0);
    prop_alpha("stairsLight", 1.0);
    char_alpha("bf", 0.0);
    char_alpha("dad", 0.0);
    char_alpha("gf", 0.0);

    after(0.06, "lightning_off");
    after(0.12, "lightning_fade");

    this.lightning_strike_beat = b;
    this.lightning_strike_offset = rand_int(8, 24);

    bf_anim("scared");
    gf_anim("scared");
}

fn onCountdownStart() {
    this.lightning_strike_beat = 0;
    this.lightning_strike_offset = 8;
}

fn onGameOver() {
    this.char_fading = false;
    reset_lightning(0.0);
}

fn onSongRetry() {
    this.lightning_strike_beat = 0;
    this.lightning_strike_offset = 8;
    this.char_fading = false;
    reset_lightning(0.0001);
}
