//! Les invocations : les dégâts de chaque attaque, avec le build.
//!
//! Une attaque d'invocation prend la caractéristique de l'élément, la Puissance
//! et les dommages de l'élément de l'invocateur, multipliés par un facteur, et
//! aucun % de dommages (finaux, aux sorts, distance ou mêlée) ni dommages
//! critiques. Le facteur vaut 1, sauf chez l'Osamodas : (1 + palier) × 25 %.
//!
//! La donnée vient d'un relevé : les attaques et leurs fourchettes au format
//! DofusBook, le coût en PA, les lancers et le critique de chacune depuis
//! l'instantané. Ni la Lance du Forgelance, ni les glyphes du Féca.

use std::sync::OnceLock;

use dofus_damage::{DamageProfile, Element};
use serde::Deserialize;

const OSAMODAS: u32 = 2;

#[derive(Clone, Debug, Deserialize)]
pub struct Releve {
    pub invocations: Vec<Invocation>,
}

/// Un sort d'invocation et ce qu'il invoque, rang par rang. La classe 0 désigne
/// les invocations communes à toutes les classes (Arakne, Chaferfu, Cawotte).
#[derive(Clone, Debug, Deserialize)]
pub struct Invocation {
    pub classe: u32,
    pub sort: u32,
    pub nom: String,
    #[serde(default)]
    pub monstre: Option<u32>,
    /// L'autre monstre que le sort peut invoquer, et sa chance : les sorts
    /// communs tirent un monstre à 80 % et l'autre de leur paire à 20 % (effets
    /// 181 à `random` 80 et 20).
    #[serde(default)]
    pub autre: Option<Autre>,
    pub rangs: Vec<Rang>,
}

/// Voir [`Invocation::autre`].
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Autre {
    pub monstre: u32,
    /// En pour cent ; le monstre du sort a le reste.
    pub chance: u8,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Rang {
    pub rang: u8,
    /// Le niveau du personnage qui débloque ce rang.
    pub niveau: u32,
    #[serde(default)]
    pub nom: Option<String>,
    /// Le palier de l'invocation, qui fixe chez l'Osamodas la part de ses
    /// caractéristiques qu'elle reçoit.
    #[serde(default)]
    pub pi: Option<u8>,
    #[serde(default)]
    pub pa: Option<i16>,
    /// -1 pour une invocation qui ne se déplace pas : une pelle, un arbre.
    #[serde(default)]
    pub pm: Option<i16>,
    pub attaques: Vec<Attaque>,
}

/// Une attaque de l'invocation : ses effets au format DofusBook (code,
/// critique, min, max, durée, sans dommages critiques), puis ce que
/// l'instantané en dit.
#[derive(Clone, Debug, Deserialize)]
pub struct Attaque {
    pub nom: String,
    pub cibles: String,
    pub effets: Vec<(String, u8, i32, i32, i32, i32)>,
    #[serde(default)]
    pub pa: Option<u8>,
    #[serde(default)]
    pub lancers_par_tour: Option<u8>,
    #[serde(default)]
    pub lancers_par_cible: Option<u8>,
    #[serde(default)]
    pub critique: Option<u8>,
    /// L'intervalle de relance, en tours : 2 pour un lancer tous les deux tours.
    #[serde(default)]
    pub relance: Option<u8>,
    /// L'attaque tue l'invocation qui la lance (« Tue la cible » de l'Explosion
    /// Ouatée).
    #[serde(default)]
    pub sacrifie: bool,
}

/// Le relevé embarqué, lu une fois.
pub fn releve() -> &'static Releve {
    static RELEVE: OnceLock<Releve> = OnceLock::new();
    RELEVE.get_or_init(|| {
        serde_json::from_str(crate::donnees::INVOCATIONS).expect("le relevé des invocations doit se lire")
    })
}

/// La part des caractéristiques, de la Puissance et des dommages de
/// l'invocateur que l'invocation reçoit, en pour cent : 100, sauf chez
/// l'Osamodas, (1 + palier) × 25 (50, 75 ou 100).
pub fn facteur(classe: u32, pi: Option<u8>) -> i64 {
    match pi {
        Some(p) if classe == OSAMODAS && p > 0 => (1 + i64::from(p)) * 25,
        _ => 100,
    }
}

