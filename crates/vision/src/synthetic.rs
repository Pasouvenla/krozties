//! Synthetic combat frames, to measure the readers until real captures exist.
//!
//! A boss map's checkerboard drawn the way the game lays it out: walkable
//! cells in two alternating shades, obstacles that rise and hide the cells
//! behind them, holes showing the void, entities with their team ring,
//! interface bars, uneven lighting and noise. What it cannot know of the real
//! game, its colours and its decor, it draws at random in each scene, so that
//! a reader tuned to one look fails the others.

use dofus_maps::{cells, Cell};

use crate::Frame;

/// A small deterministic generator: the same seed draws the same scene.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Uniform in [low, high).
    pub fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * ((self.next() >> 40) as f32 / (1u64 << 24) as f32)
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.range(0.0, 1.0) < p
    }
}

/// What a scene shows, and where.
pub struct Scene {
    /// By game cell id, 560 of them.
    pub cells: Vec<Cell>,
    pub cell_width: f32,
    /// Where the centre of game cell (0, 0) lands, in pixels.
    pub origin: (f32, f32),
    pub size: (u32, u32),
    pub seed: u64,
    /// The game cells an entity stands on.
    pub entities: Vec<u16>,
    /// How high obstacles rise, in cell widths.
    pub obstacle_height: f32,
    /// The interface bar at the bottom, as a share of the height.
    pub bar: f32,
    /// Whether entities hide their whole cell but their team ring.
    pub covering: bool,
    /// How much the light may change across the frame, as a share of it.
    pub lighting: f32,
    /// The colour of every portrait of the timeline, if not drawn at random.
    pub portraits: Option<[f32; 3]>,
}

impl Scene {
    /// The screen centre of game coordinates (x, y).
    pub fn centre(&self, x: i32, y: i32) -> (f32, f32) {
        let l = self.cell_width;
        (self.origin.0 + (x + y) as f32 * l / 2.0, self.origin.1 + (x - y) as f32 * l / 4.0)
    }

    /// `map` centred in a frame of `size`, cells `cell_width` wide, above an
    /// interface bar of a tenth of the height, drawn by `seed`.
    pub fn centred(map: Vec<Cell>, size: (u32, u32), cell_width: f32, seed: u64) -> Scene {
        // The 560 cells span 13.5 cell widths across and 9.75 down.
        let usable = size.1 as f32 * 0.9;
        let origin = (size.0 as f32 / 2.0 - 6.75 * cell_width, usable / 2.0 - 4.875 * cell_width);
        Scene {
            cells: map,
            cell_width,
            origin,
            size,
            seed,
            entities: Vec::new(),
            obstacle_height: 0.0,
            bar: 0.1,
            covering: false,
            lighting: 0.15,
            portraits: None,
        }
    }
}

struct Canvas {
    width: usize,
    height: usize,
    rgb: Vec<[f32; 3]>,
    /// By pixel, the walkable cell whose tile shows there, if any.
    floor: Vec<Option<u16>>,
    /// The walkable cell being drawn, if any.
    drawing: Option<u16>,
}

impl Canvas {
    fn put(&mut self, x: i64, y: i64, colour: [f32; 3]) {
        if x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height {
            let at = y as usize * self.width + x as usize;
            self.rgb[at] = colour;
            self.floor[at] = self.drawing;
        }
    }

    /// A diamond centred on (cx, cy), `l` wide and `l / 2` high; `edge`, if
    /// any, colours its rim.
    fn diamond(&mut self, cx: f32, cy: f32, l: f32, fill: [f32; 3], edge: Option<[f32; 3]>) {
        let (hw, hh) = (l / 2.0, l / 4.0);
        for y in (cy - hh).floor() as i64..=(cy + hh).ceil() as i64 {
            for x in (cx - hw).floor() as i64..=(cx + hw).ceil() as i64 {
                let d = ((x as f32 + 0.5 - cx) / hw).abs() + ((y as f32 + 0.5 - cy) / hh).abs();
                if d <= 1.0 {
                    self.put(x, y, if d > 0.9 { edge.unwrap_or(fill) } else { fill });
                }
            }
        }
    }

    fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, colour: [f32; 3], ring: bool) {
        for y in (cy - ry).floor() as i64..=(cy + ry).ceil() as i64 {
            for x in (cx - rx).floor() as i64..=(cx + rx).ceil() as i64 {
                let d = ((x as f32 - cx) / rx).powi(2) + ((y as f32 - cy) / ry).powi(2);
                if d <= 1.0 && (!ring || d >= 0.6) {
                    self.put(x, y, colour);
                }
            }
        }
    }
}

