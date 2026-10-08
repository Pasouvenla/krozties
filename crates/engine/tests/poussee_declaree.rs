//! « La cible subit des dommages de poussée à chaque tour » : un réglage
//! déclaré par le joueur.
//!
//! Trois sorts posent un état qui frappe quand la cible subit des dommages de
//! poussée, puis se retire : le Noa du Forgelance, la Flibuste du Steamer, la
//! Flèche Tyrannique du Crâ, dont la décharge retire aussi le poison. Coché, le
//! réglage fait frapper chaque état une fois par tour, au début du suivant ;
//! décoché, rien ne change.

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

fn solve(rs: &Ruleset, deck: &[&str], poussee: u8, horizon: u8) -> Solution {
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
        etats_declares: vec![("poussee_subie".to_string(), poussee)],
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
        // Au contact, le Noa frappe sans que la Lance soit plantée.
        reach: Reach::Melee,
    };
    Engine::new(rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

fn coup(element: Element, normal: (i32, i32), critique: (i32, i32), taux: i32) -> f64 {
    expected_line(
        &SpellLine {
            element,
            normal,
            critical: critique,
        },
        &profil(),
        FinalMultiplier::NEUTRAL,
        CritRate::from_percent(taux),
        &Resistance::NONE,
    )
    .as_f64()
}

/// Ce que chaque tour fait tomber, à son ouverture, au compte d'un sort.
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

/// Le Noa : 26-29 Air au taux du sort, 25 %, au début du tour qui suit. Rien
/// sans le réglage.
#[test]
fn le_noa_frappe_quand_la_cible_est_poussee() {
    let rs = regles("forgelance", 20);
    let attendu = coup(Element::Air, (26, 29), (31, 35), 25);
    let avec = ouverture(&solve(&rs, &["noa"], 1, 2), "noa");
    let sans = ouverture(&solve(&rs, &["noa"], 0, 2), "noa");
    assert!(
        (avec[1] - attendu).abs() < 0.01,
        "{attendu:.2} attendu au tour 2, {avec:?} mesuré"
    );
    assert!(sans.iter().all(|d| *d == 0.0), "rien sans poussée : {sans:?}");
}

/// La Flibuste : UN coup par tour, même lancée deux fois, et à 1 % de
/// critique de base, le taux de son sous-sort.
#[test]
fn la_flibuste_frappe_une_fois_par_tour_a_un_pour_cent() {
    let rs = regles("steamer", 15);
    let sol = solve(&rs, &["flibuste"], 1, 2);
    let lancers = sol.turns[0].casts.iter().filter(|c| c.id == "flibuste").count();
    assert_eq!(lancers, 2, "deux Flibustes au tour 1 attendues");
    let attendu = coup(Element::Air, (23, 25), (28, 30), 1);
    let faux = coup(Element::Air, (23, 25), (28, 30), 15);
    assert!((attendu - faux).abs() > 0.05, "les deux taux ne se distinguent plus");
    let v = ouverture(&sol, "flibuste");
    assert!(
        (v[1] - attendu).abs() < 0.01,
        "un coup à 1 % attendu au tour 2 ({attendu:.2}), {v:?} mesuré"
    );
}

/// La Flèche Tyrannique : poussée, elle décharge 28-32 d'un coup et le
/// poison disparaît. Sans poussée, le poison 20-22 tombe comme avant.
#[test]
fn la_poussee_decharge_la_fleche_tyrannique() {
    let rs = regles("cra", 9);
    let decharge = coup(Element::Fire, (28, 32), (34, 38), 1);
    let avec = solve(&rs, &["fleche_tyrannique"], 1, 2);
    assert!(
        (ouverture(&avec, "fleche_tyrannique")[1] - decharge).abs() < 0.01,
        "la décharge seule attendue au tour 2 ({decharge:.2}), {:?} mesuré",
        ouverture(&avec, "fleche_tyrannique")
    );
    // Le poison garde le taux du sort, 15 % : la convention du parc pour un
    // état, que ce réglage ne touche pas.
    let attendu = coup(Element::Fire, (20, 22), (24, 26), 15);
    let sans = solve(&rs, &["fleche_tyrannique"], 0, 2);
    let poison = ouverture(&sans, "fleche_tyrannique")[1];
    assert!(
        (poison - attendu).abs() < 0.01,
        "sans poussée, le poison et lui seul ({attendu:.2}), {poison:.2} mesuré"
    );
}
