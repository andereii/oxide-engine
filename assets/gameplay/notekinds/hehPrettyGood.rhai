// Tankman "Heh Pretty Good" note kind script (ported from hehPrettyGood.hxc to oxide-engine Rhai)

fn onCreate() {
    register_notekind("hehPrettyGood", #{
        description: "Tankman Heh Pretty Good (Week 7)",
        no_anim: true
    });
}

fn onOpponentNoteHit(ev) {
    if ev.get("kind") == "hehPrettyGood" {
        ev.set("play_anim", false);
    }
}

fn onNoteHit(ev) {
    if ev.get("kind") == "hehPrettyGood" {
        ev.set("play_anim", false);
    }
}
