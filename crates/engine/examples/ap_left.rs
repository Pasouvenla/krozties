//! Does the solver ever end a turn with AP to spare?
use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml")).unwrap();
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();
    assert!(rs.merge_snapshot(&snap).is_clean());

    let builds: Vec<(&str, Vec<&str>, DamageProfile, Vec<BuildModifier>)> = vec![
        (
            "eau",
            vec![
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
                    id: "reve".into(),
                    percent: 120,
                    when: When::OddTurns,
                },
                BuildModifier {
                    id: "bt".into(),
                    percent: 110,
                    when: When::Always,
                },
                BuildModifier {
                    id: "pp".into(),
                    percent: 105,
                    when: When::Always,
                },
            ],
        ),
        (
            "multi",
            vec![
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
                    id: "bt".into(),
                    percent: 110,
                    when: When::Always,
                },
                BuildModifier {
                    id: "pp".into(),
                    percent: 105,
                    when: When::Always,
                },
            ],
        ),
    ];

    for (label, deck, profile, modifiers) in builds {
        for cap in [2u8, 3] {
            let build = Build {
                name: label.into(),
                profile,
                base_ap: 12,
                base_mp: 3,
                crit_bonus_percent: 0,
                modifiers: modifiers.clone(),
                deck: deck.iter().map(|s| s.to_string()).collect(),
            };
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
                budgets: vec![("telefrag_per_turn".into(), cap)],
                dominance: true,
                mode: Mode::PrototypeCompat,
                reach: Reach::Ranged,
                prune_spells: true,
            };
            let s = Engine::new(&rs, build, sc).unwrap().solve();
            let left: Vec<String> = s
                .turns
                .iter()
                .map(|t| format!("T{}:{}", t.turn, t.ap_left))
                .collect();
            let total: i16 = s.turns.iter().map(|t| t.ap_left).sum();
            println!(
                "{label:<6} budget {cap}   PA restants par tour  {}   total {total}",
                left.join(" ")
            );
        }
    }
}
