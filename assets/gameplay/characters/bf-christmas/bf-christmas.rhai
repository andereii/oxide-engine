// Boyfriend (Christmas) character script (ported from bf-christmas.hxc)

fn find_slot(id) {
    if char_id("bf") == id { return "bf"; }
    if char_id("dad") == id { return "dad"; }
    if char_id("gf") == id { return "gf"; }
    ""
}

fn handle_note(target, ev) {
    let kind = ev.get("kind");
    if kind == "censor" {
        let dir = ev.get("dir");
        if dir != () {
            play_sing(target, dir, false, "censor");
        }
    } else if kind == "hey" {
        play_anim(target, "hey");
    }
}

fn onNoteHit(ev) {
    if char_id("bf") == "bf-christmas" {
        handle_note("bf", ev);
    }
}

fn onOpponentNoteHit(ev) {
    if char_id("dad") == "bf-christmas" {
        handle_note("dad", ev);
    }
}
