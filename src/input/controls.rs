use std::collections::HashMap;

use bevy::prelude::{ButtonInput, KeyCode, Resource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    NoteUp,
    NoteLeft,
    NoteRight,
    NoteDown,
    UiUp,
    UiLeft,
    UiRight,
    UiDown,
    Accept,
    Back,
    Pause,
    Reset,
    WindowFullscreen,
    WindowScreenshot,
    CutsceneAdvance,
    FreeplayFavorite,
    FreeplayLeft,
    FreeplayRight,
    FreeplayCharSelect,
    FreeplayJumpToTop,
    FreeplayJumpToBottom,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    DebugMenu,
    DebugChart,
    DebugCamera,
    DebugStage,
    DebugDisplay,
    ReloadMods,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Control {
    UiUp,
    UiLeft,
    UiRight,
    UiDown,
    NoteUp,
    NoteLeft,
    NoteRight,
    NoteDown,
    Accept,
    Back,
    Pause,
    Reset,
    WindowFullscreen,
    WindowScreenshot,
    CutsceneAdvance,
    FreeplayFavorite,
    FreeplayLeft,
    FreeplayRight,
    FreeplayCharSelect,
    FreeplayJumpToTop,
    FreeplayJumpToBottom,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    DebugMenu,
    DebugChart,
    DebugCamera,
    DebugStage,
    DebugDisplay,
    ReloadMods,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Keys,
    Gamepad(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardScheme {
    Solo,
    Duo(bool),
    None,
    Custom,
}

#[derive(Resource, Debug, Clone)]
pub struct Controls {
    pub bindings: HashMap<Action, Vec<KeyCode>>,
    pub scheme: KeyboardScheme,
    pub last_device_used: Device,
}

impl Default for Controls {
    fn default() -> Self {
        let mut controls = Self {
            bindings: HashMap::new(),
            scheme: KeyboardScheme::Solo,
            last_device_used: Device::Keys,
        };
        controls.set_keyboard_scheme(KeyboardScheme::Solo);
        controls
    }
}

impl Controls {
    pub fn set_keyboard_scheme(&mut self, scheme: KeyboardScheme) {
        self.scheme = scheme;
        self.bindings.clear();
        self.bind_keys(Action::UiUp, vec![KeyCode::KeyW, KeyCode::ArrowUp]);
        self.bind_keys(Action::UiLeft, vec![KeyCode::KeyA, KeyCode::ArrowLeft]);
        self.bind_keys(Action::UiRight, vec![KeyCode::KeyD, KeyCode::ArrowRight]);
        self.bind_keys(Action::UiDown, vec![KeyCode::KeyS, KeyCode::ArrowDown]);
        self.bind_keys(Action::Accept, vec![KeyCode::KeyZ, KeyCode::Space, KeyCode::Enter]);
        self.bind_keys(Action::Back, vec![KeyCode::KeyX, KeyCode::Backspace, KeyCode::Escape]);
        self.bind_keys(Action::Pause, vec![KeyCode::KeyP, KeyCode::Enter, KeyCode::Escape]);
        self.bind_keys(Action::Reset, vec![KeyCode::KeyR]);
        self.bind_keys(Action::CutsceneAdvance, vec![KeyCode::KeyZ, KeyCode::Space, KeyCode::Enter]);
        self.bind_keys(Action::VolumeUp, vec![KeyCode::Equal, KeyCode::NumpadAdd]);
        self.bind_keys(Action::VolumeDown, vec![KeyCode::Minus, KeyCode::NumpadSubtract]);
        self.bind_keys(Action::VolumeMute, vec![KeyCode::Digit0, KeyCode::Numpad0]);
        self.bind_keys(Action::WindowFullscreen, vec![KeyCode::F11]);
        self.bind_keys(Action::WindowScreenshot, vec![KeyCode::F3]);
        self.bind_keys(Action::ReloadMods, vec![KeyCode::F5]);
        self.bind_keys(Action::DebugDisplay, vec![KeyCode::F6]);

        match scheme {
            KeyboardScheme::Solo => {
                self.bind_keys(Action::NoteLeft, vec![KeyCode::KeyA, KeyCode::ArrowLeft]);
                self.bind_keys(Action::NoteDown, vec![KeyCode::KeyS, KeyCode::ArrowDown]);
                self.bind_keys(Action::NoteUp, vec![KeyCode::KeyW, KeyCode::ArrowUp]);
                self.bind_keys(Action::NoteRight, vec![KeyCode::KeyD, KeyCode::ArrowRight]);
            }
            KeyboardScheme::Duo(true) => {
                self.bind_keys(Action::NoteLeft, vec![KeyCode::KeyA]);
                self.bind_keys(Action::NoteDown, vec![KeyCode::KeyS]);
                self.bind_keys(Action::NoteUp, vec![KeyCode::KeyW]);
                self.bind_keys(Action::NoteRight, vec![KeyCode::KeyD]);
            }
            KeyboardScheme::Duo(false) => {
                self.bind_keys(Action::NoteLeft, vec![KeyCode::ArrowLeft]);
                self.bind_keys(Action::NoteDown, vec![KeyCode::ArrowDown]);
                self.bind_keys(Action::NoteUp, vec![KeyCode::ArrowUp]);
                self.bind_keys(Action::NoteRight, vec![KeyCode::ArrowRight]);
            }
            KeyboardScheme::None | KeyboardScheme::Custom => {}
        }
    }

    pub fn bind_keys(&mut self, action: Action, keys: Vec<KeyCode>) {
        self.bindings.insert(action, keys);
    }

    pub fn keys_for(&self, action: Action) -> &[KeyCode] {
        self.bindings.get(&action).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn pressed(&self, input: &ButtonInput<KeyCode>, action: Action) -> bool {
        self.keys_for(action).iter().any(|key| input.pressed(*key))
    }

    pub fn just_pressed(&self, input: &ButtonInput<KeyCode>, action: Action) -> bool {
        self.keys_for(action).iter().any(|key| input.just_pressed(*key))
    }

    pub fn just_released(&self, input: &ButtonInput<KeyCode>, action: Action) -> bool {
        self.keys_for(action).iter().any(|key| input.just_released(*key))
    }

    pub fn is_note_key(&self, key: KeyCode) -> bool {
        [
            Action::NoteLeft,
            Action::NoteDown,
            Action::NoteUp,
            Action::NoteRight,
        ]
        .iter()
        .any(|action| self.keys_for(*action).contains(&key))
    }

    pub fn collect_active_names(
        &self,
        input: &ButtonInput<KeyCode>,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        const ACTION_NAMES: &[(Action, &[&str])] = &[
            (Action::NoteLeft, &["NOTE_LEFT"]),
            (Action::NoteDown, &["NOTE_DOWN"]),
            (Action::NoteUp, &["NOTE_UP"]),
            (Action::NoteRight, &["NOTE_RIGHT"]),
            (Action::UiLeft, &["UI_LEFT", "LEFT"]),
            (Action::UiDown, &["UI_DOWN", "DOWN"]),
            (Action::UiUp, &["UI_UP", "UP"]),
            (Action::UiRight, &["UI_RIGHT", "RIGHT"]),
            (Action::Accept, &["ACCEPT"]),
            (Action::Back, &["BACK"]),
            (Action::Pause, &["PAUSE"]),
            (Action::Reset, &["RESET"]),
            (Action::CutsceneAdvance, &["CUTSCENE_ADVANCE"]),
            (Action::VolumeUp, &["VOLUME_UP"]),
            (Action::VolumeDown, &["VOLUME_DOWN"]),
            (Action::VolumeMute, &["VOLUME_MUTE"]),
            (Action::WindowFullscreen, &["FULLSCREEN"]),
            (Action::WindowScreenshot, &["SCREENSHOT"]),
            (Action::ReloadMods, &["RELOAD_MODS"]),
            (Action::DebugDisplay, &["DEBUG_DISPLAY"]),
        ];
        let mut down = Vec::new();
        let mut hit = Vec::new();
        let mut up = Vec::new();
        for (action, names) in ACTION_NAMES {
            if self.pressed(input, *action) {
                down.extend(names.iter().map(|s| (*s).to_string()));
            }
            if self.just_pressed(input, *action) {
                hit.extend(names.iter().map(|s| (*s).to_string()));
            }
            if self.just_released(input, *action) {
                up.extend(names.iter().map(|s| (*s).to_string()));
            }
        }
        (down, hit, up)
    }
}

pub fn primary_keycode_name(key: KeyCode) -> String {
    let raw = format!("{key:?}");
    if let Some(rest) = raw.strip_prefix("Key") {
        return rest.to_string();
    }
    if let Some(rest) = raw.strip_prefix("Digit") {
        return rest.to_string();
    }
    raw
}

pub fn keycode_aliases(key: KeyCode) -> Vec<String> {
    let raw = format!("{key:?}");
    let mut out = vec![raw.clone()];
    if let Some(rest) = raw.strip_prefix("Key") {
        out.push(rest.to_string());
    } else if let Some(rest) = raw.strip_prefix("Digit") {
        out.push(rest.to_string());
    } else if let Some(rest) = raw.strip_prefix("Arrow") {
        out.push(rest.to_string());
    } else if raw.starts_with("Shift") {
        out.push("Shift".to_string());
    } else if raw.starts_with("Control") {
        out.push("Ctrl".to_string());
    } else if raw.starts_with("Alt") {
        out.push("Alt".to_string());
    }
    out
}

