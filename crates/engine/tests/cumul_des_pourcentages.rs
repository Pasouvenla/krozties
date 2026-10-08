//! Les crans d'un même état s'additionnent, et deux états de % de dommages
//! finaux aussi : c'est une seule statistique du personnage, qui s'additionne à
//! celle des Dofus et de l'équipement. Dix projections de portail à 2 % font un
//! Eliotrope à 20 %, appliqué une fois, et non 1,02 dix fois. Les dommages
//! subis posés sur la cible, eux, restent un facteur à part.

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

fn frappe(rs: &Ruleset, sort: &str, etat: &str, valeur: u8) -> f64 {
    frappe_deux(rs, sort, &[(etat, valeur)])
}

fn frappe_deux(rs: &Ruleset, sort: &str, etats: &[(&str, u8)]) -> f64 {
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
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec![sort.to_string()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: etats.iter().map(|(n, v)| ((*n).to_string(), *v)).collect(),
        horizon: 1,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        targets: 1,
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

/// Dix crans à 2 % valent vingt pour cent, pas vingt-et-un virgule neuf.
///
/// L'amplitude se vérifie, et l'écart entre les deux lectures est le cœur du
/// test : une assertion seulement croissante passerait avec la composition.
#[test]
fn dix_projections_valent_vingt_pour_cent() {
    let rs = regles("eliotrope", 16);
    let nu = frappe(&rs, "raillerie", "portails_traverses", 0);
    let plein = frappe(&rs, "raillerie", "portails_traverses", 10);
    let additif = nu * 1.20;
    let compose = nu * 1.02_f64.powi(10);
    assert!(
        (plein - additif).abs() < 1.0,
        "dix crans doivent valoir {additif:.1}, l'addition de dix fois deux pour \
         cent, et non {compose:.1} qui serait leur composition : mesuré {plein:.1}"
    );
}

/// La courbe entière suit la droite, pas l'exponentielle.
///
/// ⚠️ PAS UNE ÉGALITÉ ENTRE CRANS SUCCESSIFS. Le calcul arrondit à chaque
/// lancer, donc les écarts mesurés oscillent autour de leur moyenne, de 14,0 à
/// 15,5 ici : une assertion cran par cran tomberait sur le bruit d'arrondi et
/// non sur le défaut. Ce qui distingue les deux lectures est l'ÉCART À LA
/// DROITE, et il se creuse à mesure qu'on monte.
#[test]
fn la_courbe_suit_la_droite_et_non_l_exponentielle() {
    let rs = regles("eliotrope", 16);
    let mesures: Vec<f64> = (0..=10)
        .map(|n| frappe(&rs, "raillerie", "portails_traverses", n))
        .collect();
    let nu = mesures[0];
    for n in 1..=10 {
        let additif = nu * (1.0 + 0.02 * n as f64);
        let compose = nu * 1.02_f64.powi(n as i32);
        let ecart_droite = (mesures[n] - additif).abs();
        let ecart_courbe = (mesures[n] - compose).abs();
        assert!(
            ecart_droite < 2.0,
            "à {n} cran(s) la mesure {:.1} doit suivre la droite {additif:.1} : {mesures:?}",
            mesures[n]
        );
        if n >= 5 {
            assert!(
                ecart_droite < ecart_courbe,
                "à {n} crans la mesure doit être PLUS PRÈS de la droite {additif:.1} \
                 que de l'exponentielle {compose:.1}, et elle vaut {:.1}",
                mesures[n]
            );
        }
    }
}

/// Les deux bonus de portail s'additionnent : deux sources de % de dommages
/// finaux, une seule statistique (le Vulbis à +10 % et le Nébuleux à +20 % font
/// ×1,30, pas ×1,32). L'un tient au nombre de sorts projetés, l'autre à l'écart
/// entre les deux portails.
#[test]
fn les_deux_bonus_de_portail_s_additionnent() {
    let rs = regles("eliotrope", 16);
    let nu = frappe_deux(&rs, "raillerie", &[]);
    let projections = frappe_deux(&rs, "raillerie", &[("portails_traverses", 10)]);
    let cases = frappe_deux(&rs, "raillerie", &[("cases_entre_portails", 10)]);
    let les_deux = frappe_deux(
        &rs,
        "raillerie",
        &[("portails_traverses", 10), ("cases_entre_portails", 10)],
    );

    // Chacun seul vaut vingt pour cent : dix crans de deux, additionnés.
    for (nom, m) in [("projections", projections), ("cases", cases)] {
        assert!(
            (m - nu * 1.20).abs() < 2.0,
            "{nom} seul doit valoir {:.1} et vaut {m:.1}",
            nu * 1.20
        );
    }
    // Ensemble ils s'ADDITIONNENT : 1,40 et non 1,20 x 1,20.
    let compose = nu * 1.20 * 1.20;
    let additionne = nu * 1.40;
    assert!(
        (les_deux - additionne).abs() < 3.0,
        "les deux ensemble doivent valoir {additionne:.1}, leur somme, et non \
         {compose:.1} : mesuré {les_deux:.1}"
    );
    assert!(
        (les_deux - compose).abs() > 3.0,
        "et ils ne doivent surtout pas se composer"
    );
}
