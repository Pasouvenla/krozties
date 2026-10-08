//! Ce que le placement apporte, et ce qu'il refuse. `FORMES_ORIENTEES` est
//! vérifiée : une forme absente rend les mêmes cases dans les quatre directions,
//! une forme présente en change. Et le placement redonne le modèle linéaire du
//! moteur quand les ennemis sont alignés à intervalle régulier.

use dofus_grid::tir::{direction_de_tir, tir, visee_permise, Contraintes, Refus, FORMES_ORIENTEES};
use dofus_grid::{cases_de_zone, Case, DIRECTIONS};

/// Les formes du parc, aux tailles que les sorts portent : une forme orientée
/// testée à une taille dégénérée serait symétrique par accident.
const FORMES_DU_PARC: &[(char, u16, u16)] = &[
    ('P', 1, 0),
    ('C', 2, 0),
    ('C', 3, 0),
    ('C', 4, 0),
    ('X', 1, 0),
    ('X', 2, 0),
    ('X', 3, 0),
    ('+', 1, 0),
    ('*', 2, 0),
    ('O', 1, 0),
    ('O', 2, 0),
    ('O', 3, 0),
    ('G', 1, 0),
    ('G', 2, 0),
    ('W', 2, 0),
    ('Q', 1, 0),
    ('Q', 3, 0),
    ('T', 1, 0),
    ('T', 2, 0),
    ('L', 2, 0),
    ('L', 3, 0),
    ('L', 4, 0),
    ('-', 2, 0),
    ('U', 1, 0),
    ('U', 2, 0),
    ('V', 1, 0),
    ('V', 2, 0),
    ('R', 1, 3),
    ('F', 1, 0),
    ('F', 2, 0),
];

fn cases_dans(forme: char, taille: u16, taille2: u16, direction: (i16, i16)) -> Vec<Case> {
    cases_de_zone(forme, taille, taille2, Case::new(0, 0), direction)
        .unwrap_or_else(|| panic!("{forme} taille {taille} devrait se dessiner"))
}

#[test]
fn la_liste_des_formes_orientees_se_verifie_forme_par_forme() {
    let mut symetriques = 0;
    let mut orientees = 0;
    for &(forme, taille, taille2) in FORMES_DU_PARC {
        let reference = cases_dans(forme, taille, taille2, DIRECTIONS[0]);
        let change = DIRECTIONS[1..]
            .iter()
            .any(|&d| cases_dans(forme, taille, taille2, d) != reference);
        if FORMES_ORIENTEES.contains(&forme) {
            assert!(
                change,
                "{forme} taille {taille} est annoncée orientée mais rend les mêmes cases \
                 dans les quatre directions : soit la liste est gonflée, soit le dessin \
                 ignore la direction qu'il devrait lire"
            );
            orientees += 1;
        } else {
            assert!(
                !change,
                "{forme} taille {taille} change de cases selon la direction du tir et \
                 n'est pas dans FORMES_ORIENTEES : un tir en biais l'orienterait au \
                 hasard, en silence"
            );
            symetriques += 1;
        }
    }
    assert!(
        symetriques >= 15 && orientees >= 6,
        "contrôle trop maigre : {symetriques} formes symétriques et {orientees} orientées"
    );
}

/// La règle de dégressivité du moteur (`taux_par_cible`), recopiée à la main :
/// les deux chemins doivent rester écrits séparément.
fn taux(eloignement: u32, percent: u32, steps: u32) -> u32 {
    100u32.saturating_sub(eloignement.min(steps) * percent)
}

#[test]
fn sur_une_ligne_reguliere_la_grille_redonne_le_modele_du_moteur() {
    // La dégressivité que la donnée porte sur la plupart des zones : 10 % par
    // cran, quatre crans au plus.
    let (percent, steps) = (10, 4);
    let mut compares = 0;
    for etalement in 1u16..=4 {
        for combien in 1usize..=5 {
            let impact = Case::new(0, 0);
            let lanceur = Case::new(-6, 0);
            // Les ennemis alignés derrière l'impact, à intervalle régulier :
            // exactement ce que le moteur suppose quand on lui déclare un
            // étalement.
            let cibles: Vec<Case> = (0..combien)
                .map(|i| Case::new(0, (i as u16 * etalement) as i16))
                .collect();
            let t = tir(
                'C',
                60,
                0,
                lanceur,
                impact,
                &cibles,
                Contraintes::libre(0, 60),
            )
            .expect("un cercle assez large touche tout le monde");
            assert_eq!(
                t.touches.len(),
                combien,
                "toutes les cibles sont dans la zone"
            );
            for (i, touche) in t.touches.iter().enumerate() {
                let du_moteur = taux(u32::from(etalement) * i as u32, percent, steps);
                let de_la_grille = taux(u32::from(touche.eloignement), percent, steps);
                assert_eq!(
                    de_la_grille, du_moteur,
                    "étalement {etalement}, cible {i} : la grille dit {de_la_grille} % \
                     et le modèle linéaire {du_moteur} %"
                );
                compares += 1;
            }
        }
    }
    assert!(compares >= 40, "seulement {compares} comparaisons");
}

