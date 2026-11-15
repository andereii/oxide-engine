// Sserafim Chaewon character script (ported from sserafim-chaewon.hxc)

fn lip_sync_offsets() {
    #{
        idle: #{ offset: [41, 3], angle: -166 },
        singUP: #{ offset: [38, 0], angle: -168 },
        singRIGHT: #{ offset: [39, 1], angle: -165 },
        singDOWN: #{ offset: [41, 3], angle: -167 },
        singLEFT: #{ offset: [40, 2], angle: -165 }
    }
}

fn onCreate() {
    this.offsets = lip_sync_offsets();
    global_set("lip_sync_sserafim_chaewon", this.offsets["idle"]);
}

fn onUpdate() {
    let target = if char_id("dad") == "sserafim-chaewon" { "dad" } else if char_id("bf") == "sserafim-chaewon" { "bf" } else { "" };
    if target == "" { return; }
    let cur = char_anim(target);
    let entry = this.offsets[cur];
    if entry != () {
        global_set("lip_sync_sserafim_chaewon", entry);
    }
}
