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
        self.bind_keys(Action::Accept, vec![KeyCode::Space, KeyCode::Enter]);
        self.bind_keys(Action::Back, vec![KeyCode::Backspace, KeyCode::Escape]);
        self.bind_keys(Action::Pause, vec![KeyCode::Enter, KeyCode::Escape]);
        self.bind_keys(Action::Reset, vec![KeyCode::KeyR]);
        self.bind_keys(Action::CutsceneAdvance, vec![KeyCode::KeyZ, KeyCode::Space, KeyCode::Enter]);
        self.bind_keys(Action::VolumeUp, vec![KeyCode::Equal, KeyCode::NumpadAdd]);
        self.bind_keys(Action::VolumeDown, vec![KeyCode::Minus, KeyCode::NumpadSubtract]);
        self.bind_keys(Action::VolumeMute, vec![KeyCode::Digit0, KeyCode::Numpad0]);
        self.bind_keys(Action::WindowFullscreen, vec![KeyCode::F11]);
        self.bind_keys(Action::WindowScreenshot, vec![KeyCode::F3]);
        self.bind_keys(Action::DebugDisplay, vec![KeyCode::F6]);

        match scheme {
            KeyboardScheme::Solo => {
                self.bind_keys(Action::NoteLeft, vec![KeyCode::KeyA, KeyCode::ArrowLeft, KeyCode::KeyD]);
                self.bind_keys(Action::NoteDown, vec![KeyCode::KeyS, KeyCode::ArrowDown, KeyCode::KeyF]);
                self.bind_keys(Action::NoteUp, vec![KeyCode::KeyW, KeyCode::ArrowUp, KeyCode::KeyJ]);
                self.bind_keys(Action::NoteRight, vec![KeyCode::ArrowRight, KeyCode::KeyK]);
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
}