/// Le meilleur et le pire élément : la plus haute caractéristique, les
/// égalités départagées par les dommages de l'élément.
fn meilleur_et_pire(profil: &DamageProfile) -> (Element, Element) {
    let elements = [Element::Neutral, Element::Earth, Element::Fire, Element::Air, Element::Water];
    let carac = |e: Element| profil.elements[e.index()].characteristic;
    let dommages = |e: Element| profil.elements[e.index()].flat_damage;
    let haut = elements.iter().map(|e| carac(*e)).max().unwrap_or(0);
    let bas = elements.iter().map(|e| carac(*e)).min().unwrap_or(0);
    let meilleur = elements
        .iter()
        .filter(|e| carac(**e) == haut)
        .max_by_key(|e| (dommages(**e), std::cmp::Reverse(e.index())))
        .copied()
        .unwrap_or(Element::Neutral);
    let pire = elements
        .iter()
        .filter(|e| carac(**e) == bas)
        .min_by_key(|e| (dommages(**e), e.index()))
        .copied()
        .unwrap_or(Element::Neutral);
    (meilleur, pire)
}

/// L'élément d'un code d'effet, et s'il vole de la vie : « da » des dégâts Air,
/// « va » un vol Air, « dme » le meilleur élément, « dpe » le pire. Tout autre
/// code ne frappe pas.
fn element_du_code(code: &str, profil: &DamageProfile) -> Option<(Element, bool)> {
    let (meilleur, pire) = meilleur_et_pire(profil);
    let vol = code.starts_with('v');
    let element = match code {
        "dn" | "vn" => Element::Neutral,
        "dt" | "vt" => Element::Earth,
        "df" | "vf" => Element::Fire,
        "de" | "ve" => Element::Water,
        "da" | "va" => Element::Air,
        "dme" | "vme" => meilleur,
        "dpe" | "vpe" => pire,
        _ => return None,
    };
    Some((element, vol))
}

/// Les éléments que frappent les attaques de cette invocation au niveau donné,
/// et si l'une frappe dans le meilleur élément de l'invocateur. Le pire élément
/// ne compte pas : il ne sert jamais un build.
pub fn elements_de(invocation: &Invocation, niveau: u32) -> (Vec<Element>, bool) {
    let Some(rang) = rang_au_niveau(invocation, niveau) else {
        return (Vec::new(), false);
    };
    let mut elements = Vec::new();
    let mut meilleur = false;
    for (code, ..) in rang.attaques.iter().flat_map(|a| &a.effets) {
        match code.as_str() {
            "dme" | "vme" => meilleur = true,
            "dpe" | "vpe" => {}
            c => {
                if let Some((e, _)) = element_du_code(c, &DamageProfile::default()) {
                    elements.push(e);
                }
            }
        }
    }
    elements.sort_by_key(|e| e.index());
    elements.dedup();
    (elements, meilleur)
}

/// Un jet d'attaque d'invocation : la caractéristique, la Puissance et les
/// dommages de l'élément de l'invocateur, chacun pris au facteur, et rien
/// d'autre ; la caractéristique ne descend pas sous zéro.
pub fn coup(base: i32, element: Element, profil: &DamageProfile, facteur: i64) -> i64 {
    let s = profil.elements[element.index()];
    let carac = (i64::from(s.characteristic.max(0)) * facteur).div_euclid(100);
    let puissance = (i64::from(profil.power) * facteur).div_euclid(100);
    let dommages = (i64::from(s.flat_damage) * facteur).div_euclid(100);
    (i64::from(base) * (100 + carac + puissance)).div_euclid(100) + dommages
}

/// Le rang qu'un personnage de ce niveau lance : le plus haut qu'il débloque.
pub fn rang_au_niveau(invocation: &Invocation, niveau: u32) -> Option<&Rang> {
    invocation.rangs.iter().filter(|r| r.niveau <= niveau).max_by_key(|r| r.rang).or(invocation.rangs.first())
}

