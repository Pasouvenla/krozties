//! Ce que des effets d'objet déclenchent d'eux-mêmes : une poussée au début de
//! chaque tour (`ResourceDef::poussees_par_tour`, les Bottes du Cul Botté), et
//! un gain qui ne part que d'un coup critique (`Effect::Gain::on_critical`, la
//! Plume de Buhorado). Un banc synthétique, un sort de 10 Eau fixe.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

fn banc(compteur: &str, taux: u8, gain: &str, poussees_bloquees: bool) -> Engine {
    let yaml = format!(
        r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: {{ fr: "Test", en: "Test" }}
resources:
{compteur}
spells:
  - id: frappe
    name: {{ fr: "Frappe", en: "Frappe" }}
    ap_cost: {{ base: 3 }}
    casts_per_turn: 2
    crit: {{ base_rate: {taux}, can_crit: true }}
    lines:
      - element: water
        normal: [10, 10]
        critical: [10, 10]
    effects:
      - effect: damage
{gain}
"#
    );
    let regles = Ruleset::from_yaml(&yaml).unwrap_or_else(|e| panic!("{e}"));
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            elements: [ElementStats { characteristic: 0, flat_damage: 0 }; 5],
            level: 200,
            ..Default::default()
        },
        base_ap: 6,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["frappe".into()],
    };
    let sc = Scenario {
        poussees_bloquees,
        targets: 1,
        horizon: 2,
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
        reach: Reach::Melee,
    };
    Engine::new(&regles, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

/// Deux poussées de deux cases au début de chaque tour, au niveau 200 :
/// `(100 + 32) × 2 / 4`, soit 66 chacune, et rien quand les poussées ne
/// butent pas.
#[test]
fn deux_poussees_au_debut_du_tour() {
    let compteur = r#"
  - id: bottes
    scope: caster
    max: 1
    default: 1
    poussees_par_tour: [2, 2]
"#;
    let tour = |bloquees| {
        let s = banc(compteur, 0, "", bloquees).solve();
        (s.turns[0].opening_damage.as_f64(), s.turns[0].damage.as_f64())
    };
    assert_eq!(tour(true), (132.0, 20.0));
    assert_eq!(tour(false), (0.0, 20.0));
}

/// Un gain de coup critique : +50 % de dommages finaux au lancer suivant,
/// seulement quand la Frappe critique au seuil de la règle.
#[test]
fn un_gain_sur_coup_critique() {
    let compteur = r#"
  - id: plume
    scope: caster
    max: 1
    default: 0
    modifies_damage:
      - { kind: final_multiplier, percent: 150, finaux: true }
"#;
    let gain = "      - effect: gain\n        resource: plume\n        on_critical: true";
    let tour = |taux| banc(compteur, taux, gain, false).solve().turns[0].damage.as_f64();
    assert_eq!(tour(100), 25.0);
    assert_eq!(tour(50), 20.0);
}

/// Des cumuls qui tiennent chacun trois tours pour eux-mêmes, sans plafond :
/// la Plume de Buhorado en 3.7. Une chaîne de trois compteurs fait vieillir les
/// cumuls de chaque tour et les retire du total au bout du troisième
/// (`shifts_into`, `drains`). Deux Frappes critiques par tour, qui poussent de 4
/// cases contre un obstacle : `132 + 10 × cumuls` de poussée chacune, plus ses
/// 10 Eau. Les cumuls vus : 0 et 1, 2 et 3, 4 et 5, puis 4 et 5 encore, ceux du
/// premier tour étant tombés.
#[test]
fn des_cumuls_de_trois_tours_sans_plafond() {
    let yaml = r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - id: plume
    scope: caster
    max: 30
    default: 0
    modifies_damage:
      - { kind: push_damage, amount: 10 }
  - id: plume_0
    scope: caster
    max: 10
    default: 0
    shifts_into: plume_1
  - id: plume_1
    scope: caster
    max: 10
    default: 0
    shifts_into: plume_2
  - id: plume_2
    scope: caster
    max: 10
    default: 0
    drains: plume
spells:
  - id: frappe
    name: { fr: "Frappe", en: "Frappe" }
    ap_cost: { base: 3 }
    casts_per_turn: 2
    crit: { base_rate: 100, can_crit: true }
    pushes: [{ cells: 4 }]
    lines:
      - element: water
        normal: [10, 10]
        critical: [10, 10]
    effects:
      - effect: damage
      - { effect: gain, resource: plume, on_critical: true }
      - { effect: gain, resource: plume_0, on_critical: true }
"#;
    let regles = Ruleset::from_yaml(yaml).unwrap_or_else(|e| panic!("{e}"));
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            elements: [ElementStats { characteristic: 0, flat_damage: 0 }; 5],
            level: 200,
            ..Default::default()
        },
        base_ap: 6,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["frappe".into()],
    };
    let sc = Scenario {
        poussees_bloquees: true,
        targets: 1,
        horizon: 4,
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
        reach: Reach::Melee,
    };
    let s = Engine::new(&regles, build, sc).unwrap_or_else(|e| panic!("{e}")).solve();
    let tours: Vec<f64> = s.turns.iter().map(|t| t.damage.as_f64()).collect();
    assert_eq!(tours, [294.0, 334.0, 374.0, 374.0]);
}
