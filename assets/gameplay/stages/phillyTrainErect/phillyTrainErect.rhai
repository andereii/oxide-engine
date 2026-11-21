// Philly Train (Erect) stage script (ported from phillyTrainErect.hxc to oxide-engine Rhai)

const TRAIN_SOUND = "gameplay/stages/phillyTrain/sounds/train-passes.ogg";
const TRAIN_FRAME_TIME = 0.041666666666666664;
const TRAIN_START_X = 1480.0;

fn onCreate() {
    this.light_colors = ["#B66F43", "#329A6D", "#932C28", "#2663AC", "#502D64"];
    this.train_enabled = true;
    this.train_moving = false;
    this.train_frame_timing = 0.0;
    this.train_cars = 8;
    this.train_finishing = false;
    this.train_cooldown = 0;
    this.train_sound_ms = 0.0;
    this.started_moving = false;
    this.train_x = TRAIN_START_X;
    this.cur_light = 0;
    this.light_alpha = 1.0;

    set_prop("train", "x", TRAIN_START_X);

    set_building("lights", 1.0);
    set_adjust_color("train", -26.0, -16.0, -5.0, 0.0);
    set_adjust_color("bf", -26.0, -16.0, -5.0, 0.0);
    set_adjust_color("dad", -26.0, -16.0, -5.0, 0.0);
    set_adjust_color("gf", -26.0, -16.0, -5.0, 0.0);
}

fn onUpdate() {
    let elapsed = dt();

    if this.light_alpha > 0.0 {
        this.light_alpha -= (beat_ms() / 1000.0) * elapsed * 1.5;
        if this.light_alpha < 0.0 {
            this.light_alpha = 0.0;
        }
        prop_alpha("lights", this.light_alpha);
    }

    if !this.train_enabled || !this.train_moving {
        return;
    }

    this.train_sound_ms += elapsed * 1000.0;
    this.train_frame_timing += elapsed;
    while this.train_frame_timing >= TRAIN_FRAME_TIME {
        this.train_frame_timing -= TRAIN_FRAME_TIME;
        if this.train_sound_ms < 4700.0 {
            continue;
        }

        if !this.started_moving {
            this.started_moving = true;
            gf_anim("hairBlow");
        }

        this.train_x -= 400.0;
        set_prop("train", "x", this.train_x);

        if this.train_x < -2000.0 && !this.train_finishing {
            this.train_x = -1150.0;
            set_prop("train", "x", this.train_x);
            this.train_cars -= 1;
            if this.train_cars <= 0 {
                this.train_finishing = true;
            }
        }

        if this.train_x < -4000.0 && this.train_finishing {
            if this.started_moving {
                gf_anim("hairFall");
            }
            this.train_x = TRAIN_START_X;
            set_prop("train", "x", this.train_x);
            this.train_moving = false;
            this.train_cars = 8;
            this.train_finishing = false;
            this.started_moving = false;
            this.train_sound_ms = 0.0;
        }
    }
}

fn onBeatHit() {
    let b = beat();

    if this.train_enabled {
        if !this.train_moving {
            this.train_cooldown += 1;
        }
        if b % 8 == 4 && rand_int(0, 99) < 30 && !this.train_moving && this.train_cooldown > 8 {
            this.train_cooldown = rand_int(-4, 0);
            this.train_moving = true;
            this.train_sound_ms = 0.0;
            sfx(TRAIN_SOUND);
        }
    }

    if b % 4 == 0 {
        this.cur_light = rand_int(0, this.light_colors.len() - 1);
        this.light_alpha = 1.0;
        prop_tint("lights", this.light_colors[this.cur_light]);
        prop_alpha("lights", 1.0);
    }
}

fn onSongEnd() {
    this.train_moving = false;
    this.train_enabled = false;
    this.train_frame_timing = 0.0;
    this.train_cars = 8;
    this.train_finishing = false;
    this.train_cooldown = 0;
    this.train_sound_ms = 0.0;
    this.started_moving = false;
    this.train_x = TRAIN_START_X;
    set_prop("train", "x", TRAIN_START_X);
    gf_anim("danceLeft");
}

fn onSongRetry() {
    this.train_enabled = true;
    this.train_moving = false;
    this.train_frame_timing = 0.0;
    this.train_cars = 8;
    this.train_finishing = false;
    this.train_cooldown = 0;
    this.train_sound_ms = 0.0;
    this.started_moving = false;
    this.train_x = TRAIN_START_X;
    this.cur_light = 0;
    this.light_alpha = 1.0;
    set_prop("train", "x", TRAIN_START_X);
    prop_alpha("lights", 1.0);
    gf_anim("danceLeft");
}
