//! L'Aiguille du Xélor porte deux mécanismes. Le sort ne frappe pas lui-même :
//! il pose un état de deux tours qui fait deux choses distinctes :
//!
//!   - un poison au début du tour de la cible, sans condition, donc deux tics ;
//!   - une ligne de dégâts déclenchée en retirant le Téléfrag d'une cible sous
//!     Aiguille, une fois par tour, donc deux détentes.
//!
//! Un lancer peut donc produire quatre frappes.

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
        name: "Xélor".into(),
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
        budgets: vec![("telefrag_per_turn".into(), 3)],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&xelor(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

/// Le poison tombe au début du tour, sans qu'aucun sort ne soit lancé.
#[test]
fn the_poison_ticks_at_turn_start() {
    let s = solve(&["aiguille", "engrenage", "refraction"]);
    let tics = s
        .turns
        .iter()
        .filter(|t| t.opening.iter().any(|n| n.contains("aiguille")))
        .count();
    assert!(
        tics >= 2,
        "au moins deux tics de poison attendus : {:#?}",
        s.turns.iter().map(|t| &t.opening).collect::<Vec<_>>()
    );
}

/// Et la détente répond au retrait du Téléfrag, séparément du poison.
#[test]
fn the_trigger_answers_the_telefrag_being_spent() {
    let s = solve(&["aiguille", "engrenage", "refraction"]);
    let procs = s
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .flat_map(|c| c.procs.iter())
        .filter(|(source, _)| source == "aiguille")
        .count();
    assert!(
        procs >= 2,
        "au moins deux détentes attendues, {procs} vue(s)"
    );
}

/// Les deux ensemble : retirer l'un des deux coûte la moitié du sort.
///
/// Contrôle par comparaison, sur le même deck sans l'Aiguille : l'écart doit
/// couvrir les poisons ET les détentes, pas l'un des deux.
#[test]
fn both_mechanisms_contribute() {
    let avec = solve(&["aiguille", "engrenage", "refraction"]);
    let sans = solve(&["engrenage", "refraction"]);
    let ouverture: f64 = avec.turns.iter().map(|t| t.opening_damage.as_f64()).sum();
    let detentes: f64 = avec
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .flat_map(|c| c.procs.iter())
        .filter(|(source, _)| source == "aiguille")
        .map(|(_, d)| d.as_f64())
        .sum();
    assert!(ouverture > 0.0, "les poisons doivent peser : {ouverture}");
    assert!(detentes > 0.0, "les détentes doivent peser : {detentes}");
    assert!(
        avec.total > sans.total,
        "l'Aiguille doit rapporter : {} contre {}",
        avec.total.as_f64(),
        sans.total.as_f64()
    );
}
