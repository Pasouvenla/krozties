//! Un sort peut être indisponible au début du combat, et pas seulement entre
//! deux lancers.
//!
//! `minCastInterval` est le délai entre deux lancers ; `initialCooldown` le
//! nombre de tours à attendre avant le premier. Dix-neuf sorts du parc en
//! portent un, tous à 1, dont Elding du Forgelance et le Pacte Bestial de
//! l'Osamodas, qui donne deux PA pour trois tours : le proposer un tour trop tôt
//! décalerait tout le budget de la rotation.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn forgelance() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/forgelance.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-20.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn tours(rs: &Ruleset, deck: &[&str]) -> Vec<Vec<String>> {
    let build = Build {
        name: "Forgelance".into(),
        profile: DamageProfile {
            power: 100,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 400,
                flat_damage: 30,
            }; 5],
            ..Default::default()
        },
        base_ap: 10,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon: 4,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
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
        .turns
        .iter()
        .map(|t| t.casts.iter().map(|c| c.id.clone()).collect())
        .collect()
}

/// Elding porte un délai initial de 1 : il ne part pas au premier tour.
#[test]
fn elding_ne_part_pas_au_premier_tour() {
    let rs = forgelance();
    let elding = rs
        .spells
        .iter()
        .find(|s| s.id == "elding")
        .expect("elding est au fichier");
    assert_eq!(
        elding.initial_cooldown, 1,
        "le délai initial d'Elding vient de la donnée, pas de ce test"
    );

    // Un deck où Elding est le seul à planter la Lance : sans lui, aucun sort
    // qui l'exige ne peut partir. C'est ce qui rend son absence VISIBLE.
    let t = tours(&rs, &["elding", "octave", "jormun"]);
    assert!(
        !t[0].contains(&"elding".to_string()),
        "Elding est indisponible au premier tour, or il y est : {:?}",
        t[0]
    );
    assert!(
        t[1..]
            .iter()
            .any(|tour| tour.contains(&"elding".to_string())),
        "Elding doit revenir dès le second tour : {t:?}"
    );
}

/// Sans le délai, il repart au premier tour : la garde mord bien. Le test
/// ci-dessus passerait aussi si Elding n'était jamais lancé pour une autre
/// raison ; on remet le délai à zéro, et on exige que le premier tour change.
#[test]
fn sans_le_delai_elding_repart_au_premier_tour() {
    let mut rs = forgelance();
    for s in rs.spells.iter_mut() {
        if s.id == "elding" {
            s.initial_cooldown = 0;
        }
    }
    let t = tours(&rs, &["elding", "octave", "jormun"]);
    assert!(
        t[0].contains(&"elding".to_string()),
        "le délai retiré, Elding doit rouvrir le combat : {:?}",
        t[0]
    );
}

/// Le délai ne se confond pas avec l'intervalle : Elding porte les deux.
#[test]
fn le_delai_initial_n_est_pas_l_intervalle() {
    let rs = forgelance();
    let elding = rs.spells.iter().find(|s| s.id == "elding").unwrap();
    assert_eq!(
        elding.initial_cooldown, 1,
        "un tour avant le premier lancer"
    );
    assert_eq!(elding.cooldown_turns, 2, "deux tours entre deux lancers");
}
