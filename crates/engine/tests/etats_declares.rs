//! Les compteurs que le joueur renseigne lui-même : une quantité que le solveur
//! ne peut pas connaître et que le joueur connaît (combien d'ennemis sont collés
//! à lui, combien de tourelles il a posées, combien d'arbres sont plantés).
//!
//! Le critère : la source énumère les paliers (« 2 ennemis au contact du
//! lanceur », avec sa fourchette en face), et un chiffre se déclare. « Si la
//! cible est poussée » n'en est pas un, et ces cas-là restent des questions
//! ouvertes.

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

/// Le deck plutôt qu'un sort seul : la Muselière de l'Ouginak ne paie son
/// compteur que si la cible porte la Proie, et la Proie se lance.
fn frappe(rs: &Ruleset, deck: &[&str], etat: &str, valeur: u8) -> f64 {
    let build = Build {
        name: deck[deck.len() - 1].into(),
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
        deck: deck.iter().map(|s| (*s).to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 1,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![(etat.to_string(), valeur)],
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

/// Les deux sorts montent avec leur compteur, et s'arrêtent à son plafond : un
/// seul test pour les deux, c'est le même mécanisme, et ce qui change d'une
/// ligne à l'autre est le plafond.
#[test]
fn chaque_compteur_monte_puis_plafonne() {
    let cas: &[(&str, u32, &[&str], &str, u8)] = &[
        // Deux sorts pour l'Ouginak : le compteur ne paie que « si la cible est
        // la Proie », que le sort du même nom pose pour 1 PA. Avec la seule
        // Muselière au deck, les six mesures sont identiques, ce que garde
        // `museliere.rs`.
        (
            "ouginak",
            18,
            &["proie", "museliere"],
            "ennemis_au_contact",
            4,
        ),
        ("sadida", 10, &["force_de_la_nature"], "arbres_feuillus", 6),
    ];
    for (classe, breed, deck, etat, plafond) in cas {
        let sort = deck[deck.len() - 1];
        let rs = regles(classe, *breed);
        let mesures: Vec<f64> = (0..=plafond + 1)
            .map(|n| frappe(&rs, deck, etat, n))
            .collect();
        for n in 1..=usize::from(*plafond) {
            assert!(
                mesures[n] > mesures[n - 1],
                "{classe}/{sort} : le cran {n} doit rapporter plus : {mesures:?}"
            );
        }
        assert_eq!(
            mesures[usize::from(*plafond) + 1],
            mesures[usize::from(*plafond)],
            "{classe}/{sort} : le plafond de {plafond} doit tenir : {mesures:?}"
        );
        // Tous les crans valent le même montant : c'est ce qui distingue un
        // palier lu dans la source d'une formule inventée.
        let ecarts: Vec<f64> = (1..=usize::from(*plafond))
            .map(|i| mesures[i] - mesures[i - 1])
            .collect();
        for e in &ecarts {
            assert!(
                (e - ecarts[0]).abs() < 0.01,
                "{classe}/{sort} : les crans doivent valoir pareil : {ecarts:?}"
            );
        }
    }
}

/// Un compteur non déclaré vaut zéro, et le sort vaut son plancher.
///
/// Contrôle du contrôle : si l'état démarrait ailleurs qu'à zéro, le test
/// précédent passerait quand même, toute la courbe étant décalée.
#[test]
fn un_compteur_non_declare_vaut_zero() {
    let rs = regles("ouginak", 18);
    let deck = ["proie", "museliere"];
    let declare_zero = frappe(&rs, &deck, "ennemis_au_contact", 0);
    let rien_declare = frappe(&rs, &deck, "compteur_qui_n_existe_pas", 3);
    assert_eq!(
        declare_zero, rien_declare,
        "ne rien déclarer doit valoir déclarer zéro : {declare_zero} / {rien_declare}"
    );
}

/// Rien ne fait bouger ces compteurs pendant le combat.
///
/// Ils décrivent une situation que le joueur pose, pas une ressource qui se
/// gagne : un sort qui en gagnerait un ferait mentir le chiffre saisi.
#[test]
fn aucun_sort_ne_gagne_ces_compteurs() {
    for (classe, breed, etat) in [
        ("ouginak", 18u32, "ennemis_au_contact"),
        ("steamer", 15, "tourelles_au_contact"),
        ("sadida", 10, "arbres_feuillus"),
    ] {
        let rs = regles(classe, breed);
        for s in &rs.spells {
            for e in &s.effects {
                let touche = match e {
                    dofus_ruleset::Effect::Gain { resource, .. }
                    | dofus_ruleset::Effect::Consume { resource, .. }
                    | dofus_ruleset::Effect::Reset { resource, .. } => resource == etat,
                    _ => false,
                };
                assert!(
                    !touche,
                    "{classe} : {} touche à {etat}, qui est renseigné par le joueur",
                    s.name.fr
                );
            }
        }
    }
}
