// Boyfriend Holding GF character script (ported from bf-holding-gf.hxc)

fn pick_jeff_death_quote() {
    let blocked = [1, 3, 8, 13, 17, 21];
    let choice = rand_int(1, 24);
    let tries = 0;
    while (choice in blocked) && tries < 16 {
        choice = rand_int(1, 24);
        tries += 1;
    }
    "gameplay/characters/bf-holding-gf/sounds/jeff-gameover-" + choice
}

fn onCreate() {
    global_set("death_quote_pool", "bf-holding-gf");
}

fn onGameOver() {
    global_set("death_quote_sfx", pick_jeff_death_quote());
}
