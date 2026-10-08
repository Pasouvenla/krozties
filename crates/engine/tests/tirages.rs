//! Un tirage dont une issue porte plusieurs lignes, à sa propre chance : le
//! tour d'un monstre invoqué, l'Arakne à 80 % et l'Arakne Majeure à 20 %.
//! Banc synthétique.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

fn moteur() -> Engine {
    let rs = Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: { fr: "Test", en: "Test" }
resources: []
spells:
  - id: coup
    name: { fr: "coup", en: "coup" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      # A 80 % : deux frappes de 10, l'une Eau, l'autre Feu.
      - { element: water, tirage: monstre, issue: 0, chance: 80, normal: [10, 10], critical: [10, 10] }
      - { element: fire, tirage: monstre, issue: 0, chance: 80, normal: [10, 10], critical: [10, 10] }
      # A 20 % : une frappe Eau de 50.
      - { element: water, tirage: monstre, issue: 1, chance: 20, normal: [50, 50], critical: [50, 50] }
      # Hors tirage, toujours : 5 de Terre.
      - { element: earth, normal: [5, 5], critical: [5, 5] }
    effects:
      - effect: damage
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats { characteristic: 0, flat_damage: 0 }; 5],
            ..Default::default()
        },
        base_ap: 1,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["coup".into()],
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
    Engine::new(&rs, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

/// 0,8 × 20 + 0,2 × 50 + 5 = 31.
#[test]
fn chaque_issue_compte_pour_sa_chance() {
    let s = moteur().replay(&[vec!["coup".to_string()]]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(s.turns[0].casts[0].damage.as_f64(), 31.0);
}

/// L'infobulle : chaque issue additionne ses lignes (20 et 50), le tirage va
/// de la plus basse à la plus haute, et la ligne hors tirage s'ajoute : 25 à
/// 55. Par élément : l'Eau de 10 à 50, le Feu à 10 sur la seule issue qui en
/// porte, la Terre à 5.
#[test]
fn la_fourchette_va_d_une_issue_a_l_autre() {
    let table = moteur().damage_table("coup");
    assert_eq!(table[0].normal, (25, 55));
    let element = |e: dofus_damage::Element| table[0].by_element.iter().find(|r| r.element == e).map(|r| r.normal);
    assert_eq!(element(dofus_damage::Element::Water), Some((10, 50)));
    assert_eq!(element(dofus_damage::Element::Fire), Some((10, 10)));
    assert_eq!(element(dofus_damage::Element::Earth), Some((5, 5)));
}

/// Des chances qui ne font pas cent pour cent se refusent au chargement.
#[test]
fn des_chances_qui_ne_font_pas_cent_se_refusent() {
    let yaml = r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: { fr: "Test", en: "Test" }
resources: []
spells:
  - id: coup
    name: { fr: "coup", en: "coup" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      - { element: water, tirage: monstre, issue: 0, chance: 80, normal: [10, 10], critical: [10, 10] }
      - { element: water, tirage: monstre, issue: 1, chance: 30, normal: [50, 50], critical: [50, 50] }
    effects:
      - effect: damage
"#;
    let e = Ruleset::from_yaml(yaml).map(|_| ()).unwrap_err().to_string();
    assert!(e.contains("110 %"), "{e}");
}
