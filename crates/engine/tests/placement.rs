//! Le placement sur grille, et ce qu'il remplace : un nombre d'ennemis et un
//! « étalement » déclarés, d'où le solveur déduit que la cible `i` est à
//! `i * etalement` cases de l'impact, exact seulement si les ennemis sont
//! alignés à intervalle régulier.
//!
//! Ces tests fixent les trois choses que le placement apporte, et la quatrième
//! qu'il ne doit pas faire :
//!
//! 1. sur le cas où la déclaration est vraie, les deux régimes donnent le même
//!    total, au chiffre près : le placement raffine le calcul déclaré, il ne le
//!    concurrence pas ;
//! 2. sur un placement réel, il mesure ce que la déclaration supposait ;
//! 3. la zone borne qui est touché : un ennemi hors du dessin ne compte plus ;
//! 4. une forme que la grille ne sait pas dessiner ne devient pas une zone
//!    vide : elle retombe sur le régime déclaré et se fait nommer.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn regles(classe: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn build(sort: &str) -> Build {
    Build {
        name: sort.into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 630,
                flat_damage: 56,
            }; 5],
            ..Default::default()
        },
        base_ap: 4,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec![sort.to_string()],
    }
}

fn scenario(cibles: u8, etalement: u8, placement: Option<Placement>) -> Scenario {
    Scenario {
        poussees_bloquees: false,
        horizon: 1,
        etalement,
        placement,
        pm_depenses: 0,
        etats_declares: vec![],
        targets: cibles,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    }
}

