//! Le facteur propre à une ligne, et la ligne qui ne critique jamais : le mur
//! de bombes du Roublard que Plombage redéclenche, dont le combo multiplie avec
//! les autres % et qui n'a pas de valeur critique, même quand Plombage
//! critique. Un banc synthétique, où seul ce qui est mesuré change d'un sort à
//! l'autre.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

/// Trois sorts au même jet, 10 en normal et 40 en critique, à 100 % de
/// critique : nu, avec un facteur de 150, avec ce facteur et sans critique.
fn banc() -> Ruleset {
    let sort_au_taux = |id: &str, extra: &str, taux: u8| {
        format!(
            r#"
  - id: {id}
    name: {{ fr: "{id}", en: "{id}" }}
    ap_cost: {{ base: 3 }}
    casts_per_turn: 1
    crit: {{ base_rate: {taux} }}
    lines:
      - element: water
        critical: [40, 40]
        normal: [10, 10]{extra}
    effects:
      - effect: damage"#
        )
    };
    let sort = |id: &str, extra: &str| sort_au_taux(id, extra, 100);
    let yaml = format!(
        r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: {{ fr: "Test", en: "Test" }}
resources: []
spells:{}{}{}{}{}{}
"#,
        sort("nu", ""),
        sort("facteur", "\n        facteur: 150"),
        sort("mur", "\n        facteur: 150\n        sans_critique: true"),
        sort("invoc", "\n        invocation: 50"),
        sort_au_taux("taux", "\n        taux_critique: 100", 0),
        sort("invoc_martinet", "\n        invocation: 50\n        facteur: 50"),
    );
    Ruleset::from_yaml(&yaml).unwrap_or_else(|e| panic!("{e}"))
}

/// Les dégâts d'un seul lancer du sort.
fn degats(spell: &str) -> f64 {
    degats_avec(spell, 0)
}

/// Les mêmes, avec des % de dommages aux sorts sur le build.
fn degats_avec(spell: &str, pourcent_sorts: i32) -> f64 {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 100,
            elements: [ElementStats { characteristic: 100, flat_damage: 0 }; 5],
            percent_spell: pourcent_sorts,
            ..Default::default()
        },
        base_ap: 3,
        base_mp: 3,
        crit_bonus_percent: 0,
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
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == spell)
        .map(|c| c.damage.as_f64())
        .sum()
}

/// Nu, le coup critique : 40 doublé par la caractéristique, plus 100 de
/// Dommages Critiques, 180. Le facteur multiplie le tout : 270. Sans
/// critique, le jet normal seul, 10 doublé, puis le facteur : 30.
#[test]
fn le_facteur_multiplie_et_le_mur_ne_critique_pas() {
    assert_eq!(degats("nu"), 180.0);
    assert_eq!(degats("facteur"), 270.0);
    assert_eq!(degats("mur"), 30.0);
}

/// Une attaque d'invocation : la moitié de la caractéristique, 40 × 1,5 = 60
/// sur le coup critique, sans les 100 de Dommages Critiques ; et 50 % de
/// dommages aux sorts du build n'y changent rien, quand le sort nu passe de
/// 180 à 270.
#[test]
fn l_attaque_d_invocation_ne_prend_que_la_part_transmise() {
    assert_eq!(degats("invoc"), 60.0);
    assert_eq!(degats_avec("nu", 50), 270.0);
    assert_eq!(degats_avec("invoc", 50), 60.0);
    // Le facteur lui reste : les 50 % du Martinet font 30.
    assert_eq!(degats("invoc_martinet"), 30.0);
}

/// Une ligne qui porte son propre taux le prend à la place de celui du sort :
/// un sort à 0 % dont la ligne dit 100 % critique toujours, 180.
#[test]
fn le_taux_propre_d_une_ligne() {
    assert_eq!(degats("taux"), 180.0);
}
