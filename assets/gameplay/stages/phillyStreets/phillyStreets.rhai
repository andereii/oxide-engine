// Philly Streets stage script (ported from phillyStreets.hxc to oxide-engine Rhai)

fn reset_car_left(state) {
    state.car_waiting = false;
    state.car_interruptable = true;
    prop_pos("phillyCars", 1200.0, 818.0);
    prop_angle("phillyCars", 0.0);
}

fn reset_car_right(state) {
    state.car2_interruptable = true;
    prop_pos("phillyCars2", 1200.0, 818.0);
    prop_angle("phillyCars2", 0.0);
}

fn finish_car1() {
    let dur = rand_range(1.8, 3.0);
    tween_prop("phillyCars", "x", 2795.0, dur, "sineIn");
    tween_prop("phillyCars", "y", 1018.0, dur, "sineIn");
    tween_prop("phillyCars", "angle", 18.0, dur, "sineIn");
    after(dur + 0.2, "free_car1");
}

fn free_car1() {
    this.car_interruptable = true;
}

fn free_car2() {
    this.car2_interruptable = true;
}

fn on_car1_stopped() {
    this.car_waiting = true;
    if !this.lights_stop {
        this.car_waiting = false;
        finish_car1();
    }
}

fn drive_car_normal(state) {
    state.car_interruptable = false;
    let v = rand_int(1, 4);
    anim("phillyCars", "car" + v);
    let dur = rand_range(1.0, 2.2);
    prop_pos("phillyCars", 1263.4, 850.7);
    prop_angle("phillyCars", -8.0);
    tween_prop("phillyCars", "x", 2795.4, dur, "linear");
    tween_prop("phillyCars", "y", 1018.7, dur, "linear");
    tween_prop("phillyCars", "angle", 18.0, dur, "linear");
    after(dur, "free_car1");
}

fn drive_car_lights(state) {
    state.car_interruptable = false;
    let v = rand_int(1, 4);
    anim("phillyCars", "car" + v);
    let dur = rand_range(1.0, 2.2);
    prop_pos("phillyCars", 1173.4, 860.7);
    prop_angle("phillyCars", -7.0);
    tween_prop("phillyCars", "x", 1563.4, dur, "cubeOut");
    tween_prop("phillyCars", "y", 826.7, dur, "cubeOut");
    tween_prop("phillyCars", "angle", -5.0, dur, "cubeOut");
    after(dur, "on_car1_stopped");
}

fn drive_car_back(state) {
    state.car2_interruptable = false;
    let v = rand_int(1, 4);
    anim("phillyCars2", "car" + v);
    let dur = rand_range(1.0, 2.2);
    prop_pos("phillyCars2", 2795.4, 1018.7);
    prop_angle("phillyCars2", 18.0);
    tween_prop("phillyCars2", "x", 1263.4, dur, "linear");
    tween_prop("phillyCars2", "y", 870.7, dur, "linear");
    tween_prop("phillyCars2", "angle", -8.0, dur, "linear");
    after(dur, "free_car2");
}

fn onCreate() {
    this.cars_enabled = true;
    this.lights_stop = false;
    this.last_change = 0;
    this.change_interval = 8;
    this.car_waiting = false;
    this.car_interruptable = true;
    this.car2_interruptable = true;

    let s = song();
    this.rain_start = 0.0;
    this.rain_end = 0.1;
    if s == "lit-up" {
        this.rain_start = 0.1;
        this.rain_end = 0.2;
    } else if s == "2hot" {
        this.rain_start = 0.2;
        this.rain_end = 0.4;
    }

    set_rain(3.6, this.rain_start, "#6680CC");
    reset_car_left(this);
    reset_car_right(this);
    anim("phillyTraffic", "togreen");
}

fn onBeatHit() {
    let b = beat();
    let next_change = this.last_change + this.change_interval;

    if b != next_change && this.car_interruptable && this.cars_enabled && rand_int(0, 99) < 10 {
        if !this.lights_stop {
            drive_car_normal(this);
        } else {
            drive_car_lights(this);
        }
    }

    if b != next_change && this.car2_interruptable && !this.lights_stop && this.cars_enabled && rand_int(0, 99) < 10 {
        drive_car_back(this);
    }

    if b == next_change {
        this.last_change = b;
        this.lights_stop = !this.lights_stop;
        if this.lights_stop {
            anim("phillyTraffic", "tored");
            this.change_interval = 20;
        } else {
            anim("phillyTraffic", "togreen");
            this.change_interval = 30;
            if this.car_waiting {
                this.car_waiting = false;
                finish_car1();
            }
        }
    }
}

fn onGameOver() {
    clear_shader("camera");
}

fn onSongRetry() {
    this.last_change = 0;
    this.change_interval = 8;
    this.lights_stop = false;
    anim("phillyTraffic", "togreen");
    reset_car_left(this);
    reset_car_right(this);
    set_rain(3.6, this.rain_start, "#6680CC");
}
