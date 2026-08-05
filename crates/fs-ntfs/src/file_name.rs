#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileNameNamespace {
    Posix,
    Win32,
    Dos,
    Win32AndDos,
    Unknown,
}

impl FileNameNamespace {
    pub(crate) fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Posix,
            1 => Self::Win32,
            2 => Self::Dos,
            3 => Self::Win32AndDos,
            _ => Self::Unknown,
        }
    }

    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Win32 => 4,
            Self::Win32AndDos => 3,
            Self::Posix => 2,
            Self::Dos => 1,
            Self::Unknown => 0,
        }
    }

    pub(crate) fn is_dos(self) -> bool {
        self == Self::Dos
    }
}
