// Spirit character script (ported from spirit.hxc)

fn find_slot(id) {
    if char_id("dad") == id { return "dad"; }
    if char_id("bf") == id { return "bf"; }
    ""
}

fn onCreate() {
    this.slot = find_slot("spirit");
    this.trail_delay = 0.069;
}
