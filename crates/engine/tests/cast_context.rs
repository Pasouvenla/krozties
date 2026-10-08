//! Cast-context percentages, end to end through the solver: a build carrying
//! `% Dommages aux sorts` sees every spell go up, melee and ranged are selected
//! per spell by its own range, and a build carrying none of them gets the same
//! numbers as without the feature.

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

const DECK: &[&str] = &[
    "couperet",
    "rassemblement",
    "epee_destructrice",
    "epee_du_destin",
    "puissance",
    "sentence",
];

fn build(profile: DamageProfile) -> Build {
    Build {
        name: "Iop".into(),
        profile,
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 25,
        modifiers: vec![],
        deck: DECK.iter().map(|s| s.to_string()).collect(),
    }
}

fn plain() -> DamageProfile {
    DamageProfile {
        power: 150,
        flat_crit_damage: 150,
        elements: [ElementStats {
            characteristic: 850,
            flat_damage: 120,
        }; 5],
        ..Default::default()
    }
}

fn total(profile: DamageProfile, reach: Reach) -> i64 {
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 5,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        mode: Mode::Expected,
        reach,
        prune_spells: true,
    };
    Engine::new(&iop(), build(profile), sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .total
        .0
}

/// The reference: a build with none of these percentages.
fn baseline() -> i64 {
    total(plain(), Reach::Ranged)
}

#[test]
fn a_build_with_no_percentages_is_unchanged_by_reach() {
    assert_eq!(
        total(plain(), Reach::Melee),
        total(plain(), Reach::Ranged),
        "with no melee or ranged percentage, where the player stands cannot \
         change a single point of damage"
    );
}

#[test]
fn spell_damage_percentage_raises_the_whole_rotation() {
    let boosted = total(
        DamageProfile {
            percent_spell: 10,
            ..plain()
        },
        Reach::Ranged,
    );
    let base = baseline();
    assert!(
        boosted > base,
        "10% spell damage must raise the total: {base} -> {boosted}"
    );
    // Every spell is a spell, so the whole rotation scales. Floors per line and
    // per roll keep it just under a clean 10%, never over.
    let ratio = (boosted as f64) / (base as f64);
    assert!(
        (1.085..=1.100).contains(&ratio),
        "expected close to x1.10 from below, got x{ratio:.4}"
    );
}

/// The point of the whole exercise: melee and ranged are NOT interchangeable,
/// and a spell whose range forces its reach ignores the player's preference.
#[test]
fn reach_is_decided_per_spell_by_its_own_range() {
    let melee_build = DamageProfile {
        percent_melee: 40,
        ..plain()
    };
    let at_contact = total(melee_build, Reach::Melee);
    let at_range = total(melee_build, Reach::Ranged);

    assert!(
        at_contact > at_range,
        "a melee-damage build must do more standing at contact: \
         {at_range} at range vs {at_contact} at contact"
    );
    // Not everything moved: spells whose minimum range is 2 or more stay ranged
    // whatever the player prefers, so the gap is strictly smaller than the full
    // 40% the build carries.
    let gap = (at_contact as f64) / (at_range as f64);
    assert!(
        gap < 1.40,
        "spells locked to range must not have taken the melee bonus (x{gap:.4})"
    );
}

fn xelor() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();
    let report = rs.merge_snapshot(&snap);
    // Loading the wrong breed matches nothing and quietly leaves every range
    // empty, which is how this test first "passed" against the Pandawa.
    assert!(
        report.unmatched.is_empty(),
        "wrong snapshot for this class: {:#?}",
        report.unmatched
    );
    rs
}

/// Reading the ranges out of the merged ruleset, so a snapshot that stops
/// carrying them fails here rather than silently making every spell "either".
///
/// Run on Xelor rather than Iop: with 25 spells modelled it actually contains
/// spells whose reach is forced, which is what makes the assertion mean
/// something.
#[test]
fn the_snapshot_supplies_a_range_for_every_spell() {
    let rs = xelor();
    let missing: Vec<&str> = rs
        .spells
        .iter()
        .filter(|s| s.range.is_none())
        .map(|s| s.id.as_str())
        .collect();
    assert!(
        missing.is_empty(),
        "no casting range for: {missing:?}; melee vs ranged would fall back to \
         the scenario's setting for these"
    );
    let forced_melee: Vec<&str> = rs
        .spells
        .iter()
        .filter(|s| matches!(s.range, Some((_, max)) if max <= 1))
        .map(|s| s.id.as_str())
        .collect();
    let forced_ranged: Vec<&str> = rs
        .spells
        .iter()
        .filter(|s| matches!(s.range, Some((min, _)) if min >= 2))
        .map(|s| s.id.as_str())
        .collect();
    assert!(
        !forced_melee.is_empty() || !forced_ranged.is_empty(),
        "a class where no spell has a forced reach would make this test vacuous"
    );
    println!("contact forcé: {forced_melee:?}");
    println!("distance forcée: {forced_ranged:?}");
}
