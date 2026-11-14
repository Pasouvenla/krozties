//! The local store of maps. It recognises a map seen before and confirms it
//! once the readings of distinct fights agree.
//!
//! A reading close enough to a known map at the same coordinates and
//! sub-area joins it; any other is a new map. The thresholds come from the 35
//! boss maps of the KrozTools: two different maps agree on up to 94 % of
//! their cells, the same map read with 5 % of its cells wrong still agrees on
//! 95 %. Below that, the safe guess is another map: several rooms of a
//! dungeon share their coordinates, and mistaking one for another would draw
//! the wrong walls.
//!
//! A frame shows where the map's cells are, not always which ones they are:
//! when the map does not fill the window, the reader may anchor its cells a
//! few cells off. A reading is compared with a map at every shift within
//! reach, and joins it at the one that lays it over the map best.
//!
//! Each map keeps the readings of its last few fights, and each cell takes
//! the kind most of them saw. A misread fight is outvoted; a game update that
//! moves a few cells wins once more fights see it than remember the old map.

use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cells::CELLS;
use crate::map::{shifted, Agreement, Cell, Reading, Seen};

/// The thresholds of recognition and confirmation.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// From this similarity on, a reading is the same map.
    pub same_map: f32,
    /// Cells known on both sides needed to compare two readings at all.
    pub min_overlap: usize,
    /// Distinct fights whose readings agree before a map is confirmed.
    pub fights_to_confirm: usize,
    /// Fights a map remembers, the most recent ones.
    pub fights_kept: usize,
    /// The share of a cell's support its kind needs for the cell to be agreed.
    pub cell_agreement: f32,
    /// The share of the known cells that must be agreed to confirm.
    pub agreed_cells: f32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            same_map: 0.95,
            min_overlap: 60,
            fights_to_confirm: 2,
            fights_kept: 3,
            cell_agreement: 2.0 / 3.0,
            agreed_cells: 0.99,
        }
    }
}

/// A map the player has fought on.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Map {
    /// Krozties' own identifier: the game shows none.
    pub id: String,
    pub coords: (i32, i32),
    pub subarea: String,
    /// One reading per fight, oldest first, the windows of a fight merged.
    readings: Vec<Reading>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Unverified,
    Confirmed,
}

impl Map {
    /// For one cell, the confidence each kind received from the readings, and
    /// how many readings knew the cell.
    fn support(&self, i: usize) -> ([f32; 4], usize) {
        let mut support = [0.0f32; 4];
        let mut seen = 0;
        for r in &self.readings {
            if let Some(k) = r.cells[i].cell.rank() {
                support[k] += r.cells[i].confidence;
                seen += 1;
            }
        }
        (support, seen)
    }

    /// Per cell, what its readings saw most (see [`vote`]).
    pub fn cells(&self) -> Vec<Seen> {
        (0..CELLS)
            .map(|i| {
                let (support, seen) = self.support(i);
                vote(support, seen)
            })
            .collect()
    }

    /// Whether enough fights read the map, and enough of its known cells have
    /// most of their support agree on whether they are walkable.
    fn confirmed(&self, settings: &Settings) -> bool {
        if self.readings.len() < settings.fights_to_confirm {
            return false;
        }
        let (mut known, mut agreed) = (0usize, 0usize);
        for i in 0..CELLS {
            let (support, _) = self.support(i);
            let (walkable, blocked) = (support[0], support[1] + support[2] + support[3]);
            if walkable + blocked <= 0.0 {
                continue;
            }
            known += 1;
            if walkable.max(blocked) / (walkable + blocked) >= settings.cell_agreement {
                agreed += 1;
            }
        }
        known > 0 && agreed as f32 >= settings.agreed_cells * known as f32
    }

    pub fn status(&self, settings: &Settings) -> Status {
        if self.confirmed(settings) {
            Status::Confirmed
        } else {
            Status::Unverified
        }
    }

    /// The distinct fights the map remembers.
    pub fn fights(&self) -> usize {
        self.readings.len()
    }

    fn absorb(&mut self, reading: Reading, settings: &Settings) {
        match self.readings.iter_mut().find(|r| r.fight == reading.fight) {
            Some(same_fight) => same_fight.merge(&reading),
            None => {
                self.readings.push(reading);
                let excess = self.readings.len().saturating_sub(settings.fights_kept.max(1));
                self.readings.drain(..excess);
            }
        }
    }
}

