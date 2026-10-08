//! Charges whose steps are not all worth the same.
//!
//! Read off the computed tooltip of Crâ's Flèche Punitive on one build: 454-501
//! with no charge, 737-784 at one, 1114-1162 at two. The gaps are 283 then 377,
//! and their ratio is exactly 32/24: the two values the game data carries for
//! effect 293, at delays 1 and 2. The ratio settles it, being invariant on one
//! build. Modelled as one flat amount per step, the second charge would add 24
//! again instead of 32.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn cra() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/cra.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-9.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn casts(spell: &str, deck: &[&str]) -> Vec<f64> {
    let build = Build {
        name: "Crâ".into(),
        profile: DamageProfile {
            power: 200,
            flat_crit_damage: 120,
            elements: [ElementStats {
                characteristic: 700,
                flat_damage: 90,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 30,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 8,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&cra(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .flat_map(|t| {
            t.casts
                .iter()
                .filter(|c| c.id == spell)
                .map(|c| c.damage.as_f64())
        })
        .collect()
}

/// The two steps are worth 24 and 32, so the second gap is a third larger than
/// the first. Checked as a ratio, which survives any build.
#[test]
fn punitive_steps_are_not_equal() {
    let d = casts("fleche_punitive", &["fleche_punitive", "fleche_glacee"]);
    assert!(d.len() >= 3, "need three casts, got {d:?}");
    let premier = d[1] - d[0];
    let second = d[2] - d[1];
    assert!(
        premier > 0.0 && second > 0.0,
        "both gaps must be positive: {d:?}"
    );
    let ratio = second / premier;
    let attendu = 32.0 / 24.0;
    assert!(
        (ratio - attendu).abs() < 0.02,
        "second gap over first is {ratio:.3}, expected {attendu:.3} (32/24): {d:?}"
    );
}

/// Flèche d'Expiation's steps ARE equal, at 36 each. Same shape of test, so a
/// change that flattened Punitive would not pass by looking like Expiation.
#[test]
fn expiation_steps_are_equal() {
    let d = casts(
        "fleche_d_expiation",
        &["fleche_d_expiation", "fleche_glacee"],
    );
    assert!(d.len() >= 3, "need three casts, got {d:?}");
    let premier = d[1] - d[0];
    let second = d[2] - d[1];
    assert!(
        (second / premier - 1.0).abs() < 0.02,
        "Expiation's steps are constant: {d:?}"
    );
}

/// A charge has to survive the spell's own relaunch interval, or it is never
/// reached. Expiation recharges in two turns and its stack was first written
/// with a one-turn duration, so it expired before the next cast could use it.
#[test]
fn a_charge_outlives_the_relaunch_interval() {
    let rs = cra();
    for (sort, ressource) in [
        ("fleche_d_expiation", "expiation_enchainee"),
        ("fleche_punitive", "punitive_enchainee"),
    ] {
        let s = rs.spells.iter().find(|s| s.id == sort).expect(sort);
        let r = rs
            .resources
            .iter()
            .find(|r| r.id == ressource)
            .expect(ressource);
        let duree = r.duration.as_ref().map_or(u8::MAX, |d| d.turns);
        assert!(
            duree > s.cooldown_turns,
            "{ressource} lasts {duree} turns while {sort} recharges in {}: the \
             charge expires before it can ever be used",
            s.cooldown_turns
        );
    }
}
