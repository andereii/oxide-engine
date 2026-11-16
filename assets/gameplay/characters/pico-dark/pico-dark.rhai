// Pico (Dark) character script (ported from pico-dark.hxc)

fn find_slot(id) {
    if char_id("bf") == id { return "bf"; }
    if char_id("dad") == id { return "dad"; }
    if char_id("gf") == id { return "gf"; }
    ""
}

fn onCreate() {
    import "gameplay/characters/pico-playable/CasingSprite" as casing_mod;
    this.casing = casing_mod::new_casing_state();
    this.last_anim = "";
    global_set("gameover_music_suffix", "-pico");
    global_set("gameover_blueball_suffix", "-pico");
    global_set("pause_music_suffix", "-pico");
    let target = find_slot("pico-dark");
    if target != "" {
        char_underlay(target, "pico-playable");
    }
}

fn onSongRetry() {
    import "gameplay/characters/pico-playable/CasingSprite" as casing_mod;
    this.casing = casing_mod::clear_casings();
}

fn onUpdate() {
    let cur = char_anim("bf");
    if cur != this.last_anim {
        this.last_anim = cur;
        global_set("pico_dark_last_anim", cur);
    }
}

fn onNoteHit(ev) {
    import "gameplay/characters/pico-playable/pico-playable" as pico_base;
    if char_id("bf") == "pico-dark" {
        if pico_base::handle_pico_note_hit("bf", ev) { this.casing.pending_rolls += 1; }
    }
}

fn onOpponentNoteHit(ev) {
    import "gameplay/characters/pico-playable/pico-playable" as pico_base;
    if char_id("dad") == "pico-dark" {
        if pico_base::handle_pico_note_hit("dad", ev) { this.casing.pending_rolls += 1; }
    }
}

fn onNoteMiss(ev) {
    import "gameplay/characters/pico-playable/pico-playable" as pico_base;
    if char_id("bf") == "pico-dark" {
        pico_base::handle_pico_note_miss("bf", ev);
    }
}
