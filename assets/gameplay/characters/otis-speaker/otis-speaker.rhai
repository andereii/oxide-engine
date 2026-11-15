// Otis (Speaker) character script (ported from otis-speaker.hxc)
import "gameplay/characters/nene/ABotAtlasSprite" as abot_mod;
import "gameplay/characters/nene/ABotVisualizer" as viz_mod;

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
    if v != "" && v != "default" {
        candidates.push("gameplay/songs/" + s + "/chart-" + v + "/notes.json");
        candidates.push("gameplay/songs/" + s + "/" + v + "/notes.json");
    }
    candidates.push("gameplay/songs/" + s + "/chart/notes.json");

    let raw = ();
    for path in candidates {
        if file_exists(path) {
            raw = read_json(path);
            if raw != () { break; }
        }
    }
    if raw == () { return empty; }

    let notes = raw["picospeaker"];
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
    this.shoot_notes = load_otis_timings();
    this.shoot_idx = 0;
    this.abot = abot_mod::new_abot_state("otis-speaker");
    this.viz = viz_mod::new_viz_state(false);
    let target = find_slot("otis-speaker");
    if target != "" {
        let pos = char_pos(target);
        spawn_sprite("abot_stereo_bg", "gameplay/characters/nene/stereo-bg.png", pos[0] + 320.0, pos[1] + 485.0, 40.0);
    }
}

fn onSongStart() {
    this.viz = viz_mod::start_viz(this.viz);
}

fn onSongRetry() {
    this.shoot_idx = 0;
    this.viz = viz_mod::reset_viz(this.viz);
}

fn onSongEnd() {
    this.viz = viz_mod::reset_viz(this.viz);
}

fn onUpdate() {
    let target = find_slot("otis-speaker");
    if target == "" { return; }
    let now_ms = time();
    while this.shoot_idx < this.shoot_notes.len() {
        let note = this.shoot_notes[this.shoot_idx];
        if note.t > now_ms { break; }
        this.shoot_idx += 1;

        let dir = note.d;
        if dir < 0 { dir = 0; }
        if dir > 3 { dir = 3; }
        play_anim(target, "shoot" + (dir + 1));
    }
}

fn onSongEvent(ev) {
    this.abot = abot_mod::on_FocusCamera(this.abot, ev);
}
