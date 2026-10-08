//! Le bouton « Annuler » : une recherche s'interrompt en cours de route.
//!
//! Seul test de ce fichier, exprès : le drapeau d'arrêt est unique pour tout le
//! processus, et chaque résolution l'oublie en démarrant. Un fichier de tests
//! d'intégration est un processus à part.

use std::time::{Duration, Instant};

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn moteur() -> Engine {
    let racine = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    // L'Ecaflip et ses seize cartes : quelques secondes, assez pour prendre un
    // arrêt en vol sans allonger la suite.
    let mut rs = Ruleset::load(format!("{racine}/data/rulesets/ecaflip.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{racine}/data/snapshots/breed-6.json")).unwrap();
    rs.merge_snapshot(&snap);
    let rs: &'static Ruleset = Box::leak(Box::new(rs));

    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 100,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 300,
                flat_damage: 0,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: [
            "pile_ou_face",
            "topkaj",
            "esprit_felin",
            "baraka",
            "griffe_joueuse",
            "griffe_de_ceangal",
            "rekop",
            "langue_rapeuse",
            "reflexes",
            "destin_d_ecaflip",
            "felintion",
            "jass",
            "belote",
            "blakjak",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: vec![],
        horizon: 8,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        targets: 3,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: rs
            .budgets
            .iter()
            .map(|b| (b.id.clone(), b.default))
            .collect(),
        dominance: true,
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(rs, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

/// Une recherche qu'on arrête rend `None`, et vite. Ce test ne couvre pas la
/// lecture de l'arrêt à l'intérieur d'une vague : les vagues de l'Ecaflip sont
/// courtes, et une classe à vagues longues coûterait des minutes de tests.
#[test]
fn une_recherche_s_arrete_en_chemin_et_ne_rend_rien() {
    let moteur = moteur();

    let debut = Instant::now();
    let complet = moteur.solve();
    let entier = debut.elapsed();
    assert!(complet.total.as_f64() > 0.0);
    assert!(
        entier > Duration::from_millis(600),
        "ce banc doit durer assez pour qu'on l'interrompe, mesuré {entier:?}"
    );

    let fil = std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(150));
        ARRET.demander();
    });
    let debut = Instant::now();
    let arrete = moteur.solve_annulable();
    let ecoule = debut.elapsed();
    fil.join().unwrap();

    assert!(arrete.is_none(), "une recherche arrêtée ne rend pas de rotation");
    assert!(
        ecoule * 2 < entier,
        "l'arrêt a été lu trop tard : {ecoule:?} contre {entier:?} pour la recherche entière"
    );

    // Et la suivante repart : l'arrêt demandé ne vaut que pour la recherche en
    // cours, sinon le bouton « Calculer » resterait mort après une annulation.
    let apres = moteur.solve_annulable();
    assert!(
        apres.is_some_and(|s| (s.total.as_f64() - complet.total.as_f64()).abs() < 0.01),
        "la recherche suivante doit rendre le même total qu'avant l'arrêt"
    );
}
