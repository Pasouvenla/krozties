//! Marks that answer what another spell does to the target.
//!
//! The Enutrof's Éboulement, Monnaie Sonnante and Orpaillage each place a mark
//! that fires when the target suffers a Range, AP or MP removal, four times,
//! then vanishes. What triggers them is the attempt, not its success, so the
//! target's removal resistance does not stop them. Modelled with a tag on the
//! removing spells rather than a list of spell ids inside the mark: the mark
//! has no business knowing which spells remove AP.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn enutrof() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/enutrof.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-3.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn solve(deck: &[&str]) -> Solution {
    let build = Build {
        name: "Enutrof".into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 142,
            elements: [ElementStats {
                characteristic: 650,
                flat_damage: 125,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 100,
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
    Engine::new(&enutrof(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

fn procs(s: &Solution, mark: &str) -> Vec<f64> {
    s.turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .flat_map(|c| c.procs.iter())
        .filter(|(source, _)| source == mark)
        .map(|(_, d)| d.as_f64())
        .collect()
}

/// A removal spell cast while the mark is up deals the mark's damage on top of
/// its own, and the mark keeps answering until its four charges are gone.
#[test]
fn a_range_removal_makes_the_mark_answer() {
    let s = solve(&["eboulement", "lancer_de_pelle", "remblai"]);
    let p = procs(&s, "eboulement");
    assert!(
        !p.is_empty(),
        "la marque n'a jamais répondu : le déclencheur par étiquette ne part pas"
    );
    assert!(
        p.iter().all(|d| *d > 0.0),
        "un proc à zéro n'est pas un proc : {p:?}"
    );
}

/// Four answers per application, no more. The mark is removed on the fourth,
/// which is what "déclenchables 4 fois, l'état est retiré dès la limite
/// atteinte" says.
#[test]
fn the_mark_answers_four_times_per_application() {
    let s = solve(&["eboulement", "lancer_de_pelle", "remblai"]);
    // Éboulement has a three-turn relaunch interval, so over five turns it is
    // cast twice at most: at most eight answers.
    let poses = s
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == "eboulement")
        .count();
    let reponses = procs(&s, "eboulement").len();
    assert!(
        reponses <= poses * 4,
        "{reponses} réponses pour {poses} pose(s) : la marque dépasse ses quatre charges"
    );
    assert!(
        poses > 0 && reponses > 0,
        "rien n'a été joué : {poses}/{reponses}"
    );
}

/// A spell that removes nothing never makes the mark answer. Remblai removes
/// Range and Banqueroute does not, so the tag has to separate them.
#[test]
fn an_untagged_spell_leaves_the_mark_alone() {
    let s = solve(&["eboulement", "banqueroute"]);
    assert!(
        procs(&s, "eboulement").is_empty(),
        "Banqueroute ne retire pas de Portée, elle ne doit rien déclencher"
    );
}

/// And without the mark, a removal spell is just a removal spell.
#[test]
fn a_removal_without_the_mark_procs_nothing() {
    let s = solve(&["lancer_de_pelle", "remblai"]);
    assert!(
        procs(&s, "eboulement").is_empty(),
        "aucune marque posée, aucun proc possible"
    );
}
