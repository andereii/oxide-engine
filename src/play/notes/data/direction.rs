use bevy::prelude::Color;

use crate::utils::constants::KEY_COUNT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NoteDirection {
    Left = 0,
    Down = 1,
    Up = 2,
    Right = 3,
}

impl NoteDirection {
    pub const ALL: [Self; 4] = [Self::Left, Self::Down, Self::Up, Self::Right];

    pub fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Left),
            1 => Some(Self::Down),
            2 => Some(Self::Up),
            3 => Some(Self::Right),
            _ => None,
        }
    }

    pub fn from_int(value: i32) -> Self {
        match value.rem_euclid(KEY_COUNT as i32) {
            0 => Self::Left,
            1 => Self::Down,
            2 => Self::Up,
            _ => Self::Right,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Down => "down",
            Self::Up => "up",
            Self::Right => "right",
        }
    }

    pub fn name_upper(self) -> &'static str {
        match self {
            Self::Left => "LEFT",
            Self::Down => "DOWN",
            Self::Up => "UP",
            Self::Right => "RIGHT",
        }
    }

    pub fn color_name(self) -> &'static str {
        match self {
            Self::Left => "purple",
            Self::Down => "blue",
            Self::Up => "green",
            Self::Right => "red",
        }
    }

    pub fn color_title(self) -> &'static str {
        match self {
            Self::Left => "Purple",
            Self::Down => "Blue",
            Self::Up => "Green",
            Self::Right => "Red",
        }
    }

    pub fn color(self) -> Color {
        match self {
            Self::Left => Color::srgb(1.0, 0.133, 0.667),
            Self::Down => Color::srgb(0.0, 0.933, 1.0),
            Self::Up => Color::srgb(0.0, 0.8, 0.0),
            Self::Right => Color::srgb(0.8, 0.067, 0.067),
        }
    }

    pub fn mirror(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Down => Self::Up,
            Self::Up => Self::Down,
            Self::Right => Self::Left,
        }
    }
}
