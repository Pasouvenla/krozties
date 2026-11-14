//! Finding the combat checkerboard on a frame.
//!
//! In the game's coordinates the cells are unit squares: cell (x, y) covers
//! x ± 1/2 and y ± 1/2. On screen, x grows along (L/2, L/4) and y along
//! (L/2, −L/4), for cells L pixels wide. So every border of the checkerboard
//! lies on a line X + 2Y = constant (between x and x + 1) or X − 2Y =
//! constant (between y and y + 1), and the lines of each family are L apart
//! along it. Projecting the strength of the edges that face each family onto
//! its axis gives a profile that repeats every L: its period is the cell
//! width, its phase where the cells sit.
//!
//! Coordinates are continuous, the centre of pixel (x, y) at (x + ½, y + ½).

use crate::gray::Gray;
use crate::Frame;

/// Where the checkerboard's cells are on a frame. Cell (i, j) of the lattice
/// has its centre at X = x0 + (i + j) L / 2, Y = y0 + (i − j) L / 4, in frame
/// pixels; i and j follow the game's x and y, up to a whole translation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lattice {
    pub cell_width: f32,
    pub x0: f32,
    pub y0: f32,
    /// How sharply the edges line up, from 0 to 1.
    pub confidence: f32,
}

impl Lattice {
    pub fn centre(&self, i: i32, j: i32) -> (f32, f32) {
        let l = self.cell_width;
        (self.x0 + (i + j) as f32 * l / 2.0, self.y0 + (i - j) as f32 * l / 4.0)
    }

    /// The point at fractional lattice coordinates.
    pub fn point(&self, i: f32, j: f32) -> (f32, f32) {
        let l = self.cell_width;
        (self.x0 + (i + j) * l / 2.0, self.y0 + (i - j) * l / 4.0)
    }

    /// The fractional lattice coordinates of a point.
    pub fn locate(&self, x: f32, y: f32) -> (f32, f32) {
        let (dx, dy) = (x - self.x0, y - self.y0);
        ((dx + 2.0 * dy) / self.cell_width, (dx - 2.0 * dy) / self.cell_width)
    }
}

/// The cell widths sought, as shares of the frame width: the whole map spans
/// 13.5 cell widths, so filling the width means cells of 1/13.5 of it.
const MIN_SHARE: f32 = 1.0 / 40.0;
const MAX_SHARE: f32 = 1.0 / 6.0;
/// Frames are measured at most this wide.
const WORK_WIDTH: usize = 1280;

/// The checkerboard of a frame, if it shows one.
pub fn find(frame: &Frame) -> Option<Lattice> {
    let (gray, factor) = Gray::from_frame(frame, WORK_WIDTH);
    let found = find_on(&gray)?;
    let f = factor as f32;
    Some(Lattice { cell_width: found.cell_width * f, x0: found.x0 * f, y0: found.y0 * f, ..found })
}

/// The two edge profiles, each value the mean edge strength of its line.
struct Profiles {
    /// Indexed by p = X + 2Y on pixel corners.
    p: Vec<f32>,
    /// Indexed by q = X − 2Y + 2 (height − 1).
    q: Vec<f32>,
}

fn profiles(gray: &Gray) -> Profiles {
    let (w, h) = (gray.width, gray.height);
    let len = w + 2 * h;
    let (mut p, mut q) = (vec![0.0f32; len], vec![0.0f32; len]);
    let (mut np, mut nq) = (vec![0u32; len], vec![0u32; len]);
    let s5 = 5f32.sqrt();
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let at = |dx: isize, dy: isize| gray.at((x as isize + dx) as usize, (y as isize + dy) as usize);
            let gx = at(1, -1) + 2.0 * at(1, 0) + at(1, 1) - at(-1, -1) - 2.0 * at(-1, 0) - at(-1, 1);
            let gy = at(-1, 1) + 2.0 * at(0, 1) + at(1, 1) - at(-1, -1) - 2.0 * at(0, -1) - at(1, -1);
            // Facing the lines X + 2Y = c: along their normal (1, 2), not
            // along their direction (2, −1). Likewise for X − 2Y = c.
            let facing = |along: f32, across: f32| {
                let (a2, b2) = (along * along, across * across);
                along.abs() * (a2 / (a2 + b2 + 1e-6)).powi(2)
            };
            let bp = x + 2 * y;
            p[bp] += facing((gx + 2.0 * gy) / s5, (2.0 * gx - gy) / s5);
            np[bp] += 1;
            let bq = x + 2 * (h - 1) - 2 * y;
            q[bq] += facing((gx - 2.0 * gy) / s5, (2.0 * gx + gy) / s5);
            nq[bq] += 1;
        }
    }
    // The mean per line: the lines through the middle of the frame are longer.
    let mean = |v: &mut Vec<f32>, n: &[u32]| {
        for (s, c) in v.iter_mut().zip(n) {
            *s = if *c >= 8 { *s / *c as f32 } else { 0.0 };
        }
    };
    mean(&mut p, &np);
    mean(&mut q, &nq);
    Profiles { p, q }
}

