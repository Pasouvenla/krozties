//! Ce qu'une charge conditionnelle garantit, sur le Gousset.
//!
//! La donnée donne au Gousset deux lignes Air, 12-14 et 30-32 : la seconde est
//! une frappe différée, pas une zone qui répète une ligne comme l'Aiguille, la
//! Réfraction ou le Pendule ; la fourchette, identique ou différente, sépare les
//! deux cas. Le sort pose une glyphe sur la case de la cible à la fin du tour de
//! lancer, si la cible est alors Téléfrag, et cette glyphe frappe au tour
//! suivant toute entité présente, cible d'origine comprise.
//!
//! Le Gousset consomme lui-même le Téléfrag qu'il exige : il faut réappliquer un
//! Téléfrag après lui, dans le même tour, pour que la glyphe se pose. D'où
//! `requires_at: turn_end`, qui relit la condition sur l'état final du tour.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn xelor() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/xelor.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-5.json")).unwrap();
    let report = rs.merge_snapshot(&snap);
    assert!(report.conflicts.is_empty(), "{:#?}", report.conflicts);
    rs
}

fn solve(deck: &[&str], horizon: u8) -> Solution {
    let build = Build {
        name: "Xélor".into(),
        profile: DamageProfile {
            power: 270,
            flat_crit_damage: 173,
            elements: [ElementStats {
                characteristic: 400,
                flat_damage: 60,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 40,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
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
        budgets: vec![("telefrag_per_turn".into(), 3)],
        dominance: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
        prune_spells: true,
    };
    Engine::new(&xelor(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

/// The two Air lines must come out DIFFERENT. Equal ranges here would mean the
/// merge handed the same figure to the immediate hit and the delayed one, which
/// is the mistake this whole file exists to prevent.
#[test]
fn the_merge_takes_the_immediate_line_not_the_delayed_one() {
    // 12-14 immediat contre 30-32 differe : si la fusion prenait la seconde,
    // le Gousset frapperait plus de deux fois trop fort et rien ne le dirait.
    let rs = xelor();
    let sort = rs
        .spells
        .iter()
        .find(|s| s.id == "gousset")
        .expect("le sort doit exister");
    let ligne = sort.lines.first().expect("une ligne");
    assert_eq!(
        ligne.normal.known().copied(),
        Some((12, 14)),
        "la fusion doit prendre la ligne immediate"
    );
    assert_eq!(ligne.critical.known().copied(), Some((15, 17)));
}

/// Sans rien pour remettre un Telefrag apres lui, le Gousset ne pose jamais sa
/// glyphe : il a mange le sien en se lancant.
#[test]
fn without_a_generator_the_glyph_never_lands() {
    let s = solve(&["gousset", "aiguille"], 5);
    let echeances = s
        .turns
        .iter()
        .flat_map(|t| t.opening.iter())
        .filter(|n| n.contains("Gousset"))
        .count();
    assert_eq!(
        echeances,
        0,
        "aucune glyphe ne doit tomber : {:#?}",
        s.turns.iter().map(|t| &t.opening).collect::<Vec<_>>()
    );
}

/// Avec un generateur, la glyphe tombe, et le solveur trouve de lui-meme le
/// vrai jeu : relancer un generateur APRES le Gousset, dans le meme tour.
#[test]
fn a_generator_cast_after_the_spell_makes_the_glyph_land() {
    let s = solve(&["gousset", "engrenage", "aiguille"], 5);
    let echeances = s
        .turns
        .iter()
        .flat_map(|t| t.opening.iter())
        .filter(|n| n.contains("Gousset"))
        .count();
    assert!(
        echeances > 0,
        "la glyphe doit tomber : {:#?}",
        s.turns.iter().map(|t| &t.opening).collect::<Vec<_>>()
    );

    // Et l'ordre est le bon : un generateur suit le Gousset dans le tour.
    let tour = s
        .turns
        .iter()
        .find(|t| t.casts.iter().any(|c| c.id == "gousset"))
        .expect("le Gousset doit etre joue");
    let apres_gousset: Vec<&str> = tour
        .casts
        .iter()
        .skip_while(|c| c.id != "gousset")
        .skip(1)
        .map(|c| c.id.as_str())
        .collect();
    assert!(
        apres_gousset.contains(&"engrenage"),
        "un generateur doit suivre le Gousset pour reposer le Telefrag : {:?}",
        tour.casts.iter().map(|c| &c.id).collect::<Vec<_>>()
    );
}

/// Le mecanisme de charge conditionnelle au LANCER existe toujours, sur la
/// Fuite du Temps : les deux moments de lecture coexistent.
#[test]
fn a_charge_conditioned_at_cast_still_exists() {
    let rs = xelor();
    let sort = rs
        .spells
        .iter()
        .find(|s| s.id == "fuite_du_temps")
        .expect("le sort doit exister");
    assert!(
        sort.effects.iter().any(|e| matches!(
            e,
            dofus_ruleset::Effect::ScheduleGain {
                requires: Some(_),
                ..
            }
        )),
        "la Fuite du Temps doit armer sous condition : {:?}",
        sort.effects
    );
    let s = solve(&["fuite_du_temps", "refraction"], 4);
    assert!(
        s.turns[0].casts.is_empty(),
        "tour 1 : {:?}",
        s.turns[0].casts
    );
}

/// La sequence trouvee sur le petit deck est-elle vraiment optimale ?
///
/// Elle a l'air etrange : deux Engrenages dans le tour, alors que le Telefrag
/// est plafonne a un. Elle ne l'est pas. Chaque generation sert : le premier
/// Engrenage pose le Telefrag que le Gousset consomme, ce qui fait proquer
/// l'Aiguille, et le second le repose pour que la glyphe se pose en fin de
/// tour. Verifie contre la recherche exhaustive, dominance coupee.
#[test]
fn the_small_deck_rotation_is_optimal() {
    let rs = xelor();
    let deck = ["gousset", "engrenage", "aiguille"];
    let build = |_: ()| Build {
        name: "Xélor".into(),
        profile: DamageProfile {
            power: 270,
            flat_crit_damage: 173,
            elements: [ElementStats {
                characteristic: 400,
                flat_damage: 60,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 40,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let scenario = |dominance: bool| Scenario {
        poussees_bloquees: false,
        horizon: 5,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![("telefrag_per_turn".into(), 3)],
        dominance,
        mode: Mode::Expected,
        reach: Reach::Ranged,
        prune_spells: false,
    };
    let reduit = Engine::new(&rs, build(()), scenario(true))
        .unwrap_or_else(|e| panic!("{e}"))
        .solve();
    let exhaustif = Engine::new(&rs, build(()), scenario(false))
        .unwrap_or_else(|e| panic!("{e}"))
        .solve();
    assert_eq!(
        reduit.total, exhaustif.total,
        "la recherche reduite n'a pas trouve l'optimum"
    );
}
