//! Un sort dont le coût en PA monte une fois lancé. La Paume Explosive du
//! Pandawa « augmente le coût en PA du sort après le premier lancer », une
//! fois : le jeu fait payer 2 puis 3 puis 3, et un tour à 7 PA n'en tient que
//! deux.

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

/// Les lancers d'un sort, tour par tour, avec le coût que le solveur a payé.
fn lancers(spell: &str, base_ap: u8, horizon: u8) -> Vec<(u8, u8)> {
    let build = Build {
        base_mp: 3,
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
        base_ap,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec![spell.to_string()],
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
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&pandawa(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .flat_map(|t| {
            t.casts
                .iter()
                .filter(|c| c.id == spell)
                .map(move |c| (t.turn, c.ap_cost))
        })
        .collect()
}

/// Le premier lancer se paie au prix nu, les suivants au prix majoré.
///
/// Le compteur se lit AVANT les effets du lancer, exactement comme la réduction
/// de coût : un sort qui pose lui-même son compteur ne se surtaxe pas au lancer
/// qui le pose.
#[test]
fn le_premier_lancer_se_paie_au_prix_nu_et_les_suivants_non() {
    let casts = lancers("paume_explosive", 12, 1);
    assert!(
        casts.len() >= 2,
        "il faut deux lancers pour comparer, obtenu {casts:?}"
    );
    assert_eq!(
        casts[0].1, 2,
        "le premier lancer coûte le prix nu : {casts:?}"
    );
    for (_, cout) in &casts[1..] {
        assert_eq!(*cout, 3, "les lancers suivants sont majorés : {casts:?}");
    }
}

/// Le surcoût plafonne. « Chaque effet est cumulable une fois. »
#[test]
fn le_surcout_ne_monte_qu_une_fois() {
    let casts = lancers("paume_explosive", 12, 1);
    let maxi = casts.iter().map(|(_, c)| *c).max().unwrap_or(0);
    assert_eq!(maxi, 3, "un seul cran de surcoût, jamais deux : {casts:?}");
}

/// Le surcoût retombe entre deux tours.
///
/// L'état porte `turns: 0`, qui veut dire « le tour de la pose et lui seul ».
/// Avec `turns: 1` il vivrait encore tout le tour suivant, et le premier lancer
/// de ce tour-là se paierait déjà au prix fort.
#[test]
fn le_surcout_retombe_au_tour_suivant() {
    let casts = lancers("paume_explosive", 12, 3);
    let mut vus = std::collections::BTreeMap::new();
    for (tour, cout) in &casts {
        vus.entry(*tour).or_insert_with(Vec::new).push(*cout);
    }
    assert!(vus.len() >= 2, "il faut deux tours, obtenu {casts:?}");
    for (tour, couts) in &vus {
        assert_eq!(
            couts[0], 2,
            "le premier lancer du tour {tour} repart au prix nu : {vus:?}"
        );
        // Sans cette seconde assertion le test passe alors même que le surcoût
        // n'existe pas : si tous les lancers coûtent 2, « le premier coûte 2 »
        // est vrai pour rien. Il faut donc exiger que le prix ait MONTÉ dans le
        // tour pour que sa retombée veuille dire quelque chose.
        assert!(
            couts.iter().any(|c| *c == 3),
            "le prix doit être monté dans le tour {tour} pour pouvoir retomber : {vus:?}"
        );
    }
}

/// Le budget de PA le voit, pas seulement l'affichage : à 7 PA, trois lancers
/// coûteraient 8, et le sort n'en tient que deux.
#[test]
fn un_tour_de_sept_pa_n_en_tient_que_deux() {
    let casts = lancers("paume_explosive", 7, 1);
    assert_eq!(
        casts.len(),
        2,
        "2 + 3 tiennent dans 7 PA, un troisième à 3 PA non : {casts:?}"
    );
    let total: u8 = casts.iter().map(|(_, c)| *c).sum();
    assert_eq!(total, 5, "deux lancers valent 2 puis 3 : {casts:?}");
}
