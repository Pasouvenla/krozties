//! À partir de 78 de critique au build, tout coup qui peut critiquer critique.
//! En dessous, le taux reste celui du sort plus celui du build.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

/// Deux sorts identiques à une chose près : l'un peut critiquer, l'autre non.
/// Le taux de base est nul, pour que seul celui du build compte.
fn banc() -> Ruleset {
    Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources: []
spells:
  - id: avec_crit
    name: { fr: "Avec critique", en: "Can crit" }
    ap_cost: { base: 3 }
    casts_per_turn: 1
    crit: { base_rate: 0 }
    lines:
      - { element: water, critical: [40, 40], normal: [10, 10] }
    effects:
      - effect: damage
  - id: sans_crit
    name: { fr: "Sans critique", en: "Cannot crit" }
    ap_cost: { base: 3 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      - { element: water, critical: [40, 40], normal: [10, 10] }
    effects:
      - effect: damage
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Les dégâts d'un lancer, pour ce critique au build.
fn degats(critique_du_build: i32, spell: &str) -> f64 {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            // Un critique doit valoir nettement plus qu'un coup normal : 180
            // contre 20 ici.
            flat_crit_damage: 100,
            elements: [ElementStats {
                characteristic: 100,
                flat_damage: 0,
            }; 5],
            ..Default::default()
        },
        base_ap: 3,
        base_mp: 3,
        crit_bonus_percent: critique_du_build,
        modifiers: vec![],
        deck: vec![spell.into()],
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
    Engine::new(&banc(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .total
        .as_f64()
}

#[test]
fn a_78_de_critique_tout_coup_critique() {
    let plein = degats(100, "avec_crit");
    let a_78 = degats(CRITIQUE_CERTAIN, "avec_crit");
    let a_77 = degats(CRITIQUE_CERTAIN - 1, "avec_crit");
    assert!((a_78 - plein).abs() < 0.001, "{a_78} contre {plein}");
    // A 77, le taux reste 77 % : 0,23 x 20 + 0,77 x 180, soit 143 contre 180.
    assert!(a_77 < a_78 * 0.85, "{a_77} contre {a_78}");
}

#[test]
fn un_sort_qui_ne_critique_pas_ne_critique_jamais() {
    let a_0 = degats(0, "sans_crit");
    assert!(a_0 > 0.0);
    assert!((degats(CRITIQUE_CERTAIN, "sans_crit") - a_0).abs() < 0.001);
}

/// Le seuil des valeurs critiques des effets est le même : 78 %. Un état qui
/// ne vaut rien en normal et 100 de caractéristique en critique, lu par un sort
/// à 5 % de base. À 77 au build, le taux vaut 82 % : sous le critique sûr, mais
/// au-dessus du seuil, donc la valeur critique.
#[test]
fn a_partir_de_78_la_valeur_critique_d_un_effet_s_applique() {
    let rs = Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - id: elan
    scope: caster
    max: 1
    default: 0
    monotone: increasing
    modifies_damage:
      - { kind: characteristic, amount: 0, critical_amount: 100 }
spells:
  - id: poser
    name: { fr: "Poser", en: "Set" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - { effect: gain, resource: elan }
  - id: frapper
    name: { fr: "Frapper", en: "Hit" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 5 }
    lines:
      - { element: water, critical: [10, 10], normal: [10, 10] }
    effects:
      - effect: damage
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let frappe = |critique_du_build: i32| {
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
            base_ap: 2,
            base_mp: 3,
            crit_bonus_percent: critique_du_build,
            modifiers: vec![],
            deck: vec!["poser".into(), "frapper".into()],
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
        let sol = Engine::new(&rs, build, sc).unwrap_or_else(|e| panic!("{e}")).solve();
        sol.turns[0]
            .casts
            .iter()
            .find(|c| c.id == "frapper")
            .map(|c| c.damage.as_f64())
            .unwrap_or_else(|| panic!("{sol}"))
    };
    // 10 de base : 20 à 100 de caractéristique, 30 avec les 100 du critique.
    assert!((frappe(77) - 30.0).abs() < 0.01, "82 % : la valeur critique, {}", frappe(77));
    assert!((frappe(60) - 20.0).abs() < 0.01, "65 % : la valeur normale, {}", frappe(60));
}
