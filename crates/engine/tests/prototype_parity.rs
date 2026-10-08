//! Parity against the hand-written prototype solver that a generic engine
//! reading declarative mechanics replaces. The comparison runs in
//! [`Mode::PrototypeCompat`], under the prototype's own assumptions: every hit
//! critical, midpoint of the range, no resistance. Nobody should optimise under
//! them, but a parity test has to hold both sides to one standard.

use std::time::Instant;

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

fn xelor() -> Ruleset {
    Ruleset::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/rulesets/xelor.yaml"
    ))
    .unwrap_or_else(|e| panic!("{e}"))
}

/// "Xelor Water (Chance)", the build the water prototype was written for.
fn water_build() -> Build {
    Build {
        name: "Xelor eau (Chance)".into(),
        profile: DamageProfile {
            power: 110,
            flat_crit_damage: 167,
            elements: [
                ElementStats {
                    characteristic: 250,
                    flat_damage: 69,
                }, // fire
                ElementStats {
                    characteristic: 260,
                    flat_damage: 65,
                }, // earth
                ElementStats {
                    characteristic: 320,
                    flat_damage: 74,
                }, // air
                ElementStats {
                    characteristic: 1118,
                    flat_damage: 131,
                }, // water,
                ElementStats::default(),
            ],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![
            // Invisible in spell data, lives entirely in the build, and it is
            // what pushes the finisher's effective cooldown from 3 turns to 4.
            BuildModifier {
                id: "reve_nebuleux".into(),
                percent: 120,
                when: When::OddTurns,
            },
            BuildModifier {
                id: "bleu_turquoise".into(),
                percent: 110,
                when: When::Always,
            },
            BuildModifier {
                id: "pourpre_profond".into(),
                percent: 105,
                when: When::Always,
            },
        ],
        deck: [
            "gelure",
            "ralentissement",
            "compte_goutte",
            "permutation",
            "petrification",
            "clepsydre",
            "glas",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    }
}

fn scenario(telefrag_per_turn: u8) -> Scenario {
    Scenario {
        poussees_bloquees: false,
        horizon: 7,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![("telefrag_per_turn".into(), telefrag_per_turn)],
        dominance: true,
        mode: Mode::PrototypeCompat,
        reach: Reach::Ranged,
        prune_spells: true,
    }
}

fn prototype_objective(cap: u8) -> f64 {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/prototype_water_solve.json"
    ))
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v[cap.to_string()]["objective"].as_f64().unwrap()
}

fn solve_with(cap: u8) -> (Solution, f64) {
    let rs = xelor();
    let engine = Engine::new(&rs, water_build(), scenario(cap)).unwrap_or_else(|e| panic!("{e}"));
    let started = Instant::now();
    let solution = engine.solve();
    let seconds = started.elapsed().as_secs_f64();
    (solution, seconds)
}

/// The engine sits within a signed band below the prototype, because known
/// differences pull it both ways.
///
/// The prototype treated every generator as unconditional. Three of Xelor's
/// send a target "to its previous position", which needs a previous position,
/// and Poussière only teleports targets that are already Telefrag: on turn one
/// only Permutation, an outright swap, generates. That costs the engine about
/// 0.9% of the seven-turn total.
///
/// `cooldown` is the interval between two casts (`minCastInterval`): a spell at
/// interval 3 cast on turn 3 comes back on turn 6, where the prototype casts
/// Glas on turn 7. That hands the engine casts the prototype never got.
///
/// The largest difference pulls the engine down: the prototype credits the
/// caster with the AP that Ralentissement steals, which never arrives in game
/// (see `AP_THEFT_LANDS`), about 9.5% of the seven-turn total at budget 2.
///
/// Hence a band with a known sign, from fifteen to five percent below the
/// prototype. A gap closing back toward zero means the phantom AP returned.
#[test]
fn the_engine_tracks_the_prototype_within_a_few_percent() {
    let mut ecarts = Vec::new();
    for cap in [2u8, 3] {
        let (solution, seconds) = solve_with(cap);
        let prototype = prototype_objective(cap);
        let total = solution.total.as_f64();
        println!("{solution}\n\n{seconds:.3}s, budget {cap}");
        let ecart = (total - prototype) / prototype;
        println!("budget {cap} : écart {:.1}%", ecart * 100.0);
        ecarts.push((cap, total, prototype, ecart));
    }
    for (cap, total, prototype, ecart) in ecarts {
        // La borne haute est à -5 % et non à zéro : c'est elle qui fait le
        // travail. Le PA volé, s'il revenait, ramènerait l'écart près de zéro.
        assert!(
            (-0.15..=-0.05).contains(&ecart),
            "budget {cap} : {total} contre {prototype}, soit {:.1}% d'écart. \
             La bande attendue va de -15% à -5%, les trois différences connues \
             réunies. Un écart qui se REFERME vers zéro est le signe que le PA \
             volé est revenu ; un écart qui se creuse, que le moteur a perdu \
             autre chose.",
            ecart * 100.0
        );
    }
}

