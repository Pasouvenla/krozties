//! Calibration suite. The expected values come from
//! `tests/fixtures/prototype_damage.json`, extracted from the two Python
//! prototypes: a drift in any stage of the pipeline fails here.

use dofus_damage::*;
use serde_json::Value;

fn fixture() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/prototype_damage.json"
    );
    let raw =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read fixture {path}: {e}"));
    serde_json::from_str(&raw).expect("fixture is not valid JSON")
}

fn element(name: &str) -> Element {
    match name {
        "Fire" => Element::Fire,
        "Earth" => Element::Earth,
        "Air" => Element::Air,
        "Water" => Element::Water,
        other => panic!("unknown element {other}"),
    }
}

fn profile_from(v: &Value) -> DamageProfile {
    let mut elements = [ElementStats::default(); 5];
    for (name, stats) in v["elements"].as_object().unwrap() {
        elements[element(name).index()] = ElementStats {
            characteristic: stats["characteristic"].as_i64().unwrap() as i32,
            flat_damage: stats["flat_damage"].as_i64().unwrap() as i32,
        };
    }
    DamageProfile {
        power: v["power"].as_i64().unwrap() as i32,
        flat_crit_damage: v["flat_crit_damage"].as_i64().unwrap() as i32,
        elements,
        ..Default::default()
    }
}

fn percents(v: &Value) -> Vec<u32> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_u64().unwrap() as u32)
        .collect()
}

/// Le multiplicateur composé comme le prototype le faisait, chaque bonus
/// multipliant le suivant (le jeu les additionne, voir
/// `les_dommages_finaux_s_additionnent`). Il sert à comparer tout le reste du
/// calcul au prototype.
fn comme_le_prototype(v: &Value) -> FinalMultiplier {
    percents(v).into_iter().fold(FinalMultiplier::NEUTRAL, FinalMultiplier::times)
}

/// The four elemental slots Glas hits. NOT `Element::ALL`, which also holds
/// Neutral: Neutral is a fifth element with its own resistance, but no Glas
/// line lands on it, and summing it in would invent a fifth hit.
const QUATRE: [Element; 4] = [Element::Fire, Element::Earth, Element::Air, Element::Water];

/// Glas hits all four elements from a single base value, so it is the sharpest
/// available test of the per-line rules: flat elemental damage and flat
/// critical damage both have to be applied four times, not once.
fn glas_total(stacks: i32, crit: bool, profile: &DamageProfile, mult: FinalMultiplier) -> i64 {
    let base = if crit { 7 } else { 6 } + 6 * stacks.min(6);
    QUATRE
        .iter()
        .map(|&e| resolve(base, e, profile, mult, crit, &Resistance::NONE))
        .sum()
}

/// Values where the prototype's floating point floors one point too low and
/// exact integer arithmetic does not: the Fire component of Glas at 3 stacks on
/// the water build, where `25 * (1 + 360 / 100)` is exactly 115 but
/// 114.99999999999999 as a double. Whether the game gives 115 is not verified.
const KNOWN_FLOAT_DIVERGENCES: &[(&str, i64)] =
    &[("water/Glas@3/odd", 1), ("water/Glas@3/even", 1)];

/// Assert against the prototype, allowing exactly the divergences above and no
/// others. Fails if a new divergence appears OR if a recorded one disappears.
fn assert_against_prototype(label: &str, got: i64, prototype: i64) {
    let expected_delta = KNOWN_FLOAT_DIVERGENCES
        .iter()
        .find(|(l, _)| *l == label)
        .map(|(_, d)| *d)
        .unwrap_or(0);
    assert_eq!(
        got - prototype,
        expected_delta,
        "{label}: exact arithmetic gives {got}, prototype gives {prototype}, \
         expected a difference of {expected_delta}"
    );
}

// ---------------------------------------------------------------------------
// Multi-element build: constant x1.155, no turn parity
// ---------------------------------------------------------------------------

#[test]
fn multi_build_glas_matches_dofusbook() {
    let fx = fixture();
    let build = &fx["multi"];
    let profile = profile_from(&build["profile"]);
    let mult = comme_le_prototype(&build["profile"]["multiplier_percent"]);

    for stacks in 0..=6 {
        let expected = build["glas_by_stacks"][stacks.to_string()]
            .as_i64()
            .unwrap();
        assert_eq!(
            glas_total(stacks, true, &profile, mult),
            expected,
            "Glas at {stacks} stacks"
        );
    }

    // The figure quoted in the brief, and the one that anchored the whole
    // calibration. If this ever moves, the pipeline is broken.
    assert_eq!(glas_total(6, true, &profile, mult), 3164);
}

