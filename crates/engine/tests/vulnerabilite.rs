//! Une vulnérabilité qui ne sert que dans le tour où on la pose.
//!
//! Quatre sorts posent « Dommages subis x N % » sur leur cible : le Fer Rouge du
//! Forgelance, la Décimation du Sacrieur, le Coupe-gorge du Sram et le Volcan du
//! Huppermage. Elle tient de la pose jusqu'au début du prochain tour du
//! lanceur : le lanceur seul n'en profite que dans le même tour d'actions.
//!
//! `turns: 1` ne dit pas cela : les durées se décomptent en fin de tour, et un
//! état posé au tour N avec `turns: 1` vit encore tout le tour N+1. C'est
//! `turns: 0` qui veut dire « ce tour-ci ».

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn classe(nom: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{nom}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    rs.merge_snapshot(
        &Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap(),
    );
    rs
}

fn moteur(rs: &Ruleset, deck: &[&str], horizon: u8) -> Engine {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 100,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 500,
                flat_damage: 0,
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
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        horizon,
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(rs, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

/// Elle amplifie ce qui la suit DANS LE MEME TOUR.
#[test]
fn la_vulnerabilite_amplifie_les_lancers_suivants() {
    let rs = classe("sram", 4);
    // Coupe-gorge pose x110 %. Sournoiserie, mono-ligne, sert de temoin.
    let s = moteur(&rs, &["coupe_gorge", "sournoiserie"], 1).solve();
    let lancers = &s.turns[0].casts;
    let apres: Vec<i64> = lancers
        .iter()
        .skip_while(|c| c.id != "coupe_gorge")
        .skip(1)
        .filter(|c| c.id == "sournoiserie")
        .map(|c| c.damage.0)
        .collect();
    let nu = moteur(&rs, &["sournoiserie"], 1).solve().turns[0].casts[0]
        .damage
        .0;
    assert!(
        !apres.is_empty(),
        "il faut au moins un lancer apres le Coupe-gorge : {lancers:?}"
    );
    for d in &apres {
        assert!(
            *d > nu,
            "un lancer sous vulnerabilite doit depasser le meme lancer sans : {d} contre {nu}"
        );
    }
}

/// Et elle ne survit PAS au tour.
///
/// C'est la moitie du sujet : une vulnerabilite qu'on croit permanente gonfle
/// toute la rotation. Le test compare le premier lancer de chaque tour, celui
/// qui tombe avant que le Coupe-gorge n'ait rien pu poser.
#[test]
fn elle_ne_survit_pas_au_tour() {
    let rs = classe("sram", 4);
    let s = moteur(&rs, &["coupe_gorge", "sournoiserie"], 3).solve();
    let premiers: Vec<i64> = s
        .turns
        .iter()
        .filter_map(|t| t.casts.first())
        .map(|c| c.damage.0)
        .collect();
    assert!(premiers.len() >= 3, "trois tours attendus : {premiers:?}");
    assert!(
        premiers.windows(2).all(|w| w[0] == w[1]),
        "le premier lancer de chaque tour doit valoir autant : si le second monte, \
         c'est que la vulnerabilite a survecu a la nuit ({premiers:?})"
    );
}
