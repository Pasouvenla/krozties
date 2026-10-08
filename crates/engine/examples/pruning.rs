//! Ce que la reduction par dominance ecarte, et pourquoi.
use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml")).unwrap();
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();
    rs.merge_snapshot(&snap);

    // Uniquement des sorts purement offensifs : c'est la que la dominance joue.
    let deck = [
        "fletrissement",
        "frappe_de_xelor",
        "perturbation",
        "regulateur",
        "pendule",
        "distorsion",
        "rouage",
        "dessechement",
        "rayon_obscur",
        "sables_du_temps",
    ];
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
        horizon: 3,
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
    let engine = Engine::new(&rs, build, sc).unwrap();
    println!("{} sorts au deck", deck.len());
    println!("{} écartés :", engine.pruned().len());
    for (a, b) in engine.pruned() {
        println!("   {a}  écarté au profit de  {b}");
    }
    let s = engine.solve();
    println!("total {:.0}", s.total.as_f64());
}
