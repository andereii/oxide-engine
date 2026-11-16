// Philly Streets stage script (ported from Funkin phillyStreets.hxc)

fn new_car_state() {
    #{
        active: false,
        delay: 0.0,
        elapsed: 0.0,
        duration: 1.0,
        ease: "linear",
        p0x: 1200.0, p0y: 818.0,
        p1x: 1200.0, p1y: 818.0,
        p2x: 1200.0, p2y: 818.0,
        r0: 0.0, r1: 0.0,
        extra_x: 0.0, extra_y: 0.0,
        on_done: ""
    }
}

fn variant_params(variant, is_lights) {
    if variant == 1 {
        return [0.0, 0.0, rand_range(1.0, 1.7)];
    }
    if variant == 2 {
        let dur = if is_lights { rand_range(0.9, 1.5) } else { rand_range(0.6, 1.2) };
        return [20.0, -15.0, dur];
    }
    if variant == 3 {
        return [30.0, 50.0, rand_range(1.5, 2.5)];
    }
    [10.0, 60.0, rand_range(1.5, 2.5)]
}

fn eval_ease(u, ease) {
    if ease == "cubeOut" {
        let k = 1.0 - u;
        return 1.0 - k * k * k;
    }
    if ease == "sineIn" {
        return 1.0 - (u * 1.5707963267948966).cos();
    }
    u
}

fn make_finish_car1(prev_car) {
    let ex = prev_car.extra_x;
    let ey = prev_car.extra_y;
    let ox = 306.6 + ex;
    let oy = 168.3 + ey;
    #{
        active: true,
        delay: rand_range(0.2, 1.2),
        elapsed: 0.0,
        duration: rand_range(1.8, 3.0),
        ease: "sineIn",
        p0x: 1950.0 - ox - 80.0, p0y: 980.0 - oy + 15.0,
        p1x: 2400.0 - ox,        p1y: 980.0 - oy - 50.0,
        p2x: 3102.0 - ox,        p2y: 1187.0 - oy + 40.0,
        r0: -5.0, r1: 18.0,
        extra_x: ex, extra_y: ey,
        on_done: "free1"
    }
}

fn make_car_normal() {
    let v = rand_int(1, 4);
    anim("phillyCars", "car" + v);
    let vp = variant_params(v, false);
    let ex = vp[0];
    let ey = vp[1];
    let dur = vp[2];
    let ox = 306.6 + ex;
    let oy = 168.3 + ey;
    let car = #{
        active: true,
        delay: 0.0,
        elapsed: 0.0,
        duration: dur,
        ease: "linear",
        p0x: 1570.0 - ox, p0y: 1049.0 - oy - 30.0,
        p1x: 2400.0 - ox, p1y: 980.0 - oy - 50.0,
        p2x: 3102.0 - ox, p2y: 1187.0 - oy + 40.0,
        r0: -8.0, r1: 18.0,
        extra_x: ex, extra_y: ey,
        on_done: "free1"
    };
    prop_pos("phillyCars", car.p0x, car.p0y);
    prop_angle("phillyCars", -8.0);
    car
}

fn make_car_lights() {
    let v = rand_int(1, 4);
    anim("phillyCars", "car" + v);
    let vp = variant_params(v, true);
    let ex = vp[0];
    let ey = vp[1];
    let dur = vp[2];
    let ox = 306.6 + ex;
    let oy = 168.3 + ey;
    let car = #{
        active: true,
        delay: 0.0,
        elapsed: 0.0,
        duration: dur,
        ease: "cubeOut",
        p0x: 1500.0 - ox - 20.0, p0y: 1049.0 - oy - 20.0,
        p1x: 1770.0 - ox - 80.0, p1y: 994.0 - oy + 10.0,
        p2x: 1950.0 - ox - 80.0, p2y: 980.0 - oy + 15.0,
        r0: -7.0, r1: -5.0,
        extra_x: ex, extra_y: ey,
        on_done: "stopped1"
    };
    prop_pos("phillyCars", car.p0x, car.p0y);
    prop_angle("phillyCars", -7.0);
    car
}

