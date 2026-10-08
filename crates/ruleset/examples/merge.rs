use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn main() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml")).unwrap();
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();

    println!("before merge: {} data gaps", rs.data_gaps().len());
    let report = rs.merge_snapshot(&snap);

    println!("\nfilled ({}):", report.filled.len());
    for f in &report.filled {
        println!("   {f}");
    }
    println!("\nconflicts ({}):", report.conflicts.len());
    for c in &report.conflicts {
        println!(
            "   {}: authored {} vs snapshot {}",
            c.path, c.authored, c.snapshot
        );
    }
    println!("\nunmatched ({}):", report.unmatched.len());
    for u in &report.unmatched {
        println!("   {u}");
    }
    println!("\nafter merge: {} data gaps", rs.data_gaps().len());
    for g in rs.data_gaps() {
        println!("   {g}");
    }
}
