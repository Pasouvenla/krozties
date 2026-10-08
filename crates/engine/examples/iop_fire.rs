//! Iop, fire path: a second class solved by the same engine, with nothing
//! added to it but vocabulary in the ruleset. The character sheet below is
//! plausible for a level 200 fire Iop but hand-written: the point is that the
//! mechanics resolve, not that the damage figure is exact.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/iop.yaml")).unwrap();
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-8.json")).unwrap();
    assert!(rs.merge_snapshot(&snap).is_clean());

    let build = Build {
        name: "Iop Feu".into(),
        profile: DamageProfile {
            power: 150,
            flat_crit_damage: 150,
            elements: [
                ElementStats {
                    characteristic: 850,
                    flat_damage: 120,
                }, // fire
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
        deck: [
            "couperet",
            "rassemblement",
            "epee_destructrice",
            "epee_du_destin",
            "puissance",
            "sentence",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    };

    for deck_label in ["avec Sentence (2 PA)", "sans Sentence"] {
        let mut b = build.clone();
        if deck_label == "sans Sentence" {
            b.deck.retain(|s| s != "sentence");
        }
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
        let engine = Engine::new(&rs, b, sc).unwrap_or_else(|e| panic!("{e}"));
        let started = std::time::Instant::now();
        let s = engine.solve();
        println!("=== {deck_label} ===");
        println!("{s}");
        println!(
            "resolu en {:.3}s, {} etats\n",
            started.elapsed().as_secs_f64(),
            s.inter_turn_states
        );
    }
}
