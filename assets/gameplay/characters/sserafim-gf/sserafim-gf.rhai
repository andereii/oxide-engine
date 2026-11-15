// Sserafim Girlfriend character script (ported from sserafim-gf.hxc)

fn onCreate() {
    this.is_beautiful = false;
    this.last_anim = "";
}

fn onSongRetry() {
    this.is_beautiful = false;
    this.last_anim = "";
}

fn onSongEvent(ev) {
    if ev.get("kind") == "sserafimBeautiful" {
        let val = ev.get("value");
        if val != () {
            let b = val["beautiful"];
            if b != () {
                this.is_beautiful = b == true;
            }
        }
    }
}

fn onUpdate() {
    let target = if char_id("gf") == "sserafim-gf" { "gf" } else if char_id("dad") == "sserafim-gf" { "dad" } else { "" };
    if target == "" { return; }
    let cur = char_anim(target);
    if cur == this.last_anim { return; }
    this.last_anim = cur;
    if this.is_beautiful && cur != "" && !cur.ends_with("-beautiful") {
        play_anim(target, cur + "-beautiful");
        this.last_anim = cur + "-beautiful";
    }
}
