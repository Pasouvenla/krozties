//! Parity against the multi-element prototype, which needs three things a
//! first schema does not get right by accident:
//!
//!   * a delayed payload whose countdown other events shorten;
//!   * a state that deals damage on its own, both at the start of a turn and
//!     when the target loses a resource;
//!   * a spell that raises its own base damage for later casts, which a
//!     resource with a duration plus a `while_resource` bonus on the spell's
//!     own line expresses.
//!
//! The first two are `Effect::Schedule` and `ResourceDef::while_present`.

use std::time::Instant;

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn xelor() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();
    assert!(rs.merge_snapshot(&snap).is_clean());
    au_glas_de_la_3_6(&mut rs);
    rs
}

/// Le prototype a été écrit sur la 3.6.12 : son Glas coûte 4 PA, se relance
/// tous les 3 tours, frappe 6 (7 en critique) par ligne et gagne 6 par
/// Téléfrag consommé. La note de sortie 3.7 l'a changé ; la comparaison n'a de
/// sens que sur les chiffres du prototype.
fn au_glas_de_la_3_6(rs: &mut Ruleset) {
    let glas = rs.spells.iter_mut().find(|s| s.id == "glas").expect("le Glas");
    glas.ap_cost.base = 4;
    glas.cooldown_turns = 3;
    for l in &mut glas.lines {
        l.normal = dofus_ruleset::Maybe::Known((6, 6));
        l.critical = dofus_ruleset::Maybe::Known((7, 7));
        for b in &mut l.base_bonus {
            if let dofus_ruleset::BaseBonus::PerResource { amount, .. } = b {
                *amount = 6;
            }
        }
    }
}

/// "Xelor multi do crit", 12 AP. No Reve Nebuleux, so the final multiplier is a
/// constant 1.155 and there is no turn parity to reason about.
fn multi_build() -> Build {
    Build {
        name: "Xelor multi (do crit)".into(),
        profile: DamageProfile {
            power: 185,
            flat_crit_damage: 151,
            elements: [
                ElementStats {
                    characteristic: 840,
                    flat_damage: 155,
                }, // fire
                ElementStats {
                    characteristic: 690,
                    flat_damage: 138,
                }, // earth
                ElementStats {
                    characteristic: 680,
                    flat_damage: 124,
                }, // air
                ElementStats {
                    characteristic: 405,
                    flat_damage: 108,
                }, // water,
                ElementStats::default(),
            ],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![
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
            "souvenir",
            "engrenage",
            "poussiere",
            "refraction",
            "horloge",
            "petrification",
            "ralentissement",
            "sablier",
            "aiguille",
            "glas",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    }
}

fn scenario(telefrag_per_turn: u8, mode: Mode) -> Scenario {
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
        mode,
        reach: Reach::Ranged,
        prune_spells: true,
    }
}

/// The prototype's own optimum, kept for the record rather than as a target.
/// The prototype treated every generator as unconditional, which its first turn
/// cannot support: Poussière only teleports targets that are already Telefrag,
/// and the three spells that send a target "to its previous position" need a
/// previous position.
fn prototype_objective(cap: u8) -> f64 {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/prototype_multi_solve.json"
    ))
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    v[cap.to_string()]["objective"].as_f64().unwrap()
}

