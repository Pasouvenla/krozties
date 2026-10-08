//! The Xelor ruleset is the phase 0 test of the schema itself: if a generic
//! format cannot express the class the prototypes already solve, nothing later
//! is worth building.

use dofus_ruleset::*;

fn xelor() -> Ruleset {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/rulesets/xelor.yaml"
    );
    Ruleset::load(path).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn loads_and_validates() {
    let rs = xelor();
    assert_eq!(rs.class, "xelor");
    // One ruleset serves every elemental path of the class: the path lives in
    // the build's deck, not here. Both prototypes' spells coexist in this file.
    for id in [
        "gelure",
        "clepsydre",
        "glas",
        "sablier",
        "aiguille",
        "horloge",
    ] {
        assert!(rs.spells.iter().any(|s| s.id == id), "missing spell `{id}`");
    }
    for id in [
        "telefrag",
        "glas_stacks",
        "petrification",
        "horloge",
        "aiguille",
    ] {
        assert!(
            rs.resources.iter().any(|r| r.id == id),
            "missing resource `{id}`"
        );
    }
    assert!(rs.problems().is_empty(), "{:?}", rs.problems());
}

/// Aux chiffres de la 3.7 : relance de 2 tours, +3 par Téléfrag consommé.
#[test]
fn glas_carries_four_lines_scaling_off_one_resource() {
    let rs = xelor();
    let glas = rs.spells.iter().find(|s| s.id == "glas").unwrap();
    assert_eq!(glas.lines.len(), 4);
    for line in &glas.lines {
        match line.base_bonus.as_slice() {
            [BaseBonus::PerResource {
                resource, amount, ..
            }] => {
                assert_eq!(resource, "glas_stacks");
                assert_eq!(*amount, 3);
            }
            other => panic!("unexpected base bonus: {other:?}"),
        }
    }
    assert_eq!(glas.cooldown_turns, 2);
    assert_eq!(glas.casts_per_turn, 1);
}

/// The ordering the prototype gets right by accident of statement order, and
/// that any reordering would silently break.
#[test]
fn clepsydre_carries_ap_before_it_consumes_the_resource_it_reads() {
    let rs = xelor();
    let clep = rs.spells.iter().find(|s| s.id == "clepsydre").unwrap();
    let carry = clep
        .effects
        .iter()
        .position(|e| matches!(e, Effect::CarryAp { .. }))
        .expect("clepsydre carries AP");
    let consume = clep
        .effects
        .iter()
        .position(|e| matches!(e, Effect::Consume { .. }))
        .expect("clepsydre consumes a telefrag");
    assert!(
        carry < consume,
        "the AP carry reads the telefrag this cast removes; it must come first"
    );
}

#[test]
fn unknown_data_is_reported_rather_than_guessed() {
    let rs = xelor();
    let gaps = rs.data_gaps();

    // Every spell is missing its base critical rate: the prototypes assumed a
    // permanent critical hit, so the figure was never needed.
    let crit_gaps = gaps
        .iter()
        .filter(|g| g.path.ends_with(".crit.base_rate"))
        .count();
    assert_eq!(
        crit_gaps,
        rs.spells.iter().filter(|s| s.crit.can_crit).count()
    );

    // Glas is the one spell whose non-critical range is stated, and only
    // because the prototype stated it.
    assert!(!gaps
        .iter()
        .any(|g| g.path.starts_with("glas.") && g.path.contains(".normal")));

    // Petrification's reduction per stack is NOT a gap: it was tested in game.
    assert!(!gaps.iter().any(|g| g.path.contains("per_stack")));

    // Sablier's delay reduction is stated too, for the same reason, even though
    // no tooltip and no datamine carries it.
    assert!(!gaps.iter().any(|g| g.path.contains("accelerated_by")));

    // Open questions are gaps of a different kind: not a missing magnitude but an
    // unsettled question about what a spell does.
    let questions: Vec<&str> = gaps
        .iter()
        .filter(|g| g.path.ends_with(".open_question"))
        .map(|g| g.path.as_str())
        .collect();

    // L'Aiguille : le lancer n'inflige rien, et les deux lignes de la donnée
    // sont ses deux déclenchements, écrits en `assumptions` (une décision,
    // non une incertitude).
    assert!(
        !questions.iter().any(|p| p.starts_with("aiguille.")),
        "les questions de l'Aiguille sont tranchees : {questions:?}"
    );
    let aiguille = rs
        .spells
        .iter()
        .find(|s| s.id == "aiguille")
        .expect("le sort doit exister");
    // Vérifié sur le contenu, pas sur le compte : une autre hypothèse peut
    // s'ajouter.
    for attendu in ["n'inflige aucun d", "relance"] {
        assert!(
            aiguille.assumptions.iter().any(|a| a.contains(attendu)),
            "l'hypothese contenant « {attendu} » a disparu : {:?}",
            aiguille.assumptions
        );
    }

    // Ce qu'un sort ne modélise pas est écrit, jamais tu : une question ouverte
    // est une incertitude, une `assumption` une décision prise.
    let declarations = rs
        .spells
        .iter()
        .filter(|s| !s.open_questions.is_empty() || !s.assumptions.is_empty())
        .count();
    assert!(
        declarations >= 12,
        "seuls {declarations} sorts déclarent leur périmètre : les autres se taisent"
    );
    assert!(
        questions.len() <= 8,
        "trop de questions ouvertes pour une classe dite finalisée : {questions:?}"
    );
}

