// Tankman Battlefield (Erect) stage script (ported from tankmanBattlefieldErect.hxc to oxide-engine Rhai)

fn apply_tankman_shaders(use_alt_mask) {
    // AdjustColor order in set_drop_shadow is (hue, saturation, brightness, contrast).
    // Original HScript setAdjustColor(-46, -38, -25, -20) is (brightness, hue, contrast, saturation).
    set_drop_shadow("bf", "#DFEF3C", 90.0, 15.0, 1.0, 0.1, -38.0, -20.0, -46.0, -25.0, "", 0.5);

    let gf_id = char_id("gf");
    if gf_id == "gf-tankmen" {
        set_drop_shadow("gf", "#DFEF3C", 90.0, 15.0, 1.0, 0.1, -38.0, -20.0, -46.0, -25.0, "gameplay/stages/tankmanBattlefieldErect/masks/mask-gf-tankmen.png", 0.4);
    } else if gf_id == "nene-tankmen" {
        set_drop_shadow("gf", "#DFEF3C", 90.0, 15.0, 1.0, 0.1, -38.0, -20.0, -46.0, -25.0, "gameplay/stages/tankmanBattlefieldErect/masks/mask-nene-tankmen.png", 0.4);
    } else {
        set_drop_shadow("gf", "#DFEF3C", 90.0, 15.0, 1.0, 0.1, -38.0, -20.0, -46.0, -25.0, "", 0.5);
    }

    let dad_id = char_id("dad");
    if dad_id == "tankman-bloody" && use_alt_mask {
        set_drop_shadow("dad", "#DFEF3C", 25.0, 15.0, 1.0, 0.3, -38.0, -20.0, -46.0, -25.0, "gameplay/stages/tankmanBattlefieldErect/masks/mask-tankman-bloody.png", 1.0);
    } else {
        set_drop_shadow("dad", "#DFEF3C", 25.0, 15.0, 1.0, 0.3, -38.0, -20.0, -46.0, -25.0, "", 0.5);
    }
}

fn onCreate() {
    apply_tankman_shaders(false);
}

fn onBeatHit() {
    if rand_int(0, 99) < 2 {
        anim("sniper", "sip");
    }
}

fn onSongEvent(ev) {
    if ev.get("kind") == "EnableMask" {
        apply_tankman_shaders(true);
    }
}

fn onGameOver() {
    clear_shader("bf");
}

fn onSongRetry() {
    apply_tankman_shaders(false);
}
