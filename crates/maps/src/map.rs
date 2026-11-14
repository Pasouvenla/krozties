//! What a cell is, how a reading reports it, and how far two readings agree.

use serde::{Deserialize, Serialize};

use crate::cells::{coords, id_at, CELLS};

/// What a cell of a combat map is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cell {
    /// Part of the checkerboard: fighters stand and walk there.
    Walkable,
    /// Not walkable, and blocks the line of sight: a wall, a pillar, a tree.
    Obstacle,
    /// Not walkable, but the line of sight goes over it: a hole, water.
    Hole,
    /// Not walkable; whether it blocks the line of sight is not known yet.
    NotWalkable,
    /// Out of view, covered, or not decided.
    Unknown,
}

impl Cell {
    pub(crate) const KNOWN: [Cell; 4] = [Cell::Walkable, Cell::Obstacle, Cell::Hole, Cell::NotWalkable];

    pub(crate) fn rank(self) -> Option<usize> {
        Cell::KNOWN.iter().position(|c| *c == self)
    }

    /// Whether a fighter can stand there, if known.
    pub fn walkable(self) -> Option<bool> {
        match self {
            Cell::Walkable => Some(true),
            Cell::Obstacle | Cell::Hole | Cell::NotWalkable => Some(false),
            Cell::Unknown => None,
        }
    }

    /// Whether two readings of a cell can be the same cell: as walkable, and,
    /// when both say what blocks it, as the same.
    pub(crate) fn agrees(self, other: Cell) -> bool {
        match (self, other) {
            (Cell::Obstacle, Cell::Hole) | (Cell::Hole, Cell::Obstacle) => false,
            _ => self.walkable().is_some() && self.walkable() == other.walkable(),
        }
    }

    fn letter(self) -> char {
        match self {
            Cell::Walkable => '.',
            Cell::Obstacle => '#',
            Cell::Hole => 'o',
            Cell::NotWalkable => 'x',
            Cell::Unknown => '?',
        }
    }

    fn from_letter(c: char) -> Option<Cell> {
        [Cell::Walkable, Cell::Obstacle, Cell::Hole, Cell::NotWalkable, Cell::Unknown].into_iter().find(|k| k.letter() == c)
    }
}

/// A cell as one reading saw it, and how sure the reader was, from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub cell: Cell,
    pub confidence: f32,
}

impl Seen {
    pub const UNKNOWN: Seen = Seen { cell: Cell::Unknown, confidence: 0.0 };

    pub fn new(cell: Cell, confidence: f32) -> Seen {
        Seen { cell, confidence: confidence.clamp(0.0, 1.0) }
    }
}

/// One fight's reading of a map: where it was and what each cell is, by game
/// cell id. Nothing else is kept: no image, no character, no chat.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(into = "Wire", try_from = "Wire")]
pub struct Reading {
    /// Shared by the readings of one fight seen through several windows.
    pub fight: String,
    /// The map coordinates the game displays.
    pub coords: (i32, i32),
    /// The sub-area name the game displays.
    pub subarea: String,
    /// One per cell, [`CELLS`] long.
    pub cells: Vec<Seen>,
    pub at_ms: u64,
}

impl Reading {
    pub(crate) fn is_complete(&self) -> bool {
        self.cells.len() == CELLS
    }

    /// The same fight read twice, through two windows: each cell kept as the
    /// surer of the two saw it.
    pub(crate) fn merge(&mut self, other: &Reading) {
        for (mine, theirs) in self.cells.iter_mut().zip(&other.cells) {
            if mine.cell == Cell::Unknown || (theirs.cell != Cell::Unknown && theirs.confidence > mine.confidence) {
                *mine = *theirs;
            }
        }
        self.at_ms = self.at_ms.max(other.at_ms);
    }
}

