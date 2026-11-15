// Darnell character script (ported from darnell.hxc)

fn handle_can_note(target, ev) {
    let kind = ev.get("kind");
    if kind == "weekend-1-lightcan" {
        play_anim(target, "lightCan");
    } else if kind == "weekend-1-kickcan" {
        play_anim(target, "kickCan");
    } else if kind == "weekend-1-kneecan" {
        play_anim(target, "kneeCan");
    }
}

fn onOpponentNoteHit(ev) {
    if char_id("dad") == "darnell" {
        handle_can_note("dad", ev);
    }
}

fn onNoteHit(ev) {
    if char_id("bf") == "darnell" {
        handle_can_note("bf", ev);
    }
}
