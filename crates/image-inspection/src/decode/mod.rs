pub(crate) mod bmp;
pub(crate) mod png;

use crate::types::StegStatus;

/// Errors raised while probing or decoding a container.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("{reason}")]
pub(crate) struct DecodeError {
    pub status: StegStatus,
    pub reason: &'static str,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl DecodeError {
    pub(crate) fn unsupported(reason: &'static str) -> Self {
        Self {
            status: StegStatus::Unsupported,
            reason,
            width: None,
            height: None,
        }
    }

    pub(crate) fn unsupported_at(reason: &'static str, width: u32, height: u32) -> Self {
        Self {
            status: StegStatus::Unsupported,
            reason,
            width: Some(width),
            height: Some(height),
        }
    }

    pub(crate) fn oversized_at(reason: &'static str, width: u32, height: u32) -> Self {
        Self {
            status: StegStatus::Oversized,
            reason,
            width: Some(width),
            height: Some(height),
        }
    }

    pub(crate) fn corrupt(reason: &'static str) -> Self {
        Self {
            status: StegStatus::Corrupt,
            reason,
            width: None,
            height: None,
        }
    }

    pub(crate) fn corrupt_at(reason: &'static str, width: u32, height: u32) -> Self {
        Self {
            status: StegStatus::Corrupt,
            reason,
            width: Some(width),
            height: Some(height),
        }
    }
}

/// Colour layout after alpha removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Channels {
    Gray,
    GrayAlpha,
    Rgb,
    Rgba,
    Indexed,
}

impl Channels {
    pub(crate) fn plane_count(self) -> usize {
        match self {
            Self::Gray | Self::GrayAlpha | Self::Indexed => 1,
            Self::Rgb | Self::Rgba => 3,
        }
    }

    pub(crate) fn alpha_excluded(self) -> bool {
        matches!(self, Self::GrayAlpha | Self::Rgba)
    }

    pub(crate) fn indexed(self) -> bool {
        matches!(self, Self::Indexed)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DecodedHeader {
    pub width: u32,
    pub height: u32,
    pub channels: Channels,
}

impl DecodedHeader {
    pub(crate) fn plane_count(self) -> usize {
        self.channels.plane_count()
    }
}

/// Streams top-down rows into the shared aggregator.
pub(crate) trait RowSource {
    fn header(&self) -> DecodedHeader;
    fn next_row(&mut self, sink: &mut dyn RowSink) -> Result<bool, DecodeError>;
}

/// Receives one logical row with alpha already removed.
pub(crate) trait RowSink {
    fn on_row(&mut self, y: u32, planes: &[&[u8]], width: usize);
}
