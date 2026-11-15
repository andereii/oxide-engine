// Spooky Mansion stage script (ported from spookyMansion.hxc to oxide-engine Rhai)

fn onCreate() {
    this.lightning_strike_beat = 0;
    this.lightning_strike_offset = 8;
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

    anim("halloweenBG", "lightning");
    this.lightning_strike_beat = b;
    this.lightning_strike_offset = rand_int(8, 24);

    bf_anim("scared");
    gf_anim("scared");
}

fn onSongRetry() {
    this.lightning_strike_beat = 0;
    this.lightning_strike_offset = 8;
}
