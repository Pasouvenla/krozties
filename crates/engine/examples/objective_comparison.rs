//! What changes when the objective stops assuming every hit crits.
//!
//! The prototypes optimise the midpoint of the critical range on every cast;
//! real critical rates are per spell. The gap between critical and normal is
//! dominated by flat critical damage, which applies once per damage line, so a
//! four-line spell like Glas is flattered four times as hard by the assumption.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn ruleset() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml")).unwrap();
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();
    let report = rs.merge_snapshot(&snap);
    assert!(report.is_clean(), "{report:?}");
    rs
}

fn build(crit_bonus: i32) -> Build {
    Build {
        name: "Xelor eau (Chance)".into(),
        profile: DamageProfile {
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
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: crit_bonus,
        modifiers: vec![
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
        deck: [
            "gelure",
            "ralentissement",
            "compte_goutte",
            "permutation",
            "petrification",
            "clepsydre",
            "glas",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    }
}

fn scenario(mode: Mode, resistance: Resistance) -> Scenario {
    Scenario {
        poussees_bloquees: false,
        horizon: 7,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance,
        budgets: vec![("telefrag_per_turn".into(), 3)],
        dominance: true,
        mode,
        reach: Reach::Ranged,
        prune_spells: true,
    }
}

fn summarise(label: &str, solution: &Solution) {
    let casts: Vec<&Cast> = solution.turns.iter().flat_map(|t| t.casts.iter()).collect();
    let glas = casts.iter().filter(|c| c.id == "glas").count();
    let clepsydre = casts.iter().filter(|c| c.id == "clepsydre").count();
    let petrif = casts.iter().filter(|c| c.id == "petrification").count();
    let glas_share: f64 = casts
        .iter()
        .filter(|c| c.id == "glas")
        .map(|c| c.damage.as_f64())
        .sum::<f64>()
        / solution.total.as_f64()
        * 100.0;
    println!(
        "{label:<38} {:>10.0} {:>10.0} {:>6} {:>6} {:>7} {:>8.1}%",
        solution.total.as_f64(),
        solution.total.as_f64() / 7.0,
        casts.len(),
        glas,
        petrif + clepsydre,
        glas_share
    );
}

fn main() {
    let rs = ruleset();
    println!("Xelor water, 7 turns, 3 Telefrags per turn, 12 AP\n");
    println!(
        "{:<38} {:>10} {:>10} {:>6} {:>6} {:>7} {:>9}",
        "objective", "total", "per turn", "casts", "Glas", "consum", "Glas %"
    );
    println!("{}", "-".repeat(92));

    let compat = Engine::new(
        &rs,
        build(0),
        scenario(Mode::PrototypeCompat, Resistance::NONE),
    )
    .unwrap()
    .solve();
    summarise("prototype compat (always critical)", &compat);

    for bonus in [0, 20, 40, 60] {
        let e = Engine::new(
            &rs,
            build(bonus),
            scenario(Mode::Expected, Resistance::NONE),
        )
        .unwrap();
        summarise(&format!("expected, critical bonus +{bonus}%"), &e.solve());
    }

    // A target with flat resistance on every element, which is subtracted once
    // per damage line and so bites a four-line spell four times.
    let flat = Resistance {
        percent: [0; 5],
        flat: [20; 5],
        critical: 0,
        ..Default::default()
    };
    let e = Engine::new(&rs, build(20), scenario(Mode::Expected, flat)).unwrap();
    summarise("expected +20%, 20 flat resistance", &e.solve());

    let mixed = Resistance {
        percent: [50, 10, 10, 10, 0],
        flat: [0; 5],
        critical: 0,
        ..Default::default()
    };
    let e = Engine::new(&rs, build(20), scenario(Mode::Expected, mixed)).unwrap();
    summarise("expected +20%, 50% fire / 10% rest", &e.solve());
}
