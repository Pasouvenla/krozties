//! Les bonus de Dofus, qu'ils arrivent par les bonus de DofusBook, par la
//! forgemagie `deg` posée sur l'emplacement du Dofus, ou par les deux à la fois
//! sans double compte.

use dofus_build::*;

fn catalogue() -> Catalogue {
    Catalogue::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/snapshots/items.json"))
        .unwrap()
}

fn build_de_tim() -> BuildInput {
    serde_json::from_str(include_str!("../../../examples/pandawa-eau-air.json"))
        .expect("le build doit se lire")
}

fn noms(r: &Resolved) -> Vec<(&str, u32)> {
    r.damage_multipliers.iter().map(|(n, p)| (n.as_str(), *p)).collect()
}

/// Chaque bonus compte une fois : le Nébuleux et le Turquoise par leurs bonus
/// actifs, leur forgemagie reconnue comme le même bonus ; le Pourpre par sa
/// forgemagie. Plus rien d'inconnu ni d'ignoré.
#[test]
fn chaque_bonus_de_dofus_compte_une_fois() {
    let r = resolve(&build_de_tim(), &catalogue());
    assert_eq!(
        noms(&r),
        vec![
            ("Rêve Nébuleux, tours impairs", 120),
            ("Rêve Nébuleux, tours pairs", 90),
            ("Bleu Turquoise", 110),
            ("Pourpre Profond", 105),
            // Le Vulbis porté, qu'aucun bonus de DofusBook ne signale.
            ("Rouge Vermeil", 110),
        ]
    );
    // L'Ocre porté : un PA de plus à chaque tour, hors des PA de la fiche.
    assert_eq!(r.pa_des_dofus, vec![("Jaune Ocre".to_string(), 1)]);
    assert!(!r.assumptions.iter().any(|a| a.contains("inconnue")), "{:#?}", r.assumptions);
    assert!(r.uninterpreted.is_empty(), "{:#?}", r.uninterpreted);
    for nom in ["Rêve Nébuleux", "Bleu Turquoise"] {
        assert!(
            r.assumptions.iter().any(|a| a.contains(nom) && a.contains("une seule fois")),
            "{nom} : {:#?}",
            r.assumptions
        );
    }
    assert!(
        r.assumptions.iter().any(|a| a.contains("Pourpre Profond") && a.contains("comptés comme son bonus")),
        "{:#?}",
        r.assumptions
    );
}

/// Sans les bonus de DofusBook, la forgemagie seule donne les mêmes bonus,
/// avec leurs règles : le Rêve Nébuleux reste tour par tour.
#[test]
fn un_bonus_saisi_en_forgemagie_devient_le_bonus_du_dofus() {
    let mut build = build_de_tim();
    build.boosts.clear();
    let r = resolve(&build, &catalogue());
    assert_eq!(
        noms(&r),
        vec![
            ("Bleu Turquoise", 110),
            ("Pourpre Profond", 105),
            ("Rêve Nébuleux, tours impairs", 120),
            ("Rêve Nébuleux, tours pairs", 90),
            ("Rouge Vermeil", 110),
        ]
    );
    assert_eq!(r.tours.get("Rêve Nébuleux, tours pairs"), Some(&Tours::Pairs));
    // Le Dofus Nébuleux n'est pas signalé : son bonus arrive par la forgemagie.
    assert!(r.uninterpreted.is_empty(), "{:#?}", r.uninterpreted);
}

/// Hors d'un Dofus qui en donne, la forgemagie `deg` compte comme des % de
/// dommages finaux ordinaires, et le dit.
#[test]
fn une_forgemagie_deg_ailleurs_est_un_pourcentage_de_dommages_finaux() {
    let mut build = build_de_tim();
    build.boosts.clear();
    build.fm = [("a1".to_string(), [("deg".to_string(), 7)].into_iter().collect())].into_iter().collect();
    let r = resolve(&build, &catalogue());
    assert_eq!(
        noms(&r),
        vec![
            ("% dommages finaux (forgemagie)", 107),
            // Les Dofus portés comptent toujours.
            ("Rêve Nébuleux, tours impairs", 120),
            ("Rêve Nébuleux, tours pairs", 90),
            ("Rouge Vermeil", 110),
        ]
    );
    assert!(
        r.assumptions.iter().any(|a| a.contains("+7") && a.contains("hors d'un Dofus")),
        "{:#?}",
        r.assumptions
    );
}

/// L'Harmonie de Pandala est l'effet du Dofus Tacheté : sans lui elle n'ajoute
/// rien, même active ; porté, ses 20 Dommages comptent, bonus actif ou non.
#[test]
fn l_harmonie_de_pandala_vient_du_tachete() {
    let sans = resolve(&build_de_tim(), &catalogue());
    assert!(
        sans.assumptions.iter().any(|a| a.contains("Harmonie de Pandala") && a.contains("Dofus Tacheté")),
        "{:#?}",
        sans.assumptions
    );
    let eau = dofus_damage::Element::Water.index();
    let fixes = |b: &BuildInput| resolve(b, &catalogue()).profile.elements[eau].flat_damage;
    // Le Tacheté à la place du Dofus des Glaces, comparé à l'emplacement vide.
    let mut vide = build_de_tim();
    vide.items[11] = 0;
    let mut tachete = build_de_tim();
    tachete.items[11] = 7112;
    let propres: i32 = catalogue()
        .item(7112)
        .map(|i| ["damage_water", "damage_all"].iter().filter_map(|k| i.stats.get(*k)).map(|r| r[1]).sum())
        .unwrap_or(0);
    assert_eq!(fixes(&tachete) - fixes(&vide) - propres, 20, "les 20 Dommages de l'Harmonie");
    let mut sans_bonus = tachete.clone();
    sans_bonus.boosts.retain(|x| x.name != "Harmonie de Pandala");
    assert_eq!(fixes(&sans_bonus), fixes(&tachete), "porté, il compte même sans bonus actif");
}

