// Sserafim Girlfriend character script (ported from sserafim-gf.hxc)

fn find_param(payload, key) {
    if payload == () {
        return ();
    }
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

fn to_bool_or(val, fallback) {
    if val == () {
        return fallback;
    }
    if type_of(val) == "bool" {
        return val;
    }
    if type_of(val) == "string" {
        let s = val;
        s.trim();
        s = s.to_lower();
        if s == "true" || s == "1" { return true; }
        if s == "false" || s == "0" { return false; }
    }
    fallback
}

fn find_gf_slot() {
    if char_id("gf") == "sserafim-gf" { return "gf"; }
    if char_id("sserafim-gf") == "sserafim-gf" { return "sserafim-gf"; }
    ""
}

fn onCreate() {
    this.is_beautiful = false;
    let slot = find_gf_slot();
    if slot != "" {
        char_suffix(slot, "");
    }
}

fn onSongRetry() {
    this.is_beautiful = false;
    let slot = find_gf_slot();
    if slot != "" {
        char_suffix(slot, "");
    }
}

fn onSongEvent(ev) {
    let kind = ev.get("kind");
    let raw_val = ev.get("value");
    let event_name = kind;
    if kind == "CustomEvent" && raw_val != () {
        let inner = raw_val["name"];
        if inner != () {
            event_name = inner;
        }
    }
    if event_name == "sserafimBeautiful" {
        let next_beautiful = to_bool_or(find_param(raw_val, "beautiful"), false);
        if next_beautiful != this.is_beautiful {
            this.is_beautiful = next_beautiful;
            let slot = find_gf_slot();
            if slot != "" {
                let suffix = if next_beautiful { "-beautiful" } else { "" };
                char_suffix(slot, suffix);
                let cur = char_anim(slot);
                if next_beautiful && cur != "" && !cur.ends_with("-beautiful") {
                    play_anim(slot, cur + "-beautiful");
                } else if !next_beautiful && cur.ends_with("-beautiful") {
                    play_anim(slot, cur.sub_string(0, cur.len() - "-beautiful".len()));
                }
            }
        }
    }
}
