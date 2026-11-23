// Sserafim Sakura character script (ported from sserafim-sakura.hxc)

fn lip_sync_offsets() {
    #{
        idle: #{ offset: [7, 2], angle: -14 },
        singUP: #{ offset: [8, 1], angle: -15 },
        singRIGHT: #{ offset: [7, 2], angle: -15 },
        singDOWN: #{ offset: [6, 3], angle: -15 },
        singLEFT: #{ offset: [7, 2], angle: -14 },
        "singUP-joint": #{ offset: [10, -1], angle: -14 },
        "singRIGHT-joint": #{ offset: [6, 3], angle: -15 },
        "singDOWN-joint": #{ offset: [5, 5], angle: -15 },
        "singLEFT-joint": #{ offset: [7, 2], angle: -16 }
    }
}

fn find_param(payload, key) {
    if payload == () { return (); }
    let list = payload["params"];
    if type_of(list) == "array" {
        for entry in list {
            if entry["name"] == key {
                return entry["value"];
            }
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

fn find_slot() {
    if char_id("bf") == "sserafim-sakura" { return "bf"; }
    if char_id("dad") == "sserafim-sakura" { return "dad"; }
    "sserafim-sakura"
}

fn apply_idle_pose(target, offsets) {
    let p = offsets["idle"];
    char_attach_pose(target, "lipsync", p.offset[0], p.offset[1], p.angle);
}

fn onCreate() {
    register_notekind("sakura-joint", #{ description: "Sakura Joint Sing (Spaghetti)", suffix: "joint", scoreable: true });
    register_notekind("sakura-bf1", #{ description: "Sakura BF Pose 1 (Spaghetti)", suffix: "bf1", scoreable: true });
    register_notekind("sakura-bf2", #{ description: "Sakura BF Pose 2 (Spaghetti)", suffix: "bf2", scoreable: true });

    this.offsets = lip_sync_offsets();
    this.shader_cleared = false;
    this.should_sing = false;
    this.last_anim = "";
    this.last_frame = 0;

    let slot = find_slot();
    char_attach_atlas(slot, "lipsync", "mouth edit", "gameplay/stages/sserafim/sserafim-lipsync");
    char_attach_style(slot, "lipsync", true, 1.0);
    char_attach_frame(slot, "lipsync", 0);
    apply_idle_pose(slot, this.offsets);
}

fn onSongRetry() {
    this.shader_cleared = false;
    this.should_sing = false;
    this.last_anim = "";
    this.last_frame = 0;
    let slot = find_slot();
    char_attach_frame(slot, "lipsync", 0);
    apply_idle_pose(slot, this.offsets);
}

fn onGameOver() {
    if char_id("bf") == "sserafim-sakura" {
        char_visible("bf", true);
        icon_visible("dad", false);
        clear_shader("bf");
        this.shader_cleared = true;
    }
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
        if items.len() >= 5 {
            this.should_sing = parse_bool_item(items[4]);
            if !this.should_sing && this.last_frame != 0 {
                this.last_frame = 0;
                char_attach_frame(find_slot(), "lipsync", 0);
            }
        }
    }
}

fn onNoteMiss(ev) {
    if !this.should_sing {
        return;
    }
    let kind = ev.get("kind");
    let dir = ev.get("dir");
    if dir == () || kind == () {
        return;
    }
    let slot = find_slot();
    if kind == "sakura-joint" {
        play_sing(slot, dir, true, "joint");
    } else if kind == "sakura-bf1" || kind == "sakura-bf2" {
        play_sing(slot, dir, true, "bf2");
    }
}

fn onUpdate() {
    let target = find_slot();
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
    if char_id("bf") == "sserafim-sakura" && !this.shader_cleared && cur == "firstDeath" {
        char_visible("bf", true);
        icon_visible("dad", false);
        clear_shader("bf");
        this.shader_cleared = true;
    }
}
