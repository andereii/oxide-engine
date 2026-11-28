use std::path::PathBuf;

use bevy::prelude::KeyCode;
use oxin_app::RenderBackend;

use crate::utils::constants::{DEFAULT_DIFFICULTY, DEFAULT_VARIATION};

#[derive(Debug, Clone)]
pub struct LaunchFlags {
    pub song_id: Option<String>,
    pub difficulty: Option<String>,
    pub variation: Option<String>,
    pub bot_play_mode: bool,
    pub practice_mode: bool,
    pub minimal_mode: bool,
    pub downscroll: Option<bool>,
    pub playback_rate: f64,
    pub start_timestamp_ms: f64,
    pub vsync: Option<bool>,
    pub no_shaders: bool,
    pub loop_song: bool,
    pub render_backend: RenderBackend,
    pub keybinds: Option<[KeyCode; 4]>,
    pub capture_timestamps_ms: Vec<f64>,
    pub capture_out_dir: Option<PathBuf>,
    pub test_crash: bool,
    pub show_help: bool,
}

impl Default for LaunchFlags {
    fn default() -> Self {
        Self {
            song_id: None,
            difficulty: None,
            variation: None,
            bot_play_mode: false,
            practice_mode: false,
            minimal_mode: false,
            downscroll: None,
            playback_rate: 1.0,
            start_timestamp_ms: 0.0,
            vsync: None,
            no_shaders: false,
            loop_song: false,
            render_backend: RenderBackend::Auto,
            keybinds: None,
            capture_timestamps_ms: Vec::new(),
            capture_out_dir: None,
            test_crash: false,
            show_help: false,
        }
    }
}

impl LaunchFlags {
    pub fn has_play_flags(&self) -> bool {
        self.song_id.is_some() || !self.capture_timestamps_ms.is_empty()
    }

    pub fn from_env_args() -> Self {
        Self::parse(std::env::args().skip(1))
    }

