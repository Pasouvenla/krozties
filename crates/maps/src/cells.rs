//! The 560 cells of a Dofus map and their three names.
//!
//! * The game numbers its cells 0 to 559, fourteen per half-row.
//! * The game's coordinates put each cell at (x, y), x from 0 to 33 and y
//!   from -19 to 13; one step along either axis is one cell of distance.
//! * Krozties draws and computes on [`Case`]s: the same axes, y flipped and
//!   the centre moved, so game (x, y) is case (x - 16, -y - 3). The 35 boss
//!   maps of the KrozTools land exactly on the game's 560 cells this way, and
//!   the flip keeps Krozties' projection the right way round.

use dofus_grid::Case;

pub const CELLS: usize = 560;
const MAP_WIDTH: i32 = 14;

/// The game coordinates of a cell, or `None` past the last cell.
pub fn coords(id: u16) -> Option<(i32, i32)> {
    if usize::from(id) >= CELLS {
        return None;
    }
    let (half_row, column) = (i32::from(id) / MAP_WIDTH, i32::from(id) % MAP_WIDTH);
    // Each pair of half-rows moves x one step right and y one step down; the
    // second half-row of a pair starts one step further right.
    let pair = half_row / 2;
    Some((pair + half_row % 2 + column, column - pair))
}

/// The cell at game coordinates (x, y), or `None` outside the map.
pub fn id_at(x: i32, y: i32) -> Option<u16> {
    let d = x - y;
    if d < 0 {
        return None;
    }
    let id = u16::try_from(d * MAP_WIDTH + y + d / 2).ok()?;
    (coords(id) == Some((x, y))).then_some(id)
}

/// The Krozties case of a cell.
pub fn case(id: u16) -> Option<Case> {
    let (x, y) = coords(id)?;
    Some(Case::new(i16::try_from(x - 16).ok()?, i16::try_from(-y - 3).ok()?))
}

/// The cell under a Krozties case, or `None` outside the map.
pub fn id_of(case: Case) -> Option<u16> {
    id_at(i32::from(case.x) + 16, -i32::from(case.y) - 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Built as the game builds its table: twenty pairs of half-rows of
    /// fourteen cells.
    fn game_table() -> Vec<(i32, i32)> {
        let (mut x, mut y, mut table) = (0, 0, Vec::new());
        for _ in 0..20 {
            table.extend((0..14).map(|b| (x + b, y + b)));
            x += 1;
            table.extend((0..14).map(|b| (x + b, y + b)));
            y -= 1;
        }
        table
    }

    #[test]
    fn every_cell_has_the_game_coordinates_and_back() {
        for (id, &(x, y)) in game_table().iter().enumerate() {
            let id = u16::try_from(id).unwrap();
            assert_eq!(coords(id), Some((x, y)), "cell {id}");
            assert_eq!(id_at(x, y), Some(id), "({x}, {y})");
            assert_eq!(id_of(case(id).unwrap()), Some(id));
        }
        assert_eq!(coords(560), None);
        assert_eq!(id_at(0, 1), None);
        assert_eq!(id_at(40, 0), None);
    }

    /// The centre of Krozties' boards is cell 272, and one step along an axis
    /// of the game is one step along an axis of Krozties.
    #[test]
    fn krozties_cases_follow_the_game_axes() {
        assert_eq!(id_of(Case::new(0, 0)), Some(272));
        let centre = case(272).unwrap();
        for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            let (x, y) = coords(272).unwrap();
            let next = id_at(x + dx, y + dy).unwrap();
            assert_eq!(case(next).unwrap().distance(centre), 1);
        }
    }
}
