//! A spell whose damage grows "on recovering the spell".
//!
//! "Les dommages du sort sont augmentés à la récupération du sort" is effect
//! 293 with a delay equal to the relaunch interval and a duration of 1: a single
//! bonus arriving when the spell comes back, not a stack, and it cannot arrive
//! before a first cast because nothing has recovered yet.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn iop() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/iop.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-8.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn casts_of(spell: &str, deck: &[&str], horizon: u8) -> Vec<(u8, f64)> {
    let build = Build {
        name: "Iop".into(),
        profile: DamageProfile {
            power: 150,
            flat_crit_damage: 100,
            elements: [ElementStats {
                characteristic: 700,
                flat_damage: 90,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 20,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        horizon,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&iop(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .flat_map(|t| {
            t.casts
                .iter()
                .filter(|c| c.id == spell)
                .map(move |c| (t.turn, c.damage.as_f64()))
        })
        .collect()
}

/// The first cast carries no bonus; later ones do.
#[test]
fn the_first_cast_has_no_recovery_bonus() {
    let casts = casts_of("colere_de_iop", &["colere_de_iop", "pression"], 8);
    assert!(
        casts.len() >= 2,
        "need at least two casts to compare, got {casts:?}"
    );
    let premier = casts[0].1;
    let suivant = casts[1].1;
    assert!(
        suivant > premier,
        "the second cast must be the stronger one: {premier} then {suivant}"
    );
    assert!(
        casts[0].0 == 1,
        "the first cast should be available immediately: {casts:?}"
    );
}

/// The bonus does not stack: every cast after the first is worth the same.
#[test]
fn the_recovery_bonus_does_not_stack() {
    let casts = casts_of("colere_de_iop", &["colere_de_iop", "pression"], 12);
    assert!(casts.len() >= 3, "need three casts, got {casts:?}");
    let apres: Vec<f64> = casts[1..].iter().map(|(_, d)| *d).collect();
    for fenetre in apres.windows(2) {
        assert!(
            (fenetre[0] - fenetre[1]).abs() < 0.5,
            "casts after the first must all be equal, got {apres:?}"
        );
    }
}

/// Casts are spaced by the relaunch interval, not crammed into one turn: a
/// spell carrying a relaunch interval has no unlimited per-turn cast count.
#[test]
fn a_spell_with_a_relaunch_interval_is_cast_once_per_turn() {
    let rs = iop();
    let fautifs: Vec<&str> = rs
        .spells
        .iter()
        .filter(|s| s.cooldown_turns > 0 && s.casts_per_turn > 1)
        .map(|s| s.id.as_str())
        .collect();
    assert!(
        fautifs.is_empty(),
        "these carry both a relaunch interval and several casts a turn: {fautifs:?}"
    );
}