    pub fn parse<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = Self::default();
        let mut iter = args.into_iter();
        while let Some(raw_arg) = iter.next() {
            let arg = raw_arg.as_ref().trim();
            if arg.is_empty() {
                continue;
            }
            if arg == "-h" || arg == "--help" {
                out.show_help = true;
                continue;
            }
            let (key, inline_val) = split_flag(arg);
            match key {
                "--song" => {
                    if let Some(v) = take_value(inline_val, &mut iter) {
                        out.song_id = Some(v);
                    }
                }
                "--difficulty" | "--diff" => {
                    if let Some(v) = take_value(inline_val, &mut iter) {
                        out.difficulty = Some(v);
                    }
                }
                "--variation" => {
                    if let Some(v) = take_value(inline_val, &mut iter) {
                        out.variation = Some(v);
                    }
                }
                "--botplay" | "--bot-play" | "--wb" => {
                    out.bot_play_mode = parse_bool_flag(inline_val, true);
                }
                "--practice" => {
                    out.practice_mode = parse_bool_flag(inline_val, true);
                }
                "--minimal" => {
                    out.minimal_mode = parse_bool_flag(inline_val, true);
                }
                "--downscroll" => {
                    out.downscroll = Some(parse_bool_flag(inline_val, true));
                }
                "--upscroll" => {
                    out.downscroll = Some(false);
                }
                "--rate" | "--playback-rate" => {
                    if let Some(v) = take_value(inline_val, &mut iter)
                        && let Ok(rate) = v.parse::<f64>()
                        && rate.is_finite()
                        && rate > 0.0
                    {
                        out.playback_rate = rate.clamp(0.1, 10.0);
                    }
                }
                "--start-ms" => {
                    if let Some(v) = take_value(inline_val, &mut iter)
                        && let Ok(ms) = v.parse::<f64>()
                        && ms.is_finite()
                    {
                        out.start_timestamp_ms = ms.max(0.0);
                    }
                }
                "--vsync" => out.vsync = Some(parse_bool_flag(inline_val, true)),
                "--no-vsync" => out.vsync = Some(false),
                "--no-shader" | "--no-shaders" | "--noshader" | "--noshaders" | "--disable-shader" | "--disable-shaders" => {
                    out.no_shaders = parse_bool_flag(inline_val, true);
                }
                "--shader" | "--shaders" => {
                    out.no_shaders = !parse_bool_flag(inline_val, true);
                }
                "--loop" | "--loop-song" => {
                    out.loop_song = parse_bool_flag(inline_val, true);
                }
                "--no-loop" => {
                    out.loop_song = false;
                }
                "--renderer" | "--render" | "--backend" => {
                    if let Some(v) = take_value(inline_val, &mut iter) {
                        out.render_backend = parse_render_backend(&v);
                    }
                }
                "--vulkan" => out.render_backend = RenderBackend::Vulkan,
                "--dx12" | "--d3d12" | "--directx12" => out.render_backend = RenderBackend::Dx12,
                "--dx11" | "--d3d11" | "--directx11" | "--gl" | "--opengl" => {
                    out.render_backend = RenderBackend::Gl;
                }
                "--metal" => out.render_backend = RenderBackend::Metal,
                "--kb" | "--keybinds" => {
                    if let Some(v) = take_value(inline_val, &mut iter) {
                        match parse_keybinds(&v) {
                            Ok(keys) => out.keybinds = Some(keys),
                            Err(err) => eprintln!("Warning: ignoring invalid --kb '{v}': {err}"),
                        }
                    }
                }
                "--capture-ms" => {
                    if let Some(v) = take_value(inline_val, &mut iter) {
                        parse_capture_ms_list(&v, &mut out.capture_timestamps_ms);
                    }
                }
                "--capture-out" => {
                    if let Some(v) = take_value(inline_val, &mut iter) {
                        out.capture_out_dir = Some(PathBuf::from(v));
                    }
                }
                "--test-crash" => {
                    out.test_crash = true;
                }
                _ => {}
            }
        }
        out
    }

    pub fn print_help() {
        println!(
            "Oxide Engine CLI flags:\n\
             \x20 --song:<id>                 Select song id (default: blammed)\n\
             \x20 --difficulty:<diff>         Select difficulty (default: {DEFAULT_DIFFICULTY})\n\
             \x20 --diff:<diff>               Alias for --difficulty\n\
             \x20 --variation:<var>           Select variation (default: {DEFAULT_VARIATION})\n\
             \x20 --botplay / --bot-play      Enable botplay mode (default: false)\n\
             \x20 --practice                  Enable practice mode (default: false)\n\
             \x20 --minimal                   Enable minimal HUD-only mode (default: false)\n\
             \x20 --upscroll                  Force upscroll note direction (default)\n\
             \x20 --downscroll                Force downscroll note direction\n\
             \x20 --rate:<f64>                Playback rate multiplier (default: 1.0)\n\
             \x20 --start-ms:<f64>            Start timestamp in ms (default: 0.0)\n\
             \x20 --vsync / --no-vsync        Override OXIDE_VSYNC\n\
             \x20 --no-shader / --no-shaders  Disable stage/character shaders (--shaders to enable)\n\
             \x20 --loop                      Loop the current song when it ends\n\
             \x20 --renderer:<api>            Select GPU backend: auto, vulkan, dx12, dx11/gl, metal\n\
             \x20 --vulkan / --dx12 / --gl    Shorthand GPU backend flags\n\
             \x20 --kb:<keys>                 Temporary note keybinds in left,down,up,right order (e.g. askl or a,s,up,right)\n\
             \x20 --capture-ms:<ms,...>       Capture screenshots at sorted ms and exit\n\
             \x20 --capture-out:<dir>         Output directory for --capture-ms (default: captures)\n\
             \x20 --test-crash                Trigger a simulated fatal crash dialog (works in debug builds)\n\
             \x20 -h, --help                  Show this help message"
        );
    }
}

pub(crate) fn normalize_capture_ms(values: &mut Vec<f64>) {
    values.retain(|ms| ms.is_finite() && *ms >= 0.0);
    values.sort_by(f64::total_cmp);
    values.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
}

