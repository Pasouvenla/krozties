//! Stolen AP is not spendable AP.
//!
//! The Xelor's Ralentissement is written, correctly, as stealing one AP from a
//! Téléfragged target. In game the steal is opposed by the target's AP-removal
//! resistance, and endgame monsters carry enough that it never lands: crediting
//! that AP to the caster would spend action points that never arrive.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn xelor() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn solve(deck: &[&str]) -> Solution {
    let build = Build {
        name: "Xélor eau".into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 142,
            elements: [ElementStats {
                characteristic: 630,
                flat_damage: 56,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 100,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
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
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&xelor(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

/// The decision itself, so that flipping it back is a deliberate act and not a
/// side effect of some other change. Checked at compile time: a runtime
/// `assert!` on a constant is one clippy rightly calls out, and the compile
/// error is the louder signal anyway.
const _: () = assert!(
    !AP_THEFT_LANDS,
    "le vol de PA ne doit pas etre suppose reussir : la cible y resiste"
);

/// Only a spell that applies a Téléfrag may hand AP back, and only two.
///
/// Counting how often Ralentissement is cast would not do (two casts in a turn
/// are legal when both are paid for), and a loose ledger check lets a one-point
/// steal hide under the two points a Téléfrag refunds: what is checked is which
/// spells refund, not how much.
#[test]
fn only_telefrag_appliers_hand_ap_back() {
    // The spells whose `gain: telefrag` carries an `ap_bonus`, read off
    // xelor.yaml. Ralentissement is deliberately absent: it steals, and a
    // steal does not land.
    const REMBOURSENT: &[&str] = &[
        "permutation",
        "gelure",
        "compte_goutte",
        "petrification",
        "souvenir",
        "aiguille",
        "pendule",
        "refraction",
    ];
    let s = solve(&[
        "gelure",
        "ralentissement",
        "compte_goutte",
        "permutation",
        "clepsydre",
        "glas",
    ]);
    let mut vus = 0;
    for tour in &s.turns {
        for paire in tour.casts.windows(2) {
            let (avant, apres) = (&paire[0], &paire[1]);
            vus += 1;
            let paye = avant.ap_left - i16::from(apres.ap_cost);
            let rendu = apres.ap_left - paye;
            assert!(
                apres.ap_left >= 0,
                "{} laisse {} PA",
                apres.id,
                apres.ap_left
            );
            if rendu == 0 {
                continue;
            }
            assert!(
                REMBOURSENT.contains(&apres.id.as_str()),
                "tour {} : {} rend {rendu} PA sans appliquer de Téléfrag",
                tour.turn,
                apres.id
            );
            assert!(
                rendu <= 2,
                "tour {} : {} rend {rendu} PA, un Téléfrag n'en rend que deux",
                tour.turn,
                apres.id
            );
        }
    }
    assert!(vus > 5, "seulement {vus} enchaînements contrôlés");
}

/// The mechanism itself, checked on the AP ledger rather than on the shape of
/// the rotation: casting Ralentissement must cost exactly its price and give
/// nothing back.
///
/// A turn CAN legitimately spend more than twelve AP, because applying a
/// Téléfrag refunds two, so "no turn exceeds the budget" is not the invariant
/// and asserting it would fail on correct rotations. What must hold is
/// narrower: across one cast of this spell, the AP left drops by its cost and
/// by nothing else.
#[test]
fn casting_ralentissement_returns_no_ap() {
    // Un deck réduit, pour que le sort soit effectivement joué : sur un deck
    // complet le solveur lui préfère Clepsydre, ce qui est le bon choix mais
    // ne prouve rien sur le compteur de PA.
    let s = solve(&["ralentissement", "permutation"]);
    let mut vus = 0;
    for tour in &s.turns {
        for paire in tour.casts.windows(2) {
            let (avant, apres) = (&paire[0], &paire[1]);
            if apres.id != "ralentissement" {
                continue;
            }
            vus += 1;
            assert_eq!(
                apres.ap_left,
                avant.ap_left - i16::from(apres.ap_cost),
                "tour {} : Ralentissement a rendu {} PA",
                tour.turn,
                apres.ap_left - (avant.ap_left - i16::from(apres.ap_cost))
            );
        }
    }
    assert!(
        vus > 0,
        "aucun Ralentissement lancé : le test ne prouve rien"
    );
}
