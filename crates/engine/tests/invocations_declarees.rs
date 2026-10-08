//! Deux sorts qui frappent par une invocation, chacun sous un réglage que le
//! joueur déclare.
//!
//! * « Votre double meurt au contact de la cible » : le double du Comploteur
//!   meurt à la fin de son troisième tour et frappe alors 26-30 dans le
//!   meilleur élément.
//! * « Une de vos Poupées meurt pendant la Malédiction » : la Malédiction
//!   Vaudou frappe encore 14-16 Eau à la mort d'une Poupée.

use dofus_damage::{
    expected_line, CritRate, DamageProfile, Element, ElementStats, FinalMultiplier, Resistance,
    SpellLine,
};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn regles(classe: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap =
        Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

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

fn solve(rs: &Ruleset, deck: &[&str], reglage: (&str, u8), horizon: u8) -> Solution {
    let build = Build {
        name: "banc".into(),
        profile: profil(),
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| (*s).to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: vec![(reglage.0.to_string(), reglage.1)],
        horizon,
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
    Engine::new(rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

/// Sans critique : ni le Comploteur ni la Malédiction ne peuvent critiquer.
fn coup(element: Element, fourchette: (i32, i32)) -> f64 {
    expected_line(
        &SpellLine {
            element,
            normal: fourchette,
            critical: fourchette,
        },
        &profil(),
        FinalMultiplier::NEUTRAL,
        CritRate::NEVER,
        &Resistance::NONE,
    )
    .as_f64()
}

fn ouverture(sol: &Solution, sort: &str) -> Vec<f64> {
    sol.turns
        .iter()
        .map(|t| {
            t.opening_sources
                .iter()
                .filter(|(s, _)| s == sort)
                .map(|(_, d)| d.as_f64())
                .sum()
        })
        .collect()
}

/// Le double lancé au tour 1 explose au début du tour 4, et à ce seul tour.
///
/// Les cinq éléments du build se valent : le meilleur vaut donc n'importe
/// lequel d'entre eux.
#[test]
fn le_double_du_comploteur_explose_trois_tours_apres() {
    let rs = regles("sram", 4);
    let attendu = coup(Element::Earth, (26, 30));
    let avec = ouverture(&solve(&rs, &["comploteur"], ("double_au_contact", 1), 4), "comploteur");
    assert_eq!(avec.len(), 4);
    assert!(
        avec[..3].iter().all(|d| *d == 0.0) && (avec[3] - attendu).abs() < 0.01,
        "{attendu:.2} attendu au seul tour 4, {avec:?} mesuré"
    );
    let sans = ouverture(&solve(&rs, &["comploteur"], ("double_au_contact", 0), 4), "comploteur");
    assert!(sans.iter().all(|d| *d == 0.0), "rien sans le réglage : {sans:?}");
}

/// La Malédiction frappe une fois de plus, au début du tour qui suit, si une
/// Poupée meurt pendant elle.
#[test]
fn la_malediction_frappe_a_la_mort_d_une_poupee() {
    let rs = regles("sadida", 10);
    let attendu = coup(Element::Water, (14, 16));
    let avec = ouverture(
        &solve(&rs, &["malediction_vaudou"], ("poupee_morte", 1), 2),
        "malediction_vaudou",
    );
    assert!(
        avec[0] == 0.0 && (avec[1] - attendu).abs() < 0.01,
        "{attendu:.2} attendu au tour 2, {avec:?} mesuré"
    );
    let sans = ouverture(
        &solve(&rs, &["malediction_vaudou"], ("poupee_morte", 0), 2),
        "malediction_vaudou",
    );
    assert!(sans.iter().all(|d| *d == 0.0), "rien sans le réglage : {sans:?}");
}
