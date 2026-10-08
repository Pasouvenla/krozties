//! Krozties : l'application de bureau. Ce fichier ne calcule rien : il expose à
//! la fenêtre ce que `dofus-app` sait faire, comme le serveur du CLI l'expose en
//! HTTP.

// Sur Windows, ne pas ouvrir de console derrière la fenêtre en release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod import;

use dofus_app::{bombes, cartes, grille, invocations, portails, reseau, solve};
use dofus_build::BuildInput;

/// Les commandes rendent le JSON déjà sérialisé, les mêmes chaînes que le
/// serveur HTTP : la page est la même des deux côtés.
type Sortie = Result<String, String>;

#[tauri::command]
fn classes() -> Sortie {
    solve::classes_json()
}

#[tauri::command]
fn resoudre_build(build: BuildInput) -> Sortie {
    solve::resolve_json(&build)
}

/// La seule commande qui sorte de la machine, vers DofusBook, sur l'identifiant
/// que l'utilisateur vient de coller.
#[tauri::command]
async fn importer_equipement(app: tauri::AppHandle, id: u64) -> Sortie {
    import::importer(app, id).await
}

/// Le calcul ne tourne pas sur le fil principal, qui dessine la fenêtre :
/// `spawn_blocking` le porte ailleurs, et la fenêtre et la barre d'avancement
/// restent vivantes. Les autres commandes, sous le dixième de seconde, restent
/// synchrones.
#[tauri::command]
async fn rotation(requete: solve::Request) -> Sortie {
    tauri::async_runtime::spawn_blocking(move || solve::solve_json(&requete))
        .await
        .map_err(|e| format!("le calcul n'a pas pu aller au bout : {e}"))?
}

/// L'arrêt du calcul en cours (bouton « Annuler ») : un simple drapeau, servi
/// pendant que la rotation occupe son fil.
#[tauri::command]
fn annuler() -> Sortie {
    solve::demander_arret();
    Ok("{\"ok\":true}".to_string())
}

/// L'avancement du calcul en cours, servi pendant que la résolution occupe son
/// fil.
#[tauri::command]
fn avancement() -> Sortie {
    Ok(solve::avancement_json())
}

#[tauri::command]
fn zones(requete: solve::Request) -> Sortie {
    grille::zones_json(&requete)
}

#[tauri::command]
fn reseau_de_pieges(requete: reseau::RequeteReseau) -> Sortie {
    reseau::reseau_json(&requete)
}

#[tauri::command]
fn portails_eliotrope(requete: portails::RequetePortails) -> Sortie {
    portails::portails_json(&requete)
}

#[tauri::command]
fn bombes_du_roublard(requete: bombes::RequeteBombes) -> Sortie {
    bombes::bombes_json(&requete)
}

#[tauri::command]
fn invocations_du_build(build: dofus_build::BuildInput) -> Sortie {
    invocations::invocations_json(&build)
}

#[tauri::command]
fn conseil_par_tour(demande: solve::DemandeDeConseil) -> Sortie {
    solve::conseil_json(&demande)
}

#[tauri::command]
fn cartes_de_boss() -> Sortie {
    Ok(cartes::cartes_json())
}

/// Les changements de sorts que la bêta en cours annonce, pour l'onglet
/// « Bêta en cours » : embarqués, comme toute la donnée.
#[tauri::command]
fn beta_en_cours() -> Sortie {
    Ok(dofus_app::notes::beta::beta_json())
}

