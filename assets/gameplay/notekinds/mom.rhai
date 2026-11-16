// Mom note kind script (ported from mom.hxc to oxide-engine Rhai)

fn onCreate() {
    register_notekind("mom", #{
        description: "Mom Sings (Week 5)",
        suffix: "-alt"
    });
}
