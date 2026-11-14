//! The overlay laid over the game window during a fight: what Krozties read
//! of the map, drawn where the game draws it. Nothing on the floor, which the
//! game already shows; the cells that cannot be walked tinted: slate with a
//! copper edge for an obstacle, blue with a dotted edge for a hole, a light
//! veil and a question mark for a cell not yet told apart; the map's state at
//! the bottom right while it is not verified. Layout B, chosen by the player
//! from three mockups.
//!
//! It never takes a click: clicks go through to the game. It shows only while
//! the companion is open, during a fight, over the game window in front.

use std::collections::HashSet;

use dofus_maps::{cells, Cell, Known, Seen};
use dofus_vision::fight::View;
use dofus_vision::read::board::Board;
use dofus_vision::Rect;
use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// The overlay window's label.
pub const LABEL: &str = "calque";
/// The event the page draws from.
const EVENT: &str = "calque";

/// What the page draws.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Drawing {
    pub cells: Vec<DrawnCell>,
    /// The map's state, while it is not verified.
    pub badge: Option<String>,
}

/// A cell to draw.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DrawnCell {
    /// Its top, right, bottom and left corners, in window points.
    pub points: [[f32; 2]; 4],
    /// "obstacle", "hole" or "undecided".
    pub kind: &'static str,
}

/// What to draw over a frame `size` pixels wide and high, at `scale` frame
/// pixels per window point: the cells of the known map if the fight's map is
/// known, else those the fight has read so far. Only the cells that cannot
/// be walked within the floor's hull or touching the floor: the others, out
/// of reach, would only cover the game.
pub fn drawing(board: &Board, view: &View, known: Option<&Known>, scale: f32, size: (f32, f32)) -> Drawing {
    // How a cell of the source becomes a cell of the board: the board lies
    // over the view at `view.shift`, the view over the map at `known.shift`.
    let (source, to_board): (&[Seen], (i32, i32)) = match known {
        Some(k) => (&k.cells, (-k.shift.0 - view.shift.0, -k.shift.1 - view.shift.1)),
        None => (&view.cells, (-view.shift.0, -view.shift.1)),
    };
    let at = |id: usize| u16::try_from(id).ok().and_then(cells::coords);
    let floor: HashSet<(i32, i32)> =
        source.iter().enumerate().filter(|(_, s)| s.cell == Cell::Walkable).filter_map(|(id, _)| at(id)).collect();
    let shape = cells::hull(floor.iter().copied().collect());
    let near = |(x, y): (i32, i32)| {
        cells::within(&shape, (x, y)) || (-1..=1).any(|dx| (-1..=1).any(|dy| floor.contains(&(x + dx, y + dy))))
    };
    let mut drawn = Vec::new();
    for (id, seen) in source.iter().enumerate() {
        let kind = match seen.cell {
            Cell::Obstacle => "obstacle",
            Cell::Hole => "hole",
            Cell::NotWalkable => "undecided",
            Cell::Walkable | Cell::Unknown => continue,
        };
        let Some((x, y)) = at(id).filter(|&c| near(c)) else { continue };
        let corners = board.diamond((x + to_board.0, y + to_board.1));
        if !corners.iter().any(|&(px, py)| px >= 0.0 && py >= 0.0 && px < size.0 && py < size.1) {
            continue;
        }
        drawn.push(DrawnCell { points: corners.map(|(px, py)| [px / scale, py / scale]), kind });
    }
    let badge = match known {
        Some(k) if k.verified => None,
        Some(k) => Some(state(k.fights + 1)),
        None => Some(state(1)),
    };
    Drawing { cells: drawn, badge }
}

/// The badge of a map not verified, at its `fights`-th fight.
fn state(fights: usize) -> String {
    let n = fights.min(2);
    format!("Carte non vérifiée · {n} combat{} sur 2", if n > 1 { "s" } else { "" })
}

/// Lays the overlay over the game window `at`, in points, and draws on it.
pub fn show(app: &AppHandle, at: &Rect, drawing: &Drawing, moved: bool) {
    let window = match app.get_webview_window(LABEL) {
        Some(window) => window,
        None => match create(app) {
            Some(window) => window,
            None => return,
        },
    };
    if moved {
        let _ = window.set_position(LogicalPosition::new(at.x, at.y));
        let _ = window.set_size(LogicalSize::new(at.width, at.height));
    }
    let _ = app.emit_to(LABEL, EVENT, drawing);
    if !window.is_visible().unwrap_or(false) {
        let _ = window.show();
    }
}

/// Takes the overlay off the game.
pub fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.hide();
    }
}

fn create(app: &AppHandle) -> Option<WebviewWindow> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("calque.html".into()))
        .title("Krozties, calque")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        .resizable(false)
        .visible(false)
        .build()
        .ok()?;
    let _ = window.set_ignore_cursor_events(true);
    #[cfg(target_os = "macos")]
    {
        let overlay = window.clone();
        let _ = app.run_on_main_thread(move || {
            if let Ok(ns_window) = overlay.ns_window() {
                // SAFETY: the overlay's live NSWindow, on the main thread.
                unsafe { panel::below_the_companion(ns_window) };
            }
            let _ = overlay.set_ignore_cursor_events(true);
        });
    }
    Some(window)
}

