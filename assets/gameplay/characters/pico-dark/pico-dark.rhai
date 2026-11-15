// Pico (Dark) character script (ported from pico-dark.hxc)
import "gameplay/characters/pico-playable/pico-playable" as pico_base;
import "gameplay/characters/pico-playable/CasingSprite" as casing_mod;

fn onCreate() {
    this.casing = casing_mod::new_casing_state();
    this.last_anim = "";
    global_set("gameover_music_suffix", "-pico");
    global_set("gameover_blueball_suffix", "-pico");
    global_set("pause_music_suffix", "-pico");
}

fn onSongRetry() {
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
    if char_id("bf") == "pico-dark" {
        if pico_base::handle_pico_note_hit("bf", ev) { this.casing.pending_rolls += 1; }
    }
}

fn onOpponentNoteHit(ev) {
    if char_id("dad") == "pico-dark" {
        if pico_base::handle_pico_note_hit("dad", ev) { this.casing.pending_rolls += 1; }
    }
}

fn onNoteMiss(ev) {
    if char_id("bf") == "pico-dark" {
        pico_base::handle_pico_note_miss("bf", ev);
    }
}
