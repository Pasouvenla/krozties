//! The pages of the combat reading, in a real browser: the overlay draws
//! what the app sends it (each kind of cell, the question mark, the badge at
//! the bottom right), and the companion turns the reading on and off.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const CHROMES: [&str; 3] = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
];

fn chrome() -> PathBuf {
    if let Some(path) = std::env::var_os("CHROME") {
        return PathBuf::from(path);
    }
    CHROMES.iter().map(PathBuf::from).find(|p| p.exists()).unwrap_or_else(|| PathBuf::from(CHROMES[0]))
}

/// A process stopped when the test ends, whether it passes or not.
struct Stopped(Child);

impl Drop for Stopped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The served folder: the app's pages, linked, and the test pages beside
/// them.
fn served(name: &str) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ui = manifest.join("../krozties/ui");
    let folder = std::env::temp_dir().join(format!("krozties-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    for entry in std::fs::read_dir(&ui).unwrap() {
        let entry = entry.unwrap();
        std::os::unix::fs::symlink(entry.path(), folder.join(entry.file_name())).unwrap();
    }
    let page = std::fs::read_to_string(ui.join("calque.html")).unwrap();
    let script = r#"<script type="module" src="calque.js"></script>"#;
    assert!(page.contains(script), "calque.html no longer loads calque.js as this test expects");
    std::fs::write(folder.join("calque-essai.html"), page.replace(script, r#"<script type="module" src="calque-essai.js"></script>"#))
        .unwrap();
    std::fs::copy(manifest.join("tests/interface/calque.js"), folder.join("calque-essai.js")).unwrap();
    // The companion's page under a stand-in for the desktop app, opened by a
    // test page.
    let page = std::fs::read_to_string(ui.join("compagnon.html")).unwrap();
    let script = r#"<script type="module" src="compagnon.js"></script>"#;
    assert!(page.contains(script), "compagnon.html no longer loads compagnon.js as this test expects");
    let stand_in = format!(r#"<script src="compagnon-tauri.js"></script>{script}"#);
    std::fs::write(folder.join("compagnon-essai.html"), page.replace(script, &stand_in)).unwrap();
    std::fs::copy(manifest.join("tests/interface/compagnon-tauri.js"), folder.join("compagnon-tauri.js")).unwrap();
    std::fs::copy(manifest.join("tests/interface/lecture.js"), folder.join("lecture-essai.js")).unwrap();
    std::fs::write(
        folder.join("lecture-essai.html"),
        r#"<!doctype html><html lang="fr"><head><meta charset="utf-8"></head><body><script type="module" src="lecture-essai.js"></script></body></html>"#,
    )
    .unwrap();
    folder
}

fn unescaped(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

/// What the test page `page` wrote in its `<pre id="{id}">`, once Chrome has
/// run it.
fn rendered(name: &str, page: &str, id: &str) -> serde_json::Value {
    let chrome_exe = chrome();
    assert!(chrome_exe.exists(), "no Chrome at {}: this test opens the page in it", chrome_exe.display());
    let folder = served(name);
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let _server = Stopped(
        Command::new(env!("CARGO_BIN_EXE_dofus"))
            .args(["serve", &port.to_string()])
            .env("KROZTIES_UI", &folder)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the CLI's server"),
    );
    let start = Instant::now();
    while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(start.elapsed() < Duration::from_secs(30), "the server does not answer");
        std::thread::sleep(Duration::from_millis(100));
    }
    let profile = folder.join("chrome-profile");
    let mut browser = Stopped(
        Command::new(&chrome_exe)
            .args([
                "--headless=new",
                "--disable-gpu",
                "--no-first-run",
                "--no-default-browser-check",
                "--use-mock-keychain",
                "--window-size=800,600",
                &format!("--user-data-dir={}", profile.display()),
                "--virtual-time-budget=10000",
                "--dump-dom",
                &format!("http://127.0.0.1:{port}/{page}"),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Chrome"),
    );
    let mut output = browser.0.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut read = Vec::new();
        let mut chunk = [0u8; 65536];
        while let Ok(n) = output.read(&mut chunk) {
            if n == 0 {
                break;
            }
            read.extend_from_slice(&chunk[..n]);
            if String::from_utf8_lossy(&read).contains("</html>") {
                break;
            }
        }
        let _ = send.send(String::from_utf8_lossy(&read).into_owned());
    });
    let document = receive.recv_timeout(Duration::from_secs(120));
    drop(browser);
    let _ = Command::new("pkill").args(["-f", &profile.display().to_string()]).status();
    let _ = std::fs::remove_dir_all(&folder);
    let document = document.expect("Chrome did not render the page in two minutes");

    let opening = format!(r#"<pre id="{id}">"#);
    let start = document.find(&opening).unwrap_or_else(|| panic!("the test page wrote no result: {document}")) + opening.len();
    let end = start + document[start..].find("</pre>").unwrap();
    serde_json::from_str(&unescaped(&document[start..end])).unwrap()
}

#[test]
fn the_overlay_draws_what_it_is_sent() {
    let r = rendered("calque", "calque-essai.html", "calque-essai");
    assert_eq!(r["errors"], serde_json::json!([]), "{r}");
    assert_eq!(r["kinds"], serde_json::json!(["obstacle", "hole", "undecided"]), "{r}");
    assert_eq!(r["marks"], serde_json::json!(["?"]), "{r}");
    assert_eq!(r["badge"], "Carte non vérifiée · 1 combat sur 2", "{r}");
    // As wide as its text, whatever the font the system draws it in.
    let (width, text) = (r["badgeWidth"].as_f64().unwrap(), r["textLength"].as_f64().unwrap());
    assert!(text > 100.0, "the badge's text has a width: {r}");
    assert_eq!(width, text.ceil() + 44.0, "the badge as wide as its text: {r}");
    let (w, h) = (r["area"][0].as_f64().unwrap(), r["area"][1].as_f64().unwrap());
    assert_eq!(r["badgeAt"], format!("translate({}, {})", w - width - 14.0, h - 44.0), "bottom right: {r}");
    assert_eq!(r["cleared"], 0, "{r}");
    assert_eq!(r["hooked"], serde_json::json!(["obstacle"]), "{r}");
}

#[test]
fn the_companion_turns_the_reading_on_and_off() {
    let r = rendered("lecture", "lecture-essai.html", "lecture-essai");
    use serde_json::json;
    let denied = "Autorisez Krozties, puis relancez-le.";
    assert_eq!(r["errors"], json!([]), "{r}");
    // Never turned on: the button, no pastille; the fiche first, which
    // « Plus tard » closes without a word to the app.
    assert_eq!(r["fresh"], json!({ "toggle": true, "pressed": "false", "pill": null }), "{r}");
    assert_eq!(r["offered"]["fiche"]["title"], "Lire le combat", "{r}");
    assert_eq!(r["offered"]["fiche"]["buttons"], json!(["Plus tard", "Activer"]), "{r}");
    assert_eq!(r["offered"]["calls"], json!([]), "{r}");
    assert_eq!(r["later"], json!({ "fiche": null, "calls": [], "stored": null, "pill": null }), "{r}");
    // « Activer »: the reading asked for, the choice kept, the fiche waiting
    // to hear where it stands; refused, it says why.
    assert_eq!(r["activating"]["calls"], json!([true]), "{r}");
    assert_eq!(r["activating"]["stored"], "true", "{r}");
    assert_eq!(r["activating"]["fiche"]["buttons"], json!(["Plus tard", "Activation… (off)"]), "{r}");
    assert_eq!(r["denied"]["fiche"]["error"], denied, "{r}");
    assert_eq!(r["denied"]["fiche"]["buttons"], json!(["Fermer", "Réessayer"]), "{r}");
    // « Réessayer »: off then on; the fiche waits past the « off », and
    // closes once the reading runs.
    assert_eq!(r["retrying"]["calls"], json!([true, false, true]), "{r}");
    assert_eq!(r["stillWaiting"], true, "{r}");
    assert_eq!(r["waiting"]["fiche"], json!(null), "{r}");
    assert_eq!(r["waiting"]["pill"]["text"], "Combat non reconnu", "{r}");
    assert_eq!(r["waiting"]["pill"]["title"], "Aucune fenêtre de Dofus n'est ouverte.", "{r}");
    assert_eq!(r["waiting"]["pressed"], "true", "{r}");
    assert_eq!(r["following"]["text"], "Combat suivi", "{r}");
    assert_eq!(r["following"]["kind"], "cp-pastille suivi", "{r}");
    // ◉ then turns it off and on, without the fiche.
    assert_eq!(r["off"]["pill"]["text"], "Lecture coupée", "{r}");
    assert_eq!(r["off"]["calls"], json!([true, false, true, false]), "{r}");
    assert_eq!((r["off"]["stored"].as_str(), r["off"]["pressed"].as_str()), (Some("false"), Some("false")), "{r}");
    assert_eq!(r["onAgain"], json!({ "fiche": null, "calls": [true, false, true, false, true], "stored": "true" }), "{r}");
    // Turned on last time: on again with the companion; the pastille at the
    // end of the bar, before the tools, and in a row of its own in the list.
    assert_eq!(r["resumed"], json!({ "calls": [true], "pill": null }), "{r}");
    assert_eq!(r["bar"], json!(["cp-tour", "cp-case", "cp-pastille", "cp-outils"]), "{r}");
    assert_eq!(r["list"], true, "{r}");
    // Stopped on an error: the pastille opens the fiche that says why.
    assert_eq!(r["stopped"]["text"], "Autorisation refusée", "{r}");
    assert_eq!((r["stopped"]["button"].as_bool(), r["stopped"]["title"].as_str()), (Some(true), Some(denied)), "{r}");
    assert_eq!(r["explained"]["error"], denied, "{r}");
    assert_eq!(r["closed"], json!({ "fiche": null, "list": true }), "{r}");
}
