// Pico blood pool helper (ported from PicoBloodPool.hxc)

fn new_blood_pool_state(x, y) {
    #{
        asset: "gameplay/stages/phillyTrainErect/blood-pool",
        x: x,
        y: y,
        scale_x: 1.0,
        scale_y: 1.0,
        extending: false
    }
}

fn update_blood_pool(state, dt_s) {
    if state.extending {
        let factor = 0.02 * dt_s;
        state.scale_x += factor;
        state.scale_y += factor;
    }
    state
}