/// Une ligne de dégâts d'une attaque d'invocation : son élément et ses
/// fourchettes de base.
#[derive(Clone, Debug, PartialEq)]
pub struct LigneJouee {
    pub element: Element,
    pub normal: (i32, i32),
    pub critique: (i32, i32),
}

/// Une attaque qu'une invocation joue dans son tour, et combien de fois : ses
/// lignes sur un monstre ennemi, son coût, son taux de critique.
#[derive(Clone, Debug, PartialEq)]
pub struct AttaqueJouee {
    pub nom: String,
    pub lignes: Vec<LigneJouee>,
    pub pa: u8,
    pub coups: u8,
    pub taux_critique: u8,
    /// Voir [`Attaque::relance`] ; 0 ou 1, à chaque tour.
    pub relance: u8,
    /// Voir [`Attaque::sacrifie`].
    pub sacrifie: bool,
}

/// Ce qu'une invocation joue dans son tour, en clair : « Brise Automnale ×1, un
/// tour sur 2 ».
pub fn en_clair(tour: &[AttaqueJouee]) -> String {
    tour.iter()
        .map(|a| match a.relance {
            r if r >= 2 => format!("{} ×{}, un tour sur {r}", a.nom, a.coups),
            _ => format!("{} ×{}", a.nom, a.coups),
        })
        .collect::<Vec<_>>()
        .join(" et ")
}

/// La part d'un lancer que chaque tour de l'invocation porte, en pour cent,
/// quand l'attaque a une relance : ses lancers sur les `vie` tours qu'elle
/// joue (au premier tour, puis tous les `relance` tours), répartis sur ces
/// tours. Sans durée de vie connue, un lancer tous les `relance` tours.
pub fn part_par_tour(relance: u8, vie: Option<u8>) -> u32 {
    let r = u32::from(relance.max(1));
    match vie {
        Some(v) if v > 0 => {
            let v = u32::from(v);
            (100 * v.div_ceil(r) + v / 2) / v
        }
        _ => 100 / r,
    }
}

/// Le palier d'Évolution qu'un groupe exige de la tourelle du Steamer qui le
/// lance : 1 sans condition (« Lanceur sans Évolution II »), 2 « Lanceur sous
/// Évolution II », 3 « Lanceur sous Évolution III ». `None` pour un groupe
/// sous un autre état, que la rotation ne suit pas.
fn palier_du_groupe(cibles: &str) -> Option<u8> {
    let mut palier = 1;
    for c in cibles.split(" - ").skip(1).filter(|c| c.contains(" sous ")) {
        match c.trim() {
            "Lanceur sous Évolution II" => palier = 2,
            "Lanceur sous Évolution III" => palier = 3,
            _ => return None,
        }
    }
    Some(palier)
}

/// Le groupe touche-t-il un monstre ennemi, au palier d'Évolution donné ? Sa
/// cible nomme l'ennemi (« Ennemi », « Tous sauf lanceur », « Monstre ennemi »),
/// et aucune condition n'exige un autre état que ce palier (« Cible sous
/// Terre ») : la rotation n'en suit pas sur l'invocation. Une cible d'alliés ou
/// d'invocations seules ne compte pas : l'Accrocs du Roquet fait 71-73 aux
/// invocations, 24-26 aux monstres.
fn touche_un_monstre_au_palier(cibles: &str, palier: u8) -> bool {
    let qui = cibles.split(" - ").next().unwrap_or("");
    let ennemi = qui.contains("Ennemi") || qui.contains("Tous") || qui.contains("Monstre ennemi");
    ennemi && palier_du_groupe(cibles) == Some(palier)
}

/// Le plus haut palier d'Évolution des attaques de l'invocation : 3 pour les
/// tourelles offensives du Steamer, 1 pour toute autre invocation.
pub fn paliers(rang: &Rang) -> u8 {
    rang.attaques.iter().filter_map(|a| palier_du_groupe(&a.cibles)).max().unwrap_or(1)
}