/// The profile's Fourier coefficient at `period`, mean removed.
fn coefficient(profile: &[f32], mean: f32, period: f32) -> (f32, f32) {
    let w = std::f32::consts::TAU / period;
    let (mut re, mut im) = (0.0f32, 0.0f32);
    for (b, v) in profile.iter().enumerate() {
        if *v == 0.0 {
            continue;
        }
        let (s, c) = (w * b as f32).sin_cos();
        re += (v - mean) * c;
        im -= (v - mean) * s;
    }
    (re, im)
}

fn power(profiles: &Profiles, means: (f32, f32), period: f32) -> f32 {
    let (a, b) = coefficient(&profiles.p, means.0, period);
    let (c, d) = coefficient(&profiles.q, means.1, period);
    a * a + b * b + c * c + d * d
}

fn mean_of(profile: &[f32]) -> f32 {
    let lit: Vec<f32> = profile.iter().copied().filter(|v| *v != 0.0).collect();
    lit.iter().sum::<f32>() / lit.len().max(1) as f32
}

/// Box sums of the luminance, for the mean of any rectangle in four reads.
struct Integral {
    width: usize,
    height: usize,
    sums: Vec<f64>,
}

impl Integral {
    fn of(gray: &Gray) -> Integral {
        let (w, h) = (gray.width, gray.height);
        let mut sums = vec![0.0f64; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut row = 0.0f64;
            for x in 0..w {
                row += f64::from(gray.at(x, y));
                sums[(y + 1) * (w + 1) + x + 1] = sums[y * (w + 1) + x + 1] + row;
            }
        }
        Integral { width: w, height: h, sums }
    }

    /// The mean of the pixels within `r` of (cx, cy) across and `r / 2` down,
    /// or `None` off the plane.
    fn mean(&self, cx: f32, cy: f32, r: f32) -> Option<f32> {
        let (x0, x1) = ((cx - r).round() as i64, (cx + r).round() as i64);
        let (y0, y1) = ((cy - r / 2.0).round() as i64, (cy + r / 2.0).round() as i64);
        if x0 < 0 || y0 < 0 || x1 > self.width as i64 || y1 > self.height as i64 || x1 <= x0 || y1 <= y0 {
            return None;
        }
        let (x0, x1, y0, y1) = (x0 as usize, x1 as usize, y0 as usize, y1 as usize);
        let w = self.width + 1;
        let s = self.sums[y1 * w + x1] - self.sums[y0 * w + x1] - self.sums[y1 * w + x0] + self.sums[y0 * w + x0];
        Some((s / ((x1 - x0) * (y1 - y0)) as f64) as f32)
    }
}

