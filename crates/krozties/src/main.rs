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

/// Ce que la rotation ne compte pas et qu'un joueur peut relever en jeu, pour
/// la page « Données manquantes ».
#[tauri::command]
fn donnees_manquantes() -> Sortie {
    dofus_app::manquantes::manquantes_json()
}

#[tauri::command]
fn ligne_de_vue(requete: cartes::RequeteVue) -> Sortie {
    cartes::vue_json(&requete)
}

/// La page des versions publiées. Fixe : la page ne peut rien faire ouvrir
/// d'autre.
const PAGE_DES_VERSIONS: &str = "https://github.com/Pasouvenla/krozties/releases/latest";

/// La version de l'application, celle du `Cargo.toml` de l'espace de travail.
#[tauri::command]
fn version_de_l_application() -> Sortie {
    serde_json::to_string(env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())
}

/// La page des versions, dans le navigateur du système.
#[tauri::command]
fn ouvrir_les_versions() -> Sortie {
    ouvrir(PAGE_DES_VERSIONS)
}

/// La mise à jour trouvée, ce qu'elle a reçu et, sur Windows, le paquet qui
/// attend son installation.
#[derive(Default)]
struct MiseAJour {
    trouvee: std::sync::Mutex<Option<tauri_plugin_updater::Update>>,
    en_attente: std::sync::Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>>,
    recu: std::sync::atomic::AtomicU64,
    total: std::sync::atomic::AtomicU64,
}

/// La version que propose `latest.json` de la dernière version publiée ;
/// `null` s'il n'y a rien de plus récent.
#[tauri::command]
async fn chercher_la_mise_a_jour(app: tauri::AppHandle, etat: tauri::State<'_, MiseAJour>) -> Sortie {
    use tauri_plugin_updater::UpdaterExt;
    let trouvee = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    let version = trouvee.as_ref().map(|m| m.version.clone());
    *etat.trouvee.lock().map_err(|e| e.to_string())? = trouvee;
    serde_json::to_string(&version).map_err(|e| e.to_string())
}

