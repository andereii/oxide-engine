// Pico (Blazin) character script (ported from pico-blazin.hxc)

fn get_target() {
    if char_id("bf") == "pico-blazin" { return "bf"; }
    if char_id("dad") == "pico-blazin" { return "dad"; }
    ""
}

fn do_alternate(state) {
    state.alternate = !state.alternate;
    if state.alternate { "1" } else { "2" }
}

fn move_to_back(target) { set_char_z(target, -300.0); }
fn move_to_front(target) { set_char_z(target, -200.0); }

fn play_block(target) {
    move_to_back(target);
    play_anim(target, "block");
    shake(0.002, 0.1);
}

fn play_cringe(target) { move_to_back(target); play_anim(target, "cringe"); }
fn play_dodge(target) { move_to_back(target); play_anim(target, "dodge"); }
fn play_idle(target) { move_to_back(target); play_anim(target, "idle"); }
fn play_fakeout(target) { move_to_back(target); play_anim(target, "fakeout"); }
fn play_uppercut_prep(target) { move_to_front(target); play_anim(target, "uppercutPrep"); }
fn play_uppercut(target, hit) {
    move_to_front(target);
    play_anim(target, "uppercut");
    if hit { shake(0.005, 0.25); }
}
fn play_uppercut_hit(target) {
    move_to_back(target);
    play_anim(target, "uppercutHit");
    shake(0.005, 0.25);
}
fn play_hit_high(target) {
    move_to_back(target);
    play_anim(target, "hitHigh");
    shake(0.0025, 0.15);
}
fn play_hit_low(target) {
    move_to_back(target);
    play_anim(target, "hitLow");
    shake(0.0025, 0.15);
}
fn play_hit_spin(target) {
    move_to_back(target);
    play_anim(target, "hitSpin");
    shake(0.0025, 0.15);
}
fn play_punch_high(alt, target) {
    move_to_front(target);
    let next = !alt;
    play_anim(target, "punchHigh" + if next { "1" } else { "2" });
    next
}
fn play_punch_low(alt, target) {
    move_to_front(target);
    let next = !alt;
    play_anim(target, "punchLow" + if next { "1" } else { "2" });
    next
}
fn play_taunt_cond(target) {
    move_to_back(target);
    if char_anim(target) == "fakeout" {
        play_anim(target, "taunt");
    } else {
        play_idle(target);
    }
}

fn is_poor_judgement(j) {
    j == "bad" || j == "shit"
}

fn is_darnell_prepping() {
    char_anim("dad") == "uppercutPrep"
}

fn is_darnell_in_uppercut() {
    let a = char_anim("dad");
    a == "uppercut" || a == "uppercut-hold"
}

fn onCreate() {
    this.alternate = false;
    this.cant_uppercut = false;
    global_set("gameover_music_suffix", "-pico");
    global_set("gameover_blueball_suffix", "-pico-gutpunch");
    global_set("pause_music_suffix", "-pico");
    let target = get_target();
    if target != "" {
        set_dance_every(target, 0.0);
        play_anim(target, "idle");
    }
}

fn onSongRetry() {
    this.alternate = false;
    this.cant_uppercut = false;
    global_set("gameover_music_suffix", "-pico");
    global_set("gameover_blueball_suffix", "-pico-gutpunch");
    global_set("pause_music_suffix", "-pico");
    let target = get_target();
    if target != "" {
        set_dance_every(target, 0.0);
        play_anim(target, "idle");
    }
}