/// How far two sets of cells agree: over the cells known on both sides and
/// walkable on one side at least, the ones the same on both, each weighed by
/// the weaker of its two confidences, so that an unsure cell barely counts.
/// What two maps share most is the void around their floors; what tells them
/// apart is the shape of the floor, its edges and the obstacles within.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Agreement {
    /// The weight of the cells that are the same.
    pub same: f32,
    /// The weight of all the cells compared.
    pub total: f32,
    /// The number of cells known on both sides.
    pub overlap: usize,
}

impl Agreement {
    pub fn of(a: &[Seen], b: &[Seen]) -> Agreement {
        let mut agreement = Agreement::default();
        for (x, y) in a.iter().zip(b) {
            if x.cell == Cell::Unknown || y.cell == Cell::Unknown {
                continue;
            }
            agreement.overlap += 1;
            if x.cell != Cell::Walkable && y.cell != Cell::Walkable && x.cell.agrees(y.cell) {
                continue;
            }
            let weight = x.confidence.min(y.confidence);
            agreement.total += weight;
            if x.cell.agrees(y.cell) {
                agreement.same += weight;
            }
        }
        agreement
    }

    /// The share of the weight that agrees.
    pub fn share(&self) -> f32 {
        if self.total > 0.0 {
            self.same / self.total
        } else {
            0.0
        }
    }
}

/// The share of agreeing cells, and the number of cells known on both sides.
#[cfg(test)]
fn similarity(a: &[Seen], b: &[Seen]) -> (f32, usize) {
    let agreement = Agreement::of(a, b);
    (agreement.share(), agreement.overlap)
}

/// The cells moved by a whole translation of the game's coordinates: cell
/// (x, y) becomes cell (x + dx, y + dy). The cells that leave the map are
/// dropped, those that enter it unknown; `None` when a walkable cell would
/// leave: the floor lies within the map.
pub(crate) fn shifted(cells: &[Seen], (dx, dy): (i32, i32)) -> Option<Vec<Seen>> {
    let mut moved = vec![Seen::UNKNOWN; CELLS];
    for (id, seen) in cells.iter().enumerate() {
        if seen.cell == Cell::Unknown {
            continue;
        }
        let (x, y) = coords(u16::try_from(id).ok()?)?;
        match id_at(x + dx, y + dy) {
            Some(to) => moved[usize::from(to)] = *seen,
            None if seen.cell == Cell::Walkable => return None,
            None => {}
        }
    }
    Some(moved)
}

/// The file form of a reading: the cells as one letter each, the
/// confidences in hundredths.
#[derive(Serialize, Deserialize)]
struct Wire {
    fight: String,
    coords: (i32, i32),
    subarea: String,
    cells: String,
    confidence: Vec<u8>,
    at_ms: u64,
}

impl From<Reading> for Wire {
    fn from(r: Reading) -> Wire {
        Wire {
            fight: r.fight,
            coords: r.coords,
            subarea: r.subarea,
            cells: r.cells.iter().map(|s| s.cell.letter()).collect(),
            confidence: r.cells.iter().map(|s| (s.confidence * 100.0).round() as u8).collect(),
            at_ms: r.at_ms,
        }
    }
}

impl TryFrom<Wire> for Reading {
    type Error = String;

