// Censor note kind script (ported from censor.hxc to oxide-engine Rhai)

fn onCreate() {
    register_notekind("censor", #{
        description: "[UH-OH!] Censor Bar"
    });
}
