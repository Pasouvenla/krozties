//! Chaque sort modélisé a sa vignette : la couverture ne mesure que la
//! modélisation, et un sort complet peut s'afficher avec un carré vide.

use std::path::Path;

#[test]
fn every_modelled_spell_has_its_icon() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut manquantes = Vec::new();
    let mut vues = 0usize;

    let dossier = std::fs::read_dir(format!("{root}/data/rulesets"))
        .expect("les rulesets doivent être là");
    for entree in dossier {
        let chemin = entree.unwrap().path();
        if chemin.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let classe = chemin.file_stem().unwrap().to_string_lossy().to_string();
        let texte = std::fs::read_to_string(&chemin).unwrap();
        for ligne in texte.lines() {
            let Some(reste) = ligne.trim().strip_prefix("dofusdb_id:") else {
                continue;
            };
            let Ok(id) = reste.trim().parse::<u32>() else {
                continue;
            };
            vues += 1;
            if !Path::new(&format!("{root}/data/icons/{id}.png")).exists() {
                manquantes.push(format!("{classe}:{id}"));
            }
        }
    }

    assert!(
        vues > 700,
        "seulement {vues} sorts inspectés : le test ne contrôle plus rien"
    );
    assert!(
        manquantes.is_empty(),
        "{} sort(s) modélisé(s) sans vignette dans data/icons.\n{}",
        manquantes.len(),
        manquantes
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );
}