/// Les attaques d'une invocation, chacune avec toutes ses lignes sur un monstre
/// ennemi, sa limite par tour et ce qu'un lancer vaut en moyenne. Un même sort
/// peut être rangé en plusieurs groupes (l'impact et la fin de tour de
/// Crapogive) : la rotation les joue ensemble, sous une limite commune.
fn attaques(rang: &Rang, profil: &DamageProfile, facteur: i64) -> Vec<(AttaqueJouee, i64, f64)> {
    attaques_au_palier(rang, profil, facteur, 1)
}

/// Les mêmes, au palier d'Évolution donné.
fn attaques_au_palier(rang: &Rang, profil: &DamageProfile, facteur: i64, palier: u8) -> Vec<(AttaqueJouee, i64, f64)> {
    let mut par_nom: Vec<(AttaqueJouee, i64, f64)> = Vec::new();
    for a in rang.attaques.iter().filter(|a| touche_un_monstre_au_palier(&a.cibles, palier)) {
        let Some(normal) = a.effets.iter().find(|e| e.1 == 0 && element_du_code(&e.0, profil).is_some()) else {
            continue;
        };
        let Some((element, _)) = element_du_code(&normal.0, profil) else { continue };
        let borne = |e: &(String, u8, i32, i32, i32, i32)| (e.2, if e.3 == 0 { e.2 } else { e.3 });
        let critique = a.effets.iter().find(|e| e.1 == 1 && e.0 == normal.0).map_or(borne(normal), borne);
        let moyenne = |(lo, hi): (i32, i32)| {
            (coup(lo, element, profil, facteur) + coup(hi, element, profil, facteur)) as f64 / 2.0
        };
        let taux = f64::from(a.critique.unwrap_or(0)) / 100.0;
        let valeur = (1.0 - taux) * moyenne(borne(normal)) + taux * moyenne(critique);
        // Une relance ou un sacrifice : un lancer par tour au plus.
        let une_fois = (a.relance.unwrap_or(0) > 0 || a.sacrifie).then_some(1);
        let limite = [a.lancers_par_tour, a.lancers_par_cible, une_fois]
            .into_iter()
            .flatten()
            .filter(|n| *n > 0)
            .map(i64::from)
            .min()
            .unwrap_or(i64::MAX);
        let ligne = LigneJouee { element, normal: borne(normal), critique };
        match par_nom.iter_mut().find(|(x, _, _)| x.nom == a.nom) {
            Some((x, l, v)) => {
                x.lignes.push(ligne);
                *v += valeur;
                *l = (*l).min(limite);
                if x.pa == 0 {
                    x.pa = a.pa.unwrap_or(0);
                }
                x.relance = x.relance.max(a.relance.unwrap_or(0));
                x.sacrifie |= a.sacrifie;
            }
            None => par_nom.push((
                AttaqueJouee {
                    nom: a.nom.clone(),
                    lignes: vec![ligne],
                    pa: a.pa.unwrap_or(0),
                    coups: 0,
                    taux_critique: a.critique.unwrap_or(0),
                    relance: a.relance.unwrap_or(0),
                    sacrifie: a.sacrifie,
                },
                limite,
                valeur,
            )),
        }
    }
    par_nom
}

/// Une attaque de l'invocation par son nom, jouée une fois : celle que le
/// Martinet de l'Osamodas fait lancer (« Tofu : Bisou Béco »).
pub fn attaque_nommee(rang: &Rang, profil: &DamageProfile, facteur: i64, nom: &str) -> Option<AttaqueJouee> {
    attaques(rang, profil, facteur)
        .into_iter()
        .find(|(a, _, _)| a.nom.trim().eq_ignore_ascii_case(nom.trim()))
        .map(|(a, _, _)| AttaqueJouee { coups: 1, ..a })
}