fn parse_capture_ms_list(raw: &str, out: &mut Vec<f64>) {
    for part in raw.split(',') {
        if let Ok(ms) = part.trim().parse::<f64>() {
            out.push(ms);
        }
    }
    normalize_capture_ms(out);
}

fn split_flag(arg: &str) -> (&str, Option<&str>) {
    if let Some((k, v)) = arg.split_once(':') {
        (k, Some(v))
    } else if let Some((k, v)) = arg.split_once('=') {
        (k, Some(v))
    } else {
        (arg, None)
    }
}

fn take_value<I, S>(inline: Option<&str>, iter: &mut I) -> Option<String>
where
    I: Iterator<Item = S>,
    S: AsRef<str>,
{
    if let Some(v) = inline {
        let trimmed = v.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    iter.next()
        .map(|s| s.as_ref().trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parse_bool_flag(inline: Option<&str>, default_on: bool) -> bool {
    match inline.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("false" | "0" | "no" | "off") => false,
        Some("true" | "1" | "yes" | "on") => true,
        _ => default_on,
    }
}

fn parse_render_backend(raw: &str) -> RenderBackend {
    match raw.trim().to_ascii_lowercase().as_str() {
        "vulkan" | "vk" => RenderBackend::Vulkan,
        "dx12" | "d3d12" | "directx12" | "directx-12" => RenderBackend::Dx12,
        "dx11" | "d3d11" | "directx11" | "directx-11" | "gl" | "opengl" | "gles" => {
            RenderBackend::Gl
        }
        "metal" => RenderBackend::Metal,
        _ => RenderBackend::Auto,
    }
}

/// Parses 4 note keybindings in `[left, down, up, right]` order.
/// Supports compact 4-character strings (e.g. `askl`, `dfjk`, `1234`) and
/// comma-separated key names (e.g. `a,s,k,l`, `left,down,up,right`, `lshift,z,slash,rshift`).
pub(crate) fn parse_keybinds(raw: &str) -> Result<[KeyCode; 4], String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("expected 4 keys".to_string());
    }

    let comma_parts: Vec<&str> = trimmed.split(',').collect();
    if comma_parts.len() == 4 && comma_parts.iter().all(|p| !p.trim().is_empty()) {
        let mut keys = [KeyCode::KeyA; 4];
        for (idx, part) in comma_parts.iter().enumerate() {
            keys[idx] = parse_keycode_token(part.trim())?;
        }
        return Ok(keys);
    }

    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() == 4 {
        let mut keys = [KeyCode::KeyA; 4];
        for (idx, ch) in chars.into_iter().enumerate() {
            keys[idx] = parse_keycode_char(ch)?;
        }
        return Ok(keys);
    }

    Err("expected 4 characters (e.g. 'askl') or 4 comma-separated keys (e.g. 'a,s,up,right')".to_string())
}

