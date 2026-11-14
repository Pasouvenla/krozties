//! What each cell of the checkerboard is, and which cell of the game it is.
//!
//! The patch of consistent cells the lattice was found with is floor: it
//! teaches the tile of each parity, once the holes and decor it took in are
//! set aside by their colour. Every other cell is read against the tiles
//! nearest to it, light changing across a map; a cell read surely becomes a
//! tile in turn, so the reading spreads over the floor. A cell is walkable
//! when part of it shows the tile of its parity, decor raised in front of it
//! hiding the rest, or when a team ring stands on it beside the floor.
//!
//! The floor is one or a few large areas. A small area beyond their hull is
//! kept only when it shows whole tiles and the checkerboard across its own
//! borders: an island does, decor or interface the colour of a tile does not,
//! and what it hides stays unknown. The other cells in view are not walkable;
//! whether one blocks the line of sight, a wall, or not, a hole, the frame
//! does not show, the lines of sight of a fight will. Beyond the floor's hull
//! they are less sure: the interface may hide floor there.
//!
//! The lattice knows the cells up to a whole translation. The map's 560 cells
//! form a fixed shape: the translation holds every walkable cell inside it
//! and shows the most of the map on the frame. When the map does not fill
//! the frame several fit; the board says how many, and the store aligns its
//! readings, so a reading only has to be consistent with itself.

use std::collections::HashMap;

use dofus_maps::{cells, Cell, Seen};

use crate::gray::Gray;
use crate::read::lattice::{patch, Integral, Lattice, WORK_WIDTH};
use crate::Frame;

/// A frame's checkerboard read cell by cell, in the game's cells.
#[derive(Clone, Debug)]
pub struct Board {
    /// The lattice, in frame pixels.
    pub lattice: Lattice,
    /// Lattice cell (i, j) is game cell (i + tx, j + ty).
    pub translation: (i32, i32),
    /// By game cell id.
    pub cells: Vec<Seen>,
    /// How surely the translation is the only one that fits, 0 to 1.
    pub anchoring: f32,
}

/// The frame shrunk like the luminance plane, one plane per channel.
fn channels(frame: &Frame) -> [Gray; 3] {
    let (width, height) = (frame.width as usize, frame.height as usize);
    let factor = width.div_ceil(WORK_WIDTH).max(1);
    let (w, h) = (width / factor, height / factor);
    let mut planes = [vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h]];
    let area = (factor * factor) as f32;
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0f32; 3];
            for dy in 0..factor {
                for dx in 0..factor {
                    let i = ((y * factor + dy) * width + x * factor + dx) * 4;
                    if let Some(p) = frame.bgra.get(i..i + 3) {
                        sum[0] += f32::from(p[2]);
                        sum[1] += f32::from(p[1]);
                        sum[2] += f32::from(p[0]);
                    }
                }
            }
            for c in 0..3 {
                planes[c][y * w + x] = sum[c] / area;
            }
        }
    }
    planes.map(|data| Gray { width: w, height: h, data })
}

