// Girlfriend (Dark) character script (ported from gf-dark.hxc)

fn onCreate() {
    this.last_anim = "";
}

fn onUpdate() {
    let cur = char_anim("gf");
    if cur != this.last_anim {
        this.last_anim = cur;
        global_set("gf_dark_last_anim", cur);
    }
}
