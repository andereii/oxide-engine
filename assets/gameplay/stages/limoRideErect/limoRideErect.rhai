// Limo Ride (Erect) stage script (ported from limoRideErect.hxc to oxide-engine Rhai)

fn reset_car_state(state) {
    state.fast_car_x = -12600.0;
    state.fast_car_vx = 0.0;
    state.fast_car_can_drive = true;
    prop_pos("fastCar", -12600.0, rand_int(350, 380) * 1.0);
}

fn reset_fast_car() {
    reset_car_state(this);
}

fn onCreate() {
    this.cars_enabled = true;
    this.shooting_star_beat = 0;
    this.shooting_star_offset = 2;

    set_adjust_color("limoDancer1", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("limoDancer2", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("limoDancer3", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("limoDancer4", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("limoDancer5", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("fastCar", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("bf", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("dad", -30.0, -20.0, -30.0, 0.0);
    set_adjust_color("gf", -30.0, -20.0, -30.0, 0.0);

    reset_car_state(this);
}

fn onUpdate() {
    if this.fast_car_vx > 0.0 {
        this.fast_car_x += this.fast_car_vx * dt();
        set_prop("fastCar", "x", this.fast_car_x);
    }
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
        anim("shootingStar", "shooting star");
        this.shooting_star_beat = b;
        this.shooting_star_offset = rand_int(4, 8);
    }
}

fn onSongRetry() {
    reset_car_state(this);
    this.shooting_star_beat = 0;
    this.shooting_star_offset = 2;
}
