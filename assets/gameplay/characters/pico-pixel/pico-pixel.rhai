// Pico (Pixel) character script (ported from pico-pixel.hxc)
import "gameplay/characters/pico-playable/pico-playable" as pico_base;
import "gameplay/characters/pico-playable/CasingSprite" as casing_mod;

fn onCreate() {
    this.casing = casing_mod::new_casing_state();
    this.shader_cleared = false;
    global_set("gameover_music_suffix", "-pixel-pico");
    global_set("gameover_blueball_suffix", "-pixel-pico");
    global_set("pause_music_suffix", "-pixel");
}

fn onSongRetry() {
    this.casing = casing_mod::clear_casings();
    this.shader_cleared = false;
}

fn onGameOver() {
    if char_id("bf") == "pico-pixel" {
        clear_shader("bf");
        this.shader_cleared = true;
    }
}

fn onUpdate() {
    if !this.shader_cleared && char_id("bf") == "pico-pixel" && char_anim("bf") == "firstDeath" {
        clear_shader("bf");
        this.shader_cleared = true;
    }
}

fn onNoteHit(ev) {
    if char_id("bf") == "pico-pixel" {
        if pico_base::handle_pico_note_hit("bf", ev) { this.casing.pending_rolls += 1; }
    }
}

fn onOpponentNoteHit(ev) {
    if char_id("dad") == "pico-pixel" {
        if pico_base::handle_pico_note_hit("dad", ev) { this.casing.pending_rolls += 1; }
    }
}

fn onNoteMiss(ev) {
    if char_id("bf") == "pico-pixel" {
        pico_base::handle_pico_note_miss("bf", ev);
    }
}
