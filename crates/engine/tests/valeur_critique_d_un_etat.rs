//! La valeur critique d'un modificateur d'état, et l'ordre dans lequel on la
//! choisit.
//!
//! Les Tirs Puissants du Crâ donnent 250 Puissance, ou 300 sur un jet critique :
//! le solveur applique une seule valeur par lancer, la critique au seuil de
//! `CRITICAL_EFFECT_THRESHOLD`. Le même état porte aussi quinze points de taux
//! de coup critique : choisir entre 250 et 300 avant de les avoir ajoutés ne
//! donne pas la même réponse qu'après. Le calcul se fait donc en deux passes :
//! le taux d'abord, tout le reste ensuite.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

/// Un état qui relève le taux de 15 points ET donne un montant différent en
/// critique. `critique` écrit ou non la seconde valeur.
fn banc(critique: bool) -> Ruleset {
    let ligne = if critique {
        "        critical_amount: 300\n"
    } else {
        ""
    };
    let yaml = format!(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: {{ fr: "Test", en: "Test" }}
resources:
  - id: nerf
    scope: caster
    max: 1
    default: 0
    monotone: increasing
    duration: {{ turns: 3, refresh: on_apply, on_expire: reset_to_default }}
    modifies_damage:
      - kind: critical_rate
        percent: 15
      - kind: characteristic
        amount: 250
{ligne}spells:
  - id: poseur
    name: {{ fr: "Poseur", en: "Setter" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines: []
    effects:
      - effect: gain
        resource: nerf
  - id: coup
    name: {{ fr: "Coup", en: "Hit" }}
    ap_cost: {{ base: 3 }}
    casts_per_turn: 1
    crit: {{ base_rate: 80 }}
    lines:
      - element: water
        critical: [20, 20]
        normal: [10, 10]
    effects:
      - effect: damage
"#
    );
    Ruleset::from_yaml(&yaml).unwrap_or_else(|e| panic!("{e}"))
}

fn degats(critique: bool) -> f64 {
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
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["poseur".into(), "coup".into()],
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
    Engine::new(&banc(critique), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == "coup")
        .map(|c| c.damage.as_f64())
        .fold(0.0, f64::max)
}

/// Le montant critique s'applique, au taux relevé par l'état.
#[test]
fn le_montant_critique_s_applique_au_taux_releve() {
    let sans = degats(false);
    let avec = degats(true);
    assert!(sans > 0.0, "le sort doit frapper, mesuré {sans}");
    assert!(
        avec > sans,
        "300 de Puissance doivent battre 250 : {sans} contre {avec}"
    );
    // 250 contre 300 de Puissance, c'est un multiplicateur de 4,5 contre 5,0
    // sur le jet : l'écart attendu est de l'ordre de dix pour cent, et exiger
    // son AMPLEUR évite qu'une assertion directionnelle passe pour un point.
    let ecart = (avec - sans) / sans;
    assert!(
        ecart > 0.05,
        "l'écart doit valoir plusieurs points, mesuré {:.1} %",
        ecart * 100.0
    );
}
