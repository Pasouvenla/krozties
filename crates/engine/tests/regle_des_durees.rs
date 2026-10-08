//! La règle des durées : un sort qui dure 3 tours, lancé au tour N, dure
//! jusqu'au tour N+2 inclus. Six relevés de cinq classes la donnent : Forme
//! Bestiale et Saoul (2 tours : N et N+1), la rune du Huppermage (posée au tour
//! N, partie à la fin du N+1), la Parade (1 tour : N seulement), le Pacte de
//! Sang et le Drain Élémentaire (3 tours : N à N+2). Un effet de durée `d`
//! s'écrit donc `turns: d - 1`.
//!
//! Un poison posé pour `d` tours frappe `d` fois : le jeu décompte les durées
//! au début du tour du lanceur, après le tour de la cible, si bien que le
//! dernier tick a lieu avant le départ. L'Infortune de l'Ecaflip, qui dure un
//! tour, frappe une fois.
//!
//! Banc d'essai synthétique : des lignes à dix, aucune caractéristique, pour
//! lire chaque chiffre sans formule.

use dofus_damage::{DamageProfile, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

fn regles(ressources: &str, sorts: &str) -> Ruleset {
    Ruleset::from_yaml(&format!(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: {{ fr: "Test", en: "Test" }}
resources:
{ressources}
spells:
  - id: frapper
    name: {{ fr: "Frapper", en: "Hit" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines:
      - {{ element: fire, normal: [10, 10], critical: [10, 10] }}
    effects:
      - effect: damage
{sorts}
"#
    ))
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Par tour : les dégâts de début de tour et ceux de chaque lancer.
fn tours(rs: &Ruleset, deck: &[&str], ap: u8, horizon: u8) -> Vec<(f64, Vec<(String, f64)>)> {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile::default(),
        base_ap: ap,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .map(|t| {
            (
                t.opening_damage.as_f64(),
                t.casts.iter().map(|c| (c.id.clone(), c.damage.as_f64())).collect(),
            )
        })
        .collect()
}

fn tour_du(plan: &[(f64, Vec<(String, f64)>)], sort: &str) -> usize {
    plan.iter()
        .position(|(_, casts)| casts.iter().any(|(id, _)| id == sort))
        .unwrap_or_else(|| panic!("{sort} jamais lancé : {plan:?}"))
}

fn frappe(plan: &[(f64, Vec<(String, f64)>)], tour: usize) -> f64 {
    plan[tour]
        .1
        .iter()
        .find(|(id, _)| id == "frapper")
        .map(|(_, d)| *d)
        .unwrap_or_else(|| panic!("pas de frappe au tour {} : {plan:?}", tour + 1))
}

/// Un buff de trois tours (`turns: 2`) posé au tour N double la frappe aux
/// tours N, N+1 et N+2, et plus au tour N+3.
#[test]
fn un_buff_de_trois_tours_agit_du_tour_n_au_tour_n_plus_2() {
    let rs = regles(
        "  - { id: buff, scope: caster, max: 1, default: 0, monotone: increasing,
      duration: { turns: 2, refresh: on_apply, on_expire: reset_to_default },
      modifies_damage: [{ kind: characteristic, amount: 100 }] }",
        "  - id: poser
    name: { fr: \"Poser\", en: \"Set\" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    cooldown_turns: 9
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - { effect: gain, resource: buff }",
    );
    let plan = tours(&rs, &["frapper", "poser"], 2, 5);
    let n = tour_du(&plan, "poser");
    assert!(n + 3 < plan.len(), "le buff doit tomber dans l'horizon : {plan:?}");
    // La frappe du tour de pose : après la pose, dans le même tour.
    for t in n..=n + 2 {
        assert_eq!(frappe(&plan, t), 20.0, "tour {} : {plan:?}", t + 1);
    }
    assert_eq!(frappe(&plan, n + 3), 10.0, "le buff dure trois tours, pas quatre : {plan:?}");
}

/// Un poison de deux tours (`turns: 1`) frappe deux fois : aux tours de la
/// cible qui suivent le tour N et le tour N+1.
#[test]
fn un_poison_de_deux_tours_frappe_deux_fois() {
    let rs = regles(
        "  - id: poison
    scope: target
    max: 1
    default: 0
    monotone: increasing
    duration: { turns: 1, refresh: on_apply, on_expire: reset_to_default }
    while_present:
      - trigger: turn_start
        lines:
          - { element: fire, normal: [10, 10], critical: [10, 10] }",
        "  - id: empoisonner
    name: { fr: \"Empoisonner\", en: \"Poison\" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    cooldown_turns: 9
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - { effect: gain, resource: poison }",
    );
    let plan = tours(&rs, &["empoisonner"], 1, 5);
    let ticks: f64 = plan.iter().map(|(ouverture, _)| ouverture).sum();
    assert_eq!(ticks, 20.0, "deux ticks de dix : {plan:?}");
    // Et d'un seul tour : une fois, pas zéro, comme l'Infortune.
    let rs = regles(
        "  - id: poison
    scope: target
    max: 1
    default: 0
    monotone: increasing
    duration: { turns: 0, refresh: on_apply, on_expire: reset_to_default }
    while_present:
      - trigger: turn_start
        lines:
          - { element: fire, normal: [10, 10], critical: [10, 10] }",
        "  - id: empoisonner
    name: { fr: \"Empoisonner\", en: \"Poison\" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    cooldown_turns: 9
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - { effect: gain, resource: poison }",
    );
    let plan = tours(&rs, &["empoisonner"], 1, 5);
    let ticks: f64 = plan.iter().map(|(ouverture, _)| ouverture).sum();
    assert_eq!(ticks, 10.0, "un tick de dix : {plan:?}");
}

/// Une Puissance « au tour suivant » (gain différé, `turns: 0`) ne vaut que
/// ce tour-là : c'est l'Âge d'Or de l'Enutrof.
#[test]
fn un_gain_differe_ne_vit_que_le_tour_ou_il_arrive() {
    let rs = regles(
        "  - { id: plus_tard, scope: caster, max: 1, default: 0, monotone: increasing,
      duration: { turns: 0, refresh: on_apply, on_expire: reset_to_default },
      modifies_damage: [{ kind: characteristic, amount: 100 }] }",
        "  - id: annoncer
    name: { fr: \"Annoncer\", en: \"Announce\" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    cooldown_turns: 9
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - { effect: schedule_gain, resource: plus_tard, delay: 1 }",
    );
    let plan = tours(&rs, &["frapper", "annoncer"], 2, 5);
    let n = tour_du(&plan, "annoncer");
    assert_eq!(frappe(&plan, n), 10.0, "rien au tour du lancer : {plan:?}");
    assert_eq!(frappe(&plan, n + 1), 20.0, "le tour suivant : {plan:?}");
    // Un seul tour doublé sur les cinq : une Puissance qui resterait ferait
    // lancer l'annonce au tour 1 et doublerait les quatre suivants.
    let total: f64 = (0..plan.len()).map(|t| frappe(&plan, t)).sum();
    assert_eq!(total, 60.0, "{plan:?}");
}

/// Un état qui retombe de lui-même en pose un autre pour le tour qui s'ouvre :
/// Saoul, en partant, pose « sorti de Saoul », que lit la Gueule de Bois.
#[test]
fn un_etat_qui_expire_pose_sa_suite_pour_un_tour() {
    let rs = regles(
        "  - id: ivre
    scope: caster
    max: 1
    default: 0
    monotone: none
    duration: { turns: 1, refresh: on_apply, on_expire: reset_to_default, then_gain: degrise }
  - { id: degrise, scope: caster, max: 1, default: 0, monotone: none,
      duration: { turns: 0, refresh: on_apply, on_expire: reset_to_default },
      modifies_damage: [{ kind: characteristic, amount: 100 }] }",
        "  - id: boire
    name: { fr: \"Boire\", en: \"Drink\" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    cooldown_turns: 9
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - { effect: gain, resource: ivre }",
    );
    let plan = tours(&rs, &["frapper", "boire"], 2, 6);
    let n = tour_du(&plan, "boire");
    assert!(n + 3 < plan.len(), "{plan:?}");
    assert_eq!(frappe(&plan, n + 1), 10.0, "encore ivre au tour N+1 : {plan:?}");
    assert_eq!(frappe(&plan, n + 2), 20.0, "dégrisé au tour N+2 : {plan:?}");
    assert_eq!(frappe(&plan, n + 3), 10.0, "pour ce tour-là seulement : {plan:?}");
}
