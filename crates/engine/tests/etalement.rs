//! L'écart entre les ennemis, et la dégressivité de zone qu'il fait jouer.
//!
//! Le jeu retire `falloff_percent` par cran d'éloignement de l'impact, au plus
//! `falloff_steps` fois. Le solveur ne sait pas où se tiennent les ennemis : le
//! joueur déclare l'écart, comme il déclare les PM dépensés et les ennemis au
//! contact. Une cible de plus rapporte toujours plus au total, simplement moins
//! que la précédente : le dernier test fixe ce point.

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

fn frappe(rs: &Ruleset, sort: &str, cibles: u8, etalement: u8) -> f64 {
    let build = Build {
        name: sort.into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 630,
                flat_damage: 56,
            }; 5],
            ..Default::default()
        },
        base_ap: 4,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec![sort.to_string()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 1,
        etalement,
        placement: None,
        pm_depenses: 0,
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
    Engine::new(rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .total
        .as_f64()
}

/// À zéro, rien ne change : c'est le défaut et l'ancien comportement.
#[test]
fn a_zero_le_reglage_ne_change_rien() {
    let rs = regles("iop", 8);
    for cibles in 1..=5u8 {
        let serres = frappe(&rs, "epee_celeste", cibles, 0);
        let un = frappe(&rs, "epee_celeste", 1, 0);
        assert!(
            (serres - un * f64::from(cibles)).abs() < 1.0,
            "à {cibles} cibles collées, le total doit rester proportionnel : \
             {serres:.1} contre {:.1}",
            un * f64::from(cibles)
        );
    }
}

/// Écartées, les cibles suivantes prennent moins, du montant que la donnée dit.
///
/// L'Épée Céleste du Iop retire dix pour cent par cran, quatre crans au plus.
/// À un d'écart, la deuxième cible prend 90 %, la troisième 80 %, la quatrième
/// 70 %, la cinquième 60 %.
#[test]
fn chaque_cible_suivante_prend_le_montant_annonce() {
    let rs = regles("iop", 8);
    let une = frappe(&rs, "epee_celeste", 1, 1);
    let taux = [1.0, 0.9, 0.8, 0.7, 0.6];
    for cibles in 1..=5usize {
        let attendu: f64 = une * taux[..cibles].iter().sum::<f64>();
        let mesure = frappe(&rs, "epee_celeste", cibles as u8, 1);
        assert!(
            (mesure - attendu).abs() < 2.0,
            "à {cibles} cibles écartées d'une case, le total doit valoir \
             {attendu:.1} et il vaut {mesure:.1}"
        );
    }
}

/// Le plafond de crans tient : au-delà, une cible de plus ne perd pas plus.
#[test]
fn le_plafond_de_crans_tient() {
    let rs = regles("iop", 8);
    let une = frappe(&rs, "epee_celeste", 1, 1);
    // Quatre crans au plus, donc la sixième cible prend le même taux que la
    // cinquième : 60 % toutes les deux.
    let cinq = frappe(&rs, "epee_celeste", 5, 1);
    let six = frappe(&rs, "epee_celeste", 6, 1);
    assert!(
        ((six - cinq) - une * 0.6).abs() < 2.0,
        "la sixième cible doit prendre 60 % comme la cinquième, soit {:.1}, \
         et elle apporte {:.1}",
        une * 0.6,
        six - cinq
    );
}

/// Toucher plus d'ennemis rapporte toujours plus : la courbe passe sous la
/// droite de proportionnalité, ce qui est le jeu, mais elle monte.
#[test]
fn la_courbe_monte_meme_si_elle_passe_sous_la_droite() {
    let rs = regles("iop", 8);
    let mesures: Vec<f64> = (1..=6u8)
        .map(|n| frappe(&rs, "epee_celeste", n, 1))
        .collect();
    for i in 1..mesures.len() {
        assert!(
            mesures[i] > mesures[i - 1],
            "une cible de plus doit rapporter plus : {mesures:?}"
        );
    }
    let droite = mesures[0] * 6.0;
    assert!(
        mesures[5] < droite,
        "et moins que la proportionnelle, {:.1} contre {droite:.1}",
        mesures[5]
    );
}

// ---------------------------------------------------------------------------
// Les sorts qui ne SONT PAS dégressifs
// ---------------------------------------------------------------------------

/// Tous les sorts de zone ne perdent pas de dégâts avec la distance : « les
/// dommages de zone ne sont pas dégressifs », disent plusieurs d'entre eux, et
/// le Forgelance en porte plus de non dégressives que de dégressives. Le réglage
/// se lit ligne par ligne, dans le `falloff_percent` de sa zone, un zéro voulant
/// dire que rien ne se retire ; vérifié des deux côtés sur la même classe.
#[test]
fn le_reglage_epargne_les_sorts_non_degressifs() {
    let rs = regles("forgelance", 20);

    // Le Lance-pierre dit « les dommages de zone ne sont pas dégressifs » :
    // vingt-cinq cases, aucune perte, quel que soit l'écart annoncé.
    for etalement in 0..=4u8 {
        let mesure = frappe(&rs, "lance_pierre", 5, etalement);
        let colle = frappe(&rs, "lance_pierre", 5, 0);
        assert!(
            (mesure - colle).abs() < 0.01,
            "le Lance-pierre n'est pas dégressif : à {etalement} d'écart il doit \
             valoir {colle:.1} et il vaut {mesure:.1}"
        );
    }

    // La Lance du Lac, elle, perd bien dix pour cent par cran.
    let colle = frappe(&rs, "lance_du_lac", 5, 0);
    let ecarte = frappe(&rs, "lance_du_lac", 5, 1);
    assert!(
        ecarte < colle,
        "la Lance du Lac est dégressive et doit perdre : {ecarte:.1} contre {colle:.1}"
    );
    // Et du montant annoncé : 100, 90, 80, 70 et 60 pour cent.
    let une = frappe(&rs, "lance_du_lac", 1, 1);
    let attendu = une * (1.0 + 0.9 + 0.8 + 0.7 + 0.6);
    assert!(
        (ecarte - attendu).abs() < 2.0,
        "cinq cibles écartées d'une case valent {attendu:.1}, mesuré {ecarte:.1}"
    );
}
