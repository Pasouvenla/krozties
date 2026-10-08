//! Les PM que le joueur compte dépenser, et les deux sorts qui les lisent. Pas
//! une simulation du déplacement : le solveur ne sait pas où se tiennent les
//! entités, le joueur sait s'il va rester planté ou traverser la salle.
//!
//! Deux lectures opposées du même réglage : la Tourbière monte avec les PM
//! dépensés, le Zénith descend avec eux puisqu'il lit les PM restants. Un
//! câblage inversé passerait l'un des deux sans qu'on s'en aperçoive.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn regles(classe: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn frappe(rs: &Ruleset, sort: &str, pm_depenses: u8, base_mp: u8) -> f64 {
    let build = Build {
        name: sort.into(),
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
        base_mp,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec![sort.to_string()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: vec![],
        etalement: 0,
        placement: None,
        horizon: 1,
        pm_depenses,
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
        .total
        .as_f64()
}

/// La Tourbière monte de cinq dégâts de base par PM dépensé, six crans au plus :
/// 31-35 nu, puis 36-40 à un PM, et ainsi de suite jusqu'à 61-65 à six.
#[test]
fn la_tourbiere_monte_avec_les_pm_depenses() {
    let rs = regles("enutrof", 3);
    let mesures: Vec<f64> = (0..=7).map(|pm| frappe(&rs, "tourbiere", pm, 7)).collect();

    for pm in 1..=6 {
        assert!(
            mesures[pm] > mesures[pm - 1],
            "le cran {pm} doit rapporter plus que le précédent : {mesures:?}"
        );
    }
    // Six crans est le plafond : le septième PM n'ajoute rien.
    assert_eq!(
        mesures[7], mesures[6],
        "le plafond de six crans doit tenir : {mesures:?}"
    );
    // Cinq points de dégâts de BASE par cran, donc un écart constant une fois
    // le facteur du build appliqué. C'est ce qui distingue un vrai palier d'une
    // formule inventée qui s'emballerait.
    let ecarts: Vec<f64> = (1..=6).map(|i| mesures[i] - mesures[i - 1]).collect();
    let premier = ecarts[0];
    for (i, e) in ecarts.iter().enumerate() {
        assert!(
            (e - premier).abs() < 0.01,
            "les six crans valent le même montant, cran {} : {ecarts:?}",
            i + 1
        );
    }
}

/// Le Zénith DESCEND quand on dépense, puisqu'il lit les PM restants.
///
/// Contrôle du contrôle de la Tourbière : les deux sorts lisent le même réglage
/// en sens inverse. Un câblage qui confondrait « dépensés » et « restants »
/// ferait passer l'un et tomber l'autre.
#[test]
fn le_zenith_descend_avec_les_pm_depenses() {
    let rs = regles("iop", 8);
    let plein = frappe(&rs, "zenith", 0, 4);
    let moitie = frappe(&rs, "zenith", 2, 4);
    let vide = frappe(&rs, "zenith", 4, 4);

    assert!(
        plein > moitie && moitie > vide,
        "le Zénith perd sa seconde ligne à mesure qu'on dépense : {plein:.1} / {moitie:.1} / {vide:.1}"
    );
    // À PM pleins la seconde ligne vaut le double de ce qu'elle vaut à moitié,
    // la proportion étant linéaire.
    let seconde_pleine = plein - vide;
    let seconde_moitie = moitie - vide;
    assert!(
        (seconde_pleine - 2.0 * seconde_moitie).abs() < 1.0,
        "la proportion est linéaire : {seconde_pleine:.1} contre deux fois {seconde_moitie:.1}"
    );
}

/// Sans PM déclarés, rien ne change pour les autres sorts.
///
/// Le réglage ne doit pas fuir sur des sorts qui n'en parlent pas : il en
/// deviendrait un facteur d'échelle global et n'apprendrait plus rien.
#[test]
fn le_reglage_ne_touche_que_les_sorts_qui_le_lisent() {
    let rs = regles("iop", 8);
    let sans = frappe(&rs, "pression", 0, 4);
    let avec = frappe(&rs, "pression", 4, 4);
    assert_eq!(
        sans, avec,
        "la Pression ne parle pas de PM et ne doit pas bouger : {sans} / {avec}"
    );
}
