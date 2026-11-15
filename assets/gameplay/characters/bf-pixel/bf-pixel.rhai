// Boyfriend (Pixel) character script (ported from bf-pixel.hxc)

fn onCreate() {
    this.shader_cleared = false;
    global_set("gameover_music_suffix", "-pixel");
    global_set("gameover_blueball_suffix", "-pixel");
    global_set("pause_music_suffix", "-pixel");
}

fn onSongRetry() {
    this.shader_cleared = false;
}

fn onGameOver() {
    if char_id("bf") == "bf-pixel" {
        clear_shader("bf");
        this.shader_cleared = true;
    }
}

fn onUpdate() {
    if !this.shader_cleared && char_id("bf") == "bf-pixel" && char_anim("bf") == "firstDeath" {
        clear_shader("bf");
        this.shader_cleared = true;
    }
}
