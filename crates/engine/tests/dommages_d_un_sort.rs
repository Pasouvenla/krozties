//! Les Dommages qu'un objet de classe donne à UN sort : « Main de Pandawa : +5
//! Dommages », la Sangle Huée (effet 283).
//!
//! Ils entrent au même étage que les Dommages de l'équipement, après l'arrondi
//! de la caractéristique : ce ne sont pas des dégâts de base (effet 293), que
//! la caractéristique multiplie. Un banc synthétique, où seul ce qui est mesuré
//! change d'un sort à l'autre.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

/// Trois sorts au même jet, 10 Eau, sans critique : nu, avec 5 Dommages
/// propres, et avec 5 de jet en plus, ce que donnent des dégâts de base.
fn banc() -> Ruleset {
    let sort = |id: &str, jet: i32, extra: &str| {
        format!(
            r#"
  - id: {id}
    name: {{ fr: "{id}", en: "{id}" }}
    ap_cost: {{ base: 3 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0 }}
    lines:
      - element: water
        critical: [{jet}, {jet}]
        normal: [{jet}, {jet}]{extra}
    effects:
      - effect: damage"#
        )
    };
    let yaml = format!(
        r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: {{ fr: "Test", en: "Test" }}
resources: []
spells:{}{}{}
"#,
        sort("nu", 10, ""),
        sort("dommages", 10, "\n        dommages_fixes: 5"),
        sort("base", 15, ""),
    );
    Ruleset::from_yaml(&yaml).unwrap_or_else(|e| panic!("{e}"))
}

fn moteur(spell: &str, pourcent_sorts: i32) -> Engine {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
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
    Engine::new(&banc(), build, sc).unwrap_or_else(|e| panic!("{e}"))
}

/// Les dégâts d'un seul lancer du sort.
fn degats(spell: &str, pourcent_sorts: i32) -> f64 {
    moteur(spell, pourcent_sorts)
        .solve()
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == spell)
        .map(|c| c.damage.as_f64())
        .sum()
}

/// 10 doublé par la caractéristique, 20. Les 5 Dommages s'ajoutent APRÈS :
/// 25. Les mêmes 5 en dégâts de base passent par la caractéristique : 30.
#[test]
fn les_dommages_du_sort_s_ajoutent_apres_la_caracteristique() {
    assert_eq!(degats("nu", 0), 20.0);
    assert_eq!(degats("dommages", 0), 25.0);
    assert_eq!(degats("base", 0), 30.0);
}

/// Comme les Dommages de l'équipement, ils passent ensuite par les % : 25 à
/// 50 % de dommages aux sorts, 37 une fois arrondi.
#[test]
fn les_pourcentages_les_multiplient() {
    assert_eq!(degats("nu", 50), 30.0);
    assert_eq!(degats("dommages", 50), 37.0);
}

/// L'infobulle lit la même chose : sa fourchette passe de 20 à 25.
#[test]
fn la_fourchette_de_l_infobulle_les_compte() {
    let ligne = |spell: &str| moteur(spell, 0).damage_table(spell)[0].normal;
    assert_eq!(ligne("nu"), (20, 20));
    assert_eq!(ligne("dommages"), (25, 25));
}
