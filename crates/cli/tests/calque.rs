//! The overlay page draws in a real browser what the app sends it: each kind
//! of cell, the question mark, the badge at the bottom right.

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

/// The served folder: the app's pages, linked, and the test page beside them.
fn served() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ui = manifest.join("../krozties/ui");
    let folder = std::env::temp_dir().join(format!("krozties-calque-{}", std::process::id()));
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
    folder
}

fn unescaped(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

#[test]
fn the_overlay_draws_what_it_is_sent() {
    let chrome_exe = chrome();
    assert!(chrome_exe.exists(), "no Chrome at {}: this test opens the page in it", chrome_exe.display());
    let folder = served();
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
                &format!("http://127.0.0.1:{port}/calque-essai.html"),
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

    let opening = r#"<pre id="calque-essai">"#;
    let start = document.find(opening).unwrap_or_else(|| panic!("the test page wrote no result: {document}")) + opening.len();
    let end = start + document[start..].find("</pre>").unwrap();
    let r: serde_json::Value = serde_json::from_str(&unescaped(&document[start..end])).unwrap();

    assert_eq!(r["errors"], serde_json::json!([]), "{r}");
    assert_eq!(r["kinds"], serde_json::json!(["obstacle", "hole", "undecided"]), "{r}");
    assert_eq!(r["marks"], serde_json::json!(["?"]), "{r}");
    assert_eq!(r["badge"], "Carte non vérifiée · 1 combat sur 2", "{r}");
    let width = r["badgeWidth"].as_f64().unwrap();
    assert!((150.0..320.0).contains(&width), "the badge as wide as its text: {r}");
    let (w, h) = (r["area"][0].as_f64().unwrap(), r["area"][1].as_f64().unwrap());
    assert_eq!(r["badgeAt"], format!("translate({}, {})", w - width - 14.0, h - 44.0), "bottom right: {r}");
    assert_eq!(r["cleared"], 0, "{r}");
}
