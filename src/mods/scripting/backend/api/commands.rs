use crate::animations::BaseAnimationData;
use crate::play::hud::subtitles::SubtitleCue;

use super::tweens::TweenProp;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptPropAnimKind {
    Static,
    Rect,
    Sparrow,
    Atlas,
}

#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub enum ScriptCommand {
    Log {
        level: LogLevel,
        mod_id: String,
        message: String,
    },
    SetStatus(String),
    SetScoreText(Option<String>),
    SetHudVisible(bool),
    FadeHud {
        alpha: f32,
        duration_s: f64,
        ease: String,
    },
    TweenCameraZoom {
        target: f64,
        duration_s: f64,
        ease: String,
    },
    TweenCameraPos {
        x: f64,
        y: f64,
        duration_s: f64,
        ease: String,
    },
    TweenScrollSpeed {
        target: f64,
        duration_s: f64,
        ease: String,
    },
    FocusCameraTarget(String),
    CameraBop {
        game_delta: f64,
        hud_delta: f64,
    },
    SetCameraBopConfig {
        intensity: f64,
        rate: f64,
    },
    CameraShake {
        intensity: f32,
        duration_s: f64,
        hud: bool,
    },
    CameraFlash {
        color: [f32; 4],
        duration_s: f64,
        hud: bool,
    },
    CameraFade {
        color: [f32; 4],
        duration_s: f64,
        fade_in: bool,
        hud: bool,
    },
    ResetCamera {
        zoom: bool,
        tweens: bool,
        snap: bool,
    },
    CancelCameraTweens,
    CharDance {
        target: String,
        force: bool,
    },
    SetCharFlip {
        target: String,
        flip_x: bool,
        flip_y: bool,
    },
    SetCharZ {
        target: String,
        z: f32,
    },
    SwapChar {
        role: String,
        char_id: String,
    },
    PlayAnim {
        target: String,
        anim: String,
        force: bool,
    },
    PlayAnimFrame {
        target: String,
        anim: String,
        force: bool,
        frame: usize,
    },
    PlaySing {
        target: String,
        dir: usize,
        miss: bool,
        suffix: String,
    },
    SetCharacterPos {
        target: String,
        x: f32,
        y: f32,
    },
    SetCharacterScale {
        target: String,
        sx: f32,
        sy: f32,
    },
    SetCharacterTint {
        target: String,
        color: [f32; 4],
    },
    SetCharacterAlpha {
        target: String,
        alpha: f32,
    },
    SetCharacterVisible {
        target: String,
        visible: bool,
    },
    SetCharacterDanceEvery {
        target: String,
        beats: f32,
    },
    SetCharTrail {
        target: String,
        length: usize,
        delay_s: f64,
        alpha: f32,
        diff: f32,
        z_layer: Option<i32>,
    },
    ClearCharTrail {
        target: String,
    },
    SetCharUnderlay {
        target: String,
        char_id: String,
        offset: [f32; 2],
        z_offset: i32,
    },
    ClearCharUnderlay {
        target: String,
    },
    SetLaneOffset {
        lane: usize,
        x: f32,
        y: f32,
    },
    SetLaneAngle {
        lane: usize,
        deg: f32,
    },
    SetLaneScale {
        lane: usize,
        sx: f32,
        sy: f32,
    },
    SetLaneAlpha {
        lane: usize,
        alpha: f32,
    },
    ResetLanes,
    SpawnProp {
        id: String,
        image_path: Option<String>,
        color: [f32; 4],
        size: [f32; 2],
        pos: [f32; 2],
        z: f32,
        hud: bool,
    },
    SpawnAnimatedProp {
        id: String,
        path: String,
        kind: ScriptPropAnimKind,
        anims: Vec<BaseAnimationData>,
        color: [f32; 4],
        size: [f32; 2],
        pos: [f32; 2],
        z: f32,
        hud: bool,
        top_left: bool,
    },
    DespawnProp(String),
    SetPropNumber {
        id: String,
        prop: TweenProp,
        value: f32,
    },
    SetPropTint {
        id: String,
        color: [f32; 4],
    },
    SetPropVisible {
        id: String,
        visible: bool,
    },
    SetPropFlip {
        id: String,
        flip_x: bool,
        flip_y: bool,
    },
    SetPropScroll {
        id: String,
        sx: f32,
        sy: f32,
    },
    SetPropZ {
        id: String,
        z: f32,
    },
    SetPropSize {
        id: String,
        w: f32,
        h: f32,
    },
    SetPropVelocity {
        id: String,
        vx: f32,
        vy: f32,
        ax: f32,
        ay: f32,
    },
    AddPropAnim {
        id: String,
        anim: BaseAnimationData,
    },
    SpawnText {
        id: String,
        text: String,
        pos: [f32; 2],
        z: f32,
        size: f32,
        color: [f32; 4],
        font: Option<String>,
        align: String,
        hud: bool,
    },
    SetText {
        id: String,
        text: String,
    },
    SetTextStyle {
        id: String,
        size: Option<f32>,
        color: Option<[f32; 4]>,
        align: Option<String>,
    },
    LoadSubtitles {
        cues: Vec<SubtitleCue>,
    },
    ClearSubtitles,
    KillNote {
        id: usize,
    },
    ModifyNote {
        id: usize,
        time_ms: Option<f64>,
        dir: Option<usize>,
        length_ms: Option<f64>,
        kind: Option<String>,
        scoreable: Option<bool>,
    },
    StrumAnim {
        lane: usize,
        state: String,
    },
    SpawnSplash {
        lane: usize,
    },
    SetNotesplashVisible(bool),
    SetStrumStyle {
        side: String,
        scale: f32,
        spacing: f32,
    },
    VwooshNotes {
        side: String,
        fade_in: bool,
    },
    SetStrumVisible {
        side: String,
        visible: bool,
    },
    RegisterNoteKind {
        id: String,
        description: String,
        note_style_id: Option<String>,
        no_anim: bool,
        suffix: String,
        scoreable: bool,
        low_priority: bool,
    },
    SetBarColors {
        opp: [f32; 4],
        ply: [f32; 4],
    },
    SetIcon {
        side: String,
        char_id: String,
    },
    SetIconScale {
        side: String,
        scale: f32,
    },
    SetIconBop {
        side: String,
        enabled: bool,
        every: i32,
        angle_deg: f32,
    },
    SpawnPopup {
        judgement: String,
        combo: Option<u32>,
    },
    SetPopupsConfig {
        visible: Option<bool>,
        offset: Option<[f32; 2]>,
    },
    ControlCountdown {
        action: String,
    },
    SeekSong(f64),
    ResumeSong,
    PlaySound {
        tag: String,
        path: String,
        volume: f32,
        looped: bool,
        pitch: f32,
    },
    StopSound(String),
    PauseSound {
        tag: String,
        paused: bool,
    },
    SetSoundParam {
        tag: String,
        volume: Option<f32>,
        pitch: Option<f32>,
    },
    SetBlendMode {
        target: String,
        mode: String,
    },
    SetCustomShader {
        target: String,
        path: String,
    },
    SetShaderParam {
        target: String,
        key: String,
        values: [f32; 4],
    },
    SetShaderTex {
        target: String,
        path: String,
    },
    TakeScreenshot(Option<String>),
    SetPlaylist {
        story_mode: Option<bool>,
        songs: Option<Vec<String>>,
        score: Option<i32>,
    },
    LoadSong {
        song_id: String,
        difficulty: Option<String>,
        variation: Option<String>,
    },
    SetAudioVolumes {
        inst: Option<f32>,
        vocals: Option<f32>,
        player_vocal: Option<f32>,
        opponent_vocal: Option<f32>,
    },
    AddNote {
        time_ms: f64,
        dir: usize,
        length_ms: f64,
        kind: String,
    },
    TriggerSongEvent {
        name: String,
        value: serde_json::Value,
    },
    PlaySfx {
        path: String,
        volume: f32,
    },
    PreloadSound {
        path: String,
    },
    SetShaderAdjustColor {
        target: String,
        hue: f32,
        saturation: f32,
        brightness: f32,
        contrast: f32,
    },
    SetShaderDropShadow {
        target: String,
        color: [f32; 3],
        angle_deg: f32,
        distance: f32,
        strength: f32,
        threshold: f32,
        hue: f32,
        saturation: f32,
        brightness: f32,
        contrast: f32,
        use_alt_mask: bool,
        alt_mask_path: Option<String>,
        mask_threshold: f32,
    },
    SetShaderWiggle {
        target: String,
        effect: String,
        speed: f32,
        frequency: f32,
        amplitude: f32,
    },
    SetCameraRain {
        scale: f32,
        start_intensity: f32,
        end_intensity: f32,
        color: [f32; 3],
        time_scale_decay: bool,
    },
    SetShaderOverlayBlend {
        target: String,
        aux_path: String,
    },
    SetShaderRainSprite {
        target: String,
        scale: f32,
        intensity: f32,
        color: [f32; 3],
    },
    SetShaderBuilding {
        target: String,
        alpha_shit: f32,
    },
    ClearShader {
        target: String,
    },
    EndSong,
    RestartSong,
    PauseSong,
}
