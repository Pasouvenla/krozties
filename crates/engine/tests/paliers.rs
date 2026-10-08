//! Les compteurs à paliers : des bonus qui dépendent de la valeur d'un
//! compteur, et non de son nombre de charges (`ResourceDef::paliers`), des PA
//! par valeur, et un compteur qui reboucle. Ainsi s'écrivent les effets d'objet
//! qui changent d'un tour à l'autre : la Surpryz aux trois premiers tours, la
//! Dofusteuse sur quatre, le Diadème de Ganymède un tour sur deux. Un banc
//! synthétique, un sort de 10 Eau fixe sans critique.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

fn banc(compteur: &str, pa: u8) -> Engine {
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
    crit: {{ base_rate: 0, can_crit: false }}
    lines:
      - element: water
        normal: [10, 10]
        critical: [10, 10]
    effects:
      - effect: damage
"#
    );
    let regles = Ruleset::from_yaml(&yaml).unwrap_or_else(|e| panic!("{e}"));
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            elements: [ElementStats { characteristic: 0, flat_damage: 0 }; 5],
            ..Default::default()
        },
        base_ap: pa,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["frappe".into()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon: 5,
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
        reach: Reach::Ranged,
    };
    Engine::new(&regles, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

fn par_tour(moteur: &Engine) -> Vec<f64> {
    moteur.solve().turns.iter().map(|t| (t.opening_damage + t.damage).as_f64()).collect()
}

/// Un compteur qui monte d'un cran par tour et porte un palier par valeur :
/// +50 % de dommages finaux au tour 1, +20 % au tour 2, rien ensuite.
#[test]
fn un_palier_par_tour() {
    let compteur = r#"
  - id: phase
    scope: caster
    max: 4
    default: 0
    monotone: increasing
    gain_per_turn: 1
    paliers:
      - [{ kind: final_multiplier, percent: 150, finaux: true }]
      - [{ kind: final_multiplier, percent: 120, finaux: true }]
"#;
    assert_eq!(par_tour(&banc(compteur, 3)), [15.0, 12.0, 10.0, 10.0, 10.0]);
}

/// Un compteur qui reboucle sur deux : +10 % les tours impairs, +30 % les
/// pairs, sans fin.
#[test]
fn un_compteur_qui_reboucle() {
    let compteur = r#"
  - id: parite
    scope: caster
    max: 2
    default: 0
    gain_per_turn: 1
    cyclique: true
    paliers:
      - [{ kind: final_multiplier, percent: 110, finaux: true }]
      - [{ kind: final_multiplier, percent: 130, finaux: true }]
"#;
    assert_eq!(par_tour(&banc(compteur, 3)), [11.0, 13.0, 11.0, 13.0, 11.0]);
}

/// Des PA par valeur : 3 de plus au premier tour seulement, de quoi lancer
/// la Frappe deux fois.
#[test]
fn des_pa_au_premier_tour() {
    let compteur = r#"
  - id: phase
    scope: caster
    max: 2
    default: 0
    monotone: increasing
    gain_per_turn: 1
    pa_par_valeur: [3]
"#;
    assert_eq!(par_tour(&banc(compteur, 3)), [20.0, 10.0, 10.0, 10.0, 10.0]);
}
