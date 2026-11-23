use std::path::PathBuf;

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
    pub capture_timestamps_ms: Vec<f64>,
    pub capture_out_dir: Option<PathBuf>,
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
            capture_timestamps_ms: Vec::new(),
            capture_out_dir: None,
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
             \x20 --capture-ms:<ms,...>       Capture screenshots at sorted ms and exit\n\
             \x20 --capture-out:<dir>         Output directory for --capture-ms (default: captures)\n\
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

