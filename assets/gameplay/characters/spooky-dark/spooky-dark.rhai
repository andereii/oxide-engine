// Spooky Kids (Dark) character script (ported from spooky-dark.hxc)

fn find_slot(id) {
    if char_id("dad") == id { return "dad"; }
    if char_id("bf") == id { return "bf"; }
    if char_id("gf") == id { return "gf"; }
    ""
}

fn onCreate() {
    let target = find_slot("spooky-dark");
    if target != "" {
        char_underlay(target, "spooky");
    }
}

fn handle_note(target, ev) {
    if ev.get("kind") == "cheer" {
        play_anim(target, "cheer");
    }
}

fn onOpponentNoteHit(ev) {
    if char_id("dad") == "spooky-dark" {
        handle_note("dad", ev);
    }
}

fn onNoteHit(ev) {
    if char_id("bf") == "spooky-dark" {
        handle_note("bf", ev);
    }
}
