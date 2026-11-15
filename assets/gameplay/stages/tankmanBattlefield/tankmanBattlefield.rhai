// Tankman Battlefield stage script (ported from tankmanBattlefield.hxc to oxide-engine Rhai)

fn onCreate() {
    this.tank_speed = rand_range(5.0, 7.0);
    this.tank_x = 400.0;
    this.clouds_x = -1100.0;
    spawn_sprite("tankClouds", "gameplay/stages/tankmanBattlefield/tank-clouds.png", -1100.0, 20.0, 0.12);
}

fn onUpdate() {
    let elapsed = dt();
    this.clouds_x += elapsed * 8.0;
    set_prop("tankClouds", "x", this.clouds_x);

    let elapsed_s = time() / 1000.0;
    let tank_angle = elapsed_s * this.tank_speed;
    let rad = (tank_angle + 180.0) * pi() / 180.0;

    prop_angle("tankRolling", tank_angle - 90.0 + 15.0);
    prop_pos(
        "tankRolling",
        this.tank_x + cos(rad) * 1500.0,
        1300.0 + sin(rad) * 1100.0
    );
}
