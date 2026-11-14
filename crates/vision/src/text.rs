//! The text on a frame, read by the system's own text recognition: Vision on
//! macOS. It runs on the machine; nothing is sent anywhere.

use crate::place::Line;
use crate::{CaptureError, Frame};

/// A part of a frame, as shares of its width and height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl Region {
    /// The whole frame.
    pub const ALL: Region = Region { left: 0.0, top: 0.0, width: 1.0, height: 1.0 };
    /// The top left of the window, where games write where the player is.
    pub const TOP_LEFT: Region = Region { left: 0.0, top: 0.0, width: 0.45, height: 0.25 };

    /// The region in frame pixels: left, top, width, height, within the frame.
    pub fn pixels(&self, frame: &Frame) -> (usize, usize, usize, usize) {
        let (w, h) = (frame.width as f32, frame.height as f32);
        let left = (self.left * w).clamp(0.0, w) as usize;
        let top = (self.top * h).clamp(0.0, h) as usize;
        let width = ((self.width * w) as usize).min(frame.width as usize - left);
        let height = ((self.height * h) as usize).min(frame.height as usize - top);
        (left, top, width, height)
    }
}

/// The lines of text in a region of a frame, their bounds in frame pixels.
pub fn lines(frame: &Frame, region: Region) -> Result<Vec<Line>, CaptureError> {
    #[cfg(target_os = "macos")]
    return crate::macos_text::lines(frame, region);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (frame, region);
        Err(CaptureError::Unsupported)
    }
}
