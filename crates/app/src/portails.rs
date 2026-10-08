//! Les portails de l'Eliotrope : par où ressort un sort projeté.
//!
//! # La règle
//!
//! Le sort va de proche en proche : du portail d'entrée au plus proche, puis de
//! celui-là au plus proche de ceux qui restent, et ressort par le dernier. Il ne
//! passe qu'une fois par chaque portail.
//!
//! Quand plusieurs portails sont à la même distance, ils se départagent sur un
//! cadran centré sur le portail où se trouve le sort, gradué comme une horloge
//! vue à l'écran : midi en haut, 3h à droite, 1h30 au coin
//! haut-droite, 7h30 au coin bas-gauche. La diagonale 7h30-1h30 le coupe en
//! une zone A au-dessus et une zone B en dessous.
//!
//! * Si l'un d'eux ouvre un arc, parcouru dans le sens horaire, qui atteint
//!   tous les autres en 180° au plus, c'est lui qui passe d'abord (cas 2 et 5
//!   des tests).
//! * Sinon, deux portails exactement opposés ou trois trop écartés pour tenir
//!   dans 180°, c'est la zone A qui passe d'abord ; et si plusieurs y sont, le
//!   premier rencontré en tournant dans le sens ANTIhoraire depuis 1h30, un
//!   portail posé sur 1h30 comptant en zone A (cas 3, 4, 6 et 7).
//!
//! Quatre portails au sol n'en mettent jamais plus de trois à égalité ;
//! au-delà, la même règle s'appliquerait.
//!
//! # Le cadran dans ce repère
//!
//! Ce damier dessine une case en `((x - y) L/2, (x + y) H/2)`, l'axe vertical
//! vers le bas. Les calculs se font sur `(x - y, -(x + y))` : l'écran sans son
//! aplatissement, vertical vers le haut. Une transformation linéaire conserve
//! les alignements, le sens de rotation et les demi-plans, donc tout ce que la
//! règle regarde ; et les heures y tombent juste, 1h30 étant exactement la
//! direction `(1, 1)`. Tout est entier : aucun angle n'est calculé en flottant,
//! et « exactement opposés » ou « posé sur 1h30 » ne dépendent d'aucun arrondi.

use crate::cartes::Plateau;
use dofus_engine::Case;

/// Le trajet d'un sort projeté dans le portail `entree` : les portails qu'il
/// traverse, dans l'ordre, l'entrée d'abord et la sortie en dernier.
///
/// `portails` sont ceux qui sont au sol ; l'entrée peut y figurer ou non.
pub fn trajet(entree: Case, portails: &[Case]) -> Vec<Case> {
    let mut restants: Vec<Case> = portails.iter().copied().filter(|p| *p != entree).collect();
    restants.sort_unstable();
    restants.dedup();
    let mut chemin = vec![entree];
    let mut courant = entree;
    while let Some(plus_pres) = restants.iter().map(|p| courant.distance(*p)).min() {
        let egaux: Vec<Case> = restants
            .iter()
            .copied()
            .filter(|p| courant.distance(*p) == plus_pres)
            .collect();
        let suivant = if egaux.len() == 1 {
            egaux[0]
        } else {
            departager(courant, &egaux)
        };
        restants.retain(|p| *p != suivant);
        chemin.push(suivant);
        courant = suivant;
    }
    chemin
}

/// La position d'un portail vu du portail courant, à l'écran redressé,
/// vertical vers le haut.
fn a_l_ecran(centre: Case, p: Case) -> (i32, i32) {
    let (dx, dy) = (i32::from(p.x - centre.x), i32::from(p.y - centre.y));
    (dx - dy, -(dx + dy))
}

fn vectoriel(u: (i32, i32), v: (i32, i32)) -> i32 {
    u.0 * v.1 - u.1 * v.0
}

fn scalaire(u: (i32, i32), v: (i32, i32)) -> i32 {
    u.0 * v.0 + u.1 * v.1
}

/// `v` est-il atteint depuis `u` en tournant dans le sens horaire de 180° au
/// plus ? À l'écran redressé, le sens horaire est celui où le produit
/// vectoriel est négatif ; nul, les deux sont alignés, et opposés si leur
/// produit scalaire est négatif.
fn horaire_en_180_au_plus(u: (i32, i32), v: (i32, i32)) -> bool {
    let c = vectoriel(u, v);
    c < 0 || (c == 0 && scalaire(u, v) < 0)
}

/// 1h30, le coin haut-droite du cadran.
const UNE_HEURE_TRENTE: (i32, i32) = (1, 1);

/// Au-dessus de la diagonale 7h30-1h30, 1h30 compris.
fn en_zone_a(v: (i32, i32)) -> bool {
    let c = vectoriel(UNE_HEURE_TRENTE, v);
    c > 0 || (c == 0 && scalaire(UNE_HEURE_TRENTE, v) > 0)
}