#[tauri::command]
fn ligne_de_vue(requete: cartes::RequeteVue) -> Sortie {
    cartes::vue_json(&requete)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            classes,
            resoudre_build,
            rotation,
            avancement,
            annuler,
            zones,
            reseau_de_pieges,
            portails_eliotrope,
            bombes_du_roublard,
            invocations_du_build,
            conseil_par_tour,
            cartes_de_boss,
            beta_en_cours,
            ligne_de_vue,
            importer_equipement
        ])
        .run(tauri::generate_context!())
        .expect("Krozties n'a pas pu démarrer");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les commandes sont des fonctions ordinaires, testables sans fenêtre : ces
    /// tests vérifient leur câblage vers `dofus-app`. `importer_equipement`, qui
    /// exige une fenêtre et le réseau, n'est vérifiée que dans le manifeste et le
    /// pont.
    fn build_d_essai() -> BuildInput {
        serde_json::from_value(serde_json::json!({
            "class": 8,
            "level": 200,
            "invested": { "strength": 400 },
        }))
        .expect("le build d'essai doit se lire")
    }

    fn requete_d_essai(deck: &[&str]) -> solve::Request {
        serde_json::from_value(serde_json::json!({
            "class": 8,
            "level": 200,
            "invested": { "strength": 400 },
            "deck": deck,
            "horizon": 2,
        }))
        .expect("la requête d'essai doit se lire")
    }

    fn json(s: &str) -> serde_json::Value {
        serde_json::from_str(s).expect("une commande doit rendre du JSON valide")
    }

    #[test]
    fn les_commandes_de_calcul_repondent_et_rendent_du_json() {
        let c = json(&classes().expect("classes"));
        assert!(c["classes"].as_array().is_some_and(|a| a.len() >= 15));

        let r = json(&resoudre_build(build_d_essai()).expect("résolution"));
        assert_eq!(r["class_name"], "Iop");

        // Ce `block_on` garde `rotation` asynchrone : synchrone, elle tournerait sur le
        // fil principal, et ce test ne compilerait plus.
        let rot = json(
            &tauri::async_runtime::block_on(rotation(requete_d_essai(&["epee_celeste"])))
                .expect("rotation"),
        );
        assert!(rot["rotation"]["total"].as_f64().is_some_and(|t| t > 0.0));

        // L'aperçu de zone exige un placement : sans ennemis posés, il n'a
        // rien à mesurer et le dit plutôt que de rendre une liste vide.
        let mut avec_placement = requete_d_essai(&["epee_celeste"]);
        avec_placement.placement = Some(
            serde_json::from_value(serde_json::json!({
                "lanceur": [-4, 0], "visee": [0, 0], "ennemis": [[0, 0], [0, 1]],
            }))
            .unwrap(),
        );
        let z = json(&zones(avec_placement).expect("zones"));
        assert!(z["sorts"].as_array().is_some_and(|a| !a.is_empty()));
        assert!(!z["sorts"][0]["touches"].as_array().unwrap().is_empty());

        let res = json(
            &reseau_de_pieges(
                serde_json::from_value(serde_json::json!({
                    "class": 4,
                    "level": 200,
                    "invested": { "agility": 400 },
                    "poses": [],
                }))
                .unwrap(),
            )
            .expect("réseau"),
        );
        assert!(res["catalogue"].as_array().is_some_and(|a| a.len() >= 10));

        let port = json(
            &portails_eliotrope(
                serde_json::from_value(serde_json::json!({
                    "portails": [[0, 0], [3, 0], [2, 4]],
                    "entree": [0, 0],
                }))
                .unwrap(),
            )
            .expect("portails"),
        );
        assert_eq!(port["bonus"], 16);
    }

    /// Le manifeste déclare exactement les commandes que le code expose, et le pont
    /// côté page nomme les mêmes : une commande oubliée dans `generate_handler!`
    /// n'échoue qu'à l'exécution.
    #[test]
    fn le_pont_et_le_manifeste_nomment_les_memes_commandes() {
        let source = include_str!("main.rs");
        let debut = source.find("generate_handler![").expect("le manifeste");
        let fin = source[debut..].find(']').expect("fin du manifeste") + debut;
        let declarees: std::collections::BTreeSet<String> = source[debut + 18..fin]
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let pont = include_str!("../ui/pont.js");
        let attendues: std::collections::BTreeSet<String> = pont
            .lines()
            .filter_map(|l| l.split("commande: '").nth(1))
            .filter_map(|l| l.split('\'').next())
            .map(str::to_string)
            .collect();

        assert_eq!(
            declarees, attendues,
            "le manifeste Tauri et le pont de la page doivent nommer les mêmes commandes"
        );
    }
}
