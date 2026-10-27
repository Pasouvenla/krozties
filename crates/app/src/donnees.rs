//! La donnée du jeu, compilée dans l'application : l'application installée n'a
//! pas accès au dossier du code, et chaque version emporte sa donnée. En mode
//! bêta, les règles et les relevés que la bêta change se lisent tels qu'elle les
//! change.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// La version du jeu décrite par la donnée, et les notes de sortie appliquées.
pub const VERSION: &str = include_str!("../../../data/version.json");

/// Les changements de sorts annoncés par la bêta en cours, pour l'onglet
/// « Bêta en cours », et les lignes de fichiers que le mode bêta rejoue
/// (`calcul`, voir `notes::beta::pour_le_calcul`).
pub const BETA: &str = include_str!("../../../data/beta.json");

/// Le catalogue des objets et des panoplies.
pub const OBJETS: &str = include_str!("../../../data/snapshots/items.json");
/// La zone de frappe de chaque type d'arme.
pub const TYPES_D_ARME: &str = include_str!("../../../data/snapshots/types-d-arme.json");

/// Les sorts que portent les objets (effets spéciaux et sorts temporaires), au
/// format des instantanés de classe.
pub const SORTS_D_OBJETS: &str = include_str!("../../../data/snapshots/sorts-d-objets.json");

/// L'icône de chaque objet, par son identifiant.
pub const ICONES_D_OBJETS: &str = include_str!("../../../data/snapshots/item-icons.json");

/// Les invocations de chaque classe et les invocations communes.
pub const INVOCATIONS: &str = include_str!("../../../data/snapshots/invocations.json");

/// Les sorts communs qui invoquent (l'Arakne, le Chaferfu et leurs variantes).
pub const COMMUNS: &str = include_str!("../../../data/communs.yaml");

/// L'instantané des sorts d'une classe, par son identifiant de race : celui de
/// la bêta quand le joueur calcule avec elle.
pub fn instantane(breed: u32) -> Option<&'static str> {
    corrige(&format!("snapshots/breed-{breed}.json")).or_else(|| instantane_compile(breed))
}

/// Les règles d'une classe, par le nom de leur fichier : celles de la bêta quand
/// le joueur calcule avec elle.
pub fn regles(nom: &str) -> Option<&'static str> {
    corrige(&format!("rulesets/{nom}.yaml")).or_else(|| regles_compilees(nom))
}

static BETA_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Le joueur calcule-t-il avec la bêta.
pub fn beta_active() -> bool {
    BETA_ACTIVE.load(Ordering::Relaxed)
}

/// Une bêta se calcule-t-elle dans cette version : `beta.json` porte des lignes,
/// et elles se rejouent sur la donnée compilée.
pub fn beta_disponible() -> bool {
    corriges().as_ref().is_ok_and(|c| !c.is_empty())
}

/// Passe toute l'application à la donnée de la bêta, ou l'en ramène.
pub fn regler_la_beta(active: bool) -> Result<bool, String> {
    if active && !beta_disponible() {
        return Err(match corriges() {
            Err(e) => format!("La bêta ne se calcule pas dans cette version de Krozties : {e}"),
            Ok(_) => "Aucune bêta à calculer dans cette version de Krozties.".to_string(),
        });
    }
    BETA_ACTIVE.store(active, Ordering::Relaxed);
    Ok(active)
}

/// Un fichier tel que la bêta le change, en mode bêta seulement.
fn corrige(chemin: &str) -> Option<&'static str> {
    if !beta_active() {
        return None;
    }
    corriges().as_ref().ok()?.get(chemin).copied()
}

/// Les fichiers que la bêta change, par leur chemin dans `data`, ses lignes
/// rejouées sur la donnée compilée, une fois.
fn corriges() -> &'static Result<HashMap<String, &'static str>, String> {
    static CORRIGES: OnceLock<Result<HashMap<String, &'static str>, String>> = OnceLock::new();
    CORRIGES.get_or_init(|| {
        let d: serde_json::Value = serde_json::from_str(BETA).map_err(|e| e.to_string())?;
        let mut corriges = HashMap::new();
        for (chemin, lignes) in d["calcul"]["fichiers"].as_object().into_iter().flatten() {
            let origine = compile(chemin).ok_or_else(|| format!("{chemin} n'est pas dans la donnée"))?;
            let texte = rejouer(origine, lignes).map_err(|e| format!("{chemin}, {e}"))?;
            corriges.insert(chemin.clone(), &*Box::leak(texte.into_boxed_str()));
        }
        Ok(corriges)
    })
}

/// Un fichier compilé, par son chemin dans `data`.
fn compile(chemin: &str) -> Option<&'static str> {
    if let Some(nom) = chemin.strip_prefix("rulesets/").and_then(|n| n.strip_suffix(".yaml")) {
        return regles_compilees(nom);
    }
    instantane_compile(chemin.strip_prefix("snapshots/breed-")?.strip_suffix(".json")?.parse().ok()?)
}

