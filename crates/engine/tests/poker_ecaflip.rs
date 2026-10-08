//! Le Poker de l'Ecaflip : une Main de quatre Cartes, une seule combinaison.
//!
//! La Rekop porte vingt et une charges, une par Main Gagnante, et ces Mains SE
//! CHEVAUCHENT. Quatre Cartes d'une couleur chacune sont un Carre Couleurs ; si
//! ce sont en plus Valet, Dame, Roi et As, c'est une Suite Royale Couleurs, qui
//! frappe presque deux fois plus fort. Le jeu n'en joue qu'une, la meilleure.
//!
//! Sans le groupe de paliers, les deux frapperaient, et la Rekop vaudrait la
//! somme de deux combinaisons que le joueur n'a jouees qu'une fois.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn ecaflip() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/ecaflip.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    rs.merge_snapshot(
        &Snapshot::load(format!("{root}/data/snapshots/breed-6.json")).unwrap(),
    );
    rs
}

fn moteur(rs: &Ruleset, deck: &[&str], horizon: u8) -> Engine {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 900,
                flat_damage: 0,
            }; 5],
            ..Default::default()
        },
        base_ap: 20,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        horizon,
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

/// Les degats du meilleur lancer de Rekop d'une rotation.
fn meilleure_rekop(rs: &Ruleset, deck: &[&str], horizon: u8) -> i64 {
    moteur(rs, deck, horizon)
        .solve()
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == "rekop")
        .map(|c| c.damage.0)
        .max()
        .unwrap_or(0)
}

/// Sans Main, la Rekop ne frappe pas.
///
/// Toutes ses lignes exigent une combinaison ; aucune n'est inconditionnelle.
/// Les quatre fourchettes 8-20 que la donnee porte sans etat sont l'ETENDUE
/// affichee dans l'infobulle, de la plus faible combinaison a la plus forte, et
/// non une charge qui partirait en plus. Les compter aurait fait frapper le
/// sort par-dessus sa propre Main.
#[test]
fn la_rekop_seule_ne_frappe_pas() {
    let rs = ecaflip();
    assert_eq!(
        meilleure_rekop(&rs, &["rekop"], 2),
        0,
        "sans Carte en Main, aucune combinaison ne tient"
    );
}

/// Quatre As valent plus que quatre Rois, dans le rapport du jeu. Le témoin est
/// le Carré de Rois : aucun porteur de buff parmi ses quatre pioches, là où le
/// Yams et Pile ou Face, qui piochent deux Cartes du Carré de Valets, donnent du
/// taux de coup critique au lanceur.
#[test]
fn le_carre_d_as_bat_le_carre_de_rois() {
    let rs = ecaflip();
    // Les quatre sorts qui piochent un As, puis la Rekop.
    let as_ = meilleure_rekop(
        &rs,
        &["toupet", "infortune", "peril", "destin_d_ecaflip", "rekop"],
        2,
    );
    // Les quatre qui piochent un Roi, aucun ne portant de buff.
    let rois = meilleure_rekop(
        &rs,
        &[
            "coussinets",
            "felintion",
            "langue_rapeuse",
            "lapement",
            "rekop",
        ],
        2,
    );
    assert!(
        as_ > 0 && rois > 0,
        "les deux Mains doivent frapper : {as_} et {rois}"
    );
    // 20 contre 16 de fourchette de base, dans chacun des quatre elements.
    let rapport = as_ as f64 / rois as f64;
    assert!(
        (rapport - 20.0 / 16.0).abs() < 0.05,
        "le Carre d'As doit valoir 20/16 fois le Carre de Rois, il vaut {rapport:.3}"
    );
}

