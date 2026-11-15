// Pico Holding Nene character script (ported from pico-holding-nene.hxc)

fn pick_pico_nene_quote() {
    let blocked = [4, 7, 8, 9];
    let choice = rand_int(1, 9);
    let tries = 0;
    while (choice in blocked) && tries < 16 {
        choice = rand_int(1, 9);
        tries += 1;
    }
    "gameplay/characters/pico-holding-nene/sounds/jeff-gameover-" + choice
}

fn onCreate() {
    this.last_looped = "";
    global_set("gameover_music_suffix", "-pico");
    global_set("gameover_blueball_suffix", "-pico-and-nene");
    global_set("pause_music_suffix", "-pico");
}

fn onSongRetry() {
    this.last_looped = "";
    global_set("gameover_music_suffix", "-pico");
    global_set("gameover_blueball_suffix", "-pico-and-nene");
    global_set("pause_music_suffix", "-pico");
}

fn onUpdate() {
    if char_id("bf") != "pico-holding-nene" { return; }
    if !char_anim_finished("bf") { return; }
    let cur = char_anim("bf");
    if cur == "deathConfirm" && this.last_looped != "deathConfirm-loop" {
        this.last_looped = "deathConfirm-loop";
        play_anim("bf", "deathConfirm-loop");
    } else if cur == "laughEnd" && this.last_looped != "laughEnd-loop" {
        this.last_looped = "laughEnd-loop";
        play_anim("bf", "laughEnd-loop");
    }
}

fn onGameOver() {
    global_set("death_quote_sfx", pick_pico_nene_quote());
}