/// Les lignes `[rang, avant, après]` d'un fichier de la bêta rejouées sur son
/// texte. Un avant qui ne s'y trouve pas dit une bêta préparée sur une autre
/// donnée : rien ne se rejoue.
pub(crate) fn rejouer(origine: &str, lignes: &serde_json::Value) -> Result<String, String> {
    let mut texte: Vec<&str> = origine.split('\n').collect();
    for l in lignes.as_array().into_iter().flatten() {
        let (Some(rang), Some(avant), Some(apres)) = (l[0].as_u64(), l[1].as_str(), l[2].as_str()) else {
            return Err(format!("ligne illisible : {l}"));
        };
        let rang = usize::try_from(rang).map_err(|e| e.to_string())?;
        if texte.get(rang) != Some(&avant) {
            return Err(format!("ligne {} : la bêta a été préparée sur une autre donnée", rang + 1));
        }
        texte[rang] = apres;
    }
    Ok(texte.join("\n"))
}

/// L'instantané compilé d'une classe, par son identifiant de race.
pub(crate) fn instantane_compile(breed: u32) -> Option<&'static str> {
    Some(match breed {
        1 => include_str!("../../../data/snapshots/breed-1.json"),
        2 => include_str!("../../../data/snapshots/breed-2.json"),
        3 => include_str!("../../../data/snapshots/breed-3.json"),
        4 => include_str!("../../../data/snapshots/breed-4.json"),
        5 => include_str!("../../../data/snapshots/breed-5.json"),
        6 => include_str!("../../../data/snapshots/breed-6.json"),
        7 => include_str!("../../../data/snapshots/breed-7.json"),
        8 => include_str!("../../../data/snapshots/breed-8.json"),
        9 => include_str!("../../../data/snapshots/breed-9.json"),
        10 => include_str!("../../../data/snapshots/breed-10.json"),
        11 => include_str!("../../../data/snapshots/breed-11.json"),
        12 => include_str!("../../../data/snapshots/breed-12.json"),
        13 => include_str!("../../../data/snapshots/breed-13.json"),
        14 => include_str!("../../../data/snapshots/breed-14.json"),
        15 => include_str!("../../../data/snapshots/breed-15.json"),
        16 => include_str!("../../../data/snapshots/breed-16.json"),
        17 => include_str!("../../../data/snapshots/breed-17.json"),
        18 => include_str!("../../../data/snapshots/breed-18.json"),
        20 => include_str!("../../../data/snapshots/breed-20.json"),
        _ => return None,
    })
}

/// Les règles compilées d'une classe, par le nom de leur fichier.
pub(crate) fn regles_compilees(nom: &str) -> Option<&'static str> {
    Some(match nom {
        "feca" => include_str!("../../../data/rulesets/feca.yaml"),
        "osamodas" => include_str!("../../../data/rulesets/osamodas.yaml"),
        "enutrof" => include_str!("../../../data/rulesets/enutrof.yaml"),
        "sram" => include_str!("../../../data/rulesets/sram.yaml"),
        "xelor" => include_str!("../../../data/rulesets/xelor.yaml"),
        "ecaflip" => include_str!("../../../data/rulesets/ecaflip.yaml"),
        "eniripsa" => include_str!("../../../data/rulesets/eniripsa.yaml"),
        "iop" => include_str!("../../../data/rulesets/iop.yaml"),
        "cra" => include_str!("../../../data/rulesets/cra.yaml"),
        "sadida" => include_str!("../../../data/rulesets/sadida.yaml"),
        "sacrieur" => include_str!("../../../data/rulesets/sacrieur.yaml"),
        "pandawa" => include_str!("../../../data/rulesets/pandawa.yaml"),
        "roublard" => include_str!("../../../data/rulesets/roublard.yaml"),
        "zobal" => include_str!("../../../data/rulesets/zobal.yaml"),
        "steamer" => include_str!("../../../data/rulesets/steamer.yaml"),
        "eliotrope" => include_str!("../../../data/rulesets/eliotrope.yaml"),
        "huppermage" => include_str!("../../../data/rulesets/huppermage.yaml"),
        "ouginak" => include_str!("../../../data/rulesets/ouginak.yaml"),
        "forgelance" => include_str!("../../../data/rulesets/forgelance.yaml"),
        _ => return None,
    })
}

/// Les objets portés qu'une note de sortie change sans en donner les chiffres
/// (`objets_a_relever` du manifeste) : ils gardent les chiffres du relevé, et
/// une remarque le dit.
pub fn objets_a_relever(portes: &[u32]) -> Option<String> {
    objets_a_relever_selon(VERSION, portes)
}

fn objets_a_relever_selon(manifeste: &str, portes: &[u32]) -> Option<String> {
    let manifeste: serde_json::Value = serde_json::from_str(manifeste).ok()?;
    let notes = manifeste["patch_notes"].as_array()?;
    let mut noms: Vec<String> = Vec::new();
    for note in notes {
        let Some(objets) = note["objets_a_relever"].as_object() else { continue };
        for id in portes.iter().filter(|i| **i != 0) {
            if let Some(nom) = objets.get(&id.to_string()).and_then(|n| n.as_str()) {
                if !noms.iter().any(|n| n == nom) {
                    noms.push(nom.to_string());
                }
            }
        }
    }
    if noms.is_empty() {
        return None;
    }
    let version = manifeste["version_du_jeu"].as_str().unwrap_or_default();
    Some(format!(
        "{} : chiffres de la 3.6.12, la note de la {version} les change sans les donner",
        noms.join(", ")
    ))
}

