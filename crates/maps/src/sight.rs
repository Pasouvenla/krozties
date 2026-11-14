//! Which cells that are not walkable block the line of sight, told by the
//! range the game shows when a spell is selected.
//!
//! The game marks the cells a spell reaches: those it can be cast on, and
//! those in range but out of sight. A cell in sight proves that nothing on
//! the way blocks: the cells not walkable that the line crosses are holes. A
//! cell out of sight proves that something on the way does: a body, an
//! obstacle; when a single cell on the way may be it, that cell is an
//! obstacle. A line that crosses a cell not known proves nothing.
//!
//! The line is Krozties' own (`dofus_grid::vue`), from centre to centre,
//! leaving out the cells it only touches at a corner. Where it parts from the
//! game's, the readings of further fights outvote the odd conclusion.

use std::collections::HashSet;

use dofus_grid::vue::traversees;

use crate::cells::{case, id_of, CELLS};
use crate::map::{Cell, Seen};

/// What the game showed of a spell's range, by game cell.
#[derive(Clone, Debug, Default)]
pub struct RangeView {
    /// The cell of the fighter whose spell is selected.
    pub caster: u16,
    /// The cells the spell can be cast on.
    pub in_sight: Vec<u16>,
    /// The cells in range that the spell cannot be cast on, for want of sight.
    pub out_of_sight: Vec<u16>,
    /// The cells fighters stand on: a body blocks the sight between others.
    pub bodies: Vec<u16>,
}

/// How surely the line of sight tells a hole from an obstacle.
const SURE: f32 = 0.8;

/// What views taught.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Learned {
    pub holes: usize,
    pub obstacles: usize,
    /// Lines the cells contradict: an obstacle or a body on a line in sight,
    /// nothing that could block on a line out of sight. The cells or the view
    /// were misread, or the game's line parts from Krozties'.
    pub conflicts: usize,
}

/// The cells a line from `from` to `to` crosses, both left out; `None` if one
/// is off the map.
fn crossed(from: u16, to: u16) -> Option<Vec<u16>> {
    traversees(case(from)?, case(to)?).into_iter().map(id_of).collect()
}

