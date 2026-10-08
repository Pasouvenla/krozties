//! Les régimes que le lanceur quitte et reprend : Sobre et Saoul du Pandawa,
//! masques du Zobal.
//!
//! Pandawa : Picole et Bombance basculent de l'un à l'autre (la donnée lit
//! l'état avant le lancer), Bombance rend 1 PA en redevenant Sobre, Gueule de
//! Bois, Lait de Bambou et Prohibition rendent Sobre, et la Gueule de Bois
//! frappe plus fort Saoul ou sortie de Saoul ce tour-ci (état 3577).
//!
//! Zobal : le Masque de l'Intrépide donne +1 PA tant qu'il est porté, dès le
//! tour de la pose, et le reprend quand un autre masque le remplace.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn regles(classe: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn moteur(rs: &Ruleset, deck: &[&str], ap: u8, horizon: u8) -> Engine {
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
        horizon,
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
    Engine::new(rs, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

fn resoudre(rs: &Ruleset, deck: &[&str], ap: u8, horizon: u8) -> Solution {
    moteur(rs, deck, ap, horizon).solve()
}

fn frappes(sol: &Solution, id: &str) -> Vec<f64> {
    sol.turns
        .iter()
        .flat_map(|t| &t.casts)
        .filter(|c| c.id == id)
        .map(|c| c.damage.as_f64())
        .collect()
}

/// Seule, la Gueule de Bois frappe Sobre (24-27). Après une Liqueur, la
/// première frappe Saoul et rend Sobre, la seconde frappe sortie de Saoul :
/// 34-37 les deux fois.
#[test]
fn la_gueule_de_bois_frappe_fort_sortie_de_saoul() {
    let rs = regles("pandawa", 12);
    let sobre = frappes(&resoudre(&rs, &["gueule_de_bois"], 9, 1), "gueule_de_bois");
    let sol = resoudre(&rs, &["liqueur", "gueule_de_bois"], 9, 1);
    let gdb = frappes(&sol, "gueule_de_bois");
    assert_eq!(gdb.len(), 2, "{gdb:?}");
    assert!((gdb[0] - gdb[1]).abs() < 0.01, "sortie de Saoul, meme frappe : {gdb:?}");
    assert!(gdb[1] > sobre[0] * 1.3, "{gdb:?} contre {sobre:?}");
}

/// La Bombance bascule : Sobre puis Saoul puis Sobre, et le second passage
/// rend son PA. Sept PA suffisent alors à deux Gueules de Bois fortes.
#[test]
fn la_bombance_bascule_et_rend_un_pa_en_redevenant_sobre() {
    let rs = regles("pandawa", 12);
    let sobre = frappes(&resoudre(&rs, &["gueule_de_bois"], 7, 1), "gueule_de_bois");
    let sol = resoudre(&rs, &["bombance", "gueule_de_bois"], 7, 1);
    let gdb = frappes(&sol, "gueule_de_bois");
    assert_eq!(gdb.len(), 2, "{gdb:?}");
    assert!(gdb.iter().all(|d| *d > sobre[0] * 1.3), "{gdb:?} contre {sobre:?}");

    // Treize PA payés sur un tour de douze, que le PA rendu rend possible :
    // Liqueur, Pandatak Saoul, Bombance, Main de Pandawa Sobre (3 + 4 + 1 +
    // 5). Rejoué plutôt que cherché : ce tour et ceux qui s'en passent se
    // valent au point près sur deux tours, et la rotation retient celle qui
    // compte le moins de lancers.
    let tour = moteur(&rs, &["liqueur", "pandatak", "main_de_pandawa", "bombance"], 12, 1)
        .replay(&[["liqueur", "pandatak", "bombance", "main_de_pandawa"].map(String::from).to_vec()])
        .unwrap_or_else(|e| panic!("{e}"));
    let payes: u32 = tour.turns[0].casts.iter().map(|c| u32::from(c.ap_cost)).sum();
    assert_eq!(payes, 13, "{tour}");
}

/// L'Intrépide : +1 PA dès la pose s'il n'était pas porté, +1 à chaque tour
/// qui s'ouvre sous lui, et un PA de plus à payer pour le quitter.
#[test]
fn l_intrepide_donne_un_pa_tant_qu_il_est_porte() {
    let rs = regles("zobal", 14);
    let sol = resoudre(
        &rs,
        &["masque_de_l_intrepide", "masque_du_psychopathe", "parafuso", "furia"],
        12,
        3,
    );
    let mut intrepide = false;
    let (mut poses, mut quittes, mut tours_sous) = (0, 0, 0);
    for t in &sol.turns {
        let Some(premier) = t.casts.first() else { continue };
        let ouverture = premier.ap_left + i16::from(premier.ap_cost);
        if intrepide {
            tours_sous += 1;
            assert_eq!(ouverture, 13, "tour ouvert sous l'Intrepide");
        } else {
            assert_eq!(ouverture, 12);
        }
        let mut reste = ouverture;
        for c in &t.casts {
            match c.id.as_str() {
                "masque_de_l_intrepide" => {
                    if !intrepide {
                        poses += 1;
                        assert_eq!(c.ap_left, reste, "la pose rend son PA");
                    }
                    intrepide = true;
                }
                "masque_du_psychopathe" => {
                    let attendu = if intrepide { 2 } else { 1 };
                    if intrepide {
                        quittes += 1;
                    }
                    assert_eq!(c.ap_cost, attendu, "quitter l'Intrepide");
                    intrepide = false;
                }
                _ => {}
            }
            reste = c.ap_left;
        }
    }
    // Rien de tout cela n'a de sens si la rotation n'y passe pas.
    assert!(poses > 0 && quittes > 0 && tours_sous > 0, "{poses} {quittes} {tours_sous}");
}

/// Saoul dure deux tours, N et N+1, disparaît au début du N+2, et pose « sorti
/// de Saoul » en retombant.
#[test]
fn saoul_dure_deux_tours_puis_pose_sorti_de_saoul() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let rs = dofus_ruleset::Ruleset::load(format!("{root}/data/rulesets/pandawa.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let saoul = rs.resource("saoul").expect("saoul");
    let d = saoul.duration.as_ref().expect("Saoul a une durée");
    assert_eq!(d.turns, 1, "deux tours : celui de la pose et le suivant");
    assert_eq!(d.then_gain.as_deref(), Some("sorti_de_saoul"));
}
