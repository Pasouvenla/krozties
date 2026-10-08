//! How the within-turn search scales with the number of spells in the deck.
//!
//! Run: cargo run --release --example scaling -p dofus-engine

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};
use std::time::Instant;

/// Le deck complet, dans l'ordre du fichier de mécaniques. Les sorts purement
/// offensifs sont dispersés, pas regroupés en fin de liste : sinon les petites
/// tailles mesurées n'en contiendraient aucun, et la réduction par dominance
/// semblerait ne rien faire.
const POOL: &[&str] = &[
    "gelure",
    "ralentissement",
    "fletrissement",
    "petrification",
    "frappe_de_xelor",
    "compte_goutte",
    "perturbation",
    "clepsydre",
    "regulateur",
    "poussiere",
    "pendule",
    "permutation",
    "distorsion",
    "souvenir",
    "rouage",
    "aiguille",
    "dessechement",
    "glas",
    "rayon_obscur",
    "engrenage",
    "sables_du_temps",
    "refraction",
    "sablier",
    "gousset",
    "horloge",
];

fn ruleset() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml")).unwrap();
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn main() {
    let rs = ruleset();
    println!(
        "{:>6} {:>10} {:>14} {:>14} {:>12} {:>10}",
        "sorts", "temps", "nœuds", "revisites", "outcomes", "dominés"
    );
    for n in [8usize, 10, 12, 14, 16, 20, 25] {
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
            deck: POOL[..n].iter().map(|s| s.to_string()).collect(),
        };
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
            budgets: vec![("telefrag_per_turn".into(), 3)],
            dominance: true,
            mode: Mode::Expected,
            reach: Reach::Ranged,
            prune_spells: true,
        };
        let engine = Engine::new(&rs, build, sc).unwrap();
        let ecartes = engine.pruned().len();
        let debut = Instant::now();
        let solution = engine.solve();
        let duree = debut.elapsed();
        let s = engine.stats();
        println!(
            "{:>6} {:>9.2}s {:>14} {:>14} {:>12} {:>10}   total {:.0}",
            n,
            duree.as_secs_f64(),
            s.nodes,
            s.revisits,
            s.outcomes,
            s.dominated,
            solution.total.as_f64(),
        );
        if ecartes > 0 {
            println!(
                "       {ecartes} sorts écartés : {}",
                engine
                    .pruned()
                    .iter()
                    .map(|(a, b)| format!("{a} (< {b})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if duree.as_secs_f64() > 60.0 {
            println!("       (arrêt : au-delà d'une minute)");
            break;
        }
    }
}
