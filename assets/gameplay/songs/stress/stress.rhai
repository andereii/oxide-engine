// Stress song script: Tankman runner group (ported from stress.hxc + TankmanSpriteGroup.hxc to oxide-engine Rhai)

fn sort_runner_notes(list) {
    let i = 1;
    while i < list.len() {
        let cur = list[i];
        let j = i - 1;
        while j >= 0 && list[j].t > cur.t {
            list[j + 1] = list[j];
            j -= 1;
        }
        list[j + 1] = cur;
        i += 1;
    }
    list
}

fn build_runner_timings() {
    let empty = [];
    let v = variation();
    let candidates = [];
    let keys = [];
    if v != "" && v != "default" {
        candidates.push("gameplay/songs/stress/chart-" + v + "/notes.json");
        candidates.push("gameplay/songs/stress/" + v + "/notes.json");
        keys.push(v + "-picospeaker");
    }
    candidates.push("gameplay/songs/stress/chart/notes.json");
    keys.push("picospeaker");

    let src = ();
    for path in candidates {
        if file_exists(path) {
            for k in keys {
                src = read_json(path, k);
                if src != () && src.len() > 0 { break; }
            }
            if src != () && src.len() > 0 { break; }
            let d_key = diff();
            if v != "" && v != "default" && !d_key.starts_with(v + "-") {
                d_key = v + "-" + d_key;
            }
            src = read_json(path, d_key);
            if src == () { src = read_json(path, "hard"); }
            if src != () { break; }
        }
    }
    if src == () { return empty; }

    let parsed = [];
    for item in src {
        let t = item["t"];
        if t == () { t = item["time"]; }
        let d = item["d"];
        if d == () { d = item["data"]; }
        if t != () && d != () {
            parsed.push(#{ t: t * 1.0, d: d % 4 });
        }
    }
    parsed = sort_runner_notes(parsed);

    let out = [];
    for n in parsed {
        // 1 in 16 chance matches TankmanSpriteGroup.initTimemap().
        if rand_int(0, 15) == 0 {
            let going_right = !(n.d == 2 || n.d == 3);
            out.push(#{ t: n.t, going_right: going_right });
        }
    }
    out
}

fn clear_active_runners(runners) {
    for r in runners {
        if prop_exists(r.id) {
            despawn(r.id);
        }
    }
}

fn tankman_z_layer() {
    if prop_exists("tankGround") && prop_exists("tankRolling") {
        return 0.5 * (get_prop_z("tankRolling") + get_prop_z("tankGround"));
    }
    if prop_exists("tankGround") {
        return get_prop_z("tankGround") - 0.5;
    }
    if prop_exists("sniper") {
        return get_prop_z("sniper") + 1.0;
    }
    if prop_exists("tankSky") {
        return get_prop_z("tankSky") + 2.0;
    }
    -497.0
}

fn flicker_alpha_at(shot_age) {
    let start_s = 10.0 / 24.0;
    if shot_age < start_s {
        return 1.0;
    }
    let flick = shot_age - start_s;
    if flick < 0.3 {
        let step = (flick / 0.1).to_int();
        return if step % 2 == 0 { 0.0 } else { 1.0 };
    }
    let flick2 = flick - 0.3;
    if flick2 < 0.3 {
        let step = (flick2 / 0.05).to_int();
        return if step % 2 == 0 { 0.0 } else { 1.0 };
    }
    0.0
}

fn onCreate() {
    this.timings = [];
    this.next_idx = 0;
    this.runner_seq = 0;
    this.runners = [];
    this.last_now_ms = 0.0;
    this.is_erect = (variation() == "pico" || stage() == "tankmanBattlefieldErect");
    this.timings = build_runner_timings();
}

fn onSongRetry() {
    clear_active_runners(this.runners);
    this.runners = [];
    this.timings = build_runner_timings();
    this.next_idx = 0;
    this.last_now_ms = 0.0;
}

fn onUpdate() {
    let now_ms = time();
    let elapsed = dt();
    if now_ms < this.last_now_ms - 50.0 {
        clear_active_runners(this.runners);
        this.runners = [];
        this.next_idx = 0;
    }
    this.last_now_ms = now_ms;

    let cutoff = now_ms + 3000.0;
    let death_total_s = (10.0 / 24.0) + 0.6;
    let z_layer = tankman_z_layer();

    while this.next_idx < this.timings.len() {
        let entry = this.timings[this.next_idx];
        if entry.t > cutoff { break; }
        this.next_idx += 1;
        if entry.t < now_ms - death_total_s * 1000.0 { continue; }

        this.runner_seq += 1;
        let rid = "stress_runner_" + this.runner_seq;
        let y_pos = if this.is_erect { 350.0 } else { 200.0 + rand_int(50, 100) * 1.0 };
        let scale = if this.is_erect { 1.10 } else { 1.0 };
        let ending_offset = rand_range(50.0, 200.0);
        let run_speed = rand_range(0.6, 1.0);
        let shot_anim = "John Shot " + rand_int(1, 2);

        // TankmanSprite ctor calls setGraphicSize(width * 0.4) + updateHitbox() -> origin=(252.5,239), offset=(151.5,143.4).
        // TankmanSpriteGroup then overrides scale.set(1.0 or 1.10), while shot switches offset.set(300,200).
        // Oxide TopLeft StageCoordMode anchors at: pos - offset + origin * (1 - scale).
        let scale_shift_x = 252.5 * (1.0 - scale);
        let scale_shift_y = 239.0 * (1.0 - scale);
        let run_dx = -151.5 + scale_shift_x;
        let run_dy = -143.4 + scale_shift_y;
        let shot_dx = -300.0 + scale_shift_x;
        let shot_dy = -200.0 + scale_shift_y;

        let x_end = if !entry.going_right {
            1280.0 * 0.02 - ending_offset
        } else {
            1280.0 * 0.74 + ending_offset
        };

        let already_shot = now_ms >= entry.t;
        let init_x = if already_shot {
            x_end + shot_dx
        } else {
            let run_factor = (now_ms - entry.t) * run_speed;
            let rx = if !entry.going_right { x_end + run_factor } else { x_end - run_factor };
            rx + run_dx
        };
        let init_y = if already_shot { y_pos + shot_dy } else { y_pos + run_dy };

        spawn_sparrow(
            rid,
            "gameplay/stages/tankmanBattlefield/tankman-killed",
            init_x,
            init_y,
            z_layer,
            [
                #{ name: "run", prefix: "tankman running", fps: 24.0, looped: true },
                #{ name: "shot", prefix: shot_anim, fps: 24.0, looped: false }
            ]
        );
        prop_scale(rid, scale, scale);
        prop_flip(rid, !entry.going_right, false);
        if this.is_erect {
            set_drop_shadow(rid, "#DFEF3C", 135.0, 15.0, 1.0, 0.4, -38.0, -20.0, -46.0, -25.0);
        }

        let phase = "run";
        let shot_age = 0.0;
        if already_shot {
            phase = "shot";
            shot_age = (now_ms - entry.t) / 1000.0;
            let frame_idx = (shot_age * 24.0).to_int();
            if frame_idx > 11 { frame_idx = 11; }
            play_anim_frame(rid, "shot", frame_idx);
            prop_alpha(rid, flicker_alpha_at(shot_age));
        } else {
            play_anim_frame(rid, "run", rand_int(0, 13));
        }

        this.runners.push(#{
            id: rid,
            strum_time: entry.t,
            going_right: entry.going_right,
            x_end: x_end,
            run_speed: run_speed,
            y: y_pos,
            run_dx: run_dx,
            run_dy: run_dy,
            shot_dx: shot_dx,
            shot_dy: shot_dy,
            phase: phase,
            shot_age: shot_age
        });
    }

    let next_runners = [];
    for r in this.runners {
        if !prop_exists(r.id) { continue; }

        if r.phase == "run" {
            if now_ms >= r.strum_time {
                r.phase = "shot";
                r.shot_age = (now_ms - r.strum_time) / 1000.0;
                play_anim(r.id, "shot");
                prop_pos(r.id, r.x_end + r.shot_dx, r.y + r.shot_dy);
                next_runners.push(r);
            } else {
                let run_factor = (now_ms - r.strum_time) * r.run_speed;
                let rx = if !r.going_right {
                    r.x_end + run_factor
                } else {
                    r.x_end - run_factor
                };
                prop_pos(r.id, rx + r.run_dx, r.y + r.run_dy);
                next_runners.push(r);
            }
        } else {
            r.shot_age += elapsed;
            if r.shot_age >= death_total_s {
                despawn(r.id);
            } else {
                prop_alpha(r.id, flicker_alpha_at(r.shot_age));
                next_runners.push(r);
            }
        }
    }
    this.runners = next_runners;
}