/// La meilleure combinaison d'attaques pour `pa` PA, chacune dans la limite
/// qui lui reste : une recherche exhaustive, sur une poignée d'attaques.
fn meilleure_combinaison(attaques: &[(AttaqueJouee, i64, f64)], restes: &[i64], pa: i64) -> (f64, Vec<i64>) {
    let Some(((premiere, _, valeur), suite)) = attaques.split_first() else {
        return (0.0, Vec::new());
    };
    let mut meilleur = (f64::MIN, Vec::new());
    let plafond = restes[0].min(pa / i64::from(premiere.pa));
    for n in 0..=plafond.max(0) {
        let (v, mut coups) = meilleure_combinaison(suite, &restes[1..], pa - n * i64::from(premiere.pa));
        let total = v + n as f64 * valeur;
        if total > meilleur.0 + 1e-9 {
            coups.insert(0, n);
            meilleur = (total, coups);
        }
    }
    meilleur
}

/// Le tour d'une invocation : la meilleure combinaison de ses attaques que ses
/// PA et ses limites de lancer permettent. Avec `deja`, ce que `pa` PA de plus
/// ajoutent à ce tour-là, dans les limites qui restent : les +2 PA de Piqûre
/// Motivante.
pub fn tour_de_l_invocation(
    rang: &Rang,
    profil: &DamageProfile,
    facteur: i64,
    pa: i64,
    deja: &[AttaqueJouee],
) -> Vec<AttaqueJouee> {
    tour_au_palier(rang, profil, facteur, pa, deja, 1)
}

/// Le tour d'une tourelle du Steamer à son palier d'Évolution : les attaques
/// de ce palier seules.
pub fn tour_au_palier(
    rang: &Rang,
    profil: &DamageProfile,
    facteur: i64,
    pa: i64,
    deja: &[AttaqueJouee],
    palier: u8,
) -> Vec<AttaqueJouee> {
    let attaques: Vec<_> =
        attaques_au_palier(rang, profil, facteur, palier).into_iter().filter(|(a, _, _)| a.pa > 0).collect();
    let restes: Vec<i64> = attaques
        .iter()
        .map(|(a, limite, _)| {
            limite - deja.iter().filter(|d| d.nom == a.nom).map(|d| i64::from(d.coups)).sum::<i64>()
        })
        .collect();
    let (_, coups) = meilleure_combinaison(&attaques, &restes, pa);
    attaques
        .into_iter()
        .zip(coups)
        .filter(|(_, n)| *n > 0)
        .map(|((a, _, _), n)| AttaqueJouee { coups: n as u8, ..a })
        .collect()
}

/// Les PA de l'invocation à ce rang.
pub fn pa_de(rang: &Rang) -> i64 {
    i64::from(rang.pa.unwrap_or(0).max(0))
}

