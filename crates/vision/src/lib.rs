//! Read-only capture of Dofus game windows, for the companion's combat reading.
//!
//! Only the pixels of the game's own windows are read, never the full screen:
//! no input is ever synthesized, no process memory is read and no network
//! traffic is touched. Frames stay in memory and are dropped once read, unless
//! the player turns on the local debug recording ([`Recorder`]).
//!
//! On macOS the capture goes through ScreenCaptureKit and needs the Screen
//! Recording permission only. Other systems are not supported yet.

mod debug;
#[cfg(target_os = "macos")]
mod macos;

use std::fmt;
use std::time::Duration;

pub use debug::Recorder;

/// The macOS bundle identifier of the Dofus 3 client, as the Ankama Launcher
/// installs it.
pub const DOFUS_BUNDLE_ID: &str = "com.Ankama.Dofus";

/// A window of the game client, as the system lists it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct GameWindow {
    pub id: u32,
    pub pid: i32,
    pub title: String,
    /// Position and size on screen, in points.
    pub frame: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One image of a game window, BGRA, rows packed without padding.
pub struct Frame {
    pub window: u32,
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Receives the frames of a stream, on the capture queue: it must return
/// quickly and hand heavy work to another thread.
pub type Sink = Box<dyn Fn(Frame) + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    /// This system has no capture backend yet.
    Unsupported,
    /// The player has not granted the Screen Recording permission.
    Denied,
    /// The window is gone.
    WindowGone(u32),
    Failed(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::Unsupported => {
                write!(f, "La lecture du combat n'est pas encore disponible sur ce système.")
            }
            CaptureError::Denied => write!(
                f,
                "Autorisez Krozties dans Réglages Système › Confidentialité et sécurité › \
                 Enregistrement de l'écran, puis relancez-le."
            ),
            CaptureError::WindowGone(_) => write!(f, "La fenêtre du jeu s'est fermée."),
            CaptureError::Failed(e) => write!(f, "La capture a échoué : {e}"),
        }
    }
}

impl std::error::Error for CaptureError {}

/// The open windows of the game client.
pub fn game_windows() -> Result<Vec<GameWindow>, CaptureError> {
    #[cfg(target_os = "macos")]
    return macos::game_windows();
    #[cfg(not(target_os = "macos"))]
    Err(CaptureError::Unsupported)
}

/// The game window the player is using: the client in front, when the client
/// in front is the game. Needs no Accessibility permission.
pub fn active_game_window(windows: &[GameWindow]) -> Option<u32> {
    #[cfg(target_os = "macos")]
    return macos::active_game_window(windows);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = windows;
        None
    }
}

/// A running capture of one game window. Dropping it stops the capture.
pub struct Stream {
    #[cfg(target_os = "macos")]
    _inner: macos::Stream,
}

impl Stream {
    /// Captures `window` at most once per `interval`, and hands each new frame
    /// to `sink`. Frames identical to the previous one are not delivered.
    pub fn start(window: u32, interval: Duration, sink: Sink) -> Result<Stream, CaptureError> {
        #[cfg(target_os = "macos")]
        return Ok(Stream { _inner: macos::Stream::start(window, interval, sink)? });
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (window, interval, sink);
            Err(CaptureError::Unsupported)
        }
    }
}
