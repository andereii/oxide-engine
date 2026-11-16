// 2hot (Weekend 1) note kind registrations (ported from 2hot.hxc to oxide-engine Rhai)

fn register_2hot_notekinds() {
    register_notekind("weekend-1-lightcan", #{ description: "Darnell Light Can (2hot)", no_anim: true });
    register_notekind("weekend-1-kneecan", #{ description: "Darnell Knee Can (2hot)", no_anim: true });
    register_notekind("weekend-1-kickcan", #{ description: "Darnell Kick Can (2hot)", no_anim: true });
    register_notekind("weekend-1-cockgun", #{ description: "Pico Cock Gun (2hot)", no_anim: true });
    register_notekind("weekend-1-firegun", #{ description: "Pico Fire Gun (2hot)", no_anim: true });
}

fn onCreate() {
    register_2hot_notekinds();
}

fn onNoteHit(ev) {
    let kind = ev.get("kind");
    if kind != () && kind.starts_with("weekend-1-") {
        ev.set("play_anim", false);
    }
}

fn onOpponentNoteHit(ev) {
    let kind = ev.get("kind");
    if kind != () && kind.starts_with("weekend-1-") {
        ev.set("play_anim", false);
    }
}
