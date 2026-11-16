// Tankman "Ugh" note kind script (ported from ugh.hxc to oxide-engine Rhai)

fn onCreate() {
    register_notekind("ugh", #{
        description: "Tankman Ugh (Week 7)",
        no_anim: true
    });
}

fn onOpponentNoteHit(ev) {
    if ev.get("kind") == "ugh" {
        ev.set("play_anim", false);
    }
}

fn onNoteHit(ev) {
    if ev.get("kind") == "ugh" {
        ev.set("play_anim", false);
    }
}
