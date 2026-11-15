// Sserafim Eunchae character script (ported from sserafim-eunchae.hxc)

fn lip_sync_offsets() {
    #{
        idle: #{ offset: [43, 6], angle: -168 },
        singUP: #{ offset: [45, 10], angle: -166 },
        singRIGHT: #{ offset: [42, 5], angle: -166 },
        singDOWN: #{ offset: [41, 3], angle: -168 },
        singLEFT: #{ offset: [43, 6], angle: -169 }
    }
}

fn onCreate() {
    this.offsets = lip_sync_offsets();
    global_set("lip_sync_sserafim_eunchae", this.offsets["idle"]);
}

fn onUpdate() {
    let target = if char_id("dad") == "sserafim-eunchae" { "dad" } else if char_id("bf") == "sserafim-eunchae" { "bf" } else { "" };
    if target == "" { return; }
    let cur = char_anim(target);
    let entry = this.offsets[cur];
    if entry != () {
        global_set("lip_sync_sserafim_eunchae", entry);
    }
}