/// How much of the frame the lattice explains as a checkerboard: the size, in
/// cells, of the largest connected patch of cells that each stand out from
/// all four neighbours the way their parity says. Across each border, a point
/// just inside each side: a light cell of the checkerboard is the lighter side
/// of all four of its borders, a dark cell the darker side of all four. The
/// floor is one large such patch. Elsewhere a cell agrees with its four
/// borders one time in eight, with its parity half of those times, and these
/// cells scatter: chance patches stay a few cells wide. Off the borders by
/// more than the step taken, both points of a border fall in the same cell;
/// on a lattice of the wrong size, three times too wide included, borders
/// cross the middle of the cells: no patch either way.
fn patch(integral: &Integral, lattice: &Lattice) -> Vec<(i32, i32)> {
    const STEP: f32 = 0.15;
    let r = lattice.cell_width / 16.0;
    let (w, h) = (integral.width as f32, integral.height as f32);
    let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)].map(|(x, y)| lattice.locate(x, y));
    let span = |pick: fn(&(f32, f32)) -> f32| {
        let values = corners.iter().map(pick);
        (values.clone().fold(f32::MAX, f32::min).floor() as i32, values.fold(f32::MIN, f32::max).ceil() as i32)
    };
    let ((imin, imax), (jmin, jmax)) = (span(|c| c.0), span(|c| c.1));
    let (ni, nj) = ((imax - imin + 1) as usize, (jmax - jmin + 1) as usize);
    let lum = |i: f32, j: f32| {
        let (x, y) = lattice.point(i, j);
        integral.mean(x, y, r)
    };
    // Each cell's vote: +1 or −1 when all four borders agree, by which parity
    // they agree with; 0 otherwise.
    let mut vote = vec![0i8; ni * nj];
    for i in imin..=imax {
        for j in jmin..=jmax {
            let (fi, fj) = (i as f32, j as f32);
            let mut sides = 0i32;
            let mut whole = true;
            for (di, dj) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let inside = lum(fi + di * (0.5 - STEP), fj + dj * (0.5 - STEP));
                let beyond = lum(fi + di * (0.5 + STEP), fj + dj * (0.5 + STEP));
                match (inside, beyond) {
                    (Some(a), Some(b)) if (a - b).abs() >= 0.5 => sides += if a > b { 1 } else { -1 },
                    _ => whole = false,
                }
            }
            if whole && sides.abs() == 4 {
                let even = (i + j).rem_euclid(2) == 0;
                vote[(i - imin) as usize * nj + (j - jmin) as usize] = if (sides > 0) == even { 1 } else { -1 };
            }
        }
    }
    // The largest 4-connected patch of equal votes.
    let mut seen = vec![false; ni * nj];
    let mut largest: Vec<(i32, i32)> = Vec::new();
    let mut stack = Vec::new();
    for start in 0..ni * nj {
        if vote[start] == 0 || seen[start] {
            continue;
        }
        let (mut members, sign) = (Vec::new(), vote[start]);
        seen[start] = true;
        stack.push(start);
        while let Some(at) = stack.pop() {
            let (ci, cj) = (at / nj, at % nj);
            members.push((ci as i32 + imin, cj as i32 + jmin));
            for (di, dj) in [(1isize, 0isize), (-1, 0), (0, 1), (0, -1)] {
                let (ti, tj) = (ci as isize + di, cj as isize + dj);
                if ti < 0 || tj < 0 || ti >= ni as isize || tj >= nj as isize {
                    continue;
                }
                let next = ti as usize * nj + tj as usize;
                if !seen[next] && vote[next] == sign {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        if members.len() > largest.len() {
            largest = members;
        }
    }
    largest
}

/// The lattice measured on the borders between the patch's cells, the floor:
/// across each, the luminance along a short segment, and where it changes
/// most, to a hundredth of a cell; then the lattice that puts every border
/// where it was measured, by least squares. Decor never enters: only borders
/// between two cells of the patch are measured.
fn measure_borders(gray: &Gray, lattice: &Lattice, patch: &[(i32, i32)]) -> Option<Lattice> {
    const REACH: f32 = 0.2;
    const STEPS: i32 = 20;
    let floor: std::collections::HashSet<(i32, i32)> = patch.iter().copied().collect();
    let mut rows: Vec<(f32, bool, f32, f32)> = Vec::new();
    for &(i, j) in patch {
        for (di, dj, is_p) in [(1, 0, true), (0, 1, false)] {
            if !floor.contains(&(i + di, j + dj)) {
                continue;
            }
            let across = |t: f32| {
                let (x, y) = lattice.point(i as f32 + di as f32 * (0.5 + t), j as f32 + dj as f32 * (0.5 + t));
                gray.sample(x, y)
            };
            let values: Option<Vec<f32>> = (-STEPS..=STEPS).map(|k| across(k as f32 * REACH / STEPS as f32)).collect();
            let Some(values) = values else {
                continue;
            };
            // The steepest change, then a parabola through it and its two
            // neighbours.
            let slopes: Vec<f32> = values.windows(3).map(|w| (w[2] - w[0]).abs()).collect();
            let (k, steepest) = slopes.iter().enumerate().fold((0, 0.0f32), |b, (k, s)| if *s > b.1 { (k, *s) } else { b });
            if steepest < 2.0 || k == 0 || k + 1 >= slopes.len() {
                continue;
            }
            let (before, after) = (slopes[k - 1], slopes[k + 1]);
            let bend = before - 2.0 * steepest + after;
            let shift = if bend.abs() > 1e-6 { (0.5 * (before - after) / bend).clamp(-0.5, 0.5) } else { 0.0 };
            // slopes[k] is centred on values[k + 1], at t = (k + 1 − STEPS) step.
            let t = (k as f32 + 1.0 + shift - STEPS as f32) * REACH / STEPS as f32;
            let border = if is_p { i } else { j } as f32 + 0.5;
            // In the family's coordinate, this border sits at (border + t) L
            // past the lattice's origin line.
            let measured = (border + t) * lattice.cell_width;
            rows.push((border, is_p, measured, steepest));
        }
    }
    if rows.len() < 12 {
        return None;
    }
    // measured = border × L' + offset of its family, offsets relative to the
    // current lattice's origin lines.
    let mut a = [[0.0f64; 3]; 3];
    let mut b = [0.0f64; 3];
    for (border, is_p, measured, w) in &rows {
        let r = [f64::from(*border), if *is_p { 1.0 } else { 0.0 }, if *is_p { 0.0 } else { 1.0 }];
        for m in 0..3 {
            b[m] += f64::from(*w) * r[m] * f64::from(*measured);
            for n in 0..3 {
                a[m][n] += f64::from(*w) * r[m] * r[n];
            }
        }
    }
    let [l, dp, dq] = solve3(a, b)?;
    let (l, dp, dq) = (l as f32, dp as f32, dq as f32);
    // The origin lines move by dp and dq along their families: p0 + dp and q0
    // + dq, with p = X + 2Y and q = X − 2Y.
    let (p0, q0) = (lattice.x0 + 2.0 * lattice.y0 + dp, lattice.x0 - 2.0 * lattice.y0 + dq);
    Some(Lattice { cell_width: l, x0: (p0 + q0) / 2.0, y0: (p0 - q0) / 4.0, confidence: lattice.confidence })
}

/// The lattice whose borders sit at `edge_p` and `edge_q` on the profiles.
fn lattice_from(gray: &Gray, period: f32, edge_p: f32, edge_q: f32) -> Lattice {
    // Centres sit half a cell from the borders. The bins index pixels: p = X
    // + 2Y at the pixel's centre is the bin plus 3/2; q is the bin minus 2
    // (height − 1) minus ½.
    let p0 = edge_p + period / 2.0 + 1.5;
    let q0 = edge_q + period / 2.0 - 2.0 * (gray.height as f32 - 1.0) - 0.5;
    Lattice { cell_width: period, x0: (p0 + q0) / 2.0, y0: (p0 - q0) / 4.0, confidence: 0.0 }
}

/// Gaussian elimination with partial pivoting; `None` if singular.
fn solve3(mut a: [[f64; 3]; 3], mut b: [f64; 3]) -> Option<[f64; 3]> {
    for col in 0..3 {
        let pivot = (col..3).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-9 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..3 {
            let f = a[row][col] / a[col][col];
            for k in col..3 {
                a[row][k] -= f * a[col][k];
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = [0.0f64; 3];
    for row in (0..3).rev() {
        let s: f64 = (row + 1..3).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - s) / a[row][row];
    }
    Some(x)
}

/// Below this patch, in cells, the frame shows no checkerboard: on twenty
/// frames without one, chance reached 17; the smallest floor of the boss
/// maps, 136 cells, makes a patch of 65.
const MIN_SCORE: f32 = 30.0;
/// The patch at which the checkerboard is beyond doubt.
const SURE_SCORE: f32 = 80.0;
/// Phases tried per period, along each family of borders.
const PHASES: usize = 16;
/// Spectrum peaks tried as cell widths.
const PEAKS: usize = 5;

fn find_on(gray: &Gray) -> Option<Lattice> {
    search(gray, MIN_SCORE)
}

/// The best lattice of the plane, if its score reaches `threshold`.
fn search(gray: &Gray, threshold: f32) -> Option<Lattice> {
    let profiles = profiles(gray);
    let means = (mean_of(&profiles.p), mean_of(&profiles.q));
    let (low, high) = (gray.width as f32 * MIN_SHARE, gray.width as f32 * MAX_SHARE);
    let strongest = |from: f32, to: f32, step: f32| {
        let mut best = (from, 0.0f32);
        let mut t = from;
        while t <= to {
            let e = power(&profiles, means, t);
            if e > best.1 {
                best = (t, e);
            }
            t += step;
        }
        best
    };
    // The strongest peaks of the spectrum, each refined.
    let mut spectrum = Vec::new();
    let mut t = low;
    while t <= high {
        spectrum.push((t, power(&profiles, means, t)));
        t += 0.5;
    }
    let mut peaks: Vec<(f32, f32)> =
        (1..spectrum.len().saturating_sub(1)).filter(|&k| spectrum[k].1 > spectrum[k - 1].1 && spectrum[k].1 >= spectrum[k + 1].1).map(|k| spectrum[k]).collect();
    peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut periods: Vec<f32> = Vec::new();
    for (peak, _) in peaks.into_iter().take(PEAKS) {
        let (peak, _) = strongest(peak - 0.6, peak + 0.6, 0.05);
        for multiple in [0.5, 1.0, 2.0, 3.0] {
            let period = peak * multiple;
            if period >= low && period <= high && !periods.iter().any(|p| (p - period).abs() < period * 0.01) {
                periods.push(period);
            }
        }
    }

    // The edges propose periods, the checkerboard decides. Raised decor drawn
    // in blocks puts its own borders off the floor's, so the strongest peak
    // may be a fraction or a multiple of the cell: each peak, its half and
    // its multiples are tried at every phase.
    let integral = Integral::of(gray);
    let mut best: Option<(f32, f32, f32, f32)> = None;
    for period in periods {
        for dp in 0..PHASES {
            for dq in 0..PHASES {
                let (ep, eq) = (dp as f32 * period / PHASES as f32, dq as f32 * period / PHASES as f32);
                let score = patch(&integral, &lattice_from(gray, period, ep, eq)).len() as f32;
                if best.is_none_or(|b| score > b.0) {
                    best = Some((score, period, ep, eq));
                }
            }
        }
    }
    let (score, period, ep, eq) = best?;
    if score < threshold {
        return None;
    }
    let chosen = lattice_from(gray, period, ep, eq);
    // The patch finds the checkerboard but places it only to a fraction of a
    // cell: a lattice slightly off may even keep more cells. The borders
    // between the patch's cells place it, measured again on each refined
    // lattice, as long as the checkerboard is not lost and the period stays.
    let floor = patch(&integral, &chosen);
    let mut lattice = chosen;
    for _ in 0..4 {
        let Some(measured) = measure_borders(gray, &lattice, &floor) else {
            break;
        };
        if (measured.cell_width - chosen.cell_width).abs() > chosen.cell_width * 0.05
            || (patch(&integral, &measured).len() as f32) < 0.3 * score
        {
            break;
        }
        lattice = measured;
    }
    Some(Lattice { confidence: (score / SURE_SCORE).min(1.0), ..lattice })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic::{boss_maps, render, Rng, Scene};
    use dofus_maps::{cells, Cell};

    /// Whether the lattice puts every visible walkable cell of the scene on a
    /// lattice centre, within a tenth of a cell. Returns the worst offset.
    fn worst_offset(scene: &Scene, lattice: &Lattice) -> f32 {
        let mut worst = 0.0f32;
        let bar = (1.0 - scene.bar) * scene.size.1 as f32;
        for id in 0..cells::CELLS as u16 {
            if scene.cells[usize::from(id)] != Cell::Walkable {
                continue;
            }
            let (x, y) = cells::coords(id).unwrap();
            let (cx, cy) = scene.centre(x, y);
            if cx < 0.0 || cy < 0.0 || cx > scene.size.0 as f32 || cy > bar {
                continue;
            }
            let (i, j) = lattice.locate(cx, cy);
            worst = worst.max((i - i.round()).abs()).max((j - j.round()).abs());
        }
        worst
    }

    /// Every boss map, its cells between 55 and 95 pixels wide on a 1280 by
    /// 800 frame, under random colours, lighting, outlines, obstacles from flat
    /// to half a cell high, and a few entities.
    #[test]
    fn the_checkerboard_is_found_on_every_boss_map() {
        let mut rng = Rng::new(7);
        let mut failures = Vec::new();
        for (n, map) in boss_maps().into_iter().enumerate() {
            let width = rng.range(55.0, 95.0);
            let mut scene = Scene::centred(map, (1280, 800), width, n as u64 + 1);
            scene.origin.0 += rng.range(-0.5, 0.5) * width;
            scene.origin.1 += rng.range(-0.5, 0.5) * width;
            scene.obstacle_height = rng.range(0.0, 0.5);
            let walkable: Vec<u16> =
                (0..cells::CELLS as u16).filter(|id| scene.cells[usize::from(*id)] == Cell::Walkable).collect();
            scene.entities = (0..5).map(|k| walkable[(k * 37 + n) % walkable.len()]).collect();
            let frame = render(&scene);
            match find(&frame) {
                Some(lattice) => {
                    let offset = worst_offset(&scene, &lattice);
                    if (lattice.cell_width - width).abs() > 0.5 || offset > 0.1 {
                        failures.push(format!("map {n}: width {width:.2} read {:.2}, offset {offset:.3}", lattice.cell_width));
                    }
                }
                None => failures.push(format!("map {n}: no checkerboard found")),
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// Twenty frames without a checkerboard, mottled void and walls scattered
    /// at random, some raised, find none.
    #[test]
    fn no_checkerboard_no_lattice() {
        let mut rng = Rng::new(99);
        for seed in 0..20u64 {
            let map: Vec<Cell> = (0..cells::CELLS).map(|_| if rng.chance(0.15) { Cell::Obstacle } else { Cell::Hole }).collect();
            let mut scene = Scene::centred(map, (1280, 800), rng.range(55.0, 95.0), 1000 + seed);
            scene.obstacle_height = rng.range(0.0, 0.5);
            assert_eq!(find(&render(&scene)), None, "seed {seed}");
        }
    }

    /// The frame size does not matter: a 2560-wide frame is measured at half
    /// size and the lattice scaled back.
    #[test]
    fn a_large_frame_is_read_at_half_size() {
        let map = boss_maps().swap_remove(4);
        let scene = Scene::centred(map, (2560, 1600), 160.0, 11);
        let lattice = find(&render(&scene)).expect("a checkerboard");
        assert!((lattice.cell_width - 160.0).abs() < 1.0, "{lattice:?}");
        assert!(worst_offset(&scene, &lattice) < 0.1, "{lattice:?}");
    }
}




#[cfg(test)]
mod measure {
    use super::*;
    use crate::synthetic::{boss_maps, render, Rng, Scene};
    use dofus_maps::{cells, Cell};

    #[test]
    #[ignore]
    fn measure_on_unseen_draws() {
        let (mut total, mut ok, mut widths, mut offsets, mut times) = (0, 0, Vec::new(), Vec::new(), Vec::new());
        for draw in [101u64, 202, 303, 404, 505] {
            let mut rng = Rng::new(draw);
            for (n, map) in boss_maps().into_iter().enumerate() {
                let width = rng.range(55.0, 95.0);
                let mut scene = Scene::centred(map, (1280, 800), width, draw * 100 + n as u64);
                scene.origin.0 += rng.range(-0.5, 0.5) * width;
                scene.origin.1 += rng.range(-0.5, 0.5) * width;
                scene.obstacle_height = rng.range(0.0, 0.5);
                let walkable: Vec<u16> = (0..cells::CELLS as u16).filter(|id| scene.cells[usize::from(*id)] == Cell::Walkable).collect();
                scene.entities = (0..5).map(|k| walkable[(k * 37 + n) % walkable.len()]).collect();
                let frame = render(&scene);
                let start = std::time::Instant::now();
                let found = find(&frame);
                times.push(start.elapsed().as_secs_f32() * 1000.0);
                total += 1;
                let Some(l) = found else { continue };
                let mut worst = 0.0f32;
                for id in walkable.iter() {
                    let (x, y) = cells::coords(*id).unwrap();
                    let (cx, cy) = scene.centre(x, y);
                    if cx < 0.0 || cy < 0.0 || cx > 1280.0 || cy > 720.0 { continue; }
                    let (i, j) = l.locate(cx, cy);
                    worst = worst.max((i - i.round()).abs()).max((j - j.round()).abs());
                }
                widths.push((l.cell_width - width).abs());
                offsets.push(worst);
                if (l.cell_width - width).abs() <= 0.5 && worst <= 0.1 { ok += 1; }
            }
        }
        let max = |v: &Vec<f32>| v.iter().copied().fold(0.0f32, f32::max);
        let mean = |v: &Vec<f32>| v.iter().sum::<f32>() / v.len().max(1) as f32;
        println!("scenes {total}, correct {ok}, found {}", widths.len());
        println!("width error mean {:.3} max {:.3} px; worst offset mean {:.3} max {:.3} cell", mean(&widths), max(&widths), mean(&offsets), max(&offsets));
        println!("time per frame mean {:.0} max {:.0} ms", mean(&times), max(&times));
    }
}
