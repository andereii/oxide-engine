// Pico (Speaker) character script (ported from pico-speaker.hxc)

fn find_slot(id) {
    if char_id("gf") == id { return "gf"; }
    if char_id("dad") == id { return "dad"; }
    if char_id("bf") == id { return "bf"; }
    ""
}

fn sort_notes(list) {
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

fn load_picospeaker_timings() {
    let empty = [];
    let s = song();
    if s == "" { return empty; }
    let v = variation();
    let candidates = [];
    let keys = [];
    if v != "" && v != "default" {
        candidates.push("gameplay/songs/" + s + "/chart-" + v + "/notes.json");
        candidates.push("gameplay/songs/" + s + "/" + v + "/notes.json");
        keys.push(v + "-picospeaker");
    }
    candidates.push("gameplay/songs/" + s + "/chart/notes.json");
    keys.push("picospeaker");

    let notes = ();
    for path in candidates {
        if file_exists(path) {
            for k in keys {
                notes = read_json(path, k);
                if notes != () { break; }
            }
            if notes != () { break; }
        }
    }
    if notes == () { return empty; }

    let list = [];
    for item in notes {
        let t = item["t"];
        if t == () { t = item["time"]; }
        let d = item["d"];
        if d == () { d = item["data"]; }
        if t != () && d != () {
            list.push(#{ t: t * 1.0, d: d });
        }
    }
    sort_notes(list)
}

fn pick_shot_dir(raw_dir) {
    let dir = raw_dir;
    if dir == 3 {
        dir -= rand_int(0, 1);
    } else {
        dir += rand_int(0, 1);
    }
    if dir < 0 { dir = 0; }
    if dir > 3 { dir = 3; }
    dir
}

fn onCreate() {
    this.shoot_notes = [];
    this.shoot_idx = 0;
    this.last_now_ms = 0.0;
    this.shoot_notes = load_picospeaker_timings();
    let target = find_slot("pico-speaker");
    if target != "" {
        play_anim(target, "idle");
    }
}

fn onSongRetry() {
    this.shoot_idx = 0;
    this.last_now_ms = 0.0;
    let target = find_slot("pico-speaker");
    if target != "" {
        play_anim(target, "idle");
    }
}

fn onUpdate() {
    let target = find_slot("pico-speaker");
    if target == "" { return; }
    let now_ms = time();
    if now_ms < this.last_now_ms - 50.0 {
        this.shoot_idx = 0;
    }
    this.last_now_ms = now_ms;

    let last_note = ();
    while this.shoot_idx < this.shoot_notes.len() {
        let note = this.shoot_notes[this.shoot_idx];
        if note.t > now_ms { break; }
        this.shoot_idx += 1;
        last_note = note;
    }
    if last_note != () {
        let dir = pick_shot_dir(last_note.d);
        if now_ms - last_note.t <= 500.0 {
            play_anim(target, "shoot" + (dir + 1));
        } else {
            play_anim(target, "shoot" + (dir + 1) + "-hold");
        }
    }
}
