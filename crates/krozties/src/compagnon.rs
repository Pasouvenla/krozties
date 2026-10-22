//! Le compagnon de combat : une petite fenêtre sans bordure, toujours au premier
//! plan, par-dessus le jeu, qui déroule la rotation tour par tour. Le jeu garde
//! le clavier : trois touches, F8, Maj+F8 et F7 d'origine, avancent d'un sort,
//! d'un tour, ou reculent, en raccourcis globaux pris seulement tant que le
//! compagnon est ouvert.

use std::str::FromStr;
use std::sync::Mutex;

use serde::Deserialize;
use tauri::{AppHandle, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub const ETIQUETTE: &str = "compagnon";

/// Les touches choisies dans la Rotation, une par geste, écrites comme le
/// module des raccourcis les lit (« Shift+F8 »).
#[derive(Debug, Deserialize)]
pub struct Touches {
    pub suivant: String,
    pub tour: String,
    pub precedent: String,
}

impl Touches {
    fn lire(&self) -> Result<Vec<(Shortcut, &'static str)>, String> {
        [(&self.suivant, "suivant"), (&self.tour, "tour"), (&self.precedent, "precedent")]
            .into_iter()
            .map(|(touche, geste)| {
                Shortcut::from_str(touche)
                    .map(|raccourci| (raccourci, geste))
                    .map_err(|_| format!("« {touche} » ne peut pas servir de raccourci."))
            })
            .collect()
    }
}

/// Les raccourcis en vigueur, et le geste que chacun transmet à la page du
/// compagnon.
pub struct Raccourcis(Mutex<Vec<(Shortcut, &'static str)>>);

impl Default for Raccourcis {
    fn default() -> Self {
        Self(Mutex::new(vec![
            (Shortcut::new(None, Code::F8), "suivant"),
            (Shortcut::new(Some(Modifiers::SHIFT), Code::F8), "tour"),
            (Shortcut::new(None, Code::F7), "precedent"),
        ]))
    }
}

fn en_vigueur(app: &AppHandle) -> Vec<(Shortcut, &'static str)> {
    app.state::<Raccourcis>().0.lock().map(|r| r.clone()).unwrap_or_default()
}

/// Prend les touches en vigueur ; rend les gestes dont une autre application
/// tient déjà la touche.
fn prendre(app: &AppHandle) -> Vec<&'static str> {
    en_vigueur(app)
        .into_iter()
        .filter(|(raccourci, _)| app.global_shortcut().register(*raccourci).is_err())
        .map(|(_, geste)| geste)
        .collect()
}

fn rendre(app: &AppHandle) {
    for (raccourci, _) in en_vigueur(app) {
        let _ = app.global_shortcut().unregister(raccourci);
    }
}

/// Le greffon des raccourcis globaux : une touche pressée passe au compagnon.
pub fn greffon() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, raccourci, evenement| {
            if evenement.state() != ShortcutState::Pressed {
                return;
            }
            let Some(geste) = en_vigueur(app).into_iter().find(|(r, _)| r == raccourci).map(|(_, g)| g) else {
                return;
            };
            if let Some(fenetre) = app.get_webview_window(ETIQUETTE) {
                let _ = fenetre.eval(format!("window.compagnonTouche && window.compagnonTouche('{geste}')"));
            }
        })
        .build()
}

/// Règle les touches. Sans touches, celles du compagnon ouvert se taisent le
/// temps d'en presser une nouvelle. Compagnon fermé, chaque touche est prise
/// puis rendue, pour savoir si une autre application la tient. Une touche
/// tenue laisse les anciennes en vigueur, et son geste se nomme dans `prises`.
pub fn regler(app: &AppHandle, touches: Option<Touches>) -> Result<String, String> {
    let ouvert = app.get_webview_window(ETIQUETTE).is_some();
    rendre(app);
    let Some(touches) = touches else {
        return Ok(serde_json::json!({ "prises": [] }).to_string());
    };
    let nouvelles = touches.lire()?;
    let etat = app.state::<Raccourcis>();
    let anciennes = std::mem::replace(&mut *etat.0.lock().map_err(|e| e.to_string())?, nouvelles);
    let prises = prendre(app);
    if !prises.is_empty() || !ouvert {
        rendre(app);
    }
    if !prises.is_empty() {
        *etat.0.lock().map_err(|e| e.to_string())? = anciennes;
        if ouvert {
            prendre(app);
        }
    }
    Ok(serde_json::json!({ "prises": prises }).to_string())
}

