//! A local web front end, deliberately small: one page and a few endpoints for
//! one person on their own machine, with a hand-rolled request parser instead of
//! a dependency tree. Not hardened; binds to the loopback interface only.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::time::Duration;

/// Browsers open speculative connections and then send nothing on them. A
/// server that reads them in turn blocks forever on the first one, so each
/// connection gets its own thread and a deadline.
const READ_TIMEOUT: Duration = Duration::from_secs(15);

use dofus_app::solve::{self, Request};

const INDEX: &str = include_str!("web/index.html");

pub fn serve(port: u16) -> Result<(), String> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .map_err(|e| format!("impossible d'écouter sur le port {port} : {e}"))?;
    println!("Interface disponible sur http://127.0.0.1:{port}/");
    println!("Ctrl-C pour arrêter.");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                std::thread::spawn(move || {
                    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
                    let _ = stream.set_write_timeout(Some(READ_TIMEOUT));
                    // Un panic reste dans la requête qui l'a provoqué : un sort mal modélisé ne
                    // fait pas tomber le serveur.
                    let issue = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        handle(stream, port)
                    }));
                    match issue {
                        Ok(Err(e)) => eprintln!("requête abandonnée : {e}"),
                        Err(_) => eprintln!(
                            "requête abandonnée : le calcul a échoué sur ce deck. \
                             Le serveur reste disponible."
                        ),
                        Ok(Ok(())) => {}
                    }
                });
            }
            Err(e) => eprintln!("connexion refusée : {e}"),
        }
    }
    Ok(())
}

struct Incoming {
    method: String,
    path: String,
    body: String,
}

fn read_request(stream: &TcpStream) -> Result<Incoming, String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();

    let mut length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).map_err(|e| e.to_string())?;
        if header.trim().is_empty() {
            break;
        }
        if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; length.min(4 * 1024 * 1024)];
    if !body.is_empty() {
        reader.read_exact(&mut body).map_err(|e| e.to_string())?;
    }
    Ok(Incoming {
        method,
        path,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &str) -> Result<(), String> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}; charset=utf-8\r\n\
         Content-Length: {}\r\nCache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|e| e.to_string())
}

/// La page, avec l'URL `javascript:` du favori déjà posée dans son ancre : le
/// bouton n'est jamais faux, même avant la première requête.
fn index_html(port: u16) -> String {
    let code = include_str!("web/bookmarklet.min.js")
        .replace("TARGET", &format!("'http://127.0.0.1:{port}/import'"));
    // Le code entre dans un attribut HTML : il porte des guillemets, des `&&`
    // et les `>` des fleches. Sans echappement, le premier `"` fermerait
    // l'attribut et le reste du script se retrouverait dans le document.
    let mut attribut = String::from("javascript:");
    for c in code.trim().chars() {
        match c {
            '&' => attribut.push_str("&amp;"),
            '<' => attribut.push_str("&lt;"),
            '>' => attribut.push_str("&gt;"),
            '"' => attribut.push_str("&quot;"),
            _ => attribut.push(c),
        }
    }
    INDEX.replace("MARQUEPAGE", &attribut)
}