/// Over a game in full screen on macOS, as the companion is, one level below
/// it: the companion stays above the overlay.
#[cfg(target_os = "macos")]
mod panel {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    const BELOW_THE_COMPANION: isize = 999;

    /// # Safety
    /// `ns_window` is the overlay's live NSWindow, on the main thread.
    pub unsafe fn below_the_companion(ns_window: *mut std::ffi::c_void) {
        unsafe {
            if crate::compagnon::plein_ecran::en_panneau(ns_window) {
                let window = ns_window.cast::<AnyObject>();
                let _: () = msg_send![window, setLevel: BELOW_THE_COMPANION];
                let _: () = msg_send![window, setIgnoresMouseEvents: true];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dofus_vision::read::lattice::Lattice;

    /// A board of lattice cells 64 pixels wide, cell (0, 0) at (100, 200),
    /// its reading anchored at (10, -5).
    fn board() -> Board {
        Board {
            lattice: Lattice { cell_width: 64.0, x0: 100.0, y0: 200.0, confidence: 1.0 },
            translation: (10, -5),
            cells: vec![Seen::UNKNOWN; cells::CELLS],
            anchoring: 1.0,
        }
    }

    fn at(cells: &mut [Seen], (x, y): (i32, i32), seen: Seen) {
        cells[usize::from(cells::id_at(x, y).unwrap())] = seen;
    }

    /// A floor of walkable cells around (x, y), `reach` cells each way.
    fn floor(cells: &mut [Seen], (x, y): (i32, i32), reach: i32) {
        for dx in -reach..=reach {
            for dy in -reach..=reach {
                at(cells, (x + dx, y + dy), Seen::new(Cell::Walkable, 0.9));
            }
        }
    }

    /// The cells read in the fight go where the board puts them, shifted as
    /// the view says, at the window's scale; the floor is not drawn, nor what
    /// lies out of its reach; a map read for the first time says so.
    #[test]
    fn the_fight_s_cells_are_drawn_where_the_board_puts_them() {
        let mut read = vec![Seen::UNKNOWN; cells::CELLS];
        floor(&mut read, (13, -4), 3);
        at(&mut read, (12, -4), Seen::new(Cell::Obstacle, 0.8));
        at(&mut read, (13, -4), Seen::new(Cell::Hole, 0.8));
        at(&mut read, (14, -4), Seen::new(Cell::NotWalkable, 0.7));
        // Out of the floor's reach.
        at(&mut read, (25, 0), Seen::new(Cell::NotWalkable, 0.5));
        // The board's cell (11, -4) is the view's (12, -4).
        let view = View { cells: read, shift: (1, 0) };
        let drawing = drawing(&board(), &view, None, 2.0, (2000.0, 2000.0));
        let kinds: Vec<&str> = drawing.cells.iter().map(|c| c.kind).collect();
        assert_eq!(kinds, ["obstacle", "hole", "undecided"]);
        // Board cell (11, -4) is lattice cell (1, 1): centre (164, 200).
        assert_eq!(drawing.cells[0].points, [[82.0, 92.0], [98.0, 100.0], [82.0, 108.0], [66.0, 100.0]]);
        assert_eq!(drawing.badge.as_deref(), Some("Carte non vérifiée · 1 combat sur 2"));
    }

    /// A known map is drawn instead, through both shifts; verified, without
    /// a badge; cells off the frame are left out.
    #[test]
    fn a_known_map_is_drawn_through_both_shifts() {
        let mut map = vec![Seen::UNKNOWN; cells::CELLS];
        floor(&mut map, (14, -2), 1);
        floor(&mut map, (22, 0), 1);
        at(&mut map, (14, -2), Seen::new(Cell::Obstacle, 0.9));
        at(&mut map, (22, 0), Seen::new(Cell::Hole, 0.9));
        let view = View { cells: vec![Seen::UNKNOWN; cells::CELLS], shift: (1, 0) };
        let known = Known { map: "m".into(), shift: (2, 1), cells: map, verified: true, fights: 3 };
        let drawing = drawing(&board(), &view, Some(&known), 1.0, (400.0, 400.0));
        // Map cell (14, -2) is view cell (12, -3), board cell (11, -3),
        // lattice cell (1, 2): centre (196, 184).
        assert_eq!(drawing.cells.len(), 1, "{drawing:?}");
        assert_eq!(drawing.cells[0].points[1], [228.0, 184.0]);
        assert_eq!(drawing.badge, None);
        let unverified = Known { verified: false, fights: 1, ..known };
        let drawing = super::drawing(&board(), &View { cells: Vec::new(), shift: (1, 0) }, Some(&unverified), 1.0, (400.0, 400.0));
        assert_eq!(drawing.badge.as_deref(), Some("Carte non vérifiée · 2 combats sur 2"));
    }
}
