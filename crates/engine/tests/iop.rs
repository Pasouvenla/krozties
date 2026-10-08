//! Second class: does the format hold without touching the engine? Iop brings
//! two things Xelor never exercised: a buff that raises the damage of every
//! spell, where Horloge only raises its own, and a stack that climbs on its own
//! while a spell recharges. Both are vocabulary in the ruleset.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn iop() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/iop.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-8.json")).unwrap();
    let report = rs.merge_snapshot(&snap);
    assert!(report.conflicts.is_empty(), "{:#?}", report.conflicts);
    assert!(report.unmatched.is_empty(), "{:#?}", report.unmatched);
    rs
}

fn build(deck: &[&str]) -> Build {
    Build {
        name: "Iop Feu".into(),
        profile: DamageProfile {
            power: 150,
            flat_crit_damage: 150,
            elements: [
                ElementStats {
                    characteristic: 850,
                    flat_damage: 120,
                },
                ElementStats {
                    characteristic: 200,
                    flat_damage: 40,
                },
                ElementStats {
                    characteristic: 200,
                    flat_damage: 40,
                },
                ElementStats {
                    characteristic: 200,
                    flat_damage: 40,
                },
                ElementStats::default(),
            ],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 25,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    }
}

const FULL: &[&str] = &[
    "couperet",
    "rassemblement",
    "epee_destructrice",
    "epee_du_destin",
    "puissance",
    "sentence",
];

fn solve(deck: &[&str]) -> Solution {
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 7,
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
        reach: Reach::Ranged,
        prune_spells: true,
    };
    Engine::new(&iop(), build(deck), sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

fn without(spell: &str) -> Vec<&str> {
    FULL.iter().copied().filter(|s| *s != spell).collect()
}

/// Iop's numbers merge from the snapshot with nothing left over but the one
/// question a datamine cannot answer.
#[test]
fn the_ruleset_resolves_against_the_snapshot() {
    let rs = iop();
    let left = rs.data_gaps();
    assert!(
        left.iter().all(|g| g.path.ends_with(".open_question")),
        "a magnitude is still missing: {left:?}"
    );
}

/// A buff carried by a state, raising every spell rather than its own line:
/// effect 138, for three turns.
#[test]
fn puissance_is_worth_casting() {
    let with = solve(FULL).total;
    let without = solve(&without("puissance")).total;
    assert!(with > without, "avec {with} contre sans {without}");
}

/// The stack that climbs while the spell recharges has to actually reach the
/// cast. If gain_per_turn were dropped the spell would be worth strictly less.
#[test]
fn the_recharge_stack_reaches_the_cast() {
    let solution = solve(FULL);
    let casts: Vec<&Cast> = solution.turns.iter().flat_map(|t| t.casts.iter()).collect();
    let destin: Vec<f64> = casts
        .iter()
        .filter(|c| c.id == "epee_du_destin")
        .map(|c| c.damage.as_f64())
        .collect();
    assert!(
        destin.len() >= 2,
        "l'Épée du Destin n'est lancée que {} fois",
        destin.len()
    );
    let best = destin.iter().cloned().fold(f64::MIN, f64::max);
    let worst = destin.iter().cloned().fold(f64::MAX, f64::min);
    assert!(
        best > worst,
        "la pile de récupération ne change rien : {destin:?}"
    );
}

/// Every build runs 12 AP so that nothing is left over, and a deck of 3 and 4
/// AP spells cannot spend them: 3+4+4 leaves one behind, every turn. Adding a
/// 2 AP option is not worth one point of AP, it is worth the whole extra spell
/// it lets you chain.
#[test]
fn a_cheap_spell_removes_the_wasted_ap() {
    let full = solve(FULL);
    let starved = solve(&without("sentence"));

    assert!(
        full.turns.iter().all(|t| t.ap_left == 0),
        "des PA restent malgré un sort à 2 PA : {:?}",
        full.turns.iter().map(|t| t.ap_left).collect::<Vec<_>>()
    );
    assert!(
        starved.turns.iter().any(|t| t.ap_left > 0),
        "sans sort bon marché, des PA devraient rester"
    );
    assert!(
        full.total.as_f64() > starved.total.as_f64() * 1.2,
        "plein {} contre appauvri {}",
        full.total,
        starved.total
    );
}
