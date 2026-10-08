//! Une condition « exactement », pour les compteurs qui codent autre chose
//! qu'une quantité. L'élément de la rune du Huppermage vaut 1 pour la Terre, 2
//! pour le Feu, 3 pour l'Eau et 4 pour l'Air : « au moins 2 » y voudrait dire
//! « Feu, Eau ou Air ». Les lignes de dégâts lisent un cran exact par
//! `active_at`, les conditions de lancer et de gain par `exactly`.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

/// `marque` code une couleur. Poser le Feu la met à 2, poser l'Air à 4, et le
/// grand sort exige le Feu, exactement.
///
/// Poser l'Air frappe pour 20, poser le Feu pour rien : lu « au moins 2 », le
/// solveur poserait l'Air et lancerait quand même le grand sort, pour 140. La
/// règle exacte l'y oblige à renoncer : 100.
fn banc() -> Ruleset {
    Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - { id: marque, scope: caster, max: 4, default: 0, monotone: none }
spells:
  - id: poser_feu
    name: { fr: "Poser Feu", en: "Lay Fire" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - { effect: reset, resource: marque }
      - { effect: gain, resource: marque, amount: 2 }
  - id: poser_air
    name: { fr: "Poser Air", en: "Lay Air" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      - { element: water, critical: [10, 10], normal: [10, 10] }
    effects:
      - effect: damage
      - { effect: reset, resource: marque }
      - { effect: gain, resource: marque, amount: 4 }
  - id: grand_sort
    name: { fr: "Grand Sort", en: "Big Spell" }
    ap_cost: { base: 2 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    requires: { kind: exactly, resource: marque, amount: 2 }
    lines:
      - { element: water, critical: [50, 50], normal: [50, 50] }
    effects:
      - effect: damage
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn une_condition_exacte_refuse_les_valeurs_au_dessus() {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 100,
                flat_damage: 0,
            }; 5],
            ..Default::default()
        },
        base_ap: 3,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: ["poser_feu", "poser_air", "grand_sort"]
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: vec![],
        horizon: 1,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let sol = Engine::new(&banc(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve();
    let noms: Vec<&str> = sol.turns[0].casts.iter().map(|c| c.spell.as_str()).collect();
    assert_eq!(noms, ["Poser Feu", "Grand Sort"], "{sol}");
    assert!(
        (sol.total.as_f64() - 100.0).abs() < 0.01,
        "cinquante de base à caractéristique 100 : 100 attendu, mesuré {:.2}",
        sol.total.as_f64()
    );
}
