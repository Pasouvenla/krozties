//! Une ressource qui arrive au début d'un tour à venir, et non au lancer.
//!
//! La Fuite du Temps du Xélor coûte un PA, ne frappe pas, et « téléporte la
//! cible à sa position précédente au tour suivant » : effet 1100 avec un délai
//! d'un tour. Le Téléfrag n'existe donc pas dans le tour du lancer ; c'est un
//! moyen de convertir un PA orphelin de fin de tour. Ce Téléfrag ne se prend
//! pas sur le budget du tour, il l'augmente de un : le budget chiffre ce que le
//! joueur met en place en se déplaçant, et ce sort s'arme un tour à l'avance.

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

fn solve(deck: &[&str], budget: u8) -> Solution {
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
        budgets: vec![("telefrag_per_turn".into(), budget)],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&xelor(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

fn compter(s: &Solution, id: &str) -> usize {
    s.turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == id)
        .count()
}

/// La charge tombe au tour SUIVANT, pas dans celui du lancer.
///
/// Vérifié sur le deck le plus dépouillé possible : la Fuite du Temps est la
/// seule source de Téléfrag, la Réfraction la seule consommatrice. Elle ne peut
/// donc frapper qu'à partir du tour qui suit un lancer.
#[test]
fn the_resource_arrives_a_turn_later() {
    let s = solve(&["fuite_du_temps", "refraction"], 3);
    // Rien au tour 1 : la cible n'a pas encore de position précédente, donc il
    // n'y a rien à téléporter et rien à armer.
    assert!(
        s.turns[0].casts.is_empty(),
        "tour 1 : {:?}",
        s.turns[0].casts
    );
    let premier_lancer = s
        .turns
        .iter()
        .position(|t| t.casts.iter().any(|c| c.id == "fuite_du_temps"))
        .expect("le sort doit être joué");
    let premiere_refraction = s
        .turns
        .iter()
        .position(|t| t.casts.iter().any(|c| c.id == "refraction"))
        .expect("la Réfraction doit finir par consommer");
    assert!(
        premiere_refraction > premier_lancer,
        "la Réfraction consomme au tour {premiere_refraction}, le Téléfrag est \
         armé au tour {premier_lancer} : il ne peut pas arriver avant"
    );
}

/// Le Téléfrag différé AUGMENTE le budget du tour au lieu d'y puiser.
///
/// Mesuré : avec un budget d'un seul Téléfrag par tour, la Réfraction ne peut
/// frapper qu'une fois par tour. La Fuite du Temps en apporte un second, et le
/// nombre de Réfractions passe de six à onze sur six tours.
///
/// Le contrôle inverse compte autant : à budget deux ou trois, le budget n'est
/// plus le facteur limitant et le sort ne change rien. Sans cette moitié, un
/// test qui verrait le sort ajouter des dégâts partout ne prouverait pas que
/// c'est bien le budget qui a bougé.
#[test]
fn the_delayed_gain_adds_one_to_the_turns_budget() {
    let sans = solve(&["engrenage", "refraction"], 1);
    let avec = solve(&["engrenage", "refraction", "fuite_du_temps"], 1);
    assert!(
        avec.total > sans.total,
        "à budget serré la Fuite du Temps doit rapporter : {} contre {}",
        avec.total.as_f64(),
        sans.total.as_f64()
    );
    assert!(
        compter(&avec, "refraction") > compter(&sans, "refraction"),
        "elle doit permettre des consommations en plus : {} contre {}",
        compter(&avec, "refraction"),
        compter(&sans, "refraction")
    );

    for budget in [2u8, 3] {
        let sans = solve(&["engrenage", "refraction"], budget);
        let avec = solve(&["engrenage", "refraction", "fuite_du_temps"], budget);
        assert_eq!(
            avec.total, sans.total,
            "à budget {budget} le budget n'est plus limitant, le sort ne doit \
             rien changer au total"
        );
    }
}

/// L'ordre du deck ne doit pas changer l'optimum.
///
/// Un sort qui ne fait rien d'autre qu'armer une charge est facile à départager
/// de travers : sur ce deck il ne rapporte aucun dégât et le solveur le joue ou
/// non selon son rang. Le TOTAL, lui, doit être le même dans les six ordres.
#[test]
fn deck_order_does_not_change_the_optimum() {
    const SORTS: [&str; 3] = ["engrenage", "refraction", "fuite_du_temps"];
    let reference = solve(&SORTS, 3).total;
    for (a, b, c) in [(0, 2, 1), (1, 0, 2), (1, 2, 0), (2, 0, 1), (2, 1, 0)] {
        let deck = [SORTS[a], SORTS[b], SORTS[c]];
        assert_eq!(
            solve(&deck, 3).total,
            reference,
            "ordre {deck:?} : l'optimum a changé"
        );
    }
}