fn parse_keycode_char(ch: char) -> Result<KeyCode, String> {
    match ch.to_ascii_lowercase() {
        'a' => Ok(KeyCode::KeyA),
        'b' => Ok(KeyCode::KeyB),
        'c' => Ok(KeyCode::KeyC),
        'd' => Ok(KeyCode::KeyD),
        'e' => Ok(KeyCode::KeyE),
        'f' => Ok(KeyCode::KeyF),
        'g' => Ok(KeyCode::KeyG),
        'h' => Ok(KeyCode::KeyH),
        'i' => Ok(KeyCode::KeyI),
        'j' => Ok(KeyCode::KeyJ),
        'k' => Ok(KeyCode::KeyK),
        'l' => Ok(KeyCode::KeyL),
        'm' => Ok(KeyCode::KeyM),
        'n' => Ok(KeyCode::KeyN),
        'o' => Ok(KeyCode::KeyO),
        'p' => Ok(KeyCode::KeyP),
        'q' => Ok(KeyCode::KeyQ),
        'r' => Ok(KeyCode::KeyR),
        's' => Ok(KeyCode::KeyS),
        't' => Ok(KeyCode::KeyT),
        'u' => Ok(KeyCode::KeyU),
        'v' => Ok(KeyCode::KeyV),
        'w' => Ok(KeyCode::KeyW),
        'x' => Ok(KeyCode::KeyX),
        'y' => Ok(KeyCode::KeyY),
        'z' => Ok(KeyCode::KeyZ),
        '0' => Ok(KeyCode::Digit0),
        '1' => Ok(KeyCode::Digit1),
        '2' => Ok(KeyCode::Digit2),
        '3' => Ok(KeyCode::Digit3),
        '4' => Ok(KeyCode::Digit4),
        '5' => Ok(KeyCode::Digit5),
        '6' => Ok(KeyCode::Digit6),
        '7' => Ok(KeyCode::Digit7),
        '8' => Ok(KeyCode::Digit8),
        '9' => Ok(KeyCode::Digit9),
        ',' => Ok(KeyCode::Comma),
        '.' => Ok(KeyCode::Period),
        '/' => Ok(KeyCode::Slash),
        ';' => Ok(KeyCode::Semicolon),
        '\'' => Ok(KeyCode::Quote),
        '[' => Ok(KeyCode::BracketLeft),
        ']' => Ok(KeyCode::BracketRight),
        '\\' => Ok(KeyCode::Backslash),
        '-' => Ok(KeyCode::Minus),
        '=' => Ok(KeyCode::Equal),
        '`' => Ok(KeyCode::Backquote),
        ' ' => Ok(KeyCode::Space),
        other => Err(format!("unsupported key '{other}'")),
    }
}

