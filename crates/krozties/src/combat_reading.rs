//! Combat reading in the companion.
//!
//! Two things for now, each turned on by an environment variable until the
//! companion offers them, since reading the game asks for the Screen
//! Recording permission, which no player should meet unannounced:
//!
//! - `KROZTIES_COMBAT_READING`: every game window is followed from fight to
//!   fight, and each fight's reading of its map goes to the local store of
//!   maps, `maps.json` in the app's data folder. Only the reading is kept:
//!   no frame, no name, no chat.
//! - `KROZTIES_DEBUG_CAPTURE`, naming a folder: the local debug recording
//!   that builds the ground-truth captures. Every game window is recorded at
//!   most once per second, and only frames that changed are written.
//!
//! Nothing leaves the machine. Errors and events go to a log file next to
//! the data, since an app opened from the Finder has no visible console.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dofus_maps::{Settings, Store};
use dofus_vision::fight::{Follower, Step};
use dofus_vision::{game_windows, CaptureError, Frame, GameWindow, Recorder, Stream};

/// The environment variable that turns the combat reading on.
const READING_ENV: &str = "KROZTIES_COMBAT_READING";
/// The environment variable that turns the recording on.
const ENV: &str = "KROZTIES_DEBUG_CAPTURE";
/// The maps read, in the app's data folder.
const STORE_FILE: &str = "maps.json";
/// What the reading did, next to the maps.
const READING_LOG: &str = "combat-reading.log";
/// How often the game windows are listed again: a client opened later is
/// recorded within this delay, and a title that changed at login is picked up.
const RESCAN: Duration = Duration::from_secs(3);
/// At most one frame per second: the placement phase and the spell bar change
/// slower than that.
const INTERVAL: Duration = Duration::from_secs(1);
/// Two windows whose fights start this close together see one fight: two
/// accounts of the player in the same fight.
const SAME_FIGHT_MS: u64 = 10_000;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Starts the combat reading on its own thread when the environment asks for
/// it, the maps kept in `data`.
pub fn start_reading_from_env(data: PathBuf) {
    if std::env::var_os(READING_ENV).is_none() {
        return;
    }
    let _ = std::thread::Builder::new().name("combat-reading".into()).spawn(move || {
        let log_file = data.join(READING_LOG);
        if let Err(e) = read(&data, &log_file) {
            log_to(&log_file, &e);
        }
    });
}

/// The fights that started lately, by window, to name alike the fights of
/// two windows that start together.
#[derive(Default)]
struct Fights {
    started: Vec<(u32, String, u64)>,
}

impl Fights {
    /// The name a fight starting on `window` at `at_ms` takes: that of a fight
    /// another window started a moment before, else its own.
    fn name(&mut self, window: u32, own: &str, at_ms: u64) -> String {
        self.started.retain(|(_, _, t)| at_ms.saturating_sub(*t) <= SAME_FIGHT_MS);
        let name = self.started.iter().find(|(w, _, _)| *w != window).map_or_else(|| own.to_string(), |f| f.1.clone());
        self.started.push((window, name.clone(), at_ms));
        name
    }
}

/// Follows every game window, and keeps each fight's reading in the store.
fn read(data: &Path, log_file: &Path) -> Result<(), String> {
    std::fs::create_dir_all(data).map_err(|e| e.to_string())?;
    let path = data.join(STORE_FILE);
    let mut store = Store::load(&path, Settings::default()).map_err(|e| e.to_string())?;
    // Frames are read on this thread at the pace reading allows: when it falls
    // behind, new frames are dropped rather than queued.
    let (tx, rx) = mpsc::sync_channel::<Frame>(2);
    let windows_log = log_file.to_path_buf();
    std::thread::Builder::new()
        .name("combat-reading-windows".into())
        .spawn(move || capture_windows(&tx, &windows_log, |_| {}))
        .map_err(|e| e.to_string())?;

    log_to(log_file, "reading started");
    let mut followers: HashMap<u32, Follower> = HashMap::new();
    let mut fights = Fights::default();
    loop {
        let received = match rx.recv_timeout(INTERVAL) {
            Ok(frame) => Some(frame),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err("the capture stopped".into()),
        };
        let now = now_ms();
        let fed = received.as_ref().map(|frame| frame.window);
        // The window of the frame reads it; the others still show what they
        // showed, and time passes for them.
        let mut steps = Vec::new();
        if let Some(frame) = &received {
            let follower = followers.entry(frame.window).or_insert_with(|| Follower::new(frame.window, Settings::default()));
            steps.push((frame.window, follower.feed(frame, now)));
        }
        for (window, follower) in followers.iter_mut().filter(|(w, _)| Some(**w) != fed) {
            steps.push((*window, follower.tick(now)));
        }
        for (window, step) in steps {
            match step {
                Step::Started(_) => {
                    let Some(follower) = followers.get_mut(&window) else { continue };
                    let own = follower.fight().unwrap_or_default().to_string();
                    let name = fights.name(window, &own, now);
                    follower.rename_fight(&name);
                    log_to(log_file, &format!("fight {name} started in window {window}"));
                }
                Step::Ended(reading) => {
                    let name = reading.fight.clone();
                    match store.add(reading) {
                        Ok(outcome) => {
                            log_to(
                                log_file,
                                &format!(
                                    "fight {name} ended: map {} {:?}, shift {:?}, verified {}",
                                    outcome.map, outcome.event, outcome.shift, outcome.verified
                                ),
                            );
                            if let Err(e) = store.save(&path) {
                                log_to(log_file, &format!("the maps could not be saved: {e}"));
                            }
                        }
                        Err(e) => log_to(log_file, &format!("fight {name} not kept: {e}")),
                    }
                }
                Step::Nothing | Step::Read(_) => {}
            }
        }
    }
}