    fn try_from(w: Wire) -> Result<Reading, String> {
        if w.cells.chars().count() != w.confidence.len() {
            return Err(format!("reading {}: {} cells for {} confidences", w.fight, w.cells.len(), w.confidence.len()));
        }
        let cells = w
            .cells
            .chars()
            .zip(&w.confidence)
            .map(|(c, p)| {
                Cell::from_letter(c)
                    .map(|cell| Seen::new(cell, f32::from(*p) / 100.0))
                    .ok_or_else(|| format!("reading {}: unknown cell {c:?}", w.fight))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Reading { fight: w.fight, coords: w.coords, subarea: w.subarea, cells, at_ms: w.at_ms })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(kinds: &str, confidence: f32) -> Vec<Seen> {
        kinds.chars().map(|c| Seen::new(Cell::from_letter(c).unwrap(), confidence)).collect()
    }

    /// The floor is compared, cell by cell; two obstacles or two holes say
    /// nothing, an obstacle against a hole tells the maps apart.
    #[test]
    fn the_floor_is_compared_and_obstacles_against_holes() {
        assert_eq!(similarity(&cells("..xx", 1.0), &cells(".xxx", 1.0)), (0.5, 4));
        assert_eq!(similarity(&cells("..x#o", 1.0), &cells("..ox#", 1.0)), (2.0 / 3.0, 5));
        assert_eq!(similarity(&cells("xx#", 1.0), &cells("xo#", 1.0)), (0.0, 3));
    }

    #[test]
    fn unknown_cells_do_not_count_and_unsure_ones_barely() {
        assert_eq!(similarity(&cells("..#o", 1.0), &cells("..#o", 1.0)), (1.0, 4));
        assert_eq!(similarity(&cells("..#?", 1.0), &cells("..#o", 1.0)), (1.0, 3));
        let (share, overlap) = similarity(&cells("....", 1.0), &cells("...#", 1.0));
        assert_eq!((share, overlap), (0.75, 4));
        let mut unsure = cells("...#", 1.0);
        unsure[3].confidence = 0.1;
        assert!(similarity(&cells("....", 1.0), &unsure).0 > 0.96);
        assert_eq!(similarity(&cells("??", 1.0), &cells("..", 1.0)), (0.0, 0));
    }

    #[test]
    fn a_shift_moves_the_known_cells_and_keeps_them_on_the_map() {
        let mut cells = vec![Seen::UNKNOWN; CELLS];
        let at = |x, y| usize::from(id_at(x, y).unwrap());
        cells[at(10, 0)] = Seen::new(Cell::Walkable, 0.9);
        cells[at(11, 1)] = Seen::new(Cell::Hole, 0.3);
        let moved = shifted(&cells, (2, -3)).unwrap();
        assert_eq!(moved[at(12, -3)], cells[at(10, 0)]);
        assert_eq!(moved[at(13, -2)], cells[at(11, 1)]);
        assert_eq!(moved.iter().filter(|s| s.cell != Cell::Unknown).count(), 2);
        assert_eq!(shifted(&cells, (0, 0)).unwrap(), cells);
        // Cell (0, 0) is the map's first: nothing lies left of it. A cell
        // not walkable leaves the map, the floor cannot.
        cells[at(0, 0)] = Seen::new(Cell::NotWalkable, 0.5);
        assert_eq!(shifted(&cells, (-1, 0)).unwrap().iter().filter(|s| s.cell != Cell::Unknown).count(), 2);
        cells[at(0, 0)] = Seen::new(Cell::Walkable, 0.9);
        assert!(shifted(&cells, (-1, 0)).is_none());
    }

    #[test]
    fn two_windows_of_a_fight_keep_the_surer_cell() {
        let mut a = Reading { fight: "f".into(), coords: (1, 2), subarea: "s".into(), cells: cells(".?#", 0.5), at_ms: 1 };
        let b = Reading { cells: vec![Seen::new(Cell::Hole, 0.9), Seen::new(Cell::Walkable, 0.4), Seen::new(Cell::Walkable, 0.2)], at_ms: 3, ..a.clone() };
        a.merge(&b);
        assert_eq!(a.cells, vec![Seen::new(Cell::Hole, 0.9), Seen::new(Cell::Walkable, 0.4), Seen::new(Cell::Obstacle, 0.5)]);
        assert_eq!(a.at_ms, 3);
    }

    #[test]
    fn a_reading_survives_its_file_form() {
        let r = Reading { fight: "f".into(), coords: (-3, 7), subarea: "Île de Moon".into(), cells: cells(".#ox?", 0.83), at_ms: 9 };
        let back: Reading = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back, r);
        assert!(serde_json::from_str::<Reading>(r#"{"fight":"f","coords":[0,0],"subarea":"","cells":".z","confidence":[1,2],"at_ms":0}"#).is_err());
    }
}