/// Une Main qui vaut deux combinaisons n'en paie qu'une.
///
/// Valet de Pique, Dame de Trefle, Roi de Coeur, As de Carreau : une Carte de
/// chaque valeur ET de chaque couleur. C'est a la fois une Suite Royale
/// Couleurs (14 dans les quatre elements), une Suite Royale (54 dans le
/// meilleur) et un Carre Couleurs (8 dans les quatre). Une seule doit partir,
/// la premiere declaree, et c'est la Suite Royale Couleurs.
///
/// Verifie en retirant le groupe : les trois s'additionnent et le total passe
/// de 56 a 118 de fourchette de base.
#[test]
fn une_main_ne_paie_qu_une_combinaison() {
    let rs = ecaflip();
    let melange = meilleure_rekop(
        &rs,
        &[
            "reflexes",
            "baraka",
            "langue_rapeuse",
            "destin_d_ecaflip",
            "rekop",
        ],
        2,
    );
    // Le Carré de Rois, quatre Cartes d'une seule valeur, ne vaut qu'une
    // combinaison possible : il sert de témoin non ambigu, aucune de ses
    // pioches ne donnant de critique.
    let rois = meilleure_rekop(
        &rs,
        &[
            "coussinets",
            "felintion",
            "langue_rapeuse",
            "lapement",
            "rekop",
        ],
        2,
    );
    assert!(melange > 0 && rois > 0, "{melange} et {rois}");
    // 14 contre 16 par element : la Suite Royale Couleurs seule. Si les trois
    // combinaisons s'ajoutaient, le rapport approcherait 118/64, pas 14/16.
    let rapport = melange as f64 / rois as f64;
    assert!(
        (rapport - 14.0 / 16.0).abs() < 0.05,
        "seule la Suite Royale Couleurs doit partir, soit 14/8 fois le Carre de Valets ; \
         le rapport vaut {rapport:.3}, ce qui trahit plusieurs combinaisons cumulees"
    );
}

/// Piocher deux fois la meme Carte ne la met pas deux fois dans la Main.
///
/// Le jeu ne pioche une Carte qu'a qui ne l'a pas deja : la donnee du sort
/// exige l'ABSENCE de l'etat. Un compteur par VALEUR aurait ignore cela et
/// aurait vu un Carre de Rois la ou il n'y a que deux Rois.
///
/// Deux mains de QUATRE lancers chacune, pour que la comparaison ne porte que
/// sur les doublons et non sur le nombre de sorts joues :
///   quatre Rois distincts   -> Carre de Rois
///   deux Rois en double     -> rien
#[test]
fn une_carte_deja_en_main_ne_compte_pas_deux_fois() {
    let rs = ecaflip();
    let distincts = meilleure_rekop(
        &rs,
        &[
            "griffe_joueuse",
            "langue_rapeuse",
            "lapement",
            "coussinets",
            "rekop",
        ],
        2,
    );
    // Griffe Joueuse et Felintion piochent le MEME Roi de Trefle ; Langue
    // Rapeuse et Feulement le meme Roi de Coeur.
    let doublons = meilleure_rekop(
        &rs,
        &[
            "griffe_joueuse",
            "felintion",
            "langue_rapeuse",
            "feulement",
            "rekop",
        ],
        2,
    );
    // Sans cette premiere assertion le test serait MUET : si la Rekop ne
    // frappait jamais, les deux seraient a zero et le test passerait sans rien
    // verifier.
    assert!(
        distincts > 0,
        "quatre Rois distincts font un Carre de Rois, or la Rekop ne frappe pas"
    );
    assert_eq!(
        doublons, 0,
        "quatre lancers pour deux Cartes distinctes ne font aucune combinaison, \
         or la Rekop frappe pour {doublons}"
    );
}

/// Le Carré Couleurs passe devant le Brelan de Valets : l'ordre de l'infobulle
/// fait foi, sur les douze Mains des 1820 possibles qu'il départage (trois
/// Valets plus une carte d'une autre valeur, les quatre couleurs étant
/// différentes).
///
/// Les deux Mains ci-dessous ne diffèrent que par la couleur de leur quatrième
/// carte. Avec le Carreau les quatre couleurs sont réunies et le Carré Couleurs
/// gagne, quatre lignes de 8, une par élément. Avec le Pique la couleur est en
/// double, et c'est le Brelan de Valets qui part, trois lignes de 8 dans le
/// meilleur élément. Le rapport vaut donc 4/3. Le Yams est dans les deux Mains,
/// et son buff de taux critique s'annule dans le rapport.
#[test]
fn le_carre_couleurs_passe_devant_le_brelan_de_valets() {
    let rs = ecaflip();
    // Valet Pique, Valet Trefle, Valet Coeur, Dame CARREAU : quatre couleurs.
    let quatre = meilleure_rekop(
        &rs,
        &["reflexes", "yams", "topkaj", "esprit_felin", "rekop"],
        3,
    );
    // Les memes trois Valets, mais un Roi de PIQUE : trois couleurs seulement.
    let trois = meilleure_rekop(
        &rs,
        &["reflexes", "yams", "topkaj", "coussinets", "rekop"],
        3,
    );
    assert!(
        quatre > 0 && trois > 0,
        "les deux Mains doivent frapper : {quatre} et {trois}"
    );
    let rapport = quatre as f64 / trois as f64;
    assert!(
        (rapport - 4.0 / 3.0).abs() < 0.05,
        "quatre lignes contre trois : le rapport doit valoir 4/3, il vaut {rapport:.3}"
    );
}
