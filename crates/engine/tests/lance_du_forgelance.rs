//! La Lance est un état, et c'est ce qui commande toute la classe.
//!
//! Le Jormun « occasionne des dommages Eau en zone jusqu'à la Lance » quand elle
//! est plantée et « autour du lanceur » sinon : sans la Lance, c'est un tout
//! autre sort, joué en mêlée sur soi-même. La question est un ordre : la Lance
//! doit avoir été plantée par un autre sort. Onze sorts la plantent (`gain`),
//! dix-huit la rappellent (`consume`), quatorze exigent qu'elle le soit
//! (`requires`).
//!
//! Ce modèle ne tient pas où la Lance se trouve : la contrainte d'ordre est
//! respectée, celle de placement ne l'est pas.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn forgelance() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/forgelance.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-20.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn lances(deck: &[&str]) -> Vec<Vec<String>> {
    lances_a(deck, Reach::Ranged)
}

fn lances_a(deck: &[&str], portee: Reach) -> Vec<Vec<String>> {
    let build = Build {
        name: "Forgelance".into(),
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
        horizon: 3,
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
        reach: portee,
    };
    Engine::new(&forgelance(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .map(|t| t.casts.iter().map(|c| c.id.clone()).collect())
        .collect()
}

/// Sans un sort qui plante la Lance, ceux qui l'exigent ne partent JAMAIS.
#[test]
fn sans_invocateur_les_sorts_de_lance_ne_partent_pas() {
    let tours = lances(&["jormun", "terre_du_milieu", "muspel"]);
    assert!(
        tours.iter().all(|t| t.is_empty()),
        "aucun de ces trois sorts n'est jouable Lance en main : {tours:?}"
    );
}

/// Avec un invocateur, il passe D'ABORD, et les autres suivent.
#[test]
fn l_invocateur_passe_avant_ceux_qui_exigent_la_lance() {
    let tours = lances(&["jormun", "terre_du_milieu", "muspel", "lance_pierre"]);
    let t1 = &tours[0];
    assert!(!t1.is_empty(), "la rotation doit exister : {tours:?}");
    assert_eq!(
        t1[0], "lance_pierre",
        "le premier lancer doit planter la Lance : {t1:?}"
    );
    // Et ce n'est pas qu'une question d'ordre : les dependants sortent bien.
    assert!(
        t1.iter().any(|c| c == "jormun" || c == "terre_du_milieu"),
        "les sorts qui exigent la Lance doivent suivre : {t1:?}"
    );
}

/// Le Jormun rappelle la Lance : il faut la replanter avant le suivant. On
/// rejoue la rotation en suivant l'état d'un tour à l'autre, et aucun sort qui
/// exige la Lance ne doit partir alors qu'un rappel est passé sans
/// replantation.
#[test]
fn un_rappel_oblige_a_replanter() {
    let tours = lances(&[
        "jormun",
        "terre_du_milieu",
        "lance_pierre",
        "javelot_foudre",
    ]);
    const PLANTE: [&str; 2] = ["lance_pierre", "javelot_foudre"];
    const EXIGE: [&str; 2] = ["jormun", "terre_du_milieu"];
    let mut plantee = false;
    let mut verifies = 0;
    for (n, t) in tours.iter().enumerate() {
        for sort in t {
            if EXIGE.contains(&sort.as_str()) {
                assert!(
                    plantee,
                    "{sort} part sans Lance plantée, tour {} : {tours:?}",
                    n + 1
                );
                verifies += 1;
            }
            if PLANTE.contains(&sort.as_str()) {
                plantee = true;
            }
            // Le Jormun la rappelle en partant.
            if sort == "jormun" {
                plantee = false;
            }
        }
    }
    assert!(
        verifies >= 3,
        "trop peu de lancers contrôlés, le test ne prouve rien : {verifies}"
    );
}

/// Au contact, les mêmes sorts partent tout de suite : armé, ils frappent
/// autour du lanceur ; désarmé, depuis la Lance, avec les mêmes fourchettes. Au
/// corps à corps l'ennemi est dans la zone des deux côtés, donc rien ne bloque.
#[test]
fn au_contact_les_sorts_de_lance_partent_sans_elle() {
    let deck = &["jormun", "terre_du_milieu", "muspel"];
    let loin = lances_a(deck, Reach::Ranged);
    let contact = lances_a(deck, Reach::Melee);
    assert!(
        loin.iter().all(|t| t.is_empty()),
        "à distance, le mode armé frappe autour du lanceur : {loin:?}"
    );
    assert!(
        contact.iter().any(|t| !t.is_empty()),
        "au contact, ces sorts touchent dans les deux modes : {contact:?}"
    );
}

/// Un sort de soutien n'est bloqué NI au loin NI au contact.
///
/// Le Prélude au Fer se lance sur le lanceur s'il est armé, sur la Lance s'il
/// est désarmé. Sa Puissance arrive dans les deux cas, quelle que soit la
/// distance : il ne porte donc aucune garde.
#[test]
fn le_prelude_ne_depend_pas_de_la_lance() {
    let rs = forgelance();
    let prelude = rs
        .spells
        .iter()
        .find(|s| s.id == "prelude_au_fer")
        .expect("le Prélude est au fichier");
    assert!(
        prelude.requires.is_none(),
        "le Prélude au Fer ne doit exiger la Lance dans aucun sens"
    );
    for portee in [Reach::Ranged, Reach::Melee] {
        let t = lances_a(&["prelude_au_fer", "octave"], portee);
        assert!(
            t.iter()
                .any(|tour| tour.contains(&"prelude_au_fer".to_string())),
            "le Prélude doit partir à {portee:?} : {t:?}"
        );
    }
}
