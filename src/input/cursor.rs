use bevy::prelude::Resource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMode {
    Default,
    Cross,
    Eraser,
    Grabbing,
    Hourglass,
    Pointer,
    Text,
    TextVertical,
    ZoomIn,
    ZoomOut,
    Crosshair,
    Cell,
    Scroll,
}

#[derive(Resource, Debug, Clone)]
pub struct CursorState {
    pub visible: bool,
    pub mode: Option<CursorMode>,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            visible: true,
            mode: Some(CursorMode::Default),
        }
    }
}

impl CursorState {
    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn set_mode(&mut self, mode: Option<CursorMode>) {
        self.mode = mode;
    }
}

