//! Les dégâts en pourcentage de la vie du lanceur, bruts : ni Puissance, ni
//! caractéristique, ni multiplicateur, ni critique, seules les résistances de
//! la cible les réduisent. La vie sort du build, à la part que le joueur déclare
//! en « Vie restante (%) », et la vie érodée de la Punition est le taux
//! d'« Érosion (%) » appliqué à la vie perdue : à 60 % de vie et 20 % d'érosion,
//! 8 % de la vie maximum.
//!
//! Le build des tests porte 4000 points de vie, une caractéristique de 630, 170
//! de Puissance, 50 % de critique et un multiplicateur de 120 % : si l'un d'eux
//! touchait ces dégâts, les chiffres attendus ne tomberaient pas juste.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn regles(classe: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap =
        Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn solve(
    rs: &Ruleset,
    deck: &[&str],
    reglages: &[(&str, u8)],
    cibles: u8,
    resistance: Resistance,
    horizon: u8,
) -> Solution {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 30,
            elements: [ElementStats {
                characteristic: 630,
                flat_damage: 56,
            }; 5],
            life: 4000,
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 50,
        modifiers: vec![BuildModifier {
            id: "multiplicateur".into(),
            percent: 120,
            when: When::Always,
        }],
        deck: deck.iter().map(|s| (*s).to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: reglages.iter().map(|(n, v)| ((*n).to_string(), *v)).collect(),
        horizon,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        targets: cibles,
        starting_turn_is_odd: true,
        resistance,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

/// Les dégâts d'un tour, sort par sort.
fn coups(sol: &Solution, tour: usize, sort: &str) -> f64 {
    sol.turns[tour]
        .casts
        .iter()
        .filter(|c| c.id == sort)
        .map(|c| c.damage.as_f64())
        .sum()
}

/// La Transfusion : 10 % de la vie restante, soit 240 à 60 % de 4000, brut.
#[test]
fn la_transfusion_frappe_dix_pour_cent_de_la_vie_restante() {
    let rs = regles("sacrieur", 11);
    let sol = solve(&rs, &["transfusion"], &[("vie_restante", 60)], 1, Resistance::NONE, 1);
    assert_eq!(coups(&sol, 0, "transfusion"), 240.0);
}

/// Les résistances Neutre, et elles seules, la réduisent : 240 à 10 % puis 5
/// fixes font 211.
#[test]
fn seules_les_resistances_reduisent_ces_degats() {
    let rs = regles("sacrieur", 11);
    let mut resistance = Resistance::NONE;
    resistance.percent[dofus_damage::Element::Neutral.index()] = 10;
    resistance.flat[dofus_damage::Element::Neutral.index()] = 5;
    let sol = solve(&rs, &["transfusion"], &[("vie_restante", 60)], 1, resistance, 1);
    assert_eq!(coups(&sol, 0, "transfusion"), 211.0);
}

/// La zone compte comme pour toute ligne : trois ennemis dans le cercle de la
/// Transfusion prennent chacun ses 240.
#[test]
fn la_zone_d_une_ligne_brute_compte_ses_cibles() {
    let rs = regles("sacrieur", 11);
    let sol = solve(&rs, &["transfusion"], &[("vie_restante", 60)], 3, Resistance::NONE, 1);
    assert_eq!(coups(&sol, 0, "transfusion"), 720.0);
}

/// Le Châtiment : 15 % de la vie restante, 360 à 60 %.
#[test]
fn le_chatiment_frappe_quinze_pour_cent_de_la_vie_restante() {
    let rs = regles("sacrieur", 11);
    let sol = solve(&rs, &["chatiment"], &[("vie_restante", 60)], 1, Resistance::NONE, 1);
    assert_eq!(coups(&sol, 0, "chatiment"), 360.0);
}

/// La Punition ajoute 35 % de la vie érodée : 20 % d'érosion sur les 40 %
/// perdus font 320 points érodés, et 35 % de 320 valent 112. Sa ligne dans le
/// meilleur élément, elle, ne change pas avec l'érosion.
#[test]
fn la_punition_frappe_la_vie_erodee() {
    let rs = regles("sacrieur", 11);
    let avec = solve(
        &rs,
        &["punition"],
        &[("vie_restante", 60), ("erosion", 20)],
        1,
        Resistance::NONE,
        1,
    );
    let sans = solve(
        &rs,
        &["punition"],
        &[("vie_restante", 60), ("erosion", 0)],
        1,
        Resistance::NONE,
        1,
    );
    let ecart = coups(&avec, 0, "punition") - coups(&sans, 0, "punition");
    assert!((ecart - 112.0).abs() < 0.01, "112 attendus, {ecart:.2} mesurés");
}

/// La Mascarade frappe 25 % de la plus grande des deux vies : la restante
/// au-dessus de 50 %, la manquante en dessous. 600 à 60 %, 700 à 30 %.
///
/// Elle a un délai initial d'un tour : son coup tombe au tour 2.
#[test]
fn la_mascarade_permute_sous_cinquante_pour_cent() {
    let rs = regles("zobal", 14);
    for (vie, attendu) in [(60u8, 600.0), (30, 700.0)] {
        let sol = solve(&rs, &["mascarade"], &[("vie_restante", vie)], 1, Resistance::NONE, 2);
        assert_eq!(
            coups(&sol, 1, "mascarade"),
            attendu,
            "à {vie} % de vie, {attendu} attendus"
        );
    }
}