/// Le Nébuleux porté compte aussi sans bonus ni forgemagie : son effet ne
/// dépend que du tour.
#[test]
fn le_nebuleux_porte_compte_sans_bonus() {
    let mut build = build_de_tim();
    build.boosts.clear();
    build.fm.clear();
    let r = resolve(&build, &catalogue());
    assert_eq!(
        noms(&r),
        vec![
            ("Rêve Nébuleux, tours impairs", 120),
            ("Rêve Nébuleux, tours pairs", 90),
            ("Rouge Vermeil", 110),
        ]
    );
    assert!(r.uninterpreted.is_empty(), "{:#?}", r.uninterpreted);
}

/// Un bonus de Dofus actif ne compte que si son Dofus est dans le build : le
/// Bleu Turquoise sans le Dofus Turquoise n'ajoute rien, le dit, et n'a pas de
/// case dans l'onglet Rotation.
#[test]
fn un_bonus_de_dofus_sans_son_dofus_ne_compte_pas() {
    let mut build = build_de_tim();
    build.items[13] = 0;
    let r = resolve(&build, &catalogue());
    assert!(!noms(&r).iter().any(|(n, _)| *n == "Bleu Turquoise"), "{:?}", noms(&r));
    assert!(
        r.assumptions.iter().any(|a| a.contains("Bleu Turquoise non compté sans le Dofus Turquoise")),
        "{:#?}",
        r.assumptions
    );
    assert!(!r.bonus_conditionnels.iter().any(|b| b.nom == "Bleu Turquoise"));
}

/// Chaque bonus qui dépend du combat a sa case dans l'onglet Rotation, avec ce
/// qu'il donne : ceux des Dofus du build, et eux seuls.
#[test]
fn les_bonus_conditionnels_ont_leur_case() {
    let r = resolve(&build_de_tim(), &catalogue());
    let cases: Vec<(&str, &str)> =
        r.bonus_conditionnels.iter().map(|b| (b.nom.as_str(), b.effet.as_str())).collect();
    assert_eq!(
        cases,
        vec![
            ("Rouge Vermeil", "+10 % de dommages finaux"),
            ("Jaune Ocre", "+1 PA"),
            ("Bleu Turquoise", "+10 % de dommages finaux"),
            ("Pourpre Profond", "+5 % de dommages finaux"),
        ]
    );
}

/// Les autres Dofus dont l'effet touche une rotation : l'Abyssal (+1 PA au
/// contact, +1 PM sinon, avec sa case), le Sylvestre (+8 Puissance par PM
/// dépensé), le Cauchemar (+100 Puissance les tours de poussée).
#[test]
fn l_abyssal_le_sylvestre_et_le_cauchemar_comptent() {
    let mut build = build_de_tim();
    build.boosts.clear();
    build.fm.clear();
    build.items[11] = 18043;
    build.items[12] = 29136;
    build.items[13] = 26066;
    let r = resolve(&build, &catalogue());
    assert!(r.pa_des_dofus.contains(&("Descente aux Abysses".to_string(), 1)), "{:?}", r.pa_des_dofus);
    assert_eq!(r.pm_si_ecarte, vec![("Descente aux Abysses".to_string(), 1)]);
    assert_eq!((r.puissance_par_pm, r.puissance_si_poussee), (8, 100));
    assert!(
        r.bonus_conditionnels.iter().any(|b| b.nom == "Descente aux Abysses" && b.effet == "+1 PA"),
        "{:?}",
        r.bonus_conditionnels.iter().map(|b| &b.nom).collect::<Vec<_>>()
    );
}

/// Un bonus de DofusBook dans une autre statistique compte comme la
/// forgemagie la lit : le Noir Ébène à +10 % de dommages à distance (cinq
/// cumuls), avec le Dofus Ébène porté ; sans lui, rien, et la fiche le dit.
#[test]
fn un_bonus_en_distance_compte_avec_son_dofus() {
    let ebene = Boost { name: "Noir Ébène".into(), stat: "dd".into(), percent: 10, ..Default::default() };
    let mut porte = build_de_tim();
    porte.fm.clear();
    porte.boosts.clear();
    porte.items[11] = 7114;
    let mut avec = porte.clone();
    avec.boosts = vec![ebene.clone()];
    let distance = |b: &BuildInput| resolve(b, &catalogue()).profile.percent_ranged;
    assert_eq!(distance(&avec) - distance(&porte), 10);

    let mut sans_dofus = build_de_tim();
    sans_dofus.fm.clear();
    sans_dofus.boosts = vec![ebene];
    let r = resolve(&sans_dofus, &catalogue());
    assert!(
        r.assumptions.iter().any(|a| a.contains("Noir Ébène non compté sans le")),
        "{:#?}",
        r.assumptions
    );
}
