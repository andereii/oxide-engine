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

fn init_blazin_camera() {
    let gf_focus = char_focus("gf");
    set_cam(gf_focus[0] + 50.0, gf_focus[1] - 90.0);
}

fn update_scrolling_sky(offset_x) {
    let tile_w = 1387.0;
    while offset_x <= -tile_w {
        offset_x += tile_w;
    }
    let i = 0;
    while i < 3 {
        prop_pos("blazin_sky_" + i, -700.0 + offset_x + i * tile_w, -120.0);
        i += 1;
    }
    offset_x
}

fn onCreate() {
    this.lightning_timer = 3.0;
    this.lightning_active = true;
    this.sky_scroll_x = 0.0;

    prop_visible("skyAdditive", false);
    prop_visible("lightning", false);
    prop_visible("foregroundMultiply", false);
    prop_visible("additionalLighten", false);

    let z_sky = get_prop_z("skyAdditive") - 0.1;
    let i = 0;
    while i < 3 {
        let id = "blazin_sky_" + i;
        spawn_sprite_tl(id, "gameplay/stages/phillyBlazin/sky-blur.png", -700.0 + i * 1387.0, -120.0, z_sky);
        prop_scroll(id, 0.0, 0.0);
        i += 1;
    }

    char_tint("bf", "#FFDEDEDE");
    char_tint("dad", "#FFDEDEDE");
    char_tint("gf", "#FF888888");

    set_rain(3.6, 0.5, "#6680CC", true);
    init_blazin_camera();
}

fn onCountdownStart() {
    init_blazin_camera();
}

fn onUpdate() {
    this.sky_scroll_x = update_scrolling_sky(this.sky_scroll_x - dt() * 35.0);

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
    init_blazin_camera();
}