/// Télécharge la mise à jour trouvée et vérifie sa signature. Rend `true` si
/// elle est installée, la nouvelle version s'ouvrant au prochain lancement ;
/// `false` sur Windows, où l'installateur ferme l'application : le paquet
/// attend que le joueur redémarre ou quitte.
#[tauri::command]
async fn installer_la_mise_a_jour(etat: tauri::State<'_, MiseAJour>) -> Sortie {
    use std::sync::atomic::Ordering;
    let maj = etat.trouvee.lock().map_err(|e| e.to_string())?.clone().ok_or("aucune mise à jour à installer")?;
    etat.recu.store(0, Ordering::Relaxed);
    etat.total.store(0, Ordering::Relaxed);
    let paquet = maj
        .download(
            |morceau, total| {
                etat.recu.fetch_add(morceau as u64, Ordering::Relaxed);
                if let Some(t) = total {
                    etat.total.store(t, Ordering::Relaxed);
                }
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    if cfg!(windows) {
        *etat.en_attente.lock().map_err(|e| e.to_string())? = Some((maj, paquet));
        return Ok("false".into());
    }
    maj.install(paquet).map_err(|e| e.to_string())?;
    Ok("true".into())
}

/// Ce que la mise à jour a reçu, en octets, sur son total.
#[tauri::command]
fn avancement_de_la_mise_a_jour(etat: tauri::State<'_, MiseAJour>) -> Sortie {
    use std::sync::atomic::Ordering;
    serde_json::to_string(&serde_json::json!({
        "recu": etat.recu.load(Ordering::Relaxed),
        "total": etat.total.load(Ordering::Relaxed),
    }))
    .map_err(|e| e.to_string())
}

/// Relance l'application, sur la version installée. Sur Windows, l'installateur
/// ferme Krozties et le rouvre une fois la mise à jour posée.
#[tauri::command]
fn redemarrer_l_application(app: tauri::AppHandle, etat: tauri::State<'_, MiseAJour>) -> Sortie {
    let en_attente = etat.en_attente.lock().map_err(|e| e.to_string())?.take();
    if let Some((maj, paquet)) = en_attente {
        maj.install(paquet).map_err(|e| e.to_string())?;
    }
    app.restart()
}

/// Le formulaire d'issue du dépôt, que la page Retours remplit par l'adresse.
const FORMULAIRE_DE_RETOUR: &str = "https://github.com/Pasouvenla/krozties/issues/new?";

/// Le formulaire d'issue rempli par la page Retours, dans le navigateur du
/// système. Seule cette adresse s'ouvre, en caractères qu'`encodeURIComponent`
/// produit : la page ne fait rien ouvrir d'autre.
#[tauri::command]
fn ouvrir_un_retour(adresse: String) -> Sortie {
    if !adresse_de_retour(&adresse) {
        return Err("adresse de retour refusée".into());
    }
    ouvrir(&adresse)
}

fn adresse_de_retour(adresse: &str) -> bool {
    adresse
        .strip_prefix(FORMULAIRE_DE_RETOUR)
        .is_some_and(|reste| reste.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.!~*'()%&=".contains(&b)))
}

/// La liste des personnages, dans le fichier que le joueur choisit : une copie
/// à garder, ou à importer sur un autre ordinateur. Rend son chemin, ou `null`
/// s'il renonce. La fenêtre d'enregistrement attend le joueur : elle s'ouvre
/// hors du fil de l'interface.
#[tauri::command]
async fn exporter_les_personnages(app: tauri::AppHandle, contenu: String) -> Sortie {
    use tauri_plugin_dialog::DialogExt;
    if contenu.len() > 20_000_000 {
        return Err("liste trop lourde pour être exportée".into());
    }
    serde_json::from_str::<serde_json::Value>(&contenu).map_err(|_| "liste illisible".to_string())?;
    let choisi = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Exporter la liste des personnages")
            .set_file_name("krozties-personnages.json")
            .add_filter("Liste de personnages Krozties", &["json"])
            .blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(choisi) = choisi else {
        return Ok("null".into());
    };
    let chemin = choisi.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&chemin, contenu).map_err(|e| format!("impossible d'écrire {} : {e}", chemin.display()))?;
    serde_json::to_string(&chemin.display().to_string()).map_err(|e| e.to_string())
}

/// Une adresse dans le navigateur du système.
fn ouvrir(adresse: &str) -> Sortie {
    let ouvreur = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };
    let mut enfant = std::process::Command::new(ouvreur)
        .arg(adresse)
        .spawn()
        .map_err(|e| format!("impossible d'ouvrir {adresse} : {e}"))?;
    // L'ouvreur est attendu à part, pour ne pas laisser de processus zombie.
    std::thread::spawn(move || {
        let _ = enfant.wait();
    });
    Ok("null".into())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // La taille, la place, l'écran et l'état agrandi de la fenêtre, retrouvés
        // à l'ouverture suivante.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(MiseAJour::default())
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
            donnees_manquantes,
            ligne_de_vue,
            importer_equipement,
            version_de_l_application,
            ouvrir_les_versions,
            ouvrir_un_retour,
            exporter_les_personnages,
            chercher_la_mise_a_jour,
            installer_la_mise_a_jour,
            avancement_de_la_mise_a_jour,
            redemarrer_l_application
        ])
        .build(tauri::generate_context!())
        .expect("Krozties n'a pas pu démarrer")
        .run(|app, evenement| {
            // Une mise à jour remise à plus tard sur Windows s'installe à la
            // fermeture, sans rouvrir l'application.
            if let tauri::RunEvent::Exit = evenement {
                use tauri::Manager;
                let en_attente = app.state::<MiseAJour>().en_attente.lock().ok().and_then(|mut e| e.take());
                if let Some((maj, paquet)) = en_attente {
                    let _ = maj.restart_after_install(false).install(paquet);
                }
            }
        });
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

    /// La version que l'application annonce est celle de son paquet : la mini
    /// fenêtre de mise à jour compare la dernière version publiée à celle-ci.
    #[test]
    fn la_version_est_celle_du_paquet() {
        assert_eq!(json(&version_de_l_application().expect("version")), env!("CARGO_PKG_VERSION"));
        let configuration = json(include_str!("../tauri.conf.json"));
        assert_eq!(configuration["version"], env!("CARGO_PKG_VERSION"), "tauri.conf.json et Cargo.toml");
    }

    /// Le manifeste déclare exactement les commandes que le code expose, et le pont
    /// La page Retours ne fait ouvrir que le formulaire d'issue du dépôt, rempli
    /// en caractères d'adresse : ni une autre page, ni un autre dépôt, ni une
    /// adresse qui casserait la ligne de commande de l'ouvreur.
    #[test]
    fn seul_le_formulaire_de_retour_s_ouvre() {
        let bonne = format!("{FORMULAIRE_DE_RETOUR}title=Calcul%20faux&body=La%20Gonflable%20(rang%201)");
        assert!(adresse_de_retour(&bonne));
        for mauvaise in [
            "https://github.com/Pasouvenla/krozties/releases/latest",
            "https://github.com/autre/depot/issues/new?title=x",
            "https://exemple.fr/?https://github.com/Pasouvenla/krozties/issues/new?",
            "https://github.com/Pasouvenla/krozties/issues/new?title=x y",
            "https://github.com/Pasouvenla/krozties/issues/new?title=x\"&calc",
        ] {
            assert!(!adresse_de_retour(mauvaise), "{mauvaise}");
        }
    }

    /// côté page nomme les mêmes : une commande oubliée dans `generate_handler!`
    /// n'échoue qu'à l'exécution.
    #[test]
    fn les_notes_de_version_suivent_la_version_de_l_application() {
        let notes: serde_json::Value =
            serde_json::from_str(include_str!("../ui/notes-de-version.json")).expect("notes lisibles");
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("configuration lisible");
        let derniere = notes["versions"]
            .as_array()
            .and_then(|v| v.iter().find(|e| e["version"] != "prochaine"))
            .and_then(|e| e["version"].as_str());
        assert_eq!(derniere, conf["version"].as_str(), "la version publiée doit avoir ses notes");
    }

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
