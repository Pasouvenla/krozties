//! La Proie de l'Ouginak, et ce qu'elle débloque. Deux variantes du même sort
//! à 1 PA posent l'état : Proie tout court, et Gibier qui ajoute 7 % aux
//! dommages subis par la cible. Onze sorts en dépendent, dont Os à Moelle et
//! Carcasse, qui gagnent 6 de dégâts de base par lancer tant que la cible est
//! marquée.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn ouginak() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/ouginak.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-18.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn solve(deck: &[&str], dominance: bool) -> Solution {
    let build = Build {
        name: "Ouginak".into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 142,
            elements: [ElementStats {
                characteristic: 650,
                flat_damage: 125,
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
        horizon: 5,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&ouginak(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

/// La réduction ne doit pas changer la réponse : trois états permanents et deux
/// cumuls sont déclarés `monotone: increasing`, et une monotonie fausse rend une
/// rotation sous-optimale en silence.
#[test]
fn dominance_changes_nothing() {
    for deck in [
        &["os_a_moelle", "carcasse", "gibier"][..],
        &["os_a_moelle", "carcasse", "gibier", "acharnement"][..],
        &["carcasse", "gibier", "acharnement", "arcanin"][..],
    ] {
        assert_eq!(
            solve(deck, true).total,
            solve(deck, false).total,
            "la dominance a changé la réponse sur {deck:?}"
        );
    }
}

/// Marquer la cible se rembourse largement : l'état conditionne les deux
/// cumuls, et Gibier ajoute 7 % par-dessus.
#[test]
fn marking_the_target_pays_for_itself() {
    let sans = solve(&["os_a_moelle", "carcasse"], true);
    let avec = solve(&["os_a_moelle", "carcasse", "gibier"], true);
    assert!(
        avec.total > sans.total,
        "un PA pour débloquer les deux cumuls doit rapporter : {} contre {}",
        avec.total.as_f64(),
        sans.total.as_f64()
    );
}

/// Sans la Proie, les cumuls ne montent pas : c'est la condition, pas un
/// bonus gratuit.
#[test]
fn the_stacks_need_the_mark() {
    let s = solve(&["os_a_moelle", "carcasse"], true);
    let degats: Vec<f64> = s
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == "os_a_moelle")
        .map(|c| c.damage.as_f64())
        .collect();
    assert!(degats.len() >= 3, "trop peu de lancers : {degats:?}");
    assert!(
        degats.windows(2).all(|w| w[0] == w[1]),
        "sans la Proie, Os à Moelle ne doit pas monter : {degats:?}"
    );
}

/// Et avec la Proie, ils montent, puis plafonnent à quatre : le « Cumul » de
/// chaque sort et la table de ses paliers donnent quatre.
#[test]
fn the_stacks_climb_to_four_and_stop() {
    let s = solve(&["os_a_moelle", "gibier"], true);
    let degats: Vec<f64> = s
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == "os_a_moelle")
        .map(|c| c.damage.as_f64())
        .collect();
    assert!(degats.len() >= 6, "trop peu de lancers : {degats:?}");
    let distinctes = {
        let mut v = degats.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.dedup();
        v.len()
    };
    assert_eq!(
        distinctes, 5,
        "cinq paliers attendus, de zéro à quatre cumuls : {degats:?}"
    );
    // Et l'écart entre deux paliers est constant : six de dégâts de base, donc
    // un pas fixe une fois la multiplication faite.
    let mut tries = degats.clone();
    tries.sort_by(|a, b| a.partial_cmp(b).unwrap());
    tries.dedup();
    let pas: Vec<f64> = tries.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(
        pas.windows(2).all(|w| (w[0] - w[1]).abs() < 1.0),
        "les paliers doivent être également espacés : {pas:?}"
    );
}

/// La Rage, et le seul de ses usages qui pèse sur les dégâts. Onze sorts
/// l'accumulent, cinq la dépensent, mais un seul de ces cinq change un chiffre :
/// l'Arcanin, 100 Puissance par cran pour 1 PA ; le Cerbère lit la Rage sans la
/// dépenser. Le plafond de deux vient de la donnée : l'effet 792 porte « état,
/// plafond », vérifiable sur Os à Moelle et Carcasse dont les 4 correspondent à
/// leur « Cumul : 4 ». Le gain d'un cran par lancer est une hypothèse déclarée.
#[test]
fn rage_is_spent_on_power_and_read_by_cerberus() {
    let sans = solve(&["molosse", "gibier"], true);
    let avec = solve(&["molosse", "gibier", "arcanin"], true);
    assert!(
        avec.total > sans.total,
        "dépenser la Rage en Puissance doit rapporter : {} contre {}",
        avec.total.as_f64(),
        sans.total.as_f64()
    );
    let cerbere = solve(&["molosse", "gibier", "arcanin", "cerbere"], true);
    assert!(
        cerbere.total > avec.total,
        "le Cerbère lit la Rage, il doit rapporter davantage"
    );
}

/// Sans rien pour l'accumuler, l'Arcanin ne part pas : il exige un cran.
#[test]
fn arcanin_needs_rage_to_be_cast() {
    // Dogue et Cubitus frappent sans donner de Rage.
    let s = solve(&["dogue", "arcanin"], true);
    assert!(
        !s.turns
            .iter()
            .flat_map(|t| t.casts.iter())
            .any(|c| c.id == "arcanin"),
        "aucun sort du deck ne donne de Rage, l'Arcanin ne doit pas être jouable : {:?}",
        s.turns[0].casts.iter().map(|c| &c.id).collect::<Vec<_>>()
    );
}

/// Et la dominance ne doit pas se tromper sur cette ressource neuve.
#[test]
fn rage_does_not_break_dominance() {
    for deck in [
        &["molosse", "gibier", "arcanin"][..],
        &["molosse", "cerbere", "arcanin"][..],
    ] {
        assert_eq!(
            solve(deck, true).total,
            solve(deck, false).total,
            "la dominance a changé la réponse sur {deck:?}"
        );
    }
}
