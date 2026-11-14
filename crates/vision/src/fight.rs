//! Following a game window from fight to fight.
//!
//! A fight shows the checkerboard; outside fights there is none. A fight
//! starts once the checkerboard has shown for a moment, so that a frame where
//! decor looks like one starts nothing, and ends once it has not shown for a
//! few seconds, so that an animation hiding the floor ends nothing. Time
//! counts, not frames: the capture delivers a frame only when the window
//! changes, and a still screen goes on showing what it showed, which the
//! caller tells by [`Follower::tick`].
//!
//! The frames of a fight are gathered into one reading by vote, laid over
//! each other: the board may anchor two frames differently. The camera does
//! not move during a fight: each frame is first checked against the lattice
//! already found, a fraction of the cost of a search.

use dofus_maps::{Gathering, Reading, Seen, Settings};

use crate::read::board::{self, Board};
use crate::read::lattice::{self, Lattice};
use crate::Frame;

/// How long the checkerboard shows before a fight is said to start.
const START_MS: u64 = 2_000;
/// How long it does not before the fight is said to end.
const END_MS: u64 = 4_000;
/// Out of fights, how often a frame is searched for a checkerboard: a search
/// costs a few hundred milliseconds, and a fight's placement lasts longer
/// than this.
const IDLE_SEARCH_MS: u64 = 3_000;

/// What a frame, or the time passing, changed.
#[derive(Debug)]
pub enum Step {
    /// Out of a fight, or nothing new.
    Nothing,
    /// A fight started; its board on the latest frame.
    Started(Board),
    /// A frame of the fight was read.
    Read(Board),
    /// The fight ended: what its frames read of the map, its place left for
    /// the caller to fill.
    Ended(Reading),
}

/// What the fight going on has read so far, and how its latest board lies
/// over it: cell (x, y) of the board is cell (x + dx, y + dy) of `cells`.
#[derive(Clone, Debug)]
pub struct View {
    pub cells: Vec<Seen>,
    pub shift: (i32, i32),
}

/// Before a fight starts: the frames that showed a board since `since`.
struct Pending {
    since: u64,
    gathering: Gathering,
    board: Board,
    shift: (i32, i32),
}

/// The fight going on.
struct Fight {
    name: String,
    gathering: Gathering,
    /// How the latest board lies over the gathering.
    shift: (i32, i32),
    /// Since when the window shows no checkerboard, if it does not.
    without_since: Option<u64>,
    last_ms: u64,
}

/// One game window followed from fight to fight.
pub struct Follower {
    window: u32,
    settings: Settings,
    lattice: Option<Lattice>,
    pending: Option<Pending>,
    fight: Option<Fight>,
    /// Full searches of the checkerboard made so far, and when the last was.
    searches: usize,
    searched_ms: Option<u64>,
}

impl Follower {
    pub fn new(window: u32, settings: Settings) -> Follower {
        Follower { window, settings, lattice: None, pending: None, fight: None, searches: 0, searched_ms: None }
    }

    /// The name of the fight going on, if any.
    pub fn fight(&self) -> Option<&str> {
        self.fight.as_ref().map(|f| f.name.as_str())
    }

    /// Names the fight going on, when another window sees the same fight.
    pub fn rename_fight(&mut self, name: &str) {
        if let Some(fight) = self.fight.as_mut() {
            fight.name = name.to_string();
        }
    }

    /// Full searches of the checkerboard made so far.
    pub fn searches(&self) -> usize {
        self.searches
    }

    /// What the fight going on has read so far, if a fight goes on.
    pub fn view(&self) -> Option<View> {
        self.fight.as_ref().map(|f| View { cells: f.gathering.cells(), shift: f.shift })
    }

    /// Reads a frame taken at `at_ms`.
    pub fn feed(&mut self, frame: &Frame, at_ms: u64) -> Step {
        // Out of fights, with nothing in view, a frame is searched now and then.
        let idle = self.fight.is_none() && self.pending.is_none();
        if idle && self.searched_ms.is_some_and(|at| at_ms.saturating_sub(at) < IDLE_SEARCH_MS) {
            return Step::Nothing;
        }
        let board = self.board(frame, at_ms);
        match (self.fight.as_mut(), board) {
            (Some(fight), Some(board)) => {
                fight.without_since = None;
                fight.last_ms = at_ms;
                // A frame no shift lays over the others is left out.
                if let Some(shift) = fight.gathering.add(&board.cells, &self.settings) {
                    fight.shift = shift;
                }
                return Step::Read(board);
            }
            (Some(fight), None) => {
                fight.without_since.get_or_insert(at_ms);
            }
            (None, Some(board)) => {
                let laid = self.pending.as_mut().and_then(|p| p.gathering.add(&board.cells, &self.settings));
                match (self.pending.as_mut(), laid) {
                    (Some(pending), Some(shift)) => {
                        pending.board = board;
                        pending.shift = shift;
                    }
                    _ => {
                        // The first frame, or another map than the frames before.
                        let mut gathering = Gathering::default();
                        gathering.add(&board.cells, &self.settings);
                        self.pending = Some(Pending { since: at_ms, gathering, board, shift: (0, 0) });
                    }
                }
            }
            (None, None) => self.pending = None,
        }
        self.tick(at_ms)
    }