fn parse_keycode_token(token: &str) -> Result<KeyCode, String> {
    let mut chars = token.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        return parse_keycode_char(ch);
    }

    let normalized = token.to_ascii_lowercase().replace(['-', '_', ' '], "");
    if let Some(rest) = normalized.strip_prefix("key")
        && rest.len() == 1
        && let Some(ch) = rest.chars().next()
    {
        return parse_keycode_char(ch);
    }
    if let Some(rest) = normalized.strip_prefix("digit")
        && rest.len() == 1
        && let Some(ch) = rest.chars().next()
    {
        return parse_keycode_char(ch);
    }

    match normalized.as_str() {
        "left" | "arrowleft" => Ok(KeyCode::ArrowLeft),
        "down" | "arrowdown" => Ok(KeyCode::ArrowDown),
        "up" | "arrowup" => Ok(KeyCode::ArrowUp),
        "right" | "arrowright" => Ok(KeyCode::ArrowRight),
        "space" | "spacebar" => Ok(KeyCode::Space),
        "tab" => Ok(KeyCode::Tab),
        "enter" | "return" => Ok(KeyCode::Enter),
        "backspace" | "bksp" => Ok(KeyCode::Backspace),
        "esc" | "escape" => Ok(KeyCode::Escape),
        "capslock" | "caps" => Ok(KeyCode::CapsLock),
        "lshift" | "shiftleft" | "leftshift" | "shift" => Ok(KeyCode::ShiftLeft),
        "rshift" | "shiftright" | "rightshift" => Ok(KeyCode::ShiftRight),
        "lctrl" | "ctrlleft" | "controlleft" | "leftctrl" | "ctrl" => Ok(KeyCode::ControlLeft),
        "rctrl" | "ctrlright" | "controlright" | "rightctrl" => Ok(KeyCode::ControlRight),
        "lalt" | "altleft" | "leftalt" | "alt" => Ok(KeyCode::AltLeft),
        "ralt" | "altright" | "rightalt" => Ok(KeyCode::AltRight),
        "comma" => Ok(KeyCode::Comma),
        "period" | "dot" => Ok(KeyCode::Period),
        "slash" => Ok(KeyCode::Slash),
        "backslash" => Ok(KeyCode::Backslash),
        "semicolon" => Ok(KeyCode::Semicolon),
        "quote" | "apostrophe" => Ok(KeyCode::Quote),
        "bracketleft" | "lbracket" => Ok(KeyCode::BracketLeft),
        "bracketright" | "rbracket" => Ok(KeyCode::BracketRight),
        "minus" => Ok(KeyCode::Minus),
        "equal" | "equals" => Ok(KeyCode::Equal),
        "backquote" | "grave" => Ok(KeyCode::Backquote),
        "insert" | "ins" => Ok(KeyCode::Insert),
        "delete" | "del" => Ok(KeyCode::Delete),
        "home" => Ok(KeyCode::Home),
        "end" => Ok(KeyCode::End),
        "pageup" | "pgup" => Ok(KeyCode::PageUp),
        "pagedown" | "pgdn" => Ok(KeyCode::PageDown),
        "num0" | "numpad0" => Ok(KeyCode::Numpad0),
        "num1" | "numpad1" => Ok(KeyCode::Numpad1),
        "num2" | "numpad2" => Ok(KeyCode::Numpad2),
        "num3" | "numpad3" => Ok(KeyCode::Numpad3),
        "num4" | "numpad4" => Ok(KeyCode::Numpad4),
        "num5" | "numpad5" => Ok(KeyCode::Numpad5),
        "num6" | "numpad6" => Ok(KeyCode::Numpad6),
        "num7" | "numpad7" => Ok(KeyCode::Numpad7),
        "num8" | "numpad8" => Ok(KeyCode::Numpad8),
        "num9" | "numpad9" => Ok(KeyCode::Numpad9),
        "numadd" | "numpadadd" => Ok(KeyCode::NumpadAdd),
        "numsub" | "numpadsubtract" => Ok(KeyCode::NumpadSubtract),
        "nummul" | "numpadmultiply" => Ok(KeyCode::NumpadMultiply),
        "numdiv" | "numpaddivide" => Ok(KeyCode::NumpadDivide),
        "numdot" | "numpaddecimal" => Ok(KeyCode::NumpadDecimal),
        "numenter" | "numpadenter" => Ok(KeyCode::NumpadEnter),
        "f1" => Ok(KeyCode::F1),
        "f2" => Ok(KeyCode::F2),
        "f3" => Ok(KeyCode::F3),
        "f4" => Ok(KeyCode::F4),
        "f5" => Ok(KeyCode::F5),
        "f6" => Ok(KeyCode::F6),
        "f7" => Ok(KeyCode::F7),
        "f8" => Ok(KeyCode::F8),
        "f9" => Ok(KeyCode::F9),
        "f10" => Ok(KeyCode::F10),
        "f11" => Ok(KeyCode::F11),
        "f12" => Ok(KeyCode::F12),
        _ => Err(format!("unknown key '{token}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_compact_kb_flag() {
        let flags = LaunchFlags::parse(["--kb", "askl"]);
        assert_eq!(
            flags.keybinds,
            Some([KeyCode::KeyA, KeyCode::KeyS, KeyCode::KeyK, KeyCode::KeyL])
        );
    }

    #[test]
    fn parses_inline_and_comma_kb_flags() {
        let flags = LaunchFlags::parse(["--kb:DFJK"]);
        assert_eq!(
            flags.keybinds,
            Some([KeyCode::KeyD, KeyCode::KeyF, KeyCode::KeyJ, KeyCode::KeyK])
        );

        let flags = LaunchFlags::parse(["--kb=lshift,z,slash,rshift"]);
        assert_eq!(
            flags.keybinds,
            Some([
                KeyCode::ShiftLeft,
                KeyCode::KeyZ,
                KeyCode::Slash,
                KeyCode::ShiftRight
            ])
        );

        let flags = LaunchFlags::parse(["--kb", "a,s,up,right"]);
        assert_eq!(
            flags.keybinds,
            Some([
                KeyCode::KeyA,
                KeyCode::KeyS,
                KeyCode::ArrowUp,
                KeyCode::ArrowRight
            ])
        );
    }

    #[test]
    fn rejects_invalid_kb_flag() {
        assert!(parse_keybinds("asd").is_err());
        assert!(parse_keybinds("asdfg").is_err());
        assert!(parse_keybinds("a,s,unknown,l").is_err());
        let flags = LaunchFlags::parse(["--kb", "abc"]);
        assert_eq!(flags.keybinds, None);
    }
}

