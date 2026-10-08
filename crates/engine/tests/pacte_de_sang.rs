//! Une Puissance accordée au lanceur par un sort qui ne frappe pas.
//!
//! Le Pacte de Sang du Sacrieur « augmente la Vitalité et la Puissance du
//! lanceur » : effet 138, cent points, trois tours, cible « C ». La plupart des
//! gains de Puissance du parc vont à un allié ciblé et ne valent rien en solo ;
//! celui-ci va au lanceur, sans condition. Le sort est invisible à la
//! couverture, qui ne compte que les sorts qui frappent : c'est ce test qui le
//! garde.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn sacrieur() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/sacrieur.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-11.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

/// Ce que la Fulgurance vaut à chaque tour, dans l'ordre.
fn fulgurance(deck: &[&str]) -> Vec<f64> {
    let build = Build {
        name: "Sacrieur".into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 630,
                flat_damage: 56,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon: 4,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&sacrieur(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .map(|t| {
            t.casts
                .iter()
                .filter(|c| c.id == "fulgurance")
                .map(|c| c.damage.as_f64())
                .sum()
        })
        .collect()
}

/// La Puissance posée profite à tout ce qui suit, y compris aux autres sorts.
#[test]
fn le_pacte_leve_la_rotation_sans_frapper_lui_meme() {
    let sans = fulgurance(&["fulgurance"]);
    let avec = fulgurance(&["fulgurance", "pacte_de_sang"]);
    assert!(sans.len() == 4 && avec.len() == 4, "{sans:?} / {avec:?}");
    assert!(
        sans.windows(2).all(|f| (f[0] - f[1]).abs() < 0.51),
        "seule, la Fulgurance ne doit pas bouger : {sans:?}"
    );
    assert!(
        avec[3] > sans[3],
        "avec le Pacte elle doit monter, mesuré {sans:?} contre {avec:?}"
    );
    // Cent points sur un facteur de 900 : environ onze pour cent sur le jet,
    // un peu moins une fois les dommages fixes ajoutés. Exiger l'AMPLEUR, une
    // assertion directionnelle passant avec un point de Puissance.
    let ecart = (avec[3] - sans[3]) / sans[3];
    assert!(
        ecart > 0.05,
        "l'écart doit valoir plusieurs points, mesuré {:.1} %",
        ecart * 100.0
    );
}
