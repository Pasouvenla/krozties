//! Combat maps as Krozties learns them from the screen: the 560 cells of a
//! Dofus map, what each cell is, and the local store that recognises a map
//! seen before and confirms it over several fights.
//!
//! Everything stays on the player's machine: no server, no account and no
//! telemetry. Each player reads each map once, and a map becomes confirmed
//! once the readings of distinct fights agree.

pub mod cells;
mod map;
pub mod sight;
mod store;

pub use map::{Cell, Reading, Seen};
pub use store::{Event, Map, Outcome, Settings, Status, Store};
