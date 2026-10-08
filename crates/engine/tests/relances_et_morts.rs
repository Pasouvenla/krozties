//! Deux mécanismes de l'Osamodas, sur un banc synthétique.
//!
//! Cortège Sauvage : « toutes les invocations Osamodas meurent au début du tour
//! du lanceur tant qu'il est sous les effets du sort ». Un compteur retombe au
//! début du tour tant qu'un état tient, APRÈS les effets de début de tour : les
//! invocations ont joué leur tour avant de mourir.
//!
//! Le Pacte Bestial : « le temps de relance des sorts d'invocation Osamodas est
//! réinitialisé [...] à la perte de l'état ou si le sort est relancé ». Une
//! relance remise par un lancer rouvre le sort dans le tour même ; la fin d'un
//! état encore présent la remet pour le tour qui s'ouvre, et un état retiré
//! avant son terme ne remet rien.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

/// Une invocation frappe 20 au début de chaque tour où elle est en jeu
/// (`armee`), un sort direct en fait 20 aussi, un autre 10.
fn banc() -> Ruleset {
    Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - id: armee
    scope: caster
    max: 3
    default: 0
    reset_at_turn_start_while: hecatombe
    while_present:
      - trigger: turn_start
        lines:
          - element: water
            critical: [10, 10]
            normal: [10, 10]
            repeats_per: [armee]
  - id: hecatombe
    scope: caster
    max: 1
    default: 0
    duration: { turns: 1 }
  - id: etat
    scope: caster
    max: 1
    default: 0
    duration: { turns: 1, then_reset_cooldowns: [invoque] }
  - id: cle
    scope: caster
    max: 1
    default: 0
spells:
  - id: durable
    name: { fr: "Durable", en: "Durable" }
    ap_cost: { base: 2 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - effect: gain
        resource: armee
  - id: kamikaze
    name: { fr: "Kamikaze", en: "Kamikaze" }
    ap_cost: { base: 2 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - effect: gain
        resource: armee
      - effect: gain
        resource: hecatombe
  - id: invoque
    name: { fr: "Invoque", en: "Summon" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    cooldown_turns: 3
    crit: { base_rate: 0, can_crit: false }
    lines:
      - element: water
        critical: [10, 10]
        normal: [10, 10]
    effects:
      - effect: damage
  - id: invoque_tardive
    name: { fr: "Invoque tardive", en: "Late summon" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    initial_cooldown: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      - element: water
        critical: [10, 10]
        normal: [10, 10]
    effects:
      - effect: damage
  - id: remise
    name: { fr: "Remise", en: "Reset" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    cooldown_turns: 5
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - effect: reset_cooldowns
        spells: [invoque, invoque_tardive]
  - id: appoint
    name: { fr: "Appoint", en: "Filler" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      - element: water
        critical: [7, 8]
        normal: [7, 8]
    effects:
      - effect: damage
  - id: gros
    name: { fr: "Gros", en: "Big" }
    ap_cost: { base: 2 }
    casts_per_turn: 1
    initial_cooldown: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      - element: water
        critical: [20, 20]
        normal: [20, 20]
    effects:
      - effect: damage
  - id: pose_cle
    name: { fr: "Pose clé", en: "Key" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - effect: gain
        resource: cle
  - id: remise_sous_cle
    name: { fr: "Remise sous clé", en: "Keyed reset" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - effect: reset_cooldowns
        spells: [invoque_tardive]
        requires: { kind: caster_has, resource: cle }
  - id: bascule
    name: { fr: "Bascule", en: "Switch" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      - element: water
        critical: [5, 5]
        normal: [5, 5]
    effects:
      - effect: damage
      - effect: toggle
        resource: etat
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Le total d'un deck sur `horizon` tours, à `pa` PA par tour.
fn total(deck: &[&str], horizon: u8, pa: u8) -> f64 {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats { characteristic: 100, flat_damage: 0 }; 5],
            ..Default::default()
        },
        base_ap: pa,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
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

/// Une invocation par tour sur trois tours. Durable, elles s'accumulent : 20
/// au deuxième tour, 40 au troisième. Sous l'état, chacune frappe au début du
/// tour qui suit puis meurt : 20 et 20. Mourir avant de frapper rendrait 0.
#[test]
fn les_invocations_frappent_puis_meurent() {
    assert_eq!(total(&["durable"], 3, 2), 60.0);
    assert_eq!(total(&["kamikaze"], 3, 2), 40.0);
}

/// Un sort au délai initial d'un tour, rouvert dans le premier tour par la
/// remise : 20 au lieu de rien.
#[test]
fn une_remise_rouvre_le_sort_dans_le_tour() {
    assert_eq!(total(&["invoque_tardive"], 1, 2), 0.0);
    assert_eq!(total(&["invoque_tardive", "remise"], 1, 2), 20.0);
}

/// La remise ne part qu'une fois en deux tours : lancée APRÈS le sort au
/// premier tour, elle lui laisse son deuxième lancer au tour suivant, 40 au
/// lieu de 20.
#[test]
fn la_remise_vaut_aussi_pour_le_tour_suivant() {
    assert_eq!(total(&["invoque"], 2, 2), 20.0);
    assert_eq!(total(&["invoque", "remise"], 2, 2), 40.0);
}

/// Remis sans être relancé dans le tour, le sort repart au tour suivant même
/// quand sa relance courait encore. Trois tours : le sort (20) au premier avec
/// l'appoint (15) ; au deuxième, la remise et l'appoint ; au troisième, le sort
/// et l'appoint, 85. Sans la remise reportée, le sort attendrait le quatrième
/// tour et le mieux serait de le relancer au deuxième, 70.
#[test]
fn une_relance_remise_attend_le_tour_suivant() {
    assert_eq!(total(&["invoque", "appoint"], 3, 2), 65.0);
    assert_eq!(total(&["invoque", "appoint", "remise"], 3, 2), 85.0);
}

/// La remise efface les lancers qui la PRÉCÈDENT dans le tour. Deux tours à 3
/// PA, et le deuxième vaut plus cher : le gros sort (40) n'ouvre qu'au deuxième
/// tour. Le sort (20), la remise et l'appoint (15) au premier ; le gros sort et
/// le sort au second : 95. Un sort lancé avant la remise qui garderait sa
/// relance laisserait l'appoint au second tour, 90.
#[test]
fn une_remise_efface_les_lancers_qui_la_precedent() {
    assert_eq!(total(&["invoque", "appoint", "gros", "remise"], 2, 3), 95.0);
}

/// Une remise sous condition : la clé posée, elle rouvre le sort ; sans clé,
/// rien.
#[test]
fn une_remise_conditionnelle() {
    assert_eq!(total(&["invoque_tardive", "remise_sous_cle"], 1, 3), 0.0);
    assert_eq!(total(&["invoque_tardive", "remise_sous_cle", "pose_cle"], 1, 3), 20.0);
}

/// La fin de l'état rend le sort. Sur trois tours, `bascule` (10) pose l'état
/// au premier, qui s'achève au deuxième : le sort (20) repart au troisième,
/// 20 + 10 + 0 + 20 + 10 = 60 si la bascule laisse l'état tranquille au
/// deuxième tour. La relancer le RETIRE : il ne rend alors plus rien à son
/// terme, et la rotation qui le croirait trouverait 70.
#[test]
fn la_fin_d_un_etat_rend_ses_sorts_s_il_tient_encore() {
    assert_eq!(total(&["invoque", "bascule"], 3, 2), 60.0);
}
