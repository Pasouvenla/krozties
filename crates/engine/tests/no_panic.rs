//! No deck may bring the engine down. A spell that cannot critical carries no
//! critical range, and the damage computation must not expect one: with
//! `panic = "abort"` a panic would take the whole server with it. This file
//! guards that; the release profile unwinding, plus `catch_unwind` per request,
//! makes a bad deck cost one answer rather than the session.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

const CLASSES: &[(&str, u32)] = &[
    ("feca", 1),
    ("osamodas", 2),
    ("enutrof", 3),
    ("sram", 4),
    ("xelor", 5),
    ("ecaflip", 6),
    ("eniripsa", 7),
    ("iop", 8),
    ("cra", 9),
    ("sadida", 10),
    ("sacrieur", 11),
    ("pandawa", 12),
    ("roublard", 13),
    ("zobal", 14),
    ("steamer", 15),
    ("eliotrope", 16),
    ("huppermage", 17),
    ("ouginak", 18),
    ("forgelance", 20),
];

fn load(name: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{name}.yaml"))
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn profile() -> DamageProfile {
    DamageProfile {
        power: 150,
        flat_crit_damage: 100,
        elements: [ElementStats {
            characteristic: 600,
            flat_damage: 80,
        }; 5],
        ..Default::default()
    }
}

/// Ce qui rend lançables les cinq premiers sorts à dégâts d'une classe : ceux
/// du Zobal exigent tous un masque.
const ENABLERS: &[(&str, &[&str])] = &[("zobal", &["masque_de_l_intrepide"])];

/// Every class solves a rotation from its own damaging spells.
///
/// Five spells each: enough to exercise the pipeline on every class without the
/// combinatorics of a full deck, which is a separate and known problem.
#[test]
fn every_class_solves_a_rotation() {
    let mut au_plafond = 0;
    for (name, breed) in CLASSES {
        let rs = load(name, *breed);
        let deck: Vec<String> = rs
            .spells
            .iter()
            .filter(|s| !s.lines.is_empty())
            .take(5)
            .map(|s| s.id.clone())
            .chain(
                ENABLERS
                    .iter()
                    .filter(|(c, _)| c == name)
                    .flat_map(|(_, s)| s.iter().map(|s| s.to_string())),
            )
            .collect();
        assert!(
            deck.len() >= 2,
            "{name} offers {} damaging spells, too few to solve anything",
            deck.len()
        );
        let build = Build {
            name: (*name).into(),
            profile: profile(),
            base_ap: 12,
            base_mp: 3,
            crit_bonus_percent: 20,
            modifiers: vec![],
            deck,
        };
        let sc = Scenario {
            poussees_bloquees: false,
            horizon: 4,
            pm_depenses: 0,
            etalement: 0,
            placement: None,
            etats_declares: vec![],
            targets: 1,
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
        let engine = Engine::new(&rs, build, sc).unwrap_or_else(|e| panic!("{name}: {e}"));
        let solution = engine.solve();
        assert!(
            solution.total.0 > 0,
            "{name} solved to zero damage from spells that all carry damage lines"
        );
        // Un cycle, ou une raison de ne pas l'avoir cherché : un cycle vide
        // sans raison est un défaut, et c'est lui que cette ligne attrape. Le
        // Huppermage aux runes dépasse la borne d'états du chercheur de cycle ;
        // la page le dit au joueur.
        let steady = engine.steady_state();
        assert!(
            !steady.cycle.is_empty() || steady.gave_up.is_some(),
            "{name} found no repeatable cycle, and no reason for it"
        );
        // Et quand elle renonce, elle s'arrête à son plafond au lieu de bâtir
        // tout le graphe pour le constater.
        if steady.gave_up.is_some() {
            au_plafond += 1;
            assert!(
                steady.states <= 2 * Engine::MAX_CYCLE_STATES
                    && steady.edges <= 2 * Engine::MAX_CYCLE_EDGES,
                "{name} : {} états et {} arêtes parcourus pour renoncer",
                steady.states,
                steady.edges
            );
        }
    }
    assert!(
        au_plafond > 0,
        "aucune classe ne fait plus renoncer la boucle : la garde ne contrôle plus rien"
    );
}

/// A spell that cannot critical must compute rather than panic. Huppermage's
/// Runification is the case: four life-steal lines, no critical lines at all,
/// and a base critical rate of zero. These spells trigger runes, which only the
/// elemental spells lay: the deck carries Lance-flamme for that.
#[test]
fn a_spell_that_cannot_critical_still_computes() {
    let rs = load("huppermage", 17);
    let sans_crit: Vec<String> = rs
        .spells
        .iter()
        .filter(|s| !s.crit.can_crit && !s.lines.is_empty())
        .map(|s| s.id.clone())
        .collect();
    assert!(
        !sans_crit.is_empty(),
        "no spell without criticals left in Huppermage; this test needs one"
    );
    let mut deck = sans_crit.clone();
    deck.push("lance_flamme".to_string());
    let build = Build {
        name: "Huppermage".into(),
        profile: profile(),
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 20,
        modifiers: vec![],
        deck,
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
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let solution = Engine::new(&rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve();
    let frappes_sans_crit: f64 = solution
        .turns
        .iter()
        .flat_map(|t| &t.casts)
        .filter(|c| sans_crit.contains(&c.id))
        .map(|c| c.damage.as_f64())
        .sum();
    assert!(frappes_sans_crit > 0.0, "{solution}");
}