/// Lequel de plusieurs portails à égalité de distance passe d'abord.
fn departager(courant: Case, egaux: &[Case]) -> Case {
    let vus: Vec<(Case, (i32, i32))> = egaux.iter().map(|p| (*p, a_l_ecran(courant, *p))).collect();

    // Celui dont l'arc horaire atteint tous les autres en 180° au plus. Deux
    // portails exactement opposés le sont l'un pour l'autre : il y en a alors
    // deux, et c'est la zone qui tranche.
    let debuts: Vec<Case> = vus
        .iter()
        .filter(|(p, u)| vus.iter().all(|(q, v)| q == p || horaire_en_180_au_plus(*u, *v)))
        .map(|(p, _)| *p)
        .collect();
    if let [seul] = debuts.as_slice() {
        return *seul;
    }

    // La zone A d'abord, et le premier rencontré en tournant dans le sens
    // antihoraire depuis 1h30. Deux portails de la zone A sont tous deux à
    // moins de 180° de 1h30 dans ce sens : le produit vectoriel les ordonne.
    let zone_a: Vec<(Case, (i32, i32))> = vus.iter().copied().filter(|(_, v)| en_zone_a(*v)).collect();
    let candidats = if zone_a.is_empty() { vus } else { zone_a };
    candidats
        .iter()
        .copied()
        .reduce(|premier, autre| {
            if vectoriel(premier.1, autre.1) < 0 {
                autre
            } else {
                premier
            }
        })
        .map(|(p, _)| p)
        .expect("au moins deux portails à départager")
}

/// Les cases que le sort parcourt dans le réseau : la somme des sauts d'un
/// portail au suivant, de l'entrée à la sortie.
pub fn cases_parcourues(chemin: &[Case]) -> u32 {
    chemin
        .windows(2)
        .map(|saut| u32::from(saut[0].distance(saut[1])))
        .sum()
}

/// Le plafond du bonus de projection, en pour cent : dix cases en donnent
/// déjà autant. Le réglage `cases_entre_portails` de la rotation s'arrête au
/// même endroit, et un test les garde d'accord.
pub const BONUS_MAX: u32 = 20;

/// Le bonus de dommages finaux d'un sort projeté, en pour cent : 2 par case
/// parcourue dans le réseau, SANS BASE, et 20 AU PLUS. Cinq cases font 10 %,
/// dix en font 20, et quatorze aussi. La donnée du jeu porte les 2 % par case
/// sur une base nulle (effet 1181), pas le plafond.
pub fn bonus_de_projection(chemin: &[Case]) -> u32 {
    (2 * cases_parcourues(chemin)).min(BONUS_MAX)
}

/// Ce que l'onglet KrozPortal envoie.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct RequetePortails {
    /// Les portails au sol.
    pub portails: Vec<(i16, i16)>,
    /// Celui dans lequel le sort est projeté.
    pub entree: (i16, i16),
    /// Pour les dégâts : la même requête que KrozZone (build, sort dans
    /// `deck`, ennemis). Son lanceur devient le portail de sortie et sa case
    /// visée la case d'ARRIVÉE, calculée : c'est de la sortie que le sort
    /// repart. Absente, seul le trajet est rendu.
    #[serde(default)]
    pub tir: Option<crate::solve::Request>,
    /// La case de l'Éliotrope. Elle fixe la case d'arrivée, et la réponse dit
    /// si le portail d'entrée est à portée et en vue du sort, comme une cible
    /// ordinaire.
    #[serde(default)]
    pub eliotrope: Option<(i16, i16)>,
    /// La carte de boss, par son identifiant dans le relevé ; absente, le damier
    /// vide. Ses murs coupent la vue, et la case d'arrivée doit être de son
    /// sol.
    #[serde(default)]
    pub carte: Option<u16>,
    /// Le côté du damier vide, sans carte : quinze par défaut.
    #[serde(default)]
    pub damier: Option<u8>,
}

/// La case où le sort atterrit : la sortie, décalée comme l'entrée l'est de
/// l'Éliotrope.
///
/// ⚠️ La case d'arrivée ne se choisit pas : l'Éliotrope vise le portail
/// d'entrée, et le sort ressort de la sortie en continuant du même vecteur.
/// [`portails_json`] exige en plus une case de sol en vue de la sortie.
pub fn case_d_arrivee(eliotrope: Case, entree: Case, sortie: Case) -> Case {
    Case::new(
        sortie.x + (entree.x - eliotrope.x),
        sortie.y + (entree.y - eliotrope.y),
    )
}

