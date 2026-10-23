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
    assert_eq!(r["majComparee"], serde_json::json!([true, false, true, false]), "{r}");
    assert_eq!(
        r["majTexte"],
        "Krozties 0.2.0 est disponible Vous utilisez la version 0.1.0. Télécharger Plus tard",
        "{r}"
    );
    // Deux ouvertures, deux demandes à GitHub ; la troisième, hors ligne, garde
    // le numéro lu.
    assert_eq!(r["majVerifiee"], serde_json::json!({ "demandes": 2, "derniere": "9.9.9" }), "{r}");
    assert_eq!(
        r["majAuto"],
        serde_json::json!({
            "annonce": "Krozties 0.10.0 est disponible Vous utilisez la version 0.9.0. Mettre à jour Plus tard",
            "pret": "Krozties 0.10.0 est prêt Il s'ouvrira au prochain lancement. Redémarrer maintenant Plus tard",
            "pretWindows": "Krozties 0.10.0 est prêt Il s'installera à la fermeture de Krozties. Redémarrer maintenant Plus tard",
            "echec": "La mise à jour n'a pas abouti Le téléchargement s'est interrompu Réessayer Télécharger sur GitHub",
            "uneLigne": { "annonce": true, "pret": true, "echec": true },
        }),
        "{r}"
    );
    // Le message pour Discord, et son bouton sous un vrai résultat.
    assert_eq!(
        r["discord"],
        serde_json::json!([
            "**Sram 200 · Agilité** · **900 dégâts** · 300 par tour · 310 sur la durée",
            "-# 3 tours · 1 ennemi · à distance · contre 20 % partout · Krozties 0.8.0",
            "```",
            "Tour  Dégâts  Sorts",
            "1-2    ≈ 300  Arnaque ×2 › Épidémie",
            "3        300  Injection Toxique",
            "",
            "Arnaque   ████████████████  60,0 %",
            "Épidémie  ███████████       40,0 %",
            "```",
            "-# [Le build sur DofusBook](<https://d-bk.net/fr/d/XXXX>)",
        ]),
        "{r}"
    );
    assert_eq!(r["copierRotation"], true, "{r}");
    assert_eq!(
        r["miseAJourBuild"],
        serde_json::json!({ "date": "lu sur DofusBook le 15/10 à 21:40", "boutons": 0 }),
        "{r}"
    );
    assert_eq!(r["reglagesGardes"], 2, "les réglages de la Rotation se gardent : {r}");
    // Les notes de mise à jour : les versions sautées, la fenêtre et la page.
    assert_eq!(
        r["nouveautes"],
        serde_json::json!({
            "depuis040": ["0.5.0", "0.4.2", "0.4.1"],
            "aJour": 0,
            "fenetre": true,
            "boutons": ["Compris", "Toutes les notes"],
            "page": { "uneParVersion": true, "premiereOuverte": true, "premiere": true, "sansDocumentation": true },
        }),
        "{r}"
    );
    assert_eq!(
        r["position"],
        serde_json::json!({
            "choix": ["Au choix", "Distance", "Mêlée"],
            "aide": "Les sorts et l'arme qui ne frappent qu'au contact sortent de la rotation.",
            "reglage": "distance",
        }),
        "{r}"
    );
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
    // L'Arakne, commune à toutes les classes, n'a d'encart que cochée.
    assert_eq!(r["invocations"]["arakne"], serde_json::json!([0, 1]), "{r}");
    // Un clic sur un lancer de la rotation : ce que chaque sort vaudrait à sa
    // place, le sort joué compris et marqué.
    assert!(r["conseil"]["lignes"].as_u64().unwrap_or(0) >= 1, "{r}");
    assert_eq!(r["conseil"]["joue"], 1, "{r}");
    // Après le calcul, la liste des sorts se replie et le détail commence en
    // haut de la page (sous toute la liste, il passait les 1 400 px) ; un
    // nouvel affichage, sans résultat, la rouvre.
    assert_eq!(
        (&r["repli"]["apresCalcul"], &r["repli"]["rouverte"]),
        (&serde_json::json!(true), &serde_json::json!(true)),
        "{}",
        r["repli"]
    );
    assert!(r["repli"]["haut"].as_i64().is_some_and(|h| h < 400), "{}", r["repli"]);
    // L'arme du build au deck, cochée d'office (elle frappe dans l'élément du
    // build), la Maîtrise normale par défaut, sa fiche, et des coups d'arme
    // dans la rotation.
    assert_eq!(
        r["arme"],
        serde_json::json!({ "au_deck": true, "cochee": true, "maitrise": "300", "titre": "Hachebarde de Guerre", "lancee": r["arme"]["lancee"] }),
        "{r}"
    );
    assert!(r["arme"]["lancee"].as_u64().unwrap_or(0) >= 1, "l'arme doit frapper : {r}");
    // Les résistances de la cible : l'ordre du moteur (Feu, Terre, Air, Eau,
    // Neutre), et 50 % Feu qui divisent par deux une rotation Feu.
    assert_eq!(r["ordreDesResistances"], serde_json::json!([[1, 4, 2, 3, 5], [10, 40, 20, 30, 50]]), "{r}");
    let entier = |v: &serde_json::Value| -> i64 {
        v.as_str().unwrap_or("").chars().filter(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0)
    };
    let (sans, avec) = (entier(&r["resistances"]["sans"]), entier(&r["resistances"]["avec"]));
    assert!(sans > 0 && (avec * 2 - sans).abs() <= sans / 50, "50 % Feu : {sans} puis {avec}");
    assert_eq!(r["resistances"]["contre"], "Feu 50 %", "{r}");
    // Les hypothèses du calcul, sous le total : l'écho d'un réglage du joueur (la
    // Maîtrise d'arme) n'y est plus.
    let remarques: Vec<&str> = r["remarques"].as_array().map_or_else(Vec::new, |v| v.iter().filter_map(|x| x.as_str()).collect());
    assert!(!remarques.iter().any(|r| r.contains("Maîtrise d'arme")), "{remarques:?}");
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
    // Un build enregistré sous un nouveau personnage depuis la barre du haut :
    // la barre le nomme, la page de gestion le range.
    let persos = &r["personnages"];
    assert!(persos["barre"][0].as_str().unwrap_or("").starts_with("Pyrus · "), "{r}");
    assert_eq!(persos["barre"][1], "Essai", "{r}");
    assert_eq!((&persos["cartes"], &persos["builds"]), (&serde_json::json!(1), &serde_json::json!(["Essai"])), "{r}");
    // La comparaison de deux builds du même Crâ : le tableau, les écarts
    // signés, ce qui fait l'écart sort par sort, et le cumul tour par tour.
    assert_eq!(
        r["comparaison"],
        serde_json::json!({
            "builds": ["Essai", "Agilité"], "lignes": true, "ecarts": true, "ecartParSort": true, "cumul": true, "copier": true,
            "charge": "Build chargé · Crâ 200 (choisi)",
        }),
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
    // Sous le résultat de la Rotation : le rythme, ses trois repères, et les dégâts par PA.
    assert_eq!(r["graphiques"], serde_json::json!({ "reperes": 3, "parPA": true }), "{r}");
    assert_eq!(r["poussees"], "Inclure les dommages de poussée", "{r}");
    // Ce que vaut chaque caractéristique, mesuré sur la rotation du Sram.
    assert_eq!(
        r["valeurs"]["tuiles"],
        serde_json::json!(["Le plus rentable", "Votre caractéristique", "Inutile ici"]),
        "{r}"
    );
    assert!(r["valeurs"]["lignes"].as_u64().unwrap_or(0) >= 3, "{r}");
    // Les touches du compagnon : une lettre seule et un doublon refusés, Maj+F9
    // gardée pour le tour, puis les touches d'origine rétablies.
    assert_eq!(
        r["touches"],
        serde_json::json!({
            "ecoute": "Pressez une touche…",
            "refus": true,
            "choisie": "Maj+F9",
            "gardee": "Shift+F9",
            "doublon": "Maj+F9 sert déjà à « Tour suivant ».",
            "resume": "Raccourcis du compagnon : F8, Maj+F9, F7",
            "retablie": "Shift+F8",
        }),
        "{r}"
    );
    // Le compagnon de combat reprend la rotation de deux tours, au clavier avec
    // les touches choisies, puis en liste.
    assert_eq!(
        r["compagnon"],
        serde_json::json!({
            "tours": 2,
            "cases": true,
            "apresF8": "1",
            "tour": "T2",
            "liste": true,
            "pied": ["F8", "Maj+F9", "F7"],
        }),
        "{r}"
    );
    // Partager : le fichier enregistré, reçu sur un autre build, rouvert à
    // l'identique, jusqu'au total.
    assert_eq!(
        r["partage"],
        serde_json::json!({
            "nom": true,
            "enregistre": true,
            "bandeau": true,
            "classe": true,
            "horizon": true,
            "coches": true,
            "total": true,
            "ferme": true,
        }),
        "{r}"
    );
    // Recaler au tour 2 : la fiche remplie d'après le curseur, les sorts du
    // deck à ajouter, puis la suite recalculée et marquée.
    assert_eq!(
        r["recaler"],
        serde_json::json!({
            "jetons": true,
            "sorts": true,
            "ferme": true,
            "erreur": null,
            "badge": "recalé",
            "recale": 1,
        }),
        "{r}"
    );
    // Les données manquantes : le Sram n'a plus rien à relever, la page le
    // dit ; sur l'onglet du Crâ, le Tir Perçant, son formulaire, le refus d'un
    // envoi vide, puis l'adresse du formulaire GitHub remplie.
    assert_eq!(
        r["manquantes"],
        serde_json::json!({
            "votreClasse": "Rien à relever pour votre classe.",
            "onglet": "Crâ",
            "formulaire": true,
            "refus": "Tapez d'abord ce que vous voyez en jeu.",
            "adresse": {
                "template": "donnee-manquante.yml",
                "classe": "Crâ",
                "sort": "Tir Perçant",
                "observation": "+15 % jusqu'à la prochaine attaque",
                "reference": "sort 32471, effet 1163",
            },
        }),
        "{r}"
    );
    // L'outil et l'onglet ouverts se retiennent pour le lancement suivant.
    assert_eq!(r["ouverture"], serde_json::json!({ "outil": "kroztools", "kroztool": "vue" }), "{r}");
    // Les invocations de la classe jouent d'office, les communes non ; le Sram
    // n'en a pas, sa case ne s'affiche pas.
    assert_eq!(
        r["invocationsCochees"],
        serde_json::json!({ "sram": { "classe": null, "communes": false }, "osamodas": { "classe": true, "communes": false } }),
        "{r}"
    );
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