fn handle(mut stream: TcpStream, port: u16) -> Result<(), String> {
    let request = read_request(&stream)?;
    let path = request.path.split('?').next().unwrap_or("/");

    match (request.method.as_str(), path) {
        // L'interface de Krozties servie depuis le disque, en développement, si
        // `KROZTIES_UI` désigne un dossier : la même interface tourne alors dans un
        // navigateur, où elle se vérifie avec ses outils.
        ("GET", p)
            if std::env::var("KROZTIES_UI").is_ok()
                && !p.starts_with("/api/")
                && !p.starts_with("/icons/") =>
        {
            let racine = std::env::var("KROZTIES_UI").unwrap_or_default();
            // `/import`, cible du favori DofusBook, rend la page : tout chemin sans
            // extension est une adresse de l'application, pas un fichier.
            let relatif = if p == "/" || !p.contains('.') {
                "index.html"
            } else {
                p.trim_start_matches('/')
            };
            // Aucun `..` ne sort du dossier servi.
            if relatif.contains("..") {
                return respond(&mut stream, "403 Forbidden", "text/plain", "refuse");
            }
            let chemin = std::path::Path::new(&racine).join(relatif);
            let kind = match chemin.extension().and_then(|e| e.to_str()) {
                Some("html") => "text/html",
                Some("js") => "text/javascript",
                Some("css") => "text/css",
                Some("json") => "application/json",
                Some("png") => "image/png",
                // ⚠️ SANS CETTE LIGNE, les icônes d'objets partaient en
                // `application/octet-stream`. Le navigateur s'en accommode
                // souvent et pas toujours : un type juste coûte une ligne.
                Some("webp") => "image/webp",
                Some("svg") => "image/svg+xml",
                _ => "application/octet-stream",
            };
            match std::fs::read(&chemin) {
                Ok(bytes) => respond_bytes_cache(&mut stream, kind, &bytes, "no-store"),
                Err(_) => respond(&mut stream, "404 Not Found", "text/plain", "introuvable"),
            }
        }
        ("GET", "/") | ("GET", "/import") => {
            respond(&mut stream, "200 OK", "text/html", &index_html(port))
        }
        // The compact form the page puts inside the bookmark's javascript: URL.
        // It must be self-contained: a page served over https cannot load a
        // script from http://127.0.0.1, so fetching it at click time is not an
        // option. Navigating there afterwards is fine.
        ("GET", "/bookmarklet.min.js") => {
            let source = include_str!("web/bookmarklet.min.js")
                .replace("TARGET", &format!("'http://127.0.0.1:{port}/import'"));
            respond(&mut stream, "200 OK", "text/plain", source.trim())
        }
        ("GET", "/bookmarklet.js") => {
            let source = include_str!("../../../tools/dofusbook_bookmarklet.js").replace(
                "https://example.invalid/import",
                &format!("http://127.0.0.1:{port}/import"),
            );
            respond(&mut stream, "200 OK", "text/javascript", &source)
        }
        // L'arrêt du calcul en cours. Sans corps : il n'y a qu'une recherche
        // à la fois, et rien à préciser.
        ("POST", "/api/cancel") => {
            solve::demander_arret();
            respond(&mut stream, "200 OK", "application/json", "{\"ok\":true}")
        }
        // L'avancement du calcul en cours. Interrogée pendant que la
        // résolution tourne, donc servie par un autre fil : `serve` en ouvre un
        // par connexion.
        ("GET", "/api/progress") => respond(
            &mut stream,
            "200 OK",
            "application/json",
            &solve::avancement_json(),
        ),
        ("GET", "/api/classes") => match solve::classes_json() {
            Ok(json) => respond(&mut stream, "200 OK", "application/json", &json),
            Err(e) => respond(
                &mut stream,
                "500 Internal Server Error",
                "application/json",
                &error_json(&e),
            ),
        },
        ("POST", "/api/resolve") => {
            match serde_json::from_str::<dofus_build::BuildInput>(&request.body)
                .map_err(|e| format!("requête illisible : {e}"))
                .and_then(|b| solve::resolve_json(&b))
            {
                Ok(json) => respond(&mut stream, "200 OK", "application/json", &json),
                Err(e) => respond(
                    &mut stream,
                    "400 Bad Request",
                    "application/json",
                    &error_json(&e),
                ),
            }
        }
        // Le réseau de pièges : ce qu'une pose couvre et ce qu'elle coûte.
        // Rien à voir avec une rotation, d'où un endpoint distinct.
        ("POST", "/api/reseau") => {
            match serde_json::from_str::<dofus_app::reseau::RequeteReseau>(&request.body)
                .map_err(|e| e.to_string())
                .and_then(|r| dofus_app::reseau::reseau_json(&r))
            {
                Ok(json) => respond(&mut stream, "200 OK", "application/json", &json),
                Err(e) => respond(
                    &mut stream,
                    "400 Bad Request",
                    "application/json",
                    &error_json(&e),
                ),
            }
        }
        // Ce que chaque sort vaudrait à la place d'un lancer de la rotation.
        ("POST", "/api/conseil") => {
            match serde_json::from_str::<dofus_app::solve::DemandeDeConseil>(&request.body)
                .map_err(|e| e.to_string())
                .and_then(|d| dofus_app::solve::conseil_json(&d))
            {
                Ok(json) => respond(&mut stream, "200 OK", "application/json", &json),
                Err(e) => respond(
                    &mut stream,
                    "400 Bad Request",
                    "application/json",
                    &error_json(&e),
                ),
            }
        }
        // Les invocations du build : les dégâts de chaque attaque.
        ("POST", "/api/invocations") => {
            match serde_json::from_str::<dofus_build::BuildInput>(&request.body)
                .map_err(|e| e.to_string())
                .and_then(|b| dofus_app::invocations::invocations_json(&b))
            {
                Ok(json) => respond(&mut stream, "200 OK", "application/json", &json),
                Err(e) => respond(
                    &mut stream,
                    "400 Bad Request",
                    "application/json",
                    &error_json(&e),
                ),
            }
        }
        // Les bombes du Roublard : leurs murs, leurs explosions, et ce que
        // chaque ennemi en prend.
        ("POST", "/api/bombes") => {
            match serde_json::from_str::<dofus_app::bombes::RequeteBombes>(&request.body)
                .map_err(|e| e.to_string())
                .and_then(|r| dofus_app::bombes::bombes_json(&r))
            {
                Ok(json) => respond(&mut stream, "200 OK", "application/json", &json),
                Err(e) => respond(
                    &mut stream,
                    "400 Bad Request",
                    "application/json",
                    &error_json(&e),
                ),
            }
        }
        // Les portails de l'Éliotrope : le trajet d'un sort projeté, sa sortie,
        // son bonus, et ses dégâts depuis la sortie.
        ("POST", "/api/portails") => {
            match serde_json::from_str::<dofus_app::portails::RequetePortails>(&request.body)
                .map_err(|e| e.to_string())
                .and_then(|r| dofus_app::portails::portails_json(&r))
            {
                Ok(json) => respond(&mut stream, "200 OK", "application/json", &json),
                Err(e) => respond(
                    &mut stream,
                    "400 Bad Request",
                    "application/json",
                    &error_json(&e),
                ),
            }
        }
        // Les cartes de boss, pour le sélecteur de plateau des KrozTools.
        ("GET", "/api/cartes") => respond(
            &mut stream,
            "200 OK",
            "application/json",
            &dofus_app::cartes::cartes_json(),
        ),
        // Les changements que la bêta en cours annonce, pour l'onglet « Bêta en
        // cours ».
        ("GET", "/api/beta") => respond(
            &mut stream,
            "200 OK",
            "application/json",
            &dofus_app::notes::beta::beta_json(),
        ),
        // La ligne de vue : ce qu'une case ne voit pas, murs et corps compris.
        ("POST", "/api/vue") => {
            match serde_json::from_str::<dofus_app::cartes::RequeteVue>(&request.body)
                .map_err(|e| e.to_string())
                .and_then(|r| dofus_app::cartes::vue_json(&r))
            {
                Ok(json) => respond(&mut stream, "200 OK", "application/json", &json),
                Err(e) => respond(
                    &mut stream,
                    "400 Bad Request",
                    "application/json",
                    &error_json(&e),
                ),
            }
        }
        // L'aperçu sur grille : la requête de `solve`, sans recherche de rotation. Un
        // endpoint à part, car le joueur déplace ses ennemis bien plus souvent qu'il ne
        // relance une recherche.
        ("POST", "/api/zone") => match serde_json::from_str::<Request>(&request.body)
            .map_err(|e| e.to_string())
            .and_then(|r| dofus_app::grille::zones_json(&r))
        {
            Ok(json) => respond(&mut stream, "200 OK", "application/json", &json),
            Err(e) => respond(
                &mut stream,
                "400 Bad Request",
                "application/json",
                &error_json(&e),
            ),
        },
        ("POST", "/api/solve") => match serde_json::from_str::<Request>(&request.body)
            .map_err(|e| format!("requête illisible : {e}"))
            .and_then(|r| solve::solve_json(&r))
        {
            Ok(json) => respond(&mut stream, "200 OK", "application/json", &json),
            Err(e) => respond(
                &mut stream,
                "400 Bad Request",
                "application/json",
                &error_json(&e),
            ),
        },
        // Spell icons, vendored so the running service never reaches out.
        ("GET", p) if p.starts_with("/icons/") && p.ends_with(".png") => {
            let name = p.trim_start_matches("/icons/");
            let safe = name
                .trim_end_matches(".png")
                .chars()
                .all(|c| c.is_ascii_digit());
            let root = std::env::var("DOFUS_DATA")
                .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../..").to_string());
            match safe.then(|| std::fs::read(format!("{root}/data/icons/{name}"))) {
                Some(Ok(bytes)) => respond_bytes(&mut stream, "image/png", &bytes),
                _ => respond(&mut stream, "404 Not Found", "text/plain", "introuvable"),
            }
        }
        _ => respond(&mut stream, "404 Not Found", "text/plain", "introuvable"),
    }
}

