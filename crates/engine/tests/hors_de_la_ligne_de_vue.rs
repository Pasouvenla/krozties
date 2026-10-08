//! Le Crâ finit son tour hors de la ligne de vue de sa cible, par défaut : la
//! Flèche Persécutrice « occasionne des dommages Air au tour suivant si la cible
//! n'est pas dans la ligne de vue du lanceur », et ce second coup compte donc à
//! chaque lancer.

use dofus_damage::{
    expected_line, CritRate, DamageProfile, Element, ElementStats, FinalMultiplier, Resistance,
    SpellLine,
};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn profil() -> DamageProfile {
    DamageProfile {
        power: 170,
        flat_crit_damage: 0,
        elements: [ElementStats {
            characteristic: 630,
            flat_damage: 56,
        }; 5],
        ..Default::default()
    }
}

/// Lancée au tour 1, la flèche frappe encore au début du tour 2 : Air 34-38,
/// 41-46 en critique, au taux de 15 % du sort. Rien au tour 1.
#[test]
fn la_persecutrice_frappe_encore_au_tour_suivant() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/cra.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-9.json")).unwrap();
    rs.merge_snapshot(&snap);
    let build = Build {
        name: "cra".into(),
        profile: profil(),
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["fleche_persecutrice".into()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: vec![],
        horizon: 2,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let sol = Engine::new(&rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve();
    let attendu = expected_line(
        &SpellLine {
            element: Element::Air,
            normal: (34, 38),
            critical: (41, 46),
        },
        &profil(),
        FinalMultiplier::NEUTRAL,
        CritRate::from_percent(15),
        &Resistance::NONE,
    )
    .as_f64();
    let v: Vec<f64> = sol
        .turns
        .iter()
        .map(|t| {
            t.opening_sources
                .iter()
                .filter(|(s, _)| s == "fleche_persecutrice")
                .map(|(_, d)| d.as_f64())
                .sum()
        })
        .collect();
    assert!(
        v[0] == 0.0 && (v[1] - attendu).abs() < 0.01,
        "{attendu:.2} attendu au seul tour 2, {v:?} mesuré"
    );
}