fn total(rs: &Ruleset, sort: &str, sc: Scenario) -> f64 {
    Engine::new(rs, build(sort), sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .total
        .as_f64()
}

/// Le lanceur à six cases de l'impact, sur l'axe : la visée est nette et toutes
/// les formes s'orientent.
fn pose(ennemis: Vec<Case>) -> Placement {
    Placement {
        lanceur: Case::new(-6, 0),
        visee: Case::new(0, 0),
        ennemis,
    }
}

/// L'Épée Céleste du Iop : cercle de rayon 2, treize cases, dix pour cent par
/// cran et quatre crans au plus. Toute la donnée vient de l'instantané.
const SORT: &str = "epee_celeste";

#[test]
fn sur_une_ligne_reguliere_le_placement_redonne_l_etalement_declare() {
    let rs = regles("iop", 8);
    // Trois ennemis alignés derrière l'impact, une case d'écart : très
    // exactement ce que « étalement 1, trois cibles » suppose. Le cercle de
    // rayon 2 les couvre tous les trois.
    let places = vec![Case::new(0, 0), Case::new(0, 1), Case::new(0, 2)];
    let mesure = total(&rs, SORT, scenario(3, 1, Some(pose(places))));
    let declare = total(&rs, SORT, scenario(3, 1, None));
    assert!(
        (mesure - declare).abs() < 1.0,
        "les deux régimes doivent tomber sur le même total quand la déclaration \
         est vraie : placé {mesure:.1}, déclaré {declare:.1}"
    );
}

#[test]
fn sur_un_placement_reel_le_total_monte_de_ce_que_l_etalement_retirait_a_tort() {
    let rs = regles("iop", 8);
    // Les mêmes trois ennemis, mais groupés comme ils le sont vraiment : deux
    // d'entre eux à UNE case de l'impact, pas un à une case et l'autre à deux.
    let groupes = vec![Case::new(0, 0), Case::new(0, 1), Case::new(1, 0)];
    let mesure = total(&rs, SORT, scenario(3, 1, Some(pose(groupes))));
    let declare = total(&rs, SORT, scenario(3, 1, None));
    let une = total(&rs, SORT, scenario(1, 0, None));

    // Déclaré : 100 + 90 + 80. Mesuré : 100 + 90 + 90. L'écart vaut donc dix
    // pour cent d'une frappe, et c'est un écart CHIFFRÉ, pas un signe.
    assert!(
        (mesure - declare - une * 0.10).abs() < 2.0,
        "l'écart doit valoir 10 % d'une frappe, soit {:.1}, et il vaut {:.1}",
        une * 0.10,
        mesure - declare
    );
}

#[test]
fn un_ennemi_hors_de_la_zone_ne_compte_plus() {
    let rs = regles("iop", 8);
    // Quatre ennemis, dont un à TROIS cases : hors du cercle de rayon 2. Le
    // régime déclaré le comptait quand même, puisqu'il ne connaît qu'un
    // nombre ; la zone dessinée le laisse dehors.
    let dont_un_dehors = vec![
        Case::new(0, 0),
        Case::new(0, 1),
        Case::new(0, 2),
        Case::new(0, 3),
    ];
    let place = total(&rs, SORT, scenario(4, 1, Some(pose(dont_un_dehors))));
    let trois_dedans = vec![Case::new(0, 0), Case::new(0, 1), Case::new(0, 2)];
    let sans_lui = total(&rs, SORT, scenario(3, 1, Some(pose(trois_dedans))));
    // Sans ce plancher, un calcul qui ne toucherait plus personne rendrait
    // zéro des deux côtés, et « le quatrième n'ajoute rien » serait vrai
    // sans rien prouver.
    assert!(
        sans_lui > 0.0,
        "les trois ennemis dans le cercle doivent bien être frappés, or le total          est {sans_lui:.1}"
    );
    assert!(
        (place - sans_lui).abs() < 1.0,
        "le quatrième est hors zone et ne doit rien ajouter : {place:.1} contre \
         {sans_lui:.1}"
    );
    // Et le régime déclaré, lui, le comptait : c'est la surestimation que le
    // placement corrige.
    let declare_quatre = total(&rs, SORT, scenario(4, 1, None));
    assert!(
        declare_quatre > place + 1.0,
        "le nombre déclaré surestime tant qu'il ignore le dessin : {declare_quatre:.1} \
         contre {place:.1}"
    );
}

#[test]
fn viser_le_vide_ne_frappe_personne() {
    let rs = regles("iop", 8);
    // Trois ennemis bien réels, mais la visée est à huit cases d'eux. Aucune
    // case de la zone ne tombe sur quelqu'un.
    let loin = Placement {
        lanceur: Case::new(-6, 0),
        visee: Case::new(0, 0),
        ennemis: vec![Case::new(8, 0), Case::new(8, 1), Case::new(9, 0)],
    };
    let mesure = total(&rs, SORT, scenario(3, 1, Some(loin)));
    assert_eq!(
        mesure, 0.0,
        "une zone qui ne couvre personne ne fait rien, et le solveur ne lance rien"
    );
}

/// La fourche : trois dents qui partent de la case visée, droit devant et en
/// diagonale vers l'avant. Le Trident de la Mer touche ce que ses dents
/// couvrent, pas davantage.
#[test]
fn la_fourche_se_place_sur_la_grille() {
    let rs = regles("forgelance", 20);
    let cibles = Cibles::Placees {
        lanceur: Case::new(-6, 0),
        visee: Case::new(0, 0),
        ennemis: vec![Case::new(0, 0), Case::new(1, 1)],
    };
    let ignores = cibles.placement_ignore(&rs);
    assert!(
        !ignores.iter().any(|s| s.contains("Trident de la Mer")),
        "la fourche se dessine, elle n'a plus rien à faire dans la liste des ignorés : \
         {ignores:?}"
    );
    assert!(
        !ignores.iter().any(|s| s.contains("Épieu Sismique")),
        "le rectangle de l'Épieu Sismique SE dessine : {ignores:?}"
    );

    // Tir vers la droite : (1, 1) est sur la dent en diagonale, (1, 0) aussi
    // serait touché, (0, 1) non, étant à côté de la case visée.
    let sort = "trident_de_la_mer";
    let place = |ennemis: Vec<Case>| {
        total(
            &rs,
            sort,
            scenario(
                2,
                0,
                Some(Placement {
                    lanceur: Case::new(-6, 0),
                    visee: Case::new(0, 0),
                    ennemis,
                }),
            ),
        )
    };
    let deux_dans_la_fourche = place(vec![Case::new(0, 0), Case::new(1, 1)]);
    let un_a_cote = place(vec![Case::new(0, 0), Case::new(0, 1)]);
    assert!(un_a_cote > 0.0, "la case visée est touchée : {un_a_cote:.1}");
    assert!(
        deux_dans_la_fourche > un_a_cote + 1.0,
        "l'ennemi sur la dent en diagonale doit être touché, celui d'à côté non : \
         {deux_dans_la_fourche:.1} contre {un_a_cote:.1}"
    );
}