#[test]
fn sur_un_placement_reel_la_grille_dit_autre_chose_que_l_etalement_declare() {
    // Trois ennemis groupés autour de l'impact, comme ils le sont vraiment en
    // combat : deux collés, un décalé. Le moteur, à qui l'on déclarerait un
    // étalement de 1, les placerait à 0, 1 et 2 cases.
    let impact = Case::new(0, 0);
    let cibles = [Case::new(0, 0), Case::new(1, 0), Case::new(0, 1)];
    let t = tir(
        'C',
        2,
        0,
        Case::new(-4, 0),
        impact,
        &cibles,
        Contraintes::libre(1, 6),
    )
    .expect("le cercle de rayon 2 couvre les trois");
    let mesures: Vec<u16> = t.touches.iter().map(|x| x.eloignement).collect();
    assert_eq!(
        mesures,
        vec![0, 1, 1],
        "les deux voisines sont à UNE case, pas une et deux"
    );

    let (percent, steps) = (10, 4);
    let de_la_grille: u32 = mesures
        .iter()
        .map(|&e| taux(u32::from(e), percent, steps))
        .sum();
    let du_modele: u32 = (0..3).map(|i| taux(i, percent, steps)).sum();
    assert_eq!(de_la_grille, 100 + 90 + 90);
    assert_eq!(du_modele, 100 + 90 + 80);
    assert!(
        de_la_grille > du_modele,
        "le placement réel vaut mieux que l'étalement déclaré, et c'est bien pour ça \
         qu'on le mesure au lieu de le supposer"
    );
}

#[test]
fn les_touches_sortent_de_la_plus_proche_a_la_plus_lointaine() {
    // Un sort plafonné frappe les plus proches. L'ordre de la liste EST cette
    // règle : la donner à l'envers ferait choisir les mauvaises cibles sans
    // qu'aucun total ne paraisse absurde.
    let impact = Case::new(0, 0);
    let cibles = [
        Case::new(3, 0),
        Case::new(0, 0),
        Case::new(1, 1),
        Case::new(-2, 0),
    ];
    let t = tir(
        'C',
        4,
        0,
        Case::new(-8, 0),
        impact,
        &cibles,
        Contraintes::libre(1, 9),
    )
    .unwrap();
    let ordre: Vec<(usize, u16)> = t
        .touches
        .iter()
        .map(|x| (x.indice, x.eloignement))
        .collect();
    assert_eq!(ordre, vec![(1, 0), (2, 2), (3, 2), (0, 3)]);
    // À égalité, le rang de déclaration tranche : deux tirs identiques doivent
    // rendre la même liste.
    assert!(ordre.windows(2).all(|w| w[0].1 <= w[1].1));
}

#[test]
fn la_ligne_depuis_le_lanceur_fait_exactement_la_distance_de_tir() {
    // La seule forme dont la longueur vient du tir. Le compteur ne pouvait que
    // la relire dans la portée du niveau ; ici elle se dessine.
    let mut vus = 0;
    for distance in 1i16..=8 {
        for (dx, dy) in DIRECTIONS {
            let lanceur = Case::new(0, 0);
            let visee = Case::new(dx * distance, dy * distance);
            let t = tir('l', 1, 63, lanceur, visee, &[], Contraintes::libre(1, 12)).unwrap();
            assert_eq!(
                t.cases.len(),
                distance as usize,
                "distance {distance} : {} cases",
                t.cases.len()
            );
            assert!(
                !t.cases.contains(&lanceur),
                "le lanceur ne se frappe pas lui-même"
            );
            assert!(t.cases.contains(&visee), "la case visée est dans la ligne");
            vus += 1;
        }
    }
    assert_eq!(vus, 32);
}