/// Les invocations d'une classe et les communes, avec les dégâts de chaque
/// attaque pour ce profil : celles qui frappent, et elles seules. Pour chacune,
/// ce que la rotation lui fait jouer à chaque tour (`jeu`, `pa_depenses`), les
/// tours qu'elle vit quand son sort la retire (`vie`), et pour un sort commun,
/// l'autre monstre qu'il peut tirer (`autre`). Une tourelle du Steamer joue selon
/// son palier d'Évolution, que les remarques du calcul détaillent : pas de `jeu`.
pub fn invocations_du_profil(classe: u32, niveau: u32, profil: &DamageProfile) -> serde_json::Value {
    let instantane = crate::solve::snapshot_for(classe).ok();
    let liste: Vec<serde_json::Value> = releve()
        .invocations
        .iter()
        .filter(|i| i.classe == classe || i.classe == 0)
        .filter_map(|i| {
            let r = rang_au_niveau(i, niveau)?;
            let f = facteur(i.classe, r.pi);
            let attaques: Vec<serde_json::Value> = r
                .attaques
                .iter()
                .filter_map(|a| {
                    let normal = a.effets.iter().find(|e| e.1 == 0 && element_du_code(&e.0, profil).is_some())?;
                    let (element, vol) = element_du_code(&normal.0, profil)?;
                    let fourchette = |e: &(String, u8, i32, i32, i32, i32)| {
                        let hi = if e.3 == 0 { e.2 } else { e.3 };
                        [coup(e.2, element, profil, f), coup(hi, element, profil, f)]
                    };
                    let critique = a.effets.iter().find(|e| e.1 == 1 && e.0 == normal.0).map(fourchette);
                    Some(serde_json::json!({
                        "nom": a.nom,
                        "cibles": a.cibles,
                        "element": element,
                        "vol": vol,
                        "normal": fourchette(normal),
                        "critique": critique,
                        "pa": a.pa,
                        "lancers_par_tour": a.lancers_par_tour,
                        "lancers_par_cible": a.lancers_par_cible,
                        "taux_critique": a.critique,
                    }))
                })
                .collect();
            (!attaques.is_empty()).then(|| {
                let tourelle = i.monstre.is_some_and(|m| crate::solve::TOURELLES_DU_STEAMER.contains(&m));
                let tour = if tourelle { Vec::new() } else { tour_de_l_invocation(r, profil, f, pa_de(r), &[]) };
                let vie = if tour.iter().any(|a| a.sacrifie) {
                    Some(1)
                } else {
                    i.monstre.zip(instantane.as_ref()).and_then(|(m, s)| crate::solve::duree_de_vie(s, i.sort, m))
                };
                let autre = i.autre.as_ref().and_then(|a| {
                    let entree = releve().invocations.iter().find(|v| v.classe == 0 && v.monstre == Some(a.monstre))?;
                    let rang = rang_au_niveau(entree, niveau)?;
                    Some(serde_json::json!({
                        "chance": a.chance,
                        "invocation": rang.nom.clone().unwrap_or_else(|| entree.nom.clone()),
                    }))
                });
                serde_json::json!({
                    "sort": i.sort,
                    "nom": i.nom,
                    "invocation": r.nom.clone().unwrap_or_else(|| i.nom.clone()),
                    "commune": i.classe == 0,
                    "rang": r.rang,
                    "facteur": f,
                    "pa": r.pa,
                    "pm": r.pm,
                    "attaques": attaques,
                    "jeu": (!tour.is_empty())
                        .then(|| en_clair(&tour)),
                    "pa_depenses": tour.iter().map(|a| i64::from(a.pa) * i64::from(a.coups)).sum::<i64>(),
                    "vie": vie,
                    "autre": autre,
                })
            })
        })
        .collect();
    serde_json::Value::Array(liste)
}

