// School stage script (ported from school.hxc to oxide-engine Rhai)

fn onCreate() {
    this.freaks_left = false;
    if song() == "roses" {
        anim("freaks", "danceLeft-scared");
    }
}

fn onBeatHit() {
    if song() == "roses" {
        this.freaks_left = !this.freaks_left;
        if this.freaks_left {
            anim("freaks", "danceLeft-scared");
        } else {
            anim("freaks", "danceRight-scared");
        }
    }
}

fn onSongRetry() {
    this.freaks_left = false;
    if song() == "roses" {
        anim("freaks", "danceLeft-scared");
    }
}
