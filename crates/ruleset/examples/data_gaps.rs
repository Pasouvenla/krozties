fn main() {
    let rs = dofus_ruleset::Ruleset::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/rulesets/xelor.yaml"
    ))
    .unwrap();
    for g in rs.data_gaps() {
        println!("  {g}");
    }
}