/// How far the store looks for the shift between a reading and a map, in
/// cells along each of the game's axes.
const REACH: i32 = 8;

/// The shift that best lays a reading over a map's cells, and how far they
/// then agree; `None` when no shift overlaps enough to compare. Best is the
/// most agreeing weight, each disagreement counting against it as much as
/// the agreements the same-map threshold allows for one: a large overlap
/// that agrees wins over a small one that agrees a little more. Among equals,
/// the smallest shift.
fn align(reading: &[Seen], map: &[Seen], settings: &Settings) -> Option<((i32, i32), Agreement)> {
    let cost = settings.same_map / (1.0 - settings.same_map).max(1e-3);
    let mut best: Option<((i32, i32), Agreement, f32)> = None;
    for dx in -REACH..=REACH {
        for dy in -REACH..=REACH {
            let Some(moved) = shifted(reading, (dx, dy)) else { continue };
            let agreement = Agreement::of(&moved, map);
            if agreement.overlap < settings.min_overlap {
                continue;
            }
            let score = agreement.same - cost * (agreement.total - agreement.same);
            let nearer = |b: (i32, i32)| dx.abs() + dy.abs() < b.0.abs() + b.1.abs();
            if best.is_none_or(|(b, _, s)| score > s || (score == s && nearer(b))) {
                best = Some(((dx, dy), agreement, score));
            }
        }
    }
    best.map(|(shift, agreement, _)| (shift, agreement))
}

/// A cell from the confidence each kind received and the number of readings
/// that knew it: walkable or not as the confidence has it; not walkable, an
/// obstacle or a hole as the readings that told them apart have it, if any
/// did. Its confidence is what it received on average, so that a disputed or
/// unsure cell stays unsure.
fn vote(support: [f32; 4], seen: usize) -> Seen {
    let (walkable, blocked) = (support[0], support[1] + support[2] + support[3]);
    if seen == 0 || walkable + blocked <= 0.0 {
        return Seen::UNKNOWN;
    }
    if walkable > blocked {
        return Seen::new(Cell::Walkable, walkable / seen as f32);
    }
    let kind = match support[1].total_cmp(&support[2]) {
        std::cmp::Ordering::Greater => Cell::Obstacle,
        std::cmp::Ordering::Less => Cell::Hole,
        std::cmp::Ordering::Equal => Cell::NotWalkable,
    };
    Seen::new(kind, blocked / seen as f32)
}

/// The frames of one fight gathered into one reading: each cell takes what
/// most frames saw, so that a frame where a fighter hides a tile does not
/// undo the others.
#[derive(Clone, Debug)]
pub struct Gathering {
    support: Vec<[f32; 4]>,
    seen: Vec<usize>,
    frames: usize,
}

impl Default for Gathering {
    fn default() -> Gathering {
        Gathering { support: vec![[0.0; 4]; CELLS], seen: vec![0; CELLS], frames: 0 }
    }
}

impl Gathering {
    /// Adds a frame's cells, laid over the frames before at the shift that
    /// fits them best: the board may anchor two frames differently. `None`,
    /// and nothing added, when no shift agrees as the same map would, or the
    /// frame does not have the 560 cells. The first frame sets the reference.
    pub fn add(&mut self, cells: &[Seen], settings: &Settings) -> Option<(i32, i32)> {
        if cells.len() != CELLS {
            return None;
        }
        let shift = if self.frames == 0 {
            (0, 0)
        } else {
            let (shift, agreement) = align(cells, &self.cells(), settings)?;
            if agreement.share() < settings.same_map {
                return None;
            }
            shift
        };
        for (i, seen) in shifted(cells, shift)?.iter().enumerate() {
            if let Some(k) = seen.cell.rank() {
                self.support[i][k] += seen.confidence;
                self.seen[i] += 1;
            }
        }
        self.frames += 1;
        Some(shift)
    }

    /// The frames gathered.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Per cell, what most frames saw.
    pub fn cells(&self) -> Vec<Seen> {
        (0..CELLS).map(|i| vote(self.support[i], self.seen[i])).collect()
    }
}

