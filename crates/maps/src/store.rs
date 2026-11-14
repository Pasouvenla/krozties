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
//! Each map keeps the readings of its last few fights, and each cell takes
//! the kind most of them saw. A misread fight is outvoted; a game update that
//! moves a few cells wins once more fights see it than remember the old map.

use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cells::CELLS;
use crate::map::{similarity, Cell, Reading, Seen};

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
    fn support(&self, i: usize) -> ([f32; 3], usize) {
        let mut support = [0.0f32; 3];
        let mut seen = 0;
        for r in &self.readings {
            if let Some(k) = r.cells[i].cell.rank() {
                support[k] += r.cells[i].confidence;
                seen += 1;
            }
        }
        (support, seen)
    }

    /// Per cell, the kind that received the most confidence. Its confidence is
    /// what it received on average from the readings that knew the cell, so
    /// that a disputed or unsure cell stays unsure.
    pub fn cells(&self) -> Vec<Seen> {
        (0..CELLS)
            .map(|i| {
                let (support, seen) = self.support(i);
                let (k, best) = support.iter().enumerate().fold((0, 0.0f32), |b, (k, s)| if *s > b.1 { (k, *s) } else { b });
                if seen == 0 || best <= 0.0 {
                    Seen::UNKNOWN
                } else {
                    Seen::new(Cell::KNOWN[k], best / seen as f32)
                }
            })
            .collect()
    }

    /// Whether enough fights read the map, and enough of its known cells have
    /// a kind most of their support agrees on.
    fn confirmed(&self, settings: &Settings) -> bool {
        if self.readings.len() < settings.fights_to_confirm {
            return false;
        }
        let (mut known, mut agreed) = (0usize, 0usize);
        for i in 0..CELLS {
            let (support, _) = self.support(i);
            let total: f32 = support.iter().sum();
            if total <= 0.0 {
                continue;
            }
            known += 1;
            if support.iter().copied().fold(0.0f32, f32::max) / total >= settings.cell_agreement {
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

    /// Adds a fight's reading, and says which cells to use for that fight.
    pub fn add(&mut self, reading: Reading) -> Result<Outcome, String> {
        if !reading.is_complete() {
            return Err(format!("a reading has {} cells instead of {CELLS}", reading.cells.len()));
        }
        let subarea = normalise(&reading.subarea);
        let candidates =
            (0..self.maps.len()).filter(|&m| self.maps[m].coords == reading.coords && normalise(&self.maps[m].subarea) == subarea);
        // Another window of a fight already read joins that fight's map;
        // otherwise the closest map, if close enough.
        let same_fight = candidates.clone().find(|&m| self.maps[m].readings.iter().any(|r| r.fight == reading.fight));
        let target = same_fight.or_else(|| {
            candidates
                .filter_map(|m| {
                    let (share, overlap) = similarity(&reading.cells, &self.maps[m].cells());
                    (overlap >= self.settings.min_overlap && share >= self.settings.same_map).then_some((m, share))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(m, _)| m)
        });

        let settings = self.settings;
        let Some(m) = target else {
            let mut map = Map {
                id: uuid::Uuid::new_v4().to_string(),
                coords: reading.coords,
                subarea: reading.subarea.clone(),
                readings: Vec::new(),
            };
            map.absorb(reading, &settings);
            let outcome = Outcome { map: map.id.clone(), event: Event::Created, cells: map.cells(), verified: false };
            self.maps.push(map);
            return Ok(outcome);
        };
        let map = &mut self.maps[m];
        let was_confirmed = map.confirmed(&settings);
        map.absorb(reading, &settings);
        let verified = map.confirmed(&settings);
        let event = if verified && !was_confirmed { Event::Confirmed } else { Event::Joined };
        Ok(Outcome { map: map.id.clone(), event, cells: map.cells(), verified })
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

    /// The 35 boss maps of the KrozTools, by game cell: floor walkable, walls
    /// obstacles, the rest of the map a hole.
    fn boss_maps() -> Vec<Vec<Cell>> {
        #[derive(Deserialize)]
        struct Survey {
            cartes: Vec<BossMap>,
        }
        #[derive(Deserialize)]
        struct BossMap {
            lignes: Vec<String>,
        }
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/cartes/insightroom.json");
        let survey: Survey = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        survey
            .cartes
            .iter()
            .map(|map| {
                (0..CELLS)
                    .map(|id| {
                        let (x, y) = crate::cells::coords(id as u16).unwrap();
                        let row: Vec<char> = map.lignes[(13 - y) as usize].chars().collect();
                        match row[x as usize] {
                            '.' => Cell::Walkable,
                            '#' => Cell::Obstacle,
                            _ => Cell::Hole,
                        }
                    })
                    .collect()
            })
            .collect()
    }

    fn sure(cells: &[Cell]) -> Vec<Seen> {
        cells.iter().map(|c| Seen::new(*c, 0.9)).collect()
    }

    /// The threshold that tells maps apart, against real maps: no two boss
    /// maps reach it, and each boss map read with one cell in fifty wrong, as
    /// surely as the rest, still does.
    #[test]
    fn the_same_map_threshold_holds_on_the_boss_maps() {
        let settings = Settings::default();
        let maps = boss_maps();
        assert_eq!(maps.len(), 35);
        for (i, a) in maps.iter().enumerate() {
            for b in &maps[i + 1..] {
                assert!(similarity(&sure(a), &sure(b)).0 < settings.same_map);
            }
            let misread: Vec<Cell> =
                a.iter().enumerate().map(|(k, c)| if k % 50 == 0 { if *c == Cell::Walkable { Cell::Hole } else { Cell::Walkable } } else { *c }).collect();
            assert!(similarity(&sure(a), &sure(&misread)).0 >= settings.same_map);
        }
    }

    #[test]
    fn an_incomplete_reading_is_refused() {
        let mut short = reading("a", &grid(10), &[]);
        short.cells.truncate(10);
        assert!(Store::new(Settings::default()).add(short).is_err());
    }
}
