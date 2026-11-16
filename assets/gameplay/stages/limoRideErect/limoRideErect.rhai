// Limo Ride (Erect) stage script (ported from limoRideErect.hxc to oxide-engine Rhai)

fn reset_fast_car() {
    this.fast_car_x = -12600.0;
    this.fast_car_vx = 0.0;
    this.fast_car_can_drive = true;
    prop_pos("fastCar", -12600.0, rand_int(350, 380) * 1.0);
}

fn spawn_mist_layer(prefix, asset, scale, scroll_x, scroll_y, z, tint_hex, alpha, vx, start_y) {
    let tile_w = 2048.0 * scale;
    let i = 0;
    while i < 3 {
        let id = prefix + "_" + i;
        let x = -650.0 + (i - 1) * tile_w;
        spawn_sprite_tl(id, asset, x, start_y, z);
        prop_scale(id, scale, scale);
        prop_scroll(id, scroll_x, scroll_y);
        prop_tint(id, tint_hex);
        prop_alpha(id, alpha);
        set_blend(id, "add");
        i += 1;
    }
    #{ prefix: prefix, tile_w: tile_w, offset_x: 0.0, vx: vx, y: start_y }
}

fn update_mist_layer(layer, y_val, elapsed_dt) {
    layer.offset_x += layer.vx * elapsed_dt;
    while layer.offset_x >= layer.tile_w {
        layer.offset_x -= layer.tile_w;
    }
    let i = 0;
    while i < 3 {
        let x = -650.0 + layer.offset_x + (i - 1) * layer.tile_w;
        prop_pos(layer.prefix + "_" + i, x, y_val);
        i += 1;
    }
    layer
}

fn onCreate() {
    this.cars_enabled = true;
    this.shooting_star_beat = 0;
    this.shooting_star_offset = 2;
    this.mist_timer = 0.0;
    this.fast_car_x = -12600.0;
    this.fast_car_vx = 0.0;
    this.fast_car_can_drive = true;
    prop_pos("fastCar", -12600.0, rand_int(350, 380) * 1.0);

    set_adjust_color("limoDancer1", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("limoDancer2", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("limoDancer3", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("limoDancer4", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("limoDancer5", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("fastCar", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("bf", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("dad", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("gf", -30.0, -20.0, -30.0, 0.0);

    set_blend("shootingStar", "add");

    let z_sunset = get_prop_z("limoSunset");
    let z_gf = char_z("gf");
    let z_bf = char_z("bf");
    let mist_mid = "gameplay/stages/limoRideErect/mist-middle.png";
    let mist_back = "gameplay/stages/limoRideErect/mist-back.png";

    this.mist1 = spawn_mist_layer("limo_mist1", mist_mid, 1.3, 1.1, 1.1, z_bf + 10.0, "#FFC6BFDE", 0.4, 1700.0, 100.0);
    this.mist2 = spawn_mist_layer("limo_mist2", mist_back, 1.0, 1.2, 1.2, z_bf + 10.1, "#FF6A4DA1", 1.0, 2100.0, 0.0);
    this.mist3 = spawn_mist_layer("limo_mist3", mist_mid, 1.5, 0.8, 0.8, z_gf - 0.1, "#FFA7D9BE", 0.5, 900.0, -20.0);
    this.mist4 = spawn_mist_layer("limo_mist4", mist_back, 1.5, 0.6, 0.6, z_gf - 0.2, "#FF9C77C7", 1.0, 700.0, -180.0);
    this.mist5 = spawn_mist_layer("limo_mist5", mist_mid, 1.5, 0.2, 0.2, z_sunset + 0.5, "#FFE7A480", 1.0, 100.0, -450.0);
}

fn onUpdate() {
    let elapsed = dt();
    if this.fast_car_vx > 0.0 {
        this.fast_car_x += this.fast_car_vx * elapsed;
        set_prop("fastCar", "x", this.fast_car_x);
    }

    this.mist_timer += elapsed;
    let t = this.mist_timer;
    this.mist1 = update_mist_layer(this.mist1, 100.0 + sin(t) * 200.0, elapsed);
    this.mist2 = update_mist_layer(this.mist2, 0.0 + sin(t * 0.8) * 100.0, elapsed);
    this.mist3 = update_mist_layer(this.mist3, -20.0 + sin(t * 0.5) * 200.0, elapsed);
    this.mist4 = update_mist_layer(this.mist4, -180.0 + sin(t * 0.4) * 300.0, elapsed);
    this.mist5 = update_mist_layer(this.mist5, -450.0 + sin(t * 0.2) * 150.0, elapsed);
}

fn onBeatHit() {
    let b = beat();

    if this.cars_enabled && this.fast_car_can_drive && rand_int(0, 99) < 10 {
        let idx = rand_int(1, 2);
        sfx("gameplay/stages/limoRide/sounds/car-pass-" + idx + ".ogg", 0.7);
        this.fast_car_vx = rand_int(170, 220) * 180.0;
        this.fast_car_can_drive = false;
        after(2.0, "reset_fast_car");
    }

    if rand_int(0, 99) < 10 && b > (this.shooting_star_beat + this.shooting_star_offset) {
        prop_pos("shootingStar", rand_int(50, 900) * 1.0, rand_int(-10, 20) * 1.0);
        prop_flip("shootingStar", rand_int(0, 1) == 1, false);
        anim("shootingStar", "shooting star");
        this.shooting_star_beat = b;
        this.shooting_star_offset = rand_int(4, 8);
    }
}

fn onCountdownStart() {
    this.fast_car_x = -12600.0;
    this.fast_car_vx = 0.0;
    this.fast_car_can_drive = true;
    prop_pos("fastCar", -12600.0, rand_int(350, 380) * 1.0);
}

fn onSongRetry() {
    this.fast_car_x = -12600.0;
    this.fast_car_vx = 0.0;
    this.fast_car_can_drive = true;
    prop_pos("fastCar", -12600.0, rand_int(350, 380) * 1.0);
    this.shooting_star_beat = 0;
    this.shooting_star_offset = 2;
}
