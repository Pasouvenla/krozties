//! Un compteur qui ne monte qu'une fois par tour, sur quatre sorts différents.
//!
//! Le Chœur Strident de l'Eniripsa « augmente ses dommages à l'application
//! d'une Moquerie par le lanceur », cinq points de dégâts de base par cran et
//! trois crans au plus. Quatre sorts de la classe appliquent une Moquerie, et le
//! texte ajoute que « les effets ne peuvent être augmentés qu'une seule fois par
//! tour ». Le plafond porte sur la ressource et non sur le sort : les quatre
//! Moqueries se partagent une seule permission par tour. Une règle du jeu, pas
//! un budget réglable.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn eniripsa() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/eniripsa.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-7.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

/// Ce que le Chœur vaut à chaque tour, sur cinq tours.
fn choeur(deck: &[&str]) -> Vec<f64> {
    let build = Build {
        name: "Eniripsa".into(),
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
        horizon: 5,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&eniripsa(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .map(|t| {
            t.casts
                .iter()
                .filter(|c| c.id == "ch_ur_strident")
                .map(|c| c.damage.as_f64())
                .sum()
        })
        .collect()
}

const UNE: &[&str] = &["ch_ur_strident", "mot_de_declin"];
const TROIS: &[&str] = &[
    "ch_ur_strident",
    "mot_de_declin",
    "mot_decourageant",
    "mot_deprimant",
];

/// Une Moquerie au deck fait monter le Chœur ; sans elle il ne monte pas.
#[test]
fn les_crans_montent_avec_les_moqueries() {
    let sans = choeur(&["ch_ur_strident"]);
    let avec = choeur(UNE);
    assert!(sans.len() == 5 && avec.len() == 5, "{sans:?} / {avec:?}");
    assert!(
        sans.windows(2).all(|f| (f[0] - f[1]).abs() < 0.51),
        "sans Moquerie le Chœur ne doit pas bouger, mesuré {sans:?}"
    );
    assert!(
        avec[4] > sans[4],
        "avec une Moquerie il doit monter, mesuré {sans:?} contre {avec:?}"
    );
    // Deux crans à l'équilibre, cinq points de dégâts de base chacun : l'écart
    // se compte en dizaines. Exiger l'AMPLEUR, une assertion directionnelle
    // passant avec un seul point.
    let ecart = (avec[4] - sans[4]) / sans[4];
    assert!(
        ecart > 0.1,
        "l'écart doit valoir plus de dix pour cent, mesuré {:.1} %",
        ecart * 100.0
    );
}

/// Trois Moqueries au deck ne valent pas plus qu'une : un cran par tour. Sans
/// le plafond, la troisième mesure passerait de 543 à 588 par lancer.
#[test]
fn un_cran_par_tour_quel_que_soit_le_nombre_de_moqueries() {
    let une = choeur(UNE);
    let trois = choeur(TROIS);
    assert!(une[4] > 0.0, "le Chœur doit frapper, mesuré {une:?}");
    assert!(
        une.iter()
            .zip(trois.iter())
            .all(|(a, b)| (a - b).abs() < 0.51),
        "trois Moqueries ne doivent rien ajouter à une seule, mesuré {une:?} contre {trois:?}"
    );
}
