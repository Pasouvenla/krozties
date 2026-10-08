//! Effects whose magnitude differs on a critical hit.
//!
//! A cast either crits or it does not. An effect like Crâ's Tirs Puissants
//! (250 Puissance normally, 300 on a critical) has one value, not an average,
//! and averaging the two would invent a bonus the game never grants. So the rule
//! is a threshold: at 78% critical or more the critical figure is used, below it
//! the normal one. Damage lines are untouched, since they carry both ranges and
//! the pipeline weights them by the real rate.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn cra() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/cra.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-9.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn total_with_crit(bonus: i32) -> i64 {
    let build = Build {
        name: "Crâ".into(),
        profile: DamageProfile {
            power: 100,
            flat_crit_damage: 50,
            elements: [ElementStats {
                characteristic: 500,
                flat_damage: 50,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: bonus,
        modifiers: vec![],
        deck: ["tirs_puissants", "fleche_punitive", "fleche_glacee"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
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
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&cra(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .total
        .0
}

/// The ruleset has to actually carry both magnitudes, or the rule has nothing
/// to choose between.
#[test]
fn the_two_magnitudes_are_both_recorded() {
    let rs = cra();
    let ressource = rs
        .resources
        .iter()
        .find(|r| r.id == "tirs_puissants")
        .expect("tirs_puissants");
    let modifier = ressource
        .modifies_damage
        .first()
        .expect("a damage modifier");
    match modifier {
        dofus_ruleset::DamageModifier::Characteristic {
            amount,
            critical_amount,
            element,
            ..
        } => {
            assert_eq!(amount.known().copied(), Some(250));
            // Les Tirs Puissants donnent de la PUISSANCE, qui n'a pas
            // d'élément et lève donc les cinq lignes. Un élément écrit ici en
            // ferait un vol de caractéristique, qui n'en lèverait qu'une.
            assert_eq!(
                *element, None,
                "la Puissance ne connaît pas d'élément, contrairement à un vol"
            );
            assert_eq!(
                *critical_amount,
                Some(300),
                "the critical figure is in the game data and has to reach the ruleset"
            );
        }
        other => panic!("unexpected modifier {other:?}"),
    }
}

/// Below the threshold the normal figure is used, above it the critical one,
/// and the difference has to show up in the damage.
#[test]
fn the_threshold_decides_which_magnitude_applies() {
    // Tirs Puissants has a low base rate, so a small build bonus stays well
    // under the threshold and a large one goes over.
    let bas = total_with_crit(0);
    let haut = total_with_crit(100);
    assert!(
        haut > bas,
        "a build that always crits must get the larger buff: {haut} against {bas}"
    );
}

/// Where the threshold itself is checked: `seuil_critique` in the engine, on
/// the rule rather than on a rotation total. Totals move with the critical rate
/// anyway, because the rate also weights the damage lines, so they cannot tell
/// whether the MAGNITUDE changed.
#[test]
fn the_rule_itself_is_unit_tested_next_to_it() {}

const CLASSES: &[(&str, u32)] = &[
    ("feca", 1),
    ("osamodas", 2),
    ("enutrof", 3),
    ("sram", 4),
    ("xelor", 5),
    ("ecaflip", 6),
    ("eniripsa", 7),
    ("iop", 8),
    ("cra", 9),
    ("sadida", 10),
    ("sacrieur", 11),
    ("pandawa", 12),
    ("roublard", 13),
    ("zobal", 14),
    ("steamer", 15),
    ("eliotrope", 16),
    ("huppermage", 17),
    ("ouginak", 18),
    ("forgelance", 20),
];

/// Les buffs portent leur valeur critique quand la donnée en donne une autre,
/// et une valeur normale prise dans la fourchette normale : Puissance (effet
/// 138), taux critique (115), Dommages Critiques (418), Résistance Critique
/// retirée (421) et Dommages Poussée (414). Ainsi le Kraps (28 à 30, 34 en
/// critique), le Yams (3 à 18 %, 21 %), les Tirs Puissants du Crâ (15 %, 17 %),
/// la Griffe Joueuse (31 à 35, 42), la Puissance du Iop (300, 350 en critique).
/// Le contrôle lit la donnée des dix-neuf classes : un buff ajouté plus tard ne
/// perdra pas ses valeurs en silence.
#[test]
fn les_buffs_critiques_portent_leur_valeur_critique() {
    use dofus_ruleset::{DamageModifier, Effect};
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut vus = Vec::new();
    for (classe, breed) in CLASSES {
        let mut rs = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
            .unwrap_or_else(|e| panic!("{e}"));
        let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
        rs.merge_snapshot(&snap);
        let brut: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(format!("{root}/data/snapshots/breed-{breed}.json"))
                .unwrap(),
        )
        .unwrap();
        // (plus petite valeur normale, plus grande, valeur critique s'il y en a
        // une) d'un effet.
        let valeurs = |dofusdb_id: u32, effet: u64| -> Option<(i64, i64, Option<i64>)> {
            let niveau = brut["spells"]
                .as_array()?
                .iter()
                .find(|s| s["id"].as_u64() == Some(u64::from(dofusdb_id)))?["levels"]
                .as_array()?
                .last()?
                .clone();
            let trouver = |liste: &str| {
                niveau[liste]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|e| e["id"].as_u64() == Some(effet))
                    .map(|e| (e["dice_num"].as_i64().unwrap_or(0), e["dice_side"].as_i64().unwrap_or(0)))
            };
            let (min, cote) = trouver("other_effects")?;
            let critique = trouver("critical_other_effects").map(|(c, _)| c);
            Some((min, if cote == 0 { min } else { cote }, critique))
        };
        for r in &rs.resources {
            for m in &r.modifies_damage {
                let (effet, normal, ecrit) = match m {
                    DamageModifier::Characteristic {
                        amount,
                        critical_amount,
                        element: None,
                        per_target: false,
                        per_mp_used: false,
                        ..
                    } => (138, amount.known().copied(), *critical_amount),
                    DamageModifier::CriticalRate {
                        percent,
                        critical_percent,
                    } => (115, percent.known().copied(), *critical_percent),
                    DamageModifier::CriticalDamage {
                        amount,
                        critical_amount,
                    } => (418, amount.known().copied(), *critical_amount),
                    DamageModifier::CriticalResistance {
                        amount,
                        critical_amount,
                    } => (421, amount.known().copied(), *critical_amount),
                    DamageModifier::PushDamage {
                        amount,
                        critical_amount,
                    } => (414, amount.known().copied(), *critical_amount),
                    _ => continue,
                };
                let poseurs = rs.spells.iter().flat_map(|s| s.alternatives()).filter(|s| {
                    s.effects
                        .iter()
                        .any(|e| matches!(e, Effect::Gain { resource, .. } if *resource == r.id))
                });
                for s in poseurs {
                    let Some((min, max, critique)) = s.dofusdb_id.and_then(|id| valeurs(id, effet)) else {
                        continue;
                    };
                    if let Some(n) = normal {
                        assert!(
                            (min..=max).contains(&i64::from(n)),
                            "{classe}/{} : {n} en normal, la donnée dit {min} à {max}",
                            r.id
                        );
                    }
                    let Some(critique) = critique else { continue };
                    if critique == min && critique == max {
                        continue;
                    }
                    vus.push(format!("{classe}/{}", r.id));
                    assert_eq!(
                        ecrit,
                        Some(critique as i32),
                        "{classe}/{} : la donnée donne {critique} sur un critique, contre {min} à {max}",
                        r.id
                    );
                }
            }
        }
    }
    assert!(vus.len() >= 6, "seulement {vus:?} : le contrôle ne contrôle plus rien");
}

/// Un banc : `poser` pose un état, `frapper` le lit. Le taux de `frapper`
/// vaut 25 % de base plus les 60 du build, soit 85 % : au-dessus du seuil
/// sans être le critique sûr, là où le seuil décide seul.
fn banc_critique(modificateur: &str, frappe: &str) -> f64 {
    let rs = Ruleset::from_yaml(&format!(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: {{ fr: "Test", en: "Test" }}
resources:
  - id: etat
    scope: caster
    max: 1
    default: 0
    monotone: increasing
    modifies_damage:
      - {modificateur}
spells:
  - id: poser
    name: {{ fr: "Poser", en: "Set" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines: []
    effects:
      - {{ effect: gain, resource: etat }}
  - id: frapper
    name: {{ fr: "Frapper", en: "Hit" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 1
    crit: {{ base_rate: 25 }}
    lines:
      - {frappe}
    effects:
      - effect: damage
"#
    ))
    .unwrap_or_else(|e| panic!("{e}"));
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 100,
                flat_damage: 0,
            }; 5],
            ..Default::default()
        },
        base_ap: 2,
        base_mp: 3,
        crit_bonus_percent: 60,
        modifiers: vec![],
        deck: vec!["poser".into(), "frapper".into()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 1,
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
    let sol = Engine::new(&rs, build, sc).unwrap_or_else(|e| panic!("{e}")).solve();
    sol.turns[0]
        .casts
        .iter()
        .find(|c| c.id == "frapper")
        .map(|c| c.damage.as_f64())
        .unwrap_or_else(|| panic!("{sol}"))
}

/// Des Dommages Critiques qui ne valent rien en normal et 100 en critique : à
/// 85 %, la valeur critique. Le jet vaut 20 (10 doublés par la
/// caractéristique), et un critique 120 : 0,15 x 20 + 0,85 x 120 = 105.
#[test]
fn les_dommages_critiques_prennent_leur_valeur_critique_au_seuil() {
    let d = banc_critique(
        "{ kind: critical_damage, amount: 0, critical_amount: 100 }",
        "{ element: water, critical: [10, 10], normal: [10, 10] }",
    );
    assert!((d - 105.0).abs() < 0.01, "{d}");
}

/// Un taux critique qui ne vaut rien en normal et 50 % en critique : à 85 %,
/// la valeur critique, qui porte le sort à 100 %. Un critique vaut 80 (40
/// doublés), un coup normal 20 : 80 à coup sûr, contre 0,15 x 20 + 0,85 x 80 =
/// 71 sans elle.
#[test]
fn le_taux_critique_prend_sa_valeur_critique_au_seuil() {
    let d = banc_critique(
        "{ kind: critical_rate, percent: 0, critical_percent: 50 }",
        "{ element: water, critical: [40, 40], normal: [10, 10] }",
    );
    assert!((d - 80.0).abs() < 0.01, "{d}");
}
