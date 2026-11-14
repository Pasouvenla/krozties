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

use dofus_maps::{Known, Reading, Settings, Store};
use dofus_vision::fight::{Follower, Step};
use dofus_vision::place::{place, Place};
use dofus_vision::read::board::Board;
use dofus_vision::text::{self, Region};
use dofus_vision::{active_game_window, game_windows, CaptureError, Frame, GameWindow, Recorder, Rect, Stream};
use tauri::{AppHandle, Manager};

use crate::calque;

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
/// How often the place is read on a window out of fights: the last one read
/// before a fight is its place, if the fight hides it.
const PLACE_EVERY_MS: u64 = 10_000;
/// A place read longer before a fight than this is not taken for its place.
const PLACE_FRESH_MS: u64 = 60_000;
/// How often a fight whose map is not known tries again to know it, as its
/// reading grows.
const RECOGNISE_EVERY_MS: u64 = 5_000;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Starts the combat reading on its own thread when the environment asks for
/// it, the maps kept in `data`.
pub fn start_reading_from_env(app: AppHandle, data: PathBuf) {
    if std::env::var_os(READING_ENV).is_none() {
        return;
    }
    let _ = std::thread::Builder::new().name("combat-reading".into()).spawn(move || {
        let log_file = data.join(READING_LOG);
        if let Err(e) = read(&app, &data, &log_file) {
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

/// The latest board of a fight, and how its frame lies over the window:
/// frame pixels per window point, and the frame's size in pixels.
struct Latest {
    board: Board,
    scale: f32,
    size: (f32, f32),
}

/// One game window: followed from fight to fight, and where it was last.
struct Window {
    follower: Follower,
    /// The latest place read, and when.
    place: Option<(Place, u64)>,
    place_read_ms: u64,
    /// The place of the fight going on.
    fight_place: Option<Place>,
    /// The fight's map, if the store knows it, and when it was last sought.
    known: Option<Known>,
    sought_ms: u64,
    latest: Option<Latest>,
}

impl Window {
    fn new(id: u32) -> Window {
        Window {
            follower: Follower::new(id, Settings::default()),
            place: None,
            place_read_ms: 0,
            fight_place: None,
            known: None,
            sought_ms: 0,
            latest: None,
        }
    }

    /// Seeks the fight's map among the known ones.
    fn seek(&mut self, store: &Store, now_ms: u64) {
        self.sought_ms = now_ms;
        let (Some(name), Some(view)) = (self.follower.fight(), self.follower.view()) else { return };
        let reading = Reading { fight: name.to_string(), coords: (0, 0), subarea: String::new(), cells: view.cells, at_ms: now_ms };
        self.known = store.recognise(&placed(reading, self.fight_place.as_ref()));
    }
}

/// The place written on a frame, if the text recognition finds one.
fn place_on(frame: &Frame) -> Option<Place> {
    text::lines(frame, Region::TOP_LEFT).ok().and_then(|lines| place(&lines))
}

/// A fight's place: the latest read, if read lately enough.
fn fight_place(latest: Option<&(Place, u64)>, now_ms: u64) -> Option<Place> {
    latest.filter(|(_, at)| now_ms.saturating_sub(*at) <= PLACE_FRESH_MS).map(|(p, _)| p.clone())
}

/// The reading with its place written in.
fn placed(mut reading: Reading, place: Option<&Place>) -> Reading {
    if let Some(place) = place {
        reading.coords = place.coords;
        reading.subarea = place.subarea.clone().unwrap_or_default();
    }
    reading
}

/// Follows every game window, keeps each fight's reading in the store, and
/// lays the overlay over the game window in front while the companion is
/// open.
fn read(app: &AppHandle, data: &Path, log_file: &Path) -> Result<(), String> {
    std::fs::create_dir_all(data).map_err(|e| e.to_string())?;
    let path = data.join(STORE_FILE);
    let mut store = Store::load(&path, Settings::default()).map_err(|e| e.to_string())?;
    // Frames are read on this thread at the pace reading allows: when it falls
    // behind, new frames are dropped rather than queued.
    let (tx, rx) = mpsc::sync_channel::<Frame>(2);
    let windows_log = log_file.to_path_buf();
    // The game windows as last listed: where they are on screen.
    let listed: Arc<Mutex<Vec<GameWindow>>> = Arc::default();
    let listing = Arc::clone(&listed);
    std::thread::Builder::new()
        .name("combat-reading-windows".into())
        .spawn(move || {
            capture_windows(&tx, &windows_log, |found| {
                *listing.lock().unwrap_or_else(|e| e.into_inner()) = found.to_vec();
            })
        })
        .map_err(|e| e.to_string())?;

    log_to(log_file, "reading started");
    let mut windows: HashMap<u32, Window> = HashMap::new();
    let mut fights = Fights::default();
    // Where the overlay lies, while it shows.
    let mut overlay: Option<Rect> = None;
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
        let list = listed.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(frame) = &received {
            let state = windows.entry(frame.window).or_insert_with(|| Window::new(frame.window));
            let step = state.follower.feed(frame, now);
            if let Step::Started(board) | Step::Read(board) = &step {
                let points = list.iter().find(|w| w.id == frame.window).map_or(0.0, |w| w.frame.width);
                let scale = if points > 0.0 { (f64::from(frame.width) / points) as f32 } else { 1.0 };
                state.latest = Some(Latest { board: board.clone(), scale, size: (frame.width as f32, frame.height as f32) });
            }
            // The place, on the first frame of a fight and now and then out
            // of fights.
            let started = matches!(step, Step::Started(_));
            let out_of_fights = state.follower.fight().is_none();
            if started || (out_of_fights && now.saturating_sub(state.place_read_ms) >= PLACE_EVERY_MS) {
                state.place_read_ms = now;
                if let Some(found) = place_on(frame) {
                    state.place = Some((found, now));
                }
            }
            if started {
                state.fight_place = fight_place(state.place.as_ref(), now);
            }
            steps.push((frame.window, step));
        }
        for (id, state) in windows.iter_mut().filter(|(w, _)| Some(**w) != fed) {
            steps.push((*id, state.follower.tick(now)));
        }
        for (window, step) in steps {
            match step {
                Step::Started(_) => {
                    let Some(state) = windows.get_mut(&window) else { continue };
                    let own = state.follower.fight().unwrap_or_default().to_string();
                    let name = fights.name(window, &own, now);
                    state.follower.rename_fight(&name);
                    let place = state.fight_place.as_ref().map_or("unknown place".to_string(), |p| format!("{:?}", p.coords));
                    state.seek(&store, now);
                    let map = state.known.as_ref().map_or("a new map".to_string(), |k| format!("map {}", k.map));
                    log_to(log_file, &format!("fight {name} started in window {window}, {place}, {map}"));
                }
                Step::Ended(reading) => {
                    let name = reading.fight.clone();
                    let place = windows.get_mut(&window).and_then(|w| {
                        w.known = None;
                        w.latest = None;
                        w.fight_place.take()
                    });
                    match store.add(placed(reading, place.as_ref())) {
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
        // A fight whose map is not known yet tries again as its reading grows.
        for state in windows.values_mut() {
            if state.known.is_none()
                && state.follower.fight().is_some()
                && now.saturating_sub(state.sought_ms) >= RECOGNISE_EVERY_MS
            {
                state.seek(&store, now);
            }
        }
        overlay = lay_overlay(app, &list, &windows, overlay);
    }
}

/// Lays the overlay over the game window in front, while the companion is
/// open and that window is in a fight; takes it off otherwise. Returns where
/// it lies.
fn lay_overlay(app: &AppHandle, list: &[GameWindow], windows: &HashMap<u32, Window>, at: Option<Rect>) -> Option<Rect> {
    let companion = app.get_webview_window(crate::compagnon::ETIQUETTE).is_some();
    let front = if companion { active_game_window(list) } else { None };
    let wanted = front.and_then(|id| {
        let state = windows.get(&id)?;
        let (view, latest) = (state.follower.view()?, state.latest.as_ref()?);
        let window = list.iter().find(|w| w.id == id)?;
        Some((window.frame, calque::drawing(&latest.board, &view, state.known.as_ref(), latest.scale, latest.size)))
    });
    match wanted {
        Some((frame, drawing)) => {
            calque::show(app, &frame, &drawing, at != Some(frame));
            Some(frame)
        }
        None => {
            if at.is_some() {
                calque::hide(app);
            }
            None
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

    /// A fight takes the place read last, if read lately; its reading
    /// carries it.
    #[test]
    fn a_fight_takes_the_place_read_lately() {
        let here = Place { coords: (4, -12), subarea: Some("Plaine des Scarafeuilles".into()) };
        let latest = (here.clone(), 100_000);
        assert_eq!(fight_place(Some(&latest), 130_000), Some(here.clone()));
        assert_eq!(fight_place(Some(&latest), 100_000 + PLACE_FRESH_MS + 1), None);
        assert_eq!(fight_place(None, 100_000), None);
        let reading = Reading { fight: "f".into(), coords: (0, 0), subarea: String::new(), cells: Vec::new(), at_ms: 0 };
        let reading = placed(reading, Some(&here));
        assert_eq!((reading.coords, reading.subarea.as_str()), ((4, -12), "Plaine des Scarafeuilles"));
    }

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
