//! Un bonus dont la condition a deux termes.
//!
//! La Paume Explosive du Pandawa gagne douze dégâts de base « sous Saoul et
//! sans Paume Explosive Saoul » : la donnée le dit en une cible, `C,*E498,
//! *e3533`, un état exigé et un état interdit, écrits par `all_of`. Le bonus ne
//! joue donc qu'au premier lancer du tour, et seulement à jeun perdu.
//!
//! L'état Sobre ne change pas les dégâts mais la zone, Croix de taille 1 contre
//! Cercle de taille 2 : les deux lignes de la donnée portent 20-22 toutes les
//! deux, et compter la seconde comme un palier doublerait le sort.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn pandawa() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/pandawa.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-12.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

/// Les dégâts de chaque Paume Explosive du premier tour, dans l'ordre.
fn paumes(deck: &[&str]) -> Vec<f64> {
    let build = Build {
        name: "Pandawa".into(),
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
        deck: deck.iter().map(|s| s.to_string()).collect(),
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
    Engine::new(&pandawa(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == "paume_explosive")
        .map(|c| c.damage.as_f64())
        .collect()
}

/// Sans Saoul, toutes les Paumes du tour valent la même chose.
///
/// C'est le témoin : le sort seul ne monte pas, et un écart mesuré plus bas ne
/// peut donc venir que de l'état.
#[test]
fn a_jeun_les_paumes_se_valent() {
    let v = paumes(&["paume_explosive"]);
    assert!(v.len() >= 2, "au moins deux Paumes attendues, mesuré {v:?}");
    // Sans ce controle le temoin passerait alors que la ligne de base a
    // disparu : deux lancers a zero degat sont eux aussi tous egaux.
    assert!(
        v.iter().all(|d| *d > 0.0),
        "la ligne de base doit frapper, mesuré {v:?}"
    );
    for f in v.windows(2) {
        assert!(
            (f[0] - f[1]).abs() < 0.51,
            "à jeun aucune Paume ne doit dépasser l'autre, mesuré {v:?}"
        );
    }
}

/// Sous Saoul, la PREMIÈRE Paume du tour frappe plus fort que les suivantes.
#[test]
fn saoul_la_premiere_paume_du_tour_frappe_plus_fort() {
    let v = paumes(&["ribote", "paume_explosive"]);
    assert!(
        v.len() >= 2,
        "au moins deux Paumes attendues sous Saoul, mesuré {v:?}"
    );
    assert!(
        v[0] > v[1],
        "le bonus ne joue qu'au premier lancer, mesuré {v:?}"
    );
    // 20-22 contre 32-34 sur le jet de base : l'écart doit valoir la moitié du
    // sort, pas un point. Une assertion seulement directionnelle passerait avec
    // un bonus de 1 au lieu de 12.
    let ecart = (v[0] - v[1]) / v[1];
    assert!(
        ecart > 0.3,
        "l'écart doit valoir plusieurs dizaines de pour cent, mesuré {:.1} % sur {v:?}",
        ecart * 100.0
    );
}
