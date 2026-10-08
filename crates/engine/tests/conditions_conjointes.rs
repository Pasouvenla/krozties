//! Plusieurs conditions à la fois (`all`), et « au plus » (`at_most`).
//!
//! Les tourelles du Steamer en ont besoin : Vapor fait évoluer une tourelle en
//! jeu, pas encore évoluée dans le tour, et que le joueur déclare dans la
//! zone. Banc synthétique, rejoué lancer par lancer.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

fn banc() -> Ruleset {
    let sort = |id: &str, ressource: &str| {
        format!(
            r#"
  - id: {id}
    name: {{ fr: "{id}", en: "{id}" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 3
    crit: {{ base_rate: 0, can_crit: false }}
    lines: []
    effects:
      - effect: gain
        resource: {ressource}"#
        )
    };
    Ruleset::from_yaml(&format!(
        r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: {{ fr: "Test", en: "Test" }}
resources:
  - {{ id: a, scope: caster, max: 1, default: 0 }}
  - {{ id: b, scope: caster, max: 3, default: 0 }}
  - {{ id: c, scope: caster, max: 1, default: 0 }}
  - {{ id: x, scope: caster, max: 1, default: 0 }}
spells:{}{}{}
  - id: cible
    name: {{ fr: "cible", en: "cible" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines: []
    effects:
      - effect: gain
        resource: x
        requires:
          kind: all
          all:
            - {{ kind: caster_has, resource: a }}
            - {{ kind: at_most, resource: b, amount: 1 }}
            - {{ kind: exactly, resource: c, amount: 0 }}
  - id: coup
    name: {{ fr: "coup", en: "coup" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines:
      - element: water
        critical: [10, 10]
        normal: [10, 10]
        base_bonus: [{{ kind: per_resource, resource: x, amount: 10 }}]
    effects:
      - effect: damage
"#,
        sort("pose_a", "a"),
        sort("monte_b", "b"),
        sort("pose_c", "c"),
    ))
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Les dégâts du coup qui clôt la séquence : 40 si `x` est posé, 20 sinon.
fn coup_apres(lancers: &[&str]) -> f64 {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats { characteristic: 100, flat_damage: 0 }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: ["pose_a", "monte_b", "pose_c", "cible", "coup"].iter().map(|s| s.to_string()).collect(),
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
    let tour: Vec<String> = lancers.iter().chain(&["coup"]).map(|s| s.to_string()).collect();
    let s = Engine::new(&banc(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .replay(&[tour])
        .unwrap_or_else(|e| panic!("{e}"));
    s.turns[0].casts.last().unwrap().damage.as_f64()
}

#[test]
fn toutes_les_conditions_a_la_fois() {
    // Les trois tiennent : `a` posé, `b` à 0 puis à 1 (au plus 1), `c` à 0.
    assert_eq!(coup_apres(&["pose_a", "cible"]), 40.0);
    assert_eq!(coup_apres(&["pose_a", "monte_b", "cible"]), 40.0);
    // Une seule manque, et rien ne se pose.
    assert_eq!(coup_apres(&["cible"]), 20.0);
    assert_eq!(coup_apres(&["pose_a", "monte_b", "monte_b", "cible"]), 20.0);
    assert_eq!(coup_apres(&["pose_a", "pose_c", "cible"]), 20.0);
}

/// Un compteur qu'une conjonction est seule à lire reste lu : ses écritures
/// ne sont pas retirées comme celles d'un compteur muet.
#[test]
fn une_conjonction_lit_tous_ses_compteurs() {
    assert_eq!(coup_apres(&["pose_a", "pose_c", "cible"]), 20.0);
    assert_eq!(coup_apres(&["pose_a", "monte_b", "monte_b", "cible"]), 20.0);
}

/// Le chargement refuse une conjonction de plus de trois clauses.
#[test]
fn quatre_clauses_sont_refusees() {
    let yaml = r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - { id: a, scope: caster, max: 1, default: 0 }
spells:
  - id: s
    name: { fr: "s", en: "s" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    requires:
      kind: all
      all:
        - { kind: caster_has, resource: a }
        - { kind: caster_has, resource: a }
        - { kind: caster_has, resource: a }
        - { kind: caster_has, resource: a }
    lines: []
    effects: []
"#;
    let e = Ruleset::from_yaml(yaml).map(|_| ()).unwrap_err().to_string();
    assert!(e.contains("au plus trois conditions"), "{e}");
}
