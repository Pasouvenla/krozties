//! L'interface de Krozties s'affiche dans un vrai navigateur, sans erreur : seul
//! test qui regarde la page plutôt que le moteur.
//!
//! Le serveur du CLI sert l'interface depuis le disque ; `tests/interface/fumee.js`
//! s'ouvre dans Chrome sans fenêtre, et le test relit ce que la page a écrit :
//! chaque outil affiché, le générateur du simulateur mené jusqu'au tour suivant,
//! et toute erreur JavaScript. Sans Chrome, le test échoue et le dit.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CHROMES: [&str; 3] = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
];

/// Le premier Chrome présent ; `CHROME` dans l'environnement l'emporte.
fn chrome() -> PathBuf {
    if let Some(chemin) = std::env::var_os("CHROME") {
        return PathBuf::from(chemin);
    }
    CHROMES.iter().map(PathBuf::from).find(|p| p.exists()).unwrap_or_else(|| PathBuf::from(CHROMES[0]))
}

/// Un processus arrêté quand le test se termine, qu'il réussisse ou non.
struct Arrete(Child);

impl Drop for Arrete {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn manifeste() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Le dossier servi : l'interface, par liens, et la page de test à côté. La
/// page de test n'entre jamais dans l'application livrée.
fn dossier_servi() -> PathBuf {
    let ui = manifeste().join("../krozties/ui");
    let dossier = std::env::temp_dir().join(format!("krozties-interface-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dossier);
    std::fs::create_dir_all(&dossier).unwrap();
    for entree in std::fs::read_dir(&ui).unwrap() {
        let entree = entree.unwrap();
        std::os::unix::fs::symlink(entree.path(), dossier.join(entree.file_name())).unwrap();
    }
    let index = std::fs::read_to_string(ui.join("index.html")).unwrap();
    let balise = r#"<script type="module" src="app.js"></script>"#;
    assert!(index.contains(balise), "index.html ne charge plus app.js comme ce test l'attend");
    let page = index.replace(balise, &format!("{balise}\n<script type=\"module\" src=\"fumee.js\"></script>"));
    std::fs::write(dossier.join("fumee.html"), page).unwrap();
    std::fs::copy(manifeste().join("tests/interface/fumee.js"), dossier.join("fumee.js")).unwrap();
    dossier
}

/// Le texte d'un nœud tel que Chrome l'écrit, rendu à lui-même.
fn sans_entites(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", "\u{a0}")
        .replace("&amp;", "&")
}

#[test]
fn chaque_outil_s_affiche_sans_erreur() {
    let chrome_exe = chrome();
    assert!(
        chrome_exe.exists(),
        "Chrome est introuvable en {} : ce test en a besoin pour ouvrir l'interface",
        chrome_exe.display()
    );
    let dossier = dossier_servi();
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let _serveur = Arrete(
        Command::new(env!("CARGO_BIN_EXE_dofus"))
            .args(["serve", &port.to_string()])
            .env("KROZTIES_UI", &dossier)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("le serveur du CLI"),
    );
    let debut = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(debut.elapsed() < Duration::from_secs(30), "le serveur ne répond pas");
        std::thread::sleep(Duration::from_millis(100));
    }

    // ⚠️ LE TEMPS DE LA PAGE EST VIRTUEL. Ses pauses passent en un instant, et
    // il s'arrête tant qu'une requête au serveur est en cours : la page attend
    // donc le moteur, même lent, sans que le test ait à deviner combien.
    let profil = dossier.join("profil-chrome");
    let mut chrome = Arrete(
        Command::new(&chrome_exe)
            .args([
                "--headless=new",
                "--disable-gpu",
                "--no-first-run",
                "--no-default-browser-check",
                "--use-mock-keychain",
                &format!("--user-data-dir={}", profil.display()),
                "--virtual-time-budget=600000",
                "--dump-dom",
                &format!("http://127.0.0.1:{port}/fumee.html"),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Chrome"),
    );
    // Chrome écrit le document sans toujours se fermer : on lit jusqu'à la fin du
    // document, puis on l'arrête.
    let mut sortie = chrome.0.stdout.take().unwrap();
    let (envoi, reception) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut lu = Vec::new();
        let mut morceau = [0u8; 65536];
        while let Ok(n) = sortie.read(&mut morceau) {
            if n == 0 {
                break;
            }
            lu.extend_from_slice(&morceau[..n]);
            if lu.windows(7).any(|w| w == b"</html>") {
                break;
            }
        }
        let _ = envoi.send(String::from_utf8_lossy(&lu).into_owned());
    });
    let document = reception.recv_timeout(Duration::from_secs(300));
    drop(chrome);
    // Les processus auxiliaires de Chrome portent le chemin du profil.
    let _ = Command::new("pkill")
        .args(["-f", &profil.display().to_string()])
        .status();
    let _ = std::fs::remove_dir_all(&dossier);
    let document = document.expect("Chrome n'a pas rendu la page en cinq minutes");

    let ouverture = r#"<pre id="fumee">"#;
    let debut = document
        .find(ouverture)
        .unwrap_or_else(|| panic!("la page de test n'a pas écrit son résultat : {document}"))
        + ouverture.len();
    let fin = debut + document[debut..].find("</pre>").unwrap();
    let r: serde_json::Value = serde_json::from_str(&sans_entites(&document[debut..fin])).unwrap();

    assert_eq!(r["erreurs"], serde_json::json!([]), "erreurs JavaScript : {r}");
    for (outil, titre) in [
        ("equipement", "Équipement"),
        ("rotation", "Rotation"),
        ("kroztools", "KrozTools"),
        ("beta", "Bêta en cours"),
    ] {
        assert_eq!(r["outils"][outil]["titre"], titre, "{outil} : {r}");
        // Entre deux bêtas, l'onglet de la bêta le dit, et rien d'autre.
        let sans_beta = serde_json::from_str::<serde_json::Value>(&dofus_app::notes::beta::beta_json())
            .ok()
            .and_then(|b| b["classes"].as_array().map(Vec::is_empty))
            .unwrap_or(false);
        if outil == "beta" && sans_beta {
            assert_eq!(r["outils"][outil]["sous"], "Aucune bêta en cours.", "{r}");
            continue;
        }
        assert!(
            r["outils"][outil]["elements"].as_u64().unwrap_or(0) > 50,
            "{outil} s'affiche presque vide : {r}"
        );
    }
    // KrozZone, KrozTrap, KrozPortal et KrozSight s'ouvrent chacun dans leur
    // onglet ; KrozBoom, pas encore écrit, reste grisé.
    for onglet in ["zone", "trap", "boom", "portal", "vue"] {
        assert_eq!(r["onglets"][onglet]["choisi"], "true", "{onglet} : {r}");
        assert!(
            r["onglets"][onglet]["elements"].as_u64().unwrap_or(0) > 50,
            "l'onglet {onglet} s'affiche presque vide : {r}"
        );
    }
    assert_eq!(r["onglets"]["fermes"], serde_json::json!([]), "{r}");
    // Les cases des bonus de Dofus suivent le build : les quatre du Pandawa, cochées, et la première décochée s'écarte du calcul.
    assert_eq!(
        r["bonusDofus"]["noms"],
        serde_json::json!(["Rouge Vermeil", "Jaune Ocre", "Bleu Turquoise", "Pourpre Profond"]),
        "{r}"
    );
    assert_eq!(r["bonusDofus"]["cochees"], 4, "{r}");
    assert_eq!(r["bonusDofus"]["ecartes"], serde_json::json!(["Rouge Vermeil"]), "{r}");
    // L'exemple de KrozBoom, calculé à la main dans `bombes.rs` : deux
    // explosions à 60 % de combos et 80 % de taux, 21 à 24 chacune ; le mur à
    // 30 % de combos, 39 à 42.
    assert_eq!(r["boom"]["explosion"], "42 à 48", "{r}");
    assert_eq!(r["boom"]["mur"], "39 à 42", "{r}");
    assert_eq!(r["boom"]["murs"], 3, "{r}");
    // 30-33 du Mur de Feu à 50 % de combos (IV et III), deux fois.
    assert_eq!(r["boom"]["plombage"], "90 à 98", "{r}");
    // Le mur de bombes du Roublard, dans la Rotation, repris de KrozBoom.
    assert_eq!(r["murDeBombes"], serde_json::json!({ "element": "feu", "combos": ["3", "2"] }), "{r}");
    // Les invocations de l'Osamodas, dans la Rotation.
    let titre = r["invocations"]["titre"].as_str().unwrap_or("");
    assert!(titre.contains("Tofu") && titre.contains("50 %"), "{r}");
    assert_eq!(r["invocations"]["beco"], serde_json::json!(["21 à 23", "25 à 28", "2"]), "{r}");
    // Un clic sur un lancer de la rotation : ce que chaque sort vaudrait à sa
    // place, le sort joué compris et marqué.
    assert!(r["conseil"]["lignes"].as_u64().unwrap_or(0) >= 1, "{r}");
    assert_eq!(r["conseil"]["joue"], 1, "{r}");
    // L'arme du build au deck, cochée d'office (elle frappe dans l'élément du
    // build), la Maîtrise normale par défaut, sa fiche, et des coups d'arme
    // dans la rotation.
    assert_eq!(
        r["arme"],
        serde_json::json!({ "au_deck": true, "cochee": true, "maitrise": "300", "titre": "Hachebarde de Guerre", "lancee": r["arme"]["lancee"] }),
        "{r}"
    );
    assert!(r["arme"]["lancee"].as_u64().unwrap_or(0) >= 1, "l'arme doit frapper : {r}");
    // Les remarques du calcul ont leur carte dans la Rotation.
    let remarques: Vec<&str> = r["remarques"].as_array().map_or_else(Vec::new, |v| v.iter().filter_map(|x| x.as_str()).collect());
    assert!(
        remarques.contains(&"Hachebarde de Guerre : Maîtrise d'arme comptée, 300 Puissance sur ses coups"),
        "{remarques:?}"
    );
    assert!(
        remarques.iter().any(|r| r.starts_with("Couronne de Brâm Barbe-Monde : +2 % de dommages finaux")),
        "{remarques:?}"
    );
    // Le sort qu'un objet ajoute à la barre, au deck et coché d'office : il
    // frappe dans l'élément du build.
    assert_eq!(
        r["sortDObjet"],
        serde_json::json!({ "au_deck": true, "cochee": true, "titre": "Pelle Fantomatique du Dopeul" }),
        "{r}"
    );
    // La fiche d'un objet au survol : le Dofus Ocre, son effet spécial compté
    // dans la Rotation, et ce qu'il fait hors du combat.
    assert_eq!(
        r["ficheObjet"],
        serde_json::json!({ "titre": "Dofus Ocre", "statut": "compté dans la Rotation", "aussi": "Attitude : Dofus Ocre" }),
        "{r}"
    );
    // L'exemple de KrozPortal : deux portails à huit cases, 2 % par case ; et
    // les dix-neuf classes au choix du lanceur.
    assert_eq!(r["portail"]["bonus"], "+16 % de dommages finaux", "{r}");
    assert_eq!(r["portail"]["classes"], 19, "{r}");
    // Le poison se chiffre : deux coups, et le moment où ils tombent.
    let etiquettes = r["poison"]["etiquettes"].as_array().cloned().unwrap_or_default();
    assert!(
        etiquettes.len() == 1 && etiquettes[0].as_str().unwrap_or("").ends_with("×2"),
        "{r}"
    );
    assert_eq!(
        r["poison"]["quand"], "Au début de chacun des 2 prochains tours de la cible :",
        "{r}"
    );
    // Survolée, une case de chaque outil montre ce qu'un clic y poserait.
    for onglet in ["zone", "trap", "boom", "portal", "vue"] {
        assert_eq!(r["apercus"][onglet], true, "aperçu de pose de {onglet} : {r}");
    }
    // Toutes les fiches de sorts se composent, sans ligne en minuscule.
    assert_eq!(r["fiches"]["erreurs"], serde_json::json!([]), "{}", r["fiches"]);
    assert!(r["fiches"]["composees"].as_u64().unwrap_or(0) > 900, "{}", r["fiches"]["composees"]);
    assert_eq!(r["fiches"]["minuscules"], serde_json::json!([]), "{}", r["fiches"]);
    // Les objets de classe d'un Crâ : ce qu'ils changent se lit sur la fiche de
    // l'objet et dans l'infobulle du sort.
    assert_eq!(
        r["objetsDeClasse"]["equipement"], "Coiffe de Robbie Capuche : +6 dégâts de base",
        "{}", r["objetsDeClasse"]
    );
    assert_eq!(
        r["objetsDeClasse"]["critique"], "80 % · 15 % de base + 35 de l'objet de classe + 30 du build",
        "{}", r["objetsDeClasse"]
    );
    assert_eq!(r["objetsDeClasse"]["fiche"][0], "Flèche Ralentissante : +6 dégâts de base", "{}", r["objetsDeClasse"]);
    // Klime choisie dans KrozSight : ses 136 cases de sol, ses 57 murs, et de
    // l'ombre depuis la case du milieu.
    assert_eq!(r["carte"]["sol"], 136, "{r}");
    assert_eq!(r["carte"]["murs"], 57, "{r}");
    assert!(r["carte"]["ombre"].as_u64().unwrap_or(0) > 0, "{r}");
    assert!(r["carte"]["choisie"].as_str().unwrap_or("").contains("Klime"), "{r}");
    let proposition = r["reseau"]["proposition"].as_str().unwrap_or("");
    assert!(
        proposition.starts_with("Tour 1 :") && proposition.contains("Poser le tour"),
        "le générateur doit proposer un tour : {r}"
    );
    assert_eq!(r["reseau"]["apres"]["tour"], "Tour 2", "poser le tour passe au suivant : {r}");
    // L'orientation d'un Sram Eau : ses sorts et ses pièges Eau, la meilleure
    // variante de chaque paire, et rien d'un autre élément.
    let coches: Vec<&str> = r["orientation"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
    for id in ["guet_apens", "larcin", "pillage", "poisse", "piege_fangeux", "piege_scelerat", "calamite"] {
        assert!(coches.contains(&id), "{id} : {coches:?}");
    }
    for id in ["piege_sournois", "peur", "cruaute", "invocation_de_l_arakne", "piege_mortel"] {
        assert!(!coches.contains(&id), "{id} : {coches:?}");
    }
    // Le concepteur : un plan de deux tours, qu'on parcourt dans les deux sens
    // jusqu'au déclenchement, où il dit ses entrées.
    let plan = &r["reseau"]["plan"];
    let etapes: Vec<&str> = plan["etapes"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(etapes, ["Tour 1", "Tour 2", "Déclenchement"], "{r}");
    assert_eq!(plan["pas"], "Tour 1", "{r}");
    assert!(plan["resume"].as_str().unwrap_or("").contains("en 2 tours"), "{r}");
    assert!(plan["panneau"].as_str().unwrap_or("").contains("Posez-les dans l'ordre des numéros"), "{r}");
    assert!(plan["numeros"].as_u64().unwrap_or(0) > 0, "les poses du tour sont numérotées : {r}");
    // Le titre du panneau s'écrit en capitales : la casse ne compte pas.
    assert!(plan["suivant"].as_str().unwrap_or("").to_lowercase().contains("tour 2 sur 2"), "{r}");
    assert!(plan["fin"].as_str().unwrap_or("").contains("Amenez l'ennemi sur la case E1"), "{r}");
    assert!(plan["entrees"].as_u64().unwrap_or(0) >= 1, "{r}");
    assert_eq!(plan["retour"], "Tour 2", "{r}");
}