/// La version du jeu décrite par la donnée embarquée : celle de la dernière
/// note de sortie appliquée.
pub fn version_du_jeu() -> String {
    serde_json::from_str::<serde_json::Value>(VERSION)
        .ok()
        .and_then(|v| v["version_du_jeu"].as_str().map(str::to_string))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La bêta embarquée se rejoue sur la donnée compilée : préparée sur une
    /// autre donnée, elle le dirait ici plutôt que chez le joueur.
    #[test]
    fn la_beta_embarquee_se_rejoue() {
        assert!(corriges().is_ok(), "{:?}", corriges().as_ref().err());
    }

    /// Une ligne de la bêta se rejoue là où se trouve son avant, et nulle part
    /// ailleurs.
    #[test]
    fn une_ligne_de_la_beta_exige_son_avant() {
        let lignes = serde_json::json!([[1, "b", "B"]]);
        assert_eq!(rejouer("a\nb\nc", &lignes).unwrap(), "a\nB\nc");
        assert!(rejouer("a\nx\nc", &lignes).is_err());
    }

    /// Chaque classe proposée a sa donnée embarquée, et chaque fichier de donnée
    /// est embarqué.
    #[test]
    fn toute_la_donnee_est_embarquee() {
        for (id, nom, _) in crate::solve::CLASSES {
            assert!(instantane(*id).is_some(), "instantané de {nom}");
            assert!(regles(nom).is_some(), "règles de {nom}");
        }
        let racine = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data");
        let fichiers = |dossier: &str| -> Vec<String> {
            std::fs::read_dir(format!("{racine}/{dossier}"))
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect()
        };
        for f in fichiers("rulesets") {
            let nom = f.strip_suffix(".yaml").expect(&f);
            let sur_le_disque = std::fs::read_to_string(format!("{racine}/rulesets/{f}")).unwrap();
            assert_eq!(regles(nom), Some(sur_le_disque.as_str()), "{f}");
        }
        for f in fichiers("snapshots").iter().filter(|f| f.starts_with("breed-")) {
            let id: u32 = f.trim_start_matches("breed-").trim_end_matches(".json").parse().unwrap();
            assert!(instantane(id).is_some(), "{f} n'est pas embarqué");
        }
    }

    /// Un objet qu'une note change sans le chiffrer porte une remarque ; un objet
    /// que la note ne touche pas, non.
    #[test]
    fn un_objet_change_par_la_note_le_dit() {
        let manifeste = r#"{"version_du_jeu": "3.8", "patch_notes": [
            {"id": "1", "objets_a_relever": {"22368": "Hachebarde de Guerre"}}]}"#;
        let r = objets_a_relever_selon(manifeste, &[22368, 0, 7754]).expect("la Hachebarde");
        assert_eq!(r, "Hachebarde de Guerre : chiffres de la 3.6.12, la note de la 3.8 les change sans les donner");
        assert_eq!(objets_a_relever_selon(manifeste, &[7754]), None);
        assert_eq!(objets_a_relever(&[22368]), None, "la 3.7 ne laisse aucun objet en attente");
    }

    /// Les instantanés disent la version du relevé ; le manifeste, celle de la
    /// dernière note de sortie appliquée.
    #[test]
    fn la_donnee_dit_sa_version() {
        const RELEVE: &str = "3.6.12";
        for (id, nom, _) in crate::solve::CLASSES {
            let v: serde_json::Value = serde_json::from_str(instantane(*id).unwrap()).unwrap();
            assert_eq!(v["game_version"], RELEVE, "instantané de {nom}");
            assert!(regles(nom).unwrap().contains(&format!("game_version: \"{RELEVE}\"")), "règles de {nom}");
        }
        let objets: serde_json::Value = serde_json::from_str(OBJETS).unwrap();
        assert_eq!(objets["game_version"], RELEVE);
        let manifeste: serde_json::Value = serde_json::from_str(VERSION).unwrap();
        let attendue = manifeste["patch_notes"]
            .as_array()
            .and_then(|n| n.last())
            .and_then(|n| n["titre"].as_str())
            .and_then(|t| t.split_whitespace().find(|m| m.chars().next().is_some_and(|c| c.is_ascii_digit())))
            .unwrap_or(RELEVE)
            .to_string();
        assert_eq!(version_du_jeu(), attendue);
    }
}

/// Le nom de chaque état, par son identifiant : « {{spell,24278,1::Infecté}} »
/// pour un état qui renvoie à son explication.
pub const ETATS: &str = include_str!("../../../data/snapshots/states.json");

/// Les effets de chaque sous-sort, sans leurs valeurs, par son identifiant.
pub const SOUS_SORTS: &str = include_str!("../../../data/snapshots/subspells.json");
