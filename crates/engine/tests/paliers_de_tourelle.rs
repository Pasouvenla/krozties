//! Une tourelle du Steamer à paliers, sur un banc synthétique.
//!
//! Un seul compteur dit si la tourelle est là et à quel palier : 0 absente,
//! 1 à 3 pour l'Évolution I à III. Elle frappe au début de chaque tour selon
//! son palier. La Surtension la porte à l'Évolution III puis lui retire un
//! cran au tour suivant, après qu'elle a joué (`lose_at_turn_start_while`) ;
//! le Sabotage la rétrograde, jamais sous l'Évolution I (`consume` sous
//! condition).

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

fn banc() -> Ruleset {
    let palier = |k: u8, degats: u8| {
        format!(
            r#"
          - element: water
            critical: [{degats}, {degats}]
            normal: [{degats}, {degats}]
            active_at: {{ resource: tourelle, exactly: {k} }}"#
        )
    };
    let sort = |id: &str, effets: &str| {
        format!(
            r#"
  - id: {id}
    name: {{ fr: "{id}", en: "{id}" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines: []
    effects:{effets}"#
        )
    };
    Ruleset::from_yaml(&format!(
        r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: {{ fr: "Test", en: "Test" }}
resources:
  - id: tourelle
    scope: caster
    max: 3
    default: 0
    lose_at_turn_start_while: surtension
    while_present:
      - trigger: turn_start
        lines:{}{}{}
  - id: surtension
    scope: caster
    max: 1
    default: 0
    duration: {{ turns: 1 }}
spells:{}{}{}
"#,
        palier(1, 10),
        palier(2, 20),
        palier(3, 30),
        sort("pose", "\n      - effect: gain\n        resource: tourelle"),
        sort(
            "monte",
            "\n      - effect: gain\n        resource: tourelle\n        amount: 3\n      - effect: gain\n        resource: surtension"
        ),
        sort(
            "retrograde",
            "\n      - effect: consume\n        resource: tourelle\n        requires: { kind: at_least, scope: caster, resource: tourelle, amount: 2 }"
        ),
    ))
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Les dégâts de début de tour, tour par tour.
fn ouvertures(tours: &[&[&str]]) -> Vec<f64> {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats { characteristic: 100, flat_damage: 0 }; 5],
            ..Default::default()
        },
        base_ap: 6,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: ["pose", "monte", "retrograde"].iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon: 1,
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
    let tours: Vec<Vec<String>> = tours.iter().map(|t| t.iter().map(|s| s.to_string()).collect()).collect();
    Engine::new(&banc(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .replay(&tours)
        .unwrap_or_else(|e| panic!("{e}"))
        .turns
        .iter()
        .map(|t| t.opening_damage.as_f64())
        .collect()
}

/// Posée, la tourelle frappe à l'Évolution I (20). Surtensée, elle joue son
/// tour à l'Évolution III (60), puis redescend d'un cran : Évolution II (40),
/// et pas plus bas, l'état de la Surtension tombé.
#[test]
fn la_surtension_retombe_d_un_cran_apres_le_tour_de_la_tourelle() {
    assert_eq!(ouvertures(&[&["pose"], &["monte"], &[], &[], &[]]), [0.0, 20.0, 60.0, 40.0, 40.0]);
}

/// Rétrogradée à l'Évolution I, elle y reste et frappe encore ; à
/// l'Évolution III, elle perd un cran.
#[test]
fn le_sabotage_ne_retire_pas_la_tourelle() {
    assert_eq!(ouvertures(&[&["pose"], &["retrograde"], &[]]), [0.0, 20.0, 20.0]);
    assert_eq!(ouvertures(&[&["pose"], &["monte"], &["retrograde"], &[]]), [0.0, 20.0, 60.0, 20.0]);
}