fn solve(cap: u8) -> (Solution, f64) {
    let engine = Engine::new(
        &xelor(),
        multi_build(),
        scenario(cap, Mode::PrototypeCompat),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let started = Instant::now();
    let solution = engine.solve();
    (solution, started.elapsed().as_secs_f64())
}

/// The engine stays close to the prototype, within a signed band, because
/// known differences pull in opposite directions.
///
/// It loses on turn one, where the prototype's Telefrag generators generate
/// nothing real. It gains on relaunch intervals: `cooldown` is the interval
/// between two casts (`minCastInterval`), so Glas at interval 3 cast on turn 3
/// comes back on turn 6, where the prototype casts it on turn 7.
///
/// Deux autres différences tirent vers le bas, le prototype ayant tort les deux
/// fois : il crédite le PA que vole le Ralentissement (voir `AP_THEFT_LANDS`),
/// et il fait empoisonner l'Aiguille à chaque tour, alors qu'elle pose un état
/// qui répond à la perte d'un Téléfrag, une fois par tour et deux fois en tout.
/// D'où une bande signée, de quinze pour cent en dessous à trois au-dessus : un
/// écart qui se referme vers zéro signale qu'une correction a été défaite.
#[test]
fn the_engine_tracks_the_prototype_within_a_few_percent() {
    for cap in [2u8, 3] {
        let (solution, seconds) = solve(cap);
        let prototype = prototype_objective(cap);
        let total = solution.total.as_f64();
        println!("{solution}\n\n{seconds:.3}s, budget {cap}");
        let ecart = (total - prototype) / prototype;
        assert!(
            (-0.15..=0.03).contains(&ecart),
            "budget {cap} : {total} contre {prototype} au prototype, soit \
             {:.1}% d'écart. La bande attendue va de -15% à +3%, les quatre \
             différences connues réunies. Un écart qui se REFERME vers zéro \
             est le signe qu'une correction a été défaite.",
            ecart * 100.0
        );
    }
}

/// Turn one cannot generate a Telefrag from nothing. Only Permutation and
/// Engrenage teleport unconditionally; everything else needs either an existing
/// Telefrag or a previous position to send the target back to.
#[test]
fn the_opening_turn_uses_an_unconditional_generator() {
    let (solution, _) = solve(3);
    let premier = &solution.turns[0];
    let ouvreurs: Vec<&str> = premier.casts.iter().map(|c| c.id.as_str()).collect();
    let inconditionnels = ["permutation", "engrenage"];
    let position = ouvreurs.iter().position(|id| inconditionnels.contains(id));
    let genere_avant = ouvreurs
        .iter()
        .take(position.unwrap_or(0))
        .any(|id| ["gelure", "souvenir", "compte_goutte", "poussiere"].contains(id));
    assert!(
        !genere_avant,
        "un générateur conditionnel est lancé avant tout déplacement : {ouvreurs:?}"
    );
}

/// The delayed payload has to actually land. If schedules were silently dropped
/// the totals above could still be reached by casting something else, so pin the
/// mechanic itself rather than only the number it contributes.
#[test]
fn the_delayed_payload_resolves() {
    let (solution, _) = solve(3);
    // Le libellé suit le nom du sort, pas son identifiant interne.
    let interessant = |n: &String| {
        let n = n.to_lowercase();
        n.contains("sablier") || n.contains("charge déclenchée") || n.contains("retardé")
    };
    let mentions: usize = solution
        .turns
        .iter()
        .map(|t| {
            t.opening.iter().filter(|n| interessant(n)).count()
                + t.casts
                    .iter()
                    .flat_map(|c| c.notes.iter())
                    .filter(|n| interessant(n))
                    .count()
        })
        .sum();
    assert!(
        mentions > 0,
        "aucun déclenchement de Sablier dans la rotation"
    );
}

/// Des dégâts tombent avant qu'un seul sort ne soit lancé, ce que la recherche
/// à l'intérieur d'un tour ne voit jamais : le tic de début de tour de la
/// Sentence du Iop.
#[test]
fn the_state_ticks_before_anything_is_cast() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/iop.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-8.json")).unwrap();
    rs.merge_snapshot(&snap);

    let build = Build {
        name: "Iop".into(),
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
        deck: vec!["sentence".into(), "pression".into()],
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
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let solution = Engine::new(&rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve();
    let ticking = solution
        .turns
        .iter()
        .filter(|t| t.opening_damage > dofus_damage::Damage::ZERO)
        .count();
    assert!(ticking > 0, "aucun tour ne commence par des dégâts d'état");
}

/// Pareto pruning is lossless only if the monotonicity the ruleset declares is
/// true, and a false declaration returns a suboptimal rotation in silence
/// rather than failing. So it is checked rather than trusted: the same instance
/// solved with and without the filter must reach the same optimum.
///
/// Run at a shorter horizon than the parity tests, because the prune-free
/// search is what the pruning exists to avoid.
#[test]
fn dominance_changes_nothing() {
    let rs = xelor();
    for cap in [2u8, 3] {
        for horizon in [3u8, 4, 5] {
            let mut with = scenario(cap, Mode::PrototypeCompat);
            with.horizon = horizon;
            let mut without = with.clone();
            without.dominance = false;

            let pruned = Engine::new(&rs, multi_build(), with).unwrap().solve();
            let exhaustive = Engine::new(&rs, multi_build(), without).unwrap().solve();

            assert_eq!(
                pruned.total, exhaustive.total,
                "horizon {horizon}, budget {cap}: élagué {} contre exhaustif {}",
                pruned.total, exhaustive.total
            );
            assert!(pruned.inter_turn_states <= exhaustive.inter_turn_states);
        }
    }
}
