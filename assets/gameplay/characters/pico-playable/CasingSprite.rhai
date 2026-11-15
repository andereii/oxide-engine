// Bullet casing helper (ported from CasingSprite.hxc)

fn new_casing_state() {
    #{
        casings: [],
        pending_rolls: 0
    }
}

fn casing_roll_params() {
    let factor_a = rand_range(3.0, 10.0);
    let factor_b = rand_range(1.0, 2.0);
    let vel_x = 20.0 * factor_b;
    let drag_x = factor_a * factor_b;
    #{
        asset: "gameplay/characters/pico-playable/pico-casing",
        z_index: 1900.0,
        angle: 125.1,
        velocity_x: vel_x,
        drag_x: drag_x,
        angular_velocity: 100.0,
        angular_drag: (drag_x / vel_x) * 100.0
    }
}

fn clear_casings() {
    new_casing_state()
}
