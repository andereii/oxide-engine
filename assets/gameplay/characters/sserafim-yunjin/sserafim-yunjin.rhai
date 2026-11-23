// Sserafim Yunjin character script (ported from sserafim-yunjin.hxc)

fn lip_sync_offsets() {
    #{
        idle: #{ offset: [8, 6], angle: 23 },
        singUP: #{ offset: [6, 8], angle: 22 },
        singRIGHT: #{ offset: [6, 8], angle: 23 },
        singDOWN: #{ offset: [8, 6], angle: 23 },
        singLEFT: #{ offset: [6, 8], angle: 23 }
    }
}

fn find_param(payload, key) {
    if payload == () { return (); }
    let list = payload["params"];
    if type_of(list) == "array" {
        for entry in list {
            if entry["name"] == key { return entry["value"]; }
        }
    }
    payload[key]
}

fn parse_bool_item(raw) {
    if type_of(raw) == "bool" { return raw; }
    let s = raw.to_string();
    s.trim();
    s = s.to_lower();
    s == "true" || s == "1"
}

fn onCreate() {
    this.offsets = lip_sync_offsets();
    this.should_sing = false;
    this.last_anim = "";
    this.last_frame = 0;

    let target = "sserafim-yunjin";
    char_attach_atlas(target, "lipsync", "mouth yunjin", "gameplay/stages/sserafim/sserafim-lipsync-yunjin");
    char_attach_style(target, "lipsync", false, 1.0);
    char_attach_frame(target, "lipsync", 0);
    let p = this.offsets["idle"];
    char_attach_pose(target, "lipsync", p.offset[0], p.offset[1], p.angle);
}

fn onSongRetry() {
    this.should_sing = false;
    this.last_anim = "";
    this.last_frame = 0;
    let target = "sserafim-yunjin";
    char_attach_frame(target, "lipsync", 0);
    let p = this.offsets["idle"];
    char_attach_pose(target, "lipsync", p.offset[0], p.offset[1], p.angle);
}

fn onSongEvent(ev) {
    let kind = ev.get("kind");
    let raw_val = ev.get("value");
    let event_name = kind;
    if kind == "CustomEvent" && raw_val != () {
        let inner = raw_val["name"];
        if inner != () { event_name = inner; }
    }
    if event_name == "sserafimSing" {
        let raw_arr = find_param(raw_val, "singing");
        let items = if type_of(raw_arr) == "array" {
            raw_arr
        } else if type_of(raw_arr) == "string" {
            raw_arr.split(",")
        } else {
            []
        };
        if items.len() >= 1 {
            this.should_sing = parse_bool_item(items[0]);
            if !this.should_sing && this.last_frame != 0 {
                this.last_frame = 0;
                char_attach_frame("sserafim-yunjin", "lipsync", 0);
            }
        }
    }
}

fn onUpdate() {
    let target = "sserafim-yunjin";
    let cur = char_anim(target);
    if cur != this.last_anim {
        this.last_anim = cur;
        let entry = this.offsets[cur];
        if entry != () {
            char_attach_pose(target, "lipsync", entry.offset[0], entry.offset[1], entry.angle);
        }
    }
    if this.should_sing {
        let raw_f = (time() / 1000.0) * 24.0;
        let f = if raw_f < 1.0 { 0 } else { raw_f.to_int() - 1 };
        if f != this.last_frame {
            this.last_frame = f;
            char_attach_frame(target, "lipsync", f);
        }
    }
}
