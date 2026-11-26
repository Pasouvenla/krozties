//! Les lancers qu'un état ajoute à un sort, par tour et par cible avec eux :
//! la Varappe du Pandawa donne à Karcham deux lancers de plus et un de plus
//! par cible, le tour où elle part.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn moteur(rs: &Ruleset, deck: &[&str], ap: u8, cibles: u8) -> Engine {
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
        base_ap: ap,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 1,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: cibles,
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

fn lances(sol: &Solution) -> Vec<String> {
    sol.turns.iter().flat_map(|t| t.casts.iter().map(|c| c.id.clone())).collect()
}

fn combien(sol: &Solution, sort: &str) -> usize {
    lances(sol).iter().filter(|id| *id == sort).count()
}

/// `frappe` part quatre fois par tour, une fois par cible ; `elan` lui donne
/// deux lancers de plus par tour et un de plus par cible.
fn banc() -> Ruleset {
    Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - { id: elan, scope: caster, max: 1, default: 0, monotone: increasing, duration: { turns: 0, refresh: on_apply, on_expire: reset_to_default } }
spells:
  - id: elan
    name: { fr: "Élan", en: "Momentum" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - { effect: gain, resource: elan }
  - id: frappe
    name: { fr: "Frappe", en: "Strike" }
    ap_cost: { base: 1 }
    casts_per_turn: 4
    casts_per_target: 1
    extra_casts: { resource: elan, per_stack: 2, cap: 2, per_target: 1 }
    crit: { base_rate: 0, can_crit: false }
    lines:
      - { element: water, critical: [10, 10], normal: [10, 10] }
    effects:
      - effect: damage
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Le plafond se relit sur ses deux limites : un lancer de plus sur une cible,
/// deux sur deux, trois sur trois, et pas plus de six par tour.
#[test]
fn un_lancer_par_cible_de_plus_se_lit_sur_les_deux_plafonds() {
    let rs = banc();
    for (cibles, sans, avec) in [(1, 1, 2), (2, 2, 4), (3, 3, 6), (5, 4, 6)] {
        let sol = moteur(&rs, &["frappe"], 20, cibles).solve();
        assert_eq!(combien(&sol, "frappe"), sans, "{cibles} cible(s), sans élan : {:?}", lances(&sol));
        let sol = moteur(&rs, &["elan", "frappe"], 20, cibles).solve();
        assert_eq!(combien(&sol, "frappe"), avec, "{cibles} cible(s), avec élan : {:?}", lances(&sol));
    }
}

/// Sur un seul ennemi, Karcham ne le porte qu'une fois par tour, donc un seul
/// jet ; la Varappe lui en donne un second, donc deux jets.
#[test]
fn la_varappe_fait_porter_deux_fois_la_meme_cible() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/pandawa.yaml")).unwrap_or_else(|e| panic!("{e}"));
    rs.merge_snapshot(&Snapshot::load(format!("{root}/data/snapshots/breed-12.json")).unwrap());
    let jets = |sol: &Solution| combien(sol, "cascade") + combien(sol, "eau_de_vie");

    let sans = moteur(&rs, &["karcham", "cascade", "eau_de_vie"], 12, 1).solve();
    assert_eq!(jets(&sans), 1, "{:?}", lances(&sans));

    let avec = moteur(&rs, &["karcham", "varappe", "cascade", "eau_de_vie"], 12, 1).solve();
    let seq = lances(&avec);
    assert_eq!(jets(&avec), 2, "{seq:?}");
    let varappe = seq.iter().position(|id| id == "varappe").expect("la Varappe part");
    let karchams: Vec<usize> = seq.iter().enumerate().filter(|(_, id)| *id == "karcham").map(|(i, _)| i).collect();
    assert_eq!(karchams.len(), 2, "{seq:?}");
    assert!(karchams[1] > varappe, "le second Karcham suit la Varappe : {seq:?}");
    assert!(avec.total.as_f64() > sans.total.as_f64(), "{} contre {}", avec.total.as_f64(), sans.total.as_f64());
}
