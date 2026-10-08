//! Les glyphes du Féca : un seul par tour, une aura sur les trois, et des
//! glyphes qui frappent. Les deux premiers tirent le total en sens opposés : un
//! correctif à moitié appliqué passerait si on ne regardait que le sens.
//!
//! 1. « Empêche l'utilisation d'autres glyphes élémentaires dans le même
//!    tour. » Huit sorts portent la phrase, et la donnée aussi :
//!    `states_criterion` vaut « HS!238 » sur les huit (« le lanceur n'a pas
//!    l'état 238 »), et chacun s'applique l'état 238 en se lançant.
//! 2. Le glyphe-aura du Pâturage donne 10 % de dommages finaux aux alliés,
//!    lanceur compris s'il se tient dessus, et seulement si le joueur le
//!    déclare ; le Refuge réduit les dommages subis, le Verglas donne de la
//!    Fuite.
//! 3. La rotation suppose une cible immobile, qui reste dans chaque glyphe posé
//!    sur elle : deux coups au début de ses tours pour un glyphe élémentaire, un
//!    pour la Défiance, et un de plus pour chaque glyphe encore au sol quand la
//!    Transhumance le déclenche, glyphes-auras compris.

use dofus_damage::{
    expected_line, CritRate, DamageProfile, Element, ElementStats, FinalMultiplier, Resistance,
    SpellLine,
};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn regles() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/feca.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-1.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn profil() -> DamageProfile {
    DamageProfile {
        power: 170,
        flat_crit_damage: 0,
        elements: [ElementStats {
            characteristic: 630,
            flat_damage: 56,
        }; 5],
        ..Default::default()
    }
}

fn solve(rs: &Ruleset, deck: &[&str], sur_le_glyphe: u8, horizon: u8) -> Solution {
    solve_pa(rs, deck, sur_le_glyphe, horizon, 12)
}

