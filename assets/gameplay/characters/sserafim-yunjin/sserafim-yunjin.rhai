// Sserafim Yunjin character script (ported from sserafim-yunjin.hxc)

fn lip_sync_offsets() {
    #{
        idle: #{ offset: [8, 6], angle: 23 },
        singUP: #{ offset: [6, 8], angle: 22 },
        singRIGHT: #{ offset: [6, 8], angle: 23 },
        singDOWN: #{ offset: [8, 6], angle: 23 },
        singLEFT: #{ offset: [6, 8], angle: 23 }
    }
}

fn onCreate() {
    this.offsets = lip_sync_offsets();
    global_set("lip_sync_sserafim_yunjin", this.offsets["idle"]);
}

fn onUpdate() {
    let target = if char_id("dad") == "sserafim-yunjin" { "dad" } else if char_id("bf") == "sserafim-yunjin" { "bf" } else { "" };
    if target == "" { return; }
    let cur = char_anim(target);
    let entry = this.offsets[cur];
    if entry != () {
        global_set("lip_sync_sserafim_yunjin", entry);
    }
}