/// The first cast of turn one has to be something that can actually generate.
#[test]
fn the_opening_cast_can_generate() {
    let (solution, _) = solve_with(3);
    let ouvreurs: Vec<&str> = solution.turns[0]
        .casts
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    let conditionnels = ["gelure", "compte_goutte", "poussiere", "souvenir"];
    let permutation = ouvreurs.iter().position(|id| *id == "permutation");
    let avant = ouvreurs.iter().take(permutation.unwrap_or(0));
    assert!(
        !avant.clone().any(|id| conditionnels.contains(id)),
        "un générateur conditionnel précède tout déplacement : {ouvreurs:?}"
    );
}

/// A looser Telefrag budget can never make the optimum worse: it only removes a
/// constraint. Cheap to check and it catches a whole class of pruning error.
#[test]
fn a_looser_budget_never_lowers_the_optimum() {
    let two = solve_with(2).0.total;
    let three = solve_with(3).0.total;
    assert!(
        three >= two,
        "three telefrags scored {three} against two at {two}"
    );
}

/// Expected mode must refuse to run rather than invent the numbers it lacks.
#[test]
fn expected_mode_refuses_to_guess_missing_data() {
    let rs = xelor();
    let mut sc = scenario(2);
    sc.mode = Mode::Expected;
    let text = match Engine::new(&rs, water_build(), sc) {
        Ok(_) => panic!("the engine should refuse to solve without the data it needs"),
        Err(e) => e.to_string(),
    };
    assert!(text.contains("crit.base_rate"), "{text}");
    assert!(text.contains("normal"), "{text}");
}

// ---------------------------------------------------------------------------
// The real objective
// ---------------------------------------------------------------------------

use dofus_ruleset::snapshot::Snapshot;

fn xelor_with_numbers() -> Ruleset {
    let mut rs = xelor();
    let snap = Snapshot::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/snapshots/breed-5.json"
    ))
    .unwrap();
    assert!(rs.merge_snapshot(&snap).is_clean());
    rs
}

fn expected_total(crit_bonus: i32, resistance: Resistance) -> f64 {
    let rs = xelor_with_numbers();
    let mut build = water_build();
    build.crit_bonus_percent = crit_bonus;
    let mut sc = scenario(3);
    sc.mode = Mode::Expected;
    sc.resistance = resistance;
    Engine::new(&rs, build, sc).unwrap().solve().total.as_f64()
}

/// Assuming every hit crits is an upper bound on the expectation, never a
/// forecast. On this build it overstates the total by 45% at no critical bonus
/// and still by 12% at a bonus of +60%.
#[test]
fn assuming_criticals_bounds_the_expectation_from_above() {
    let compat = solve_with(3).0.total.as_f64();
    for bonus in [0, 20, 40, 60, 80] {
        let expected = expected_total(bonus, Resistance::NONE);
        assert!(
            expected < compat,
            "expected damage at +{bonus}% crit ({expected}) should stay under the \
             always-critical bound ({compat})"
        );
    }
}

#[test]
fn more_critical_rate_never_lowers_expected_damage() {
    let mut previous = f64::MIN;
    for bonus in [0, 20, 40, 60] {
        let total = expected_total(bonus, Resistance::NONE);
        assert!(
            total > previous,
            "+{bonus}% crit scored {total} after {previous}"
        );
        previous = total;
    }
}

#[test]
fn resistance_never_raises_damage() {
    let bare = expected_total(20, Resistance::NONE);
    let flat = expected_total(
        20,
        Resistance {
            percent: [0; 5],
            flat: [20; 5],
            critical: 0,
            ..Default::default()
        },
    );
    let percent = expected_total(
        20,
        Resistance {
            percent: [50, 10, 10, 10, 0],
            flat: [0; 5],
            critical: 0,
            ..Default::default()
        },
    );
    assert!(flat < bare, "{flat} vs {bare}");
    assert!(percent < bare, "{percent} vs {bare}");
}
