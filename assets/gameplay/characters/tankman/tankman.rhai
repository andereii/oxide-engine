// Tankman character script (ported from tankman.hxc)

fn handle_note(target, ev) {
    let kind = ev.get("kind");
    if kind == "ugh" {
        play_anim(target, "ugh");
    } else if kind == "hehPrettyGood" {
        play_anim(target, "hehPrettyGood");
    }
}

fn onOpponentNoteHit(ev) {
    if char_id("dad") == "tankman" {
        handle_note("dad", ev);
    }
}

fn onNoteHit(ev) {
    if char_id("bf") == "tankman" {
        handle_note("bf", ev);
    }
}
