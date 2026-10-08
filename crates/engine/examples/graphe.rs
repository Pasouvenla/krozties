//! Taille du graphe entre tours, avant de choisir un algorithme de cycle.
use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};
use std::time::Instant;

const POOL: &[&str] = &[
    "aiguille",
    "gelure",
    "ralentissement",
    "compte_goutte",
    "permutation",
    "petrification",
    "clepsydre",
    "glas",
    "souvenir",
    "engrenage",
    "poussiere",
    "refraction",
    "horloge",
    "sablier",
    "dessechement",
    "distorsion",
    "fletrissement",
    "frappe_de_xelor",
    "gousset",
    "pendule",
    "perturbation",
    "rayon_obscur",
    "rouage",
    "regulateur",
    "sables_du_temps",
];

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml")).unwrap();
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();
    rs.merge_snapshot(&snap);

    for n in [16usize, 20, 25] {
        let build = Build {
            name: "X".into(),
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
            horizon: 7,
            pm_depenses: 0,
            etalement: 0,
            placement: None,
            etats_declares: vec![],
            targets: 1,
            starting_turn_is_odd: true,
            resistance: Resistance::NONE,
            budgets: vec![("telefrag_per_turn".into(), 3)],
            dominance: true,
            prune_spells: true,
            mode: Mode::Expected,
            reach: Reach::Ranged,
        };
        let e = Engine::new(&rs, build, sc).unwrap();
        let t = Instant::now();
        let (noeuds, aretes) = e.graph_size();
        println!(
            "{n:>3} sorts : {noeuds:>7} nœuds, {aretes:>9} arêtes  ({:.2}s)",
            t.elapsed().as_secs_f64()
        );
    }
}