/// Lists the game windows again and again, and captures each into `tx` until
/// it closes; `known` hears of every listing. Returns when the permission is
/// refused.
fn capture_windows(tx: &mpsc::SyncSender<Frame>, log_file: &Path, known: impl Fn(&[GameWindow])) {
    let mut streams: HashMap<u32, Stream> = HashMap::new();
    loop {
        match game_windows() {
            Ok(listed) => {
                streams.retain(|id, _| listed.iter().any(|w| w.id == *id));
                known(&listed);
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
                            log_to(log_file, &format!("window {} captured ({})", window.id, window.title));
                            streams.insert(window.id, stream);
                        }
                        Err(e) => log_to(log_file, &format!("window {}: {e}", window.id)),
                    }
                }
            }
            Err(CaptureError::Denied) => {
                log_to(log_file, &CaptureError::Denied.to_string());
                return;
            }
            Err(e) => log_to(log_file, &e.to_string()),
        }
        std::thread::sleep(RESCAN);
    }
}

fn log_to(file: &Path, message: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
        let _ = writeln!(file, "{} {message}", now_ms());
    }
}

/// Starts the recording on its own thread when the environment asks for it.
pub fn start_debug_recording_from_env() {
    let Some(dir) = std::env::var_os(ENV) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let _ = std::thread::Builder::new().name("debug-capture".into()).spawn(move || {
        if let Err(e) = record(&dir) {
            log_to(&dir.join("capture.log"), &e);
        }
    });
}

fn record(dir: &Path) -> Result<(), String> {
    let recorder = Recorder::new(dir).map_err(|e| e.to_string())?;
    let log_file = dir.join("capture.log");
    // The latest known state of each window: its title changes at login.
    let windows: Arc<Mutex<HashMap<u32, GameWindow>>> = Arc::default();
    // Frames are written on one thread at the pace the disk allows: when it
    // falls behind, new frames are dropped rather than queued.
    let (tx, rx) = mpsc::sync_channel::<Frame>(2);
    let known = Arc::clone(&windows);
    let writer_log = log_file.clone();
    std::thread::Builder::new()
        .name("debug-capture-writer".into())
        .spawn(move || {
            for frame in rx {
                let window = known.lock().unwrap_or_else(|e| e.into_inner()).get(&frame.window).cloned();
                if let Some(window) = window {
                    if let Err(e) = recorder.record(&window, &frame) {
                        log_to(&writer_log, &format!("write failed: {e}"));
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;

    log_to(&log_file, "recording started");
    capture_windows(&tx, &log_file, |listed| {
        let mut known = windows.lock().unwrap_or_else(|e| e.into_inner());
        known.clear();
        known.extend(listed.iter().map(|w| (w.id, w.clone())));
    });
    Err(CaptureError::Denied.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two windows whose fights start together see one fight; later, or in
    /// one window, two.
    #[test]
    fn two_windows_starting_together_see_one_fight() {
        let mut fights = Fights::default();
        assert_eq!(fights.name(1, "1-1000", 1_000), "1-1000");
        assert_eq!(fights.name(2, "2-3000", 3_000), "1-1000");
        assert_eq!(fights.name(2, "2-30000", 30_000), "2-30000");
        assert_eq!(fights.name(2, "2-31000", 31_000), "2-31000");
        assert_eq!(fights.name(3, "3-35000", 35_000), "2-30000");
    }
}
