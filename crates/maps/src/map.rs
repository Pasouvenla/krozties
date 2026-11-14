//! What a cell is, how a reading reports it, and how far two readings agree.

use serde::{Deserialize, Serialize};

use crate::cells::CELLS;

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
    /// Covered by the interface, or not decided.
    Unknown,
}

impl Cell {
    pub(crate) const KNOWN: [Cell; 3] = [Cell::Walkable, Cell::Obstacle, Cell::Hole];

    pub(crate) fn rank(self) -> Option<usize> {
        Cell::KNOWN.iter().position(|c| *c == self)
    }

    fn letter(self) -> char {
        match self {
            Cell::Walkable => '.',
            Cell::Obstacle => '#',
            Cell::Hole => 'o',
            Cell::Unknown => '?',
        }
    }

    fn from_letter(c: char) -> Option<Cell> {
        [Cell::Walkable, Cell::Obstacle, Cell::Hole, Cell::Unknown].into_iter().find(|k| k.letter() == c)
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

/// How far two sets of cells agree: over the cells known on both sides, the
/// share that are the same, each cell weighed by the weaker of its two
/// confidences, so that an unsure cell barely counts. Returns the share and
/// the number of cells known on both sides.
pub(crate) fn similarity(a: &[Seen], b: &[Seen]) -> (f32, usize) {
    let (mut same, mut total, mut overlap) = (0.0f32, 0.0f32, 0usize);
    for (x, y) in a.iter().zip(b) {
        if x.cell == Cell::Unknown || y.cell == Cell::Unknown {
            continue;
        }
        overlap += 1;
        let weight = x.confidence.min(y.confidence);
        total += weight;
        if x.cell == y.cell {
            same += weight;
        }
    }
    (if total > 0.0 { same / total } else { 0.0 }, overlap)
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
    fn two_windows_of_a_fight_keep_the_surer_cell() {
        let mut a = Reading { fight: "f".into(), coords: (1, 2), subarea: "s".into(), cells: cells(".?#", 0.5), at_ms: 1 };
        let b = Reading { cells: vec![Seen::new(Cell::Hole, 0.9), Seen::new(Cell::Walkable, 0.4), Seen::new(Cell::Walkable, 0.2)], at_ms: 3, ..a.clone() };
        a.merge(&b);
        assert_eq!(a.cells, vec![Seen::new(Cell::Hole, 0.9), Seen::new(Cell::Walkable, 0.4), Seen::new(Cell::Obstacle, 0.5)]);
        assert_eq!(a.at_ms, 3);
    }

    #[test]
    fn a_reading_survives_its_file_form() {
        let r = Reading { fight: "f".into(), coords: (-3, 7), subarea: "Île de Moon".into(), cells: cells(".#o?", 0.83), at_ms: 9 };
        let back: Reading = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back, r);
        assert!(serde_json::from_str::<Reading>(r#"{"fight":"f","coords":[0,0],"subarea":"","cells":".x","confidence":[1,2],"at_ms":0}"#).is_err());
    }
}
