//! Des cumuls qui durent chacun trois tours, portés par un seul total.
//!
//! Les dommages finaux du Pacte Bestial : chaque invocation sacrifiée donne les
//! siens pour trois tours, cumulables sans limite. Le total porte le bonus, une
//! chaîne de trois compteurs fait vieillir chaque cumul ; le bout de la chaîne
//! retire du total ce qui arrive à son terme (`drains`).

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

fn banc() -> Ruleset {
    Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - id: bonus
    scope: caster
    max: 20
    default: 0
    modifies_damage:
      - kind: final_multiplier
        percent: 110
        finaux: true
  - id: age0
    scope: caster
    max: 20
    default: 0
    shifts_into: age1
  - id: age1
    scope: caster
    max: 20
    default: 0
    shifts_into: age2
  - id: age2
    scope: caster
    max: 20
    default: 0
    drains: bonus
spells:
  - id: sacrifice
    name: { fr: "Sacrifice", en: "Sacrifice" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - effect: gain
        resource: bonus
      - effect: gain
        resource: age0
  - id: coup
    name: { fr: "Coup", en: "Hit" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      - element: water
        critical: [10, 10]
        normal: [10, 10]
    effects:
      - effect: damage
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn total(horizon: u8) -> f64 {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats { characteristic: 100, flat_damage: 0 }; 5],
            ..Default::default()
        },
        base_ap: 2,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["sacrifice".into(), "coup".into()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon,
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
    Engine::new(&banc(), build, sc).unwrap_or_else(|e| panic!("{e}")).solve().total.as_f64()
}

/// Un sacrifice puis un coup de 20 par tour, chaque sacrifice valant +10 % pour
/// trois tours : 22, 24, 26, puis 26 tant que le plus ancien part quand un
/// nouveau arrive. Sans terme, le quatrième tour vaudrait 28 et le cinquième
/// 30.
#[test]
fn chaque_cumul_part_a_son_terme() {
    assert_eq!(total(3), 72.0);
    assert_eq!(total(5), 124.0);
}
