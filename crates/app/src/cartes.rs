//! Les cartes de boss, et le plateau sur lequel les KrozTools posent leurs
//! pièces. Les 35 cartes de donjon et de raid sont relevées dans
//! `data/cartes/insightroom.json`.
//!
//! # Le repère
//!
//! Le relevé range ses cases en 33 lignes et 34 colonnes, 560 cases en forme de
//! carte de Dofus. La colonne suit notre `x` et la ligne notre `y` : une case se
//! dessine en `((colonne - ligne) L/2, (colonne + ligne) H/2)`, la projection du
//! damier. Seule l'origine change : le centre des 560 cases, colonne 16,5 et
//! ligne 16, est ramené vers (0, 0), comme celui du damier vide.
//!
//! # Le damier vide
//!
//! Un carré sans mur, de quinze cases de côté par défaut et de vingt au plus :
//! les 35 cartes ont 233 cases de sol en moyenne, et le carré le plus proche est
//! celui de quinze, 225 cases.
//!
//! Les cartes sont embarquées dans le binaire : une carte illisible le serait dès
//! les tests plutôt qu'à l'usage.

use dofus_engine::Case;
use std::collections::BTreeSet;
use std::sync::OnceLock;

/// La colonne et la ligne du relevé qui deviennent (0, 0).
const ORIGINE: (i16, i16) = (16, 16);

/// Les dimensions de la grille du relevé.
const LIGNES: usize = 33;
const COLONNES: usize = 34;

/// Le côté du damier vide, par défaut et au plus.
pub const COTE_DEFAUT: u8 = 15;
pub const COTE_MAX: u8 = 20;

/// Les coordonnées extrêmes d'un damier de `cote` cases, sur chaque axe :
/// de -7 à 7 pour quinze, de -10 à 9 pour vingt. La page dessine le même.
fn bornes(cote: u8) -> (i16, i16) {
    let bas = -(i16::from(cote) / 2);
    (bas, bas + i16::from(cote) - 1)
}

/// Une carte de boss.
#[derive(Debug)]
pub struct Carte {
    pub id: u16,
    pub nom: String,
    pub categorie: String,
    sol: BTreeSet<Case>,
    murs: BTreeSet<Case>,
}

#[derive(serde::Deserialize)]
struct Fichier {
    cartes: Vec<Brute>,
}

#[derive(serde::Deserialize)]
struct Brute {
    id: u16,
    nom: String,
    categorie: String,
    lignes: Vec<String>,
}

fn lire(texte: &str) -> Result<Vec<Carte>, String> {
    let fichier: Fichier = serde_json::from_str(texte).map_err(|e| e.to_string())?;
    fichier
        .cartes
        .into_iter()
        .map(|brute| {
            if brute.lignes.len() != LIGNES {
                return Err(format!("carte {} : {} lignes", brute.id, brute.lignes.len()));
            }
            let (mut sol, mut murs) = (BTreeSet::new(), BTreeSet::new());
            for (ligne, texte) in brute.lignes.iter().enumerate() {
                if texte.chars().count() != COLONNES {
                    return Err(format!("carte {}, ligne {ligne} : largeur {}", brute.id, texte.len()));
                }
                for (colonne, signe) in texte.chars().enumerate() {
                    let case = Case::new(colonne as i16 - ORIGINE.0, ligne as i16 - ORIGINE.1);
                    match signe {
                        '.' => {
                            sol.insert(case);
                        }
                        '#' => {
                            murs.insert(case);
                        }
                        ' ' => {}
                        autre => return Err(format!("carte {} : signe inconnu {autre:?}", brute.id)),
                    }
                }
            }
            Ok(Carte {
                id: brute.id,
                nom: brute.nom,
                categorie: brute.categorie,
                sol,
                murs,
            })
        })
        .collect()
}

/// Toutes les cartes, lues une fois.
pub fn cartes() -> &'static [Carte] {
    static CARTES: OnceLock<Vec<Carte>> = OnceLock::new();
    CARTES.get_or_init(|| {
        lire(include_str!("../../../data/cartes/insightroom.json"))
            .expect("les cartes embarquées se lisent, un test le vérifie")
    })
}

