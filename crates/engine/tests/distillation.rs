//! La Distillation du Pandawa, et son bonus qui n'existe qu'à partir du second
//! lancer.
//!
//! « Les dommages du sort et du poison sont augmentés pour chaque poison du
//! sort déclenché sur une cible (cumulable 4 fois). » Le poison se pose au
//! lancer et se déclenche au début du tour de la cible : le tout premier lancer
//! du combat ne peut rien avoir déclenché et sort nu, quel que soit le nombre
//! d'ennemis en zone. D'où `per_extra_target_while` plutôt que
//! `per_extra_target`, qui se replie à la compilation et paierait ce premier
//! lancer plein tarif.
//!
//! Les dégâts d'un état se tabulent sur le domaine de ses compteurs : un bonus
//! de base porté par la ligne d'un poison doit y être compté.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

/// Le build est choisi pour que le multiplicateur soit rond : 170 Puissance et
/// 630 dans la caractéristique font 800, donc un facteur NEUF sur le jet de
/// base. Un cran de quatre points de dégâts de base vaut alors 36 exactement,
/// et l'amplitude devient vérifiable au lieu d'être seulement croissante.
const MULTIPLICATEUR: f64 = 9.0;
const CRAN_BASE: f64 = 4.0;
const CRAN: f64 = CRAN_BASE * MULTIPLICATEUR;

fn regles() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/pandawa.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-12.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

