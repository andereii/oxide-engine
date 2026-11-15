// Boyfriend character script (ported from bf.hxc)

fn find_slot(id) {
    if char_id("bf") == id { return "bf"; }
    if char_id("dad") == id { return "dad"; }
    if char_id("gf") == id { return "gf"; }
    ""
}

fn pick_tankman_death_quote() {
    let blocked = [1, 3, 8, 13, 17, 21];
    let choice = rand_int(1, 24);
    let tries = 0;
    while (choice in blocked) && tries < 16 {
        choice = rand_int(1, 24);
        tries += 1;
    }
    "gameplay/characters/bf-holding-gf/sounds/jeff-gameover-" + choice
}

fn onCreate() {
    this.fakeout_active = false;
    this.fakeout_sound_played = false;
    this.slot = find_slot("bf");
}

fn onSongRetry() {
    this.fakeout_active = false;
    this.fakeout_sound_played = false;
}

fn onUpdate() {
    let target = if this.slot == "" { find_slot("bf") } else { this.slot };
    if target == "" { return; }
    let cur = char_anim(target);
    if cur == "fakeoutDeath" {
        if !this.fakeout_sound_played {
            sfx("gameplay/playable-characters/bf/game-over/loss-sfx-fakeout", 1.0);
            this.fakeout_sound_played = true;
            this.fakeout_active = true;
        }
        if char_anim_finished(target) && this.fakeout_active {
            this.fakeout_active = false;
            play_anim(target, "firstDeath");
        }
    }
}

fn onGameOver() {
    if char_id("dad") == "tankman" {
        global_set("death_quote_sfx", pick_tankman_death_quote());
    }
}