fn handle_blazin_hit(state, ev) {
    let target = get_target();
    if target == "" { return state; }
    let kind = ev.get("kind");
    if kind == () || !kind.starts_with("weekend-1-") { return state; }

    let judge = ev.get("judgement");
    if judge == () { judge = ""; }
    if is_poor_judgement(judge) && (hp() <= 0.6) && is_darnell_prepping() {
        state.alternate = play_punch_high(state.alternate, target);
        return state;
    }

    if state.cant_uppercut {
        play_block(target);
        state.cant_uppercut = false;
        return state;
    }

    if kind == "weekend-1-punchlow" || kind == "weekend-1-punchlowblocked" || kind == "weekend-1-punchlowdodged" || kind == "weekend-1-punchlowspin" {
        state.alternate = play_punch_low(state.alternate, target);
    }
    else if kind == "weekend-1-punchhigh" || kind == "weekend-1-punchhighblocked" || kind == "weekend-1-punchhighdodged" || kind == "weekend-1-punchhighspin" {
        state.alternate = play_punch_high(state.alternate, target);
    }
    else if kind == "weekend-1-blockhigh" || kind == "weekend-1-blocklow" || kind == "weekend-1-blockspin" {
        play_block(target);
    }
    else if kind == "weekend-1-dodgehigh" || kind == "weekend-1-dodgelow" || kind == "weekend-1-dodgespin" {
        play_dodge(target);
    }
    else if kind == "weekend-1-hithigh" { play_hit_high(target); }
    else if kind == "weekend-1-hitlow" { play_hit_low(target); }
    else if kind == "weekend-1-hitspin" { play_hit_spin(target); }
    else if kind == "weekend-1-picouppercutprep" { play_uppercut_prep(target); }
    else if kind == "weekend-1-picouppercut" { play_uppercut(target, true); }
    else if kind == "weekend-1-darnelluppercutprep" { play_idle(target); }
    else if kind == "weekend-1-darnelluppercut" { play_uppercut_hit(target); }
    else if kind == "weekend-1-idle" { play_idle(target); }
    else if kind == "weekend-1-fakeout" { play_fakeout(target); }
    else if kind == "weekend-1-taunt" { play_taunt_cond(target); }
    else if kind == "weekend-1-tauntforce" { play_anim(target, "taunt"); }
    else if kind == "weekend-1-reversefakeout" { play_idle(target); }
    state
}

fn onNoteHit(ev) {
    let next = handle_blazin_hit(#{ alternate: this.alternate, cant_uppercut: this.cant_uppercut }, ev);
    this.alternate = next.alternate;
    this.cant_uppercut = next.cant_uppercut;
}

fn onOpponentNoteHit(ev) {
    let next = handle_blazin_hit(#{ alternate: this.alternate, cant_uppercut: this.cant_uppercut }, ev);
    this.alternate = next.alternate;
    this.cant_uppercut = next.cant_uppercut;
}

fn onNoteMiss(ev) {
    let target = get_target();
    if target == "" { return; }
    let kind = ev.get("kind");
    if kind == () || !kind.starts_with("weekend-1-") { return; }

    if is_darnell_in_uppercut() {
        play_uppercut_hit(target);
        return;
    }

    let hc = ev.get("health_change");
    if hc != () && (hp() + hc) <= 0.0 {
        play_hit_low(target);
        return;
    }

    if this.cant_uppercut {
        play_block(target);
        this.cant_uppercut = false;
        return;
    }

    if kind == "weekend-1-punchlow" || kind == "weekend-1-punchlowblocked" || kind == "weekend-1-punchlowdodged" || kind == "weekend-1-punchlowspin" {
        play_cringe(target);
    }
    else if kind == "weekend-1-punchhigh" || kind == "weekend-1-punchhighblocked" || kind == "weekend-1-punchhighdodged" || kind == "weekend-1-punchhighspin" {
        play_cringe(target);
    }
    else if kind == "weekend-1-blockhigh" || kind == "weekend-1-dodgehigh" || kind == "weekend-1-hithigh" {
        play_hit_high(target);
    }
    else if kind == "weekend-1-blocklow" || kind == "weekend-1-dodgelow" || kind == "weekend-1-hitlow" {
        play_hit_low(target);
    }
    else if kind == "weekend-1-blockspin" || kind == "weekend-1-dodgespin" || kind == "weekend-1-hitspin" {
        play_hit_spin(target);
    }
    else if kind == "weekend-1-picouppercutprep" {
        this.alternate = play_punch_high(this.alternate, target);
        this.cant_uppercut = true;
    }
    else if kind == "weekend-1-picouppercut" { play_uppercut(target, false); }
    else if kind == "weekend-1-darnelluppercutprep" { play_idle(target); }
    else if kind == "weekend-1-darnelluppercut" { play_uppercut_hit(target); }
    else if kind == "weekend-1-idle" { play_idle(target); }
    else if kind == "weekend-1-fakeout" { play_hit_high(target); }
    else if kind == "weekend-1-taunt" { play_taunt_cond(target); }
    else if kind == "weekend-1-tauntforce" { play_anim(target, "taunt"); }
    else if kind == "weekend-1-reversefakeout" { play_idle(target); }
}

fn onNoteGhostMiss(ev) {
    let target = get_target();
    if target == "" { return; }
    let hc = ev.get("health_change");
    if hc != () && (hp() + hc) <= 0.0 {
        play_hit_low(target);
    } else {
        this.alternate = play_punch_high(this.alternate, target);
    }
}