/// Un deck d'un seul sort : ce qui bouge d'un tour à l'autre ne peut venir que
/// de lui. Rend, par tour, `(ouverture, lancers)` : le poison tombe dans la
/// première, le coup immédiat dans la seconde.
fn tours(rs: &Ruleset, targets: u8, horizon: u8) -> Vec<(f64, f64)> {
    let build = Build {
        name: "distillation".into(),
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
        base_mp: 4,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["distillation".to_string()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: vec![],
        horizon,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        targets,
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
        .turns
        .iter()
        .map(|t| (t.opening_damage.as_f64(), t.damage.as_f64()))
        .collect()
}

/// Le premier lancer du combat sort NU, quel que soit le nombre d'ennemis.
///
/// C'est la moitié du sort qui se joue ici : avec un `per_extra_target`
/// ordinaire, replié à la compilation, ce lancer aurait touché ses quatre crans
/// alors qu'aucun poison n'a encore pu se déclencher.
#[test]
fn le_premier_lancer_ne_touche_aucun_cran() {
    let rs = regles();
    let seul = tours(&rs, 1, 1)[0].1;
    for cibles in 2..=7u8 {
        let t = tours(&rs, cibles, 1)[0];
        assert_eq!(t.0, 0.0, "rien n'a encore pu se déclencher au tour un");
        assert!(
            (t.1 - seul * f64::from(cibles)).abs() < 0.01,
            "à {cibles} ennemis le premier lancer doit valoir {cibles} fois \
             la ligne nue, soit {:.1}, et il vaut {:.1}",
            seul * f64::from(cibles),
            t.1
        );
    }
}

/// Les lancers suivants portent UN cran par ennemi au-delà du premier.
///
/// L'écart se vérifie au point près : quatre points de dégâts de base par cran,
/// multipliés par neuf, par cible touchée. Une assertion seulement croissante
/// passerait avec un bonus de travers.
#[test]
fn les_lancers_suivants_montent_dun_cran_par_ennemi_supplementaire() {
    let rs = regles();
    for cibles in 1..=5u8 {
        let t = tours(&rs, cibles, 2);
        let crans = f64::from(cibles - 1);
        let attendu = t[0].1 + CRAN * crans * f64::from(cibles);
        assert!(
            (t[1].1 - attendu).abs() < 0.01,
            "à {cibles} ennemis, le second lancer doit valoir {attendu:.1} \
             ({crans} cran(s) sur chacune des {cibles} cibles), il vaut {:.1}",
            t[1].1
        );
    }
}

/// Le poison monte du même cran que le coup immédiat, et se fige à la pose.
///
/// « Les dommages du sort et du poison sont augmentés » : le texte nomme les
/// deux. Figé à la pose : le poison du premier lancer, posé à vide, reste nu
/// quand il tombe au tour deux, alors que le bonus est armé entre-temps.
#[test]
fn le_poison_monte_du_meme_cran_que_le_coup() {
    let rs = regles();
    let t = tours(&rs, 5, 3);
    let nu = t[0].1;
    // Le poison du tour un se déclenche en ouverture du tour deux.
    assert!(
        (t[1].0 - nu).abs() < 0.01,
        "le poison du premier lancer se fige nu, {nu:.1}, il vaut {:.1}",
        t[1].0
    );
    // Celui du tour deux, en ouverture du tour trois, porte les crans de son
    // lancer.
    let attendu = nu + CRAN * 4.0 * 5.0;
    assert!(
        (t[2].0 - attendu).abs() < 0.01,
        "le poison doit valoir {attendu:.1} comme le coup immédiat, il vaut {:.1}",
        t[2].0
    );
    assert!(
        (t[2].0 - t[1].1).abs() < 0.01,
        "les deux lignes portent la même fourchette et le même cran : {:.1} contre {:.1}",
        t[2].0,
        t[1].1
    );
}

/// Quatre crans est le plafond, comme le texte le dit. DofusBook affiche un
/// palier de plus, « 6 cibles sous poison » à 33-36 : le fichier suit le texte
/// du sort et s'arrête à quatre, et au-delà de cinq ennemis il sous-estime de
/// quatre points de base.
#[test]
fn le_plafond_de_quatre_crans_tient() {
    let rs = regles();
    let a_cinq = tours(&rs, 5, 2);
    let cran_a_cinq = (a_cinq[1].1 - a_cinq[0].1) / 5.0;
    for cibles in [6u8, 7] {
        let t = tours(&rs, cibles, 2);
        let cran = (t[1].1 - t[0].1) / f64::from(cibles);
        assert!(
            (cran - cran_a_cinq).abs() < 0.01,
            "à {cibles} ennemis le bonus par cible doit rester celui de quatre \
             crans, soit {cran_a_cinq:.1}, et il vaut {cran:.1}"
        );
    }
}

/// En duel le sort ne monte jamais, et c'est voulu.
///
/// Un seul ennemi, donc aucun ennemi au-delà du premier, donc aucun cran : le
/// Pandawa qui tape seul voit les 13-16 du lancer nu à tous les tours. Contrôle
/// du contrôle des trois tests précédents, qui pourraient tous passer avec un
/// bonus qui s'appliquerait sans regarder le nombre de cibles.
#[test]
fn en_duel_la_distillation_ne_monte_pas() {
    let rs = regles();
    let t = tours(&rs, 1, 4);
    let nu = t[0].1;
    for (i, (ouverture, lancer)) in t.iter().enumerate().skip(1) {
        assert!(
            (lancer - nu).abs() < 0.01 && (ouverture - nu).abs() < 0.01,
            "tour {} : rien ne doit bouger en duel, {ouverture:.1} / {lancer:.1} contre {nu:.1}",
            i + 1
        );
    }
}

/// Le poison garde le bonus de sa pose jusqu'à sa DERNIÈRE frappe.
///
/// Le poison de trois tours frappe une dernière fois au début du tour où sa
/// durée s'achève : le drapeau qui dit qu'il a été posé avec le bonus doit
/// encore y être lu, d'où sa durée d'un tour de plus. Relance forcée à trois
/// tours pour que le poison du tour quatre, boosté, aille au bout sans être
/// rafraîchi : il tombe aux tours cinq, six et sept, au même montant.
#[test]
fn le_poison_garde_son_bonus_jusqu_a_sa_derniere_frappe() {
    let mut rs = regles();
    for s in rs.spells.iter_mut().filter(|s| s.id == "distillation") {
        s.cooldown_turns = 3;
    }
    let t = tours(&rs, 5, 7);
    let lances: Vec<usize> = (0..t.len()).filter(|&i| t[i].1 > 0.0).map(|i| i + 1).collect();
    assert_eq!(lances, [1, 4, 7], "{t:?}");
    let booste = t[3].1;
    for tour in [5, 6, 7] {
        assert!(
            (t[tour - 1].0 - booste).abs() < 0.01,
            "tour {tour} : le poison du tour quatre doit valoir {booste:.1}, il vaut {:.1}",
            t[tour - 1].0
        );
    }
}
