// Spooky Kids (Dark) character script (ported from spooky-dark.hxc)

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
