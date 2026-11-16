// Spirit character script (ported from spirit.hxc)

fn find_slot(id) {
    if char_id("dad") == id { return "dad"; }
    if char_id("bf") == id { return "bf"; }
    ""
}

fn onCreate() {
    let slot = find_slot("spirit");
    if slot != "" {
        char_trail(slot, 4, 0.4, 0.3, 0.069, 190);
    }
}
