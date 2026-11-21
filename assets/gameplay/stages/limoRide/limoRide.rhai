// Limo Ride stage script (ported from limoRide.hxc to oxide-engine Rhai)

fn reset_fast_car() {
    this.fast_car_x = -12600.0;
    this.fast_car_vx = 0.0;
    this.fast_car_can_drive = true;
    prop_pos("fastCar", -12600.0, rand_int(350, 380) * 1.0);
}

fn onCreate() {
    this.cars_enabled = true;
    this.fast_car_x = -12600.0;
    this.fast_car_vx = 0.0;
    this.fast_car_can_drive = true;
    prop_pos("fastCar", -12600.0, rand_int(350, 380) * 1.0);
    set_overlay_blend("limoSunset", "gameplay/stages/limoRide/limo-overlay.png");
}

fn onUpdate() {
    if this.fast_car_vx > 0.0 {
        this.fast_car_x += this.fast_car_vx * dt();
        set_prop("fastCar", "x", this.fast_car_x);
    }
}

fn onBeatHit() {
    if !this.cars_enabled || !this.fast_car_can_drive {
        return;
    }
    if rand_int(0, 99) >= 10 {
        return;
    }

    let idx = rand_int(1, 2);
    sfx("gameplay/stages/limoRide/sounds/car-pass-" + idx + ".ogg", 0.7);
    this.fast_car_vx = rand_int(170, 220) * 180.0;
    this.fast_car_can_drive = false;
    after(2.0, "reset_fast_car");
}

fn onSongRetry() {
    this.fast_car_x = -12600.0;
    this.fast_car_vx = 0.0;
    this.fast_car_can_drive = true;
    prop_pos("fastCar", -12600.0, rand_int(350, 380) * 1.0);
}