/// Où se posent les pièces d'un outil : le damier vide de tant de cases de
/// côté, ou une carte.
#[derive(Clone, Copy, Debug)]
pub enum Plateau {
    Damier(u8),
    Carte(&'static Carte),
}

impl Plateau {
    /// Le plateau d'une requête : la carte nommée, qui doit exister, sinon le
    /// damier vide du côté demandé, quinze cases par défaut.
    pub fn de(carte: Option<u16>, damier: Option<u8>) -> Result<Plateau, String> {
        match carte {
            Some(id) => cartes()
                .iter()
                .find(|c| c.id == id)
                .map(Plateau::Carte)
                .ok_or_else(|| format!("carte inconnue : {id}")),
            None => match damier.unwrap_or(COTE_DEFAUT) {
                cote @ 1..=COTE_MAX => Ok(Plateau::Damier(cote)),
                cote => Err(format!("un damier de {cote} cases de côté : {COTE_MAX} au plus")),
            },
        }
    }

    /// Une case où l'on peut se tenir.
    pub fn est_sol(&self, c: Case) -> bool {
        match self {
            Plateau::Damier(cote) => {
                let (bas, haut) = bornes(*cote);
                (bas..=haut).contains(&c.x) && (bas..=haut).contains(&c.y)
            }
            Plateau::Carte(carte) => carte.sol.contains(&c),
        }
    }

    /// Les cases de sol, dans l'ordre des coordonnées.
    pub fn sol(&self) -> Vec<Case> {
        match self {
            Plateau::Damier(cote) => {
                let (bas, haut) = bornes(*cote);
                (bas..=haut)
                    .flat_map(|x| (bas..=haut).map(move |y| Case::new(x, y)))
                    .collect()
            }
            Plateau::Carte(carte) => carte.sol.iter().copied().collect(),
        }
    }

    /// Les murs : ils coupent la vue. Le damier vide n'en a pas.
    pub fn murs(&self) -> Vec<Case> {
        match self {
            Plateau::Damier(_) => Vec::new(),
            Plateau::Carte(carte) => carte.murs.iter().copied().collect(),
        }
    }

    /// Les cases hors du sol qui touchent le sol, diagonales comprises : là
    /// où bute un déplacement. Les murs et les trous en font partie. Le damier
    /// vide a son bord aussi : il arrête une poussée comme celui d'une carte.
    pub fn bords(&self) -> Vec<Case> {
        let sol: BTreeSet<Case> = self.sol().into_iter().collect();
        let mut bords: BTreeSet<Case> = BTreeSet::new();
        for c in &sol {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let voisine = Case::new(c.x + dx, c.y + dy);
                    if !sol.contains(&voisine) {
                        bords.insert(voisine);
                    }
                }
            }
        }
        bords.into_iter().collect()
    }

    fn est_mur(&self, c: Case) -> bool {
        matches!(self, Plateau::Carte(carte) if carte.murs.contains(&c))
    }

    /// `de` voit-il `vers` ? Les murs coupent la vue, et les `corps` aussi :
    /// en combat, toute créature posée entre deux cases la coupe. Un corps sur
    /// l'une des deux cases ne compte pas, on vise bien un ennemi.
    pub fn en_vue(&self, de: Case, vers: Case, corps: &[Case]) -> bool {
        dofus_grid::vue::en_vue(de, vers, |c| self.est_mur(c) || corps.contains(&c))
    }
}

/// Les cartes pour la page : leurs noms, leur catégorie, leur sol et leurs
/// murs, dans le repère des outils.
pub fn cartes_json() -> String {
    let paires = |cases: &BTreeSet<Case>| cases.iter().map(|c| [c.x, c.y]).collect::<Vec<_>>();
    serde_json::json!({
        "cartes": cartes().iter().map(|c| serde_json::json!({
            "id": c.id,
            "nom": c.nom,
            "categorie": c.categorie,
            "sol": paires(&c.sol),
            "murs": paires(&c.murs),
        })).collect::<Vec<_>>(),
    })
    .to_string()
}