#[test]
fn le_rectangle_de_l_epieu_sismique_fait_ses_douze_cases_devant_le_lanceur() {
    // Épieu Sismique du Forgelance : `param1` 1 et `param2` 3, soit trois de
    // large par quatre de profond. Le compteur en donne douze ; le dessin doit
    // en poser douze, et du bon côté.
    let lanceur = Case::new(0, 0);
    let visee = Case::new(2, 0);
    let t = tir('R', 1, 3, lanceur, visee, &[], Contraintes::libre(1, 6)).unwrap();
    assert_eq!(t.cases.len(), 12);
    let devant = t.cases.iter().filter(|c| c.x >= visee.x).count();
    assert_eq!(
        devant, 12,
        "le rectangle s'enfonce dans le sens du tir, il ne recule pas"
    );
    let profondeur = t.cases.iter().map(|c| c.x).max().unwrap() - visee.x;
    assert_eq!(
        profondeur, 3,
        "quatre rangs, de l'impact à trois cases devant"
    );
}

#[test]
fn chaque_refus_se_provoque_par_le_defaut_qu_il_nomme() {
    let l = Case::new(0, 0);
    let en_ligne = Contraintes {
        portee_min: 2,
        portee_max: 5,
        en_ligne: true,
        en_diagonale: false,
    };

    assert_eq!(
        visee_permise(l, Case::new(1, 0), en_ligne),
        Err(Refus::TropPres {
            distance: 1,
            minimum: 2
        })
    );
    assert_eq!(
        visee_permise(l, Case::new(6, 0), en_ligne),
        Err(Refus::TropLoin {
            distance: 6,
            maximum: 5
        })
    );
    // Deux cases plus une : la portée tient, l'axe non.
    assert_eq!(
        visee_permise(l, Case::new(2, 1), en_ligne),
        Err(Refus::HorsAxe)
    );
    assert_eq!(visee_permise(l, Case::new(3, 0), en_ligne), Ok(()));

    // Une diagonale satisfait un sort en diagonale et pas un sort en ligne.
    let en_diag = Contraintes {
        portee_min: 1,
        portee_max: 8,
        en_ligne: false,
        en_diagonale: true,
    };
    assert_eq!(visee_permise(l, Case::new(2, 2), en_diag), Ok(()));
    assert_eq!(
        visee_permise(l, Case::new(2, 2), en_ligne),
        Err(Refus::HorsAxe)
    );
    // Les deux à faux laissent viser librement.
    assert_eq!(
        visee_permise(l, Case::new(2, 1), Contraintes::libre(1, 8)),
        Ok(())
    );

    // Une forme orientée visée en biais ne s'oriente pas au hasard.
    let biais = Case::new(2, 1);
    assert_eq!(direction_de_tir(l, biais), None);
    assert_eq!(
        tir('V', 2, 0, l, biais, &[], Contraintes::libre(1, 8)),
        Err(Refus::DirectionIndeterminee)
    );
    // Une forme symétrique, elle, part sans direction.
    assert!(tir('C', 2, 0, l, biais, &[], Contraintes::libre(1, 8)).is_ok());

    // Et une forme dont le dessin n'est pas établi se refuse, au lieu de rendre
    // une zone plausible.
    assert_eq!(
        tir('?', 1, 0, l, Case::new(2, 0), &[], Contraintes::libre(1, 8)),
        Err(Refus::FormeNonDessinee('?'))
    );
}

/// Le demi-cercle et la fourche de taille 2, case par case, tir vers la droite :
/// deux bras en diagonale qui reviennent vers le lanceur, et trois dents de trois
/// cases qui partent de la case visée.
#[test]
fn les_captures_du_demi_cercle_et_de_la_fourche() {
    let trie = |mut v: Vec<Case>| {
        v.sort_unstable();
        v
    };
    let c = |pts: &[(i16, i16)]| trie(pts.iter().map(|&(x, y)| Case::new(x, y)).collect());
    assert_eq!(
        trie(cases_dans('U', 2, 0, (1, 0))),
        c(&[(0, 0), (-1, 1), (-1, -1), (-2, 2), (-2, -2)])
    );
    assert_eq!(
        trie(cases_dans('F', 2, 0, (1, 0))),
        c(&[
            (0, 0),
            (1, 0),
            (2, 0),
            (3, 0),
            (1, 1),
            (2, 2),
            (3, 3),
            (1, -1),
            (2, -2),
            (3, -3),
        ])
    );
}
