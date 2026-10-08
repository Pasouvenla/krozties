//! Un état qui relève le taux de coup critique du lanceur.
//!
//! La Nervosité du Sacrieur donne « +7 % de Critique pendant trois tours »,
//! effet 115 de valeur 7 : le seul modificateur qui ne touche aucun étage du
//! calcul mais le tirage. Un banc synthétique, où seul le pourcentage change
//! entre les deux mesures : sur le vrai fichier, changer le deck change la
//! rotation pour des raisons étrangères au critique.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

/// Deux sorts identiques à une chose près, l'un pouvant critiquer et l'autre
/// non, et un état qui donne `percent` points de taux.
fn banc(percent: i32) -> Ruleset {
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
        percent: {percent}
spells:
  - id: poseur
    name: {{ fr: "Poseur", en: "Setter" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines: []
    effects:
      - effect: gain
        resource: nerf
  - id: avec_crit
    name: {{ fr: "Avec critique", en: "Can crit" }}
    ap_cost: {{ base: 3 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0 }}
    lines:
      - element: water
        critical: [40, 40]
        normal: [10, 10]
    effects:
      - effect: damage
  - id: sans_crit
    name: {{ fr: "Sans critique", en: "Cannot crit" }}
    ap_cost: {{ base: 3 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines:
      - element: water
        critical: [40, 40]
        normal: [10, 10]
    effects:
      - effect: damage
"#
    );
    Ruleset::from_yaml(&yaml).unwrap_or_else(|e| panic!("{e}"))
}

/// Les dégâts d'un sort, lancé après le poseur, dans le même tour.
fn degats(percent: i32, spell: &str) -> f64 {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            // Un coup critique doit valoir nettement plus qu'un coup normal,
            // sinon sept points de taux ne pèsent rien et le test mesure du
            // bruit d'arrondi.
            flat_crit_damage: 100,
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
        deck: vec!["poseur".into(), spell.into()],
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
    Engine::new(&banc(percent), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == spell)
        .map(|c| c.damage.as_f64())
        .fold(0.0, f64::max)
}

/// Sept points de taux valent sept points de taux, et rien d'autre ne bouge.
#[test]
fn le_taux_monte_avec_l_etat() {
    let sans = degats(0, "avec_crit");
    let avec = degats(7, "avec_crit");
    assert!(sans > 0.0, "le sort doit frapper, mesuré {sans}");
    assert!(
        avec > sans,
        "sept points de taux doivent faire monter les dégâts : {sans} contre {avec}"
    );
    // Un coup critique vaut ici 40 de jet plus 100 de Dommages Critiques contre
    // 10 de jet : l'écart est large, et sept pour cent de cet écart se voient.
    // Exiger l'AMPLEUR et pas seulement le sens : une assertion directionnelle
    // passerait avec un point de taux au lieu de sept.
    let ecart = (avec - sans) / sans;
    assert!(
        ecart > 0.03,
        "l'écart doit valoir plusieurs points, mesuré {:.1} %",
        ecart * 100.0
    );
}

/// Un sort qui ne PEUT pas critiquer n'y gagne rien.
///
/// Le garde-fou. Un taux de zéro et une interdiction de critiquer valent tous
/// deux « jamais » dans le moteur, et les confondre donnerait des coups
/// critiques aux poisons du Sadida et à la Runification du Huppermage, que la
/// donnée du jeu déclare sans critique.
#[test]
fn un_sort_sans_critique_n_y_gagne_rien() {
    let sans = degats(0, "sans_crit");
    let avec = degats(7, "sans_crit");
    assert!(sans > 0.0, "le sort doit frapper, mesuré {sans}");
    assert!(
        (avec - sans).abs() < 0.001,
        "un sort sans critique ne gagne aucun point de taux : {sans} contre {avec}"
    );
}

/// Le buff profite à toute la suite de la rotation, pas au sort qui le pose :
/// des sorts comme les Tirs Puissants du Crâ ne frappent pas eux-mêmes et ne
/// valent que par les sorts lancés après eux. Le poseur du banc ne porte aucune
/// ligne de dégâts et déclare `can_crit: false` : tout l'écart mesuré vient des
/// autres sorts.
#[test]
fn le_buff_profite_aux_sorts_lances_apres_lui() {
    let rs = banc(7);
    let poseur = rs.spells.iter().find(|s| s.id == "poseur").unwrap();
    assert!(
        poseur.lines.is_empty() && !poseur.crit.can_crit,
        "le poseur doit être un pur utilitaire, sinon le test mesure autre chose"
    );
    let sans = degats(0, "avec_crit");
    let avec = degats(7, "avec_crit");
    assert!(
        avec > sans,
        "un sort qui ne frappe pas doit quand même faire monter la suite : {sans} contre {avec}"
    );
}

/// Les deux sorts sont identiques hors le droit de critiquer.
///
/// Sans ce contrôle, le test ci-dessus passerait aussi si `sans_crit` était
/// devenu un sort différent au fil des éditions.
#[test]
fn les_deux_sorts_du_banc_ne_different_que_par_le_droit_de_critiquer() {
    let rs = banc(0);
    let a = rs.spells.iter().find(|s| s.id == "avec_crit").unwrap();
    let b = rs.spells.iter().find(|s| s.id == "sans_crit").unwrap();
    assert_eq!(a.ap_cost.base, b.ap_cost.base);
    assert_eq!(a.lines.len(), b.lines.len());
    assert!(a.crit.can_crit && !b.crit.can_crit);
}
