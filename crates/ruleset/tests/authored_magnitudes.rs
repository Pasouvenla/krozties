//! Every hand-written damage range must exist on the spell it belongs to.
//!
//! The merge fills gaps, it does not audit a magnitude already `Known`, and a
//! resource without `dofusdb_source` is skipped entirely: nothing else checks
//! these numbers. The check is per spell, not per class, so that another spell
//! carrying the same range cannot give a wrong number an alibi.

use dofus_ruleset::{Maybe, Ruleset};
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

/// The ranges one spell of the snapshot holds, whatever the kind: plain damage
/// and life steal both end up as a range in a ruleset, so both count.
fn ranges_of(snapshot: &serde_json::Value, dofusdb_id: u32) -> BTreeSet<(String, i64, i64)> {
    let mut out = BTreeSet::new();
    let Some(spell) = snapshot["spells"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|s| s["id"].as_u64() == Some(u64::from(dofusdb_id)))
    else {
        return out;
    };
    for level in spell["levels"].as_array().into_iter().flatten() {
        // Les dégâts posés au sol aussi : les glyphes du Féca sont écrits en
        // états, sans autre ligne à comparer.
        for key in [
            "normal_lines",
            "critical_lines",
            "placed_lines",
            "placed_critical_lines",
        ] {
            for line in level[key].as_array().into_iter().flatten() {
                let (Some(el), Some(r)) = (line["element"].as_str(), line["range"].as_array())
                else {
                    continue;
                };
                if let (Some(a), Some(b)) = (r[0].as_i64(), r[1].as_i64()) {
                    out.insert((el.to_string(), a, b));
                }
            }
        }
        // Les dégâts dans le meilleur élément (2822) ou le pire (2832) ne sont
        // pas des lignes : le jeu les range parmi les effets, leurs bornes dans
        // `dice_num`/`dice_side`.
        for key in ["other_effects", "critical_other_effects"] {
            for e in level[key].as_array().into_iter().flatten() {
                let id = e["id"].as_i64();
                if id != Some(2822) && id != Some(2832) {
                    continue;
                }
                let (Some(a), Some(b)) = (e["dice_num"].as_i64(), e["dice_side"].as_i64()) else {
                    continue;
                };
                // `dice_side` a zero veut dire valeur FIXE, pas borne haute a
                // zero : la Main de Pandawa frappe a 70 tout rond.
                let haut = if b == 0 { a } else { b };
                for el in ["fire", "earth", "water", "air", "neutral"] {
                    out.insert((el.to_string(), a, haut));
                }
            }
        }
    }
    out
}

/// Les fourchettes que DofusBook donne au même sort. Il déplie les paliers d'un
/// cumul, que l'instantané range dans un bonus de base (les six crans du poison
/// des Toxines). L'alibi reste borné au même sort : une fourchette n'existe que si
/// l'une des deux captures la porte sur ce sort.
fn ranges_dofusbook(breed: u32, dofusdb_id: u32) -> BTreeSet<(String, i64, i64)> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut out = BTreeSet::new();
    let Ok(raw) = std::fs::read_to_string(format!(
        "{root}/data/snapshots/dofusbook-{breed}.json"
    )) else {
        return out;
    };
    let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
    for sort in doc["sorts"].as_array().into_iter().flatten() {
        if sort["id"].as_u64() != Some(u64::from(dofusdb_id)) {
            continue;
        }
        for g in sort["g"].as_array().into_iter().flatten() {
            for e in g[2].as_array().into_iter().flatten() {
                let code = e[0].as_str().unwrap_or_default();
                // `d` degats, `v` vol de vie, `g` degats d'un glyphe : tous
                // finissent en fourchette dans un ruleset. Les soins et les
                // poussees, non.
                let Some(reste) = code
                    .strip_prefix('d')
                    .or_else(|| code.strip_prefix('v'))
                    .or_else(|| code.strip_prefix('g'))
                else {
                    continue;
                };
                if code == "dv" {
                    continue; // pourcentage de vie, pas une fourchette d'element
                }
                let (Some(a), Some(b)) = (e[2].as_i64(), e[3].as_i64()) else {
                    continue;
                };
                let haut = if b == 0 { a } else { b };
                match reste {
                    "t" => out.insert(("earth".into(), a, haut)),
                    "f" => out.insert(("fire".into(), a, haut)),
                    "a" => out.insert(("air".into(), a, haut)),
                    "e" => out.insert(("water".into(), a, haut)),
                    "n" => out.insert(("neutral".into(), a, haut)),
                    // Meilleur et pire element : la fourchette est la meme dans
                    // les cinq, l'element se decidant sur le build.
                    "me" | "pe" => {
                        for el in ["fire", "earth", "water", "air", "neutral"] {
                            out.insert((el.to_string(), a, haut));
                        }
                        true
                    }
                    _ => false,
                };
            }
        }
    }
    out
}

