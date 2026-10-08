//! A quels tours un sort a relance revient-il reellement ?
use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml")).unwrap();
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();
    rs.merge_snapshot(&snap);
    let cd = rs
        .spells
        .iter()
        .find(|s| s.id == "glas")
        .unwrap()
        .cooldown_turns;
    println!("Glas : cooldown_turns = {cd}\n");

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
        // Glas exige un Telefrag : on lui donne de quoi en produire.
        deck: ["glas", "engrenage", "refraction"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 9,
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
    let s = Engine::new(&rs, build, sc).unwrap().solve();
    let mut tours = Vec::new();
    for t in &s.turns {
        let n = t.casts.iter().filter(|c| c.id == "glas").count();
        if n > 0 {
            tours.push(t.turn);
        }
        println!(
            "  Tour {:>2} : {}",
            t.turn,
            t.casts
                .iter()
                .map(|c| c.spell.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!("\nGlas lancé aux tours {tours:?}");
    if tours.len() >= 2 {
        let ecarts: Vec<u8> = tours.windows(2).map(|w| w[1] - w[0]).collect();
        println!("écart entre deux Glas : {ecarts:?} tours");
    }
}
