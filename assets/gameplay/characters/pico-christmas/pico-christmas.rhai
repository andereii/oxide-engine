// Pico (Christmas) character script (ported from pico-christmas.hxc)
import "gameplay/characters/pico-playable/pico-playable" as pico_base;
import "gameplay/characters/pico-playable/CasingSprite" as casing_mod;

fn onCreate() {
    this.casing = casing_mod::new_casing_state();
    global_set("gameover_music_suffix", "-pico");
    global_set("gameover_blueball_suffix", "-pico");
    global_set("pause_music_suffix", "-pico");
}

fn onSongRetry() {
    this.casing = casing_mod::clear_casings();
}

fn onNoteHit(ev) {
    if char_id("bf") == "pico-christmas" {
        if pico_base::handle_pico_note_hit("bf", ev) { this.casing.pending_rolls += 1; }
    }
}

fn onOpponentNoteHit(ev) {
    if char_id("dad") == "pico-christmas" {
        if pico_base::handle_pico_note_hit("dad", ev) { this.casing.pending_rolls += 1; }
    }
}

fn onNoteMiss(ev) {
    if char_id("bf") == "pico-christmas" {
        pico_base::handle_pico_note_miss("bf", ev);
    }
}
