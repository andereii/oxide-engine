// Stress (Pico mix) song script (ported from stress-pico.hxc to oxide-engine Rhai)

fn reset_stress_pico_actors() {
    if char_exists("dad") && char_anim("dad") == "redheadsAnim" {
        dance("dad");
    }
    if char_exists("bf") && char_anim("bf") == "knifeToss" {
        dance("bf");
    }
}

fn onCreate() {
}

fn onSongRetry() {
    reset_stress_pico_actors();
}