/// Learns from the views which cells not walkable are holes and which are
/// obstacles, and writes it in `cells`, by game cell.
pub fn infer(cells: &mut [Seen], views: &[RangeView]) -> Learned {
    let mut learned = Learned::default();
    if cells.len() != CELLS {
        return learned;
    }
    // Each line, whether it was in sight, and the bodies of its view.
    let mut lines: Vec<(Vec<usize>, bool, HashSet<usize>)> = Vec::new();
    for view in views {
        let bodies: HashSet<usize> = view.bodies.iter().map(|&b| usize::from(b)).collect();
        let targets = view.in_sight.iter().map(|&t| (t, true)).chain(view.out_of_sight.iter().map(|&t| (t, false)));
        for (target, in_sight) in targets {
            if let Some(line) = crossed(view.caster, target) {
                lines.push((line.into_iter().map(usize::from).collect(), in_sight, bodies.clone()));
            }
        }
    }

    // A line in sight: whatever it crosses lets the sight through.
    for (line, _, bodies) in lines.iter().filter(|l| l.1) {
        for &c in line {
            if cells[c].cell == Cell::NotWalkable && !bodies.contains(&c) {
                cells[c] = Seen::new(Cell::Hole, SURE.max(cells[c].confidence));
                learned.holes += 1;
            }
        }
    }
    // A line out of sight with one cell only that may block: that cell does.
    // An obstacle found explains the other lines it lies on, and leaves their
    // other cells as they were: one pass learns all there is, once the holes
    // are known.
    for (line, _, bodies) in lines.iter().filter(|l| !l.1) {
        let explained = line.iter().any(|c| bodies.contains(c) || cells[*c].cell == Cell::Obstacle);
        if explained || line.iter().any(|c| cells[*c].cell == Cell::Unknown) {
            continue;
        }
        let mut may_block = line.iter().filter(|c| cells[**c].cell == Cell::NotWalkable);
        if let (Some(&c), None) = (may_block.next(), may_block.next()) {
            cells[c] = Seen::new(Cell::Obstacle, SURE.max(cells[c].confidence));
            learned.obstacles += 1;
        }
    }
    learned.conflicts = lines
        .iter()
        .filter(|(line, in_sight, bodies)| {
            let blocks = |c: &usize| bodies.contains(c) || cells[*c].cell == Cell::Obstacle;
            let may_block = |c: &usize| blocks(c) || matches!(cells[*c].cell, Cell::NotWalkable | Cell::Unknown);
            if *in_sight {
                line.iter().any(blocks)
            } else {
                !line.iter().any(may_block)
            }
        })
        .count();
    learned
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cells::coords;

    /// The 35 boss maps of the KrozTools, by game cell.
    fn boss_maps() -> Vec<Vec<Cell>> {
        #[derive(serde::Deserialize)]
        struct Survey {
            cartes: Vec<BossMap>,
        }
        #[derive(serde::Deserialize)]
        struct BossMap {
            lignes: Vec<String>,
        }
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/cartes/insightroom.json");
        let survey: Survey = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        survey
            .cartes
            .iter()
            .map(|map| {
                (0..CELLS as u16)
                    .map(|id| {
                        let (x, y) = coords(id).unwrap();
                        match map.lignes[(13 - y) as usize].chars().nth(x as usize) {
                            Some('.') => Cell::Walkable,
                            Some('#') => Cell::Obstacle,
                            _ => Cell::Hole,
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// A small deterministic generator.
    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }
    }

    /// What the game would show of a spell of `range` cast from `caster`,
    /// with fighters on `bodies`, on the true map.
    fn view_of(map: &[Cell], caster: u16, range: (u16, u16), bodies: &[u16]) -> RangeView {
        let from = case(caster).unwrap();
        let mut view = RangeView { caster, bodies: bodies.to_vec(), ..RangeView::default() };
        for target in (0..CELLS as u16).filter(|&t| t != caster && map[usize::from(t)] == Cell::Walkable) {
            let distance = from.distance(case(target).unwrap());
            if distance < range.0 || distance > range.1 {
                continue;
            }
            let blocks = |c: dofus_grid::Case| {
                let id = id_of(c).unwrap();
                map[usize::from(id)] == Cell::Obstacle || bodies.contains(&id)
            };
            if dofus_grid::vue::en_vue(from, case(target).unwrap(), blocks) {
                view.in_sight.push(target);
            } else {
                view.out_of_sight.push(target);
            }
        }
        view
    }

    /// The map as the board reads it: the floor, and the rest not walkable.
    fn as_read(map: &[Cell]) -> Vec<Seen> {
        map.iter()
            .map(|c| if *c == Cell::Walkable { Seen::new(Cell::Walkable, 0.9) } else { Seen::new(Cell::NotWalkable, 0.7) })
            .collect()
    }

    /// Over the boss maps, `count` spells selected at random places: cells
    /// told wrong, lines in conflict, and the walls and holes told among
    /// those some line crosses.
    fn tell(seed: u64, count: usize) -> (usize, usize, (usize, usize), (usize, usize)) {
        let mut rng = Rng(seed);
        let (mut wrong, mut conflicts) = (0, 0);
        let (mut walls, mut walls_told, mut holes, mut holes_told) = (0, 0, 0, 0);
        for (n, map) in boss_maps().iter().enumerate() {
            let floor: Vec<u16> = (0..CELLS as u16).filter(|&c| map[usize::from(c)] == Cell::Walkable).collect();
            let mut cells = as_read(map);
            let views: Vec<RangeView> = (0..count)
                .map(|_| {
                    let bodies: Vec<u16> = (0..4).map(|_| floor[rng.below(floor.len())]).collect();
                    view_of(map, bodies[0], (1, 2 + rng.below(9) as u16), &bodies)
                })
                .collect();
            conflicts += infer(&mut cells, &views).conflicts;
            // The cells some line between two floor cells crosses: the
            // only ones whose sight matters in a fight.
            let mut between = HashSet::new();
            for view in &views {
                for &t in view.in_sight.iter().chain(&view.out_of_sight) {
                    between.extend(crossed(view.caster, t).unwrap());
                }
            }
            for c in 0..CELLS {
                match (map[c], cells[c].cell) {
                    (Cell::Obstacle, Cell::Hole) | (Cell::Hole, Cell::Obstacle) => {
                        wrong += 1;
                        println!("map {n}: cell {c} truly {:?}, told {:?}", map[c], cells[c].cell);
                    }
                    (Cell::Obstacle, told) if between.contains(&(c as u16)) => {
                        walls += 1;
                        walls_told += usize::from(told == Cell::Obstacle);
                    }
                    (Cell::Hole, told) if between.contains(&(c as u16)) => {
                        holes += 1;
                        holes_told += usize::from(told == Cell::Hole);
                    }
                    _ => {}
                }
            }
        }
        (wrong, conflicts, (walls_told, walls), (holes_told, holes))
    }

    /// Eight spells selected on every boss map: no cell is ever told wrong,
    /// no line contradicts the map, and the walls and holes the lines cross
    /// are told apart as often as measured.
    #[test]
    fn the_range_tells_walls_from_holes_on_every_boss_map() {
        let (wrong, conflicts, walls, holes) = tell(0x9E37_79B9_7F4A_7C15, 8);
        println!("walls told {walls:?}, holes told {holes:?}");
        assert_eq!((wrong, conflicts), (0, 0));
        assert!(walls.0 * 100 >= walls.1 * 45, "{walls:?} walls");
        assert!(holes.0 * 100 >= holes.1 * 75, "{holes:?} holes");
    }

    #[test]
    #[ignore = "measurement, run on demand"]
    fn measure_by_number_of_spells() {
        for count in [4, 8, 20, 40] {
            for seed in [1u64, 2, 3] {
                let (wrong, conflicts, walls, holes) = tell(seed * 0x2545_F491_4F6C_DD1D, count);
                println!("{count} spells, draw {seed}: wrong {wrong}, conflicts {conflicts}, walls {walls:?}, holes {holes:?}");
            }
        }
    }

    /// A cell not known on a line out of sight could be what blocks it: the
    /// line proves nothing of the others.
    #[test]
    fn a_cell_not_known_leaves_a_line_unexplained() {
        let id = |x: i16, y: i16| id_of(dofus_grid::Case::new(x, y)).unwrap();
        let mut cells = vec![Seen::new(Cell::Walkable, 0.9); CELLS];
        cells[usize::from(id(1, 0))] = Seen::new(Cell::NotWalkable, 0.7);
        cells[usize::from(id(2, 0))] = Seen::UNKNOWN;
        let view = RangeView { caster: id(0, 0), out_of_sight: vec![id(3, 0)], ..RangeView::default() };
        assert_eq!(infer(&mut cells, std::slice::from_ref(&view)), Learned::default());
        assert_eq!(cells[usize::from(id(1, 0))].cell, Cell::NotWalkable);
        // Known walkable, the other cell leaves the first as the only one
        // that may block.
        cells[usize::from(id(2, 0))] = Seen::new(Cell::Walkable, 0.9);
        assert_eq!(infer(&mut cells, &[view]).obstacles, 1);
        assert_eq!(cells[usize::from(id(1, 0))].cell, Cell::Obstacle);
    }

    /// A body on the way explains a line out of sight: the cell not walkable
    /// beside it is not taken for a wall.
    #[test]
    fn a_body_on_the_way_explains_the_line() {
        let id = |x: i16, y: i16| id_of(dofus_grid::Case::new(x, y)).unwrap();
        let mut cells = vec![Seen::new(Cell::Walkable, 0.9); CELLS];
        cells[usize::from(id(1, 0))] = Seen::new(Cell::NotWalkable, 0.7);
        let view =
            RangeView { caster: id(0, 0), out_of_sight: vec![id(3, 0)], bodies: vec![id(2, 0)], ..RangeView::default() };
        assert_eq!(infer(&mut cells, &[view]), Learned::default());
        assert_eq!(cells[usize::from(id(1, 0))].cell, Cell::NotWalkable);
    }

    /// A wall on a line shown in sight contradicts the reading, and so does a
    /// line out of sight with nothing to block it: counted, and nothing is
    /// overwritten.
    #[test]
    fn a_wall_on_a_line_in_sight_is_a_conflict() {
        let id = |x: i16, y: i16| id_of(dofus_grid::Case::new(x, y)).unwrap();
        let mut cells = vec![Seen::new(Cell::Walkable, 0.9); CELLS];
        cells[usize::from(id(1, 0))] = Seen::new(Cell::Obstacle, 0.8);
        let view = RangeView { caster: id(0, 0), in_sight: vec![id(2, 0)], ..RangeView::default() };
        assert_eq!(infer(&mut cells, &[view]).conflicts, 1);
        assert_eq!(cells[usize::from(id(1, 0))].cell, Cell::Obstacle);
        // Out of sight across open floor: nothing could have blocked.
        let open = RangeView { caster: id(0, 3), out_of_sight: vec![id(3, 3)], ..RangeView::default() };
        assert_eq!(infer(&mut cells, &[open]).conflicts, 1);
    }
}
