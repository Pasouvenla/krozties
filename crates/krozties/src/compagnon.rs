//! Le compagnon de combat : une petite fenêtre sans bordure, toujours au premier
//! plan, par-dessus le jeu, qui déroule la rotation tour par tour. Le jeu garde
//! le clavier : F8 avance d'un sort, Maj+F8 d'un tour, F7 recule, en raccourcis
//! globaux pris seulement tant que le compagnon est ouvert.

use tauri::{AppHandle, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub const ETIQUETTE: &str = "compagnon";

/// Les raccourcis, et le geste que chacun transmet à la page du compagnon.
fn raccourcis() -> [(Shortcut, &'static str); 3] {
    [
        (Shortcut::new(None, Code::F8), "suivant"),
        (Shortcut::new(Some(Modifiers::SHIFT), Code::F8), "tour"),
        (Shortcut::new(None, Code::F7), "precedent"),
    ]
}

/// Le greffon des raccourcis globaux : une touche pressée passe au compagnon.
pub fn greffon() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, raccourci, evenement| {
            if evenement.state() != ShortcutState::Pressed {
                return;
            }
            let Some((_, geste)) = raccourcis().into_iter().find(|(r, _)| r == raccourci) else {
                return;
            };
            if let Some(fenetre) = app.get_webview_window(ETIQUETTE) {
                let _ = fenetre.eval(format!("window.compagnonTouche && window.compagnonTouche('{geste}')"));
            }
        })
        .build()
}

/// Ouvre le compagnon, ou le ramène devant s'il l'est déjà.
pub async fn ouvrir(app: AppHandle) -> Result<String, String> {
    if let Some(fenetre) = app.get_webview_window(ETIQUETTE) {
        let _ = fenetre.show();
        let _ = fenetre.set_focus();
        return Ok("true".into());
    }
    WebviewWindowBuilder::new(&app, ETIQUETTE, WebviewUrl::App("compagnon.html".into()))
        .title("Krozties, compagnon de combat")
        .inner_size(420.0, 64.0)
        .min_inner_size(160.0, 56.0)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .build()
        .map_err(|e| format!("le compagnon n'a pas pu s'ouvrir : {e}"))?;
    for (raccourci, _) in raccourcis() {
        // Une touche qu'une autre application tient déjà : le compagnon reste
        // utilisable à la souris.
        let _ = app.global_shortcut().register(raccourci);
    }
    Ok("true".into())
}

/// Rend les touches aux autres applications.
pub fn liberer(app: &AppHandle) {
    for (raccourci, _) in raccourcis() {
        let _ = app.global_shortcut().unregister(raccourci);
    }
}

pub fn fermer(app: &AppHandle) {
    if let Some(fenetre) = app.get_webview_window(ETIQUETTE) {
        let _ = fenetre.close();
    }
    liberer(app);
}

/// La taille que demande la disposition choisie, en points.
pub fn ajuster(fenetre: &tauri::Window, largeur: f64, hauteur: f64) -> Result<String, String> {
    fenetre
        .set_size(LogicalSize::new(largeur.clamp(160.0, 900.0), hauteur.clamp(56.0, 700.0)))
        .map_err(|e| e.to_string())?;
    Ok("null".into())
}
