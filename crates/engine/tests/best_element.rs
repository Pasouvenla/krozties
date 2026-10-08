//! Les dégâts « dans le meilleur élément du lanceur » frappent vraiment.
//!
//! Dix-neuf sorts du jeu, du Carnavalo du Zobal à l'Intimidation du Iop,
//! frappent dans l'élément où le lanceur est le plus fort. Le jeu les range
//! parmi les effets, sous l'identifiant 2822, bornes dans `dice_num` et
//! `dice_side`, pas comme des lignes de dégâts. DofusBook répète la même
//! fourchette dans les cinq éléments : elle ne dépend pas de l'élément,
//! seulement du meilleur.

use dofus_damage::{DamageProfile, Element, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn classe(nom: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{nom}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

/// Un build dont UN element domine largement, pour que « le meilleur » ne soit
/// pas ambigu.
fn build_mono(dominant: Element, deck: &[&str]) -> Build {
    let mut elements = [ElementStats {
        characteristic: 100,
        flat_damage: 0,
    }; 5];
    elements[dominant.index()] = ElementStats {
        characteristic: 900,
        flat_damage: 0,
    };
    Build {
        name: format!("mono {dominant:?}"),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements,
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    }
}

fn moteur(rs: &Ruleset, build: Build) -> Engine {
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 3,
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
    Engine::new(rs, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

fn degats_du_sort(rs: &Ruleset, dominant: Element, sort: &str) -> (i64, i64) {
    let t = moteur(rs, build_mono(dominant, &[sort])).damage_table(sort);
    assert!(
        !t.is_empty(),
        "{sort} : aucune ligne de degats. L'effet 2822 n'est pas lu."
    );
    t[0].normal
}

/// Le coeur du sujet : ces sorts ne valent pas zero.
#[test]
fn les_sorts_meilleur_element_infligent_des_degats() {
    for (nom, breed, sorts) in [
        ("iop", 8u32, &["intimidation"][..]),
        (
            "osamodas",
            2,
            &["fouet", "discipline", "cravache", "martinet"][..],
        ),
        ("sacrieur", 11, &["punition"][..]),
        ("eniripsa", 7, &["scalpel"][..]),
        ("steamer", 15, &["sonar"][..]),
        ("huppermage", 17, &["tribut", "supernova"][..]),
        ("forgelance", 20, &["charge_heroique"][..]),
        ("ecaflip", 6, &["tout_ou_rien"][..]),
        ("pandawa", 12, &["ebriete", "main_de_pandawa"][..]),
        ("zobal", 14, &["carnavalo"][..]),
    ] {
        let rs = classe(nom, breed);
        for sort in sorts {
            let d = degats_du_sort(&rs, Element::Fire, sort);
            assert!(d.0 > 0, "{nom}/{sort} inflige {d:?} degats");
        }
    }
}

/// « Meilleur » veut dire meilleur, pas Feu, la valeur par défaut du champ
/// `element`. On change l'élément dominant du build et on exige que les dégâts
/// ne bougent pas, la fourchette étant la même dans les cinq éléments : la ligne
/// a suivi le build.
#[test]
fn le_meilleur_element_suit_le_build() {
    let rs = classe("iop", 8);
    let feu = degats_du_sort(&rs, Element::Fire, "intimidation");
    for autre in [
        Element::Earth,
        Element::Water,
        Element::Air,
        Element::Neutral,
    ] {
        let d = degats_du_sort(&rs, autre, "intimidation");
        assert_eq!(
            d, feu,
            "Intimidation vaut {d:?} sur un build {autre:?} contre {feu:?} sur un build Feu : \
             la ligne est restee sur un element fixe au lieu de suivre le meilleur"
        );
    }
    // Et le chiffre est bien celui d'un build a 900 de caracteristique, pas
    // celui d'un build a 100 : sans quoi l'egalite ci-dessus serait vraie pour
    // la mauvaise raison, tous les elements etant lus au minimum.
    let faible = moteur(
        &rs,
        Build {
            profile: DamageProfile {
                elements: [ElementStats {
                    characteristic: 100,
                    flat_damage: 0,
                }; 5],
                ..Default::default()
            },
            ..build_mono(Element::Fire, &["intimidation"])
        },
    )
    .damage_table("intimidation")[0]
        .normal;
    assert!(
        feu.0 > faible.0,
        "900 de caracteristique donne {feu:?}, 100 en donne {faible:?} : \
         la ligne ne lit pas le build"
    );
}

/// Le palier Psychopathe du Carnavalo : 24-28 « Lanceur sans Psychopathe » et
/// 39-43 « Lanceur sous Psychopathe ».
#[test]
fn le_carnavalo_gagne_son_palier_sous_psychopathe() {
    let rs = classe("zobal", 14);
    let e = moteur(
        &rs,
        build_mono(Element::Fire, &["carnavalo", "masque_du_psychopathe"]),
    );
    let t = e.damage_table("carnavalo");
    assert_eq!(
        t.len(),
        2,
        "le Carnavalo doit avoir DEUX paliers, sans et sous Psychopathe, pas {}",
        t.len()
    );
    assert!(
        t[1].normal.0 > t[0].normal.0,
        "le palier sous Psychopathe ({:?}) ne depasse pas le palier sans ({:?})",
        t[1].normal,
        t[0].normal
    );
}

/// La Supernova frappe dans le meilleur ET dans le pire element.
#[test]
fn la_supernova_a_ses_deux_lignes() {
    let rs = classe("huppermage", 17);
    let e = moteur(&rs, build_mono(Element::Fire, &["supernova"]));
    let t = e.damage_table("supernova");
    let elements: Vec<Element> = t[0].by_element.iter().map(|r| r.element).collect();
    assert_eq!(
        elements.len(),
        2,
        "la Supernova doit porter deux lignes, meilleur et pire element, pas {elements:?}"
    );
    assert!(
        elements.contains(&Element::Fire),
        "le meilleur element du build, Feu, est absent de {elements:?}"
    );
    assert!(
        elements.iter().any(|e| *e != Element::Fire),
        "les deux lignes sont sur le meme element : le pire n'a pas ete resolu"
    );
}