#[test]
fn multi_build_spell_ranges_match() {
    let fx = fixture();
    let build = &fx["multi"];
    let profile = profile_from(&build["profile"]);
    let mult = comme_le_prototype(&build["profile"]["multiplier_percent"]);

    for (name, spell) in build["spells"].as_object().unwrap() {
        let base = spell["base_crit_range"].as_array().unwrap();
        let line = SpellLine {
            element: element(spell["element"].as_str().unwrap()),
            normal: (
                base[0].as_i64().unwrap() as i32,
                base[1].as_i64().unwrap() as i32,
            ),
            critical: (
                base[0].as_i64().unwrap() as i32,
                base[1].as_i64().unwrap() as i32,
            ),
        };
        let want = spell["crit_range"].as_array().unwrap();
        let got = range(&line, &profile, mult, true, &Resistance::NONE);
        assert_eq!(
            got,
            (want[0].as_i64().unwrap(), want[1].as_i64().unwrap()),
            "{name} critical range"
        );
    }
}

// ---------------------------------------------------------------------------
// Water build: Reve Nebuleux makes odd turns x1.386 and even turns x1.155
// ---------------------------------------------------------------------------

#[test]
fn water_build_parity_multipliers_match() {
    let fx = fixture();
    let build = &fx["water"];
    let profile = profile_from(&build["profile"]);
    let odd =
        comme_le_prototype(&build["profile"]["multiplier_percent_odd"]);
    let even =
        comme_le_prototype(&build["profile"]["multiplier_percent_even"]);

    for (name, spell) in build["spells"].as_object().unwrap() {
        let base = spell["base_crit_range"].as_array().unwrap();
        let line = SpellLine {
            element: element(spell["element"].as_str().unwrap()),
            normal: (
                base[0].as_i64().unwrap() as i32,
                base[1].as_i64().unwrap() as i32,
            ),
            critical: (
                base[0].as_i64().unwrap() as i32,
                base[1].as_i64().unwrap() as i32,
            ),
        };
        for (label, mult) in [("odd", odd), ("even", even)] {
            let want = spell[format!("crit_range_{label}")].as_array().unwrap();
            let got = range(&line, &profile, mult, true, &Resistance::NONE);
            assert_eq!(
                got,
                (want[0].as_i64().unwrap(), want[1].as_i64().unwrap()),
                "{name} critical range on {label} turns"
            );
        }
    }

    for (label, mult) in [("odd", odd), ("even", even)] {
        for stacks in 0..=6 {
            let prototype = build[format!("glas_by_stacks_{label}")][stacks.to_string()]
                .as_i64()
                .unwrap();
            assert_against_prototype(
                &format!("water/Glas@{stacks}/{label}"),
                glas_total(stacks, true, &profile, mult),
                prototype,
            );
        }
    }
}

/// Les % de dommages finaux s'additionnent. Sur un mur de Roublard : 5792 sans
/// rien ; 7530 avec +10 % et +20 % (×1,30, quand le produit ×1,32 donnerait
/// 7645) ; 5792 avec +10 % et −10 % (le produit donnerait 5734).
#[test]
fn les_dommages_finaux_s_additionnent() {
    assert_eq!(FinalMultiplier::from_percents(&[110, 120]).apply(10_000), 13_000);
    assert_eq!(FinalMultiplier::from_percents(&[110, 90]).apply(10_000), 10_000);
    assert_eq!(FinalMultiplier::from_percents(&[120, 110, 105]).apply(10_000), 13_500);
    // Et ils forment un facteur à part, que les autres multiplient : le +7 %
    // aux sorts du Kaboom donnait ×1,064 par-dessus les +30 % finaux.
    let finaux = FinalMultiplier::from_percents(&[110, 120]);
    assert_eq!(finaux.and(FinalMultiplier::NEUTRAL.times_bonus(7)).apply(10_000), 13_910);
}