/// Pour l'interface : les invocations du build, attaque par attaque.
pub fn invocations_json(build: &dofus_build::BuildInput) -> Result<String, String> {
    let build = build.normalise();
    let resolu = crate::solve::resolve_build(&build)?;
    Ok(serde_json::json!({
        "classe": build.class,
        "invocations": invocations_du_profil(build.class, build.level, &resolu.profile),
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dofus_damage::ElementStats;

    fn profil(carac_air: i32, puissance: i32, dommages_air: i32) -> DamageProfile {
        let mut elements = [ElementStats { characteristic: 0, flat_damage: 0 }; 5];
        elements[Element::Air.index()] = ElementStats { characteristic: carac_air, flat_damage: dommages_air };
        DamageProfile { power: puissance, elements, flat_crit_damage: 100, ..Default::default() }
    }

    /// La formule sur des valeurs calculées à la main. 800 d'Agilité, 150 de
    /// Puissance, 60 de dommages Air, base 21 : au facteur 100, 21 × 10,5 =
    /// 220,5, puis 60, 280. Au facteur 50 de l'Osamodas, la moitié de chacun :
    /// 21 × 5,75 = 120,75, puis 30, 150.
    #[test]
    fn la_formule_de_dofusbook() {
        let p = profil(800, 150, 60);
        assert_eq!(coup(21, Element::Air, &p, 100), 280);
        assert_eq!(coup(21, Element::Air, &p, 50), 150);
        // Une caractéristique négative compte pour zéro.
        assert_eq!(coup(21, Element::Air, &profil(-50, 0, 0), 100), 21);
    }

    /// Le palier fixe la part de l'Osamodas : 50, 75 et 100 %. Hors de
    /// l'Osamodas, tout passe, quel que soit le palier.
    #[test]
    fn le_facteur_de_l_osamodas() {
        assert_eq!((facteur(2, Some(1)), facteur(2, Some(2)), facteur(2, Some(3))), (50, 75, 100));
        assert_eq!(facteur(2, Some(0)), 100);
        assert_eq!(facteur(10, Some(1)), 100);
    }

    /// Le Tofu d'un Osamodas niveau 200 : rang 3, à 50 %. Béco-béco frappe
    /// 21-23 d'Air, 25-28 en critique, sans les 100 de dommages critiques ;
    /// 2 PA, deux fois par cible, 5 % de critique.
    #[test]
    fn le_tofu_de_l_osamodas() {
        let p = profil(800, 150, 60);
        let v = invocations_du_profil(2, 200, &p);
        let tofu = v.as_array().unwrap().iter().find(|i| i["nom"] == "Tofu").expect("le Tofu");
        assert_eq!((tofu["rang"].as_u64(), tofu["facteur"].as_i64()), (Some(3), Some(50)));
        let beco = tofu["attaques"].as_array().unwrap().iter().find(|a| a["nom"] == "Béco-béco").unwrap();
        assert_eq!(beco["normal"], serde_json::json!([coup(21, Element::Air, &p, 50), coup(23, Element::Air, &p, 50)]));
        assert_eq!(beco["critique"], serde_json::json!([coup(25, Element::Air, &p, 50), coup(28, Element::Air, &p, 50)]));
        assert_eq!((beco["pa"].as_u64(), beco["lancers_par_cible"].as_u64(), beco["taux_critique"].as_u64()), (Some(2), Some(2), Some(5)));
        // Bisou Béco vole de la vie ; ses soins aux alliés ne sont pas des dégâts.
        let bisous: Vec<_> = tofu["attaques"].as_array().unwrap().iter().filter(|a| a["nom"] == "Bisou Béco").collect();
        assert_eq!(bisous.len(), 1, "{bisous:?}");
        assert_eq!(bisous[0]["vol"], true);
    }

    /// Une tourelle du Steamer joue son palier de base : ses paliers sous
    /// Évolution II et III dépendent d'un état que la rotation ne suit pas.
    #[test]
    fn la_tourelle_joue_son_palier_de_base() {
        let p = profil(800, 150, 60);
        let harponneuse = releve().invocations.iter().find(|i| i.nom == "Harponneuse").unwrap();
        let rang = rang_au_niveau(harponneuse, 200).unwrap();
        let tour = tour_de_l_invocation(rang, &p, 100, pa_de(rang), &[]);
        assert_eq!(tour.len(), 1, "{tour:?}");
        assert_eq!((tour[0].nom.as_str(), tour[0].lignes[0].normal), ("Espadon", (18, 20)));
    }

    /// Le Tofu remplit ses 4 PA : Bisou Béco (22-25, une fois par cible) vaut
    /// plus qu'un Béco-béco (21-23), et le second coup va à Béco-béco. Avec les
    /// 2 PA de Piqûre Motivante, un second Béco-béco, sa limite étant deux.
    #[test]
    fn le_tofu_remplit_ses_pa() {
        let p = profil(0, 0, 0);
        let tofu = releve().invocations.iter().find(|i| i.nom == "Tofu").unwrap();
        let rang = rang_au_niveau(tofu, 200).unwrap();
        let tour = tour_de_l_invocation(rang, &p, 50, pa_de(rang), &[]);
        let coups: Vec<(&str, u8)> = tour.iter().map(|a| (a.nom.as_str(), a.coups)).collect();
        assert_eq!(coups, [("Béco-béco", 1), ("Bisou Béco", 1)]);
        let en_plus = tour_de_l_invocation(rang, &p, 50, 2, &tour);
        let coups: Vec<(&str, u8)> = en_plus.iter().map(|a| (a.nom.as_str(), a.coups)).collect();
        assert_eq!(coups, [("Béco-béco", 1)]);
        // Rien de plus : les deux attaques sont à leur limite.
        let encore: Vec<AttaqueJouee> = tour.iter().cloned().chain(en_plus).collect();
        assert!(tour_de_l_invocation(rang, &p, 50, 2, &encore).is_empty());
    }

    /// Les groupes d'une même attaque se jouent ensemble, sur un monstre
    /// ennemi sans état : l'Accrocs du Roquet frappe 24-26, pas les 71-73
    /// réservés aux invocations, une fois par cible ; le Rayon Quadramental du
    /// Gardien frappe en Neutre, pas ses groupes d'allié ni ceux sous état ;
    /// Crapogive frappe à l'impact ET en fin de tour, en un lancer.
    #[test]
    fn les_groupes_d_une_attaque_se_jouent_ensemble() {
        let p = profil(0, 0, 0);
        let tour = |nom: &str| {
            let i = releve().invocations.iter().find(|i| i.nom == nom).unwrap();
            let r = rang_au_niveau(i, 200).unwrap();
            tour_de_l_invocation(r, &p, facteur(i.classe, r.pi), pa_de(r), &[])
        };
        let roquet = tour("Lance-roquet");
        assert_eq!(roquet.len(), 1, "{roquet:?}");
        assert_eq!((roquet[0].coups, roquet[0].lignes.len(), roquet[0].lignes[0].normal), (1, 1, (24, 26)));
        let gardien = tour("Gardien Élémentaire");
        assert_eq!((gardien[0].coups, gardien[0].lignes.len()), (1, 1), "{gardien:?}");
        assert_eq!(gardien[0].lignes[0].element, Element::Neutral);
        let crapipaud = tour("Crapipaud");
        let crapogive = crapipaud.iter().find(|a| a.nom == "Crapogive").expect("Crapogive");
        assert_eq!((crapogive.coups, crapogive.lignes.len()), (1, 2), "{crapipaud:?}");
    }

    /// Une attaque que les PA de l'invocation permettent de lancer plusieurs
    /// fois par tour porte une limite écrite, par tour, par cible, une relance
    /// ou un sacrifice : sans
    /// elle, la rotation la joue autant de fois que les PA le permettent.
    #[test]
    fn une_attaque_lancee_plusieurs_fois_porte_sa_limite() {
        let mut sans_limite = Vec::new();
        for i in &releve().invocations {
            for r in &i.rangs {
                let pa = r.pa.unwrap_or(0);
                for a in r.attaques.iter().filter(|a| a.pa.is_some_and(|c| c > 0 && pa >= 2 * i16::from(c))) {
                    let limite = r
                        .attaques
                        .iter()
                        .filter(|b| b.nom == a.nom)
                        .any(|b| {
                            b.lancers_par_tour.unwrap_or(0) > 0
                                || b.lancers_par_cible.unwrap_or(0) > 0
                                || b.relance.unwrap_or(0) > 0
                                || b.sacrifie
                        });
                    if !limite {
                        sans_limite.push(format!("{} rang {} : {}", i.nom, r.rang, a.nom));
                    }
                }
            }
        }
        assert!(sans_limite.is_empty(), "{sans_limite:?}");
    }

    /// Une attaque à relance 2 sur une invocation qui joue trois tours part
    /// deux fois, aux premier et troisième : chaque tour en porte 67 %.
    #[test]
    fn la_relance_se_repartit_sur_la_vie_de_l_invocation() {
        assert_eq!((part_par_tour(2, Some(3)), part_par_tour(2, None), part_par_tour(0, Some(3))), (67, 50, 100));
    }

    /// Une poupée du Sadida reçoit tout, faute d'être Osamodas ; les
    /// invocations communes suivent toutes les classes ; la Lance du Forgelance
    /// n'est pas là.
    #[test]
    fn hors_de_l_osamodas_tout_passe() {
        let p = profil(800, 150, 60);
        let sadida = invocations_du_profil(10, 200, &p);
        assert!(sadida.as_array().unwrap().iter().filter(|i| i["commune"] == false).all(|i| i["facteur"] == 100));
        assert!(sadida.as_array().unwrap().iter().any(|i| i["commune"] == true));
        let forgelance = invocations_du_profil(20, 200, &p);
        assert!(forgelance.as_array().unwrap().iter().all(|i| i["commune"] == true), "{forgelance}");
    }
}
