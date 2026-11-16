// Blazin' (Weekend 1) note kind registrations (ported from blazin.hxc to oxide-engine Rhai)

fn register_blazin_notekinds() {
    let defs = [
        ["weekend-1-punchhigh", "Punch High (Blazin')"],
        ["weekend-1-punchhighdodged", "Punch High (Dodge) (Blazin')"],
        ["weekend-1-punchhighblocked", "Punch High (Block) (Blazin')"],
        ["weekend-1-punchhighspin", "Punch High (Spin) (Blazin')"],
        ["weekend-1-punchlow", "Punch Low (Blazin')"],
        ["weekend-1-punchlowdodged", "Punch Low (Dodge) (Blazin')"],
        ["weekend-1-punchlowblocked", "Punch Low (Block) (Blazin')"],
        ["weekend-1-punchlowspin", "Punch Low (Spin) (Blazin')"],
        ["weekend-1-picouppercutprep", "Pico Uppercut (Prep) (Blazin')"],
        ["weekend-1-picouppercut", "Pico Uppercut (Blazin')"],
        ["weekend-1-blockhigh", "Block High (Blazin')"],
        ["weekend-1-blocklow", "Block Low (Blazin')"],
        ["weekend-1-blockspin", "Block High (Spin) (Blazin')"],
        ["weekend-1-dodgehigh", "Dodge High (Blazin')"],
        ["weekend-1-dodgelow", "Dodge Low (Blazin')"],
        ["weekend-1-dodgespin", "Dodge High (Spin) (Blazin')"],
        ["weekend-1-hithigh", "Hit High (Blazin')"],
        ["weekend-1-hitlow", "Hit Low (Blazin')"],
        ["weekend-1-hitspin", "Hit High (Spin) (Blazin')"],
        ["weekend-1-darnelluppercutprep", "Darnell Uppercut (Prep) (Blazin')"],
        ["weekend-1-darnelluppercut", "Darnell Uppercut (Blazin')"],
        ["weekend-1-idle", "Idle (Blazin')"],
        ["weekend-1-fakeout", "Fakeout (Blazin')"],
        ["weekend-1-taunt", "Taunt (If Fakeout) (Blazin')"],
        ["weekend-1-tauntforce", "Taunt (Forced) (Blazin')"],
        ["weekend-1-reversefakeout", "Fakeout (Reverse) (Blazin')"]
    ];
    for entry in defs {
        register_notekind(entry[0], #{ description: entry[1], no_anim: true });
    }
}

fn onCreate() {
    register_blazin_notekinds();
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

fn onNoteMiss(ev) {
    let kind = ev.get("kind");
    if kind != () && kind.starts_with("weekend-1-") {
        ev.set("play_anim", false);
    }
}
