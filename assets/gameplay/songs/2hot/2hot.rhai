// 2hot song script (ported from 2hot.hxc + SpraycanAtlasSprite.hxc to oxide-engine Rhai)

fn register_2hot_kinds() {
    register_notekind("weekend-1-lightcan", #{ description: "Darnell Light Can (2hot)", no_anim: true });
    register_notekind("weekend-1-kneecan", #{ description: "Darnell Knee Can (2hot)", no_anim: true });
    register_notekind("weekend-1-kickcan", #{ description: "Darnell Kick Can (2hot)", no_anim: true });
    register_notekind("weekend-1-cockgun", #{ description: "Pico Cock Gun (2hot)", no_anim: true });
    register_notekind("weekend-1-firegun", #{ description: "Pico Fire Gun (2hot)", no_anim: true });
}

fn philly_prop_names() {
    [
        "solid",
        "phillySkyline",
        "phillyForegroundCity2",
        "phillyConstruction",
        "phillyHighwayLights",
        "phillyHighwayLights_lightmap",
        "phillyHighway",
        "phillySmog",
        "phillyCars",
        "phillyCars2",
        "phillyTraffic",
        "phillyTraffic_lightmap",
        "phillyForeground",
        "spraycanPile"
    ]
}

fn set_stage_tints(hex, include_chars) {
    for name in philly_prop_names() {
        if prop_exists(name) {
            prop_tint(name, hex);
        }
    }
    if include_chars {
        if char_exists("dad") { char_tint("dad", hex); }
        if char_exists("gf") { char_tint("gf", hex); }
    }
}

fn rgb_gray_hex(byte_val) {
    let hex_digits = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "A", "B", "C", "D", "E", "F"];
    let clamped = clamp(byte_val, 0.0, 255.0).to_int();
    let hi = clamped / 16;
    let lo = clamped % 16;
    let pair = hex_digits[hi] + hex_digits[lo];
    "#FF" + pair + pair + pair
}

fn onCreate() {
    register_2hot_kinds();
    this.has_played_cutscene = false;
    this.gun_cocked = false;
    this.gun_timer = 0.0;
    this.can_seq = 0;
    this.cans = [];
    this.fx_seq = 0;
    this.fxs = [];
    this.darken_delay = -1.0;
    this.darken_phase = 0;
    this.darken_elapsed = 0.0;
    this.blacken_timer = -1.0;
}

fn onSongRetry() {
    for can in this.cans {
        if prop_exists(can.id) {
            despawn(can.id);
        }
    }
    for fx_id in this.fxs {
        if prop_exists(fx_id) {
            despawn(fx_id);
        }
    }
    this.cans = [];
    this.fxs = [];
    this.gun_cocked = false;
    this.gun_timer = 0.0;
    this.darken_delay = -1.0;
    this.darken_phase = 0;
    this.darken_elapsed = 0.0;
    this.blacken_timer = -1.0;
    set_stage_tints("#FFFFFFFF", true);
}

fn onOpponentNoteHit(ev) {
    let kind = ev.get("kind");
    if kind == () { return; }

    if kind == "weekend-1-lightcan" || kind == "weekend-1-kneecan" {
        ev.set("play_anim", false);
    } else if kind == "weekend-1-kickcan" {
        ev.set("play_anim", false);
        let pile_pos = if prop_exists("spraycanPile") { get_prop_pos("spraycanPile") } else { [920.0, 1045.0] };
        let pile_z = if prop_exists("spraycanPile") { get_prop_z("spraycanPile") } else { 880.0 };
        let cx = pile_pos[0] - 10.0;
        let cy = pile_pos[1] - 550.0;
        let cz = pile_z - 2.0;

        this.can_seq += 1;
        let cid = "2hot_spraycan_" + this.can_seq;
        spawn_atlas(
            cid,
            "gameplay/songs/2hot/spraycan",
            cx,
            cy,
            cz,
            [
                #{ name: "can-start", start: 0, end: 18, fps: 24.0, looped: false },
                #{ name: "can-hit", start: 19, end: 25, fps: 24.0, looped: false },
                #{ name: "can-shot", start: 26, end: 42, fps: 24.0, looped: false }
            ]
        );
        play_anim(cid, "can-start");
        this.cans.push(#{
            id: cid,
            state: "arcing",
            phase: "start",
            x: cx,
            y: cy,
            z: cz,
            age: 0.0,
            explode_spawned: false
        });
    }
}

fn onNoteHit(ev) {
    let kind = ev.get("kind");
    if kind == () { return; }

    if kind == "weekend-1-lightcan" || kind == "weekend-1-kneecan" || kind == "weekend-1-kickcan" {
        onOpponentNoteHit(ev);
    } else if kind == "weekend-1-cockgun" {
        ev.set("play_anim", false);
        this.gun_cocked = true;
        this.gun_timer = 1.0;
    } else if kind == "weekend-1-firegun" {
        ev.set("play_anim", false);
        if this.gun_cocked {
            let shot_idx = -1;
            let i = 0;
            while i < this.cans.len() {
                if this.cans[i].state == "arcing" {
                    shot_idx = i;
                    break;
                }
                i += 1;
            }
            if shot_idx >= 0 {
                let can = this.cans[shot_idx];
                can.state = "shot";
                can.phase = "shot";
                can.age = 0.0;
                can.explode_spawned = false;
                play_anim(can.id, "can-shot");
                this.cans[shot_idx] = can;
                this.darken_delay = 1.0 / 24.0;
            }
        } else {
            ev.cancel();
        }
    }
}

fn onNoteMiss(ev) {
    let kind = ev.get("kind");
    if kind == () { return; }

    if kind == "weekend-1-cockgun" {
        ev.set("play_anim", false);
        ev.set("health_change", 0.0);
    } else if kind == "weekend-1-firegun" || kind == "weekend-1-firegun-hip" || kind == "weekend-1-firegun-far" {
        ev.set("play_anim", false);
        ev.set("health_change", 0.0);
        this.gun_cocked = false;
        this.gun_timer = 0.0;

        let i = 0;
        while i < this.cans.len() {
            if this.cans[i].state == "arcing" {
                let can = this.cans[i];
                can.state = "impacted";
                this.cans[i] = can;
                break;
            }
            i += 1;
        }

        // Can explosion damage (0.5 out of 2.0 health bar, matching 2hot.hxc HEALTH_LOSS = 0.25 * 2).
        add_hp(-0.5);
        if hp() <= 0.0 {
            global_set("gameover_music_suffix", "-pico-explode");
            global_set("gameover_blueball_suffix", "-pico-explode");
            global_set("gameover_anim_suffix", "-explosion");
            if !practice() {
                set_stage_tints("#FF000000", true);
                this.blacken_timer = 1.0;
            }
        }
    }
}

fn onUpdate() {
    let elapsed = dt();

    if this.gun_cocked {
        this.gun_timer -= elapsed;
        if this.gun_timer <= 0.0 {
            this.gun_cocked = false;
            this.gun_timer = 0.0;
        }
    }

    if this.darken_delay > 0.0 {
        this.darken_delay -= elapsed;
        if this.darken_delay <= 0.0 {
            this.darken_delay = -1.0;
            set_stage_tints("#FF111111", false);
            this.darken_phase = 1;
            this.darken_elapsed = 0.0;
        }
    }

    if this.darken_phase == 1 {
        this.darken_elapsed += elapsed;
        if this.darken_elapsed >= (1.0 / 24.0) {
            this.darken_phase = 2;
            this.darken_elapsed = 0.0;
            set_stage_tints("#FF222222", false);
        }
    } else if this.darken_phase == 2 {
        this.darken_elapsed += elapsed;
        let t = clamp(this.darken_elapsed / 1.4, 0.0, 1.0);
        let hex = rgb_gray_hex(34.0 + 221.0 * t);
        set_stage_tints(hex, false);
        if t >= 1.0 {
            this.darken_phase = 0;
            set_stage_tints("#FFFFFFFF", false);
        }
    }

    if this.blacken_timer > 0.0 {
        this.blacken_timer -= elapsed;
        if this.blacken_timer <= 0.0 {
            this.blacken_timer = -1.0;
            set_stage_tints("#FFFFFFFF", true);
        }
    }

    let next_cans = [];
    for can in this.cans {
        can.age += elapsed;
        if can.phase == "shot" && !can.explode_spawned && can.age >= (3.0 / 24.0) {
            can.explode_spawned = true;
            this.fx_seq += 1;
            let fx_id = "2hot_fx_" + this.fx_seq;
            spawn_sparrow(
                fx_id,
                "gameplay/songs/2hot/graphics/spraycan-explosion",
                can.x + 150.0,
                can.y - 250.0,
                can.z + 1.0,
                [#{ name: "idle", prefix: "Explosion 1 movie0", fps: 24.0, looped: false }]
            );
            play_anim(fx_id, "idle");
            this.fxs.push(fx_id);
        }

        if prop_anim_done(can.id) {
            if can.phase == "start" {
                can.phase = "hit";
                can.age = 0.0;
                play_anim(can.id, "can-hit");
                next_cans.push(can);
            } else if can.phase == "hit" {
                this.fx_seq += 1;
                let fx_id = "2hot_fx_" + this.fx_seq;
                spawn_sparrow(
                    fx_id,
                    "gameplay/songs/2hot/graphics/spraycan-explosion-ez",
                    can.x + 750.0,
                    can.y - 100.0,
                    can.z + 1.0,
                    [#{ name: "idle", prefix: "explosion round 1 short0", fps: 24.0, looped: false }]
                );
                play_anim(fx_id, "idle");
                this.fxs.push(fx_id);
                despawn(can.id);
            } else {
                despawn(can.id);
            }
        } else {
            next_cans.push(can);
        }
    }
    this.cans = next_cans;

    let next_fxs = [];
    for fx_id in this.fxs {
        if prop_exists(fx_id) && prop_anim_done(fx_id) {
            despawn(fx_id);
        } else if prop_exists(fx_id) {
            next_fxs.push(fx_id);
        }
    }
    this.fxs = next_fxs;
}

fn onSongEnd(ev) {
    if !this.has_played_cutscene {
        this.has_played_cutscene = true;
        ev.cancel();
        play_video("gameplay/songs/2hot/cutscene.mp4", "ending");
    } else {
        this.has_played_cutscene = false;
    }
}
