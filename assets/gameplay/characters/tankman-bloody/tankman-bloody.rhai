// Tankman (Bloody) character script (ported from tankman-bloody.hxc)

fn get_target() {
    if char_id("dad") == "tankman-bloody" { return "dad"; }
    if char_id("bf") == "tankman-bloody" { return "bf"; }
    ""
}

fn onCreate() {
    this.is_bloody = false;
    this.last_anim = "";
    this.played_ending = false;
}

fn onSongRetry() {
    this.is_bloody = false;
    this.last_anim = "";
    this.played_ending = false;
}

fn onUpdate() {
    let target = get_target();
    if target == "" { return; }
    let cur = char_anim(target);
    if cur == this.last_anim { return; }
    this.last_anim = cur;

    if cur == "redheadsAnim" {
        this.is_bloody = true;
    } else if cur == "stressPicoEnding" && !this.played_ending {
        this.played_ending = true;
        sfx("gameplay/songs/stress/pico/sounds/end-cutscene", 1.0);
    }

    if this.is_bloody && cur != "" && !cur.ends_with("-bloody") && cur != "redheadsAnim" && cur != "stressPicoEnding" {
        play_anim(target, cur + "-bloody");
        this.last_anim = cur + "-bloody";
    }
}

fn handle_note(state, target, ev) {
    let kind = ev.get("kind");
    if kind == "ugh" {
        let anim_name = if state.is_bloody { "ugh-bloody" } else { "ugh" };
        play_anim(target, anim_name);
    } else if kind == "hehPrettyGood" {
        let anim_name = if state.is_bloody { "hehPrettyGood-bloody" } else { "hehPrettyGood" };
        play_anim(target, anim_name);
    }
}

fn onOpponentNoteHit(ev) {
    if char_id("dad") == "tankman-bloody" {
        handle_note(this, "dad", ev);
    }
}

fn onNoteHit(ev) {
    if char_id("bf") == "tankman-bloody" {
        handle_note(this, "bf", ev);
    }
}
