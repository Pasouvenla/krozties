//! La ligne de vue : une case en voit-elle une autre ?
//!
//! Le trait va du centre d'une case au centre de l'autre et traverse, dans
//! l'ordre, les cases qu'il coupe. Quand il passe exactement par un coin, il
//! saute en diagonale : les deux cases qui touchent ce coin ne comptent pas.
//! Seules les cases intermédiaires sont regardées, ni le départ ni l'arrivée,
//! et une seule qui bloque coupe la vue.
//!
//! Ce qui bloque, c'est l'appelant qui le dit : les murs d'une carte, les corps
//! entre les deux cases. Un trou ne bloque pas. La règle est symétrique : le
//! trait coupe les mêmes cases dans les deux sens.

use crate::Case;

/// Les cases que le trait de `de` à `vers` traverse, dans l'ordre, sans les
/// deux extrémités.
pub fn traversees(de: Case, vers: Case) -> Vec<Case> {
    let (dx, dy) = (i32::from(vers.x) - i32::from(de.x), i32::from(vers.y) - i32::from(de.y));
    let (sx, sy) = (dx.signum() as i16, dy.signum() as i16);
    let (nx, ny) = (dx.abs(), dy.abs());
    // Le trait coupe la i-ième ligne verticale de la grille à t = (2i + 1) / 2nx
    // et la j-ième horizontale à t = (2j + 1) / 2ny. Les comparer revient à
    // comparer (2i + 1)·ny et (2j + 1)·nx, en entiers : à égalité, c'est un coin.
    let (mut i, mut j) = (0, 0);
    let mut case = de;
    let mut cases = Vec::new();
    while i < nx || j < ny {
        let ecart = (2 * i + 1) * ny - (2 * j + 1) * nx;
        let par_x = j == ny || (i < nx && ecart <= 0);
        let par_y = i == nx || (j < ny && ecart >= 0);
        if par_x {
            case = Case::new(case.x + sx, case.y);
            i += 1;
        }
        if par_y {
            case = Case::new(case.x, case.y + sy);
            j += 1;
        }
        if i < nx || j < ny {
            cases.push(case);
        }
    }
    cases
}

/// `de` voit-il `vers`, quand `bloque` dit quelles cases coupent la vue ?
pub fn en_vue(de: Case, vers: Case, bloque: impl Fn(Case) -> bool) -> bool {
    !traversees(de, vers).into_iter().any(bloque)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(x: i16, y: i16) -> Case {
        Case::new(x, y)
    }

    /// En ligne droite, les cases du milieu, et elles seules.
    #[test]
    fn en_ligne_droite_les_cases_du_milieu() {
        assert_eq!(traversees(c(0, 0), c(4, 0)), vec![c(1, 0), c(2, 0), c(3, 0)]);
        assert_eq!(traversees(c(0, 0), c(0, -3)), vec![c(0, -1), c(0, -2)]);
        assert!(traversees(c(0, 0), c(1, 0)).is_empty());
        assert!(traversees(c(2, 2), c(2, 2)).is_empty());
    }

    /// Par un coin exact, le trait saute en diagonale : les deux cases qui
    /// touchent le coin ne comptent pas.
    #[test]
    fn par_un_coin_le_trait_saute_en_diagonale() {
        assert_eq!(traversees(c(0, 0), c(2, 2)), vec![c(1, 1)]);
        assert!(traversees(c(0, 0), c(1, 1)).is_empty());
        let murs = [c(1, 0), c(0, 1)];
        assert!(en_vue(c(0, 0), c(1, 1), |x| murs.contains(&x)));
    }

    /// Hors des coins, le trait passe d'une case à sa voisine, du côté par
    /// lequel il sort : de (0,0) à (3,1), il coupe (1,0), puis le coin entre
    /// (1,0) et (2,1).
    #[test]
    fn hors_des_coins_une_case_voisine_a_la_fois() {
        assert_eq!(traversees(c(0, 0), c(3, 1)), vec![c(1, 0), c(2, 1)]);
        assert_eq!(traversees(c(0, 0), c(1, 3)), vec![c(0, 1), c(1, 2)]);
        assert_eq!(traversees(c(0, 0), c(2, 1)), vec![c(1, 0), c(1, 1)]);
    }

    /// Un mur entre les deux coupe la vue ; un mur sur l'une des deux cases,
    /// non : seules les cases intermédiaires comptent.
    #[test]
    fn seules_les_cases_intermediaires_bloquent() {
        let murs = [c(2, 0)];
        assert!(!en_vue(c(0, 0), c(4, 0), |x| murs.contains(&x)));
        assert!(en_vue(c(0, 0), c(2, 0), |x| murs.contains(&x)));
        assert!(en_vue(c(2, 0), c(4, 0), |x| murs.contains(&x)));
    }

    /// Dans les deux sens, les mêmes cases, en ordre inverse.
    #[test]
    fn le_trait_est_symetrique() {
        for (ax, ay, bx, by) in (-6..=6).flat_map(|ax| (-6..=6).flat_map(move |ay| {
            (-6..=6).flat_map(move |bx| (-6..=6).map(move |by| (ax, ay, bx, by)))
        })) {
            let mut retour = traversees(c(bx, by), c(ax, ay));
            retour.reverse();
            assert_eq!(traversees(c(ax, ay), c(bx, by)), retour, "({ax},{ay}) vers ({bx},{by})");
        }
    }
}
