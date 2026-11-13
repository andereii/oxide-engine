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
}

