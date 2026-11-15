// Pico doppelganger cutscene timing helpers (ported from PicoDopplegangerSprite.hxc)

fn doppelganger_asset() {
    "gameplay/stages/phillyTrainErect/pico-doppelganger"
}

fn doppelganger_sound_events(shoot, explode) {
    let events = [#{ delay: 0.3, sound: "gameplay/stages/phillyTrainErect/cutscene/pico-gasp" }];
    if shoot {
        events.push(#{ delay: 6.29, sound: "gameplay/stages/phillyTrainErect/cutscene/pico-shoot" });
        events.push(#{ delay: 10.33, sound: "gameplay/stages/phillyTrainErect/cutscene/pico-spin" });
    } else if explode {
        events.push(#{ delay: 3.7, sound: "gameplay/stages/phillyTrainErect/cutscene/pico-cigarette-2" });
        events.push(#{ delay: 8.75, sound: "gameplay/stages/phillyTrainErect/cutscene/pico-explode" });
    } else {
        events.push(#{ delay: 3.7, sound: "gameplay/stages/phillyTrainErect/cutscene/pico-cigarette-1" });
    }
    events
}