/// Le trajet d'un sort projeté, sa sortie, son bonus, et ce qu'il inflige
/// depuis la sortie quand la requête porte un tir.
pub fn portails_json(requete: &RequetePortails) -> Result<String, String> {
    let plateau = Plateau::de(requete.carte, requete.damier)?;
    let portails: Vec<Case> = requete
        .portails
        .iter()
        .map(|&(x, y)| Case::new(x, y))
        .collect();
    if let Some(p) = portails.iter().find(|p| !plateau.est_sol(**p)) {
        return Err(format!("le portail en ({}, {}) n'est pas sur le sol du plateau", p.x, p.y));
    }
    let entree = Case::new(requete.entree.0, requete.entree.1);
    if !portails.contains(&entree) {
        return Err("le portail d'entrée n'est pas au sol".into());
    }
    let chemin = trajet(entree, &portails);
    let sortie = *chemin.last().expect("le trajet part au moins de l'entrée");
    let parcourues = cases_parcourues(&chemin);
    let bonus = bonus_de_projection(&chemin);
    let lire = |texte: String| -> Result<serde_json::Value, String> {
        serde_json::from_str(&texte).map_err(|e| e.to_string())
    };
    let arrivee = requete
        .eliotrope
        .filter(|_| chemin.len() >= 2)
        .map(|(x, y)| case_d_arrivee(Case::new(x, y), entree, sortie));
    // La portée du sort vue de l'Éliotrope, visée sur le portail d'entrée.
    let depuis_eliotrope = match (&requete.tir, requete.eliotrope) {
        (Some(r), Some(eliotrope)) => {
            let mut r = r.clone();
            if let Some(p) = r.placement.as_mut() {
                p.lanceur = eliotrope;
                p.visee = requete.entree;
                p.carte = requete.carte;
                p.damier = requete.damier;
            }
            let zones = lire(crate::grille::zones_json(&r)?)?;
            Some(zones["sorts"][0]["portee"].clone())
        }
        _ => None,
    };
    // ⚠️ Le sort ne ressort que vers du sol en vue de la sortie. La vue n'est
    // exigée que d'un sort qui l'exige, comme pour viser l'entrée, et toujours
    // sans sort choisi. Les corps la coupent, l'Éliotrope compris quand il se
    // tient entre la sortie et l'arrivée.
    let vue_requise = depuis_eliotrope
        .as_ref()
        .is_none_or(|p| p["ligne_de_vue"] != serde_json::Value::Bool(false));
    let mut corps: Vec<Case> = requete
        .tir
        .as_ref()
        .and_then(|r| r.placement.as_ref())
        .map(|p| p.ennemis.iter().map(|&(x, y)| Case::new(x, y)).collect())
        .unwrap_or_default();
    corps.extend(requete.eliotrope.map(|(x, y)| Case::new(x, y)));
    let refus_arrivee = arrivee.and_then(|a| {
        if !plateau.est_sol(a) {
            Some("hors_sol")
        } else if vue_requise && !plateau.en_vue(sortie, a, &corps) {
            Some("hors_vue")
        } else {
            None
        }
    });
    let tir = match (&requete.tir, arrivee, refus_arrivee) {
        (Some(r), Some(arrivee), None) => {
            let mut r = r.clone();
            if let Some(p) = r.placement.as_mut() {
                p.lanceur = (sortie.x, sortie.y);
                p.visee = (arrivee.x, arrivee.y);
                p.carte = requete.carte;
                p.damier = requete.damier;
            }
            Some(lire(crate::grille::zones_avec_bonus(&r, bonus)?)?)
        }
        _ => None,
    };
    Ok(serde_json::json!({
        "trajet": chemin.iter().map(|c| [c.x, c.y]).collect::<Vec<_>>(),
        "sortie": [sortie.x, sortie.y],
        "cases": parcourues,
        "bonus": bonus,
        "arrivee": arrivee.map(|c| [c.x, c.y]),
        // Pourquoi le sort ne ressort pas : `hors_sol` ou `hors_vue`.
        "arrivee_refus": refus_arrivee,
        "tir": tir,
        "depuis_eliotrope": depuis_eliotrope,
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(x: i16, y: i16) -> Case {
        Case::new(x, y)
    }

    /// Les heures du cadran de rayon 4 autour de (0,0), dans ce repère. Elles
    /// servent à construire les cas de la règle sans compter les cases à la
    /// main, et le premier test vérifie qu'elles tombent où il faut.
    const MIDI: (i16, i16) = (-2, -2);
    const H1_30: (i16, i16) = (0, -4);
    const H3: (i16, i16) = (2, -2);
    const H4_30: (i16, i16) = (4, 0);
    const H6: (i16, i16) = (2, 2);
    const H7_30: (i16, i16) = (0, 4);
    const H9: (i16, i16) = (-2, 2);
    const H10_30: (i16, i16) = (-4, 0);

    /// ⚠️ LE CADRAN DANS CE REPÈRE. Ce damier met `(1,0)` en bas à droite de
    /// l'écran et `(0,1)` en bas à gauche : midi, en haut, est donc `(-2,-2)`
    /// à quatre cases, et 1h30, le coin haut-droite, `(0,-4)`. Le test le dit
    /// en angles pour qu'une erreur de repère ne passe pas inaperçue.
    #[test]
    fn les_heures_tombent_au_bon_endroit_de_l_ecran() {
        let heure = |(x, y): (i16, i16)| {
            let (sx, sy) = a_l_ecran(c(0, 0), c(x, y));
            let a = f64::from(sx).atan2(f64::from(sy)).to_degrees();
            (if a < 0.0 { a + 360.0 } else { a }).round() as i32
        };
        assert_eq!(
            [MIDI, H1_30, H3, H4_30, H6, H7_30, H9, H10_30].map(heure),
            [0, 45, 90, 135, 180, 225, 270, 315]
        );
    }

    /// Cas 1 : sans égalité, de proche en proche, une fois par portail.
    #[test]
    fn cas_1_de_proche_en_proche() {
        // Entrée en (0,0) ; (1,1) à 2, (4,1) à 5, (-3,0) à 3 de l'entrée.
        let t = trajet(c(0, 0), &[c(4, 1), c(1, 1), c(-3, 0)]);
        // De (1,1) : (-3,0) à 5, (4,1) à 3.
        assert_eq!(t, vec![c(0, 0), c(1, 1), c(4, 1), c(-3, 0)]);
    }

    /// Cas 2 : deux portails à égalité, à 9h et vers 5h, soit 120° en sens
    /// horaire de l'un à l'autre. L'arc part de celui de 5h.
    #[test]
    fn cas_2_l_arc_horaire_de_moins_de_180_degres() {
        let vers_5h = c(3, 1);
        assert_eq!(trajet(c(0, 0), &[c(H9.0, H9.1), vers_5h])[1], vers_5h);
        // L'ordre de la liste n'y change rien.
        assert_eq!(trajet(c(0, 0), &[vers_5h, c(H9.0, H9.1)])[1], vers_5h);
    }

    /// Cas 3 : deux portails exactement opposés, vers 10h et vers 4h. Celui de
    /// la zone A, au-dessus de la diagonale, passe d'abord.
    #[test]
    fn cas_3_a_l_oppose_la_zone_a_passe_d_abord() {
        let vers_10h = c(-3, 1);
        let vers_4h = c(3, -1);
        assert_eq!(trajet(c(0, 0), &[vers_4h, vers_10h])[1], vers_10h);
    }

    /// Cas 4 : opposés sur la diagonale même, à 7h30 et 1h30 : celui de 1h30.
    #[test]
    fn cas_4_sur_la_diagonale_celui_de_1h30() {
        let t = trajet(c(0, 0), &[c(H7_30.0, H7_30.1), c(H1_30.0, H1_30.1)]);
        assert_eq!(t[1], c(H1_30.0, H1_30.1));
    }

    /// Cas 5 : trois à égalité tenant dans moins de 180°, à 4h30, vers 7h et à
    /// 9h. L'arc horaire part de 4h30, puis de proche en proche.
    #[test]
    fn cas_5_trois_dans_moins_de_180_degres() {
        let vers_7h = c(1, 3);
        let t = trajet(c(0, 0), &[c(H9.0, H9.1), vers_7h, c(H4_30.0, H4_30.1)]);
        assert_eq!(t, vec![c(0, 0), c(H4_30.0, H4_30.1), vers_7h, c(H9.0, H9.1)]);
    }

    /// Trois à égalité tenant dans 180° tout juste, à 3h, 6h et 9h : l'arc
    /// horaire de 3h à 9h les contient, il part de 3h : 180° compris.
    #[test]
    fn trois_dans_180_degres_tout_juste_suivent_l_arc_horaire() {
        let t = trajet(c(0, 0), &[c(H9.0, H9.1), c(H6.0, H6.1), c(H3.0, H3.1)]);
        assert_eq!(t, vec![c(0, 0), c(H3.0, H3.1), c(H6.0, H6.1), c(H9.0, H9.1)]);
    }

    /// Cas 6 : trois à égalité trop écartés pour 180°, vers 11h, à 3h et à
    /// 7h30. Seul celui de 11h est en zone A : il passe d'abord.
    #[test]
    fn cas_6_un_seul_en_zone_a() {
        let vers_11h = c(-3, -1);
        let t = trajet(c(0, 0), &[c(H3.0, H3.1), c(H7_30.0, H7_30.1), vers_11h]);
        assert_eq!(t, vec![c(0, 0), vers_11h, c(H3.0, H3.1), c(H7_30.0, H7_30.1)]);
    }

    /// Cas 7 : trois trop écartés, dont deux en zone A, vers 12h30 et à 9h.
    /// Le premier rencontré en tournant dans le sens antihoraire depuis 1h30
    /// passe d'abord : celui de 12h30.
    #[test]
    fn cas_7_deux_en_zone_a_le_premier_depuis_1h30_en_antihoraire() {
        let vers_12h30 = c(-1, -3);
        let vers_5h = c(3, 1);
        let t = trajet(c(0, 0), &[c(H9.0, H9.1), vers_5h, vers_12h30]);
        assert_eq!(t, vec![c(0, 0), vers_12h30, c(H9.0, H9.1), vers_5h]);
    }

    /// Posé exactement sur 1h30, un portail compte en zone A, et c'est lui qui
    /// passe d'abord.
    #[test]
    fn un_portail_sur_1h30_compte_en_zone_a() {
        let t = trajet(c(0, 0), &[c(H9.0, H9.1), c(H1_30.0, H1_30.1), c(H6.0, H6.1)]);
        assert_eq!(t[1], c(H1_30.0, H1_30.1));
    }

    /// Les sept exercices corrigés d'un tutoriel du forum officiel : chaque
    /// trajet est celui de la correction, la sortie étant le portail qu'elle
    /// cercle.
    ///
    /// Les positions sont lues à l'écran, une case valant 86 pixels sur 43,
    /// l'entrée (l'étoile) en (0,0). Avec les cas vérifiés en jeu, ce sont les
    /// seuls où la règle est confrontée à un résultat qu'elle n'a pas produit ;
    /// les tests par cas ne vérifient que la lecture de la règle.
    #[test]
    fn les_sept_exercices_du_tutoriel() {
        let exercices: [(&str, Vec<(i16, i16)>, Vec<(i16, i16)>); 7] = [
            // Trois à 4 cases vers midi, 2h et 5h : l'arc horaire part de midi.
            ("1", vec![(3, 1), (1, -3), (-2, -2)], vec![(-2, -2), (1, -3), (3, 1)]),
            // Opposés sur la diagonale, 1h30 et 7h30 : celui de 1h30.
            ("2", vec![(0, 3), (0, -3)], vec![(0, -3), (0, 3)]),
            // Trois à 2 cases, midi, 9h, 4h30, trop écartés : zone A, midi.
            ("3", vec![(2, 0), (-1, 1), (-1, -1)], vec![(-1, -1), (-1, 1), (2, 0)]),
            // Opposés vers 1h et 7h : la zone A.
            ("4", vec![(1, 4), (-1, -4)], vec![(-1, -4), (1, 4)]),
            // Sans égalité : de proche en proche.
            ("5", vec![(5, 0), (-3, -2), (-2, 2)], vec![(-2, 2), (-3, -2), (5, 0)]),
            // Deux à 7 cases vers 9h et 1h : l'arc horaire part de 9h.
            ("6", vec![(-2, -5), (-4, 3)], vec![(-4, 3), (-2, -5)]),
            // Trois à 5 cases trop écartés, un seul en zone A.
            ("7", vec![(1, 4), (3, -2), (-4, -1)], vec![(-4, -1), (3, -2), (1, 4)]),
        ];
        for (numero, portails, attendu) in exercices {
            let portails: Vec<Case> = portails.iter().map(|&(x, y)| c(x, y)).collect();
            let mut attendu: Vec<Case> = attendu.iter().map(|&(x, y)| c(x, y)).collect();
            attendu.insert(0, c(0, 0));
            assert_eq!(trajet(c(0, 0), &portails), attendu, "exercice {numero}");
        }
    }

    /// Six cas vérifiés en jeu, relevés sur les captures d'un second tutoriel
    /// du forum, une case valant 112 pixels sur 56 : chaque montage donne des
    /// portails exactement équidistants de l'entrée, ce qu'une lecture fausse
    /// d'une case ne donnerait pas.
    ///
    /// Sa règle est la même, dite autrement : le portail « à gauche de
    /// l'angle » qui contient les autres est le début de l'arc horaire, et le
    /// balayage antihoraire qui départage les portails opposés part de « 2h10 »,
    /// la ligne droite de cases vers le haut à droite de l'écran. C'est notre
    /// 1h30 : sur l'écran aplati, la direction `(0, -1)` tombe à 63°.
    #[test]
    fn les_six_cas_verifies_en_jeu_du_second_tutoriel() {
        let cas: [(&str, Vec<(i16, i16)>, Vec<(i16, i16)>); 6] = [
            // Trois à six cases, trop écartés : le premier depuis 2h10.
            ("1", vec![(-3, 3), (-1, -5), (4, 2)], vec![(-1, -5), (-3, 3), (4, 2)]),
            ("2", vec![(-4, 2), (2, 4), (2, -4)], vec![(-4, 2), (2, 4), (2, -4)]),
            // Deux opposés et un troisième dans l'angle plat : l'arc horaire.
            ("3", vec![(-1, 4), (1, -4), (-3, -2)], vec![(-1, 4), (-3, -2), (1, -4)]),
            // Un portail pile sur 2h10, puis une égalité au saut suivant.
            ("4", vec![(0, -5), (-4, 1), (1, 4)], vec![(0, -5), (1, 4), (-4, 1)]),
            ("5", vec![(0, -5), (-4, -1), (2, 3)], vec![(0, -5), (-4, -1), (2, 3)]),
            // Trois dans moins de 180°, puis une égalité à 11° d'écart.
            ("6", vec![(-4, -1), (-3, -2), (2, 3)], vec![(2, 3), (-4, -1), (-3, -2)]),
        ];
        for (numero, portails, attendu) in cas {
            let portails: Vec<Case> = portails.iter().map(|&(x, y)| c(x, y)).collect();
            let mut attendu: Vec<Case> = attendu.iter().map(|&(x, y)| c(x, y)).collect();
            attendu.insert(0, c(0, 0));
            assert_eq!(trajet(c(0, 0), &portails), attendu, "cas {numero}");
        }
    }

    /// La distance se compte depuis le portail COURANT, pas depuis l'entrée.
    ///
    /// De l'entrée (0,0), (1,0) est le plus proche. De là, (4,0) est à 3 et
    /// (0,-4) à 5 : le sort va à (4,0). Mesurées depuis l'entrée, les deux
    /// seraient à égalité, à 4.
    #[test]
    fn la_distance_se_compte_depuis_le_portail_courant() {
        let t = trajet(c(0, 0), &[c(0, -4), c(4, 0), c(1, 0)]);
        assert_eq!(t, vec![c(0, 0), c(1, 0), c(4, 0), c(0, -4)]);
    }

    /// Le cadran se recentre sur le portail COURANT.
    ///
    /// De l'entrée (0,1), le sort va en (0,0). De là, (4,0) à 4h30 et (-3,1)
    /// vers 10h sont à égalité : l'arc horaire de l'un à l'autre fait 162°, il
    /// part de (4,0). Vu depuis l'entrée, l'ordre s'inverse, et la distance
    /// aussi désignerait (-3,1).
    #[test]
    fn le_cadran_se_recentre_sur_le_portail_courant() {
        let t = trajet(c(0, 1), &[c(-3, 1), c(4, 0), c(0, 0)]);
        assert_eq!(t, vec![c(0, 1), c(0, 0), c(4, 0), c(-3, 1)]);
    }

    /// Deux pour cent par case parcourue, sans base : cinq cases font 10 %,
    /// dix en font 20. Rien sans second portail.
    #[test]
    fn le_bonus_vaut_deux_pour_cent_par_case_sans_base() {
        assert_eq!(bonus_de_projection(&[c(0, 0), c(2, 3)]), 10);
        assert_eq!(bonus_de_projection(&[c(0, 0), c(4, -6)]), 20);
        assert_eq!(bonus_de_projection(&[c(0, 0), c(1, 0)]), 2);
        assert_eq!(bonus_de_projection(&[c(0, 0)]), 0);
    }

    /// 20 % au plus : onze cases, ou quinze en
    /// deux sauts, valent autant que dix. Les cases, elles, se comptent toutes,
    /// et la réponse les donne : de (0,0), le sort va d'abord en (-2,1), le
    /// plus proche, puis en (4,3), soit 3 + 8 cases.
    #[test]
    fn le_bonus_s_arrete_a_vingt_pour_cent() {
        assert_eq!(bonus_de_projection(&[c(0, 0), c(6, -5)]), 20);
        assert_eq!(bonus_de_projection(&[c(0, 0), c(4, 3), c(-2, 1)]), 20);
        let requete = RequetePortails {
            portails: vec![(0, 0), (4, 3), (-2, 1)],
            entree: (0, 0),
            tir: None,
            eliotrope: None,
            carte: None,
            damier: None,
        };
        let r: serde_json::Value = serde_json::from_str(&portails_json(&requete).unwrap()).unwrap();
        assert_eq!((r["cases"].as_u64(), r["bonus"].as_u64()), (Some(11), Some(20)));
    }

    /// Le réglage de la rotation s'arrête là où le bonus s'arrête : ses dix
    /// crans de 2 % font les 20 % du plafond.
    #[test]
    fn la_rotation_plafonne_au_meme_endroit() {
        let regles = crate::solve::load_ruleset(16).unwrap();
        let cases = regles
            .resources
            .iter()
            .find(|r| r.id == "cases_entre_portails")
            .expect("le réglage de la rotation");
        assert_eq!(2 * u32::from(cases.max), BONUS_MAX);
    }

    /// L'exemple posé : entrée A, puis B, sortie C, A-B à 3 cases, B-C à
    /// 5, A-C à 6. Le bonus suit les cases PARCOURUES, 3 + 5 = 8, soit 16 %, et
    /// non l'écart entrée-sortie, qui en ferait 12.
    #[test]
    fn le_bonus_suit_les_cases_parcourues() {
        let requete = RequetePortails {
            portails: vec![(0, 0), (3, 0), (2, 4)],
            entree: (0, 0),
            tir: None,
            eliotrope: None,
            carte: None,
            damier: None,
        };
        let r: serde_json::Value = serde_json::from_str(&portails_json(&requete).unwrap()).unwrap();
        assert_eq!(r["trajet"], serde_json::json!([[0, 0], [3, 0], [2, 4]]));
        assert_eq!(r["sortie"], serde_json::json!([2, 4]));
        assert_eq!(r["cases"], 8);
        assert_eq!(r["bonus"], 16);
    }

    /// Le sort ressort de la sortie avec le vecteur qui va de l'Éliotrope à
    /// l'entrée : l'Éliotrope trois cases à gauche de l'entrée, le sort
    /// atterrit trois cases à droite de la sortie.
    #[test]
    fn le_sort_atterrit_decale_comme_l_entree_l_est_de_l_eliotrope() {
        assert_eq!(case_d_arrivee(c(-5, 1), c(-2, 1), c(2, -3)), c(5, -3));
        let requete = RequetePortails {
            portails: vec![(-2, 1), (2, -3)],
            entree: (-2, 1),
            tir: None,
            eliotrope: Some((-5, 1)),
            carte: None,
            damier: None,
        };
        let r: serde_json::Value = serde_json::from_str(&portails_json(&requete).unwrap()).unwrap();
        assert_eq!(r["arrivee"], serde_json::json!([5, -3]));
    }

    /// Le sort repart de la sortie vers l'arrivée, et ses dégâts portent le
    /// bonus : ceux du Poing Fulgurant sur l'ennemi posé à l'arrivée valent
    /// 1,16 fois ceux d'un lancer ordinaire depuis la même case, huit cases
    /// parcourues.
    #[test]
    fn le_tir_repart_de_la_sortie_avec_le_bonus() {
        let tir = |lanceur: (i16, i16)| -> crate::solve::Request {
            serde_json::from_value(serde_json::json!({
                "class": 16,
                "level": 200,
                // Des nombres assez grands pour que l'arrondi ne pèse pas.
                "invested": { "chance": 400 },
                "deck": ["poing_fulgurant"],
                "placement": { "lanceur": lanceur, "visee": [3, 4], "ennemis": [[3, 4]] }
            }))
            .unwrap()
        };
        let requete = RequetePortails {
            portails: vec![(0, 0), (3, 0), (2, 4)],
            entree: (0, 0),
            // Le lanceur et la visée envoyés sont ignorés : la sortie et
            // l'arrivée comptent, l'Éliotrope une case à gauche de l'entrée
            // faisant atterrir le sort une case à droite de la sortie.
            tir: Some(tir((-5, -5))),
            eliotrope: Some((-1, 0)),
            carte: None,
            damier: None,
        };
        let r: serde_json::Value = serde_json::from_str(&portails_json(&requete).unwrap()).unwrap();
        let projete = r["tir"]["sorts"][0]["touches"][0]["normal"][1].as_i64().expect("un coup");
        let direct: serde_json::Value =
            serde_json::from_str(&crate::grille::zones_json(&tir((2, 4))).unwrap()).unwrap();
        let sec = direct["sorts"][0]["touches"][0]["normal"][1].as_i64().expect("un coup");
        let rapport = projete as f64 / sec as f64;
        assert!((rapport - 1.16).abs() < 0.02, "{projete} contre {sec} : {rapport:.3}");
    }

    /// Un allié projette ses sorts dans les portails d'un Éliotrope, avec le
    /// même bonus : l'Épée de Iop, projetée à huit cases parcourues, frappe
    /// 1,16 fois ce qu'elle frappe lancée de la sortie.
    #[test]
    fn un_allie_projette_ses_sorts_avec_le_meme_bonus() {
        let tir = |lanceur: (i16, i16)| -> crate::solve::Request {
            serde_json::from_value(serde_json::json!({
                "class": 8,
                "level": 200,
                "invested": { "strength": 400 },
                "deck": ["epee_de_iop"],
                "placement": { "lanceur": lanceur, "visee": [3, 4], "ennemis": [[3, 4]] }
            }))
            .unwrap()
        };
        let requete = RequetePortails {
            portails: vec![(0, 0), (3, 0), (2, 4)],
            entree: (0, 0),
            tir: Some(tir((-5, -5))),
            eliotrope: Some((-1, 0)),
            carte: None,
            damier: None,
        };
        let r: serde_json::Value = serde_json::from_str(&portails_json(&requete).unwrap()).unwrap();
        assert_eq!(r["bonus"], 16);
        let projete = r["tir"]["sorts"][0]["touches"][0]["normal"][1].as_i64().expect("un coup");
        let direct: serde_json::Value =
            serde_json::from_str(&crate::grille::zones_json(&tir((2, 4))).unwrap()).unwrap();
        let sec = direct["sorts"][0]["touches"][0]["normal"][1].as_i64().expect("un coup");
        let rapport = projete as f64 / sec as f64;
        assert!((rapport - 1.16).abs() < 0.02, "{projete} contre {sec} : {rapport:.3}");
    }

    /// Le portail d'entrée se vise depuis l'Éliotrope comme une cible
    /// ordinaire : collé à lui, rien à redire ; à six cases, hors de portée du
    /// Poing Fulgurant, et la réponse le dit.
    #[test]
    fn l_entree_se_vise_depuis_l_eliotrope() {
        let requete = |eliotrope: (i16, i16)| RequetePortails {
            portails: vec![(0, 0), (3, 0), (2, 4)],
            entree: (0, 0),
            tir: Some(
                serde_json::from_value(serde_json::json!({
                    "class": 16, "level": 200, "deck": ["poing_fulgurant"],
                    "placement": { "lanceur": [0, 0], "visee": [3, 4], "ennemis": [[3, 4]] }
                }))
                .unwrap(),
            ),
            eliotrope: Some(eliotrope),
            carte: None,
            damier: None,
        };
        let pres: serde_json::Value =
            serde_json::from_str(&portails_json(&requete((0, 1))).unwrap()).unwrap();
        assert!(pres["depuis_eliotrope"]["visee"].is_null(), "{}", pres["depuis_eliotrope"]);
        let loin: serde_json::Value =
            serde_json::from_str(&portails_json(&requete((0, 6))).unwrap()).unwrap();
        assert_eq!(loin["depuis_eliotrope"]["visee"]["motif"], "trop_loin", "{}", loin["depuis_eliotrope"]);
    }

    /// Un portail d'entrée qui n'est pas au sol est refusé plutôt que deviné.
    #[test]
    fn une_entree_hors_du_sol_est_refusee() {
        let requete = RequetePortails {
            portails: vec![(0, 0), (3, 0)],
            entree: (1, 1),
            tir: None,
            eliotrope: None,
            carte: None,
            damier: None,
        };
        assert!(portails_json(&requete).is_err());
    }

    /// Une projection sur Klime : l'Éliotrope en (-5, -3), les portails en
    /// `entree` et en (-2, -6), la sortie. Le sort atterrit à la sortie plus
    /// l'écart de l'entrée à l'Éliotrope.
    fn sur_klime(entree: (i16, i16)) -> serde_json::Value {
        let requete = RequetePortails {
            portails: vec![entree, (-2, -6)],
            entree,
            tir: Some(
                serde_json::from_value(serde_json::json!({
                    "class": 16, "level": 200, "deck": ["poing_fulgurant"],
                    "placement": { "lanceur": [0, 0], "visee": [0, 0], "ennemis": [] }
                }))
                .unwrap(),
            ),
            eliotrope: Some((-5, -3)),
            carte: Some(62),
            damier: None,
        };
        serde_json::from_str(&portails_json(&requete).unwrap()).unwrap()
    }

    /// Le sort ne ressort que vers du sol en vue de la sortie. Sur Klime, la ligne y = -6 porte un mur en x = 1 et un
    /// trou en x = 4 : atterrir sur l'un ou l'autre est refusé, atterrir
    /// derrière le mur aussi, faute de vue. Refusé, le sort ne frappe pas.
    #[test]
    fn sur_une_carte_l_arrivee_doit_etre_du_sol_en_vue() {
        let libre = sur_klime((-3, -3));
        assert_eq!(libre["arrivee"], serde_json::json!([0, -6]));
        assert!(libre["arrivee_refus"].is_null(), "{libre}");
        assert!(libre["tir"].is_object());
        for (entree, arrivee, motif) in [
            ((-2, -3), [1, -6], "hors_sol"),
            ((1, -3), [4, -6], "hors_sol"),
            ((0, -3), [3, -6], "hors_vue"),
        ] {
            let r = sur_klime(entree);
            assert_eq!(r["arrivee"], serde_json::json!(arrivee));
            assert_eq!(r["arrivee_refus"], motif, "{r}");
            assert!(r["tir"].is_null());
        }
    }

    /// Un portail se pose sur le sol de la carte, pas sur un mur.
    #[test]
    fn un_portail_sur_un_mur_est_refuse() {
        let requete = RequetePortails {
            portails: vec![(-2, -6), (1, -6)],
            entree: (-2, -6),
            tir: None,
            eliotrope: None,
            carte: Some(62),
            damier: None,
        };
        assert!(portails_json(&requete).is_err());
    }

    /// L'Éliotrope lui-même coupe la vue quand il se tient entre la sortie et
    /// l'arrivée : sortie en (0, 0), arrivée en (4, 0), lui en (2, 0). Un pas
    /// de côté, et le sort passe.
    #[test]
    fn l_eliotrope_entre_la_sortie_et_l_arrivee_coupe_la_vue() {
        let requete = |eliotrope: (i16, i16), entree: (i16, i16)| RequetePortails {
            portails: vec![entree, (0, 0)],
            entree,
            tir: None,
            eliotrope: Some(eliotrope),
            carte: None,
            damier: None,
        };
        let barre: serde_json::Value =
            serde_json::from_str(&portails_json(&requete((2, 0), (6, 0))).unwrap()).unwrap();
        assert_eq!(barre["arrivee"], serde_json::json!([4, 0]));
        assert_eq!(barre["arrivee_refus"], "hors_vue");
        let de_cote: serde_json::Value =
            serde_json::from_str(&portails_json(&requete((2, 1), (6, 1))).unwrap()).unwrap();
        assert_eq!(de_cote["arrivee"], serde_json::json!([4, 0]));
        assert!(de_cote["arrivee_refus"].is_null());
    }
}
