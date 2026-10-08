//! Le tir : un lanceur, une case visée, des cibles posées, et qui est touché.
//! L'éloignement de chaque cible se mesure sur la grille. Ni ligne de vue (c'est
//! `vue`, pour l'appelant qui a une carte), ni dégâts (c'est le moteur).

use crate::{cases_de_zone, Case};

/// Les contraintes de lancement d'un sort. `en_ligne` veut dire « que sur un
/// axe » ; les deux booléens à vrai autorisent l'un ou l'autre, les deux à faux
/// laissent viser librement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contraintes {
    pub portee_min: u16,
    pub portee_max: u16,
    pub en_ligne: bool,
    pub en_diagonale: bool,
}

impl Contraintes {
    /// Une portée libre, sans contrainte d'axe. Le défaut de la plupart des
    /// sorts du parc.
    pub const fn libre(portee_min: u16, portee_max: u16) -> Self {
        Self {
            portee_min,
            portee_max,
            en_ligne: false,
            en_diagonale: false,
        }
    }
}

/// Pourquoi un tir ne part pas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refus {
    TropPres {
        distance: u16,
        minimum: u16,
    },
    TropLoin {
        distance: u16,
        maximum: u16,
    },
    /// Le sort exige un axe ou une diagonale, la visée n'en suit aucun.
    HorsAxe,
    /// La forme lit la direction du tir, et la visée n'est pas alignée : la
    /// donnée ne dit pas comment le jeu oriente une zone visée en biais.
    DirectionIndeterminee,
    /// La forme n'est pas dessinée. Voir `cases_de_zone`.
    FormeNonDessinee(char),
}

/// Une cible atteinte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Touche {
    /// Son rang dans la liste passée à `tir`, pour la retrouver.
    pub indice: usize,
    pub case: Case,
    /// Sa distance à la case d'impact, en cases (zéro sur l'impact), que la
    /// dégressivité consomme.
    pub eloignement: u16,
}

/// Ce qu'un tir couvre et qui il atteint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tir {
    pub impact: Case,
    /// Toutes les cases de la zone, triées, sans doublon.
    pub cases: Vec<Case>,
    /// Les cibles atteintes, de la plus proche de l'impact à la plus lointaine.
    /// ⚠️ L'ordre compte : un sort plafonné à `n` cibles frappe les `n` premières.
    pub touches: Vec<Touche>,
}

/// Les formes qui lisent la direction du tir. Un test vérifie que les autres
/// rendent les mêmes cases dans les quatre directions.
pub const FORMES_ORIENTEES: [char; 7] = ['T', 'L', '-', 'U', 'V', 'R', 'F'];

/// La direction d'un tir, du lanceur vers la case visée. `None` hors des quatre
/// axes, diagonale comprise : rien ne dit quel axe oriente alors une zone. Un sort
/// à zone symétrique s'en passe, les autres se refusent.
pub fn direction_de_tir(lanceur: Case, visee: Case) -> Option<(i16, i16)> {
    let dx = visee.x - lanceur.x;
    let dy = visee.y - lanceur.y;
    match (dx == 0, dy == 0) {
        (false, true) => Some((dx.signum(), 0)),
        (true, false) => Some((0, dy.signum())),
        _ => None,
    }
}

/// La visée respecte-t-elle la portée et l'axe.
pub fn visee_permise(lanceur: Case, visee: Case, c: Contraintes) -> Result<(), Refus> {
    let distance = lanceur.distance(visee);
    if distance < c.portee_min {
        return Err(Refus::TropPres {
            distance,
            minimum: c.portee_min,
        });
    }
    if distance > c.portee_max {
        return Err(Refus::TropLoin {
            distance,
            maximum: c.portee_max,
        });
    }
    if !c.en_ligne && !c.en_diagonale {
        return Ok(());
    }
    let (dx, dy) = (visee.x - lanceur.x, visee.y - lanceur.y);
    // Se viser soi-même satisfait tous les axes : il n'y a pas de biais.
    if dx == 0 && dy == 0 {
        return Ok(());
    }
    let sur_axe = c.en_ligne && (dx == 0 || dy == 0);
    let sur_diagonale = c.en_diagonale && dx.abs() == dy.abs();
    if sur_axe || sur_diagonale {
        Ok(())
    } else {
        Err(Refus::HorsAxe)
    }
}

/// Tirer : qui est touché, et à quelle distance de l'impact. Deux cibles peuvent
/// partager une case : la grille ne connaît pas les corps.
pub fn tir(
    forme: char,
    taille: u16,
    taille2: u16,
    lanceur: Case,
    visee: Case,
    cibles: &[Case],
    contraintes: Contraintes,
) -> Result<Tir, Refus> {
    visee_permise(lanceur, visee, contraintes)?;
    let direction = direction_de_tir(lanceur, visee);
    let cases = if forme == 'l' {
        // La ligne depuis le lanceur, seule forme dont la longueur vient du tir : de la
        // case qui suit le lanceur jusqu'à la visée incluse, soit `distance` cases.
        let Some((dx, dy)) = direction else {
            return Err(Refus::DirectionIndeterminee);
        };
        let longueur = i16::try_from(lanceur.distance(visee)).unwrap_or(0);
        (1..=longueur)
            .map(|i| Case::new(lanceur.x + dx * i, lanceur.y + dy * i))
            .collect()
    } else {
        let direction = match direction {
            Some(d) => d,
            // Une forme symétrique se moque de la direction : un test le
            // vérifie forme par forme. Une forme orientée, elle, ne peut pas
            // être devinée, et le tir se refuse.
            None if FORMES_ORIENTEES.contains(&forme) => return Err(Refus::DirectionIndeterminee),
            None => (1, 0),
        };
        cases_de_zone(forme, taille, taille2, visee, direction)
            .ok_or(Refus::FormeNonDessinee(forme))?
    };
    let mut touches: Vec<Touche> = cibles
        .iter()
        .enumerate()
        .filter(|(_, c)| cases.contains(c))
        .map(|(indice, &case)| Touche {
            indice,
            case,
            eloignement: visee.distance(case),
        })
        .collect();
    touches.sort_by_key(|t| (t.eloignement, t.indice));
    Ok(Tir {
        impact: visee,
        cases,
        touches,
    })
}
