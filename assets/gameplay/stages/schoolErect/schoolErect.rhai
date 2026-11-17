// School (Erect) stage script (ported from schoolErect.hxc to oxide-engine Rhai)

fn apply_school_erect_shaders() {
    let bf_id = char_id("bf");
    let bf_mask = "gameplay/characters/bf-pixel/bf-pixel-mask.png";
    if bf_id == "pico-pixel" {
        bf_mask = "gameplay/characters/pico-pixel/pico-pixel-mask.png";
    }
    set_drop_shadow("bf", "#52351d", 90.0, 5.0, 1.0, 0.1, -10.0, -23.0, -66.0, 24.0, bf_mask, 1.0);

    let gf_id = char_id("gf");
    let gf_mask = "gameplay/characters/gf-pixel/gf-pixel-mask.png";
    if gf_id == "nene-pixel" {
        gf_mask = "gameplay/characters/nene-pixel/nene-pixel-mask.png";
        set_adjust_color("abot_pixel_body", -10.0, -23.0, -66.0, 24.0);
        set_adjust_color("abot_pixel_back", -10.0, -23.0, -66.0, 24.0);
        set_adjust_color("abot_pixel_head", -10.0, -23.0, -66.0, 24.0);
        let i = 1;
        while i <= 7 {
            set_adjust_color("abot_viz_" + i, -10.0, -23.0, -66.0, 24.0);
            i += 1;
        }
        set_drop_shadow("abot_pixel_speaker", "#52351d", 90.0, 5.0, 1.0, 1.0, -10.0, -23.0, -66.0, 24.0, "gameplay/characters/nene-pixel/abot-pixel/abot-pixel-speaker-mask.png", 0.0);
    }
    set_drop_shadow("gf", "#52351d", 90.0, 3.0, 1.0, 0.3, -10.0, -25.0, -42.0, 5.0, gf_mask, 1.0);

    set_drop_shadow("dad", "#52351d", 90.0, 5.0, 1.0, 0.1, -10.0, -23.0, -66.0, 24.0, "gameplay/characters/senpai/senpai-mask.png", 1.0);
}

fn onCreate() {
    this.freaks_left = false;
    if song() == "roses" {
        anim("freaks", "danceLeft-scared");
    }
    apply_school_erect_shaders();
}

fn onBeatHit() {
    if song() == "roses" {
        this.freaks_left = !this.freaks_left;
        if this.freaks_left {
            anim("freaks", "danceLeft-scared");
        } else {
            anim("freaks", "danceRight-scared");
        }
    }
}

fn onSongRetry() {
    this.freaks_left = false;
    if song() == "roses" {
        anim("freaks", "danceLeft-scared");
    }
}
