//! Where the time actually goes, before any of it is optimised away.
use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};
use std::time::Instant;

fn ruleset() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml")).unwrap();
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();
    assert!(rs.merge_snapshot(&snap).is_clean());
    rs
}

fn build(name: &str, deck: &[&str], profile: DamageProfile, mods: Vec<BuildModifier>) -> Build {
    Build {
        name: name.into(),
        profile,
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: mods,
        deck: deck.iter().map(|s| s.to_string()).collect(),
    }
}

fn water() -> Build {
    build(
        "eau",
        &[
            "gelure",
            "ralentissement",
            "compte_goutte",
            "permutation",
            "petrification",
            "clepsydre",
            "glas",
        ],
        DamageProfile {
            power: 110,
            flat_crit_damage: 167,
            elements: [
                ElementStats {
                    characteristic: 250,
                    flat_damage: 69,
                },
                ElementStats {
                    characteristic: 260,
                    flat_damage: 65,
                },
                ElementStats {
                    characteristic: 320,
                    flat_damage: 74,
                },
                ElementStats {
                    characteristic: 1118,
                    flat_damage: 131,
                },
                ElementStats::default(),
            ],
            ..Default::default()
        },
        vec![
            BuildModifier {
                id: "reve_nebuleux".into(),
                percent: 120,
                when: When::OddTurns,
            },
            BuildModifier {
                id: "bleu_turquoise".into(),
                percent: 110,
                when: When::Always,
            },
            BuildModifier {
                id: "pourpre_profond".into(),
                percent: 105,
                when: When::Always,
            },
        ],
    )
}

fn multi() -> Build {
    build(
        "multi",
        &[
            "souvenir",
            "engrenage",
            "poussiere",
            "refraction",
            "horloge",
            "petrification",
            "ralentissement",
            "sablier",
            "aiguille",
            "glas",
        ],
        DamageProfile {
            power: 185,
            flat_crit_damage: 151,
            elements: [
                ElementStats {
                    characteristic: 840,
                    flat_damage: 155,
                },
                ElementStats {
                    characteristic: 690,
                    flat_damage: 138,
                },
                ElementStats {
                    characteristic: 680,
                    flat_damage: 124,
                },
                ElementStats {
                    characteristic: 405,
                    flat_damage: 108,
                },
                ElementStats::default(),
            ],
            ..Default::default()
        },
        vec![
            BuildModifier {
                id: "bleu_turquoise".into(),
                percent: 110,
                when: When::Always,
            },
            BuildModifier {
                id: "pourpre_profond".into(),
                percent: 105,
                when: When::Always,
            },
        ],
    )
}

fn main() {
    let rs = ruleset();
    println!(
        "{:<10} {:>7} {:>9} {:>12} {:>10} {:>12} {:>9}",
        "build", "horizon", "temps", "noeuds", "recherches", "revisites", "etats"
    );
    println!("{}", "-".repeat(76));
    for (label, b) in [("eau", water()), ("multi", multi())] {
        for horizon in [5u8, 7] {
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
                budgets: vec![("telefrag_per_turn".into(), 3)],
                dominance: true,
                mode: Mode::PrototypeCompat,
                reach: Reach::Ranged,
                prune_spells: true,
            };
            let engine = Engine::new(&rs, b.clone(), sc).unwrap();
            let t0 = Instant::now();
            let solution = engine.solve();
            let s = engine.stats();
            println!(
                "{label:<10} {horizon:>7} {:>8.2}s {:>12} {:>10} {:>12} {:>9}",
                t0.elapsed().as_secs_f64(),
                s.nodes,
                s.turn_searches,
                s.revisits,
                solution.inter_turn_states
            );
        }
    }
}
