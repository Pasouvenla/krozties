//! Opener plus loop, instead of a fixed number of turns.
//!
//! A fixed horizon cannot answer "is it worth delaying this spell": on the last
//! turns of the window a delay is free, because the fight stops before the cost
//! lands. `steady_state` finds the cycle with the highest mean weight over the
//! between-turn graph (Karp, and Howard past Karp's bounds), then the best way
//! into it.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn load(name: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{name}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn iop_engine(horizon: u8) -> Engine {
    let build = Build {
        name: "Iop".into(),
        profile: DamageProfile {
            power: 150,
            flat_crit_damage: 150,
            elements: [ElementStats {
                characteristic: 850,
                flat_damage: 120,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 25,
        modifiers: vec![],
        deck: [
            "couperet",
            "rassemblement",
            "epee_destructrice",
            "epee_du_destin",
            "puissance",
            "sentence",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        horizon,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&load("iop", 8), build, sc).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_loop_is_found_and_it_repeats() {
    let steady = iop_engine(7).steady_state();
    assert!(!steady.cycle.is_empty(), "no cycle found at all");
    assert!(
        steady.cycle.len() <= 12,
        "a cycle of {} turns is not a rotation anybody can follow",
        steady.cycle.len()
    );
}

/// The advertised per-turn figure must be the cycle's own damage divided by its
/// own length. A number that does not match what the turns show is worse than
/// no number.
#[test]
fn the_advertised_average_matches_the_cycle_it_describes() {
    let steady = iop_engine(7).steady_state();
    let total: i64 = steady
        .cycle
        .iter()
        .map(|t| t.opening_damage.0 + t.damage.0)
        .sum();
    let (num, den) = steady.per_turn;
    assert_eq!(
        i128::from(total) * i128::from(den),
        i128::from(num) * i128::from(steady.cycle.len() as u32),
        "advertised {num}/{den} per turn, but the {} turns shown total {total}",
        steady.cycle.len()
    );
}

/// The steady state must be at least as good as any fixed horizon, since a
/// fixed horizon is one particular finite play and the cycle is the best
/// repeatable one. Anything less means the search missed a cycle.
#[test]
fn the_loop_beats_every_fixed_horizon() {
    let steady = iop_engine(7).steady_state();
    let per_turn = steady.per_turn_value();
    for horizon in [3u8, 5, 7, 9, 12] {
        let fixed = iop_engine(horizon).solve().total.as_f64() / f64::from(horizon);
        assert!(
            per_turn >= fixed - 1.0,
            "horizon {horizon} averages {fixed:.1} per turn, above the steady \
             state's {per_turn:.1}"
        );
    }
}

/// Puissance lasts three turns and cannot be recast for four, so a cycle that
/// keeps it up has to contain exactly one cast of it. This is the concrete
/// question a fixed horizon could not answer.
#[test]
fn the_cycle_maintains_the_temporary_buff() {
    let steady = iop_engine(7).steady_state();
    let casts: usize = steady
        .cycle
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == "puissance")
        .count();
    assert_eq!(
        casts,
        1,
        "a {}-turn cycle casting Puissance {casts} times: it lasts 3 turns and \
         recharges in 4, so exactly one upkeep per cycle is what maintaining it \
         means",
        steady.cycle.len()
    );
}

/// Xelor too, so the result is not an artefact of one class.
#[test]
fn xelor_also_settles_into_a_cycle() {
    let build = Build {
        name: "Xélor".into(),
        profile: DamageProfile {
            power: 270,
            flat_crit_damage: 173,
            elements: [ElementStats {
                characteristic: 400,
                flat_damage: 60,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 40,
        modifiers: vec![],
        deck: ["gelure", "engrenage", "refraction", "glas", "sablier"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 7,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![("telefrag_per_turn".into(), 3)],
        dominance: true,
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let steady = Engine::new(&load("xelor", 5), build, sc)
        .unwrap()
        .steady_state();
    assert!(!steady.cycle.is_empty());
    assert!(steady.per_turn_value() > 0.0);
}

/// Damage attributed to a state goes to the spell that applies it, not to the
/// one that consumes it: Aiguille applies a state, Réfraction consumes it and
/// collects the damage, and the player knows which spell they owe it to.
#[test]
fn a_states_damage_is_credited_to_the_spell_that_applies_it() {
    let build = Build {
        name: "Xélor".into(),
        profile: DamageProfile {
            power: 270,
            flat_crit_damage: 173,
            elements: [ElementStats {
                characteristic: 400,
                flat_damage: 60,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 40,
        modifiers: vec![],
        deck: ["gelure", "aiguille", "refraction", "engrenage"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 6,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![("telefrag_per_turn".into(), 3)],
        dominance: true,
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let solution = Engine::new(&load("xelor", 5), build, sc).unwrap().solve();

    let mut aiguille = 0i64;
    let mut attributed = 0i64;
    for turn in &solution.turns {
        for (source, damage) in &turn.opening_sources {
            attributed += damage.0;
            if source == "aiguille" {
                aiguille += damage.0;
            }
        }
        for cast in &turn.casts {
            let from_procs: i64 = cast.procs.iter().map(|(_, d)| d.0).sum();
            attributed += cast.damage.0;
            for (source, damage) in &cast.procs {
                if source == "aiguille" {
                    aiguille += damage.0;
                }
            }
            let _ = from_procs;
        }
    }

    assert!(
        aiguille > 0,
        "Aiguille applies a damaging state, so it cannot be worth nothing"
    );
    // Nothing invented and nothing lost: the parts add up to the whole.
    assert_eq!(
        attributed,
        solution
            .turns
            .iter()
            .map(|t| t.opening_damage.0 + t.damage.0)
            .sum::<i64>(),
        "the attribution must account for every point of damage, exactly once"
    );
}

/// A deck with no resources at all still has a cycle: every turn is the same.
/// The walk Karp hands back holds the starting vertex of each edge, so on a
/// graph where only parity separates two states, the cycle 0 → 1 → 0 leaves a
/// walk of [0, 1] where nothing repeats, and the cycle must not come out empty.
#[test]
fn a_deck_with_no_resources_still_cycles() {
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
        crit_bonus_percent: 20,
        modifiers: vec![],
        deck: [
            "fleche_cinglante",
            "fleche_perforante",
            "fleche_persecutrice",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 5,
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
    let steady = Engine::new(&load("cra", 9), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .steady_state();

    assert!(
        !steady.cycle.is_empty(),
        "a rotation advertised at {:.0} damage per turn over zero turns is not a rotation",
        steady.per_turn_value()
    );
    assert!(steady.per_turn_value() > 0.0);
    let total: i64 = steady
        .cycle
        .iter()
        .map(|t| t.opening_damage.0 + t.damage.0)
        .sum();
    let (num, den) = steady.per_turn;
    assert_eq!(
        i128::from(total) * i128::from(den),
        i128::from(num) * i128::from(steady.cycle.len() as u32),
    );
}

/// Au-delà du produit états × arêtes que Karp admet, Howard trouve la boucle :
/// ce deck est sous les 4 000 états mais au-delà des cent millions d'opérations
/// de Karp, et sa boucle doit valoir, tour par tour, ce qu'elle annonce.
#[test]
fn howard_prend_le_relais_au_dela_des_bornes_de_karp() {
    let build = Build {
        name: "Xélor".into(),
        profile: DamageProfile {
            power: 270,
            flat_crit_damage: 173,
            elements: [ElementStats {
                characteristic: 400,
                flat_damage: 60,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 40,
        modifiers: vec![],
        // Sept sorts : 2 910 états, donc SOUS les 4 000 admis, mais 121 559
        // arêtes, dont le produit dépasse les cent millions.
        deck: [
            "engrenage",
            "souvenir",
            "aiguille",
            "sablier",
            "petrification",
            "clepsydre",
            "glas",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 7,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![("telefrag_per_turn".into(), 3)],
        dominance: true,
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let steady = Engine::new(&load("xelor", 5), build, sc)
        .unwrap()
        .steady_state();
    // Les deux faits qui font de ce deck un cas pour Howard.
    assert!(
        steady.states < dofus_engine::Engine::MAX_LOOP_STATES,
        "le nombre d'états doit rester SOUS le plafond de Karp, sinon le test ne prouve rien"
    );
    assert!(
        steady.states * steady.edges > dofus_engine::Engine::MAX_LOOP_WORK,
        "et le produit doit le dépasser : {} états, {} arêtes",
        steady.states,
        steady.edges
    );
    assert_eq!(steady.gave_up, None, "Howard doit trouver la boucle");
    assert!(!steady.cycle.is_empty());
    let total: i64 = steady
        .cycle
        .iter()
        .map(|t| t.opening_damage.0 + t.damage.0)
        .sum();
    let (num, den) = steady.per_turn;
    assert_eq!(
        i128::from(total) * i128::from(den),
        i128::from(num) * i128::from(steady.cycle.len() as u32),
        "annoncé {num}/{den} par tour, pour {} tours valant {total}",
        steady.cycle.len()
    );
}

/// Howard et Karp trouvent la même moyenne, au point près, partout où Karp
/// répond : cinq sorts à dégâts par classe, les dix-neuf classes, sans les
/// mécaniques coûteuses. Cette égalité donne à Howard le droit de chercher seul
/// au-delà des bornes de Karp. Chaque boucle de Howard doit aussi valoir, tour
/// par tour, la moyenne qu'elle annonce.
#[test]
fn howard_trouve_la_meme_moyenne_que_karp() {
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
    let mut comparees = 0;
    for (classe, breed) in CLASSES {
        let rs = load(classe, *breed).sans_mecaniques_couteuses();
        let deck: Vec<String> = rs
            .spells
            .iter()
            .filter(|s| !s.lines.is_empty())
            .take(5)
            .map(|s| s.id.clone())
            .chain(
                (*classe == "zobal")
                    .then(|| "masque_de_l_intrepide".to_string()),
            )
            .collect();
        let build = Build {
            name: (*classe).into(),
            profile: DamageProfile {
                power: 150,
                flat_crit_damage: 100,
                elements: [ElementStats {
                    characteristic: 600,
                    flat_damage: 80,
                }; 5],
                ..Default::default()
            },
            base_ap: 12,
            base_mp: 3,
            crit_bonus_percent: 20,
            modifiers: vec![],
            deck,
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
            budgets: rs.budgets.iter().map(|b| (b.id.clone(), b.default)).collect(),
            dominance: true,
            prune_spells: true,
            mode: Mode::Expected,
            reach: Reach::Ranged,
        };
        let moteur = Engine::new(&rs, build, sc).unwrap_or_else(|e| panic!("{classe}: {e}"));
        let karp = moteur.steady_state();
        let howard = moteur.steady_state_howard();
        assert!(howard.gave_up.is_none(), "{classe} : Howard renonce");
        assert!(!howard.cycle.is_empty(), "{classe} : Howard sans boucle");
        let total: i64 = howard
            .cycle
            .iter()
            .map(|t| t.opening_damage.0 + t.damage.0)
            .sum();
        let (num, den) = howard.per_turn;
        assert_eq!(
            i128::from(total) * i128::from(den),
            i128::from(num) * i128::from(howard.cycle.len() as u32),
            "{classe} : Howard annonce {num}/{den} par tour pour {} tours valant {total}",
            howard.cycle.len()
        );
        if karp.gave_up.is_some() {
            continue;
        }
        let (kn, kd) = karp.per_turn;
        assert_eq!(
            i128::from(num) * i128::from(kd),
            i128::from(kn) * i128::from(den),
            "{classe} : Howard {num}/{den} contre Karp {kn}/{kd}"
        );
        comparees += 1;
    }
    assert!(comparees >= 15, "{comparees} classes seulement comparées");
}
