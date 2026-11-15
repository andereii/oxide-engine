// Sserafim Kazuha character script (ported from sserafim-kazuha.hxc)

fn lip_sync_offsets() {
    #{
        idle: #{ offset: [5, 4], angle: -13 },
        singUP: #{ offset: [7, 2], angle: -14 },
        singRIGHT: #{ offset: [7, 2], angle: -13 },
        singDOWN: #{ offset: [4, 6], angle: -12 },
        singLEFT: #{ offset: [5, 4], angle: -14 }
    }
}

fn onCreate() {
    this.offsets = lip_sync_offsets();
    global_set("lip_sync_sserafim_kazuha", this.offsets["idle"]);
}

fn onUpdate() {
    let target = if char_id("dad") == "sserafim-kazuha" { "dad" } else if char_id("bf") == "sserafim-kazuha" { "bf" } else { "" };
    if target == "" { return; }
    let cur = char_anim(target);
    let entry = this.offsets[cur];
    if entry != () {
        global_set("lip_sync_sserafim_kazuha", entry);
    }
}
