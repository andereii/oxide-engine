// Pico (Playable) character script (ported from pico-playable.hxc)

fn pick_pico_death_quote() {
    let blocked = [4, 7, 8, 9];
    let choice = rand_int(1, 9);
    let tries = 0;
    while (choice in blocked) && tries < 16 {
        choice = rand_int(1, 9);
        tries += 1;
    }
    "gameplay/characters/pico-holding-nene/sounds/jeff-gameover-" + choice
}

fn handle_pico_note_hit(target, ev) {
    let kind = ev.get("kind");
    if kind == "hey" {
        play_anim(target, "hey");
    } else if kind == "cheer" {
        play_anim(target, "cheer");
    } else if kind == "censor" {
        let dir = ev.get("dir");
        if dir != () {
            play_sing(target, dir, false, "censor");
        }
    } else if kind == "weekend-1-cockgun" {
        play_anim(target, "cock");
        sfx("gameplay/characters/pico-playable/sounds/gun-prep", 1.0);
        return true;
    } else if kind == "weekend-1-firegun" {
        play_anim(target, "shoot");
        let n = rand_int(1, 4);
        sfx("gameplay/characters/pico-playable/sounds/shot-" + n, 1.0);
    }
    false
}

fn handle_pico_note_miss(target, ev) {
    let kind = ev.get("kind");
    if kind == "weekend-1-firegun" {
        play_anim(target, "shootMISS");
        sfx("gameplay/characters/pico-playable/sounds/bonk", 1.0);
    }
}

fn onCreate() {
    import "./CasingSprite" as casing_mod;
    import "./PicoBloodPool" as blood_mod;
    import "./PicoDopplegangerSprite" as dopple_mod;
    this.casing = casing_mod::new_casing_state();
    this.blood_pool = blood_mod::new_blood_pool_state(0.0, 0.0);
    this.doppelganger_asset = dopple_mod::doppelganger_asset();
    global_set("gameover_music_suffix", "-pico");
    global_set("gameover_blueball_suffix", "-pico");
    global_set("pause_music_suffix", "-pico");
}

fn onSongRetry() {
    import "./CasingSprite" as casing_mod;
    this.casing = casing_mod::clear_casings();
    global_set("gameover_music_suffix", "-pico");
    global_set("gameover_blueball_suffix", "-pico");
    global_set("pause_music_suffix", "-pico");
}

fn onUpdate() {
    import "./PicoBloodPool" as blood_mod;
    this.blood_pool = blood_mod::update_blood_pool(this.blood_pool, dt());
}

fn onNoteHit(ev) {
    if char_id("bf") == "pico-playable" {
        if handle_pico_note_hit("bf", ev) { this.casing.pending_rolls += 1; }
    }
}

fn onOpponentNoteHit(ev) {
    if char_id("dad") == "pico-playable" {
        if handle_pico_note_hit("dad", ev) { this.casing.pending_rolls += 1; }
    }
}

fn onNoteMiss(ev) {
    if char_id("bf") == "pico-playable" {
        handle_pico_note_miss("bf", ev);
    }
}

fn onGameOver() {
    if char_id("dad") == "tankman" {
        global_set("death_quote_sfx", pick_pico_death_quote());
    }
}
