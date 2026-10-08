//! Un état qui donne des PA à chaque tour, et non un remboursement au lancer.
//!
//! Le Pacte Bestial de l'Osamodas « applique l'état Bestial sur le lanceur :
//! augmente ses PA », effet 111, deux points, trois tours, cible « C » : il
//! ouvre les tours suivants avec deux PA de plus, là où `ap_bonus` rend des PA
//! dans le tour du lancer.
//!
//! Le plafond d'élagage en dépend : `prune_dominated` ne laisse tomber un sort
//! que si d'autres peuvent absorber tout le budget du tour, et un budget
//! sous-estimé ferait tomber un sort qu'il fallait garder. Le dernier test
//! compare donc la recherche élaguée à la recherche exhaustive.
//!
//! Le deck porte vingt PA de capacité pour un budget de douze : ce sont les PA
//! qui mordent, pas les plafonds de lancers.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn osamodas() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/osamodas.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-2.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn solve(deck: &[&str], elaguer: bool) -> Solution {
    let build = Build {
        name: "Osamodas".into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 630,
                flat_damage: 56,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon: 4,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: elaguer,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&osamodas(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

/// Combien de lancers chaque tour tient, le Pacte lui-même mis à part.
fn lancers_par_tour(deck: &[&str]) -> Vec<usize> {
    solve(deck, false)
        .turns
        .iter()
        .map(|t| t.casts.iter().filter(|c| c.id != "pacte_bestial").count())
        .collect()
}

/// Un deck dont la capacité dépasse le budget : vingt PA pour douze.
const SANS: &[&str] = &[
    "cri_du_corbac",
    "pics_du_prespic",
    "saute_granouille",
    "souffle_draconique",
];
const AVEC: &[&str] = &[
    "cri_du_corbac",
    "pics_du_prespic",
    "saute_granouille",
    "souffle_draconique",
    "pacte_bestial",
];

/// Le Pacte ouvre les tours suivants avec deux PA de plus. Il a un délai
/// initial d'un tour : il ne peut pas partir au premier tour du combat.
#[test]
fn les_pa_du_pacte_ouvrent_les_tours_d_apres() {
    let sans = lancers_par_tour(SANS);
    let avec = lancers_par_tour(AVEC);
    assert!(sans.len() == 4 && avec.len() == 4, "{sans:?} / {avec:?}");
    assert!(
        sans.iter().all(|n| *n == sans[0]),
        "sans le Pacte, tous les tours se valent : {sans:?}"
    );
    // Tour 1 : le Pacte est encore indisponible, rien ne peut différer.
    assert_eq!(
        avec[0], sans[0],
        "le premier tour ne connaît pas le Pacte, {sans:?} contre {avec:?}"
    );
    // Tour 2 : il part, et ses deux PA sont pris sur ce tour-là sans rien lui
    // rendre. Le tour du lancer PERD un sort.
    assert_eq!(
        avec[1],
        sans[1] - 1,
        "le tour du lancer paie le Pacte, {sans:?} contre {avec:?}"
    );
    // Tours 3 et 4 : les deux PA arrivent au début du tour.
    for t in 2..4 {
        assert_eq!(
            avec[t],
            sans[t] + 1,
            "le tour {} doit tenir un lancer de plus, {sans:?} contre {avec:?}",
            t + 1
        );
    }
}

/// Et le total suit : plus de lancers, plus de dégâts.
#[test]
fn le_pacte_paie_malgre_les_deux_pa_qu_il_coute() {
    let sans = solve(SANS, false).total.as_f64();
    let avec = solve(AVEC, false).total.as_f64();
    assert!(
        avec > sans,
        "le Pacte doit rapporter plus qu'il ne coûte, {sans:.0} contre {avec:.0}"
    );
    // Un lancer perdu au tour du lancer, un lancer gagné aux tours 3 et 4 :
    // net d'un lancer sur les vingt du scénario, soit environ quatre points.
    // L'écart est encadré des deux côtés : `> 0` passerait avec un Pacte qui ne
    // rendrait qu'un PA sur deux, et la borne haute attrape un état trop
    // généreux.
    let ecart = (avec - sans) / sans;
    assert!(
        (0.02..0.06).contains(&ecart),
        "un lancer net sur vingt vaut environ quatre points, mesuré {:.1} %",
        ecart * 100.0
    );
}

/// Élaguer ne change pas la réponse sur l'Osamodas. Ce test ne contrôle pas le
/// plafond : aucun sort n'est élagué sur ce deck, et c'est un filet de
/// non-régression. La preuve est sur le banc synthétique juste après.
#[test]
fn l_elagage_ne_change_pas_la_reponse_sur_l_osamodas() {
    let elague = solve(AVEC, true).total;
    let exhaustif = solve(AVEC, false).total;
    assert_eq!(
        elague,
        exhaustif,
        "l'élagage a changé la réponse : {} contre {}",
        elague.as_f64(),
        exhaustif.as_f64()
    );
}

// -- le plafond d'élagage, sur un banc fait pour le mettre en défaut ----------

/// Un banc où l'élagage MORD, et où deux PA d'état décident s'il doit mordre.
///
/// `fort` et `faible` coûtent le même prix et `fort` frappe plus fort : sur un
/// budget que `fort` peut remplir à lui seul, `faible` ne sert à rien et tombe.
/// Six lancers de `fort` à 2 PA absorbent douze PA, exactement le budget nu.
/// Le `poseur` en ajoute deux, et les deux derniers ne peuvent aller qu'à
/// `faible` : il ne faut donc PLUS l'élaguer.
///
/// C'est tout l'enjeu du plafond. Calculé sans les PA des états, il vaut douze,
/// `faible` tombe, et le solveur rend un total plus bas SANS RIEN DIRE.
fn banc() -> Ruleset {
    Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - id: pile
    scope: caster
    max: 1
    default: 0
    monotone: increasing
    grants_ap: 2
spells:
  - id: poseur
    name: { fr: "Poseur", en: "Setter" }
    ap_cost: { base: 2 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - effect: gain
        resource: pile
  - id: fort
    name: { fr: "Fort", en: "Strong" }
    ap_cost: { base: 2 }
    casts_per_turn: 6
    crit: { base_rate: 0, can_crit: false }
    lines:
      - element: water
        critical: [20, 20]
        normal: [20, 20]
    effects:
      - effect: damage
  - id: faible
    name: { fr: "Faible", en: "Weak" }
    ap_cost: { base: 2 }
    casts_per_turn: 6
    crit: { base_rate: 0, can_crit: false }
    lines:
      - element: water
        critical: [10, 10]
        normal: [10, 10]
    effects:
      - effect: damage
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn banc_moteur(elaguer: bool) -> Engine {
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
        deck: vec!["poseur".into(), "fort".into(), "faible".into()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon: 4,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: elaguer,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&banc(), build, sc).unwrap_or_else(|e| panic!("{e}"))
}

/// Le sort faible SURVIT à l'élagage, parce que les deux PA de l'état
/// débordent ce que le sort fort peut absorber.
#[test]
fn les_pa_d_etat_sauvent_un_sort_de_l_elagage() {
    let elague = banc_moteur(true);
    assert!(
        elague.pruned().is_empty(),
        "aucun sort ne doit tomber : douze PA absorbés contre quatorze au budget, \
         élagués {:?}",
        elague.pruned()
    );
    // Et le fond : la réponse élaguée vaut la réponse exhaustive. Sans le
    // plafond corrigé, `faible` tombe et ce total baisse en silence.
    assert_eq!(
        elague.solve().total,
        banc_moteur(false).solve().total,
        "l'élagage a changé la réponse du banc"
    );
}
