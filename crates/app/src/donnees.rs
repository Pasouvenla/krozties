//! La donnée du jeu, compilée dans l'application : l'application installée n'a
//! pas accès au dossier du code, et chaque version emporte sa donnée.

/// La version du jeu décrite par la donnée, et les notes de sortie appliquées.
pub const VERSION: &str = include_str!("../../../data/version.json");

/// Les changements de sorts annoncés par la bêta en cours, pour l'onglet
/// « Bêta en cours ». Rien n'entre dans le calcul.
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

/// L'instantané des sorts d'une classe, par son identifiant de race.
pub fn instantane(breed: u32) -> Option<&'static str> {
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

/// Les règles d'une classe, par le nom de leur fichier.
pub fn regles(nom: &str) -> Option<&'static str> {
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