/// A map the store knows, as a reading finds it.
#[derive(Clone, Debug)]
pub struct Known {
    pub map: String,
    /// Cell (x, y) of the reading is cell (x + dx, y + dy) of the map.
    pub shift: (i32, i32),
    /// The map's cells.
    pub cells: Vec<Seen>,
    pub verified: bool,
    /// The distinct fights the map remembers.
    pub fights: usize,
}

/// What the store made of a reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    /// No known map matched: a new, unverified map.
    Created,
    /// The reading joined a known map.
    Joined,
    /// The reading made its map confirmed.
    Confirmed,
}

/// The outcome of a reading, and the cells to use for this fight.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub map: String,
    pub event: Event,
    /// The map's cells, this fight's reading included.
    pub cells: Vec<Seen>,
    /// Whether the map is confirmed.
    pub verified: bool,
    /// Cell (x, y) of the reading is cell (x + dx, y + dy) of the map.
    pub shift: (i32, i32),
}

/// The maps of one player, on their machine.
pub struct Store {
    settings: Settings,
    maps: Vec<Map>,
}

const FORMAT: &str = "krozties-maps";

#[derive(Deserialize)]
struct File {
    format: String,
    maps: Vec<Map>,
}

#[derive(Serialize)]
struct FileRef<'a> {
    format: &'a str,
    version: u32,
    maps: &'a [Map],
}

impl Store {
    pub fn new(settings: Settings) -> Store {
        Store { settings, maps: Vec::new() }
    }

    /// The store saved at `path`, or an empty one if there is none yet.
    pub fn load(path: &Path, settings: Settings) -> io::Result<Store> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let file: File = serde_json::from_str(&text).map_err(io::Error::other)?;
                if file.format != FORMAT {
                    return Err(io::Error::other(format!("{} is not a Krozties map store", path.display())));
                }
                Ok(Store { settings, maps: file.maps })
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Store::new(settings)),
            Err(e) => Err(e),
        }
    }

    /// Saves the store, through a temporary file renamed over the old one: a
    /// crash midway leaves the previous store whole.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let text = serde_json::to_string(&FileRef { format: FORMAT, version: 1, maps: &self.maps })?;
        let temporary = path.with_extension("tmp");
        {
            let mut file = std::fs::File::create(&temporary)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
        }
        std::fs::rename(&temporary, path)
    }

    pub fn maps(&self) -> &[Map] {
        &self.maps
    }

    /// The map a reading is, among those at its place, and the shift that
    /// lays the reading over it: the closest, if close enough; first, the map
    /// another window of the same fight joined, when it is the same map: two
    /// windows taken for one fight may be two fights.
    fn closest(&self, reading: &Reading) -> Option<(usize, (i32, i32))> {
        let subarea = normalise(&reading.subarea);
        let candidates =
            (0..self.maps.len()).filter(|&m| self.maps[m].coords == reading.coords && normalise(&self.maps[m].subarea) == subarea);
        let settings = self.settings;
        let fits = |m: usize| {
            let (shift, agreement) = align(&reading.cells, &self.maps[m].cells(), &settings)?;
            (agreement.share() >= settings.same_map).then_some((m, shift, agreement.share()))
        };
        let same_fight = candidates
            .clone()
            .filter(|&m| self.maps[m].readings.iter().any(|r| r.fight == reading.fight))
            .find_map(fits)
            .map(|(m, shift, _)| (m, shift));
        same_fight.or_else(|| candidates.filter_map(fits).max_by(|a, b| a.2.total_cmp(&b.2)).map(|(m, shift, _)| (m, shift)))
    }

    /// The known map a reading is, if any, the reading left out of it: what
    /// a fight going on shows while its own reading is not done.
    pub fn recognise(&self, reading: &Reading) -> Option<Known> {
        if !reading.is_complete() {
            return None;
        }
        let (m, shift) = self.closest(reading)?;
        let map = &self.maps[m];
        Some(Known {
            map: map.id.clone(),
            shift,
            cells: map.cells(),
            verified: map.confirmed(&self.settings),
            fights: map.fights(),
        })
    }

    /// Adds a fight's reading, and says which cells to use for that fight.
    pub fn add(&mut self, reading: Reading) -> Result<Outcome, String> {
        if !reading.is_complete() {
            return Err(format!("a reading has {} cells instead of {CELLS}", reading.cells.len()));
        }
        let settings = self.settings;
        let target = self.closest(&reading);

        let Some((m, shift)) = target else {
            let mut map = Map {
                id: uuid::Uuid::new_v4().to_string(),
                coords: reading.coords,
                subarea: reading.subarea.clone(),
                readings: Vec::new(),
            };
            map.absorb(reading, &settings);
            let outcome =
                Outcome { map: map.id.clone(), event: Event::Created, cells: map.cells(), verified: false, shift: (0, 0) };
            self.maps.push(map);
            return Ok(outcome);
        };
        let mut reading = reading;
        reading.cells = shifted(&reading.cells, shift).unwrap_or(reading.cells);
        let map = &mut self.maps[m];
        let was_confirmed = map.confirmed(&settings);
        map.absorb(reading, &settings);
        let verified = map.confirmed(&settings);
        let event = if verified && !was_confirmed { Event::Confirmed } else { Event::Joined };
        Ok(Outcome { map: map.id.clone(), event, cells: map.cells(), verified, shift })
    }
}

