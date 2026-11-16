// Darnell (Blazin) character script (ported from darnell-blazin.hxc)

fn get_target() {
    if char_id("dad") == "darnell-blazin" { return "dad"; }
    if char_id("bf") == "darnell-blazin" { return "bf"; }
    ""
}

fn do_alternate(state) {
    state.alternate = !state.alternate;
    if state.alternate { "1" } else { "2" }
}

fn play_block(target) {
    play_anim(target, "block");
    shake(0.002, 0.1);
}

fn play_cringe(target) { play_anim(target, "cringe"); }
fn play_dodge(target) { play_anim(target, "dodge"); }
fn play_idle(target) { play_anim(target, "idle"); }
fn play_fakeout(target) { play_anim(target, "fakeout"); }
fn play_pissed(target) { play_anim(target, "pissed"); }
fn play_pissed_cond(target) {
    if char_anim(target) == "cringe" {
        play_pissed(target);
    } else {
        play_idle(target);
    }
}
fn play_uppercut_prep(target) { play_anim(target, "uppercutPrep"); }
fn play_uppercut(target) { play_anim(target, "uppercut"); }
fn play_uppercut_hit(target) { play_anim(target, "uppercutHit"); }
fn play_hit_high(target) {
    play_anim(target, "hitHigh");
    shake(0.0025, 0.15);
}
fn play_hit_low(target) {
    play_anim(target, "hitLow");
    shake(0.0025, 0.15);
}
fn play_spin(target) {
    play_anim(target, "hitSpin");
    shake(0.0025, 0.15);
}
fn play_punch_high(alt, target) {
    let next = !alt;
    play_anim(target, "punchHigh" + if next { "1" } else { "2" });
    next
}
fn play_punch_low(alt, target) {
    let next = !alt;
    play_anim(target, "punchLow" + if next { "1" } else { "2" });
    next
}

fn is_poor_judgement(j) {
    j == "bad" || j == "shit"
}

fn is_player_low_health() {
    hp() <= 0.6
}

fn onCreate() {
    this.alternate = false;
    this.cant_uppercut = false;
    let target = get_target();
    if target != "" {
        set_dance_every(target, 0.0);
        play_anim(target, "idle");
    }
}

fn onSongRetry() {
    this.alternate = false;
    this.cant_uppercut = false;
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
    if is_poor_judgement(judge) && is_player_low_health() && rand_int(1, 100) <= 30 {
        play_uppercut_prep(target);
        return state;
    }

    if state.cant_uppercut {
        state.alternate = play_punch_high(state.alternate, target);
        return state;
    }

    if kind == "weekend-1-punchlow" { play_hit_low(target); }
    else if kind == "weekend-1-punchlowblocked" { play_block(target); }
    else if kind == "weekend-1-punchlowdodged" { play_dodge(target); }
    else if kind == "weekend-1-punchlowspin" { play_spin(target); }
    else if kind == "weekend-1-punchhigh" { play_hit_high(target); }
    else if kind == "weekend-1-punchhighblocked" { play_block(target); }
    else if kind == "weekend-1-punchhighdodged" { play_dodge(target); }
    else if kind == "weekend-1-punchhighspin" { play_spin(target); }
    else if kind == "weekend-1-blockhigh" || kind == "weekend-1-blockspin" || kind == "weekend-1-dodgehigh" || kind == "weekend-1-dodgespin" || kind == "weekend-1-hithigh" || kind == "weekend-1-hitspin" {
        state.alternate = play_punch_high(state.alternate, target);
    }
    else if kind == "weekend-1-blocklow" || kind == "weekend-1-dodgelow" || kind == "weekend-1-hitlow" {
        state.alternate = play_punch_low(state.alternate, target);
    }
    else if kind == "weekend-1-picouppercutprep" {
        // Darnell keeps current prep/idle until uppercut resolves
    }
    else if kind == "weekend-1-picouppercut" { play_uppercut_hit(target); }
    else if kind == "weekend-1-darnelluppercutprep" { play_uppercut_prep(target); }
    else if kind == "weekend-1-darnelluppercut" { play_uppercut(target); }
    else if kind == "weekend-1-idle" { play_idle(target); }
    else if kind == "weekend-1-fakeout" { play_cringe(target); }
    else if kind == "weekend-1-taunt" { play_pissed_cond(target); }
    else if kind == "weekend-1-tauntforce" { play_pissed(target); }
    else if kind == "weekend-1-reversefakeout" { play_fakeout(target); }

    state.cant_uppercut = false;
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

    if char_anim(target) == "uppercutPrep" {
        play_uppercut(target);
        return;
    }

    let hc = ev.get("health_change");
    if hc != () && (hp() + hc) <= 0.0 {
        this.alternate = play_punch_low(this.alternate, target);
        return;
    }

    if this.cant_uppercut {
        this.alternate = play_punch_high(this.alternate, target);
        return;
    }

    if kind == "weekend-1-punchlow" || kind == "weekend-1-punchlowblocked" || kind == "weekend-1-punchlowdodged" || kind == "weekend-1-punchlowspin" {
        this.alternate = play_punch_low(this.alternate, target);
    }
    else if kind == "weekend-1-punchhigh" || kind == "weekend-1-punchhighblocked" || kind == "weekend-1-punchhighdodged" || kind == "weekend-1-punchhighspin" {
        this.alternate = play_punch_high(this.alternate, target);
    }
    else if kind == "weekend-1-blockhigh" || kind == "weekend-1-blockspin" || kind == "weekend-1-dodgehigh" || kind == "weekend-1-dodgespin" || kind == "weekend-1-hithigh" || kind == "weekend-1-hitspin" {
        this.alternate = play_punch_high(this.alternate, target);
    }
    else if kind == "weekend-1-blocklow" || kind == "weekend-1-dodgelow" || kind == "weekend-1-hitlow" {
        this.alternate = play_punch_low(this.alternate, target);
    }
    else if kind == "weekend-1-picouppercutprep" {
        play_hit_high(target);
        this.cant_uppercut = true;
    }
    else if kind == "weekend-1-picouppercut" { play_dodge(target); }
    else if kind == "weekend-1-darnelluppercutprep" { play_uppercut_prep(target); }
    else if kind == "weekend-1-darnelluppercut" { play_uppercut(target); }
    else if kind == "weekend-1-idle" { play_idle(target); }
    else if kind == "weekend-1-fakeout" { play_cringe(target); }
    else if kind == "weekend-1-taunt" { play_pissed_cond(target); }
    else if kind == "weekend-1-tauntforce" { play_pissed(target); }
    else if kind == "weekend-1-reversefakeout" { play_fakeout(target); }

    this.cant_uppercut = false;
}

fn onNoteGhostMiss(ev) {
    let target = get_target();
    if target == "" { return; }
    let hc = ev.get("health_change");
    if hc != () && (hp() + hc) <= 0.0 {
        this.alternate = play_punch_low(this.alternate, target);
    } else if rand_int(0, 1) == 0 {
        play_dodge(target);
    } else {
        play_block(target);
    }
}
