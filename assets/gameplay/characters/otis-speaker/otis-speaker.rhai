// Otis (Speaker) character script (ported from otis-speaker.hxc)

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

fn load_otis_timings() {
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

fn onCreate() {
    import "gameplay/characters/nene/ABotAtlasSprite" as abot_mod;
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    this.shoot_notes = load_otis_timings();
    this.shoot_idx = 0;
    this.last_now_ms = 0.0;
    this.abot = abot_mod::new_abot_state("otis-speaker");
    this.viz = viz_mod::new_viz_state(false);
    let target = find_slot("otis-speaker");
    if target != "" {
        let ab = abot_mod::spawn_abot(target, 170.0, 455.0);
        viz_mod::spawn_viz(ab[0], ab[1], ab[2], false);
    }
}

fn onSongStart() {
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    this.viz = viz_mod::start_viz(this.viz);
}

fn onSongRetry() {
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    this.shoot_idx = 0;
    this.last_now_ms = 0.0;
    this.viz = viz_mod::reset_viz(this.viz);
}

fn onSongEnd() {
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    this.viz = viz_mod::reset_viz(this.viz);
}

fn onUpdate() {
    let target = find_slot("otis-speaker");
    if target == "" { return; }
    let now_ms = time();
    if now_ms < this.last_now_ms - 50.0 {
        this.shoot_idx = 0;
    }
    this.last_now_ms = now_ms;

    let latest_dir = ();
    while this.shoot_idx < this.shoot_notes.len() {
        let note = this.shoot_notes[this.shoot_idx];
        if note.t > now_ms { break; }
        this.shoot_idx += 1;
        latest_dir = note.d;
    }
    if latest_dir != () {
        let dir = latest_dir;
        if dir < 0 { dir = 0; }
        if dir > 3 { dir = 3; }
        play_anim(target, "shoot" + (dir + 1));
    }
}

fn onBeatHit() {
    import "gameplay/characters/nene/ABotAtlasSprite" as abot_mod;
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    abot_mod::dance_abot();
    viz_mod::pulse_viz(this.viz);
}

fn onSongEvent(ev) {
    import "gameplay/characters/nene/ABotAtlasSprite" as abot_mod;
    this.abot = abot_mod::on_FocusCamera(this.abot, ev);
}

fn onNoteHit(ev) {
    import "gameplay/characters/nene/ABotVisualizer" as viz_mod;
    viz_mod::pulse_viz(this.viz);
}
