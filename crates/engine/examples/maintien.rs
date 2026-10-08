//! Le solveur entretient-il un bonus temporaire, et l'horizon fausse-t-il sa
//! decision ?
use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/iop.yaml")).unwrap();
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-8.json")).unwrap();
    rs.merge_snapshot(&snap);

    for horizon in [3u8, 5, 7, 9, 12] {
        let build = Build {
            name: "Iop".into(),
            profile: DamageProfile {
                power: 150,
                flat_crit_damage: 150,
                elements: [ElementStats {
                    characteristic: 850,
                    flat_damage: 120,
                }; 5],
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
        let s = Engine::new(&rs, build, sc).unwrap().solve();
        let tours: Vec<u8> = s
            .turns
            .iter()
            .filter(|t| t.casts.iter().any(|c| c.id == "puissance"))
            .map(|t| t.turn)
            .collect();
        let ecarts: Vec<u8> = tours.windows(2).map(|w| w[1] - w[0]).collect();
        println!(
            "horizon {horizon:>2} : Puissance aux tours {tours:?}  écarts {ecarts:?}  \
             moyenne {:.0}/tour",
            s.total.as_f64() / f64::from(horizon)
        );
    }
}
