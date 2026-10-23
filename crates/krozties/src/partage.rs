//! Une rotation partagée : un fichier `.krozties`, enregistré depuis la
//! Rotation, puis reçu par un double-clic, un dépôt sur la fenêtre ou au
//! lancement. La page le lit, et propose de l'ouvrir.

use std::path::Path;
use std::sync::Mutex;

use tauri::{AppHandle, Manager};

/// Au-delà, un fichier n'est pas une rotation.
const POIDS_MAX: u64 = 1_000_000;

/// La rotation reçue que la page n'a pas encore lue : son texte, ou ce qui
/// empêche de la lire.
#[derive(Default)]
pub struct RotationRecue(Mutex<Option<String>>);

/// Garde la rotation d'un fichier `.krozties`, puis prévient la page.
pub fn recevoir(app: &AppHandle, chemin: &Path) {
    if chemin.extension().and_then(|e| e.to_str()) != Some("krozties") {
        return;
    }
    let recue = match std::fs::metadata(chemin) {
        Ok(m) if m.len() > POIDS_MAX => serde_json::json!({ "erreur": "Ce fichier est trop lourd pour une rotation Krozties." }),
        _ => match std::fs::read_to_string(chemin) {
            Ok(texte) => serde_json::json!({ "texte": texte }),
            Err(e) => serde_json::json!({ "erreur": format!("Ce fichier ne se lit pas : {e}.") }),
        },
    };
    if let Ok(mut gardee) = app.state::<RotationRecue>().0.lock() {
        *gardee = Some(recue.to_string());
    }
    if let Some(fenetre) = app.get_webview_window("main") {
        let _ = fenetre.eval("window.rotationRecue && window.rotationRecue()");
    }
}

/// Sous Windows et Linux, un fichier ouvert d'un double-clic arrive en
/// argument du lancement ; sous macOS, par `RunEvent::Opened`.
pub fn au_lancement(app: &AppHandle) {
    for argument in std::env::args().skip(1) {
        recevoir(app, Path::new(&argument));
    }
}

/// La rotation reçue, une seule fois : `null` s'il n'y en a pas.
pub fn prendre(app: &AppHandle) -> Result<String, String> {
    let etat = app.state::<RotationRecue>();
    let recue = etat.0.lock().map_err(|e| e.to_string())?.take();
    Ok(recue.unwrap_or_else(|| "null".into()))
}

/// Enregistre une rotation dans le fichier que le joueur choisit. Rend son
/// chemin, ou `null` s'il renonce. La fenêtre d'enregistrement attend le
/// joueur : elle s'ouvre hors du fil de l'interface.
pub async fn enregistrer(app: AppHandle, nom: String, contenu: String) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;
    if contenu.len() as u64 > POIDS_MAX {
        return Err("rotation trop lourde pour être enregistrée".into());
    }
    serde_json::from_str::<serde_json::Value>(&contenu).map_err(|_| "rotation illisible".to_string())?;
    let choisi = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Enregistrer la rotation")
            .set_file_name(&nom)
            .add_filter("Rotation Krozties", &["krozties"])
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