fn solve_pa(rs: &Ruleset, deck: &[&str], sur_le_glyphe: u8, horizon: u8, pa: u8) -> Solution {
    let build = Build {
        name: "feca".into(),
        profile: profil(),
        base_ap: pa,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| (*s).to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: vec![("sur_le_glyphe_aura".to_string(), sur_le_glyphe)],
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

const GLYPHES: &[&str] = &["paturage", "refuge", "verglas", "vigie"];

/// Un seul glyphe élémentaire par tour, jamais deux.
///
/// Quatre glyphes à 3 PA tiennent dans un tour de 12 PA, et le solveur les
/// posait tous les quatre. C'est le compte des lancers qui se vérifie ici, pas
/// un total : un total peut baisser pour de mauvaises raisons.
#[test]
fn un_seul_glyphe_elementaire_par_tour() {
    let rs = regles();
    let sol = solve(&rs, GLYPHES, 0, 3);
    for (i, tour) in sol.turns.iter().enumerate() {
        // Le deck ne contient QUE des glyphes élémentaires : tout lancer du
        // tour en est un, et il n'y a rien à filtrer. Un filtre sur le nom
        // serait ici un filtre qui ne filtre rien, et le test compterait juste
        // sans qu'on puisse le voir.
        let poses = tour.casts.len();
        assert!(
            poses <= 1,
            "tour {} : {poses} glyphes posés, le jeu n'en autorise qu'un : {:?}",
            i + 1,
            tour.casts.iter().map(|c| &c.spell).collect::<Vec<_>>()
        );
    }
}

/// Le verrou tombe bien à la fin du tour, il ne dure pas.
///
/// « Dans le même tour », dit le texte. Un état qui vivrait un tour de plus
/// interdirait le glyphe du tour suivant, et le test précédent passerait quand
/// même en ne posant qu'un glyphe tous les deux tours.
#[test]
fn le_verrou_tombe_a_la_fin_du_tour() {
    let rs = regles();
    let sol = solve(&rs, &["paturage"], 0, 4);
    assert_eq!(sol.turns.len(), 4);
    for (i, tour) in sol.turns.iter().enumerate() {
        assert_eq!(
            tour.casts.len(),
            1,
            "tour {} : le Pâturage doit repartir à chaque tour : {:?}",
            i + 1,
            tour.casts.iter().map(|c| &c.spell).collect::<Vec<_>>()
        );
    }
}

/// Déclarer qu'on se tient sur le glyphe vaut 10 % de dommages finaux, mais
/// seulement le tour de sa pose, et pour ce qui part après lui.
///
/// Pas de glyphe, pas d'aura : le glyphe-aura ne vit que le tour où il est
/// posé, bien que la donnée écrive 2. Le Pâturage est mis en relance pour ne
/// pas repartir au tour 2 : ce tour-là ne doit rien gagner au réglage. Au tour
/// 1 il part en premier, frappe sans son propre bonus, et ce qui le suit prend
/// ses 10 %.
#[test]
fn l_aura_du_paturage_ne_vit_que_le_tour_de_sa_pose() {
    let mut rs = regles();
    rs.spells
        .iter_mut()
        .find(|s| s.id == "paturage")
        .expect("le Pâturage est au fichier")
        .cooldown_turns = 2;
    let deck = &["paturage", "retour_du_baton"];
    let sans = solve(&rs, deck, 0, 2);
    let avec = solve(&rs, deck, 1, 2);

    let (t_sans, t_avec) = (sans.turns[1].damage.as_f64(), avec.turns[1].damage.as_f64());
    assert!(
        (t_sans - t_avec).abs() < 0.01,
        "tour 2, sans Pâturage : rien à gagner, {t_sans:.1} contre {t_avec:.1}"
    );

    let premier = &avec.turns[0];
    assert_eq!(
        premier.casts[0].id, "paturage",
        "le Pâturage doit ouvrir le tour pour que la suite en profite"
    );
    let coup = |sol: &Solution, sort: &str| -> f64 {
        sol.turns[0]
            .casts
            .iter()
            .find(|c| c.id == sort)
            .unwrap_or_else(|| panic!("{sort} absent du tour 1"))
            .damage
            .as_f64()
    };
    assert!(
        (coup(&avec, "paturage") - coup(&sans, "paturage")).abs() < 0.01,
        "le Pâturage frappe avant de poser son aura"
    );
    let attendu = (coup(&sans, "retour_du_baton") * 1.10).floor();
    for c in premier.casts.iter().skip(1) {
        assert!(
            (c.damage.as_f64() - attendu).abs() <= 1.0,
            "{} après le Pâturage : {:.1} attendu, {:.1} mesuré",
            c.spell,
            attendu,
            c.damage.as_f64()
        );
    }
}

/// Ni le Refuge ni le Verglas ne touchent aux dégâts.
///
/// Leurs auras protègent et donnent de la Fuite. Le réglage ne doit rien leur
/// faire, sans quoi la même réserve recopiée sur les trois sorts aurait fini
/// par se traduire en trois bonus.
#[test]
fn les_deux_autres_auras_ne_changent_rien() {
    let rs = regles();
    for sort in ["refuge", "verglas"] {
        let sans = solve(&rs, &[sort], 0, 3).total.as_f64();
        let avec = solve(&rs, &[sort], 1, 3).total.as_f64();
        assert!(
            (sans - avec).abs() < 0.01,
            "{sort} : le réglage ne doit rien changer, {sans:.1} contre {avec:.1}"
        );
    }
}

/// Ce qu'une ligne vaut avec le build des tests, pris au moteur de dégâts.
fn attendu(element: Element, normal: (i32, i32), critique: (i32, i32), taux: CritRate) -> f64 {
    expected_line(
        &SpellLine {
            element,
            normal,
            critical: critique,
        },
        &profil(),
        FinalMultiplier::NEUTRAL,
        taux,
        &Resistance::NONE,
    )
    .as_f64()
}

/// Ce qu'un sort fait tomber au début de chaque tour, par ses glyphes.
fn ouverture(sol: &Solution, sort: &str) -> Vec<f64> {
    sol.turns
        .iter()
        .map(|t| {
            t.opening_sources
                .iter()
                .filter(|(s, _)| s == sort)
                .map(|(_, d)| d.as_f64())
                .sum()
        })
        .collect()
}

/// Ce que les déclenchements rendent à un sort, tour par tour.
fn declenche(sol: &Solution, sort: &str) -> Vec<f64> {
    sol.turns
        .iter()
        .map(|t| {
            t.casts
                .iter()
                .flat_map(|c| c.procs.iter())
                .filter(|(s, _)| s == sort)
                .map(|(_, d)| d.as_f64())
                .sum()
        })
        .collect()
}

/// Un glyphe de début de tour frappe DEUX fois, au début des deux tours qui
/// suivent sa pose, puis s'éteint.
///
/// Sur quatre tours, la Terre Brûlée repart au quatrième et ce glyphe-là
/// frappe hors de l'horizon : deux coups exactement, chacun valant le Feu
/// 30-34 du sous-sort. Un coup de plus ou de moins se voit.
#[test]
fn un_glyphe_frappe_la_cible_deux_fois() {
    let rs = regles();
    let sol = solve(&rs, &["terre_brulee"], 0, 4);
    let coup = attendu(Element::Fire, (30, 34), (30, 34), CritRate::NEVER);
    let v = ouverture(&sol, "terre_brulee");
    let coups: Vec<f64> = v.iter().copied().filter(|d| *d > 0.0).collect();
    assert_eq!(coups.len(), 2, "deux coups attendus sur quatre tours : {v:?}");
    for c in coups {
        assert!((c - coup).abs() < 0.01, "{c:.2} mesuré contre {coup:.2} : {v:?}");
    }
}

/// La Transhumance fait frapper une fois de plus un glyphe encore au sol, et
/// le coup revient au sort qui l'a posé.
#[test]
fn la_transhumance_declenche_les_glyphes_au_sol() {
    let rs = regles();
    let sol = solve(&rs, &["prairie", "transhumance"], 0, 1);
    let coup = attendu(Element::Air, (31, 35), (31, 35), CritRate::NEVER);
    let v = declenche(&sol, "prairie");
    assert!(
        (v[0] - coup).abs() < 0.01,
        "Prairie puis Transhumance : {:.2} attendu, {v:?} mesuré",
        coup
    );
}

/// Un glyphe de deux tours posé au tour N reste au sol jusqu'à la fin du tour
/// N+1, et pas au-delà : la règle des durées.
///
/// La Transhumance est retardée pour ne partir qu'au tour 2, puis qu'au tour 3.
/// Au tour 2 elle trouve la Prairie du tour 1 : un coup de début de tour et un
/// déclenché. Au tour 3 elle ne la trouve plus : les deux coups de début de tour
/// de la cible, et rien de déclenché. Deux coups dans les deux cas, là où un
/// glyphe qui durerait un tour de plus en donnerait trois.
#[test]
fn un_glyphe_reste_au_sol_jusqu_a_la_fin_du_tour_n_plus_un() {
    let coup = attendu(Element::Air, (31, 35), (31, 35), CritRate::NEVER);
    for delai in [1u8, 2] {
        let mut rs = regles();
        rs.spells
            .iter_mut()
            .find(|s| s.id == "transhumance")
            .expect("la Transhumance est au fichier")
            .initial_cooldown = delai;
        let sol = solve(&rs, &["prairie", "transhumance"], 0, delai + 1);
        let total = sol.total.as_f64();
        assert!(
            (total - 2.0 * coup).abs() < 0.01,
            "Transhumance au tour {} : deux coups attendus ({:.2}), {total:.2} mesuré",
            delai + 1,
            2.0 * coup
        );
    }
}

/// Les glyphes-auras ne frappent QUE déclenchés, et leur coup critique au taux
/// de base de leur sous-sort, 1 %, et non aux 10 % du Pâturage.
#[test]
fn le_glyphe_aura_ne_frappe_que_declenche_et_critique_a_un_pour_cent() {
    let rs = regles();
    let seul = solve(&rs, &["paturage"], 0, 3);
    assert!(
        ouverture(&seul, "paturage").iter().all(|d| *d == 0.0),
        "sans Transhumance, le glyphe-aura ne doit rien faire tomber"
    );

    let sol = solve(&rs, &["paturage", "transhumance"], 0, 1);
    let bon = attendu(Element::Air, (31, 35), (37, 42), CritRate::from_percent(1));
    let faux = attendu(Element::Air, (31, 35), (37, 42), CritRate::from_percent(10));
    assert!(
        (bon - faux).abs() > 0.05,
        "les deux taux donnent le même chiffre : le test ne distingue plus rien"
    );
    let v = declenche(&sol, "paturage");
    assert!(
        (v[0] - bon).abs() < 0.01,
        "{:.2} attendu à 1 % de critique ({:.2} à 10 %), {v:?} mesuré",
        bon,
        faux
    );
}

/// La Défiance frappe une fois, dans les quatre éléments : son glyphe de fin
/// de tour ne dure qu'un tour.
#[test]
fn la_defiance_frappe_une_fois_dans_les_quatre_elements() {
    let rs = regles();
    let sol = solve(&rs, &["defiance"], 0, 3);
    let coup: f64 = [Element::Earth, Element::Fire, Element::Water, Element::Air]
        .into_iter()
        .map(|e| attendu(e, (21, 22), (21, 22), CritRate::NEVER))
        .sum();
    let v = ouverture(&sol, "defiance");
    let coups: Vec<f64> = v.iter().copied().filter(|d| *d > 0.0).collect();
    assert_eq!(coups.len(), 1, "un seul coup attendu sur trois tours : {v:?}");
    assert!(
        (coups[0] - coup).abs() < 0.01,
        "{:.2} attendu, {v:?} mesuré",
        coup
    );
}

/// Une aura ne se déclenche que dans le tour de sa pose, bien que la donnée
/// écrive 2. Trois PA ne laissent passer qu'un sort par tour, et le Pâturage est
/// mis en relance pour ne pas repartir au tour 2 : la Transhumance, qui ne part
/// qu'au tour 2, n'y trouve plus l'aura du tour 1, et le total est le seul coup
/// du Pâturage. Dans le même tour, elle la déclenche : c'est le test du taux de
/// critique, plus haut.
#[test]
fn une_aura_ne_se_declenche_que_dans_son_tour_de_pose() {
    let mut rs = regles();
    rs.spells
        .iter_mut()
        .find(|s| s.id == "transhumance")
        .expect("la Transhumance est au fichier")
        .initial_cooldown = 1;
    rs.spells
        .iter_mut()
        .find(|s| s.id == "paturage")
        .expect("le Pâturage est au fichier")
        .cooldown_turns = 2;
    let seul = solve_pa(&rs, &["paturage"], 0, 2, 3).total.as_f64();
    let sol = solve_pa(&rs, &["paturage", "transhumance"], 0, 2, 3);
    let v = declenche(&sol, "paturage");
    assert!(
        v.iter().all(|d| *d == 0.0),
        "l'aura du tour 1 ne doit plus se déclencher au tour 2 : {v:?}"
    );
    assert!(
        (sol.total.as_f64() - seul).abs() < 0.01,
        "{:.2} mesuré, {seul:.2} attendu : le seul coup du Pâturage",
        sol.total.as_f64()
    );
}
