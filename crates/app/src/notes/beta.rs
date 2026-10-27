//! La bêta en cours : les changements de sorts qu'annoncent ses notes, classe
//! par classe, prêts à afficher dans l'onglet « Bêta en cours », et ce que le
//! mode bêta rejoue pour calculer avec eux.
//!
//! `dofus beta <note.json>…` les tire des notes et `data/beta.json` les embarque
//! dans l'application, qui ne sort jamais sur le réseau. Rien de la bêta n'entre
//! dans le calcul tant que le joueur ne passe pas en mode bêta. Chaque
//! changement se montre au plus haut grade : « Portée maximale : 4 / 5 / 6 →
//! 6 / 7 / 8 » devient « Portée maximale, 6 → 8 ».

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use super::{lire, Champ, Etat, Ligne, Note};
use crate::solve::{load_ruleset, CLASSES};

/// La donnée de l'onglet et du mode bêta, telle que l'application la sert :
/// sans les lignes de fichiers, que seul le calcul lit, avec la version en
/// ligne, et ce que le mode bêta peut faire ici (`disponible`) et fait
/// (`active`).
pub fn beta_json() -> String {
    static SERVIE: OnceLock<serde_json::Value> = OnceLock::new();
    let mut d = SERVIE
        .get_or_init(|| {
            let mut d: serde_json::Value = serde_json::from_str(crate::donnees::BETA).unwrap_or_default();
            if let Some(c) = d.get_mut("calcul").and_then(serde_json::Value::as_object_mut) {
                c.remove("fichiers");
            }
            let version: serde_json::Value = serde_json::from_str(crate::donnees::VERSION).unwrap_or_default();
            d["version_en_ligne"] = version["version_du_jeu"].clone();
            d
        })
        .clone();
    d["disponible"] = crate::donnees::beta_disponible().into();
    d["active"] = crate::donnees::beta_active().into();
    d.to_string()
}