fn shade(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

/// Draws the scene.
pub fn render(scene: &Scene) -> Frame {
    render_with_floor(scene).0
}

/// Draws the scene, and tells by pixel which walkable cell's tile shows there.
pub fn render_with_floor(scene: &Scene) -> (Frame, Vec<Option<u16>>) {
    let (width, height) = (scene.size.0 as usize, scene.size.1 as usize);
    let mut rng = Rng::new(scene.seed);
    let void = [rng.range(10.0, 45.0), rng.range(10.0, 45.0), rng.range(15.0, 55.0)];
    let mut canvas =
        Canvas { width, height, rgb: vec![void; width * height], floor: vec![None; width * height], drawing: None };
    // The void is mottled in blocks, as decor far below would be.
    let block = 6;
    for by in 0..height.div_ceil(block) {
        for bx in 0..width.div_ceil(block) {
            let k = rng.range(0.7, 1.3);
            for y in by * block..((by + 1) * block).min(height) {
                for x in bx * block..((bx + 1) * block).min(width) {
                    canvas.rgb[y * width + x] = shade(void, k);
                }
            }
        }
    }
    let light = [rng.range(110.0, 175.0), rng.range(110.0, 175.0), rng.range(70.0, 140.0)];
    let dark = shade(light, 1.0 - rng.range(0.08, 0.22));
    let outline = rng.chance(0.5).then(|| shade(dark, 0.8));
    let decor = [rng.range(60.0, 130.0), rng.range(45.0, 110.0), rng.range(25.0, 85.0)];
    let l = scene.cell_width;

    // Back to front: the further up the screen, the further back.
    let mut order: Vec<u16> = (0..cells::CELLS as u16).collect();
    order.sort_by_key(|&id| {
        let (x, y) = cells::coords(id).unwrap();
        (x - y, x + y)
    });
    for id in order {
        let (x, y) = cells::coords(id).unwrap();
        let (cx, cy) = scene.centre(x, y);
        match scene.cells[usize::from(id)] {
            Cell::Walkable => {
                let tile = if (x + y).rem_euclid(2) == 0 { light } else { dark };
                canvas.drawing = Some(id);
                canvas.diamond(cx, cy, l, shade(tile, rng.range(0.97, 1.03)), outline);
                canvas.drawing = None;
            }
            Cell::Obstacle => {
                let rise = scene.obstacle_height * l * rng.range(0.5, 1.0);
                let side = shade(decor, 0.65);
                let mut t = 0.0;
                while t < rise {
                    canvas.diamond(cx, cy - t, l, side, None);
                    t += 2.0;
                }
                canvas.diamond(cx, cy - rise, l, shade(decor, rng.range(0.9, 1.1)), Some(shade(decor, 0.5)));
            }
            Cell::Hole | Cell::Unknown => {}
        }
        if scene.entities.contains(&id) {
            let team = if rng.chance(0.5) { [200.0, 40.0, 40.0] } else { [40.0, 70.0, 210.0] };
            let body = [rng.range(40.0, 220.0), rng.range(40.0, 220.0), rng.range(40.0, 220.0)];
            if scene.covering {
                canvas.ellipse(cx, cy, 0.5 * l, 0.25 * l, body, false);
                canvas.ellipse(cx, cy, 0.38 * l, 0.19 * l, team, true);
            } else {
                canvas.ellipse(cx, cy, 0.38 * l, 0.19 * l, team, true);
                canvas.ellipse(cx, cy - 0.45 * l, 0.22 * l, 0.42 * l, body, false);
            }
        }
    }

    // The interface: a bar at the bottom, the timeline at the top right.
    let bar_top = ((1.0 - scene.bar) * height as f32) as usize;
    let panel = [rng.range(30.0, 60.0), rng.range(30.0, 60.0), rng.range(30.0, 60.0)];
    for y in bar_top..height {
        for x in 0..width {
            canvas.rgb[y * width + x] = panel;
            canvas.floor[y * width + x] = None;
        }
    }
    for k in 0..6 {
        let x0 = width - 60 - k * 50;
        let drawn = [rng.range(80.0, 230.0), rng.range(80.0, 230.0), 90.0];
        canvas.ellipse(x0 as f32, 40.0, 20.0, 20.0, scene.portraits.unwrap_or(drawn), false);
    }

    // Uneven lighting, then sensor-like noise.
    let (gx, gy) = (rng.range(-scene.lighting, scene.lighting), rng.range(-scene.lighting, scene.lighting));
    let noise = rng.range(2.0, 6.0);
    let mut bgra = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let light = 1.0 + gx * (x as f32 / width as f32 - 0.5) + gy * (y as f32 / height as f32 - 0.5);
            let c = canvas.rgb[y * width + x];
            let n = rng.range(-noise, noise);
            let px = |v: f32| (v * light + n).clamp(0.0, 255.0) as u8;
            bgra.extend_from_slice(&[px(c[2]), px(c[1]), px(c[0]), 255]);
        }
    }
    (Frame { window: 1, width: scene.size.0, height: scene.size.1, bgra }, canvas.floor)
}

/// The 35 boss maps of the KrozTools, by game cell: floor walkable, walls
/// obstacles, the rest of the map holes.
pub fn boss_maps() -> Vec<Vec<Cell>> {
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
            (0..cells::CELLS as u16)
                .map(|id| {
                    let (x, y) = cells::coords(id).unwrap();
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
