// Girlfriend character script (ported from gf.hxc)

fn get_target() {
    if char_id("gf") == "gf" { return "gf"; }
    if char_id("dad") == "gf" { return "dad"; }
    if char_id("bf") == "gf" { return "bf"; }
    ""
}

fn onCreate() {
    this.was_hair_playing = false;
}

fn onUpdate() {
    let target = get_target();
    if target == "" { return; }
    let cur = char_anim(target);
    if cur.starts_with("hair") && !char_anim_finished(target) {
        if !this.was_hair_playing {
            this.was_hair_playing = true;
            set_dance_every(target, 0.0);
        }
    } else if this.was_hair_playing {
        this.was_hair_playing = false;
        set_dance_every(target, 1.0);
    }
}