    /// The window still shows its latest frame at `now_ms`: a fight starts or
    /// ends when it has shown, or not shown, the checkerboard long enough.
    pub fn tick(&mut self, now_ms: u64) -> Step {
        if let Some(fight) = &self.fight {
            if fight.without_since.is_some_and(|since| now_ms.saturating_sub(since) >= END_MS) {
                let fight = self.fight.take().expect("a fight going on");
                self.lattice = None;
                return Step::Ended(Reading {
                    fight: fight.name,
                    coords: (0, 0),
                    subarea: String::new(),
                    cells: fight.gathering.cells(),
                    at_ms: fight.last_ms,
                });
            }
            return Step::Nothing;
        }
        match &self.pending {
            Some(pending) if now_ms.saturating_sub(pending.since) >= START_MS => {
                let pending = self.pending.take().expect("frames pending");
                let name = format!("{}-{}", self.window, pending.since);
                let board = pending.board.clone();
                self.fight = Some(Fight {
                    name,
                    gathering: pending.gathering,
                    shift: pending.shift,
                    without_since: None,
                    last_ms: now_ms,
                });
                Step::Started(board)
            }
            _ => Step::Nothing,
        }
    }

    /// The board of a frame: on the lattice already found if the frame still
    /// shows it, more loosely in a fight, else on a new search.
    fn board(&mut self, frame: &Frame, at_ms: u64) -> Option<Board> {
        let held = self.fight.is_some();
        let confirmed = self.lattice.and_then(|known| lattice::confirm(frame, &known, held));
        let found = confirmed.or_else(|| {
            self.searches += 1;
            self.searched_ms = Some(at_ms);
            lattice::find(frame)
        });
        // A fight keeps its checkerboard while a frame hides it: the next
        // frame is checked at the same place.
        self.lattice = if held { found.or(self.lattice) } else { found };
        board::read(frame, &found?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{boss_maps, render, Scene};
    use dofus_maps::{cells, Cell, Seen};

    /// A frame of `map` with fighters on the walkable cells `turn` picks; the
    /// map's colours do not change from frame to frame, as in a fight.
    fn frame_of(map: &[Cell], turn: u64) -> Frame {
        let mut scene = Scene::centred(map.to_vec(), (1280, 800), 64.0, 300);
        let floor: Vec<u16> = (0..cells::CELLS as u16).filter(|&c| map[usize::from(c)] == Cell::Walkable).collect();
        scene.entities = (0..4).map(|k| floor[(k * 53 + turn as usize * 7) % floor.len()]).collect();
        scene.obstacle_height = 0.3;
        render(&scene)
    }

    /// A frame out of fights: the void and the interface.
    fn empty(turn: u64) -> Frame {
        render(&Scene::centred(vec![Cell::Hole; cells::CELLS], (1280, 800), 64.0, 900 + turn))
    }

    /// Whether a reading is `map`, laid over it as the store would.
    fn is_map(reading: &Reading, map: &[Cell]) -> bool {
        let truth: Vec<Seen> = map
            .iter()
            .map(|c| if *c == Cell::Walkable { Seen::new(Cell::Walkable, 0.9) } else { Seen::new(Cell::NotWalkable, 0.7) })
            .collect();
        let mut gathering = Gathering::default();
        gathering.add(&truth, &Settings::default());
        gathering.add(&reading.cells, &Settings::default()).is_some()
    }

    fn kind(step: &Step) -> &'static str {
        match step {
            Step::Nothing => "nothing",
            Step::Started(_) => "started",
            Step::Read(_) => "read",
            Step::Ended(_) => "ended",
        }
    }

    /// Out of a fight, into one, its frames read, and out: the fight is said
    /// to start on its third frame and to end on the fifth without, and its
    /// reading is the map. Frames of the fight are checked on the lattice
    /// found, not searched again.
    #[test]
    fn a_fight_is_followed_from_start_to_end() {
        let map = boss_maps().swap_remove(6);
        let mut follower = Follower::new(7, Settings::default());
        let mut steps = Vec::new();
        let mut ended = None;
        let frames = (0..2).map(empty).chain((0..8).map(|t| frame_of(&map, t))).chain((10..15).map(empty));
        for (t, frame) in frames.enumerate() {
            let step = follower.feed(&frame, 1000 * t as u64);
            steps.push(kind(&step));
            if let Step::Ended(reading) = step {
                ended = Some(reading);
            }
        }
        // Out of fights a frame is searched every three seconds: the board is
        // first seen at 3 s, the fight starts 2 s later.
        let expected = ["nothing", "nothing", "nothing", "nothing", "nothing", "started", "read", "read", "read", "read"];
        assert_eq!(steps[..10], expected);
        assert_eq!(steps[10..], ["nothing", "nothing", "nothing", "nothing", "ended"]);
        let reading = ended.expect("the fight's reading");
        assert_eq!((reading.fight.as_str(), reading.at_ms), ("7-3000", 9000));
        assert!(is_map(&reading, &map), "the reading is not the map");
        assert!(follower.searches() <= 2 + 1 + 5, "{} searches", follower.searches());
        assert_eq!(follower.fight(), None);
    }

    /// The capture sends no frame while the window does not change: a still
    /// screen starts a fight and ends it as time passes.
    #[test]
    fn a_still_screen_counts_by_the_clock() {
        let map = boss_maps().swap_remove(14);
        let mut follower = Follower::new(2, Settings::default());
        assert_eq!(kind(&follower.feed(&frame_of(&map, 0), 10_000)), "nothing");
        assert_eq!(kind(&follower.tick(11_000)), "nothing");
        assert_eq!(kind(&follower.tick(12_000)), "started");
        assert_eq!(kind(&follower.tick(30_000)), "nothing", "a still fight goes on");
        assert_eq!(kind(&follower.feed(&empty(1), 31_000)), "nothing");
        assert_eq!(kind(&follower.tick(34_000)), "nothing");
        assert_eq!(kind(&follower.tick(35_000)), "ended");
    }

    /// The view of a fight lays each board over what the fight read: a cell
    /// of the latest board, shifted, is the same cell of the view.
    #[test]
    fn the_view_lays_the_latest_board_over_the_fight() {
        let map = boss_maps().swap_remove(13);
        let mut follower = Follower::new(1, Settings::default());
        assert!(follower.view().is_none());
        let mut latest = None;
        for t in 0..5 {
            if let Step::Started(board) | Step::Read(board) = follower.feed(&frame_of(&map, t), 1000 * t) {
                latest = Some(board);
            }
        }
        let (board, view) = (latest.expect("a board"), follower.view().expect("a fight"));
        let mut checked = 0;
        for id in 0..cells::CELLS as u16 {
            if board.cells[usize::from(id)].cell != Cell::Walkable {
                continue;
            }
            let (x, y) = cells::coords(id).unwrap();
            let there = cells::id_at(x + view.shift.0, y + view.shift.1).expect("on the map");
            assert_eq!(view.cells[usize::from(there)].cell, Cell::Walkable, "cell {id}");
            checked += 1;
        }
        assert!(checked > 100, "{checked} cells checked");
    }

    /// Another window seeing the same fight names it alike.
    #[test]
    fn a_fight_can_take_another_window_s_name() {
        let map = boss_maps().swap_remove(5);
        let mut follower = Follower::new(9, Settings::default());
        for t in 0..3 {
            follower.feed(&frame_of(&map, t), 1000 * t);
        }
        assert_eq!(follower.fight(), Some("9-0"));
        follower.rename_fight("4-0");
        let ended = (3..8).find_map(|t| match follower.feed(&empty(t), 1000 * t) {
            Step::Ended(reading) => Some(reading),
            _ => None,
        });
        assert_eq!(ended.expect("the fight's reading").fight, "4-0");
    }

    /// One frame that shows a checkerboard does not start a fight; a frame
    /// where the floor does not show, in the middle of a fight, does not end
    /// it.
    #[test]
    fn a_single_frame_neither_starts_nor_ends_a_fight() {
        let map = boss_maps().swap_remove(11);
        let mut follower = Follower::new(1, Settings::default());
        let frames = [frame_of(&map, 0)]
            .into_iter()
            .chain((1..5).map(empty))
            .chain((5..11).map(|t| frame_of(&map, t)))
            .chain([empty(11)])
            .chain((12..18).map(|t| frame_of(&map, t)));
        let steps: Vec<&str> = frames.enumerate().map(|(t, f)| kind(&follower.feed(&f, 1000 * t as u64))).collect();
        assert!(steps[..5].iter().all(|s| *s == "nothing"), "{steps:?}");
        assert_eq!(steps.iter().filter(|s| **s == "started").count(), 1, "{steps:?}");
        assert!(!steps.contains(&"ended"), "{steps:?}");
        assert!(steps[12..].iter().all(|s| *s == "read"), "{steps:?}");
        assert!(follower.fight().is_some());
    }

    /// Frames of another map before the fight is said to start: the count
    /// starts over with them.
    #[test]
    fn another_map_before_the_start_starts_the_count_over() {
        let maps = boss_maps();
        let (a, b) = (&maps[8], &maps[19]);
        let mut follower = Follower::new(5, Settings::default());
        let frames = [frame_of(a, 0), frame_of(a, 1)].into_iter().chain((2..5).map(|t| frame_of(b, t)));
        let mut steps = Vec::new();
        for (t, frame) in frames.enumerate() {
            steps.push(kind(&follower.feed(&frame, 1000 * t as u64)));
        }
        assert_eq!(steps, ["nothing", "nothing", "nothing", "nothing", "started"]);
        let ended = (5..10).map(empty).enumerate().find_map(|(t, f)| match follower.feed(&f, 1000 * (5 + t as u64)) {
            Step::Ended(reading) => Some(reading),
            _ => None,
        });
        assert!(is_map(&ended.expect("the fight's reading"), b));
    }

    /// A fight on the most broken boss map goes on as fighters crowd it,
    /// though a search would not find its checkerboard any more, and after a
    /// frame where the floor does not show.
    #[test]
    fn a_crowded_fight_goes_on() {
        let map = boss_maps().swap_remove(15);
        let floor: Vec<u16> = (0..cells::CELLS as u16).filter(|&c| map[usize::from(c)] == Cell::Walkable).collect();
        // One palette for the whole fight, fighters coming in.
        let frame = |turn: u64, every: Option<usize>| {
            let mut scene = Scene::centred(map.clone(), (1280, 800), 64.0, 1615);
            if let Some(every) = every {
                scene.entities = floor.iter().skip(turn as usize % 3).step_by(every).copied().collect();
            }
            render(&scene)
        };
        let mut follower = Follower::new(1, Settings::default());
        let mut steps = Vec::new();
        for t in 0..4 {
            steps.push(kind(&follower.feed(&frame(t, None), 1000 * t)));
        }
        assert_eq!(steps.last(), Some(&"read"), "{steps:?}");
        let crowded = frame(9, Some(12));
        assert_eq!(lattice::find(&crowded), None, "a search finds this floor: the test proves nothing");
        for t in 4..14 {
            // A frame where the floor does not show at all, then the crowd
            // again: the checkerboard is kept at its place.
            if t == 8 {
                assert_eq!(kind(&follower.feed(&empty(t), 1000 * t)), "nothing");
                continue;
            }
            let step = follower.feed(&frame(t, Some(12)), 1000 * t);
            assert!(matches!(step, Step::Read(_)), "frame {t}: {}", kind(&step));
        }
    }

    /// Out of fights, a frame is searched every few seconds only.
    #[test]
    fn out_of_fights_a_frame_is_searched_now_and_then() {
        let mut follower = Follower::new(1, Settings::default());
        for t in 0..10 {
            follower.feed(&empty(t), 1000 * t);
        }
        assert_eq!(follower.searches(), 4, "searched at 0, 3, 6 and 9 seconds");
    }

    /// Two fights in a row on two maps are two readings, each its map.
    #[test]
    fn two_fights_in_a_row_are_two_readings() {
        let maps = boss_maps();
        let (a, b) = (&maps[2], &maps[26]);
        let mut follower = Follower::new(3, Settings::default());
        let frames = (0..4)
            .map(|t| frame_of(a, t))
            .chain((4..9).map(empty))
            .chain((9..17).map(|t| frame_of(b, t)))
            .chain((17..22).map(empty));
        let readings: Vec<Reading> = frames
            .enumerate()
            .filter_map(|(t, f)| match follower.feed(&f, 1000 * t as u64) {
                Step::Ended(reading) => Some(reading),
                _ => None,
            })
            .collect();
        assert_eq!(readings.len(), 2);
        assert!(is_map(&readings[0], a) && is_map(&readings[1], b));
        assert!(!is_map(&readings[0], b) && !is_map(&readings[1], a));
        assert_ne!(readings[0].fight, readings[1].fight);
    }
}
