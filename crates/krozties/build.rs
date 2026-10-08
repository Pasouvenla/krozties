//! Les icônes sont copiées dans le dossier servi, où le serveur du CLI et
//! l'application de bureau (sans serveur, `tauri://localhost`) les trouvent à la
//! même adresse relative. La copie est ignorée par Git : la source reste
//! `data/icons`.

use std::path::Path;

fn main() {
    copier(
        "data/icons",
        "ui/icons",
        "les icônes de sorts",
        "data/icons introuvable : les icônes de sorts manqueront",
    );
    // Les icônes d'objets sont réduites à 64 pixels à l'import, pas ici.
    copier(
        "data/item-icons",
        "ui/objets",
        "les icônes d'objets",
        "data/item-icons introuvable : la fiche de build sortira sans images.",
    );
    tauri_build::build();
}

fn copier(depuis: &str, vers: &str, quoi: &str, plainte: &str) {
    let racine = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = racine.join(depuis);
    let cible = Path::new(env!("CARGO_MANIFEST_DIR")).join(vers);
    if !source.is_dir() {
        println!("cargo:warning={plainte}");
        return;
    }
    let _ = std::fs::create_dir_all(&cible);
    let mut copiees = 0usize;
    let Ok(entrees) = std::fs::read_dir(&source) else { return };
    for entree in entrees.flatten() {
        let nom = entree.file_name();
        let destination = cible.join(&nom);
        // Ne recopier que ce qui manque : `build.rs` tourne à chaque
        // compilation, et recopier huit cents fichiers à chaque fois pour rien
        // rallongerait toutes les constructions.
        let a_jour = std::fs::metadata(&destination)
            .and_then(|d| Ok(d.len()))
            .ok()
            .zip(entree.metadata().ok().map(|m| m.len()))
            .is_some_and(|(a, b)| a == b);
        if a_jour {
            continue;
        }
        if std::fs::copy(entree.path(), &destination).is_ok() {
            copiees += 1;
        }
    }
    if copiees > 0 {
        println!("cargo:warning={copiees} fichiers copiés pour {quoi}");
    }
    println!("cargo:rerun-if-changed=../../{depuis}");
}
