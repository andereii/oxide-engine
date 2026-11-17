// Girlfriend (Dark) character script (ported from gf-dark.hxc)

fn find_slot(id) {
    if char_id("gf") == id { return "gf"; }
    if char_id("dad") == id { return "dad"; }
    if char_id("bf") == id { return "bf"; }
    ""
}

fn onCreate() {
    this.slot = find_slot("gf-dark");
    this.last_anim = "";
    if this.slot != "" {
        char_underlay(this.slot, "gf", -45.5, 0.0);
    }
}

fn onUpdate() {
    let target = if this.slot == "" { find_slot("gf-dark") } else { this.slot };
    if target == "" { return; }
    let cur = char_anim(target);
    if cur != this.last_anim {
        this.last_anim = cur;
        global_set("gf_dark_last_anim", cur);
    }
}
