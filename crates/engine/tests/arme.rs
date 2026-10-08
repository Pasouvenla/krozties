//! L'arme du build, lancée comme un sort.
//!
//! Le banc reprend la Hachebarde de Guerre sur un build Ouginak Feu : 46 à 55
//! Feu, +20 en critique, 1348 d'Intelligence, 410 de Puissance, 123 Dommages
//! Feu, 90 Dommages critiques, et la Maîtrise d'arme normale, 300 Puissance.
//! DofusBook y affiche 2009 à 2358, 2949 à 3299 en critique, avec 80,18 % de
//! dommages finaux composés, que la formule redonne au point près. Le banc
//! prend 80 % tout rond, que le moteur sait écrire : 2007 à 2356, 2946 à 3295.

use dofus_damage::{DamageProfile, Element, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

fn banc() -> Ruleset {
    Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: { fr: "Test", en: "Test" }
resources: []
spells:
  - id: arme
    name: { fr: "Hachebarde de Guerre", en: "Hachebarde de Guerre" }
    ap_cost: { base: 5 }
    casts_per_turn: 1
    crit: { base_rate: 15 }
    range: [1, 2]
    arme: true
    lines:
      - element: fire
        normal: [46, 55]
        critical: [66, 75]
        puissance_propre: 300
    effects:
      - effect: damage
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn moteur(pourcent_sorts: i32, pourcent_armes: i32) -> Engine {
    let mut elements = [ElementStats { characteristic: 0, flat_damage: 0 }; 5];
    elements[Element::Fire.index()] = ElementStats { characteristic: 1348, flat_damage: 123 };
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 410,
            flat_crit_damage: 90,
            elements,
            percent_spell: pourcent_sorts,
            percent_weapon: pourcent_armes,
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 6,
        crit_bonus_percent: 0,
        modifiers: vec![BuildModifier { id: "finaux".into(), percent: 180, when: When::Always }],
        deck: vec!["arme".into()],
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
        reach: Reach::Melee,
    };
    Engine::new(&banc(), build, sc).unwrap_or_else(|e| panic!("{e}"))
}

/// La formule : la Maîtrise dans le multiplicateur de la caractéristique, le
/// bonus critique dans le jet, les Dommages critiques avec les Dommages Feu,
/// puis les % une seule fois.
#[test]
fn la_hachebarde_suit_l_onglet_arme_de_dofusbook() {
    let ligne = &moteur(0, 0).damage_table("arme")[0];
    assert_eq!(ligne.normal, (2007, 2356));
    assert_eq!(ligne.critical, Some((2946, 3295)));
}

/// Les % de dommages aux sorts ne touchent pas l'arme ; ses % de dommages
/// d'armes, si : 10 % font 2007 × 1,1 × 1,8 / 1,8, soit 2207.
#[test]
fn les_pourcentages_d_armes_remplacent_ceux_des_sorts() {
    assert_eq!(moteur(50, 0).damage_table("arme")[0].normal, (2007, 2356));
    assert_eq!(moteur(0, 10).damage_table("arme")[0].normal.0, 2207);
}
