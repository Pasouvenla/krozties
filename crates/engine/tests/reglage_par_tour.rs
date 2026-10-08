//! Un réglage déclaré vaut par tour, et le sort qui l'efface l'efface vraiment.
//!
//! « Les effets sont retirés après utilisation du sort », disent la
//! Chausse-trappe du Sram et la Fanfaronnade de l'Ecaflip. Les deux montent de
//! plusieurs dégâts de base par cran d'un compteur que le joueur renseigne, et
//! le vident en partant. Deux erreurs symétriques guettent :
//!
//! * sans remise à zéro, la Chausse-trappe part plusieurs fois par tour et
//!   touche son bonus à chaque fois, là où le jeu ne le donne qu'au premier
//!   lancer ;
//! * avec une remise à zéro mais sans rien pour reposer le réglage, le compteur
//!   tombe à zéro pour le reste du combat.
//!
//! Un réglage déclaré décrit ce que le joueur fait dans un tour : il revient à
//! sa valeur au début de chaque tour.

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

fn solve(rs: &Ruleset, sort: &str, etat: &str, valeur: u8, horizon: u8) -> Solution {
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
        etats_declares: vec![(etat.to_string(), valeur)],
        horizon,
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
}

/// Un seul des trois lancers du tour touche le bonus.
///
/// L'amplitude se vérifie, pas le sens : huit points de dégâts de base par
/// piège, cinq pièges, et le multiplicateur du build vaut neuf, donc 360 de
/// bonus pour le tour, une seule fois et non trois.
#[test]
fn la_chausse_trappe_ne_touche_son_bonus_qu_une_fois_par_tour() {
    let rs = regles("sram", 4);
    let nu = solve(&rs, "chausse_trappe", "pieges_declenches", 0, 1);
    let avec = solve(&rs, "chausse_trappe", "pieges_declenches", 5, 1);
    // DEUX lancers et non trois : le sort autorise trois lancers par tour mais
    // deux par cible, et le scénario n'a qu'un ennemi. C'est le bon plafond, et
    // il suffit à ce test : deux lancers, un seul bonus.
    let lancers = nu.turns[0].casts.len();
    assert_eq!(lancers, 2, "sur une cible, le sort part deux fois");

    let ecart = avec.total.as_f64() - nu.total.as_f64();
    let un_cran = 8.0 * 9.0 * 5.0;
    assert!(
        (ecart - un_cran).abs() < 1.0,
        "le bonus doit tomber UNE fois, soit {un_cran:.0}, et il vaut {ecart:.1} \
         sur {lancers} lancers"
    );
}

/// Et il revient au tour suivant.
///
/// Contrôle du contrôle du test précédent : une remise à zéro sans repos le
/// ferait passer aussi, en ne donnant plus jamais le bonus.
#[test]
fn le_reglage_revient_au_tour_suivant() {
    let rs = regles("sram", 4);
    let mesures: Vec<f64> = (1..=4)
        .map(|h| {
            let nu = solve(&rs, "chausse_trappe", "pieges_declenches", 0, h);
            let avec = solve(&rs, "chausse_trappe", "pieges_declenches", 5, h);
            avec.total.as_f64() - nu.total.as_f64()
        })
        .collect();
    let un_cran = 8.0 * 9.0 * 5.0;
    for (i, m) in mesures.iter().enumerate() {
        let attendu = un_cran * (i + 1) as f64;
        assert!(
            (m - attendu).abs() < 1.0,
            "sur {} tour(s) le bonus doit valoir {attendu:.0} et il vaut {m:.1} : {mesures:?}",
            i + 1
        );
    }
}

/// La Fanfaronnade ne part qu'une fois par tour : rien ne change pour elle.
/// Elle porte le même texte et la même remise à zéro, et le test dit que le
/// correctif n'a pas déplacé un chiffre déjà juste.
#[test]
fn la_fanfaronnade_ne_bouge_pas() {
    let rs = regles("ecaflip", 6);
    let nu = solve(&rs, "fanfaronnade", "deplacements_forces", 0, 3);
    let avec = solve(&rs, "fanfaronnade", "deplacements_forces", 4, 3);
    assert_eq!(
        nu.turns.iter().map(|t| t.casts.len()).sum::<usize>(),
        3,
        "un lancer par tour, trois tours"
    );
    let ecart = avec.total.as_f64() - nu.total.as_f64();
    let attendu = 6.0 * 9.0 * 4.0 * 3.0;
    assert!(
        (ecart - attendu).abs() < 1.0,
        "les trois lancers doivent toucher leur bonus, soit {attendu:.0}, et l'écart \
         vaut {ecart:.1}"
    );
}
