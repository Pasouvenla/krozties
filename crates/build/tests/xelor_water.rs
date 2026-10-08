//! Resolve the imported Xelor build and hold it against the figures the Python
//! prototype was fed: a DofusBook link becomes item ids, item ids become
//! statistics through the catalogue, and those are what the damage pipeline
//! consumes.

use std::collections::BTreeMap;

use dofus_build::*;
use dofus_damage::Element;

fn map(pairs: &[(&str, i32)]) -> BTreeMap<String, i32> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

fn resolved() -> Resolved {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let catalogue = Catalogue::load(format!("{root}/data/snapshots/items.json")).unwrap();
    let input = BuildInput {
        class: 5,
        level: 200,
        items: vec![
            34330, 31761, 32234, 24035, 31762, 17575, 34332, 32236, 34331, 13673, 0, 8698, 7043,
            6980, 739, 7754, 7115,
        ],
        invested: map(&[("chance", 398)]),
        forgemagic: [
            ("a1", map(&[("pa", 1)])),
            ("a2", map(&[("cc", 5)])),
            ("am", map(&[("cc", 8)])),
            ("ar", map(&[("cc", 6)])),
            ("bo", map(&[("cc", 6)])),
            ("br", map(&[("dc", 8)])),
            ("ca", map(&[("cc", 7)])),
            ("ce", map(&[("cc", 6)])),
            ("ch", map(&[("cc", 6)])),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect(),
        boosts: vec![
            Boost {
                name: "Rêve Nébuleux".into(),
                stat: "deg".into(),
                percent: 20,
                class_id: None,
                ..Default::default()
            },
            Boost {
                name: "Bleu Turquoise".into(),
                stat: "deg".into(),
                percent: 10,
                class_id: None,
                ..Default::default()
            },
            Boost {
                name: "Pourpre Profond".into(),
                stat: "deg".into(),
                percent: 5,
                class_id: None,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    resolve(&input, &catalogue)
}

/// The four elemental damage figures and the flat critical damage, which are
/// the numbers the pipeline is most sensitive to and where double-counting
/// all-element damage would show up immediately.
#[test]
fn the_damage_figures_match_the_calibrated_prototype() {
    let r = resolved();
    let flat = |e: Element| r.profile.elements[e.index()].flat_damage;
    assert_eq!(flat(Element::Water), 131);
    assert_eq!(flat(Element::Fire), 69);
    assert_eq!(flat(Element::Air), 74);
    assert_eq!(flat(Element::Earth), 65);
    assert_eq!(r.profile.flat_crit_damage, 167);
}

#[test]
fn the_characteristic_matches() {
    let r = resolved();
    assert_eq!(
        r.profile.elements[Element::Water.index()].characteristic,
        1118
    );
}

/// Dofus bonuses carry no parameters in the game data (a bare effect 984);
/// DofusBook's boosts have them. The Rêve Nébuleux reads turn by turn: +20%
/// final damage on odd turns, −10% on even turns (spell 5454).
#[test]
fn the_dofus_bonuses_resolve_to_the_hardcoded_multipliers() {
    let r = resolved();
    // Un bonus de Dofus ne compte que si son Dofus est dans le build : ce Xélor n'a
    // pas de Dofus Pourpre, son Pourpre Profond tombe ; le Vulbis porté compte son
    // Rouge Vermeil (+10 %), la cible passive ne tapant jamais.
    assert_eq!(
        r.damage_multipliers,
        vec![
            ("Rêve Nébuleux, tours impairs".to_string(), 120),
            ("Rêve Nébuleux, tours pairs".to_string(), 90),
            ("Bleu Turquoise".to_string(), 110),
            ("Rouge Vermeil".to_string(), 110),
        ]
    );
    assert!(r.assumptions.iter().any(|a| a.contains("Pourpre Profond non compté sans le")));
    // Ils s'additionnent en une seule statistique de % de dommages finaux :
    // +20 +10 +10 = ×1,40 aux tours impairs, −10 +10 +10 = ×1,10 aux tours pairs.
    let facteur = |garde: Tours| {
        let du_tour: Vec<u32> = r
            .damage_multipliers
            .iter()
            .filter(|(n, _)| r.tours.get(n).is_none_or(|t| *t == garde))
            .map(|(_, p)| *p)
            .collect();
        dofus_damage::FinalMultiplier::from_percents(&du_tour).apply(10_000)
    };
    assert_eq!(facteur(Tours::Impairs), 14_000);
    assert_eq!(facteur(Tours::Pairs), 11_000);
}

#[test]
fn the_active_sets_are_found() {
    let r = resolved();
    let mut names: Vec<&str> = r.sets_active.iter().map(|(n, _)| n.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "Panoplie Rhoarim",
            "Panoplie de Culbutœuf",
            "Panoplie du Gouffre"
        ]
    );
}

/// Two figures the equipment does not account for, asserted at their resolved
/// value so that a change either way is noticed: Power resolves to 30 (the
/// two-piece Rhoarim bonus) against the prototype's 110, and AP to 11 against
/// 12.
#[test]
fn the_unexplained_gaps_stay_visible() {
    let r = resolved();
    assert_eq!(
        r.profile.power, 30,
        "prototype: 110, écart non expliqué de 80"
    );
    assert_eq!(r.base_ap, 11, "prototype: 12, écart non expliqué de 1");
}

/// An item whose whole point is a scripted effect contributes no statistics and
/// says so, unless its effect arrives through the bonuses: the Dofus Nébuleux is
/// not flagged when its Rêve Nébuleux is active.
#[test]
fn scripted_items_are_flagged_rather_than_silently_empty() {
    let r = resolved();
    assert!(
        !r.uninterpreted.iter().any(|u| u.contains("Nébuleux")),
        "son bonus est actif : {:?}",
        r.uninterpreted
    );
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let catalogue = Catalogue::load(format!("{root}/data/snapshots/items.json")).unwrap();
    // Le Dofus Argenté n'a que son effet spécial, qui se gagne sous 20 % de
    // vie : sans bonus actif, il se signale.
    let sans_bonus = BuildInput { class: 5, level: 200, items: vec![19629], ..Default::default() };
    let r = resolve(&sans_bonus, &catalogue);
    assert!(
        r.uninterpreted
            .iter()
            .any(|u| u.contains("Dofus Argenté") && u.contains("ne compte que s'il est actif")),
        "{:?}",
        r.uninterpreted
    );
}