/// A sub-area name as the store compares it: no case, no accents, single
/// spaces.
fn normalise(name: &str) -> String {
    let mut flat = String::with_capacity(name.len());
    for c in name.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' | 'á' => flat.push('a'),
            'é' | 'è' | 'ê' | 'ë' => flat.push('e'),
            'î' | 'ï' | 'í' => flat.push('i'),
            'ô' | 'ö' | 'ó' => flat.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => flat.push('u'),
            'ç' => flat.push('c'),
            'œ' => flat.push_str("oe"),
            'æ' => flat.push_str("ae"),
            c if c.is_alphanumeric() => flat.push(c),
            _ => flat.push(' '),
        }
    }
    flat.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cells::coords;

    /// A map of the 560 cells, mostly walkable, with obstacles and holes laid
    /// out by `seed`.
    fn grid(seed: u32) -> Vec<Cell> {
        let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
        (0..CELLS)
            .map(|_| {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                match (state >> 16) % 10 {
                    0 => Cell::Obstacle,
                    1 => Cell::Hole,
                    _ => Cell::Walkable,
                }
            })
            .collect()
    }

    /// A fight's reading of `cells`: sure of every cell, except the ones in
    /// `misread`, read as walls with little confidence.
    fn reading(fight: &str, cells: &[Cell], misread: &[usize]) -> Reading {
        Reading {
            fight: fight.into(),
            coords: (4, -12),
            subarea: "Plaine des Scarafeuilles".into(),
            cells: cells
                .iter()
                .enumerate()
                .map(|(i, c)| if misread.contains(&i) { Seen::new(Cell::Obstacle, 0.3) } else { Seen::new(*c, 0.9) })
                .collect(),
            at_ms: 0,
        }
    }

    fn kinds(cells: &[Seen]) -> Vec<Cell> {
        cells.iter().map(|s| s.cell).collect()
    }

    /// `cells` with every `step`-th cell turned to `into`.
    fn changed(cells: &[Cell], step: usize, into: Cell) -> Vec<Cell> {
        cells.iter().enumerate().map(|(i, c)| if i % step == 0 { into } else { *c }).collect()
    }

    #[test]
    fn a_map_is_confirmed_by_a_second_fight_despite_misreadings() {
        let truth = grid(1);
        let mut store = Store::new(Settings::default());
        let first = store.add(reading("a", &truth, &[3, 40])).unwrap();
        assert_eq!((first.event, first.verified), (Event::Created, false));
        let second = store.add(reading("b", &truth, &[100])).unwrap();
        assert_eq!((second.event, second.verified, second.map.as_str()), (Event::Confirmed, true, first.map.as_str()));
        assert_eq!(kinds(&second.cells), truth, "the sure readings outweigh the unsure misreadings");
        assert_eq!(store.maps().len(), 1);
        assert_eq!(store.maps()[0].status(&Settings::default()), Status::Confirmed);
    }

    #[test]
    fn the_windows_of_one_fight_count_once() {
        let truth = grid(2);
        let mut store = Store::new(Settings::default());
        store.add(reading("a", &truth, &[])).unwrap();
        let again = store.add(reading("a", &truth, &[])).unwrap();
        assert_eq!((again.event, again.verified), (Event::Joined, false));
        assert_eq!(store.maps()[0].fights(), 1);
    }

    #[test]
    fn another_room_or_sub_area_is_another_map() {
        let mut store = Store::new(Settings::default());
        store.add(reading("a", &grid(3), &[])).unwrap();
        assert_eq!(store.add(reading("b", &grid(4), &[])).unwrap().event, Event::Created);
        let mut elsewhere = reading("c", &grid(3), &[]);
        elsewhere.subarea = "Cania".into();
        assert_eq!(store.add(elsewhere).unwrap().event, Event::Created);
        assert_eq!(store.maps().len(), 3);
        // Case, accents and spacing do not make another sub-area.
        let mut same = reading("d", &grid(3), &[]);
        same.subarea = "  plaine des  SCARAFEUILLES ".into();
        assert_eq!(store.add(same).unwrap().event, Event::Confirmed);
    }

    /// A fight that misreads a seventh of the map is too far from it: it is
    /// kept as another, unverified map, and the confirmed map does not move.
    #[test]
    fn a_badly_misread_fight_does_not_touch_a_confirmed_map() {
        let truth = grid(5);
        let mut store = Store::new(Settings::default());
        store.add(reading("a", &truth, &[])).unwrap();
        let confirmed = store.add(reading("b", &truth, &[])).unwrap().map;
        let odd = store.add(reading("c", &changed(&truth, 7, Cell::Obstacle), &[])).unwrap();
        assert_eq!((odd.event, odd.verified), (Event::Created, false));
        let map = store.maps().iter().find(|m| m.id == confirmed).unwrap();
        assert_eq!(kinds(&map.cells()), truth);
        assert_eq!(map.status(&Settings::default()), Status::Confirmed);
    }

    /// A fight that misreads a few cells with confidence joins the map, and is
    /// outvoted on them.
    #[test]
    fn a_slightly_misread_fight_is_outvoted() {
        let truth = grid(6);
        let mut store = Store::new(Settings::default());
        store.add(reading("a", &truth, &[])).unwrap();
        store.add(reading("b", &truth, &[])).unwrap();
        let odd = store.add(reading("c", &changed(&truth, 50, Cell::Hole), &[])).unwrap();
        assert_eq!((odd.event, odd.verified), (Event::Joined, true));
        assert_eq!(kinds(&odd.cells), truth);
    }

    /// The game moves a few cells: once two fights saw the change, the map
    /// remembers more fights with it than without, and follows.
    #[test]
    fn the_map_follows_a_change_seen_in_two_fights() {
        let truth = grid(7);
        let update = changed(&truth, 50, Cell::Hole);
        let mut store = Store::new(Settings::default());
        store.add(reading("a", &truth, &[])).unwrap();
        store.add(reading("b", &truth, &[])).unwrap();
        store.add(reading("c", &update, &[])).unwrap();
        let after = store.add(reading("d", &update, &[])).unwrap();
        assert_eq!((after.event, after.verified), (Event::Joined, true));
        assert_eq!(kinds(&after.cells), update);
        assert_eq!(store.maps()[0].fights(), 3);
    }

    #[test]
    fn a_reading_too_partial_to_compare_matches_nothing() {
        let truth = grid(8);
        let mut store = Store::new(Settings::default());
        store.add(reading("a", &truth, &[])).unwrap();
        let mut partial = reading("b", &truth, &[]);
        for s in partial.cells.iter_mut().skip(40) {
            *s = Seen::UNKNOWN;
        }
        assert_eq!(store.add(partial).unwrap().event, Event::Created);
    }

    #[test]
    fn the_store_survives_its_file() {
        let truth = grid(9);
        let path = std::env::temp_dir().join(format!("krozties-maps-{}.json", std::process::id()));
        let mut store = Store::new(Settings::default());
        let first = store.add(reading("a", &truth, &[])).unwrap();
        store.save(&path).unwrap();
        let mut back = Store::load(&path, Settings::default()).unwrap();
        let second = back.add(reading("b", &truth, &[])).unwrap();
        assert_eq!((second.map, second.event), (first.map, Event::Confirmed));
        assert!(Store::load(&path.with_extension("missing"), Settings::default()).unwrap().maps().is_empty());
        std::fs::write(&path, r#"{"format":"autre","maps":[]}"#).unwrap();
        assert!(Store::load(&path, Settings::default()).is_err());
        let _ = std::fs::remove_file(&path);
    }

    /// The 35 boss maps of the KrozTools, by name and game cell: floor
    /// walkable, walls obstacles, the rest of the map a hole.
    fn boss_maps() -> Vec<(String, Vec<Cell>)> {
        #[derive(Deserialize)]
        struct Survey {
            cartes: Vec<BossMap>,
        }
        #[derive(Deserialize)]
        struct BossMap {
            nom: String,
            lignes: Vec<String>,
        }
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/cartes/insightroom.json");
        let survey: Survey = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        survey
            .cartes
            .into_iter()
            .map(|map| {
                let cells = (0..CELLS)
                    .map(|id| {
                        let (x, y) = coords(id as u16).unwrap();
                        let row: Vec<char> = map.lignes[(13 - y) as usize].chars().collect();
                        match row[x as usize] {
                            '.' => Cell::Walkable,
                            '#' => Cell::Obstacle,
                            _ => Cell::Hole,
                        }
                    })
                    .collect();
                (map.nom, cells)
            })
            .collect()
    }

    /// Versions of one boss fought in the same arena: one map, as far as the
    /// floor goes.
    const SAME_ARENA: [&[&str]; 2] = [&["Balladone", "Belladona", "Belladone 3.0"], &["Kralamoure Géant", "Kralamour 2.0"]];

    fn same_arena(a: &str, b: &str) -> bool {
        SAME_ARENA.iter().any(|group| group.contains(&a) && group.contains(&b))
    }

    /// A boss map as the board reads it with the whole map in view: the
    /// floor surely, the other cells not walkable, less surely beyond the
    /// floor.
    fn as_read(map: &[Cell]) -> Vec<Seen> {
        let floor: Vec<(i32, i32)> =
            (0..CELLS).filter(|&i| map[i] == Cell::Walkable).map(|i| coords(i as u16).unwrap()).collect();
        (0..CELLS)
            .map(|i| {
                let (x, y) = coords(i as u16).unwrap();
                let row = floor.iter().any(|f| f.1 == y && f.0 < x) && floor.iter().any(|f| f.1 == y && f.0 > x);
                let column = floor.iter().any(|f| f.0 == x && f.1 < y) && floor.iter().any(|f| f.0 == x && f.1 > y);
                match map[i] {
                    Cell::Walkable => Seen::new(Cell::Walkable, 0.9),
                    _ if row && column => Seen::new(Cell::NotWalkable, 0.7),
                    _ => Seen::new(Cell::NotWalkable, 0.5),
                }
            })
            .collect()
    }

    /// `cells` with one cell in fifty of the floor and around it misread.
    fn misread(cells: &[Seen]) -> Vec<Seen> {
        cells
            .iter()
            .enumerate()
            .map(|(k, s)| match s.cell {
                Cell::Walkable if k % 50 == 0 => Seen::new(Cell::NotWalkable, s.confidence),
                Cell::NotWalkable if k % 50 == 0 && s.confidence > 0.5 => Seen::new(Cell::Walkable, s.confidence),
                _ => *s,
            })
            .collect()
    }

    /// The threshold against real maps read as the board reads them, and
    /// anchored a few cells off: no boss map reaches the map of another arena
    /// at any shift; each boss map read again with one cell in fifty of the
    /// floor and around it wrong, and moved, is laid back where it was and
    /// reaches it.
    #[test]
    fn the_same_map_threshold_holds_at_any_shift() {
        let settings = Settings::default();
        let bosses = boss_maps();
        assert_eq!(bosses.len(), 35);
        let maps: Vec<Vec<Seen>> = bosses.iter().map(|(_, m)| as_read(m)).collect();
        let (mut closest, mut farthest, mut fixed) = (0.0f32, 1.0f32, 0);
        for (i, a) in maps.iter().enumerate() {
            for (j, b) in maps.iter().enumerate() {
                if i == j || same_arena(&bosses[i].0, &bosses[j].0) {
                    continue;
                }
                if let Some((_, agreement)) = align(a, b, &settings) {
                    closest = closest.max(agreement.share());
                }
            }
            // A floor that spans the map cannot be anchored elsewhere.
            let moves = [(2, -1), (-3, 2), (1, 3), (-1, -2), (4, 0), (0, -4), (1, 0), (0, 1), (-1, 0), (0, -1)];
            let Some((moved, by)) = moves.iter().find_map(|&by| shifted(&misread(a), by).map(|m| (m, by))) else {
                fixed += 1;
                continue;
            };
            let (back, agreement) = align(&moved, a, &settings).expect("an overlap");
            assert_eq!(back, (-by.0, -by.1), "map {i} laid back elsewhere");
            farthest = farthest.min(agreement.share());
        }
        println!("closest other arena {closest:.3}, farthest same map {farthest:.3}, {fixed} floors span the map");
        assert!(closest < settings.same_map, "another map reaches {closest}");
        assert!(farthest >= settings.same_map, "the same map falls to {farthest}");
    }

    /// The same map read again, anchored elsewhere: it joins and confirms the
    /// map at the shift that lays it back, and says that shift.
    #[test]
    fn a_reading_anchored_elsewhere_joins_its_map() {
        let (truth, by) = boss_maps()
            .iter()
            .map(|(_, m)| as_read(m))
            .find_map(|t| shifted(&t, (-2, 3)).map(|_| (t, (-2, 3))))
            .expect("a floor with room to move");
        let first = Reading { fight: "a".into(), coords: (4, -12), subarea: "Ruche".into(), cells: truth.clone(), at_ms: 0 };
        let second = Reading { fight: "b".into(), cells: shifted(&truth, by).unwrap(), ..first.clone() };
        let mut store = Store::new(Settings::default());
        store.add(first).unwrap();
        let outcome = store.add(second).unwrap();
        assert_eq!((outcome.event, outcome.shift), (Event::Confirmed, (-by.0, -by.1)));
        assert_eq!(kinds(&outcome.cells), kinds(&truth));
        assert_eq!(store.maps().len(), 1);
    }

    /// A large overlap that agrees but for two cells wins over a small one
    /// that agrees entirely: elsewhere on the map, a stretch of open floor
    /// matches any stretch of open floor.
    #[test]
    fn a_large_overlap_wins_over_a_small_perfect_one() {
        let at = |(x, y): (i32, i32)| usize::from(crate::cells::id_at(x, y).unwrap());
        let band = |low: i32, high: i32, wide: bool| {
            (0..CELLS as u16).map(|id| coords(id).unwrap()).filter(move |&(x, y)| {
                (low..=high).contains(&(x - y)) && (wide || (2..=25).contains(&(x + y)))
            })
        };
        // The reading sees the lower rows: a stretch of floor and its edges.
        let mut reading = vec![Seen::UNKNOWN; CELLS];
        for cell in band(0, 13, true) {
            reading[at(cell)] = Seen::new(Cell::NotWalkable, 0.7);
        }
        for cell in band(2, 11, false) {
            reading[at(cell)] = Seen::new(Cell::Walkable, 0.9);
        }
        // The map knows the same, and open floor sixteen half-rows further.
        let mut map = reading.clone();
        for cell in band(18, 23, false) {
            map[at(cell)] = Seen::new(Cell::Walkable, 0.9);
        }
        // Two misreadings, that a shift onto the far floor leaves out.
        for cell in band(8, 11, false).step_by(20).take(2) {
            reading[at(cell)] = Seen::new(Cell::NotWalkable, 0.9);
        }
        let (shift, agreement) = align(&reading, &map, &Settings::default()).unwrap();
        assert_eq!(shift, (0, 0), "laid over the far floor: {agreement:?}");
    }

    /// A cell most readings see as not walkable is not walkable, though they
    /// do not agree on what blocks it.
    #[test]
    fn a_cell_is_walkable_or_not_by_all_its_readings() {
        let first = Reading { fight: "a".into(), coords: (0, 0), subarea: "s".into(), cells: vec![Seen::new(Cell::Walkable, 0.9); CELLS], at_ms: 0 };
        let mut map = Map { id: "m".into(), coords: (0, 0), subarea: "s".into(), readings: vec![first.clone()] };
        let blocked = |kind: Cell| Reading { cells: vec![Seen::new(kind, 0.6); CELLS], ..first.clone() };
        map.readings.push(Reading { fight: "b".into(), ..blocked(Cell::Obstacle) });
        map.readings.push(Reading { fight: "c".into(), ..blocked(Cell::NotWalkable) });
        assert!(map.cells().iter().all(|s| s.cell == Cell::Obstacle), "{:?}", map.cells()[0]);
    }

    /// Two fights that agree on the floor confirm the map, though one tells
    /// the walls from the holes and the other does not.
    #[test]
    fn walls_named_by_one_fight_only_do_not_stop_confirmation() {
        let truth = grid(12);
        let mut store = Store::new(Settings::default());
        let vague: Vec<Cell> =
            truth.iter().map(|c| if *c == Cell::Walkable { Cell::Walkable } else { Cell::NotWalkable }).collect();
        store.add(reading("a", &vague, &[])).unwrap();
        let second = store.add(reading("b", &truth, &[])).unwrap();
        assert_eq!((second.event, second.verified), (Event::Confirmed, true));
    }

    /// Frames of a fight anchored differently are laid over each other; a
    /// frame where most of the floor is read wrong is outvoted by the others;
    /// a frame of another map is refused.
    #[test]
    fn the_frames_of_a_fight_are_gathered_by_vote() {
        let settings = Settings::default();
        let bosses = boss_maps();
        // A boss map whose floor leaves room to move it.
        let (truth, moved, by) = bosses
            .iter()
            .map(|(_, m)| as_read(m))
            .find_map(|t| [(1, 0), (0, 1), (-1, 0), (0, -1)].iter().find_map(|&by| shifted(&t, by).map(|m| (t.clone(), m, by))))
            .expect("room to move a floor");
        let mut gathering = Gathering::default();
        assert_eq!(gathering.add(&truth, &settings), Some((0, 0)));
        assert_eq!(gathering.add(&moved, &settings), Some((-by.0, -by.1)));
        // A frame where a tenth of the floor is hidden, read not walkable but
        // close enough to be the same map.
        let mut hidden = truth.clone();
        let floor: Vec<usize> = (0..CELLS).filter(|&i| truth[i].cell == Cell::Walkable).collect();
        for &i in floor.iter().step_by(40) {
            hidden[i] = Seen::new(Cell::NotWalkable, 0.7);
        }
        assert!(gathering.add(&hidden, &settings).is_some());
        assert_eq!(kinds(&gathering.cells()), kinds(&truth));
        assert_eq!(gathering.frames(), 3);
        let other = as_read(&bosses[17].1);
        assert_eq!(gathering.add(&other, &settings), None);
        assert_eq!(gathering.frames(), 3);
    }

    /// A known map is recognised without the reading joining it; an unknown
    /// one is not.
    #[test]
    fn a_map_is_recognised_without_being_changed() {
        let bosses = boss_maps();
        let (n, truth, moved, by) = bosses
            .iter()
            .enumerate()
            .find_map(|(n, (_, m))| {
                let t = as_read(m);
                [(1, 0), (0, 1), (-1, 0), (0, -1)].iter().find_map(|&by| shifted(&t, by).map(|m| (n, t.clone(), m, by)))
            })
            .expect("room to move a floor");
        let at = |cells: Vec<Seen>, fight: &str| Reading { fight: fight.into(), coords: (2, 2), subarea: "s".into(), cells, at_ms: 0 };
        let mut store = Store::new(Settings::default());
        let first = store.add(at(truth, "a")).unwrap();
        let known = store.recognise(&at(moved, "b")).expect("the map known");
        assert_eq!((known.map.as_str(), known.shift, known.verified, known.fights), (first.map.as_str(), (-by.0, -by.1), false, 1));
        assert_eq!(store.maps()[0].fights(), 1, "recognising adds nothing");
        let other = (0..bosses.len()).find(|&k| k != n && !same_arena(&bosses[k].0, &bosses[n].0)).unwrap();
        assert!(store.recognise(&at(as_read(&bosses[other].1), "c")).is_none());
    }

    /// Two windows taken for one fight that show two maps make two maps.
    #[test]
    fn one_fight_on_two_maps_is_two_maps() {
        let bosses = boss_maps();
        let mut store = Store::new(Settings::default());
        let at = |cells: Vec<Seen>| Reading { fight: "same".into(), coords: (1, 1), subarea: "s".into(), cells, at_ms: 0 };
        let first = store.add(at(as_read(&bosses[2].1))).unwrap();
        let second = store.add(at(as_read(&bosses[25].1))).unwrap();
        assert_eq!(second.event, Event::Created);
        assert_ne!(first.map, second.map);
    }

    #[test]
    fn an_incomplete_reading_is_refused() {
        let mut short = reading("a", &grid(10), &[]);
        short.cells.truncate(10);
        assert!(Store::new(Settings::default()).add(short).is_err());
    }
}
