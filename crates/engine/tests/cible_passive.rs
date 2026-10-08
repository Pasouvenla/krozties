//! La cible de la rotation est passive : elle ne fait rien à son tour, comme un
//! poutch. Quatre sorts de l'Ecaflip ajoutent un coup selon ce que fait la
//! cible à son tour : l'Infortune si elle ne fait pas de coup critique, le
//! Destin d'Ecaflip si elle en fait un, le Toupet si elle déplace quelqu'un, le
//! Péril si elle est soignée. Une cible passive déclenche donc l'Infortune à
//! chaque fois, et jamais les trois autres, écrits en hypothèses.

use dofus_damage::{
    expected_line, CritRate, DamageProfile, Element, ElementStats, FinalMultiplier, Resistance,
    SpellLine,
};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn regles() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/ecaflip.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-6.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn profil() -> DamageProfile {
    DamageProfile {
        power: 170,
        flat_crit_damage: 0,
        elements: [ElementStats {
            characteristic: 630,
            flat_damage: 56,
        }; 5],
        ..Default::default()
    }
}

fn solve(rs: &Ruleset, deck: &[&str], horizon: u8) -> Solution {
    let build = Build {
        name: "ecaflip".into(),
        profile: profil(),
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| (*s).to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: vec![],
        horizon,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

/// L'Infortune frappe une fois, à la fin du tour de la cible qui suit le
/// lancer : 31-33 Eau, critique 38, au taux de 20 % du sort.
///
/// Le sort est mis en relance pour ne partir qu'au tour 1 sur trois tours :
/// un seul coup attendu, au début du tour 2. L'état ne dure qu'un tour, et un
/// coup de plus au tour 3 se verrait.
#[test]
fn l_infortune_frappe_a_la_fin_du_tour_de_la_cible() {
    let mut rs = regles();
    rs.spells
        .iter_mut()
        .find(|s| s.id == "infortune")
        .expect("l'Infortune est au fichier")
        .cooldown_turns = 3;
    let sol = solve(&rs, &["infortune"], 3);
    let coup = expected_line(
        &SpellLine {
            element: Element::Water,
            normal: (31, 33),
            critical: (38, 38),
        },
        &profil(),
        FinalMultiplier::NEUTRAL,
        CritRate::from_percent(20),
        &Resistance::NONE,
    )
    .as_f64();
    let v: Vec<f64> = sol
        .turns
        .iter()
        .map(|t| {
            t.opening_sources
                .iter()
                .filter(|(s, _)| s == "infortune")
                .map(|(_, d)| d.as_f64())
                .sum()
        })
        .collect();
    assert_eq!(v.len(), 3);
    assert!(v[0] == 0.0 && v[2] == 0.0, "un seul coup, au tour 2 : {v:?}");
    assert!(
        (v[1] - coup).abs() < 0.01,
        "{:.2} attendu au tour 2, {v:?} mesuré",
        coup
    );
}
