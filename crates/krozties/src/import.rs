//! Lire un équipement DofusBook à partir de son lien. Leur API ne répond qu'à un
//! moteur de rendu servi par leur origine : l'application en est un, dans une
//! fenêtre invisible.
//!
//! ⚠️ Aucune API de Tauri n'est donnée à leur page : le script injecté ne peut
//! que naviguer vers une adresse convenue, que l'on intercepte et annule.

use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use tauri::{WebviewUrl, WebviewWindowBuilder};

const SCRIPT: &str = include_str!("importeur.js");

/// L'adresse que le script vise pour rendre sa reponse. Elle n'est jamais
/// atteinte : la navigation est annulee des qu'on l'a lue.
const MARQUEUR: &str = "https://www.dofusbook.net/krozties-import?d=";

/// Trente secondes : une page de DofusBook se charge en deux à trois secondes ;
/// au-delà, quelque chose est cassé.
const DELAI: Duration = Duration::from_secs(30);

/// ⚠️ UNE COMMANDE ASYNCHRONE, ET CE N'EST PAS UN DETAIL. La documentation de
/// Tauri le demande : creer une vue depuis une commande synchrone bloque le fil
/// principal sous Windows.
pub async fn importer(app: tauri::AppHandle, id: u64) -> Result<String, String> {
    let (envoyeur, recepteur) = mpsc::channel::<String>();
    // `on_navigation` exige une fermeture `Sync`, ce qu'un `Sender` n'est pas.
    // Le `Option` sert aussi a n'envoyer qu'une fois.
    let partage = Arc::new(Mutex::new(Some(envoyeur)));
    let pour_navigation = partage.clone();

    let adresse = format!("https://www.dofusbook.net/desktop/fr/equipement/{id}/objets");
    let script = SCRIPT
        .replace("__ID__", &id.to_string())
        .replace("__MARQUEUR__", MARQUEUR);

    // Une etiquette par appel : Tauri refuse deux fenetres du meme nom, et un
    // second import echouerait sans rien dire du pourquoi.
    let etiquette = format!(
        "import-{id}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );

    let fenetre = WebviewWindowBuilder::new(
        &app,
        &etiquette,
        WebviewUrl::External(adresse.parse().map_err(|e| format!("adresse illisible : {e}"))?),
    )
    .visible(false)
    .initialization_script(&script)
    .on_navigation(move |url| {
        if !url.as_str().starts_with(MARQUEUR) {
            return true;
        }
        // La bibliotheque d'URL a deja defait le pourcentage.
        let charge = url
            .query_pairs()
            .find(|(cle, _)| cle == "d")
            .map(|(_, valeur)| valeur.into_owned())
            .unwrap_or_default();
        if let Ok(mut garde) = pour_navigation.lock() {
            if let Some(envoyeur) = garde.take() {
                let _ = envoyeur.send(charge);
            }
        }
        // On annule : leur page ne bouge pas, et l'adresse convenue n'est
        // jamais demandee a leur serveur.
        false
    })
    .build()
    .map_err(|e| format!("la fenêtre de lecture n'a pas pu s'ouvrir : {e}"))?;

    let attente = tauri::async_runtime::spawn_blocking(move || recepteur.recv_timeout(DELAI))
        .await
        .map_err(|e| format!("l'attente a échoué : {e}"))?;

    // Fermer dans tous les cas, succes comme echec : une fenetre invisible
    // laissee ouverte est une fenetre que personne ne peut fermer.
    let _ = fenetre.close();

    let brut = attente.map_err(|_| {
        format!(
            "DofusBook n'a pas répondu en {} secondes. Vérifiez votre connexion, \
             et que l'équipement est public.",
            DELAI.as_secs()
        )
    })?;

    // Le script rend soit `{ erreur }`, soit `{ build, nom }`. On laisse passer
    // le JSON tel quel : la page sait le lire, et le reserialiser ici
    // n'ajouterait qu'une occasion de le deformer.
    let valeur: serde_json::Value =
        serde_json::from_str(&brut).map_err(|e| format!("réponse illisible : {e}"))?;
    if let Some(erreur) = valeur.get("erreur").and_then(|e| e.as_str()) {
        return Err(erreur.to_string());
    }
    Ok(brut)
}