/// Ouvre le compagnon avec les touches choisies, ou le ramène devant s'il l'est
/// déjà. Rend les gestes dont une autre application tient la touche.
pub async fn ouvrir(app: AppHandle, touches: Option<Touches>) -> Result<String, String> {
    if let Some(fenetre) = app.get_webview_window(ETIQUETTE) {
        let _ = fenetre.show();
        let _ = fenetre.set_focus();
        return Ok(serde_json::json!({ "prises": [] }).to_string());
    }
    if let Some(touches) = touches {
        let nouvelles = touches.lire()?;
        *app.state::<Raccourcis>().0.lock().map_err(|e| e.to_string())? = nouvelles;
    }
    let fenetre = WebviewWindowBuilder::new(&app, ETIQUETTE, WebviewUrl::App("compagnon.html".into()))
        .title("Krozties, compagnon de combat")
        .inner_size(420.0, 64.0)
        .min_inner_size(160.0, 56.0)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        .build()
        .map_err(|e| format!("le compagnon n'a pas pu s'ouvrir : {e}"))?;
    #[cfg(target_os = "macos")]
    {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            // SAFETY : la fenêtre vient d'être créée, et l'on est sur le fil principal.
            let panneau = fenetre.ns_window().is_ok_and(|ns_window| unsafe { plein_ecran::en_panneau(ns_window) });
            if panneau {
                let _ = handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = fenetre;
    // Une touche qu'une autre application tient déjà : le compagnon reste
    // utilisable à la souris, et la Rotation le dit.
    let prises = prendre(&app);
    Ok(serde_json::json!({ "prises": prises }).to_string())
}

/// Le compagnon fermé : les touches reviennent aux autres applications, l'icône
/// au Dock sur macOS, et la fenêtre principale au premier plan.
pub fn liberer(app: &AppHandle) {
    rendre(app);
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(principale) = app.get_webview_window("main") {
        let _ = principale.unminimize();
        let _ = principale.show();
        let _ = principale.set_focus();
    }
}

/// Ferme le compagnon ; sa fermeture appelle `liberer`.
pub fn fermer(app: &AppHandle) {
    if let Some(fenetre) = app.get_webview_window(ETIQUETTE) {
        let _ = fenetre.close();
    }
}

/// La taille que demande la disposition choisie, en points.
pub fn ajuster(fenetre: &tauri::Window, largeur: f64, hauteur: f64) -> Result<String, String> {
    fenetre
        .set_size(LogicalSize::new(largeur.clamp(160.0, 900.0), hauteur.clamp(56.0, 700.0)))
        .map_err(|e| e.to_string())?;
    Ok("null".into())
}

/// Par-dessus un jeu en plein écran, sur macOS. Le plein écran occupe un bureau
/// à lui, où ne s'affiche qu'un panneau qui n'active pas son application, au
/// niveau de l'économiseur d'écran, d'une application sans icône dans le Dock :
/// d'où le passage en accessoire tant que le compagnon est ouvert.
#[cfg(target_os = "macos")]
mod plein_ecran {
    use objc2::ffi::object_setClass;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
    use objc2::{class, msg_send, sel};
    use std::sync::OnceLock;

    const NIVEAU_DE_L_ECONOMISEUR: isize = 1000;
    const PANNEAU_NON_ACTIVANT: usize = 1 << 7;
    /// Tous les bureaux, plein écran compris, ceux des autres applications
    /// aussi ; immobile ; hors du cycle des fenêtres.
    const COMPORTEMENT: usize = 1 | 1 << 4 | 1 << 6 | 1 << 8 | 1 << 18;

    /// Un panneau qui ne prend jamais le clavier : le jeu le garde, même après
    /// un clic sur le compagnon.
    fn classe() -> &'static AnyClass {
        static CLASSE: OnceLock<&'static AnyClass> = OnceLock::new();
        CLASSE.get_or_init(|| {
            extern "C-unwind" fn non(_: &AnyObject, _: Sel) -> Bool {
                Bool::NO
            }
            let panneau = AnyClass::get(c"NSPanel").expect("AppKit sans NSPanel");
            let mut classe =
                ClassBuilder::new(c"KroztiesCompagnon", panneau).expect("classe KroztiesCompagnon déjà déclarée");
            // SAFETY : les deux méthodes ont la signature de celles qu'elles remplacent.
            unsafe {
                classe.add_method(sel!(canBecomeKeyWindow), non as extern "C-unwind" fn(_, _) -> _);
                classe.add_method(sel!(canBecomeMainWindow), non as extern "C-unwind" fn(_, _) -> _);
            }
            classe.register()
        })
    }

    /// Fait de la fenêtre du compagnon ce panneau, et dit si c'est fait. Un
    /// panneau a la taille d'une fenêtre, sans variable de plus : changer la
    /// classe ne déborde pas.
    ///
    /// WebKit observe la fenêtre (KVO), et une observation ne se retire que sous
    /// la classe qui l'a posée : changer de classe sous ses yeux lui ferait lever
    /// une exception à la fermeture. La vue web sort donc de la fenêtre le temps
    /// du changement, ce qui retire ses observations, puis y revient et les pose
    /// à nouveau.
    ///
    /// # Safety
    /// `ns_window` est la NSWindow vivante du compagnon, sur le fil principal.
    pub unsafe fn en_panneau(ns_window: *mut std::ffi::c_void) -> bool {
        let fenetre = ns_window.cast::<AnyObject>();
        unsafe {
            let contenu: Option<Retained<AnyObject>> = msg_send![fenetre, contentView];
            let Some(contenu) = contenu else {
                return false;
            };
            let attente: Retained<AnyObject> = msg_send![class!(NSView), new];
            let _: () = msg_send![fenetre, setContentView: &*attente];
            object_setClass(fenetre, classe());
            let _: () = msg_send![fenetre, setContentView: &*contenu];
            let masque: usize = msg_send![fenetre, styleMask];
            let _: () = msg_send![fenetre, setStyleMask: masque | PANNEAU_NON_ACTIVANT];
            let _: () = msg_send![fenetre, setFloatingPanel: true];
            let _: () = msg_send![fenetre, setHidesOnDeactivate: false];
            let _: () = msg_send![fenetre, setLevel: NIVEAU_DE_L_ECONOMISEUR];
            let _: () = msg_send![fenetre, setCollectionBehavior: COMPORTEMENT];
        }
        true
    }
}