fn snapshot(breed: u32) -> serde_json::Value {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let raw = std::fs::read_to_string(format!("{root}/data/snapshots/breed-{breed}.json"))
        .unwrap_or_else(|e| panic!("breed-{breed}.json: {e}"));
    serde_json::from_str(&raw).unwrap()
}

#[test]
fn no_hand_written_range_contradicts_its_own_spell() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut fautes = Vec::new();
    let mut verifiees = 0usize;

    for (classe, breed) in CLASSES {
        // Loaded WITHOUT merging: after a merge every range is `Known` and the
        // hand-written ones can no longer be told from the filled ones.
        let rs = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
            .unwrap_or_else(|e| panic!("{classe}: {e}"));
        let snap = snapshot(*breed);

        // `el` vaut `None` sur une ligne « meilleur » ou « pire » element : sa
        // fourchette est la meme dans les cinq, et l'element se decide sur le
        // build. On la compare alors a n'importe lequel.
        let mut controler = |quoi: &str, id: u32, el: Option<String>, r: (i32, i32)| {
            let mut connues = ranges_of(&snap, id);
            let du_livre = ranges_dofusbook(*breed, id);
            connues.extend(du_livre.iter().cloned());
            if connues.is_empty() {
                return; // le sort n'est dans aucune capture, rien a comparer
            }
            verifiees += 1;
            let (a, b) = (i64::from(r.0), i64::from(r.1));
            let trouve = match &el {
                Some(e) => connues.contains(&(e.clone(), a, b)),
                None => connues.iter().any(|(_, x, y)| *x == a && *y == b),
            };
            if !trouve {
                fautes.push(format!(
                    "{classe}: {quoi} porte {} [{}, {}], absent du sort {id} \
                     dont les fourchettes sont {:?}",
                    el.as_deref().unwrap_or("meilleur/pire element"),
                    r.0,
                    r.1,
                    connues
                ));
            }
        };
        let nom_element = |l: &dofus_ruleset::LineDef| -> Option<String> {
            if l.best_element || l.worst_element {
                None
            } else {
                Some(format!("{:?}", l.element?).to_lowercase())
            }
        };

        for s in &rs.spells {
            let Some(id) = s.dofusdb_id else { continue };
            for l in &s.lines {
                if let Maybe::Known(r) = &l.normal {
                    controler(&s.id, id, nom_element(l), *r);
                }
                if let Maybe::Known(r) = &l.critical {
                    controler(&s.id, id, nom_element(l), *r);
                }
            }
        }

        for res in &rs.resources {
            // The source spell, declared or else the spell of the same name:
            // a state is written next to the spell that applies it, and both
            // carry the same id by convention.
            let id = res.dofusdb_source.or_else(|| {
                rs.spells
                    .iter()
                    .find(|s| s.id == res.id)
                    .and_then(|s| s.dofusdb_id)
            });
            let Some(id) = id else { continue };
            for eff in &res.while_present {
                for l in &eff.lines {
                    if let Maybe::Known(r) = &l.normal {
                        controler(&res.id, id, nom_element(l), *r);
                    }
                    if let Maybe::Known(r) = &l.critical {
                        controler(&res.id, id, nom_element(l), *r);
                    }
                }
            }
        }
    }

    assert!(
        verifiees > 20,
        "seulement {verifiees} fourchettes contrôlées : le test ne contrôle plus rien"
    );
    assert!(
        fautes.is_empty(),
        "{} fourchette(s) écrite(s) à la main que leur propre sort ne porte pas :\n{}",
        fautes.len(),
        fautes.join("\n")
    );
}
