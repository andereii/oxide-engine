// Sserafim Sakura character script (ported from sserafim-sakura.hxc)

fn lip_sync_offsets() {
    #{
        idle: #{ offset: [7, 2], angle: -14 },
        singUP: #{ offset: [8, 1], angle: -15 },
        singRIGHT: #{ offset: [7, 2], angle: -15 },
        singDOWN: #{ offset: [6, 3], angle: -15 },
        singLEFT: #{ offset: [7, 2], angle: -14 },
        "singUP-joint": #{ offset: [10, -1], angle: -14 },
        "singRIGHT-joint": #{ offset: [6, 3], angle: -15 },
        "singDOWN-joint": #{ offset: [5, 5], angle: -15 },
        "singLEFT-joint": #{ offset: [7, 2], angle: -16 }
    }
}

fn find_slot() {
    if char_id("bf") == "sserafim-sakura" { return "bf"; }
    if char_id("dad") == "sserafim-sakura" { return "dad"; }
    ""
}

fn onCreate() {
    this.offsets = lip_sync_offsets();
    this.shader_cleared = false;
    global_set("lip_sync_sserafim_sakura", this.offsets["idle"]);
}

fn onSongRetry() {
    this.shader_cleared = false;
}

fn onGameOver() {
    if char_id("bf") == "sserafim-sakura" {
        char_visible("bf", true);
        clear_shader("bf");
        this.shader_cleared = true;
    }
}

fn onUpdate() {
    let target = find_slot();
    if target == "" { return; }
    let cur = char_anim(target);
    let entry = this.offsets[cur];
    if entry != () {
        global_set("lip_sync_sserafim_sakura", entry);
    }
    if target == "bf" && !this.shader_cleared && cur == "firstDeath" {
        char_visible("bf", true);
        clear_shader("bf");
        this.shader_cleared = true;
    }
}

fn onNoteHit(ev) {
    if char_id("bf") == "sserafim-sakura" && ev.get("kind") == "sakura-joint" {
        let dir = ev.get("dir");
        if dir != () {
            play_sing("bf", dir, false, "joint");
        }
    }
}

fn onOpponentNoteHit(ev) {
    if char_id("dad") == "sserafim-sakura" && ev.get("kind") == "sakura-joint" {
        let dir = ev.get("dir");
        if dir != () {
            play_sing("dad", dir, false, "joint");
        }
    }
}
