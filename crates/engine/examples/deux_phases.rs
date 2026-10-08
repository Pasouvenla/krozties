//! Combien coûtent `solve` et `steady_state`, et ce que le cache leur rend.
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

    println!(
        "{:>6} {:>10} {:>12} {:>10}",
        "sorts", "solve", "steady", "total"
    );
    for n in [25usize] {
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
            dominance: true,
            prune_spells: true,
            mode: Mode::Expected,
            reach: Reach::Ranged,
        };
        let e = Engine::new(&rs, build, sc).unwrap();
        let d0 = 0.0;
        let t1 = Instant::now();
        let s = e.solve();
        let d1 = t1.elapsed().as_secs_f64();
        let _ = d0;
        let t2 = Instant::now();
        let st = e.steady_state();
        let d2 = t2.elapsed().as_secs_f64();
        println!(
            "{n:>6} {d1:>9.2}s {d2:>11.2}s {:>9.2}s   total {:.0}, boucle {} tours",
            d1 + d2,
            s.total.as_f64(),
            st.cycle.len()
        );
        if d1 + d2 > 600.0 {
            println!("       (arrêt)");
            break;
        }
    }
}
