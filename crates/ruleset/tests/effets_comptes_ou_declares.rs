//! Un effet que le moteur sait dire est compté, ou bien il est déclaré.
//!
//! Le moteur parle quatre langues de bonus : la Puissance (effet 138), les
//! Dommages fixes (112), le multiplicateur de dommages subis (1163) et le taux de
//! coup critique (115). Un sort qui frappe et porte l'un d'eux avec une magnitude
//! lisible le compte, ou dit pourquoi il ne le compte pas. Un `dice_side` non nul
//! est une fourchette, moyennée comme celle d'une ligne de dégâts : certains de
//! ces effets valent exactement les dégâts du sort, et c'est voulu.

use dofus_ruleset::{DamageModifier, Effect, Ruleset};
use std::collections::BTreeSet;

const CLASSES: &[(&str, u32)] = &[
    ("feca", 1),
    ("osamodas", 2),
    ("enutrof", 3),
    ("sram", 4),
    ("xelor", 5),
    ("ecaflip", 6),
    ("eniripsa", 7),
    ("iop", 8),
    ("cra", 9),
    ("sadida", 10),
    ("sacrieur", 11),
    ("pandawa", 12),
    ("roublard", 13),
    ("zobal", 14),
    ("steamer", 15),
    ("eliotrope", 16),
    ("huppermage", 17),
    ("ouginak", 18),
    ("forgelance", 20),
];

/// Les quatre effets du jeu que le vocabulaire sait porter, et le `kind` qui
/// les dit.
const PARLANTS: &[(i64, &str, &str)] = &[
    (138, "characteristic", "Puissance"),
    (112, "flat_damage", "Dommages fixes"),
    (1163, "final_multiplier", "dommages subis"),
    (115, "critical_rate", "taux de coup critique"),
    (418, "critical_damage", "Dommages Critiques"),
    (
        421,
        "critical_resistance",
        "Résistance Critique de la cible",
    ),
    (414, "push_damage", "Dommages Poussée"),
];

fn kind_de(m: &DamageModifier) -> &'static str {
    match m {
        DamageModifier::Characteristic { .. } => "characteristic",
        DamageModifier::FlatDamage { .. } => "flat_damage",
        DamageModifier::FinalMultiplier { .. } => "final_multiplier",
        DamageModifier::BaseDamage { .. } => "base_damage",
        DamageModifier::CriticalRate { .. } => "critical_rate",
        DamageModifier::CriticalDamage { .. } => "critical_damage",
        DamageModifier::CriticalResistance { .. } => "critical_resistance",
        DamageModifier::PushDamage { .. } => "push_damage",
        DamageModifier::DomainPercent { .. } => "domain_percent",
    }
}

#[test]
fn un_effet_lisible_est_compte_ou_declare() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut fautes = Vec::new();
    let mut controles = 0usize;

    for (classe, breed) in CLASSES {
        let rs = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
            .unwrap_or_else(|e| panic!("{classe}: {e}"));
        let brut =
            std::fs::read_to_string(format!("{root}/data/snapshots/breed-{breed}.json"))
                .unwrap_or_else(|e| panic!("breed-{breed}.json: {e}"));
        let snap: serde_json::Value = serde_json::from_str(&brut).unwrap();

        for spell in &rs.spells {
            let Some(id) = spell.dofusdb_id else { continue };
            // Un sort qui ne frappe pas n'a pas de total à gonfler.
            if !spell.effects.iter().any(|e| matches!(e, Effect::Damage)) {
                continue;
            }
            let Some(sp) = snap["spells"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|s| s["id"].as_u64() == Some(u64::from(id)))
            else {
                continue;
            };
            let Some(lvl) = sp["levels"].as_array().and_then(|l| l.last()) else {
                continue;
            };
            // Ce que ce sort alimente déjà : les `kind` portés par les
            // ressources qu'il pose, et ceux de ses propres lignes.
            let mut nourris: BTreeSet<&str> = BTreeSet::new();
            for e in &spell.effects {
                if let Effect::Gain { resource, .. } = e {
                    if let Some(r) = rs.resource(resource) {
                        nourris.extend(r.modifies_damage.iter().map(kind_de));
                    }
                }
            }
            let dit = format!(
                "{} {}",
                spell.assumptions.join(" "),
                spell.open_questions.join(" ")
            )
            .to_lowercase();

            for e in lvl["other_effects"].as_array().into_iter().flatten() {
                let Some(eid) = e["id"].as_i64() else {
                    continue;
                };
                let Some((_, kind, quoi)) = PARLANTS.iter().find(|(i, _, _)| *i == eid) else {
                    continue;
                };
                let (Some(num), Some(side)) = (e["dice_num"].as_i64(), e["dice_side"].as_i64())
                else {
                    continue;
                };
                // Une magnitude nulle ne dit rien ; tout le reste se lit, que
                // ce soit une valeur fixe (`side` à zéro) ou une fourchette.
                let _ = side;
                if num == 0 {
                    continue;
                }
                controles += 1;
                if nourris.contains(kind) || dit.contains(&format!("effet {eid}")) {
                    continue;
                }
                fautes.push(format!(
                    "{classe}: `{}` porte l'effet {eid} ({quoi}) à {num}, que le moteur sait dire, \
                     sans le compter ni dire pourquoi. Le compter, ou l'écrire dans ses \
                     `assumptions` en le nommant « effet {eid} ».",
                    spell.id
                ));
            }
        }
    }

    assert!(
        controles >= 10,
        "seulement {controles} effets contrôlés : le test ne contrôle plus rien"
    );
    assert!(
        fautes.is_empty(),
        "{} effet(s) lisible(s) ni comptés ni déclarés :\n{}",
        fautes.len(),
        fautes.join("\n")
    );
}
