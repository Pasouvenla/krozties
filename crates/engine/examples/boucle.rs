//! Opener et boucle : la rotation qui se repete, et ce qu'elle rapporte par tour.
use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};
use std::time::Instant;

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/iop.yaml")).unwrap();
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-8.json")).unwrap();
    rs.merge_snapshot(&snap);

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
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let e = Engine::new(&rs, build, sc).unwrap();
    let t = Instant::now();
    let ss = e.steady_state();
    println!(
        "{} états, calculé en {:.3}s\n",
        ss.states,
        t.elapsed().as_secs_f64()
    );
    println!("OPENER ({} tours)", ss.opener.len());
    for tp in &ss.opener {
        println!(
            "  T{} : {}",
            tp.turn,
            tp.casts
                .iter()
                .map(|c| c.spell.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!(
        "\nBOUCLE ({} tours), {:.0} dégâts par tour",
        ss.cycle.len(),
        ss.per_turn_value()
    );
    for tp in &ss.cycle {
        println!(
            "  {} : {}",
            if tp.odd { "impair" } else { "pair  " },
            tp.casts
                .iter()
                .map(|c| c.spell.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}