/// Une journee de cache : la politique des icones, qui ne changent jamais.
fn respond_bytes(stream: &mut TcpStream, kind: &str, body: &[u8]) -> Result<(), String> {
    respond_bytes_cache(stream, kind, body, "max-age=86400")
}

/// Les fichiers de l'interface ne se mettent pas en cache : après une
/// modification, la page doit servir le nouveau module.
fn respond_bytes_cache(
    stream: &mut TcpStream,
    kind: &str,
    body: &[u8],
    cache: &str,
) -> Result<(), String> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\
         Cache-Control: {cache}\r\nX-Content-Type-Options: nosniff\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(head.as_bytes())
        .map_err(|e| e.to_string())?;
    stream.write_all(body).map_err(|e| e.to_string())
}

fn error_json(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le favori part complet dans le HTML, pas rempli après coup : sinon, glissé
    /// dans la barre trop tôt, il n'est qu'un lien vers l'outil et n'importe rien.
    #[test]
    fn le_favori_est_servi_deja_rempli() {
        let page = index_html(7878);
        let deb = page
            .find(r#"id="marquepage" href=""#)
            .expect("l'ancre du favori doit exister");
        let deb = deb + r#"id="marquepage" href=""#.len();
        let fin = deb + page[deb..].find('"').expect("l'attribut doit se fermer");
        let href = &page[deb..fin];

        assert!(
            href.starts_with("javascript:"),
            "le favori doit porter son code des le premier octet, trouve : {}",
            &href[..href.len().min(40)]
        );
        // Sans le port, le favori ouvrirait le vide. La substitution du port se
        // fait dans le meme geste que celle du code, les deux se verifient ici.
        assert!(
            href.contains("127.0.0.1:7878/import"),
            "le favori doit viser le port sur lequel le serveur tourne"
        );
        // Un `"` non échappé fermerait l'attribut et déverserait le script dans le
        // document.
        assert!(
            href.len() > 800,
            "l'attribut s'est ferme trop tot, un caractere n'est pas echappe : {} caracteres",
            href.len()
        );
        assert!(
            !page.contains("MARQUEPAGE"),
            "le gabarit n'a pas ete substitue"
        );
    }
}