/// A product that is exact in decimal need not be exact in binary. Integer
/// arithmetic makes the question moot; this pins the boundary case that a
/// floating-point implementation can get wrong.
#[test]
fn multiplier_is_exact_at_integer_boundaries() {
    // 1.20 * 1.10 * 1.05 = 1.386 exactly, by the multiplicative path the
    // other factors still take. 1000 * 1.386 = 1386.0 exactly; a float
    // implementation can produce 1385.9999999999998 here and floor to 1385.
    let m = FinalMultiplier::NEUTRAL.times(120).times(110).times(105);
    assert_eq!(m.apply(1000), 1386);
    assert_eq!(FinalMultiplier::NEUTRAL.times(110).times(105).apply(2000), 2310);
    // Et la somme des finaux, exacte elle aussi : 1,35.
    assert_eq!(FinalMultiplier::from_percents(&[120, 110, 105]).apply(1000), 1350);
    assert_eq!(FinalMultiplier::NEUTRAL.apply(777), 777);
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

#[test]
fn expectation_lies_between_the_normal_and_critical_values() {
    let profile = DamageProfile {
        power: 185,
        flat_crit_damage: 151,
        elements: [ElementStats {
            characteristic: 840,
            flat_damage: 155,
        }; 5],
        ..Default::default()
    };
    let mult = FinalMultiplier::from_percents(&[110, 105]);
    let line = SpellLine {
        element: Element::Fire,
        normal: (26, 29),
        critical: (28, 30),
    };

    let never = expected_line(&line, &profile, mult, CritRate::NEVER, &Resistance::NONE);
    let always = expected_line(&line, &profile, mult, CritRate::ALWAYS, &Resistance::NONE);
    let half = expected_line(
        &line,
        &profile,
        mult,
        CritRate::from_percent(50),
        &Resistance::NONE,
    );

    assert!(
        never < half && half < always,
        "{never:?} {half:?} {always:?}"
    );
    // Halfway means halfway. Each of the three is rounded to the nearest
    // fixed-point unit independently, so allow one unit of 1e-4 damage.
    assert!(
        (half.0 * 2 - (never.0 + always.0)).abs() <= 1,
        "halfway crit rate is not halfway: {never:?} {half:?} {always:?}"
    );
}

/// Flat resistance is subtracted per line, so it hurts a four-line spell four
/// times as much as a one-line spell of the same total damage. This is the
/// concrete reason a solver that ignores resistance overvalues Glas.
#[test]
fn flat_resistance_penalises_multi_line_spells() {
    let profile = DamageProfile {
        power: 185,
        flat_crit_damage: 151,
        elements: [ElementStats {
            characteristic: 840,
            flat_damage: 155,
        }; 5],
        ..Default::default()
    };
    let mult = FinalMultiplier::from_percents(&[110, 105]);
    let res = Resistance {
        percent: [0; 5],
        flat: [20; 5],
        critical: 0,
        ..Default::default()
    };

    let four_line: i64 = QUATRE
        .iter()
        .map(|&e| resolve(43, e, &profile, mult, true, &res))
        .sum();
    let four_line_bare: i64 = QUATRE
        .iter()
        .map(|&e| resolve(43, e, &profile, mult, true, &Resistance::NONE))
        .sum();

    assert_eq!(
        four_line_bare - four_line,
        80,
        "four lines pay the flat four times"
    );
}

#[test]
fn resistance_never_produces_negative_damage() {
    let profile = DamageProfile {
        power: 0,
        flat_crit_damage: 0,
        elements: [ElementStats::default(); 5],
        ..Default::default()
    };
    let res = Resistance {
        percent: [50; 5],
        flat: [9999; 5],
        critical: 0,
        ..Default::default()
    };
    assert_eq!(
        resolve(
            10,
            Element::Air,
            &profile,
            FinalMultiplier::NEUTRAL,
            false,
            &res
        ),
        0
    );
}

#[test]
fn damage_is_monotone_in_base_roll() {
    let profile = DamageProfile {
        power: 110,
        flat_crit_damage: 167,
        elements: [ElementStats {
            characteristic: 1118,
            flat_damage: 131,
        }; 5],
        ..Default::default()
    };
    let mult = FinalMultiplier::from_percents(&[120, 110, 105]);
    let mut previous = i64::MIN;
    for base in 0..200 {
        let d = resolve(
            base,
            Element::Water,
            &profile,
            mult,
            true,
            &Resistance::NONE,
        );
        assert!(
            d >= previous,
            "damage went down from base {} to {}",
            base - 1,
            base
        );
        previous = d;
    }
}
// ---------------------------------------------------------------------------
// Cast-context percentages: % spells, % weapons, % melee, % ranged
// ---------------------------------------------------------------------------

/// Iop's Pression, worked by hand: 26-30 Earth (31-36 critical), 800 Strength
/// and 100 Power (x10), +100 Earth damage, +50 critical damage, 5% spell, 2%
/// ranged and 10% final damage, cast at range. Expected 424-471 normal, 541-600
/// critical: 360 x 1.05 x 1.02 x 1.10 = 424.116 floors to 424, where flooring
/// after each percentage would give 423.
fn pression() -> (DamageProfile, SpellLine) {
    let mut elements = [ElementStats::default(); 5];
    elements[Element::Earth.index()] = ElementStats {
        characteristic: 800,
        flat_damage: 100,
    };
    let profile = DamageProfile {
        power: 100,
        flat_crit_damage: 50,
        elements,
        percent_spell: 5,
        percent_ranged: 2,
        ..DamageProfile::default()
    };
    let line = SpellLine {
        element: Element::Earth,
        normal: (26, 30),
        critical: (31, 36),
    };
    (profile, line)
}

#[test]
fn cast_context_percentages_match_the_worked_example() {
    let (profile, line) = pression();
    let mult = FinalMultiplier::for_cast(&profile, &Resistance::NONE, CastContext::SPELL_RANGED)
        .times(110); // 10% final damage
    assert_eq!(
        range(&line, &profile, mult, false, &Resistance::NONE),
        (424, 471),
        "normal range"
    );
    assert_eq!(
        range(&line, &profile, mult, true, &Resistance::NONE),
        (541, 600),
        "critical range"
    );
}

/// The stages before the percentages, checked in isolation so a failure above
/// says WHICH stage moved.
#[test]
fn worked_example_intermediate_stages() {
    let (profile, line) = pression();
    let neutral = FinalMultiplier::NEUTRAL;
    // Characteristic and Power: 26 x 10 = 260, plus 100 Earth damage.
    assert_eq!(
        range(&line, &profile, neutral, false, &Resistance::NONE),
        (360, 400),
        "characteristic scaling then flat elemental damage"
    );
    // On a critical hit the base roll differs AND flat critical damage lands.
    assert_eq!(
        range(&line, &profile, neutral, true, &Resistance::NONE),
        (460, 510),
        "critical roll plus flat critical damage"
    );
}

/// Flooring once, not between each percentage. Stated as its own test because
/// it is the single assertion that distinguishes the right formula from a
/// plausible wrong one.
#[test]
fn percentages_are_floored_once_not_between_each_step() {
    let (profile, line) = pression();
    let mult = FinalMultiplier::for_cast(&profile, &Resistance::NONE, CastContext::SPELL_RANGED)
        .times(110);
    let once = range(&line, &profile, mult, false, &Resistance::NONE).0;

    let stepwise = {
        let raw = 360i64;
        let a = raw * 105 / 100;
        let b = a * 102 / 100;
        b * 110 / 100
    };
    assert_eq!(once, 424);
    assert_eq!(stepwise, 423, "the wrong way rounds down one unit lower");
    assert_ne!(once, stepwise);
}

/// Melee and ranged are exclusive: a cast takes one or the other, never both.
#[test]
fn melee_and_ranged_never_apply_together() {
    let mut elements = [ElementStats::default(); 5];
    elements[Element::Earth.index()] = ElementStats {
        characteristic: 900,
        flat_damage: 0,
    };
    let profile = DamageProfile {
        elements,
        percent_melee: 50,
        percent_ranged: 10,
        ..DamageProfile::default()
    };
    let line = SpellLine {
        element: Element::Earth,
        normal: (100, 100),
        critical: (100, 100),
    };
    let at_contact =
        FinalMultiplier::for_cast(&profile, &Resistance::NONE, CastContext::SPELL_MELEE);
    let at_range =
        FinalMultiplier::for_cast(&profile, &Resistance::NONE, CastContext::SPELL_RANGED);
    assert_eq!(
        range(&line, &profile, at_contact, false, &Resistance::NONE).0,
        1500,
        "1000 raw x 1.50"
    );
    assert_eq!(
        range(&line, &profile, at_range, false, &Resistance::NONE).0,
        1100,
        "1000 raw x 1.10, NOT x1.50 x1.10"
    );
}

/// A build carrying none of these percentages must produce exactly the numbers
/// it produced before they existed. This is what makes the change safe for the
/// whole existing calibration suite.
#[test]
fn absent_percentages_change_nothing() {
    let mut elements = [ElementStats::default(); 5];
    elements[Element::Fire.index()] = ElementStats {
        characteristic: 743,
        flat_damage: 87,
    };
    let profile = DamageProfile {
        power: 30,
        flat_crit_damage: 61,
        elements,
        ..DamageProfile::default()
    };
    for ctx in [
        CastContext::SPELL_MELEE,
        CastContext::SPELL_RANGED,
        CastContext::WEAPON_MELEE,
        CastContext::WEAPON_RANGED,
    ] {
        assert_eq!(
            FinalMultiplier::for_cast(&profile, &Resistance::NONE, ctx),
            FinalMultiplier::NEUTRAL,
            "{ctx:?} must contribute nothing when the build carries no percentages"
        );
    }
}

/// Domain resistances mirror the attacker's percentages and reduce.
#[test]
fn domain_resistance_reduces_multiplicatively() {
    let profile = DamageProfile {
        percent_spell: 20,
        ..DamageProfile::default()
    };
    let resistance = Resistance {
        percent_spell: 10,
        ..Resistance::NONE
    };
    let m = FinalMultiplier::for_cast(&profile, &resistance, CastContext::SPELL_RANGED);
    // 1000 x 1.20 x 0.90 = 1080, not 1000 x 1.10 = 1100.
    assert_eq!(m.apply(1000), 1080);
}
