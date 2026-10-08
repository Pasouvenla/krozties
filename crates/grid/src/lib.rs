//! La grille de Dofus : des cases, des distances, et les cases qu'une zone
//! couvre. Le module ne calcule aucun dégât et ne connaît aucun sort : étant
//! donné une forme, une taille et un point d'impact, il rend les cases touchées.
//!
//! # Le repère
//!
//! Une case porte deux entiers. Les quatre directions du jeu font varier l'un
//! ou l'autre de un : la distance entre deux cases est la somme des écarts
//! absolus.

/// Une case de la grille.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Case {
    pub x: i16,
    pub y: i16,
}

impl Case {
    pub const fn new(x: i16, y: i16) -> Self {
        Self { x, y }
    }

    /// La distance du jeu entre deux cases.
    pub fn distance(self, autre: Case) -> u16 {
        let dx = (self.x - autre.x).unsigned_abs();
        let dy = (self.y - autre.y).unsigned_abs();
        dx + dy
    }

    fn plus(self, dx: i16, dy: i16) -> Case {
        Case::new(self.x + dx, self.y + dy)
    }
}

/// Les quatre directions du jeu, dans l'ordre horaire.
pub const DIRECTIONS: [(i16, i16); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// Les quatre diagonales.
pub const DIAGONALES: [(i16, i16); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// Les cases qu'une zone couvre autour d'un point d'impact. `direction` est
/// celle du tir, du lanceur vers l'impact ; seules les formes orientées la lisent
/// (cône, demi-cercle, fourche). Rend `None` pour la ligne depuis le lanceur `l`,
/// dont la longueur est la distance de tir : c'est `tir` qui la dessine.
pub fn cases_de_zone(
    forme: char,
    taille: u16,
    taille2: u16,
    impact: Case,
    direction: (i16, i16),
) -> Option<Vec<Case>> {
    let r = i16::try_from(taille).unwrap_or(0);
    let mut out = Vec::new();
    match forme {
        // Une seule case.
        'P' => out.push(impact),
        // Cercle : tout ce qui est à portée `r`, la distance étant la somme
        // des écarts. Deux fois r fois (r + 1), plus un.
        'C' => {
            for dx in -r..=r {
                for dy in -r..=r {
                    if dx.abs() + dy.abs() <= r {
                        out.push(impact.plus(dx, dy));
                    }
                }
            }
        }
        // Anneau : la couronne seule, à la distance exacte.
        'O' => {
            for dx in -r..=r {
                for dy in -r..=r {
                    if dx.abs() + dy.abs() == r {
                        out.push(impact.plus(dx, dy));
                    }
                }
            }
        }
        // ⚠️ `X` et `+` ont le même nombre de cases (`4N + 1`), pas la même forme. La
        // lettre décrit ce que le joueur voit, et l'écran tourne le repère de 45
        // degrés : une croix sur les quatre directions du jeu apparaît en X, une croix
        // sur les diagonales en +.
        'X' => {
            out.push(impact);
            for (dx, dy) in DIRECTIONS {
                for i in 1..=r {
                    out.push(impact.plus(dx * i, dy * i));
                }
            }
        }
        '+' => {
            out.push(impact);
            for (dx, dy) in DIAGONALES {
                for i in 1..=r {
                    out.push(impact.plus(dx * i, dy * i));
                }
            }
        }
        // Étoile : la croix ET les diagonales, le centre compté une fois.
        '*' => {
            out.push(impact);
            for (dx, dy) in DIRECTIONS.iter().chain(DIAGONALES.iter()) {
                for i in 1..=r {
                    out.push(impact.plus(dx * i, dy * i));
                }
            }
        }
        // Croix en diagonale : quatre bras, de la distance `taille2` à
        // `taille`, et le centre seulement quand `taille2` vaut zéro.
        'Q' => {
            let debut = i16::try_from(taille2.max(1)).unwrap_or(1);
            if taille2 == 0 {
                out.push(impact);
            }
            for (dx, dy) in DIAGONALES {
                for i in debut..=r {
                    out.push(impact.plus(dx * i, dy * i));
                }
            }
        }
        // Carré plein.
        'G' => {
            for dx in -r..=r {
                for dy in -r..=r {
                    out.push(impact.plus(dx, dy));
                }
            }
        }
        // Carré privé de ses quatre coins.
        'W' => {
            for dx in -r..=r {
                for dy in -r..=r {
                    if !(dx.abs() == r && dy.abs() == r) || r == 0 {
                        out.push(impact.plus(dx, dy));
                    }
                }
            }
        }
        // Ligne perpendiculaire au tir, `r` de chaque côté.
        'T' => {
            let (px, py) = (-direction.1, direction.0);
            out.push(impact);
            for i in 1..=r {
                out.push(impact.plus(px * i, py * i));
                out.push(impact.plus(-px * i, -py * i));
            }
        }
        // Ligne droite depuis l'impact, dans le sens du tir.
        'L' | '-' => {
            for i in 0..r.max(1) {
                out.push(impact.plus(direction.0 * i, direction.1 * i));
            }
        }
        // Le « demi-cercle » est un V qui revient vers le lanceur : la case visée,
        // puis à chaque cran une case de chaque côté, un cran en arrière. `2N + 1`
        // cases.
        'U' => {
            let (px, py) = (-direction.1, direction.0);
            out.push(impact);
            for i in 1..=r {
                let (ax, ay) = (-direction.0 * i, -direction.1 * i);
                out.push(impact.plus(ax + px * i, ay + py * i));
                out.push(impact.plus(ax - px * i, ay - py * i));
            }
        }
        // Cône, ouvert à l'opposé du lanceur : des rangs de 1, 3, 5 cases.
        'V' => {
            let (px, py) = (-direction.1, direction.0);
            for rang in 0..=r {
                for k in -rang..=rang {
                    out.push(impact.plus(direction.0 * rang + px * k, direction.1 * rang + py * k));
                }
            }
        }
        // Rectangle : `2 * taille + 1` de large, `taille2 + 1` de profond, depuis
        // l'impact dans le sens du tir. Le jeu donne les côtés, pas la position, et une
        // profondeur paire ne peut pas être centrée sur l'impact : ce placement n'est
        // pas vérifié en jeu.
        'R' => {
            let profondeur = i16::try_from(taille2).unwrap_or(0);
            let (px, py) = (-direction.1, direction.0);
            for avant in 0..=profondeur {
                for cote in -r..=r {
                    out.push(impact.plus(
                        direction.0 * avant + px * cote,
                        direction.1 * avant + py * cote,
                    ));
                }
            }
        }
        // La fourche : trois dents qui partent de la case visée, droit devant et en
        // diagonale vers l'avant, de `taille + 1` cases chacune.
        'F' => {
            let (px, py) = (-direction.1, direction.0);
            out.push(impact);
            for i in 1..=r + 1 {
                let (ax, ay) = (direction.0 * i, direction.1 * i);
                out.push(impact.plus(ax, ay));
                out.push(impact.plus(ax + px * i, ay + py * i));
                out.push(impact.plus(ax - px * i, ay - py * i));
            }
        }
        // La ligne depuis le lanceur, que `tir` dessine parce qu'il sait d'où
        // part le tir.
        'l' => return None,
        _ => return None,
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}

/// Le tir : placer le lanceur et les cibles, et dire qui est touché.
pub mod tir;
pub mod vue;
