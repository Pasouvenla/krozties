//! Combat reading in the companion. For now, the local debug recording that
//! builds the ground-truth captures: when Krozties is launched with
//! `KROZTIES_DEBUG_CAPTURE` naming a folder, every game window is recorded at
//! most once per second, and only frames that changed are written.
//!
//! The frames never leave the machine. Errors go to `capture.log` in the same
//! folder, since an app opened from the Finder has no visible console.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dofus_vision::{game_windows, CaptureError, Frame, GameWindow, Recorder, Stream};

/// The environment variable that turns the recording on.
const ENV: &str = "KROZTIES_DEBUG_CAPTURE";
/// How often the game windows are listed again: a client opened later is
/// recorded within this delay, and a title that changed at login is picked up.
const RESCAN: Duration = Duration::from_secs(3);
/// At most one frame per second: the placement phase and the spell bar change
/// slower than that.
const INTERVAL: Duration = Duration::from_secs(1);

/// Starts the recording on its own thread when the environment asks for it.
pub fn start_debug_recording_from_env() {
    let Some(dir) = std::env::var_os(ENV) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let _ = std::thread::Builder::new().name("debug-capture".into()).spawn(move || {
        if let Err(e) = record(&dir) {
            log(&dir, &e);
        }
    });
}

fn log(dir: &std::path::Path, message: &str) {
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("capture.log")) {
        let _ = writeln!(file, "{millis} {message}");
    }
}

fn record(dir: &std::path::Path) -> Result<(), String> {
    let recorder = Recorder::new(dir).map_err(|e| e.to_string())?;
    // The latest known state of each window: its title changes at login.
    let windows: Arc<Mutex<HashMap<u32, GameWindow>>> = Arc::default();
    // Frames are written on one thread at the pace the disk allows: when it
    // falls behind, new frames are dropped rather than queued.
    let (tx, rx) = mpsc::sync_channel::<Frame>(2);
    let known = Arc::clone(&windows);
    let folder = dir.to_path_buf();
    std::thread::Builder::new()
        .name("debug-capture-writer".into())
        .spawn(move || {
            for frame in rx {
                let window = known.lock().unwrap_or_else(|e| e.into_inner()).get(&frame.window).cloned();
                if let Some(window) = window {
                    if let Err(e) = recorder.record(&window, &frame) {
                        log(&folder, &format!("write failed: {e}"));
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;

    log(dir, "recording started");
    let mut streams: HashMap<u32, Stream> = HashMap::new();
    loop {
        match game_windows() {
            Ok(listed) => {
                streams.retain(|id, _| listed.iter().any(|w| w.id == *id));
                {
                    let mut known = windows.lock().unwrap_or_else(|e| e.into_inner());
                    known.clear();
                    known.extend(listed.iter().map(|w| (w.id, w.clone())));
                }
                for window in &listed {
                    if streams.contains_key(&window.id) {
                        continue;
                    }
                    let tx = tx.clone();
                    let sink = Box::new(move |frame: Frame| {
                        let _ = tx.try_send(frame);
                    });
                    match Stream::start(window.id, INTERVAL, sink) {
                        Ok(stream) => {
                            log(dir, &format!("window {} recorded ({})", window.id, window.title));
                            streams.insert(window.id, stream);
                        }
                        Err(e) => log(dir, &format!("window {}: {e}", window.id)),
                    }
                }
            }
            Err(CaptureError::Denied) => return Err(CaptureError::Denied.to_string()),
            Err(e) => log(dir, &e.to_string()),
        }
        std::thread::sleep(RESCAN);
    }
}
