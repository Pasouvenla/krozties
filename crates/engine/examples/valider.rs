//! Un fichier de mecaniques genere charge-t-il, et que reste-t-il d'inconnu ?
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut args = std::env::args().skip(1);
    let nom = args.next().unwrap_or_else(|| "cra".into());
    let breed: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(9);

    let mut rs = match Ruleset::load(format!("{root}/data/rulesets/{nom}.yaml")) {
        Ok(r) => r,
        Err(e) => {
            println!("CHARGEMENT ÉCHOUÉ : {e}");
            std::process::exit(1);
        }
    };
    println!("chargé : {} sorts", rs.spells.len());
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    let report = rs.merge_snapshot(&snap);
    println!(
        "  {} valeurs remplies par l'instantané",
        report.filled.len()
    );
    println!("  {} conflits", report.conflicts.len());
    for c in report.conflicts.iter().take(5) {
        println!("     {c:?}");
    }
    println!("  {} non appariés", report.unmatched.len());
    for u in report.unmatched.iter().take(5) {
        println!("     {u}");
    }
    let gaps = rs.data_gaps();
    println!("  {} trous de données restants", gaps.len());
    for g in gaps.iter().take(5) {
        println!("     {g}");
    }
    let cov = snap.coverage(&rs);
    println!(
        "  couverture : {}/{} sorts offensifs ({} %)",
        cov.modelled,
        cov.damaging,
        cov.percent()
    );
}
