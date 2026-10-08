//! Resolve the imported Xelor build and compare it with the figures the Python
//! prototype was fed: link, item ids, catalogue, then the exact numbers the
//! damage pipeline consumes.

use dofus_build::*;
use dofus_damage::Element;
use std::collections::BTreeMap;

fn map(pairs: &[(&str, i32)]) -> BTreeMap<String, i32> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let catalogue = Catalogue::load(format!("{root}/data/snapshots/items.json")).unwrap();

    let input = BuildInput {
        class: 5,
        level: 200,
        // Ankama ids, from DofusBook's `official` field.
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

    let r = resolve(&input, &catalogue);

    // What the prototype was fed.
    let attendu: &[(&str, i32, i32)] = &[
        (
            "Chance",
            r.profile.elements[Element::Water.index()].characteristic,
            1118,
        ),
        ("Puissance", r.profile.power, 110),
        (
            "Dommages Eau",
            r.profile.elements[Element::Water.index()].flat_damage,
            131,
        ),
        (
            "Dommages Feu",
            r.profile.elements[Element::Fire.index()].flat_damage,
            69,
        ),
        (
            "Dommages Air",
            r.profile.elements[Element::Air.index()].flat_damage,
            74,
        ),
        (
            "Dommages Terre",
            r.profile.elements[Element::Earth.index()].flat_damage,
            65,
        ),
        ("Dommages critiques", r.profile.flat_crit_damage, 167),
        ("PA", i32::from(r.base_ap), 12),
    ];

    println!(
        "{:<20} {:>8} {:>10}  ecart",
        "statistique", "resolu", "prototype"
    );
    println!("{}", "-".repeat(52));
    let mut exacts = 0;
    for (nom, obtenu, cible) in attendu {
        let ecart = obtenu - cible;
        if ecart == 0 {
            exacts += 1;
        }
        println!(
            "{nom:<20} {obtenu:>8} {cible:>10}  {}",
            if ecart == 0 {
                "=".into()
            } else {
                format!("{ecart:+}")
            }
        );
    }
    println!("\n{exacts}/{} exacts", attendu.len());

    println!("\nmultiplicateurs de degats :");
    for (nom, p) in &r.damage_multipliers {
        println!("   {nom} : x{:.2}", f64::from(*p) / 100.0);
    }
    println!("\npanoplies actives :");
    for (nom, n) in &r.sets_active {
        println!("   {nom} ({n} pieces)");
    }
    println!("\nnon interprete :");
    for u in &r.uninterpreted {
        println!("   {u}");
    }
    println!("\nhypotheses :");
    for a in &r.assumptions {
        println!("   {a}");
    }
}