/// Le mode bêta : les notes de la bêta appliquées à une copie des règles et des
/// relevés du dossier `data`, comme une note de sortie le serait, et les lignes
/// de fichiers qu'elles changent, que l'application rejoue sur sa donnée
/// compilée quand le joueur passe en bêta (`donnees::regler_la_beta`). Avec
/// elles, ce que le mode compte (`sorts`, `lignes`) et ce qu'il ne compte pas
/// (`non_comptes`). Une application refusée arrête tout : le mode bêta
/// calculerait faux.
pub fn pour_le_calcul(notes: &[Note], data: &Path) -> Result<serde_json::Value, String> {
    static ESSAIS: AtomicUsize = AtomicUsize::new(0);
    let copie = std::env::temp_dir().join(format!(
        "krozties-beta-{}-{}",
        std::process::id(),
        ESSAIS.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&copie);
    let resultat = copier_la_donnee(data, &copie).and_then(|chemins| {
        // Ce que le mode ne sait pas compter : les lignes d'un sort que la
        // donnée ne relit pas, et celles qu'une application laisse.
        let mut non_comptes = 0;
        let mut appliquees = Vec::new();
        for note in notes {
            non_comptes += super::lire_avec(note, &|c| super::appliquer::charger_depuis(&copie, c).ok())
                .iter()
                .filter(|l| !l.sorts.is_empty())
                .filter(|l| !matches!(l.etat, Etat::AAppliquer | Etat::DejaAJour | Etat::AutreGrade))
                .count();
            let a = super::appliquer::appliquer_a(note, &copie, "")?;
            non_comptes += a.laissees.len();
            appliquees.extend(a.appliquees);
        }
        let mut fichiers = serde_json::Map::new();
        for chemin in chemins {
            let lire_dans = |d: &Path| std::fs::read_to_string(d.join(&chemin)).map_err(|e| format!("{chemin}: {e}"));
            let (avant, apres) = (lire_dans(data)?, lire_dans(&copie)?);
            if avant == apres {
                continue;
            }
            let (a, b): (Vec<&str>, Vec<&str>) = (avant.split('\n').collect(), apres.split('\n').collect());
            if a.len() != b.len() {
                return Err(format!("{chemin} : la bêta change le nombre de lignes, ce que le mode bêta ne rejoue pas"));
            }
            let lignes: Vec<serde_json::Value> = a
                .iter()
                .zip(&b)
                .enumerate()
                .filter(|(_, (x, y))| x != y)
                .map(|(i, (x, y))| serde_json::json!([i, x, y]))
                .collect();
            fichiers.insert(chemin, lignes.into());
        }
        let mut sorts: Vec<(u32, &str)> =
            appliquees.iter().flat_map(|l| l.sorts.iter().map(move |s| (l.classe, s.as_str()))).collect();
        sorts.sort_unstable();
        sorts.dedup();
        Ok(serde_json::json!({
            "sorts": sorts.len(),
            "lignes": appliquees.len(),
            "non_comptes": non_comptes,
            "fichiers": fichiers,
        }))
    });
    let _ = std::fs::remove_dir_all(&copie);
    resultat
}

/// Copie ce qu'une note touche du dossier `data` : les règles, les relevés des
/// classes et `version.json`. Rend leurs chemins dans `data`, triés.
fn copier_la_donnee(data: &Path, copie: &Path) -> Result<Vec<String>, String> {
    let gardes: [(&str, fn(&str) -> bool); 2] = [
        ("rulesets", |n| n.ends_with(".yaml")),
        ("snapshots", |n| n.starts_with("breed-") && n.ends_with(".json")),
    ];
    let mut chemins = Vec::new();
    for (dossier, garde) in gardes {
        std::fs::create_dir_all(copie.join(dossier)).map_err(|e| format!("{}: {e}", copie.display()))?;
        let source = data.join(dossier);
        for entree in std::fs::read_dir(&source).map_err(|e| format!("{}: {e}", source.display()))? {
            let nom = entree.map_err(|e| e.to_string())?.file_name().to_string_lossy().into_owned();
            if !garde(&nom) {
                continue;
            }
            let chemin = format!("{dossier}/{nom}");
            std::fs::copy(data.join(&chemin), copie.join(&chemin)).map_err(|e| format!("{chemin}: {e}"))?;
            chemins.push(chemin);
        }
    }
    std::fs::copy(data.join("version.json"), copie.join("version.json")).map_err(|e| format!("version.json: {e}"))?;
    chemins.sort();
    Ok(chemins)
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

    /// Le mode bêta : une note de bêta qui retire un PA à la Flèche de Recul,
    /// appliquée à une copie de la donnée, donne les lignes de fichiers qui,
    /// rejouées sur la donnée compilée, la font coûter un PA de moins. La phrase
    /// sur le sort ne se compte pas, et la donnée du dépôt ne bouge pas.
    #[test]
    fn la_beta_se_rejoue_sur_la_donnee_compilee() {
        let data = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data"));
        let avant = std::fs::read_to_string(data.join("rulesets/cra.yaml")).unwrap();
        let r = load_ruleset(9).unwrap();
        let s = r.spells.iter().find(|s| s.id == "fleche_de_recul").unwrap();
        let pa = s.ap_cost.base;
        let note = Note {
            id: "1".into(),
            titre: "Patch Notes Bêta 9.9 - 17/09/2026".into(),
            forum: "Beta".into(),
            url: String::new(),
            noeuds: vec![
                noeud("h3", 0, "Crâ"),
                noeud("li", 2, &s.name.fr),
                noeud("li", 3, &format!("Coût en PA : {pa} → {}", pa - 1)),
                noeud("li", 3, "Le sort fait autre chose."),
            ],
        };
        let calcul = pour_le_calcul(&[note], data).unwrap();
        assert_eq!(
            (calcul["sorts"].as_u64(), calcul["lignes"].as_u64(), calcul["non_comptes"].as_u64()),
            (Some(1), Some(1), Some(1)),
            "{calcul}"
        );
        let fichiers = calcul["fichiers"].as_object().unwrap();
        let rejoue = |chemin: &str, origine: &str| match fichiers.get(chemin) {
            Some(lignes) => crate::donnees::rejouer(origine, lignes).unwrap(),
            None => origine.to_string(),
        };
        let mut regles =
            dofus_ruleset::Ruleset::from_yaml(&rejoue("rulesets/cra.yaml", crate::donnees::regles_compilees("cra").unwrap()))
                .unwrap();
        let releve = dofus_ruleset::snapshot::Snapshot::from_json(&rejoue(
            "snapshots/breed-9.json",
            crate::donnees::instantane_compile(9).unwrap(),
        ))
        .unwrap();
        assert!(regles.merge_snapshot(&releve).conflicts.is_empty());
        let recul = regles.spells.iter().find(|s| s.id == "fleche_de_recul").unwrap();
        assert_eq!(recul.ap_cost.base, pa - 1);
        assert_eq!(std::fs::read_to_string(data.join("rulesets/cra.yaml")).unwrap(), avant);
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
