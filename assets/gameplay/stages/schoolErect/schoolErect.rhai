// School (Erect) stage script (ported from schoolErect.hxc to oxide-engine Rhai)

fn apply_school_erect_shaders() {
    let bf_id = char_id("bf");
    let bf_mask = "gameplay/characters/bf-pixel/bf-pixel-mask.png";
    if bf_id == "pico-pixel" {
        bf_mask = "gameplay/characters/pico-pixel/pico-pixel-mask.png";
    }
    set_drop_shadow("bf", "#52351d", 90.0, 5.0, 1.0, 0.1, -66.0, -10.0, 24.0, -23.0, bf_mask, 1.0);

    let gf_id = char_id("gf");
    let gf_mask = "gameplay/characters/gf-pixel/gf-pixel-mask.png";
    if gf_id == "nene-pixel" {
        gf_mask = "gameplay/characters/nene-pixel/nene-pixel-mask.png";
    }
    set_drop_shadow("gf", "#52351d", 90.0, 3.0, 1.0, 0.3, -42.0, -10.0, 5.0, -25.0, gf_mask, 1.0);

    set_drop_shadow("dad", "#52351d", 90.0, 5.0, 1.0, 0.1, -66.0, -10.0, 24.0, -23.0, "gameplay/characters/senpai/senpai-mask.png", 1.0);
}

fn onCreate() {
    if song() == "roses" {
        anim("freaks", "idle-scared");
    }
    apply_school_erect_shaders();
}
