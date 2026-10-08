use dofus_ruleset::{snapshot::Snapshot, Ruleset};
fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/iop.yaml")).unwrap();
    println!("avant fusion : {} lacunes", rs.data_gaps().len());
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-8.json")).unwrap();
    let report = rs.merge_snapshot(&snap);
    println!(
        "rempli {} valeurs, {} conflits, {} non apparies",
        report.filled.len(),
        report.conflicts.len(),
        report.unmatched.len()
    );
    for c in &report.conflicts {
        println!("   CONFLIT {}: {} vs {}", c.path, c.authored, c.snapshot);
    }
    for u in &report.unmatched {
        println!("   NON APPARIE {u}");
    }
    println!("apres fusion : {} lacunes", rs.data_gaps().len());
    for g in rs.data_gaps() {
        println!("   {g}");
    }
}
