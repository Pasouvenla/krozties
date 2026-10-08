//! Un compteur qui ne court que depuis un lancer, et la fenêtre qu'il ouvre.
//!
//! Le Mot Secret de l'Eniripsa gagne trente dégâts de base « dans 2 tours si le
//! sort n'est pas relancé au tour suivant » : effet 293, valeur 30, délai 2,
//! durée 1, plus l'effet 406 qui annule quand on relance. La condition est une
//! abstention, écrite en deux compteurs : un drapeau que le lancer arme, et une
//! maturation qui monte d'un cran par tour tant que le drapeau tient, que le
//! lancer remet à zéro. Le cran deux est la fenêtre, le trois la fenêtre
//! manquée.
//!
//! `gain_per_turn` seul monterait dès le premier tour, sans lancer : c'est ce que
//! `gain_per_turn_while` empêche. Une charge différée fausserait le sort : le
//! moteur refuse de relancer un sort dont une charge est en vol, là où le jeu
//! autorise le relancement et annule la maturation.
//!
//! Le banc est synthétique et son bonus énorme, pour que mûrir gagne à coup sûr.
//! Sur le vrai Mot Secret, deux lancers à trente valent mieux qu'un à soixante,
//! les dommages fixes tombant deux fois.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

/// `garde` écrit ou non la condition sur la maturation.
fn banc(garde: bool) -> Ruleset {
    let ligne = if garde {
        "    gain_per_turn_while: arme\n"
    } else {
        ""
    };
    let yaml = format!(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: {{ fr: "Test", en: "Test" }}
resources:
  - id: arme
    scope: caster
    max: 1
    default: 0
    monotone: increasing
  - id: matu
    scope: caster
    max: 3
    default: 0
    monotone: none
    gain_per_turn: 1
{ligne}spells:
  - id: coup
    name: {{ fr: "Coup", en: "Hit" }}
    ap_cost: {{ base: 6 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines:
      - element: water
        active_at: {{ resource: matu, exactly: 0 }}
        critical: [10, 10]
        normal: [10, 10]
      - element: water
        active_at: {{ resource: matu, exactly: 1 }}
        critical: [10, 10]
        normal: [10, 10]
      - element: water
        active_at: {{ resource: matu, exactly: 2 }}
        critical: [90, 90]
        normal: [90, 90]
      - element: water
        active_at: {{ resource: matu, exactly: 3 }}
        critical: [10, 10]
        normal: [10, 10]
    effects:
      - effect: damage
      - effect: reset
        resource: matu
      - effect: gain
        resource: arme
"#
    );
    Ruleset::from_yaml(&yaml).unwrap_or_else(|e| panic!("{e}"))
}

/// `(numéro de tour, dégâts)` de chaque lancer, dans l'ordre.
fn lancers(garde: bool) -> Vec<(u8, f64)> {
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
        base_ap: 6,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["coup".into()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon: 6,
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
    Engine::new(&banc(garde), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .flat_map(|t| t.casts.iter().map(|c| (t.turn, c.damage.as_f64())))
        .collect()
}

/// Rien ne mûrit avant le premier lancer : il se paie au tarif nu, quel que
/// soit le nombre de tours laissés passer.
#[test]
fn la_maturation_ne_court_pas_avant_le_premier_lancer() {
    let avec = lancers(true);
    let sans = lancers(false);
    assert!(!avec.is_empty() && !sans.is_empty(), "{avec:?} / {sans:?}");
    assert_eq!(
        avec[0].0, 1,
        "avec la garde, rien à attendre : le premier lancer tombe au tour un, mesuré {avec:?}"
    );
    assert!(
        sans[0].0 > 1 && sans[0].1 > avec[0].1,
        "sans la garde le solveur doit attendre et toucher le bonus d'entrée, \
         ce qui est le défaut : mesuré {sans:?} contre {avec:?}"
    );
}

/// Un lancer est bonifié si et seulement si il suit le précédent de deux tours.
/// Sur ce banc, lancers aux tours 1, 2, 4 et 6 : les deux premiers nus, les deux
/// derniers bonifiés.
#[test]
fn un_lancer_est_bonifie_exactement_quand_il_suit_de_deux_tours() {
    let v = lancers(true);
    assert!(
        v.len() >= 3,
        "au moins trois lancers attendus, mesuré {v:?}"
    );
    let nu = v[0].1;
    assert!(nu > 0.0, "le lancer nu doit frapper, mesuré {v:?}");
    let mut bonifies = 0;
    for f in v.windows(2) {
        let ecart = f[1].0 - f[0].0;
        let bonifie = f[1].1 > nu * 2.0;
        assert_eq!(
            bonifie,
            ecart == 2,
            "un lancer est bonifié si et seulement s'il suit de deux tours, \
             écart {ecart} et bonifié {bonifie} dans {v:?}"
        );
        bonifies += usize::from(bonifie);
    }
    // Sans ce compte, l'équivalence ci-dessus tiendrait aussi sur une rotation
    // où AUCUN lancer n'est bonifié et aucun écart ne vaut deux.
    assert!(
        bonifies >= 2,
        "deux lancers bonifiés au moins sur six tours, mesuré {v:?}"
    );
}

// -- et sur le vrai Mot Secret ------------------------------------------------

fn eniripsa() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/eniripsa.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-7.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn mot_secret(resistance_plate: i32) -> Vec<(u8, f64)> {
    let build = Build {
        name: "Eniripsa".into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 630,
                flat_damage: 0,
            }; 5],
            ..Default::default()
        },
        base_ap: 4,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["mot_secret".into()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon: 6,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        starting_turn_is_odd: true,
        resistance: Resistance {
            flat: [resistance_plate; 5],
            ..Resistance::NONE
        },
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(&eniripsa(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .flat_map(|t| t.casts.iter().map(|c| (t.turn, c.damage.as_f64())))
        .collect()
}

/// Mûrir ne paie que contre une réduction plate, et le solveur le trouve seul.
///
/// Sur une cible nue, deux lancers à vingt-huit valent mieux qu'un à
/// cinquante-huit, et le Mot Secret se relance chaque tour. Avec cent cinquante
/// de réduction plate sur la cible, l'arbitrage bascule : un gros coup n'en perd
/// qu'une fois là où deux petits en perdent deux. Sans une telle cible, la
/// fenêtre ne s'ouvrirait jamais sur le vrai fichier.
#[test]
fn le_mot_secret_ne_murit_que_quand_ca_paie() {
    let nue = mot_secret(0);
    assert!(
        nue.len() == 6 && nue.windows(2).all(|f| (f[0].1 - f[1].1).abs() < 0.51),
        "sur cible nue le sort se relance chaque tour, tous les lancers égaux : {nue:?}"
    );
    let plate = mot_secret(150);
    assert!(
        plate.len() < nue.len(),
        "sous réduction plate le solveur doit sauter des tours, {plate:?} contre {nue:?}"
    );
    let nu = plate[0].1;
    let gros = plate.last().unwrap().1;
    assert!(
        gros > nu * 2.0,
        "et le lancer mûri doit valoir plusieurs fois le lancer nu, mesuré {plate:?}"
    );
}