fn median(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn parity(i: i32, j: i32) -> usize {
    (i + j).rem_euclid(2) as usize
}

/// Whether a team ring stands on the cell: enough strongly red or blue pixels
/// over its diamond.
fn team_ring(planes: &[Gray; 3], lattice: &Lattice, i: i32, j: i32) -> bool {
    let (cx, cy) = lattice.centre(i, j);
    let (hw, hh) = (lattice.cell_width / 2.0, lattice.cell_width / 4.0);
    let (w, h) = (planes[0].width as i64, planes[0].height as i64);
    let (mut team, mut all) = (0usize, 0usize);
    for y in (cy - hh) as i64..=(cy + hh) as i64 {
        for x in (cx - hw) as i64..=(cx + hw) as i64 {
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            if ((x as f32 + 0.5 - cx) / hw).abs() + ((y as f32 + 0.5 - cy) / hh).abs() > 1.0 {
                continue;
            }
            let (x, y) = (x as usize, y as usize);
            let (r, g, b) = (planes[0].at(x, y), planes[1].at(x, y), planes[2].at(x, y));
            all += 1;
            if (r > 150.0 && r > g + 70.0 && r > b + 70.0) || (b > 150.0 && b > r + 70.0 && b > g + 30.0) {
                team += 1;
            }
        }
    }
    all > 0 && team as f32 >= 0.06 * all as f32
}

/// The points of a cell its colour is read at, in lattice offsets from its
/// centre: far enough inside that the lattice's error stays within the cell,
/// spread so that decor raised in front of the cell leaves some in view.
const POINTS: [(f32, f32); 9] = [
    (0.0, 0.0),
    (0.25, 0.0),
    (-0.25, 0.0),
    (0.0, 0.25),
    (0.0, -0.25),
    (0.25, 0.25),
    (-0.25, -0.25),
    (0.25, -0.25),
    (-0.25, 0.25),
];

/// The tiles of the floor patch nearest to a cell are its reference: light
/// and decor vary across a map, a tile's neighbours share its lighting.
const NEAREST: usize = 6;

/// A lattice cell's place on screen, in cell widths.
fn position(i: i32, j: i32) -> (f32, f32) {
    ((i + j) as f32 / 2.0, (i - j) as f32 / 4.0)
}

/// A lattice cell and its colour.
type Tile = ((i32, i32), [f32; 3]);

/// The floor's tiles as the patch shows them, each cell's colour at its
/// centre, by parity.
struct Tiles {
    cells: [Vec<Tile>; 2],
}

impl Tiles {
    /// The colour a floor tile at (i, j) has: the median of the nearest tiles
    /// of its parity, itself left out.
    fn reference(&self, i: i32, j: i32) -> Option<[f32; 3]> {
        let at = position(i, j);
        let mut near: Vec<(f32, [f32; 3])> = self.cells[parity(i, j)]
            .iter()
            .filter(|(cell, _)| *cell != (i, j))
            .map(|(cell, c)| {
                let p = position(cell.0, cell.1);
                ((p.0 - at.0).powi(2) + (p.1 - at.1).powi(2), *c)
            })
            .collect();
        if near.is_empty() {
            return None;
        }
        let k = NEAREST.min(near.len());
        near.select_nth_unstable_by(k - 1, |a, b| a.0.total_cmp(&b.0));
        let near = &near[..k];
        Some([0, 1, 2].map(|ch| median(&mut near.iter().map(|n| n.1[ch]).collect::<Vec<_>>())))
    }
}

/// The convex hull of lattice cells, counter-clockwise.
fn hull(mut points: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
    points.sort_unstable();
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let cross = |o: (i32, i32), a: (i32, i32), b: (i32, i32)| {
        i64::from(a.0 - o.0) * i64::from(b.1 - o.1) - i64::from(a.1 - o.1) * i64::from(b.0 - o.0)
    };
    let mut lower: Vec<(i32, i32)> = Vec::new();
    for &p in &points {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<(i32, i32)> = Vec::new();
    for &p in points.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Whether a cell lies within a counter-clockwise hull, its edges included.
fn within(hull: &[(i32, i32)], cell: (i32, i32)) -> bool {
    if hull.len() < 3 {
        return false;
    }
    (0..hull.len()).all(|k| {
        let (a, b) = (hull[k], hull[(k + 1) % hull.len()]);
        i64::from(b.0 - a.0) * i64::from(cell.1 - a.1) - i64::from(b.1 - a.1) * i64::from(cell.0 - a.0) >= 0
    })
}

/// The cells of a frame read on the lattice, at the plane's scale: by lattice
/// cell, what it is. Walkable where a tile of its parity shows, or a team
/// ring; within the floor's hull, the other cells are obstacles or holes, the
/// only ones a fight's moves and lines of sight cross; beyond it, unknown.
fn classify(frame: &Frame, lattice: &Lattice, gray: &Gray) -> Option<HashMap<(i32, i32), Seen>> {
    let floor = patch(&Integral::of(gray), lattice);
    if floor.len() < 8 {
        return None;
    }
    let planes = channels(frame);
    let integrals: Vec<Integral> = planes.iter().map(Integral::of).collect();
    let r = lattice.cell_width / 16.0;
    let colour_at = |i: f32, j: f32| -> Option<[f32; 3]> {
        let (x, y) = lattice.point(i, j);
        Some([integrals[0].mean(x, y, r)?, integrals[1].mean(x, y, r)?, integrals[2].mean(x, y, r)?])
    };

    let mut tiles = Tiles { cells: [Vec::new(), Vec::new()] };
    for &(i, j) in &floor {
        if let Some(c) = colour_at(i as f32, j as f32) {
            tiles.cells[parity(i, j)].push(((i, j), c));
        }
    }
    // A hole or a dark obstacle walled in by floor is darker than its four
    // neighbours, as a dark tile is: the patch takes it in. Its colour gives
    // it away, far from the parity's tiles.
    for cells in &mut tiles.cells {
        let typical = [0, 1, 2].map(|ch| median(&mut cells.iter().map(|c| c.1[ch]).collect::<Vec<_>>()));
        let mut spread: Vec<f32> = cells.iter().map(|c| distance(c.1, typical)).collect();
        let limit = (3.0 * median(&mut spread)).max(24.0);
        cells.retain(|c| distance(c.1, typical) <= limit);
    }
    // How far a tile of the patch strays from its neighbours.
    let mut strays: Vec<f32> = tiles
        .cells
        .iter()
        .flatten()
        .filter_map(|&((i, j), c)| tiles.reference(i, j).map(|t| distance(c, t)))
        .collect();
    strays.sort_by(f32::total_cmp);
    let tolerance = (1.5 * strays.get(strays.len() * 95 / 100).copied().unwrap_or(0.0)).clamp(8.0, 32.0);

    let (w, h) = (gray.width as f32, gray.height as f32);
    let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)].map(|(x, y)| lattice.locate(x, y));
    let range = |pick: fn(&(f32, f32)) -> f32| {
        let values = corners.iter().map(pick);
        (values.clone().fold(f32::MAX, f32::min).floor() as i32, values.fold(f32::MIN, f32::max).ceil() as i32)
    };
    let ((imin, imax), (jmin, jmax)) = (range(|c| c.0), range(|c| c.1));
    let in_view: Vec<(i32, i32)> = (imin..=imax)
        .flat_map(|i| (jmin..=jmax).map(move |j| (i, j)))
        .filter(|&(i, j)| {
            let (cx, cy) = lattice.centre(i, j);
            cx >= 0.0 && cy >= 0.0 && cx < w && cy < h
        })
        .collect();
    // Each cell against the tiles nearest to it. A cell read surely, most of
    // its points matching, becomes a tile its neighbours are read against in
    // turn: the reading spreads from the patch over the floor, following the
    // light as it changes.
    let mut read: HashMap<(i32, i32), Seen> = HashMap::new();
    let mut other = Vec::new();
    for _ in 0..4 {
        read.clear();
        other.clear();
        for &(i, j) in &in_view {
            let Some(tile) = tiles.reference(i, j) else { continue };
            let colours: Vec<[f32; 3]> =
                POINTS.iter().filter_map(|(di, dj)| colour_at(i as f32 + di, j as f32 + dj)).collect();
            let matches = colours.iter().filter(|c| distance(**c, tile) <= tolerance).count();
            let seen = match matches {
                0 if colours.is_empty() => continue,
                0 => {
                    other.push((i, j));
                    continue;
                }
                1 => Seen::new(Cell::Walkable, 0.6),
                2 => Seen::new(Cell::Walkable, 0.7),
                3 | 4 => Seen::new(Cell::Walkable, 0.8),
                _ => Seen::new(Cell::Walkable, 0.9),
            };
            read.insert((i, j), seen);
        }
        let known: std::collections::HashSet<(i32, i32)> = tiles.cells.iter().flatten().map(|c| c.0).collect();
        let sure: Vec<Tile> = read
            .iter()
            .filter(|(cell, s)| s.confidence >= 0.9 && !known.contains(cell))
            .filter_map(|(&(i, j), _)| colour_at(i as f32, j as f32).map(|c| ((i, j), c)))
            .collect();
        if sure.is_empty() {
            break;
        }
        for (cell, c) in sure {
            tiles.cells[parity(cell.0, cell.1)].push((cell, c));
        }
    }
    // The floor is one or a few large areas.
    let floor_areas = areas(&read.keys().copied().collect());
    let largest = floor_areas.iter().map(Vec::len).max().unwrap_or(0);
    let main =
        hull(floor_areas.iter().filter(|a| a.len() >= (largest / 10).max(3)).flatten().copied().collect());
    // An entity stands on the floor: a team ring away from it is something
    // else, a portrait of the timeline for one.
    let on_floor = |i: i32, j: i32| {
        within(&main, (i, j))
            || [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(di, dj)| read.contains_key(&(i + di, j + dj)))
    };
    let (rings, other): (Vec<_>, Vec<_>) =
        other.into_iter().partition(|&(i, j)| on_floor(i, j) && team_ring(&planes, lattice, i, j));
    for cell in rings {
        read.insert(cell, Seen::new(Cell::Walkable, 0.7));
    }
    // A small area beyond the floor's hull may be an island of the map, or
    // decor or interface that matches a tile or a team, the timeline's
    // portraits for one: an island shows whole tiles and the checkerboard
    // across its own borders, a patch of colour does not.
    let small: Vec<Vec<(i32, i32)>> = areas(&read.keys().copied().collect())
        .into_iter()
        .filter(|a| a.len() < (largest / 10).max(3) && a.iter().any(|&cell| !within(&main, cell)))
        .collect();
    let steps = |a: (i32, i32), b: (i32, i32)| {
        let (Some(ta), Some(tb)) = (tiles.reference(a.0, a.1), tiles.reference(b.0, b.1)) else { return false };
        let (di, dj) = ((b.0 - a.0) as f32, (b.1 - a.1) as f32);
        let clear = [-0.2f32, 0.0, 0.2]
            .iter()
            .filter(|&&t| {
                let side = |k: f32| colour_at(a.0 as f32 + k * di + t * dj, a.1 as f32 + k * dj + t * di);
                match (side(0.25), side(0.75)) {
                    (Some(ca), Some(cb)) => {
                        distance(ca, ta) < distance(ca, tb) && distance(cb, tb) < distance(cb, ta)
                    }
                    _ => false,
                }
            })
            .count();
        clear >= 2
    };
    let mut unknown = Vec::new();
    for area in small {
        let whole = 3 * area.iter().filter(|cell| read.get(cell).is_some_and(|s| s.confidence >= 0.9)).count()
            >= 2 * area.len();
        let members: std::collections::HashSet<(i32, i32)> = area.iter().copied().collect();
        let (mut borders, mut stepping) = (0, 0);
        for &(i, j) in &area {
            for next in [(i + 1, j), (i, j + 1)] {
                if members.contains(&next) {
                    borders += 1;
                    stepping += usize::from(steps((i, j), next));
                }
            }
        }
        if !whole || borders == 0 || 3 * stepping < 2 * borders {
            for cell in area {
                read.remove(&cell);
                unknown.push(cell);
            }
        }
    }
    let shape = hull(read.keys().copied().collect());
    for cell in other {
        let sure = if within(&shape, cell) { 0.7 } else { 0.5 };
        read.insert(cell, Seen::new(Cell::NotWalkable, sure));
    }
    for cell in unknown {
        read.insert(cell, Seen::UNKNOWN);
    }
    Some(read)
}

/// Reads the cells of the checkerboard that `found` places on the frame.
pub fn read(frame: &Frame, found: &Lattice) -> Option<Board> {
    let (gray, factor) = Gray::from_frame(frame, WORK_WIDTH);
    let f = factor as f32;
    let lattice = Lattice { cell_width: found.cell_width / f, x0: found.x0 / f, y0: found.y0 / f, ..*found };
    let read = classify(frame, &lattice, &gray)?;
    let fits = anchorings(&read, &lattice, gray.width as f32, gray.height as f32);
    let translation = *fits.first()?;
    let cells = (0..cells::CELLS as u16)
        .map(|id| {
            let (x, y) = cells::coords(id).unwrap_or_default();
            read.get(&(x - translation.0, y - translation.1)).copied().unwrap_or(Seen::UNKNOWN)
        })
        .collect();
    Some(Board { lattice: *found, translation, cells, anchoring: 1.0 / fits.len() as f32 })
}

/// The areas of cells joined by their sides, the way a fighter walks.
fn areas(cells: &std::collections::HashSet<(i32, i32)>) -> Vec<Vec<(i32, i32)>> {
    let mut seen = std::collections::HashSet::new();
    let mut found = Vec::new();
    for &start in cells {
        if !seen.insert(start) {
            continue;
        }
        let (mut area, mut stack) = (vec![start], vec![start]);
        while let Some((i, j)) = stack.pop() {
            for next in [(i + 1, j), (i - 1, j), (i, j + 1), (i, j - 1)] {
                if cells.contains(&next) && seen.insert(next) {
                    area.push(next);
                    stack.push(next);
                }
            }
        }
        found.push(area);
    }
    found
}

/// The translations that fit the reading best, the preferred first: those
/// holding every walkable cell inside the map's shape and showing the most of
/// the map on the plane, the camera framing the map. All of them fit the
/// frame as well: the others say how ambiguous the choice was. The first puts
/// the map in the middle across the frame, where a game draws a map narrower
/// than its window, then the floor in the middle of the map.
fn anchorings(read: &HashMap<(i32, i32), Seen>, lattice: &Lattice, w: f32, h: f32) -> Vec<(i32, i32)> {
    let walkable: Vec<(i32, i32)> = read.iter().filter(|(_, s)| s.cell == Cell::Walkable).map(|(k, _)| *k).collect();
    if walkable.is_empty() {
        return Vec::new();
    }
    let span = |pick: fn(&(i32, i32)) -> i32| {
        (walkable.iter().map(pick).min().unwrap_or(0), walkable.iter().map(pick).max().unwrap_or(0))
    };
    let ((imin, imax), (jmin, jmax)) = (span(|c| c.0), span(|c| c.1));
    let all: Vec<(i32, i32)> = (0..cells::CELLS as u16).filter_map(cells::coords).collect();
    let mean = |cells: &[(i32, i32)]| {
        let n = cells.len() as f32;
        (cells.iter().map(|c| c.0 as f32).sum::<f32>() / n, cells.iter().map(|c| c.1 as f32).sum::<f32>() / n)
    };
    let (centre, floor_centre) = (mean(&all), mean(&walkable));
    let mut ranked = Vec::new();
    for tx in -imax..=33 - imin {
        for ty in -19 - jmax..=13 - jmin {
            let outside = walkable.iter().filter(|(i, j)| cells::id_at(i + tx, j + ty).is_none()).count();
            let shown = all
                .iter()
                .filter(|(x, y)| {
                    let (cx, cy) = lattice.centre(x - tx, y - ty);
                    cx >= 0.0 && cy >= 0.0 && cx < w && cy < h
                })
                .count();
            // In half cells, the steps a translation moves the map across.
            let across = ((lattice.point(centre.0 - tx as f32, centre.1 - ty as f32).0 - w / 2.0).abs()
                / (lattice.cell_width / 2.0))
                .round() as i32;
            let off_centre =
                (floor_centre.0 + tx as f32 - centre.0).abs() + (floor_centre.1 + ty as f32 - centre.1).abs();
            ranked.push(((tx, ty), outside, shown, across, off_centre));
        }
    }
    ranked.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)).then(a.3.cmp(&b.3)).then(a.4.total_cmp(&b.4)));
    let Some(&(_, outside, shown, _, _)) = ranked.first() else { return Vec::new() };
    ranked.into_iter().take_while(|r| r.1 == outside && r.2 == shown).map(|r| r.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read::lattice::find;
    use crate::synthetic::{boss_maps, render_with_floor, Rng, Scene};

    /// The share of a walkable cell's diamond where its own tile still shows.
    fn shown(scene: &Scene, floor: &[Option<u16>], id: u16) -> f32 {
        let (x, y) = cells::coords(id).unwrap();
        let (cx, cy) = scene.centre(x, y);
        let (hw, hh) = (scene.cell_width / 2.0, scene.cell_width / 4.0);
        let (w, h) = (scene.size.0 as i64, scene.size.1 as i64);
        let (mut own, mut all) = (0usize, 0usize);
        for py in (cy - hh).floor() as i64..=(cy + hh).ceil() as i64 {
            for px in (cx - hw).floor() as i64..=(cx + hw).ceil() as i64 {
                if ((px as f32 + 0.5 - cx) / hw).abs() + ((py as f32 + 0.5 - cy) / hh).abs() > 1.0 {
                    continue;
                }
                all += 1;
                if px >= 0 && py >= 0 && px < w && py < h && floor[(py * w + px) as usize] == Some(id) {
                    own += 1;
                }
            }
        }
        own as f32 / all.max(1) as f32
    }

    /// What the board read at the place the scene drew game cell `id`.
    fn read_at(scene: &Scene, board: &Board, id: u16) -> Seen {
        placed(scene, board, id).unwrap_or(Seen::UNKNOWN)
    }

    /// The same, `None` when the board's translation puts the cell off the
    /// map: a board anchored a few cells off leaves some cells of the map's
    /// edge out, and the store lays its readings back over each other.
    fn placed(scene: &Scene, board: &Board, id: u16) -> Option<Seen> {
        let (x, y) = cells::coords(id).unwrap();
        let (cx, cy) = scene.centre(x, y);
        let (i, j) = board.lattice.locate(cx, cy);
        cells::id_at(i.round() as i32 + board.translation.0, j.round() as i32 + board.translation.1)
            .map(|k| board.cells[usize::from(k)])
    }

    /// Whether the scene drew game cell `id` above the interface bar.
    fn in_view(scene: &Scene, id: u16) -> bool {
        let (x, y) = cells::coords(id).unwrap();
        let (cx, cy) = scene.centre(x, y);
        cx >= 0.0 && cy >= 0.0 && cx < scene.size.0 as f32 && cy < (1.0 - scene.bar) * scene.size.1 as f32
    }

    /// A boss map drawn with raised obstacles and five entities.
    fn scene(rng: &mut Rng, n: usize, map: Vec<Cell>, seed: u64) -> Scene {
        let width = rng.range(55.0, 95.0);
        let mut scene = Scene::centred(map, (1280, 800), width, seed + n as u64);
        scene.obstacle_height = rng.range(0.0, 0.5);
        let walkable: Vec<u16> =
            (0..cells::CELLS as u16).filter(|id| scene.cells[usize::from(*id)] == Cell::Walkable).collect();
        scene.entities = (0..5).map(|k| walkable[(k * 37 + n) % walkable.len()]).collect();
        scene
    }

    /// Counts over the cells in view, each as (cells read so, cells).
    #[derive(Default)]
    struct Tally {
        /// Walkable tiles at least half in view, read walkable.
        seen: (usize, usize),
        /// Walkable tiles mostly hidden by decor, read walkable.
        hidden: (usize, usize),
        /// The other cells, read walkable.
        other: (usize, usize),
        /// The other cells between floor cells, read not walkable.
        between: (usize, usize),
        /// Walkable cells beside no other floor, read not walkable.
        alone: (usize, usize),
        /// The other cells beyond the floor's hull, read not walkable.
        beyond: (usize, usize),
        /// Entities in view whose cell is not read walkable.
        failures: Vec<String>,
    }

    impl Tally {
        fn print(&self) {
            println!("walkable tiles at least half in view read walkable: {} of {}", self.seen.0, self.seen.1);
            println!("walkable tiles mostly hidden read walkable: {} of {}", self.hidden.0, self.hidden.1);
            println!("other cells read walkable: {} of {}", self.other.0, self.other.1);
            println!("cells between floor cells read not walkable: {} of {}", self.between.0, self.between.1);
            println!("walkable cells alone read not walkable: {} of {}", self.alone.0, self.alone.1);
            println!("cells beyond the floor's hull read not walkable: {} of {}", self.beyond.0, self.beyond.1);
        }
    }

    fn add(tally: &mut (usize, usize), hit: bool) {
        *tally = (tally.0 + usize::from(hit), tally.1 + 1);
    }

    /// Reads every boss map drawn by `rng` and `seed`.
    fn measure(rng: u64, seed: u64) -> Tally {
        let mut rng = Rng::new(rng);
        let mut tally = Tally::default();
        for (n, map) in boss_maps().into_iter().enumerate() {
            let scene = scene(&mut rng, n, map, seed);
            let (frame, floor) = render_with_floor(&scene);
            let lattice = find(&frame).expect("a checkerboard");
            let board = read(&frame, &lattice).expect("a board");
            let walkable: Vec<(i32, i32)> = (0..cells::CELLS as u16)
                .filter(|id| scene.cells[usize::from(*id)] == Cell::Walkable && in_view(&scene, *id))
                .map(|id| cells::coords(id).unwrap())
                .collect();
            // A cell alone, beside no other floor, cannot be told from decor
            // the colour of a tile: the reader leaves it unknown, and the
            // floor's hull without it.
            let joined: Vec<(i32, i32)> = walkable
                .iter()
                .filter(|&&(x, y)| walkable.iter().any(|&(a, b)| (a - x).abs() + (b - y).abs() == 1))
                .copied()
                .collect();
            let shape = hull(joined.clone());
            for id in (0..cells::CELLS as u16).filter(|id| in_view(&scene, *id)) {
                let truly_walkable = scene.cells[usize::from(id)] == Cell::Walkable;
                let Some(read) = placed(&scene, &board, id).or(truly_walkable.then_some(Seen::UNKNOWN)) else {
                    continue;
                };
                let (x, y) = cells::coords(id).unwrap();
                let walkable = read.cell == Cell::Walkable;
                let known = read.cell.walkable() == Some(false);
                if truly_walkable {
                    if !joined.contains(&(x, y)) {
                        add(&mut tally.alone, known);
                    } else if shown(&scene, &floor, id) >= 0.5 {
                        add(&mut tally.seen, walkable);
                    } else {
                        add(&mut tally.hidden, walkable);
                    }
                } else {
                    add(&mut tally.other, walkable);
                    // Strictly inside: on the hull's edge a cell may be left
                    // out by the cells the frame cuts.
                    if within(&shape, (x, y)) && joined.iter().filter(|c| c.0 == x || c.1 == y).count() >= 2 {
                        add(&mut tally.between, known);
                    } else if !within(&shape, (x, y)) {
                        add(&mut tally.beyond, known);
                    }
                }
            }
            for &id in &scene.entities {
                let (x, y) = cells::coords(id).unwrap();
                let (cx, cy) = scene.centre(x, y);
                // The timeline's portraits, top right, hide what is under them.
                let under_timeline = cy < 80.0 && cx > 900.0;
                if in_view(&scene, id) && !under_timeline && read_at(&scene, &board, id).cell != Cell::Walkable {
                    tally.failures.push(format!("map {n}: entity cell {id} at ({cx:.0}, {cy:.0}) not read walkable"));
                }
            }
        }
        tally
    }

    fn share(tally: (usize, usize)) -> f32 {
        tally.0 as f32 / tally.1.max(1) as f32
    }

    /// On every boss map: the walkable cells in view are read walkable,
    /// entities' cells included, unless decor hides most of them; almost no
    /// other cell is; the cells between floor cells and beyond the floor are
    /// read not walkable; a walkable cell alone is read walkable or unknown,
    /// never not walkable.
    #[test]
    fn the_cells_of_every_boss_map_are_read() {
        let tally = measure(17, 500);
        tally.print();
        assert!(tally.failures.is_empty(), "{:#?}", tally.failures);
        assert!(share(tally.seen) >= 0.99, "{:?} tiles in view read walkable", tally.seen);
        assert!(share(tally.other) <= 0.01, "{:?} other cells read walkable", tally.other);
        assert!(share(tally.between) >= 0.98, "{:?} cells between read not walkable", tally.between);
        assert_eq!(tally.alone.0, 0, "walkable cells alone read not walkable");
        assert!(share(tally.beyond) >= 0.95, "{:?} cells beyond read not walkable", tally.beyond);
    }

    /// The reader on draws it was never adjusted on.
    #[test]
    #[ignore = "measurement, run on demand"]
    fn measure_on_unseen_draws() {
        for draw in 0..4u64 {
            println!("draw {draw}:");
            let tally = measure(1000 + draw, 3000 + 100 * draw);
            tally.print();
            println!("entity cells not read walkable: {}", tally.failures.len());
        }
    }

    /// Light that changes by nearly half across the frame: a tile in the
    /// light is lighter than the other tile in the shade, each tile is read
    /// against its neighbours.
    #[test]
    fn uneven_light_is_read() {
        let (mut seen, mut other) = ((0, 0), (0, 0));
        for (n, map) in boss_maps().into_iter().enumerate().step_by(3) {
            let mut scene = Scene::centred(map, (1280, 800), 64.0, 1300 + n as u64);
            scene.lighting = 0.45;
            let (frame, _) = render_with_floor(&scene);
            let board = read(&frame, &find(&frame).expect("a checkerboard")).expect("a board");
            for id in (0..cells::CELLS as u16).filter(|id| in_view(&scene, *id)) {
                let walkable = read_at(&scene, &board, id).cell == Cell::Walkable;
                if scene.cells[usize::from(id)] == Cell::Walkable {
                    add(&mut seen, walkable);
                } else {
                    add(&mut other, walkable);
                }
            }
        }
        println!("walkable read walkable {seen:?}, other read walkable {other:?}");
        assert!(share(seen) >= 0.99, "{seen:?}");
        assert!(share(other) <= 0.01, "{other:?}");
    }

    /// Pillars every few cells, each walled in by floor, darker than its
    /// four neighbours as a dark tile is: they are not taken for floor.
    #[test]
    fn pillars_walled_in_by_floor_are_not_floor() {
        let map: Vec<Cell> = (0..cells::CELLS as u16)
            .map(|id| {
                let (x, y) = cells::coords(id).unwrap();
                let inside = (4..=35).contains(&(x - y)) && (3..=24).contains(&(x + y));
                match (inside, (x + y).rem_euclid(2) == 1 && x.rem_euclid(5) < 2) {
                    (false, _) => Cell::Hole,
                    (true, true) => Cell::Hole,
                    (true, false) => Cell::Walkable,
                }
            })
            .collect();
        let (mut floor, mut pillars) = ((0, 0), (0, 0));
        for seed in 0..3 {
            let scene = Scene::centred(map.clone(), (1280, 800), 64.0, 1400 + seed);
            let (frame, _) = render_with_floor(&scene);
            let board = read(&frame, &find(&frame).expect("a checkerboard")).expect("a board");
            for id in (0..cells::CELLS as u16).filter(|id| in_view(&scene, *id)) {
                let (x, y) = cells::coords(id).unwrap();
                let read = read_at(&scene, &board, id).cell;
                if scene.cells[usize::from(id)] == Cell::Walkable {
                    add(&mut floor, read == Cell::Walkable);
                } else if (4..=35).contains(&(x - y)) && (3..=24).contains(&(x + y)) {
                    add(&mut pillars, read.walkable() == Some(false));
                }
            }
        }
        println!("floor read walkable {floor:?}, pillars read not walkable {pillars:?}");
        assert!(share(floor) >= 0.99, "{floor:?}");
        assert!(share(pillars) >= 0.98, "{pillars:?}");
    }

    /// An entity hiding its whole cell but its team ring stands on a
    /// walkable cell.
    #[test]
    fn a_team_ring_makes_its_cell_walkable() {
        let mut failures = Vec::new();
        let mut total = 0;
        for (n, map) in boss_maps().into_iter().enumerate().step_by(4) {
            let mut scene = Scene::centred(map, (1280, 800), 64.0, 700 + n as u64);
            scene.covering = true;
            let walkable: Vec<u16> = (0..cells::CELLS as u16)
                .filter(|id| scene.cells[usize::from(*id)] == Cell::Walkable && in_view(&scene, *id))
                .collect();
            scene.entities = walkable.iter().step_by(23).copied().collect();
            let (frame, _) = render_with_floor(&scene);
            let board = read(&frame, &find(&frame).expect("a checkerboard")).expect("a board");
            for &id in &scene.entities {
                let (x, y) = cells::coords(id).unwrap();
                let (cx, cy) = scene.centre(x, y);
                if cy < 80.0 && cx > 900.0 {
                    continue;
                }
                total += 1;
                if read_at(&scene, &board, id).cell != Cell::Walkable {
                    failures.push((n, id));
                }
            }
        }
        println!("{} of {total} covered entity cells read walkable", total - failures.len());
        assert!(failures.is_empty(), "{failures:?}");
    }

    /// Portraits in a team's colour, side by side away from a small floor,
    /// are no entities: a team ring counts on the floor only.
    #[test]
    fn team_colours_away_from_the_floor_are_no_entity() {
        let map: Vec<Cell> = (0..cells::CELLS as u16)
            .map(|id| {
                let (x, y) = cells::coords(id).unwrap();
                if (14..=27).contains(&(x - y)) && (8..=17).contains(&(x + y)) { Cell::Walkable } else { Cell::Hole }
            })
            .collect();
        let mut stray = Vec::new();
        for (seed, team) in [[200.0, 40.0, 40.0], [40.0, 70.0, 210.0]].into_iter().enumerate() {
            let mut scene = Scene::centred(map.clone(), (1280, 800), 64.0, 1500 + seed as u64);
            scene.portraits = Some(team);
            let (frame, _) = render_with_floor(&scene);
            let lattice = find(&frame).expect("a checkerboard");
            let (gray, _) = Gray::from_frame(&frame, WORK_WIDTH);
            for (&(i, j), seen) in &classify(&frame, &lattice, &gray).expect("a reading") {
                let (x, y) = lattice.centre(i, j);
                if seen.cell == Cell::Walkable && y < 100.0 {
                    stray.push((seed, x as i32, y as i32));
                }
            }
        }
        assert!(stray.is_empty(), "{stray:?}");
    }

    #[test]
    #[ignore = "measurement, run on demand"]
    fn measure_time() {
        for (size, width) in [((1280u32, 800u32), 64.0f32), ((2560, 1600), 128.0)] {
            let scene = Scene::centred(boss_maps().swap_remove(3), size, width, 77);
            let frame = render_with_floor(&scene).0;
            let start = std::time::Instant::now();
            let lattice = find(&frame).unwrap();
            let found = start.elapsed();
            let board = read(&frame, &lattice).unwrap();
            println!("{size:?}: lattice {:?}, board {:?}, {} cells", found, start.elapsed() - found, board.cells.len());
        }
    }

    /// The reading of a boss map's frame, at a window size and cell width.
    fn reading_of(map: Vec<Cell>, size: (u32, u32), width: f32, seed: u64, fight: &str) -> dofus_maps::Reading {
        let scene = Scene::centred(map, size, width, seed);
        let frame = render_with_floor(&scene).0;
        let board = read(&frame, &find(&frame).expect("a checkerboard")).expect("a board");
        dofus_maps::Reading { fight: fight.into(), coords: (0, 0), subarea: "Salle".into(), cells: board.cells, at_ms: 0 }
    }

    /// A boss map read through windows of two sizes, the cells anchored
    /// differently, is one map of the store; another arena at the same
    /// place is another map.
    #[test]
    fn two_windows_of_a_map_make_one_map_of_the_store() {
        use dofus_maps::{Event, Settings, Store};
        let maps = boss_maps();
        let (mut confirmed, mut shifted) = (0, 0);
        for n in [0usize, 7, 12, 21, 30] {
            let mut store = Store::new(Settings::default());
            let first = store.add(reading_of(maps[n].clone(), (1280, 800), 58.0, 40 + n as u64, "a")).unwrap();
            let second = store.add(reading_of(maps[n].clone(), (1600, 1000), 84.0, 60 + n as u64, "b")).unwrap();
            assert_eq!(first.map, second.map, "map {n} read twice made two maps");
            assert_ne!(second.event, Event::Created);
            confirmed += usize::from(second.verified);
            shifted += usize::from(second.shift != (0, 0));
            let other = store.add(reading_of(maps[n + 1].clone(), (1280, 800), 58.0, 80 + n as u64, "c")).unwrap();
            assert_eq!(other.event, Event::Created, "map {} taken for map {n}", n + 1);
        }
        println!("{confirmed} of 5 maps confirmed by their second window, {shifted} laid back by a shift");
        assert!(shifted > 0, "no window anchored the map elsewhere: the test proves nothing of the alignment");
    }

    /// With the whole map in view, the true translation is among those that
    /// fit, the one chosen a few cells from it at most, and the board says how
    /// sure it is.
    #[test]
    fn the_whole_map_in_view_is_anchored() {
        let (mut exact, mut total) = (0, 0);
        let mut failures = Vec::new();
        for (n, map) in boss_maps().into_iter().enumerate() {
            // The 10.25 cell widths of the map's height above the bar.
            let scene = Scene::centred(map, (1280, 800), 70.0, 900 + n as u64);
            let (frame, _) = render_with_floor(&scene);
            let lattice = find(&frame).expect("a checkerboard");
            let board = read(&frame, &lattice).expect("a board");
            let (gray, _) = Gray::from_frame(&frame, WORK_WIDTH);
            let fits = anchorings(&classify(&frame, &lattice, &gray).unwrap(), &lattice, 1280.0, 800.0);
            let id = (0..cells::CELLS as u16).find(|id| scene.cells[usize::from(*id)] == Cell::Walkable).unwrap();
            let (x, y) = cells::coords(id).unwrap();
            let (i, j) = lattice.locate(scene.centre(x, y).0, scene.centre(x, y).1);
            let truth = (x - i.round() as i32, y - j.round() as i32);
            total += 1;
            exact += usize::from(board.translation == truth);
            let off = ((board.translation.0 - truth.0).abs(), (board.translation.1 - truth.1).abs());
            if !fits.contains(&truth) || off.0 > 3 || off.1 > 3 || (board.anchoring - 1.0 / fits.len() as f32).abs() > 1e-6
            {
                failures.push((n, truth, board.translation, fits.len()));
            }
        }
        println!("translation exact on {exact} of {total} maps");
        assert!(failures.is_empty(), "{failures:?}");
    }


}
