//! Coverage is measured, never claimed.
//!
//! Nothing here blocks an incomplete class. These tests make its incompleteness
//! a number computed from the snapshot, so that it cannot drift away from what
//! the interface tells the player.

use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn load(name: &str, breed: u32) -> (Ruleset, Snapshot) {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{name}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    (rs, snap)
}

/// Xelor references every one of its damaging spells. That is the weaker of the
/// two claims, and the one this project used to make on its own.
#[test]
fn xelor_references_every_damaging_spell() {
    let (rs, snap) = load("xelor", 5);
    let c = snap.coverage(&rs);
    assert!(
        c.missing.is_empty(),
        "Xelor referenced every damaging spell and must keep doing so. Missing: {:?}",
        c.missing
    );
    assert_eq!(c.percent(), 100);
    assert!(
        c.unknown.is_empty(),
        "spells the snapshot does not know, usually a mistyped dofusdb_id: {:?}",
        c.unknown
    );
}

/// Presence and completeness are two figures, and this test keeps them apart:
/// referencing a spell imports its numbers, not what it does. It runs on a class
/// that still has a gap, the Ecaflip, whose Rekop and Tromperie draw at random.
#[test]
fn presence_is_not_completeness() {
    let (rs, snap) = load("ecaflip", 6);
    let c = snap.coverage(&rs);
    assert_eq!(c.modelled, c.damaging, "this test assumes full presence");
    assert!(
        c.fully_modelled < c.modelled,
        "l'Ecaflip porte des trous déclarés, il ne peut pas être complet"
    );
    assert!(
        !c.is_complete(),
        "a class with {} incomplete spells must not report itself complete",
        c.partial.len()
    );
    assert!(
        c.percent_complete() < c.percent(),
        "the honest figure ({} %) has to sit below the presence figure ({} %)",
        c.percent_complete(),
        c.percent()
    );
    // Naming the incomplete spells is what makes the gap workable rather than
    // merely deplorable.
    assert_eq!(c.partial.len(), c.modelled - c.fully_modelled);
    assert!(c.partial.iter().all(|(nom, n)| !nom.is_empty() && *n > 0));
    // Sorted worst first: that is where the work starts.
    assert!(c.partial.windows(2).all(|w| w[0].1 >= w[1].1));
}

/// Une classe annoncée complète l'est en résolvant ses questions, non en les
/// effaçant : ce qui a été tranché reste écrit en `assumptions`, et le test exige
/// ces déclarations.
#[test]
fn a_complete_class_still_declares_what_it_decided() {
    let (rs, snap) = load("xelor", 5);
    let c = snap.coverage(&rs);
    assert!(
        c.is_complete(),
        "le Xélor doit être complet : {} sort(s) partiel(s) {:?}",
        c.partial.len(),
        c.partial
    );
    assert_eq!(c.fully_modelled, 25);

    let declarations = rs
        .spells
        .iter()
        .filter(|s| !s.assumptions.is_empty())
        .count();
    assert!(
        declarations >= 13,
        "seuls {declarations} sorts déclarent ce qui a été décidé : les trous          ont été effacés plutôt que résolus"
    );
    // Les quatre points tranchés en jeu laissent chacun leur trace.
    for (id, mot) in [
        ("aiguille", "aucun dégât"),
        ("fletrissement", "trois fois"),
        // Seule la première des deux téléportations engendre.
        ("pendule", "la PREMIÈRE"),
        // Sa condition se lit à la fin du tour, ce qui oblige à réappliquer
        // un Téléfrag après lui.
        ("gousset", "FIN du tour"),
    ] {
        let s = rs.spells.iter().find(|s| s.id == id).expect(id);
        assert!(
            s.assumptions.iter().any(|a| a.contains(mot)),
            "{id} ne dit plus ce qui a été décidé sur « {mot} » : {:?}",
            s.assumptions
        );
    }
}

