// Parents (Christmas) character script (ported from parents-christmas.hxc)

fn handle_note(target, ev) {
    if ev.get("kind") == "mom" {
        let dir = ev.get("dir");
        if dir != () {
            play_sing(target, dir, false, "alt");
        }
    }
}

fn onOpponentNoteHit(ev) {
    if char_id("dad") == "parents-christmas" {
        handle_note("dad", ev);
    }
}

fn onNoteHit(ev) {
    if char_id("bf") == "parents-christmas" {
        handle_note("bf", ev);
    }
}
