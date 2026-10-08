//! The spell tooltip's damage table: what a spell hits for on this build, one
//! row per charge, normal and critical side by side. The rows must climb with
//! the charges, and the per-element split must add up to the total shown above
//! it: a split that does not sum is worse than no split.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn ruleset(fichier: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{fichier}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn moteur(rs: &Ruleset, deck: &[&str]) -> Engine {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 200,
            flat_crit_damage: 120,
            elements: [ElementStats {
                characteristic: 700,
                flat_damage: 90,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 30,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 4,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(rs, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

/// One row per charge, each strictly above the last. A table that does not
/// climb would say the charges are worthless, which is the opposite of why the
/// player stacks them.
#[test]
fn charges_climb_row_by_row() {
    let rs = ruleset("cra", 9);
    let table = moteur(&rs, &["fleche_punitive"]).damage_table("fleche_punitive");
    assert_eq!(table.len(), 3, "sans charge, une charge, deux charges");
    for (i, r) in table.iter().enumerate() {
        assert_eq!(r.charges as usize, i);
        assert!(r.normal.0 > 0 && r.normal.1 >= r.normal.0, "{r:?}");
    }
    assert!(
        table[1].normal.0 > table[0].normal.0 && table[2].normal.0 > table[1].normal.0,
        "chaque palier doit dépasser le précédent : {table:?}"
    );
    // The steps are worth 24 then 32, so the second gap is the larger. Checked
    // as a ratio because the absolute figures depend on the build above.
    let premier = (table[1].normal.0 - table[0].normal.0) as f64;
    let second = (table[2].normal.0 - table[1].normal.0) as f64;
    let ratio = second / premier;
    assert!(
        (ratio - 32.0 / 24.0).abs() < 0.02,
        "second écart sur le premier : {ratio:.3}, attendu {:.3}",
        32.0 / 24.0
    );
}

/// The critical column is only there when the spell can actually critical.
#[test]
fn the_critical_column_is_absent_when_the_spell_cannot_critical() {
    let rs = ruleset("cra", 9);
    let m = moteur(&rs, &["fleche_punitive"]);
    for r in m.damage_table("fleche_punitive") {
        let c = r.critical.expect("la Flèche Punitive critique");
        assert!(
            c.0 > r.normal.0,
            "un critique doit dépasser le normal: {r:?}"
        );
    }
}

/// The Xelor's Glas strikes in four elements from one roll. Its per-element
/// rows must add up to the total, normal AND critical, or the tooltip is
/// showing two contradictory figures at once.
#[test]
fn the_element_split_adds_up_to_the_total() {
    let rs = ruleset("xelor", 5);
    let table = moteur(&rs, &["glas", "aiguille"]).damage_table("glas");
    assert!(!table.is_empty(), "le Glas doit produire une table");
    for r in &table {
        assert_eq!(r.by_element.len(), 4, "quatre éléments: {r:?}");
        let (lo, hi) = r
            .by_element
            .iter()
            .fold((0i64, 0i64), |(a, b), e| (a + e.normal.0, b + e.normal.1));
        assert_eq!((lo, hi), r.normal, "la ventilation doit sommer: {r:?}");
        if let Some(total) = r.critical {
            let (clo, chi) = r.by_element.iter().fold((0i64, 0i64), |(a, b), e| {
                let c = e.critical.expect("chaque élément critique aussi");
                (a + c.0, b + c.1)
            });
            assert_eq!((clo, chi), total, "ventilation critique: {r:?}");
        }
    }
}

/// A single-element spell reports its one element, so the tooltip can tell
/// "one element" from "several" without guessing.
#[test]
fn a_single_element_spell_reports_one_element() {
    let rs = ruleset("cra", 9);
    let table = moteur(&rs, &["fleche_punitive"]).damage_table("fleche_punitive");
    for r in &table {
        assert_eq!(r.by_element.len(), 1, "{r:?}");
        assert_eq!(r.by_element[0].normal, r.normal);
    }
}

/// A spell whose damage comes from the state it applies has no table of its
/// own. The Xelor's Aiguille carries no line: it poisons at turn start and
/// fires when a Téléfrag is consumed, and a table would print "Dégâts 0 - 0"
/// under a spell that hits.
#[test]
fn a_spell_without_lines_of_its_own_has_no_table() {
    let rs = ruleset("xelor", 5);
    let m = moteur(&rs, &["glas", "aiguille"]);
    assert!(
        m.damage_table("aiguille").is_empty(),
        "l'Aiguille n'a pas de jet propre, elle ne doit pas produire de table"
    );
    assert!(!m.damage_table("glas").is_empty(), "le Glas en a un, lui");
}

/// The target is taken to be above half health, always: the only reading that
/// makes a rotation comparable across fights. The Sram's Attaque Mortelle
/// carries both readings in the data, 43-48 above half health and 54-60 below,
/// 52-58 then 65-72 on a critical; the ratio is 1.25 in both pairs, one hit at
/// two regimes.
///
/// Checked on a build with no flat damage, no percentages and no resistance,
/// where the pipeline reduces to `roll x (100 + carac + power) / 100`, so the
/// two candidate lines are 86-96 and 108-120 and cannot be confused.
#[test]
fn a_spell_stronger_below_half_health_uses_the_healthy_target_line() {
    let rs = ruleset("sram", 4);
    let build = Build {
        name: "banc nu".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 100,
                flat_damage: 0,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["attaque_mortelle".into()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 2,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let m = Engine::new(&rs, build, sc).unwrap_or_else(|e| panic!("{e}"));
    let table = m.damage_table("attaque_mortelle");
    assert_eq!(table.len(), 1, "pas de charge sur ce sort: {table:?}");
    assert_eq!(
        table[0].normal,
        (86, 96),
        "la ligne d'une cible au-dessus de 50 % de vie est 43-48, pas 54-60"
    );
    assert_eq!(
        table[0].critical,
        Some((104, 116)),
        "et son critique est 52-58, pas 65-72"
    );
}
