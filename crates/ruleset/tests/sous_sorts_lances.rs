//! Ce qu'un sort lance, et qui frappe sans forcément être compté.
//!
//! Six effets lancent un autre sort (1160, 792, 2794, 2960, 1017, 2160) : leur
//! paramètre est un identifiant de sort, leur `dice_side` son grade, et le
//! résultat est dans `data/snapshots/subspells.json`. La plupart relancent le
//! sort lui-même, déjà compté, mais pas tous : la Potion Magique du Pandawa,
//! atteinte depuis sept sorts, porte des fourchettes qu'aucun d'eux ne porte.
//! Toute fourchette d'un sous-sort lancé est comptée dans le fichier ou déclarée
//! sur le sort qui la lance, jamais muette.

use serde_json::Value;
use std::collections::BTreeSet;

/// Les dix-neuf classes, pas seulement les complètes : ce test ne demande pas
/// qu'un sort compte ce que son sous-sort inflige, seulement qu'il le dise.
const TOUTES: &[(&str, u32)] = &[
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

/// Les effets qui lancent un autre sort.
const LANCEURS: &[u64] = &[1160, 792, 2794, 2960, 1017, 2160];

fn racine() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../..").to_string()
}

fn json(chemin: &str) -> Value {
    let brut = std::fs::read_to_string(format!("{}/{chemin}", racine()))
        .unwrap_or_else(|e| panic!("{chemin} : {e}"));
    serde_json::from_str(&brut).unwrap_or_else(|e| panic!("{chemin} : {e}"))
}

/// Les identifiants d'effet élémentaires, dégâts et vol de vie confondus, plus
/// le meilleur et le pire élément.
fn est_une_ligne(effet: u64) -> bool {
    (91..=100).contains(&effet) || matches!(effet, 2822 | 2832 | 2828)
}

#[test]
fn aucune_fourchette_de_sous_sort_ne_reste_muette() {
    let cache = json("data/snapshots/subspells.json");
    let mut muettes = Vec::new();
    // ⚠️ COMBIEN DE FOURCHETTES CE TEST A-T-IL VRAIMENT REGARDEES. Un chemin de
    // fichier de travers, un cache vide, un champ renomme : la boucle tournerait
    // sans rien examiner et le test passerait au vert en ne prouvant rien.
    let mut examinees = 0usize;

    for (classe, breed) in TOUTES {
        let snap = json(&format!("data/snapshots/breed-{breed}.json"));
        let fichier =
            std::fs::read_to_string(format!("{}/data/rulesets/{classe}.yaml", racine()))
                .unwrap();

        // Toutes les fourchettes que le fichier porte, d'où qu'elles viennent
        // (ligne de sort, ligne d'état, charge différée). Le texte brut suffit :
        // une fourchette écrite est comptée, quelle que soit la clef qui la porte.
        let comptees: BTreeSet<String> = fichier
            .lines()
            .filter_map(|l| {
                let l = l.trim();
                let reste = l
                    .strip_prefix("normal: ")
                    .or_else(|| l.strip_prefix("critical: "))?;
                let reste = reste.trim_start_matches('[').trim_end_matches(']');
                let (a, b) = reste.split_once(',')?;
                Some(format!("{}-{}", a.trim(), b.trim()))
            })
            .collect();

        for sort in snap["spells"].as_array().unwrap() {
            let nom = sort["name"]["fr"].as_str().unwrap_or("?");
            let bloc = bloc_du_sort(&fichier, sort["id"].as_u64().unwrap_or(0));
            // `normal: unknown` est une fourchette comptée, que la fusion remplit
            // depuis l'instantané : les lignes du sort la portent, même si le texte
            // du fichier ne l'écrit pas.
            let propres: BTreeSet<String> = sort["levels"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|lv| {
                    ["normal_lines", "critical_lines", "placed_lines"]
                        .into_iter()
                        .flat_map(move |clef| lv[clef].as_array().cloned().unwrap_or_default())
                })
                .filter_map(|l| {
                    let r = l["range"].as_array()?;
                    Some(format!("{}-{}", r[0].as_i64()?, r[1].as_i64()?))
                })
                .collect();
            // Le dernier grade seul : le parc modélise le sort au niveau
            // maximum, et chaque grade lance le grade correspondant du sous-sort.
            if let Some(niveau) = sort["levels"].as_array().unwrap().last() {
                for effet in niveau["other_effects"].as_array().unwrap_or(&Vec::new()) {
                    let id = effet["id"].as_u64().unwrap_or(0);
                    if !LANCEURS.contains(&id) {
                        continue;
                    }
                    let Some(cible) = effet["dice_num"].as_u64() else {
                        continue;
                    };
                    let grade = effet["dice_side"].as_u64().unwrap_or(1);
                    let Some(sous) = cache.get(cible.to_string()) else {
                        continue;
                    };
                    let sous_nom = sous["nom"].as_str().unwrap_or("?");
                    for g in sous["grades"].as_array().unwrap_or(&Vec::new()) {
                        if g["grade"].as_u64() != Some(grade) {
                            continue;
                        }
                        for ligne in g["lignes_degats"].as_array().unwrap_or(&Vec::new()) {
                            let t = ligne.as_array().unwrap();
                            let (e, lo, hi) = (
                                t[0].as_u64().unwrap_or(0),
                                t[1].as_i64().unwrap_or(0),
                                t[2].as_i64().unwrap_or(0),
                            );
                            if !est_une_ligne(e) {
                                continue;
                            }
                            examinees += 1;
                            let f = format!("{lo}-{hi}");
                            // Comptée quelque part, ou nommée sur le sort qui la
                            // lance. Le nom ne vaut que s'il diffère de celui du
                            // sort, toujours présent dans son propre bloc : sinon,
                            // il faut la fourchette, écrite dans une ligne ou une
                            // réserve.
                            let nomme_autrement = sous_nom != nom;
                            let dite = propres.contains(&f)
                                || comptees.contains(&f)
                                || bloc.as_deref().is_some_and(|b| {
                                    b.contains(&f) || (nomme_autrement && b.contains(sous_nom))
                                });
                            if !dite {
                                muettes.push(format!(
                                    "{classe} / {nom} lance « {sous_nom} » qui frappe {f}, \
                                     et le fichier n'en dit rien"
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        examinees >= 70,
        "ce test n'a regarde que {examinees} fourchette(s) : il ne prouve plus rien"
    );
    muettes.sort_unstable();
    muettes.dedup();
    assert!(muettes.is_empty(), "{}", muettes.join("\n"));
}

/// Le bloc YAML d'un sort, repéré par son `dofusdb_id`.
fn bloc_du_sort(fichier: &str, dofusdb_id: u64) -> Option<String> {
    let marque = format!("dofusdb_id: {dofusdb_id}\n");
    let i = fichier.find(&marque)?;
    let debut = fichier[..i].rfind("\n  - id: ")?;
    let fin = fichier[i..]
        .find("\n  - id: ")
        .map_or(fichier.len(), |j| i + j);
    Some(fichier[debut..fin].to_string())
}
