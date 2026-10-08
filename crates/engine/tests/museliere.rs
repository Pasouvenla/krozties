//! La Muselière de l'Ouginak, et sa garde : « si la cible est la Proie ».
//!
//! Le sort monte de 22 dégâts de base par ennemi au contact du lanceur, quatre
//! au plus : 37-41 nu, puis 59-63, 81-85, 103-107 et 125-129. La condition est
//! dans la donnée autant que dans le texte : l'effet 293 porte les 22 points, et
//! l'effet 1019 qui compte les crans vise « A,E516 », un ennemi portant l'état
//! 516, la Proie. Le nombre d'ennemis au contact est déclaré ; la Proie est
//! simulée, le sort du même nom la posant pour 1 PA et sans limite de durée.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

/// 170 Puissance et 630 dans la caractéristique font 800, donc un facteur NEUF
/// sur le jet de base : un cran de 22 points vaut 198 exactement.
const CRAN: f64 = 22.0 * 9.0;

fn regles() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/ouginak.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-18.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn frappe(rs: &Ruleset, deck: &[&str], au_contact: u8) -> f64 {
    let build = Build {
        name: "museliere".into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 630,
                flat_damage: 56,
            }; 5],
            ..Default::default()
        },
        base_ap: 6,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| (*s).to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: vec![("ennemis_au_contact".to_string(), au_contact)],
        horizon: 1,
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
        reach: Reach::Melee,
    };
    Engine::new(rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .total
        .as_f64()
}

/// Sans la Proie, le compteur ne rapporte rien.
#[test]
fn sans_la_proie_le_compteur_ne_rapporte_rien() {
    let rs = regles();
    let nu = frappe(&rs, &["museliere"], 0);
    for au_contact in 1..=4u8 {
        let avec = frappe(&rs, &["museliere"], au_contact);
        assert!(
            (avec - nu).abs() < 0.01,
            "à {au_contact} ennemi(s) au contact mais sans Proie, la Muselière \
             doit valoir sa ligne nue {nu:.1}, et elle vaut {avec:.1}"
        );
    }
}

/// Avec la Proie, chaque ennemi au contact vaut 22 points de dégâts de base.
///
/// L'écart se vérifie au point près plutôt que dans le sens : un bonus de
/// travers passerait une assertion seulement croissante.
#[test]
fn avec_la_proie_chaque_ennemi_au_contact_vaut_un_cran() {
    let rs = regles();
    let deck = ["proie", "museliere"];
    let nu = frappe(&rs, &deck, 0);
    for au_contact in 1..=4u8 {
        let avec = frappe(&rs, &deck, au_contact);
        let attendu = nu + CRAN * f64::from(au_contact);
        assert!(
            (avec - attendu).abs() < 0.01,
            "à {au_contact} ennemi(s) au contact, la Muselière doit valoir \
             {attendu:.1} et elle vaut {avec:.1}"
        );
    }
}

/// Le solveur marque la Proie de lui-même quand ça rapporte.
///
/// Elle coûte 1 PA une fois et ne s'efface jamais : à quatre ennemis au contact
/// elle ouvre 88 points de dégâts de base. Le budget est réglé à 6 PA, juste de
/// quoi payer la Muselière et la Proie, donc le choix est réel.
#[test]
fn le_solveur_marque_la_proie_quand_elle_rapporte() {
    let rs = regles();
    let sans_le_sort = frappe(&rs, &["museliere"], 4);
    let avec_le_sort = frappe(&rs, &["proie", "museliere"], 4);
    assert!(
        avec_le_sort > sans_le_sort,
        "la Proie dans le deck doit valoir mieux que sans : {avec_le_sort:.1} \
         contre {sans_le_sort:.1}"
    );
    assert!(
        (avec_le_sort - sans_le_sort - CRAN * 4.0).abs() < 0.01,
        "et elle doit valoir exactement les quatre crans : {:.1}",
        avec_le_sort - sans_le_sort
    );
}

/// Quatre est le plafond, et c'est le compteur qui le porte.
#[test]
fn le_plafond_de_quatre_ennemis_tient() {
    let rs = regles();
    let deck = ["proie", "museliere"];
    let quatre = frappe(&rs, &deck, 4);
    for au_contact in [5u8, 8] {
        assert!(
            (frappe(&rs, &deck, au_contact) - quatre).abs() < 0.01,
            "au-delà de quatre ennemis au contact, rien ne monte plus"
        );
    }
}
