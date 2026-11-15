// Boyfriend (Dark) character script (ported from bf-dark.hxc)

fn find_slot(id) {
    if char_id("bf") == id { return "bf"; }
    if char_id("dad") == id { return "dad"; }
    if char_id("gf") == id { return "gf"; }
    ""
}

fn onCreate() {
    this.slot = find_slot("bf-dark");
    this.last_anim = "";
}

fn onUpdate() {
    let target = if this.slot == "" { find_slot("bf-dark") } else { this.slot };
    if target == "" { return; }
    let cur = char_anim(target);
    if cur != this.last_anim {
        this.last_anim = cur;
        global_set("bf_dark_last_anim", cur);
    }
}