/// Ce que l'onglet de la ligne de vue envoie.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct RequeteVue {
    /// La carte, ou le damier vide si absente.
    #[serde(default)]
    pub carte: Option<u16>,
    /// Le côté du damier vide, quinze par défaut.
    #[serde(default)]
    pub damier: Option<u8>,
    /// La case d'où l'on regarde.
    pub depuis: (i16, i16),
    /// Les créatures posées, qui coupent la vue comme en combat.
    #[serde(default)]
    pub corps: Vec<(i16, i16)>,
}

/// Les cases que `depuis` ne voit pas : le sol et les murs.
pub fn vue_json(requete: &RequeteVue) -> Result<String, String> {
    let plateau = Plateau::de(requete.carte, requete.damier)?;
    let depuis = Case::new(requete.depuis.0, requete.depuis.1);
    if !plateau.est_sol(depuis) {
        return Err("on ne regarde que depuis une case de sol".into());
    }
    let corps: Vec<Case> = requete.corps.iter().map(|&(x, y)| Case::new(x, y)).collect();
    let cache = |c: &Case| !plateau.en_vue(depuis, *c, &corps);
    let sol = plateau.sol();
    let sol_cache: Vec<[i16; 2]> = sol.iter().filter(|c| cache(c)).map(|c| [c.x, c.y]).collect();
    let murs_caches: Vec<[i16; 2]> = plateau
        .murs()
        .iter()
        .filter(|c| cache(c))
        .map(|c| [c.x, c.y])
        .collect();
    Ok(serde_json::json!({
        "sol": sol.len(),
        "sol_cache": sol_cache,
        "murs_caches": murs_caches,
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carte(id: u16) -> &'static Carte {
        cartes().iter().find(|c| c.id == id).expect("carte relevée")
    }

    /// Les 35 cartes se lisent, et chacune compte le sol et les murs que le
    /// relevé a comptés à la source. Klime, au vieux format qui ne liste que
    /// ses murs et ses trous : 560 cases moins 57 murs et 367 trous.
    #[test]
    fn les_35_cartes_se_lisent_avec_leurs_murs() {
        assert_eq!(cartes().len(), 35);
        let klime = carte(62);
        assert_eq!(klime.nom, "Klime");
        assert_eq!((klime.sol.len(), klime.murs.len()), (560 - 57 - 367, 57));
        let balladone = carte(84);
        assert_eq!((balladone.sol.len(), balladone.murs.len()), (212, 20));
        assert!(cartes().iter().all(|c| !c.sol.is_empty() && c.sol.is_disjoint(&c.murs)));
    }

    /// Le repère, lu sur le dessin de Klime : sa ligne 7 est un mur de la colonne 11
    /// à la 21, et la ligne 8 commence par un mur en colonne 11 suivi de sol. Une
    /// erreur d'axe ou d'origine décalerait toute la carte.
    #[test]
    fn la_colonne_donne_x_et_la_ligne_y() {
        let klime = carte(62);
        assert!((-5..=5).all(|x| klime.murs.contains(&Case::new(x, -9))));
        assert!(klime.murs.contains(&Case::new(-5, -8)));
        assert!(klime.sol.contains(&Case::new(-4, -8)));
        // Toutes les cases tiennent dans la grille décalée, et le sol presque
        // plein de Bella a son centre en (0, 0).
        let toutes = cartes().iter().flat_map(|c| c.sol.iter().chain(&c.murs).copied());
        assert!(toutes.into_iter().all(|c| (-16..=17).contains(&c.x) && (-16..=16).contains(&c.y)));
        let bella = carte(154);
        let n = bella.sol.len() as i32;
        let (sx, sy) = bella
            .sol
            .iter()
            .fold((0i32, 0i32), |(a, b), c| (a + i32::from(c.x), b + i32::from(c.y)));
        assert!((sx / n).abs() <= 1 && (sy / n).abs() <= 1, "centre de Bella : {} {}", sx / n, sy / n);
    }

    /// La ligne de vue contre une référence relevée : six cartes, trois points de
    /// vue chacune, dans le repère du relevé. Le repère et la règle sont vérifiés
    /// ensemble.
    #[test]
    fn la_ligne_de_vue_suit_les_releves_des_cartes() {
        #[derive(serde::Deserialize)]
        struct Releve {
            cas: Vec<Cas>,
        }
        #[derive(serde::Deserialize)]
        struct Cas {
            carte: u16,
            depuis: [i16; 2],
            cachees: Vec<[i16; 2]>,
        }
        let releve: Releve =
            serde_json::from_str(include_str!("../tests/fixtures/vue-cartes.json")).unwrap();
        let ici = |[ligne, colonne]: [i16; 2]| [colonne - ORIGINE.0, ligne - ORIGINE.1];
        assert_eq!(releve.cas.len(), 18);
        for cas in releve.cas {
            let [x, y] = ici(cas.depuis);
            let reponse: serde_json::Value = serde_json::from_str(
                &vue_json(&RequeteVue { carte: Some(cas.carte), damier: None, depuis: (x, y), corps: vec![] })
                    .unwrap(),
            )
            .unwrap();
            let mut calculees: Vec<[i16; 2]> = ["sol_cache", "murs_caches"]
                .iter()
                .flat_map(|cle| serde_json::from_value::<Vec<[i16; 2]>>(reponse[*cle].clone()).unwrap())
                .collect();
            let mut attendues: Vec<[i16; 2]> = cas.cachees.into_iter().map(ici).collect();
            calculees.sort_unstable();
            attendues.sort_unstable();
            assert_eq!(calculees, attendues, "carte {} depuis {:?}", cas.carte, (x, y));
        }
    }

    #[test]
    fn une_carte_inconnue_est_refusee() {
        assert!(Plateau::de(Some(1), None).is_err());
        assert!(matches!(Plateau::de(None, None), Ok(Plateau::Damier(COTE_DEFAUT))));
        assert!(Plateau::de(None, Some(COTE_MAX + 1)).is_err());
    }

    /// Le damier par défaut a la taille moyenne des cartes de boss : un relevé qui
    /// changerait la moyenne fait échouer ce test.
    #[test]
    fn le_damier_par_defaut_a_la_taille_moyenne_des_cartes() {
        let moyenne = cartes().iter().map(|c| c.sol.len()).sum::<usize>() as f64 / cartes().len() as f64;
        assert_eq!(moyenne.sqrt().round() as u8, COTE_DEFAUT, "moyenne {moyenne:.0}");
        assert_eq!(Plateau::Damier(COTE_DEFAUT).sol().len(), 225);
    }

    /// Les bords d'une carte entourent son sol : chaque voisine d'une case de
    /// sol est du sol ou un bord, et aucun bord n'est du sol.
    #[test]
    fn les_bords_entourent_le_sol() {
        let klime = Plateau::de(Some(62), None).unwrap();
        let bords = klime.bords();
        assert!(bords.iter().all(|c| !klime.est_sol(*c)));
        for c in klime.sol() {
            for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let v = Case::new(c.x + dx, c.y + dy);
                assert!(klime.est_sol(v) || bords.contains(&v), "{v:?}");
            }
        }
        // Le damier vide a son bord, l'anneau qui l'entoure : 17 × 17 − 15 × 15.
        let bord = Plateau::Damier(COTE_DEFAUT).bords();
        assert_eq!(bord.len(), 64);
        assert!(bord.iter().all(|c| c.x.abs().max(c.y.abs()) == 8), "{bord:?}");
    }

    /// Un côté pair se décale d'une case : vingt cases vont de -10 à 9.
    #[test]
    fn le_damier_de_vingt_va_de_moins_dix_a_neuf() {
        let d = Plateau::Damier(20);
        assert_eq!(d.sol().len(), 400);
        assert!(d.est_sol(Case::new(-10, 9)) && !d.est_sol(Case::new(10, 0)));
        let q = Plateau::Damier(15);
        assert!(q.est_sol(Case::new(7, -7)) && !q.est_sol(Case::new(8, 0)));
    }

    /// Un corps coupe la vue, sauf sur l'une des deux cases.
    #[test]
    fn un_corps_coupe_la_vue() {
        let t = Plateau::Damier(COTE_DEFAUT);
        let (a, b) = (Case::new(-3, 0), Case::new(3, 0));
        assert!(t.en_vue(a, b, &[]));
        assert!(!t.en_vue(a, b, &[Case::new(0, 0)]));
        assert!(t.en_vue(a, b, &[a, b]));
    }
}