fn make_car_back() {
    let v = rand_int(1, 4);
    anim("phillyCars2", "car" + v);
    let vp = variant_params(v, false);
    let ex = vp[0];
    let ey = vp[1];
    let dur = vp[2];
    let ox = 306.6 + ex;
    let oy = 168.3 + ey;
    let car = #{
        active: true,
        delay: 0.0,
        elapsed: 0.0,
        duration: dur,
        ease: "linear",
        p0x: 3102.0 - ox, p0y: 1127.0 - oy + 60.0,
        p1x: 2400.0 - ox, p1y: 980.0 - oy - 30.0,
        p2x: 1570.0 - ox, p2y: 1049.0 - oy - 10.0,
        r0: 18.0, r1: -8.0,
        extra_x: ex, extra_y: ey,
        on_done: "free2"
    };
    prop_pos("phillyCars2", car.p0x, car.p0y);
    prop_angle("phillyCars2", 18.0);
    car
}

fn step_car(car, prop_name, delta_s) {
    if !car.active { return car; }
    if car.delay > 0.0 {
        car.delay -= delta_s;
        if car.delay > 0.0 { return car; }
        car.elapsed += -car.delay;
        car.delay = 0.0;
    } else {
        car.elapsed += delta_s;
    }
    let u = car.elapsed / car.duration;
    if u > 1.0 { u = 1.0; }
    let t = eval_ease(u, car.ease);
    let omt = 1.0 - t;
    let x = omt * omt * car.p0x + 2.0 * omt * t * car.p1x + t * t * car.p2x;
    let y = omt * omt * car.p0y + 2.0 * omt * t * car.p1y + t * t * car.p2y;
    let ang = car.r0 + (car.r1 - car.r0) * t;
    prop_pos(prop_name, x, y);
    prop_angle(prop_name, ang);

    if u >= 1.0 {
        car.active = false;
    }
    car
}

fn onCreate() {
    this.cars_enabled = true;
    this.lights_stop = false;
    this.last_change = 0;
    this.change_interval = 8;
    this.car_waiting = false;
    this.car_interruptable = true;
    this.car2_interruptable = true;
    this.car1 = new_car_state();
    this.car2 = new_car_state();
    prop_pos("phillyCars", 1200.0, 818.0);
    prop_angle("phillyCars", 0.0);
    prop_pos("phillyCars2", 1200.0, 818.0);
    prop_angle("phillyCars2", 0.0);

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
    anim("phillyTraffic", "togreen");
}

fn onUpdate() {
    let delta_s = dt();
    if delta_s <= 0.0 { return; }

    if this.car1.active {
        this.car1 = step_car(this.car1, "phillyCars", delta_s);
        if !this.car1.active {
            if this.car1.on_done == "free1" {
                this.car_interruptable = true;
            } else if this.car1.on_done == "stopped1" {
                this.car_waiting = true;
                if !this.lights_stop {
                    this.car_waiting = false;
                    this.car1 = make_finish_car1(this.car1);
                }
            }
        }
    }

    if this.car2.active {
        this.car2 = step_car(this.car2, "phillyCars2", delta_s);
        if !this.car2.active && this.car2.on_done == "free2" {
            this.car2_interruptable = true;
        }
    }
}

fn onBeatHit() {
    let b = beat();
    let next_change = this.last_change + this.change_interval;

    if b != next_change && this.car_interruptable && this.cars_enabled && rand_int(0, 99) < 10 {
        this.car_interruptable = false;
        if !this.lights_stop {
            this.car1 = make_car_normal();
        } else {
            this.car1 = make_car_lights();
        }
    }

    if b != next_change && this.car2_interruptable && !this.lights_stop && this.cars_enabled && rand_int(0, 99) < 10 {
        this.car2_interruptable = false;
        this.car2 = make_car_back();
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
                this.car1 = make_finish_car1(this.car1);
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
    this.car_waiting = false;
    this.car_interruptable = true;
    this.car2_interruptable = true;
    this.car1 = new_car_state();
    this.car2 = new_car_state();
    prop_pos("phillyCars", 1200.0, 818.0);
    prop_angle("phillyCars", 0.0);
    prop_pos("phillyCars2", 1200.0, 818.0);
    prop_angle("phillyCars2", 0.0);
    anim("phillyTraffic", "togreen");
    set_rain(3.6, this.rain_start, "#6680CC");
}