/// A spell absent from a ruleset must be named, not merely counted. The test
/// removes one on purpose, so that it keeps asserting something once every class
/// is complete.
#[test]
fn a_missing_spell_is_named_not_just_counted() {
    let (mut rs, snap) = load("iop", 8);
    let retire = rs
        .spells
        .iter()
        .position(|s| !s.lines.is_empty())
        .expect("a damaging spell to remove");
    let nom_retire = rs.spells[retire].name.fr.clone();
    rs.spells.remove(retire);

    let c = snap.coverage(&rs);
    assert_eq!(
        c.modelled,
        c.damaging - 1,
        "removing one damaging spell must lower the count by exactly one"
    );
    assert!(!c.is_complete());
    assert_eq!(
        c.missing.len(),
        c.damaging - c.modelled,
        "every unmodelled spell must be named, not just counted"
    );
    assert!(
        c.missing.contains(&nom_retire),
        "the removed spell {nom_retire:?} is not in the missing list: {:?}",
        c.missing
    );
    assert!(c.percent() < 100);
}

/// `percent` rounds down, so a class one spell short can never display 100 %.
#[test]
fn percent_never_rounds_up_to_a_hundred() {
    let (rs, snap) = load("xelor", 5);
    let mut c = snap.coverage(&rs);
    c.modelled -= 1;
    c.missing.push("un sort".into());
    assert!(!c.is_complete());
    assert!(
        c.percent() < 100,
        "{}/{} displayed as {} %",
        c.modelled,
        c.damaging,
        c.percent()
    );
}

/// A class with no snapshot data at all is not "complete", it is unknown.
#[test]
fn an_empty_coverage_is_not_complete() {
    let c = dofus_ruleset::snapshot::Coverage::default();
    assert!(!c.is_complete(), "zero of zero must not pass for done");
    assert_eq!(c.percent(), 0);
}

/// Casting constraints reach the ruleset from the snapshot: Sablier de Xélor can
/// only be cast in a straight line and needs no line of sight, and nothing else
/// in the ruleset says so.
#[test]
fn casting_constraints_merge_from_the_snapshot() {
    let (rs, _) = load("xelor", 5);
    let sablier = rs
        .spells
        .iter()
        .find(|s| s.id == "sablier")
        .expect("sablier");
    let cast = sablier
        .cast
        .expect("casting rules merged from the snapshot");
    assert!(cast.in_line, "Sablier only fires in a straight line");
    assert!(
        !cast.needs_line_of_sight,
        "Sablier reaches through obstacles, which is the whole point of saying so"
    );

    // A constraint every spell carries would tell a player nothing. These have
    // to actually separate one spell from another.
    let with_rules: Vec<_> = rs.spells.iter().filter_map(|s| s.cast).collect();
    assert_eq!(
        with_rules.len(),
        rs.spells.len(),
        "every modelled spell must get its constraints, not just some"
    );
    let in_line = with_rules.iter().filter(|c| c.in_line).count();
    assert!(
        in_line > 0 && in_line < with_rules.len(),
        "\"in line only\" is on {in_line} of {} spells: a constraint that is \
         either universal or absent carries no information",
        with_rules.len()
    );
    let blind = with_rules.iter().filter(|c| !c.needs_line_of_sight).count();
    assert!(
        blind > 0 && blind < with_rules.len(),
        "\"no line of sight\" is on {blind} of {}",
        with_rules.len()
    );
}

/// A decision is not a gap. The Sram's Attaque Mortelle carries 43-48 above half
/// health and 54-60 below; the target is taken to be healthy, so the second range
/// is left out on purpose, as an assumption rather than an open question.
#[test]
fn a_settled_choice_is_an_assumption_not_an_open_question() {
    let (rs, _) = load("sram", 4);
    let spell = rs
        .spells
        .iter()
        .find(|s| s.id == "attaque_mortelle")
        .expect("le sort doit exister");
    assert!(
        spell.open_questions.is_empty(),
        "rien n'est ouvert sur ce sort : {:?}",
        spell.open_questions
    );
    assert_eq!(spell.assumptions.len(), 1, "{:?}", spell.assumptions);
    let note = &spell.assumptions[0];
    assert!(
        note.contains("43-48") && note.contains("54-60"),
        "la note doit citer les deux fourchettes : {note}"
    );
    // Elle s'affiche au joueur : elle se rédige en français accentué.
    assert!(
        note.contains("moitié") && note.contains("supposée"),
        "texte destiné au joueur, donc accentué : {note}"
    );
    assert!(
        !rs.data_gaps()
            .iter()
            .any(|g| g.path.starts_with("attaque_mortelle")),
        "une décision prise ne doit plus compter comme un trou de données"
    );
}
