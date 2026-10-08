//! Les modes de lancer : un sort, plusieurs façons de le lancer, un seul
//! quota.
//!
//! Un sort du jeu peut faire deux choses distinctes selon ce qu'on vise, et le
//! joueur choisit à chaque lancer : l'Accumulation du Iop frappe un ennemi ou se
//! charge sur le lanceur. Trois règles tiennent le mécanisme, chacune testée sur
//! un banc dont les chiffres se calculent à la main :
//!
//! 1. les modes se partagent les lancers du tour ;
//! 2. un mode lancé met tout le sort en relance ;
//! 3. un mode lancé sur soi n'a qu'une cible, quel que soit le nombre
//!    d'ennemis.
//!
//! Les tests suivants rejouent les sorts concernés sur la vraie donnée :
//! l'Accumulation du Iop, le Javelot-foudre du Forgelance, le Pinceau Tribal de
//! l'Eniripsa et la Runification du Huppermage.

use dofus_damage::{DamageProfile, Element, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

/// Caractéristique 100, rien d'autre : un dégât de base vaut double.
fn profil() -> DamageProfile {
    DamageProfile {
        power: 0,
        flat_crit_damage: 0,
        elements: [ElementStats {
            characteristic: 100,
            flat_damage: 0,
        }; 5],
        ..Default::default()
    }
}

fn scenario(horizon: u8, targets: u8) -> Scenario {
    Scenario {
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
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    }
}

fn resoudre(rs: &Ruleset, deck: &[&str], base_ap: u8, horizon: u8, targets: u8) -> Solution {
    resoudre_avec(rs, deck, base_ap, profil(), scenario(horizon, targets))
}

fn resoudre_avec(
    rs: &Ruleset,
    deck: &[&str],
    base_ap: u8,
    profile: DamageProfile,
    sc: Scenario,
) -> Solution {
    let build = Build {
        name: "banc".into(),
        profile,
        base_ap,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| (*s).to_string()).collect(),
    };
    Engine::new(rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

fn noms(tour: &TurnPlan) -> Vec<&str> {
    tour.casts.iter().map(|c| c.spell.as_str()).collect()
}

/// Un sort, deux modes, sans aucune ressource : `fort` frappe 20, `faible`
/// frappe 10.
fn banc(lancers: u8, relance: u8) -> Ruleset {
    Ruleset::from_yaml(&format!(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: {{ fr: "Test", en: "Test" }}
resources: []
spells:
  - id: frappe
    name: {{ fr: "Frappe", en: "Strike" }}
    ap_cost: {{ base: 2 }}
    casts_per_turn: {lancers}
    cooldown_turns: {relance}
    crit: {{ base_rate: 0, can_crit: false }}
    lines:
      - element: water
        critical: [20, 20]
        normal: [20, 20]
    effects:
      - effect: damage
    mode_name: {{ fr: "fort", en: "strong" }}
    modes:
      - name: {{ fr: "faible", en: "weak" }}
        lines:
          - element: water
            critical: [10, 10]
            normal: [10, 10]
        effects:
          - effect: damage
"#
    ))
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Deux lancers par tour pour le SORT, pas deux par mode.
///
/// Sans partage, douze PA paient deux `fort` et deux `faible`, 40 + 40 + 20 +
/// 20 = 120. Le jeu n'en permet que deux : deux `fort`, 80.
#[test]
fn les_modes_se_partagent_les_lancers_du_tour() {
    let sol = resoudre(&banc(2, 0), &["frappe"], 12, 1, 1);
    let tour = &sol.turns[0];
    assert_eq!(
        noms(tour),
        ["Frappe (fort)", "Frappe (fort)"],
        "deux lancers au plus, et les deux au meilleur mode"
    );
    assert!(
        (sol.total.as_f64() - 80.0).abs() < 0.01,
        "deux fois 20 de base à caractéristique 100 : 80, mesuré {:.2}",
        sol.total.as_f64()
    );
}

/// Un mode lancé met les autres en relance.
///
/// Intervalle de 2 et un lancer par tour : le sort part aux tours 1 et 3. Si
/// chaque mode avait sa propre relance, `faible` remplirait les tours 2 et 4.
/// Un seul lancer par tour, pour que la relance soit la SEULE chose qui vide
/// les tours pairs.
#[test]
fn un_mode_lance_met_tout_le_sort_en_relance() {
    let sol = resoudre(&banc(1, 2), &["frappe"], 12, 4, 1);
    let lances: Vec<usize> = sol.turns.iter().map(|t| t.casts.len()).collect();
    assert_eq!(
        lances,
        [1, 0, 1, 0],
        "le sort repart un tour sur deux, quel que soit le mode : {:?}",
        sol.turns.iter().map(noms).collect::<Vec<_>>()
    );
}

/// Un mode sur soi n'a qu'une cible, même face à trois ennemis.
///
/// Une limite de un par cible se multiplie par le nombre d'ennemis pour un
/// mode qui les vise, pas pour celui qui vise le lanceur. Ici `sur soi` frappe
/// plus fort que `ennemi` : sans la règle, trois ennemis lui ouvriraient trois
/// lancers et le solveur les prendrait tous.
#[test]
fn un_mode_sur_soi_ne_compte_qu_une_cible() {
    let rs = Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources: []
spells:
  - id: frappe
    name: { fr: "Frappe", en: "Strike" }
    ap_cost: { base: 2 }
    casts_per_turn: 4
    casts_per_target: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      - element: water
        critical: [10, 10]
        normal: [10, 10]
    effects:
      - effect: damage
    mode_name: { fr: "ennemi", en: "enemy" }
    modes:
      - name: { fr: "sur soi", en: "on self" }
        on_self: true
        lines:
          - element: water
            critical: [50, 50]
            normal: [50, 50]
        effects:
          - effect: damage
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let sol = resoudre(&rs, &["frappe"], 12, 1, 3);
    let tour = &sol.turns[0];
    let sur_soi = noms(tour).iter().filter(|n| n.ends_with("(sur soi)")).count();
    assert_eq!(sur_soi, 1, "un seul lancer sur soi par tour : {:?}", noms(tour));
    assert_eq!(tour.casts.len(), 4, "quatre lancers en tout : {:?}", noms(tour));
    // 100 sur soi, puis trois fois 20 : 160.
    assert!(
        (sol.total.as_f64() - 160.0).abs() < 0.01,
        "100 + 3 × 20 attendu, mesuré {:.2}",
        sol.total.as_f64()
    );
}

/// Un sort qui pose un compteur plafonné à un ne part plus une fois le
/// plafond atteint, même s'il lui reste des lancers.
#[test]
fn un_sort_bloque_au_plafond_ne_part_plus() {
    let rs = Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - id: pose
    scope: target
    max: 1
    default: 0
    monotone: increasing
spells:
  - id: frappe
    name: { fr: "Frappe", en: "Strike" }
    ap_cost: { base: 2 }
    casts_per_turn: 3
    crit: { base_rate: 0, can_crit: false }
    blocked_at_max: pose
    lines:
      - element: water
        critical: [10, 10]
        normal: [10, 10]
    effects:
      - effect: damage
      - effect: gain
        resource: pose
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let sol = resoudre(&rs, &["frappe"], 12, 1, 1);
    assert_eq!(sol.turns[0].casts.len(), 1, "{:?}", noms(&sol.turns[0]));
}

fn classe(nom: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{nom}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    let rapport = rs.merge_snapshot(&snap);
    // Les lignes des modes se remplissent comme les autres, sans reste.
    let des_modes: Vec<String> = rapport
        .unmatched
        .iter()
        .cloned()
        .chain(rapport.conflicts.iter().map(|c| c.path.clone()))
        .filter(|p| p.contains(".modes["))
        .collect();
    assert!(des_modes.is_empty(), "{nom} : {des_modes:?}");
    rs
}

fn iop() -> Ruleset {
    classe("iop", 8)
}

/// L'Accumulation se charge sur soi, puis frappe chargée : la charge ne vient
/// que d'un lancer sur soi, qui ne frappe pas. Douze PA paient trois lancers,
/// le quota du sort : la charge d'abord, puis deux coups chargés, la limite de
/// deux par cible.
///
/// Six PA n'en paient que deux, et deux coups nus valent mieux qu'une charge
/// suivie d'un seul coup : ce tour-là donne la valeur d'un coup nu. L'écart
/// entre les deux est le bonus de 24, doublé par la caractéristique.
#[test]
fn accumulation_se_charge_sur_soi_avant_de_frapper() {
    let rs = iop();
    let charge = resoudre(&rs, &["accumulation"], 12, 1, 1);
    assert_eq!(
        noms(&charge.turns[0]),
        [
            "Accumulation (sur soi)",
            "Accumulation (sur un ennemi)",
            "Accumulation (sur un ennemi)"
        ]
    );
    let coups = &charge.turns[0].casts;
    assert!(
        (coups[0].damage.as_f64()).abs() < 0.01,
        "la charge ne frappe pas : {:.2}",
        coups[0].damage.as_f64()
    );
    assert!(
        (coups[1].damage.as_f64() - coups[2].damage.as_f64()).abs() < 0.01,
        "les deux coups portent la même charge"
    );

    let nu = resoudre(&rs, &["accumulation"], 6, 1, 1);
    assert_eq!(
        noms(&nu.turns[0]),
        ["Accumulation (sur un ennemi)", "Accumulation (sur un ennemi)"],
    );
    // ⚠️ LE SECOND COUP EST CELUI QUI TRAHIT LA CHARGE GRATUITE. Le premier est
    // nu dans les deux modèles ; avec l'ancien, le second frappait chargé.
    let nus = &nu.turns[0].casts;
    assert!(
        (nus[0].damage.as_f64() - nus[1].damage.as_f64()).abs() < 0.01,
        "un coup ne charge plus rien : {:.2} puis {:.2}",
        nus[0].damage.as_f64(),
        nus[1].damage.as_f64()
    );
    let ecart = coups[1].damage.as_f64() - nus[0].damage.as_f64();
    assert!(
        (ecart - 48.0).abs() <= 1.0,
        "24 de base à caractéristique 100 valent 48, mesuré {ecart:.2}"
    );
}

// ---------------------------------------------------------------------------
// Javelot-foudre : planter OU rebondir
// ---------------------------------------------------------------------------

fn place(lanceur: (i16, i16), visee: (i16, i16), ennemis: &[(i16, i16)]) -> Placement {
    Placement {
        lanceur: Case::new(lanceur.0, lanceur.1),
        visee: Case::new(visee.0, visee.1),
        ennemis: ennemis.iter().map(|e| Case::new(e.0, e.1)).collect(),
    }
}

fn sur_la_grille(horizon: u8, p: Placement) -> Scenario {
    let mut sc = scenario(horizon, u8::try_from(p.ennemis.len()).unwrap());
    sc.placement = Some(p);
    sc
}

/// Le mode qui frappe prend la même ligne que celui qui plante, et une zone
/// venue de la donnée.
#[test]
fn javelot_les_deux_modes_frappent_la_meme_ligne() {
    let rs = classe("forgelance", 20);
    let javelot = rs.spells.iter().find(|s| s.id == "javelot_foudre").unwrap();
    let principale = javelot.lines[0].area.as_ref().expect("zone du mode qui plante");
    let monstre = javelot.modes[0].lines[0].area.as_ref().expect("zone du mode sur un monstre");
    assert_eq!((principale.shape, monstre.shape), ('l', 'l'));
    assert!(javelot.modes[0].lines[0].normal.known() == Some(&(28, 32)));
}

/// Case vide : la Lance se plante. Monstre visé : le javelot rebondit, et
/// seulement s'il reste un ennemi à deux cases que la ligne n'a pas touché.
#[test]
fn javelot_la_case_visee_choisit_le_mode() {
    let rs = classe("forgelance", 20);
    let deck = ["javelot_foudre"];

    // Un ennemi sur la ligne, la case visée vide derrière lui.
    let vide = resoudre_avec(&rs, &deck, 12, profil(), sur_la_grille(1, place((-4, 0), (0, 0), &[(-2, 0)])));
    assert_eq!(noms(&vide.turns[0]), ["Javelot-foudre (planter la Lance)"]);

    // Monstre visé, un second à une case de lui : ligne plus rebond.
    let rebond = resoudre_avec(&rs, &deck, 12, profil(), sur_la_grille(1, place((-4, 0), (0, 0), &[(0, 0), (1, 0)])));
    // Monstre visé, le second hors de portée du rebond : la ligne seule.
    let seul = resoudre_avec(&rs, &deck, 12, profil(), sur_la_grille(1, place((-4, 0), (0, 0), &[(0, 0), (6, 6)])));
    for sol in [&rebond, &seul] {
        assert_eq!(
            noms(&sol.turns[0]),
            ["Javelot-foudre (sur un monstre)", "Javelot-foudre (sur un monstre)"],
            "monstre visé : jamais de Lance plantée"
        );
    }
    let (avec, sans) = (
        rebond.turns[0].casts[0].damage.as_f64(),
        seul.turns[0].casts[0].damage.as_f64(),
    );
    assert!(sans > 0.0);
    assert!(
        (avec - 2.0 * sans).abs() < 0.01,
        "le rebond frappe comme la ligne : {avec:.2} contre deux fois {sans:.2}"
    );
}

/// Ydra exige la Lance plantée à distance : elle ne part qu'après un Javelot
/// lancé sur une case vide. Monstre visé, aucune Lance, aucune Ydra, alors
/// que planter la débloquerait et rapporterait plus.
#[test]
fn javelot_sur_un_monstre_ne_debloque_pas_ydra() {
    let rs = classe("forgelance", 20);
    let deck = ["javelot_foudre", "ydra"];
    let vide = resoudre_avec(&rs, &deck, 12, profil(), sur_la_grille(1, place((-4, 0), (0, 0), &[(-2, 0)])));
    assert!(
        noms(&vide.turns[0]).contains(&"Ydra"),
        "Lance plantée, Ydra part : {:?}",
        noms(&vide.turns[0])
    );
    let monstre = resoudre_avec(&rs, &deck, 12, profil(), sur_la_grille(1, place((-4, 0), (0, 0), &[(0, 0), (1, 0)])));
    assert!(
        !noms(&monstre.turns[0]).contains(&"Ydra"),
        "monstre visé, pas de Lance : {:?}",
        noms(&monstre.turns[0])
    );
}

/// Sans placement, la ligne prend jusqu'à cinq ennemis ; le rebond n'a
/// quelqu'un qu'à partir du sixième, et le solveur ne plante plus.
#[test]
fn javelot_sans_placement_le_rebond_prend_le_sixieme() {
    let rs = classe("forgelance", 20);
    let cinq = resoudre(&rs, &["javelot_foudre"], 12, 1, 5);
    let six = resoudre(&rs, &["javelot_foudre"], 12, 1, 6);
    assert_eq!(
        noms(&six.turns[0]),
        ["Javelot-foudre (sur un monstre)", "Javelot-foudre (sur un monstre)"]
    );
    let (d5, d6) = (
        cinq.turns[0].casts[0].damage.as_f64(),
        six.turns[0].casts[0].damage.as_f64(),
    );
    assert!(
        (d6 - d5 * 6.0 / 5.0).abs() < 0.01,
        "six frappes contre cinq : {d6:.2} contre {d5:.2}"
    );
}

// ---------------------------------------------------------------------------
// Pinceau Tribal : peindre, propager, consommer
// ---------------------------------------------------------------------------

/// La propagation frappe avec la seconde ligne du sort, celle en cercle.
#[test]
fn pinceau_la_propagation_prend_la_ligne_en_cercle() {
    let rs = classe("eniripsa", 7);
    let pinceau = rs.spells.iter().find(|s| s.id == "pinceau_tribal").unwrap();
    let simple = pinceau.lines[0].area.as_ref().unwrap();
    let cercle = pinceau.modes[0].lines[0].area.as_ref().unwrap();
    assert_eq!((simple.shape, simple.size), ('P', Some(1)));
    assert_eq!((cercle.shape, cercle.size), ('C', Some(2)));
}

const PEINDRE: &str = "Pinceau Tribal (sur une cible non peinte)";
const PROPAGER: &str = "Pinceau Tribal (sur une cible peinte)";
const CONSOMMER: &str = "Pinceau Tribal (sur soi)";

/// Trois ennemis, aucun peint : on en peint un, et on ne propage pas depuis
/// lui dans le même tour, faute d'un autre ennemi peint. Propager
/// rapporterait pourtant trois frappes, bien plus que la consommation d'une
/// seule cible : c'est la règle qui l'interdit, pas le calcul.
#[test]
fn pinceau_ne_propage_pas_depuis_la_cible_qu_il_vient_de_peindre() {
    let rs = classe("eniripsa", 7);
    let sol = resoudre(&rs, &["pinceau_tribal"], 12, 1, 3);
    assert_eq!(noms(&sol.turns[0]), [PEINDRE, CONSOMMER]);
}

/// Un seul ennemi ne se peint qu'une fois. Sans plafond, deux Peintures sur
/// lui feraient consommer deux cibles au tour suivant.
#[test]
fn pinceau_un_seul_ennemi_ne_se_peint_qu_une_fois() {
    let rs = classe("eniripsa", 7);
    let sol = resoudre(&rs, &["pinceau_tribal"], 12, 2, 1);
    for tour in &sol.turns {
        assert_eq!(noms(tour), [PEINDRE, CONSOMMER]);
    }
}

/// La propagation peint JUSQU'À ce que sa zone atteint, treize ennemis pour
/// le cercle de deux cases : deux cibles déjà peintes ne s'y ajoutent pas.
/// La consommation qui suit frappe donc treize fois, à +40 de base, le
/// cinquième cran étant le dernier.
#[test]
fn pinceau_la_propagation_peint_jusqu_a_sa_zone() {
    let rs = classe("eniripsa", 7);
    let sol = resoudre(&rs, &["pinceau_tribal"], 12, 2, 14);
    assert_eq!(noms(&sol.turns[1]), [PROPAGER, CONSOMMER]);
    let propagation = sol.turns[1].casts[0].damage.as_f64();
    let consommation = sol.turns[1].casts[1].damage.as_f64();
    let attendu = propagation + 13.0 * 80.0;
    assert!(
        (consommation - attendu).abs() <= 6.5,
        "treize frappes à +40 de base attendues vers {attendu:.2}, mesuré {consommation:.2}"
    );
}

/// Trois ennemis sur deux tours : deux cibles peintes, puis une propagation
/// qui les peint toutes et la consommation qui frappe les trois, chacune avec
/// 8 de base par ennemi peint.
#[test]
fn pinceau_propage_puis_consomme_sur_tous_les_ennemis() {
    let rs = classe("eniripsa", 7);
    let sol = resoudre(&rs, &["pinceau_tribal"], 12, 2, 3);
    assert_eq!(noms(&sol.turns[0]), [PEINDRE, PEINDRE]);
    assert_eq!(noms(&sol.turns[1]), [PROPAGER, CONSOMMER]);
    let propagation = sol.turns[1].casts[0].damage.as_f64();
    let consommation = sol.turns[1].casts[1].damage.as_f64();
    // Trois frappes nues, puis trois frappes à +24 de base, doublé par la
    // caractéristique. Les deux lignes ont les mêmes fourchettes.
    let attendu = propagation + 3.0 * 48.0;
    assert!(
        (consommation - attendu).abs() <= 1.5,
        "{consommation:.2} attendu vers {attendu:.2}"
    );
}

/// Les deux compteurs de la peinture déclarent un sens : le vérifier contre la
/// recherche sans élagage.
#[test]
fn pinceau_l_elagage_ne_change_rien() {
    let rs = classe("eniripsa", 7);
    let mut sans = scenario(3, 3);
    sans.dominance = false;
    let elague = resoudre(&rs, &["pinceau_tribal"], 12, 3, 3).total.as_f64();
    let complet = resoudre_avec(&rs, &["pinceau_tribal"], 12, profil(), sans).total.as_f64();
    assert!((elague - complet).abs() < 0.01, "{elague:.2} contre {complet:.2}");
}

// ---------------------------------------------------------------------------
// Runification : l'élément de la rune
// ---------------------------------------------------------------------------

/// Feu à 300, le reste à zéro : une frappe Feu de 15 vaut 60, une frappe
/// d'un autre élément 15.
fn profil_feu() -> DamageProfile {
    let mut p = profil();
    p.elements = [ElementStats {
        characteristic: 0,
        flat_damage: 0,
    }; 5];
    p.elements[Element::Fire.index()].characteristic = 300;
    p
}

/// La rune prend l'élément du sort qui la pose, et la Runification ne frappe
/// que dans celui-là. Avant, ses quatre éléments s'additionnaient : 105.
#[test]
fn runification_frappe_dans_l_element_de_la_rune() {
    let rs = classe("huppermage", 17);
    let sol = resoudre_avec(&rs, &["lance_flamme", "runification"], 12, profil_feu(), scenario(1, 1));
    let tour = &sol.turns[0];
    assert_eq!(
        tour.casts.first().map(|c| c.spell.as_str()),
        Some("Lance-flamme"),
        "pas de rune, pas de Runification : {:?}",
        noms(tour)
    );
    let runes: Vec<f64> = tour
        .casts
        .iter()
        .filter(|c| c.id == "runification")
        .map(|c| c.damage.as_f64())
        .collect();
    assert_eq!(runes.len(), 2, "{:?}", noms(tour));
    // La PREMIÈRE frappe au nu : quinze de base à 300 d'Intelligence, soit 60.
    assert!(
        (runes[0] - 60.0).abs() < 0.01,
        "une frappe Feu de 15 vaut 60, mesuré {:.2}",
        runes[0]
    );
    // La seconde porte les cinquante points d'Intelligence que la première a
    // donnés, effet 126, « selon l'élément de la rune » : 350 au lieu de 300,
    // donc 67,5 arrondi à 67.
    assert!(
        (runes[1] - 67.0).abs() < 0.01,
        "la seconde frappe porte les +50 d'Intelligence de la première, 67 attendu, mesuré {:.2}",
        runes[1]
    );
}

/// Sur soi, une rune vivante par cible, et jamais plus de runes qu'il n'y en a :
/// « Déclenche toutes ses runes occupées par une entité ». À trois ennemis et
/// trois runes, trois frappes ; à trois ennemis et une seule rune, une seule.
#[test]
fn runification_sur_soi_ne_declenche_que_les_runes_vivantes() {
    let rs = classe("huppermage", 17);
    // Douze PA : trois Lance-flamme posent trois runes, la Runification les
    // déclenche toutes les trois.
    let sol = resoudre_avec(&rs, &["lance_flamme", "runification"], 12, profil_feu(), scenario(1, 3));
    let tour = &sol.turns[0];
    let runes: Vec<&Cast> = tour.casts.iter().filter(|c| c.id == "runification").collect();
    assert_eq!(runes.len(), 1, "{:?}", noms(tour));
    assert_eq!(runes[0].spell, "Runification (sur soi)");
    assert!(
        (runes[0].damage.as_f64() - 180.0).abs() < 0.01,
        "trois runes Feu à 60, mesuré {:.2}",
        runes[0].damage.as_f64()
    );

    // Cinq PA : une seule Lance-flamme, donc une seule rune, et la Runification
    // ne frappe qu'une fois malgré les trois ennemis.
    let sol = resoudre_avec(&rs, &["lance_flamme", "runification"], 5, profil_feu(), scenario(1, 3));
    let tour = &sol.turns[0];
    let runes: Vec<&Cast> = tour.casts.iter().filter(|c| c.id == "runification").collect();
    assert_eq!(runes.len(), 1, "{:?}", noms(tour));
    assert!(
        (runes[0].damage.as_f64() - 60.0).abs() < 0.01,
        "une seule rune vivante : 60 et non 180, mesuré {:.2}",
        runes[0].damage.as_f64()
    );
}

// ---------------------------------------------------------------------------
// Surcharge Runique : les runes vivantes, tour par tour
// ---------------------------------------------------------------------------

/// Trois compteurs qui glissent, une pose et une lecture.
fn banc_runes(defauts: (u8, u8, u8), lire_des: u8, prendre: bool) -> Ruleset {
    let prend = if prendre {
        r#"
  - id: prend
    name: { fr: "Prend", en: "Take" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    cooldown_turns: 5
    crit: { base_rate: 0, can_crit: false }
    lines:
      - { element: water, critical: [50, 50], normal: [50, 50] }
    effects:
      - effect: damage
      - effect: consume_across
        resources: [a, b, c]
        amount: 2
"#
    } else {
        ""
    };
    Ruleset::from_yaml(&format!(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: {{ fr: "Test", en: "Test" }}
resources:
  - {{ id: a, scope: caster, max: 6, default: {}, monotone: increasing, shifts_into: b }}
  - {{ id: b, scope: caster, max: 6, default: {}, monotone: increasing, shifts_into: c }}
  - {{ id: c, scope: caster, max: 6, default: {}, monotone: increasing }}
spells:
  - id: pose
    name: {{ fr: "Pose", en: "Lay" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 2
    crit: {{ base_rate: 0, can_crit: false }}
    lines: []
    effects:
      - {{ effect: gain, resource: a }}
  - id: lit
    name: {{ fr: "Lit", en: "Read" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 1
    initial_cooldown: {lire_des}
    crit: {{ base_rate: 0, can_crit: false }}
    lines:
      - element: water
        critical: [10, 10]
        normal: [10, 10]
        repeats_per: [a, b, c]
    effects:
      - effect: damage
{prend}"#,
        defauts.0, defauts.1, defauts.2
    ))
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Une pose vit le tour où elle tombe et les deux suivants, puis disparaît.
///
/// Deux poses par tour, et la lecture n'ouvre qu'au quatrième tour : elle y
/// voit les poses des tours 2 et 3, plus celle qu'elle fait juste avant,
/// soit cinq. Celles du tour 1 sont mortes. Sans glissement, les sept
/// resteraient ; avec une vie trop courte, il n'en resterait que trois.
#[test]
fn une_pose_vit_deux_tours_apres_le_sien() {
    let rs = banc_runes((0, 0, 0), 3, false);
    let sol = resoudre(&rs, &["pose", "lit"], 2, 4, 1);
    assert!(
        (sol.total.as_f64() - 100.0).abs() < 0.01,
        "cinq poses vivantes à 20 : 100 attendu, mesuré {:.2} ({:?})",
        sol.total.as_f64(),
        sol.turns.iter().map(noms).collect::<Vec<_>>()
    );
}

/// Le retrait réparti puise dans l'ordre donné : les poses récentes d'abord.
///
/// Au départ, deux poses d'un tour (`b`) et trois de deux tours (`c`). Le
/// retrait de deux vide `b` ; celles de `c` meurent en fin de tour. La lecture
/// du tour 2 ne voit donc rien. Puiser dans `c` d'abord en laisserait deux.
#[test]
fn le_retrait_reparti_puise_dans_l_ordre() {
    let rs = banc_runes((0, 2, 3), 1, true);
    let sol = resoudre(&rs, &["pose", "lit", "prend"], 1, 2, 1);
    assert!(
        (sol.total.as_f64() - 100.0).abs() < 0.01,
        "le retrait seul frappe, 100 attendu, mesuré {:.2} ({:?})",
        sol.total.as_f64(),
        sol.turns.iter().map(noms).collect::<Vec<_>>()
    );
}

/// Sur trois tours, la Surcharge frappe une fois par Lance-flamme lancé :
/// les runes du premier tour vivent encore au troisième.
#[test]
fn surcharge_frappe_une_fois_par_rune_vivante() {
    let rs = classe("huppermage", 17);
    let sol = resoudre_avec(
        &rs,
        &["lance_flamme", "surcharge_runique"],
        12,
        profil_feu(),
        scenario(3, 1),
    );
    let tous: Vec<&Cast> = sol.turns.iter().flat_map(|t| &t.casts).collect();
    let poses = tous.iter().filter(|c| c.id == "lance_flamme").count();
    let surcharge: f64 = tous
        .iter()
        .filter(|c| c.id == "surcharge_runique")
        .map(|c| c.damage.as_f64())
        .sum();
    assert_eq!(poses, 6, "{:?}", sol.turns.iter().map(noms).collect::<Vec<_>>());
    assert!(
        (surcharge - 6.0 * 40.0).abs() < 0.01,
        "six runes Feu à 40 : 240 attendu, mesuré {surcharge:.2}"
    );
}
