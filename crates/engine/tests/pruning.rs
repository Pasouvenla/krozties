//! Dropping a spell must never change the answer.
//!
//! A solver that quietly ignores part of the deck is worse than a slow one, so
//! every case here runs the search twice, with the reduction and exhaustively,
//! and requires the two totals to be equal to the unit. The reduction sits under
//! the same `dominance` flag as the end-of-turn pruning, which is what makes
//! that comparison possible.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn charger(nom: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{nom}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn xelor() -> Ruleset {
    charger("xelor", 5)
}

fn run(rs: &Ruleset, deck: &[&str], reduce: bool) -> (i64, usize, u64) {
    let build = Build {
        name: "Xélor".into(),
        profile: DamageProfile {
            power: 270,
            flat_crit_damage: 173,
            elements: [ElementStats {
                characteristic: 400,
                flat_damage: 60,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 40,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 4,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![("telefrag_per_turn".into(), 3)],
        dominance: reduce,
        mode: Mode::Expected,
        reach: Reach::Ranged,
        prune_spells: true,
    };
    let engine = Engine::new(rs, build, sc).unwrap_or_else(|e| panic!("{e}"));
    let solution = engine.solve();
    (
        solution.total.0,
        engine.pruned().len(),
        engine.stats().nodes,
    )
}

const CASES: &[&[&str]] = &[
    // Purely offensive spells only: where the reduction has the most to say.
    &[
        "fletrissement",
        "frappe_de_xelor",
        "perturbation",
        "regulateur",
    ],
    &[
        "pendule",
        "distorsion",
        "rouage",
        "rayon_obscur",
        "dessechement",
    ],
    &[
        "fletrissement",
        "frappe_de_xelor",
        "perturbation",
        "regulateur",
        "pendule",
        "distorsion",
        "rouage",
        "rayon_obscur",
    ],
    // Mixed decks: AP generators raise the turn's budget, which is exactly what
    // makes dropping a spell unsafe if the bound is computed carelessly.
    &[
        "gelure",
        "ralentissement",
        "fletrissement",
        "frappe_de_xelor",
        "perturbation",
        "pendule",
        "distorsion",
    ],
    &[
        "gelure",
        "engrenage",
        "refraction",
        "pendule",
        "rouage",
        "fletrissement",
        "regulateur",
        "sables_du_temps",
    ],
    // A deck where nothing may be dropped: every spell carries an effect.
    &["gelure", "engrenage", "refraction", "clepsydre", "glas"],
];

#[test]
fn the_reduction_never_changes_the_total() {
    let rs = xelor();
    for deck in CASES {
        let (reduced, dropped, _) = run(&rs, deck, true);
        let (exhaustive, _, _) = run(&rs, deck, false);
        assert_eq!(
            reduced, exhaustive,
            "dropping {dropped} spell(s) changed the answer on {deck:?}"
        );
    }
}

/// The reduction has to actually reduce something, or it is dead weight
/// pretending to be an optimisation.
#[test]
fn the_reduction_actually_reduces() {
    // Cinq sorts du Roublard qui ne portent qu'une ligne de dégâts et aucun
    // autre effet, à coûts et fourchettes différents : exactement la situation
    // où l'un domine l'autre à tous les coups. La classe n'a aucun buff global ;
    // le jour où elle en gagne un, il faudra en prendre une autre parmi celles
    // qui n'en ont pas, pas relâcher l'assertion.
    const DECK: &[&str] = &[
        "extraction",
        "dagues_boomerang",
        "espingole",
        "pulsar",
        "mousquet",
    ];
    let rs = charger("roublard", 13);
    let (reduit, dropped, avec) = run(&rs, DECK, true);
    let (exhaustif, _, sans) = run(&rs, DECK, false);
    assert_eq!(
        reduit, exhaustif,
        "l'élagage a changé la réponse en retirant {dropped} sort(s)"
    );
    assert!(
        avec <= sans,
        "l'élagage a visité PLUS de nœuds : {avec} contre {sans}"
    );
    assert!(
        dropped > 0,
        "aucun sort retiré : la réduction ne fait rien sur {DECK:?}"
    );
}

/// A spell carrying any effect beyond damage is never dropped: its worth
/// depends on the state, which a static comparison cannot see.
#[test]
fn spells_with_effects_are_never_dropped() {
    let rs = xelor();
    let (_, dropped, _) = run(
        &rs,
        &["gelure", "engrenage", "refraction", "clepsydre", "glas"],
        true,
    );
    assert_eq!(
        dropped, 0,
        "a spell that gains, spends or schedules must never be dropped on a \
         damage comparison alone"
    );
}
