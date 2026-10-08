//! Un poison dont le montant monte avec le nombre de tours qu'il a tenu.
//!
//! Les Toxines du Sram sont le seul sort du parc dans ce cas : six paliers,
//! « Ennemi - Fin de tour » à 7-9 puis « 1 tour avec cible sous poison » à
//! 13-15, et ainsi jusqu'à 37-39. Les dégâts d'un état se tabulent sur le
//! domaine de son compteur et se lisent à l'exécution : calculés une fois à
//! compteurs nuls, une ligne sous `active_at` n'y serait jamais active.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn sram() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/sram.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-4.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

/// Ce que le poison des Toxines vaut à l'ouverture de chaque tour.
///
/// Un deck réduit aux Toxines : le sort ne frappe pas au lancer, donc tout ce
/// qui sort ici vient de l'état, et rien d'autre ne peut expliquer un écart.
fn tics() -> Vec<f64> {
    let build = Build {
        name: "Sram".into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 630,
                flat_damage: 56,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["toxines".into()],
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
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&sram(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .map(|t| {
            t.opening_sources
                .iter()
                .filter(|(src, _)| src == "toxines")
                .map(|(_, d)| d.as_f64())
                .sum::<f64>()
        })
        .collect()
}

#[test]
fn les_paliers_du_poison_montent_tour_apres_tour() {
    let v = tics();
    let ticks: Vec<f64> = v.iter().copied().filter(|d| *d > 0.0).collect();
    assert!(
        ticks.len() >= 5,
        "cinq tics au moins sur sept tours, mesuré {v:?}"
    );
    for f in ticks.windows(2) {
        assert!(
            f[1] > f[0],
            "chaque tic doit battre le précédent, mesuré {ticks:?}"
        );
    }
    // 7-9 contre 37-39 sur le jet de base : l'AMPLEUR compte autant que le
    // sens, une assertion seulement directionnelle passant avec un seul palier
    // de plus au lieu de cinq.
    let rapport = ticks[ticks.len() - 1] / ticks[0];
    assert!(
        rapport > 2.5,
        "le dernier palier doit valoir plusieurs fois le premier, mesuré {rapport:.2} sur {ticks:?}"
    );
}
