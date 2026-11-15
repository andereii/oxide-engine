// Philly Blazin stage script (ported from phillyBlazin.hxc to oxide-engine Rhai)

fn cleanup_lightning() {
    prop_visible("skyAdditive", false);
    prop_visible("foregroundMultiply", false);
    prop_visible("additionalLighten", false);
    prop_visible("lightning", false);
}

fn apply_lightning() {
    prop_visible("skyAdditive", true);
    prop_alpha("skyAdditive", 0.7);
    tween_prop("skyAdditive", "alpha", 0.0, 1.5, "linear");

    prop_visible("foregroundMultiply", true);
    prop_alpha("foregroundMultiply", 0.64);
    tween_prop("foregroundMultiply", "alpha", 0.0, 1.5, "linear");

    prop_visible("additionalLighten", true);
    prop_alpha("additionalLighten", 0.3);
    tween_prop("additionalLighten", "alpha", 0.0, 0.3, "linear");

    let lx = if rand_int(0, 99) < 65 {
        rand_int(-250, 280) * 1.0
    } else {
        rand_int(780, 900) * 1.0
    };
    set_prop("lightning", "x", lx);
    prop_visible("lightning", true);
    anim("lightning", "strike");

    let idx = rand_int(1, 3);
    sfx("gameplay/stages/phillyBlazin/sounds/lightning-" + idx + ".ogg", 1.0);
    after(1.5, "cleanup_lightning");
}

fn onCreate() {
    this.lightning_timer = 3.0;
    this.lightning_active = true;

    prop_visible("skyAdditive", false);
    prop_visible("lightning", false);
    prop_visible("foregroundMultiply", false);
    prop_visible("additionalLighten", false);

    char_tint("bf", "#FFDEDEDE");
    char_tint("dad", "#FFDEDEDE");
    char_tint("gf", "#FF888888");

    set_rain(3.6, 0.5, "#6680CC", true);
}

fn onUpdate() {
    if !this.lightning_active {
        this.lightning_timer = 1.0;
        return;
    }

    this.lightning_timer -= dt();
    if this.lightning_timer <= 0.0 {
        apply_lightning();
        this.lightning_timer = rand_range(7.0, 15.0);
    }
}

fn onSongEnd() {
    this.lightning_active = false;
}

fn onGameOver() {
    clear_shader("camera");
}

fn onSongRetry() {
    this.lightning_active = true;
    this.lightning_timer = 3.0;
    cleanup_lightning();
    set_rain(3.6, 0.5, "#6680CC", true);
}
