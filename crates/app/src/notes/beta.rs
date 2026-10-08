//! L'onglet « Bêta en cours » : les changements de sorts qu'annoncent les notes
//! de la bêta, classe par classe, prêts à afficher.
//!
//! `dofus beta <note.json>…` les tire des notes, à blanc, et `data/beta.json`
//! les embarque dans l'application, qui ne sort jamais sur le réseau. Rien de la
//! bêta n'entre dans le calcul. Chaque changement se montre au plus haut grade :
//! « Portée maximale : 4 / 5 / 6 → 6 / 7 / 8 » devient « Portée maximale,
//! 6 → 8 ».

use super::{lire, Champ, Etat, Ligne, Note};
use crate::solve::{load_ruleset, CLASSES};

/// La donnée de l'onglet, telle que l'application la sert.
pub fn beta_json() -> String {
    crate::donnees::BETA.to_string()
}

/// La valeur d'un côté de la flèche, au plus haut grade : le dernier morceau
/// d'une suite « 4 / 5 / 6 », sans point final.
fn au_plus_haut(texte: &str) -> String {
    texte.rsplit('/').next().unwrap_or(texte).trim().trim_end_matches('.').trim().to_string()
}

/// Une ligne telle que l'onglet la montre : un changement, « libellé, de →
/// vers », ou une phrase.
fn montrer(l: &Ligne) -> serde_json::Value {
    let libelle = match (l.champ, l.niveau) {
        (Some(Champ::Degats), Some(n)) => Some(format!("Dégâts (niv. {n})")),
        _ => l.texte.split_once(':').map(|(a, _)| a.trim().to_string()),
    };
    match (libelle, &l.apres) {
        (Some(libelle), Some(apres)) => serde_json::json!({
            "libelle": libelle,
            "de": l.avant.as_deref().map(au_plus_haut),
            "vers": au_plus_haut(apres),
        }),
        _ => serde_json::json!({ "texte": l.texte }),
    }
}

/// Les notes de la bêta, lues à blanc, rangées pour l'onglet : leurs titres, et
/// chaque classe touchée avec ses phrases générales et ses sorts.
pub fn pour_l_onglet(notes: &[Note]) -> serde_json::Value {
    struct Sort {
        ids: Vec<String>,
        lignes: Vec<serde_json::Value>,
    }
    struct Classe {
        id: u32,
        generales: Vec<String>,
        sorts: Vec<Sort>,
    }
    let mut classes: Vec<Classe> = Vec::new();
    for note in notes {
        for l in lire(note) {
            if l.etat == Etat::AutreGrade {
                continue;
            }
            let i = match classes.iter().position(|c| c.id == l.classe) {
                Some(i) => i,
                None => {
                    classes.push(Classe { id: l.classe, generales: Vec::new(), sorts: Vec::new() });
                    classes.len() - 1
                }
            };
            let c = &mut classes[i];
            if l.sorts.is_empty() {
                c.generales.push(l.texte.clone());
                continue;
            }
            let j = match c.sorts.iter().position(|s| s.ids == l.sorts) {
                Some(j) => j,
                None => {
                    c.sorts.push(Sort { ids: l.sorts.clone(), lignes: Vec::new() });
                    c.sorts.len() - 1
                }
            };
            c.sorts[j].lignes.push(montrer(&l));
        }
    }
    let version = notes.first().and_then(|n| {
        n.titre
            .split(|c: char| !(c.is_ascii_digit() || c == '.'))
            .find(|m| m.contains('.') && m.split('.').all(|p| !p.is_empty()))
            .map(str::to_string)
    });
    let classes: Vec<serde_json::Value> = classes
        .into_iter()
        .map(|c| {
            let regles = load_ruleset(c.id).ok();
            let nom_du_sort = |id: &str| {
                regles
                    .as_ref()
                    .and_then(|r| r.spells.iter().find(|s| s.id == id))
                    .map_or_else(|| id.to_string(), |s| s.name.fr.clone())
            };
            serde_json::json!({
                "classe": c.id,
                "nom": CLASSES.iter().find(|k| k.0 == c.id).map_or("?", |k| k.2),
                // Ce que la pastille de la classe compte : ses phrases et
                // les lignes de ses sorts, un Pandawa sans sort nommé ayant
                // aussi ses changements.
                "changements": c.generales.len() + c.sorts.iter().map(|s| s.lignes.len()).sum::<usize>(),
                "generales": c.generales,
                "sorts": c.sorts.iter().map(|s| serde_json::json!({
                    "ids": s.ids,
                    "nom": s.ids.iter().map(|id| nom_du_sort(id)).collect::<Vec<_>>().join(", "),
                    "lignes": s.lignes,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::json!({
        "version": version,
        "notes": notes.iter().map(|n| serde_json::json!({ "id": n.id, "titre": n.titre, "url": n.url })).collect::<Vec<_>>(),
        "classes": classes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::Noeud;

    fn noeud(balise: &str, profondeur: u8, texte: &str) -> Noeud {
        Noeud { balise: balise.into(), profondeur, texte: texte.into() }
    }

    /// La donnée embarquée se lit, et chaque classe a son nom.
    #[test]
    fn la_beta_embarquee_se_lit() {
        let d: serde_json::Value = serde_json::from_str(&beta_json()).unwrap();
        for c in d["classes"].as_array().unwrap() {
            assert!(c["nom"].as_str().is_some_and(|n| n != "?"), "{c}");
        }
    }

    /// Une note d'essai : la phrase générale, le sort et ses deux changements
    /// au plus haut grade, la ligne d'un niveau plus bas omise, une phrase.
    #[test]
    fn un_changement_se_montre_au_plus_haut_grade() {
        let r = load_ruleset(9).unwrap();
        let s = r.spells.iter().find(|s| s.id == "fleche_de_recul").unwrap();
        let note = Note {
            id: "1".into(),
            titre: "Patch Notes Bêta 9.9 - 17/09/2026".into(),
            forum: "Beta".into(),
            url: String::new(),
            noeuds: vec![
                noeud("h3", 0, "Crâ"),
                noeud("li", 1, "Une phrase générale."),
                noeud("li", 2, &s.name.fr),
                noeud("li", 3, "Portée maximale : 4 / 5 / 6 → 6 / 7 / 8"),
                noeud("li", 3, "Niveau 10 : 1 à 2 (3 à 4) → 2 à 3 (4 à 5)"),
                noeud("li", 3, "Niveau 99 : 5 à 6 (7 à 8) → 6 à 7 (8 à 9)"),
                noeud("li", 3, "Le sort fait autre chose."),
            ],
        };
        let d = pour_l_onglet(&[note]);
        assert_eq!(d["version"], "9.9");
        let c = &d["classes"][0];
        assert_eq!((c["nom"].as_str(), c["changements"].as_u64()), (Some("Crâ"), Some(4)));
        assert_eq!(c["generales"], serde_json::json!(["Une phrase générale."]));
        let lignes = c["sorts"][0]["lignes"].as_array().unwrap();
        assert_eq!(c["sorts"][0]["nom"], s.name.fr.as_str());
        assert_eq!(lignes[0], serde_json::json!({ "libelle": "Portée maximale", "de": "6", "vers": "8" }));
        assert_eq!(lignes[1]["libelle"], "Dégâts (niv. 99)", "{lignes:?}");
        assert_eq!(lignes[2], serde_json::json!({ "texte": "Le sort fait autre chose." }));
        assert_eq!(lignes.len(), 3, "le niveau 10 est omis : {lignes:?}");
    }
}
