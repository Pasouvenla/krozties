//! Une Puissance accordée au lanceur, qui profite à toute la suite.
//!
//! La Terre du Milieu du Forgelance « augmente la Puissance du lanceur pour
//! chaque ennemi dans la zone d'effet » : effet 138, cinquante points, deux
//! tours. La Puissance entre dans la formule au même endroit que la
//! caractéristique, `100 + carac + puissance`, et s'écrit donc en modificateur
//! `characteristic`. Ce test se tient sur une cible unique ; le bonus par
//! ennemi est l'objet de `puissance_par_cible.rs`.

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

/// Ce que chaque lancer vaut, tour par tour, sur un deck d'un seul sort.
fn par_tour(sort: &str, deck: &[&str]) -> Vec<f64> {
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
        horizon: 3,
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
    Engine::new(&forgelance(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .map(|t| {
            t.casts
                .iter()
                .filter(|c| c.id == sort)
                .map(|c| c.damage.as_f64())
                .sum()
        })
        .collect()
}

/// Le premier lancer se paie sans la Puissance, les suivants avec.
#[test]
fn la_puissance_posee_profite_au_lancer_suivant() {
    // Lance Pierre est indispensable ici : la Terre du Milieu exige que la
    // Lance soit plantée, et seule dans un deck, elle ne part jamais.
    let v = par_tour("terre_du_milieu", &["terre_du_milieu", "lance_pierre"]);
    assert!(
        v.len() >= 2 && v[0] > 0.0,
        "deux tours attendus, mesuré {v:?}"
    );
    assert!(
        v[1] > v[0],
        "le second lancer profite de la Puissance du premier, mesuré {v:?}"
    );
    // Cinquante points sur un facteur de 900, c'est un peu plus de cinq pour
    // cent avant les dommages fixes. Exiger l'AMPLEUR : une assertion
    // seulement directionnelle passerait avec un point de Puissance.
    let ecart = (v[1] - v[0]) / v[0];
    assert!(
        ecart > 0.03,
        "l'écart doit valoir plusieurs points, mesuré {:.1} % sur {v:?}",
        ecart * 100.0
    );
}

/// Et il profite aux AUTRES sorts, pas seulement à celui qui l'a posée.
#[test]
fn la_puissance_profite_aussi_au_reste_de_la_rotation() {
    // Le Fer Rouge plante lui-meme la Lance, il se suffit donc a lui seul.
    let seul = par_tour("fer_rouge", &["fer_rouge"]);
    let avec = par_tour("fer_rouge", &["fer_rouge", "terre_du_milieu"]);
    assert!(
        seul.len() >= 2 && avec.len() >= 2,
        "{seul:?} contre {avec:?}"
    );
    assert!(
        avec[1] > seul[1],
        "le Fer Rouge du second tour doit profiter de la Puissance, \
         mesuré {} seul contre {} accompagné",
        seul[1],
        avec[1]
    );
}