#[test]
fn dangling_references_are_rejected() {
    let bad = r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources: []
spells:
  - id: broken
    name: { fr: "Cassé", en: "Broken" }
    ap_cost: { base: 2 }
    casts_per_turn: 1
    crit: { base_rate: 15 }
    effects:
      - effect: consume
        resource: does_not_exist
"#;
    let err = Ruleset::from_yaml(bad).unwrap_err();
    let text = err.to_string();
    assert!(text.contains("does_not_exist"), "{text}");
}

// ---------------------------------------------------------------------------
// Vendored numbers
// ---------------------------------------------------------------------------

use dofus_ruleset::snapshot::Snapshot;

fn snapshot() -> Snapshot {
    Snapshot::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/snapshots/breed-5.json"
    ))
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Every authored figure agrees with the snapshot, to the unit, across seven
/// spells: AP costs, critical ranges, Glas's non-critical base of 6. This is
/// also the patch detector: when a rebalance moves a number, it fails with the
/// field and both values.
#[test]
fn authored_numbers_agree_with_the_vendored_snapshot() {
    let mut rs = xelor();
    let report = rs.merge_snapshot(&snapshot());
    assert!(
        report.conflicts.is_empty(),
        "authored numbers disagree with DofusDB: {:#?}",
        report.conflicts
    );
    assert!(report.unmatched.is_empty(), "{:#?}", report.unmatched);
}

/// The thirteen values Xelor could not supply are exactly the thirteen the
/// snapshot fills: seven base critical rates and six non-critical ranges.
#[test]
fn the_snapshot_closes_every_missing_magnitude() {
    let mut rs = xelor();
    let before = rs.data_gaps().len();
    let report = rs.merge_snapshot(&snapshot());
    assert!(
        report.filled.len() > 20,
        "only filled {}",
        report.filled.len()
    );

    // What survives the merge is exactly what a datamine cannot answer: two
    // questions about what Aiguille actually does. Every missing NUMBER is gone.
    let left = rs.data_gaps();
    assert!(
        left.iter().all(|g| g.path.ends_with(".open_question")),
        "a magnitude is still missing: {left:?}"
    );
    assert!(left.len() < before);
}

/// Critical rates are per spell, not per build: 5% on Gelure and Glas, 10% on
/// Compte-goutte, 15% on Petrification and Clepsydre. No build-wide figure
/// substitutes for them.
#[test]
fn critical_rates_vary_by_spell() {
    let mut rs = xelor();
    rs.merge_snapshot(&snapshot());
    let rate = |id: &str| -> u8 {
        *rs.spells
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .crit
            .base_rate
            .known()
            .unwrap()
    };
    assert_eq!(rate("gelure"), 5);
    assert_eq!(rate("compte_goutte"), 10);
    assert_eq!(rate("petrification"), 15);
    assert_eq!(rate("glas"), 5);
}
