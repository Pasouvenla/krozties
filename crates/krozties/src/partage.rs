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

/// Au-delà, un lien n'est pas celui d'une rotation.
const LIEN_MAX: usize = 16_000;

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
    garder(app, recue);
}

/// Garde un lien de rotation que le système ouvre (`krozties://r#…`), puis
/// prévient la page.
pub fn recevoir_un_lien(app: &AppHandle, lien: &str) {
    if lien.starts_with("krozties:") && lien.len() <= LIEN_MAX {
        garder(app, serde_json::json!({ "lien": lien }));
    }
}

fn garder(app: &AppHandle, recue: serde_json::Value) {
    if let Ok(mut gardee) = app.state::<RotationRecue>().0.lock() {
        *gardee = Some(recue.to_string());
    }
    if let Some(fenetre) = app.get_webview_window("main") {
        let _ = fenetre.eval("window.rotationRecue && window.rotationRecue()");
    }
}

/// La fenêtre principale au premier plan, quand un fichier ou un lien arrive
/// à Krozties déjà ouvert.
pub fn montrer(app: &AppHandle) {
    if let Some(fenetre) = app.get_webview_window("main") {
        let _ = fenetre.unminimize();
        let _ = fenetre.show();
        let _ = fenetre.set_focus();
    }
}

/// Ce que le lancement apporte : sous Windows et Linux, un fichier ouvert d'un
/// double-clic arrive en argument (sous macOS, par `RunEvent::Opened`) ; un
/// lien, par le greffon des liens, au lancement comme ensuite.
pub fn au_lancement(app: &AppHandle) {
    use tauri_plugin_deep_link::DeepLinkExt;
    for argument in std::env::args().skip(1) {
        recevoir(app, Path::new(&argument));
    }
    let suite = app.clone();
    app.deep_link().on_open_url(move |evenement| {
        for url in evenement.urls() {
            recevoir_un_lien(&suite, url.as_str());
        }
        montrer(&suite);
    });
    if let Ok(Some(urls)) = app.deep_link().get_current() {
        for url in urls {
            recevoir_un_lien(app, url.as_str());
        }
    }
    // Une AppImage n'inscrit pas le protocole à l'installation.
    #[cfg(target_os = "linux")]
    let _ = app.deep_link().register_all();
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
