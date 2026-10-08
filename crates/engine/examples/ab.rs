//! Le meme deck, avec et sans reduction. La seule comparaison qui dise
//! quelque chose sur ce que la reduction apporte.
use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};
use std::time::Instant;

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

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml")).unwrap();
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();
    rs.merge_snapshot(&snap);

    let n: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(16);
    println!("deck de {n} sorts, horizon 5\n");
    // Deuxième argument : « nominal » pour ne mesurer que la configuration
    // réellement servie, les deux autres n'étant là que pour la vérifier.
    let nominal_seul = std::env::args().nth(2).as_deref() == Some("nominal");
    let configs: &[(&str, bool, bool)] = if nominal_seul {
        &[("+ réduction", true, true)]
    } else {
        &[
            ("exhaustif", false, false),
            ("dominance seule", true, false),
            ("+ réduction", true, true),
        ]
    };
    for &(etiquette, dominance, reduire) in configs {
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
            horizon: 5,
            pm_depenses: 0,
            etalement: 0,
            placement: None,
            etats_declares: vec![],
            targets: 1,
            starting_turn_is_odd: true,
            resistance: Resistance::NONE,
            budgets: vec![("telefrag_per_turn".into(), 3)],
            dominance,
            mode: Mode::Expected,
            reach: Reach::Ranged,
            prune_spells: reduire,
        };
        let e = Engine::new(&rs, build, sc).unwrap();
        let t = Instant::now();
        let s = e.solve();
        println!(
            "{:<16} {:>8.2}s  {:>13} nœuds  {} écartés  total {:.0}",
            etiquette,
            t.elapsed().as_secs_f64(),
            e.stats().nodes,
            e.pruned().len(),
            s.total.as_f64()
        );
    }
}
