//! Les données manquantes : ce que la rotation ne compte pas et qu'un joueur
//! peut relever en jeu, classe par classe, puis les objets qu'une note de mise
//! à jour change sans donner leurs chiffres.
//!
//! Tout part des questions ouvertes des règles. Chacune devient un point à
//! relever (une valeur, un effet, un état) ou sort de la liste avec sa raison :
//! elle ne touche que les alliés, dépend du placement, du hasard… Une question
//! qu'aucune règle ne reconnaît est une erreur : elle demande une décision.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use dofus_ruleset::snapshot::{PlacedEffect, Spell};
use dofus_ruleset::SpellDef;
use serde::Serialize;

use crate::effets_secondaires::{self, EffetLu, Nature};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Genre {
    Valeur,
    Effet,
    Etat,
}

/// Un point à relever en jeu.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Point {
    pub genre: Genre,
    /// Ce que la donnée dit, ou ce qu'il faut relever.
    pub texte: String,
    /// La consigne, et ce que la donnée porte.
    pub aide: String,
    /// De quoi retrouver le point dans les règles : le sort, l'effet ou l'état.
    pub reference: String,
}

/// Pourquoi une question sort de la liste.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Raison {
    Allies,
    Placement,
    Hasard,
    Invocation,
    Mecanique,
    Interne,
    SansIncidence,
}

impl Raison {
    fn dire(self) -> &'static str {
        match self {
            Raison::Allies => "Ne touchent que les alliés",
            Raison::Placement => "Dépendent du placement ou des PM",
            Raison::Hasard => "Dépendent du hasard",
            Raison::Invocation => "Dépendent d'une invocation",
            Raison::Mecanique => "Font partie de la mécanique du sort : invocation, pose, portail",
            Raison::Interne => "Tiennent à un état que le jeu n'affiche pas",
            Raison::SansIncidence => "Ne changent pas vos dégâts",
        }
    }
}

/// Ce qu'une question devient.
#[derive(Debug, PartialEq)]
pub enum Lecture {
    Points(Vec<Point>),
    Hors(Raison),
    /// Aucune règle ne la reconnaît.
    Inconnue,
}

/// Les caractéristiques qui, données au lanceur, changent ses dégâts.
const GAINS: &[&str] = &[
    "PA",
    "Puissance",
    "Dommages",
    "Force",
    "Agilité",
    "Chance",
    "Intelligence",
    "% de Critique",
    "Dommages Poussée",
    "Dommages Critiques",
    "% de Dommages mêlée",
    "% de Dommages distance",
    "% de Dommages aux sorts",
];

/// Les modificateurs d'un sort qui changent ce que la rotation peut lancer.
const MODIFICATEURS_UTILES: &[&str] = &[
    "PA de coût en moins",
    "PA de coût en plus",
    "lancer par tour",
    "lancer par cible",
    "dégâts de base",
    "tour de relance en moins",
    "tour de relance fixé",
];

/// Les déplacements : repousser, attirer, échanger, téléporter, porter, jeter.
const DEPLACEMENTS: &[u32] = &[5, 6, 8, 50, 51, 783, 1041, 1042, 1099, 1100, 1101, 1102, 1103, 1104, 1105, 1106];

/// Les états qui empêchent un déplacement : ils ne changent que le placement.
const IMMUNITES: &[&str] = &["Pesanteur", "Indéplaçable", "Enraciné", "Inébranlable"];

/// Les effets qui invoquent : une créature, une copie, une bombe, une créature
/// contrôlable.
const EFFETS_D_INVOCATION: &[u32] = &[180, 181, 1008, 1011];

const RELEVER: &str = "La rotation ne le compte pas. Relevez en jeu sa valeur, sa durée et ce qui le déclenche.";
const SOUS_UN_ETAT: &str = "Il ne joue que sous un état que la donnée ne nomme pas, et la rotation ne le compte \
                            pas. Relevez en jeu cet état, la valeur et la durée.";
const DECRIRE: &str = "Relevez en jeu ce qu'il fait : sa valeur, sa durée, ce qui le déclenche.";

/// Un effet lu dans l'instantané : un point s'il peut changer les dégâts du
/// lanceur, sa raison sinon. `invoque` : le sort invoque une créature.
fn juger(lu: &EffetLu, invoque: bool) -> Result<(), Raison> {
    if lu.nature == Nature::Mecanique {
        return Err(Raison::Mecanique);
    }
    if !lu.lanceur && !lu.ennemis {
        return Err(if lu.invocations { Raison::Invocation } else { Raison::Allies });
    }
    match &lu.nature {
        Nature::Caracteristique { verbe, nom } => {
            let gagne = (lu.lanceur && matches!(*verbe, "donne" | "rend")) || *verbe == "vole";
            let affaiblit = lu.ennemis && matches!(*verbe, "retire" | "vole") && nom.contains("Résistance");
            if (gagne && GAINS.contains(nom)) || affaiblit {
                Ok(())
            } else if matches!(*nom, "PM" | "Portée") {
                Err(Raison::Placement)
            } else {
                Err(Raison::SansIncidence)
            }
        }
        // La relance d'un sort d'invocation se fixe à la mort de la créature,
        // que la rotation ne fait jamais mourir.
        Nature::Modificateur("tour de relance fixé") if invoque => Err(Raison::Invocation),
        Nature::Modificateur(nom) if MODIFICATEURS_UTILES.contains(nom) => Ok(()),
        Nature::Modificateur(_) => Err(Raison::Placement),
        Nature::Phrase if DEPLACEMENTS.contains(&lu.id) => Err(Raison::Placement),
        // Des dommages subis accrus chez l'ennemi, des dommages finaux accrus
        // chez le lanceur.
        Nature::Phrase if (lu.id == 1163 && lu.n > 100 && lu.ennemis) || (lu.id == 1171 && lu.lanceur) => Ok(()),
        Nature::Phrase => Err(Raison::SansIncidence),
        Nature::SousSort(_) | Nature::Inconnue | Nature::Mecanique => Ok(()),
    }
}

fn point_d_effet(lu: Option<&EffetLu>, sort: &str, id: u32) -> Point {
    let quand = |l: &EffetLu| match l.delai {
        1 => ", au tour suivant".to_string(),
        d if d > 1 => format!(", dans {d} tours"),
        _ => String::new(),
    };
    let inconnu = "Un effet que la donnée ne nomme pas".to_string();
    let (texte, aide) = match lu {
        Some(l) if l.phrase.is_some() => (
            l.phrase.clone().unwrap_or_default(),
            if l.condition_sans_nom { SOUS_UN_ETAT } else { RELEVER }.to_string(),
        ),
        Some(l) if matches!(l.nature, Nature::SousSort(_)) => {
            let nom = match &l.nature {
                Nature::SousSort(n) if !n.is_empty() => format!("« {n} »"),
                _ => "un sort que la donnée ne décrit pas".to_string(),
            };
            (
                format!("Lance {nom}{}", quand(l)),
                "La donnée n'en porte pas les valeurs. Relevez en jeu ce qu'il fait.".to_string(),
            )
        }
        Some(EffetLu { valeur: Some(v), duree, .. }) => {
            let duree = if duree.is_empty() { String::new() } else { format!(", {duree}") };
            (inconnu, format!("La donnée porte « {v} »{duree}. {DECRIRE}"))
        }
        _ => (inconnu, DECRIRE.to_string()),
    };
    Point { genre: Genre::Effet, texte, aide, reference: format!("sort {sort}, effet {id}") }
}

/// Un état d'une question « Effets 950/951 », posé ou retiré par le sort.
#[derive(Clone, Debug, PartialEq)]
pub struct EtatLu {
    pub id: u32,
    pub nom: String,
    pub retire: bool,
    /// Les sorts qui le lisent, tels que la question les nomme, puis le nombre
    /// de ceux qu'elle tait (« et 12 autre(s) »).
    pub lu_par: Vec<String>,
    pub autres: u32,
}

/// Les états d'une question « Effets 950/951 : ce sort applique « X » (id),
/// lu par A, B ; retire « Y » (id)… », et sa raison de sortir de la liste
/// pour un état qui ne fait qu'empêcher (incurable, invulnérable…), ou que le
/// jeu n'affiche pas.
pub fn etats_de(question: &str) -> Option<Vec<Result<EtatLu, Raison>>> {
    let reste = question.strip_prefix("Effets 950/951 : ce sort ")?;
    let fin = reste.find(". Ces etats-la").unwrap_or(reste.len());
    let mut sortie = Vec::new();
    for morceau in reste[..fin].split(" ; ") {
        let (retire, morceau) = if let Some(m) = morceau.strip_prefix("applique ") {
            (false, m)
        } else {
            (true, morceau.strip_prefix("retire ")?)
        };
        let (nom_brut, apres) = morceau.strip_prefix('«')?.split_once('»')?;
        let nom_brut = nom_brut.trim();
        let id: u32 = apres.trim_start().strip_prefix('(')?.split_once(')')?.0.parse().ok()?;
        if apres.contains("un drapeau") {
            sortie.push(Err(Raison::SansIncidence));
            continue;
        }
        let (nom, interne) = effets_secondaires::etat(id).unwrap_or_else(|| {
            let nom = nom_brut.split_once("::").map_or(nom_brut, |(_, n)| n.trim_end_matches("}}"));
            (nom.to_string(), false)
        });
        if interne {
            sortie.push(Err(Raison::Interne));
            continue;
        }
        if IMMUNITES.contains(&nom.as_str()) {
            sortie.push(Err(Raison::Placement));
            continue;
        }
        // « lu par A, B et 12 autre(s) » : la liste, puis ce qu'elle tait.
        let (lu_par, autres) = match apres.split_once("lu par ") {
            None => (Vec::new(), 0),
            Some((_, l)) => {
                let (noms, autres) = match l.rsplit_once(" et ") {
                    Some((a, b)) if b.ends_with("autre(s)") => {
                        (a, b.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0))
                    }
                    _ => (l, 0),
                };
                (noms.split(", ").map(str::to_string).collect(), autres)
            }
        };
        sortie.push(Ok(EtatLu { id, nom, retire, lu_par, autres }));
    }
    Some(sortie)
}

/// Une question en dehors des états, par son texte.
pub(crate) fn lire_question(
    question: &str,
    aleatoire: bool,
    effet: &dyn Fn(u32) -> Option<EffetLu>,
    invoque: bool,
    sort: &str,
) -> Lecture {
    if aleatoire {
        return Lecture::Hors(Raison::Hasard);
    }
    if let Some(n) = question.strip_prefix("Aucune limite de lancers par tour") {
        let borne = n.split_once("borne ici a ").and_then(|(_, r)| r.split(|c: char| !c.is_ascii_digit()).next());
        return Lecture::Points(vec![Point {
            genre: Genre::Valeur,
            texte: "Le nombre de lancers par tour".into(),
            aide: match borne {
                Some(b) if !b.is_empty() => format!("La donnée n'en fixe aucun ; Krozties en compte {b} au plus."),
                _ => "La donnée n'en fixe aucun.".into(),
            },
            reference: format!("sort {sort}, lancers par tour"),
        }]);
    }
    if question.contains("augmente la Rage, d'un montant que la donnee ne porte pas") {
        return Lecture::Points(vec![Point {
            genre: Genre::Valeur,
            texte: "La Rage gagnée à chaque lancer".into(),
            aide: "La donnée ne porte pas ce gain, et la rotation ne le compte pas.".into(),
            reference: format!("sort {sort}, Rage"),
        }]);
    }
    if let Some(id) = question
        .strip_prefix("Effet ")
        .and_then(|r| r.split_once(" non modelise. "))
        .and_then(|(n, _)| n.parse::<u32>().ok())
    {
        let lu = effet(id);
        return match lu.as_ref().map(|l| juger(l, invoque)) {
            Some(Err(r)) => Lecture::Hors(r),
            _ => Lecture::Points(vec![point_d_effet(lu.as_ref(), sort, id)]),
        };
    }
    let contient = |mots: &[&str]| mots.iter().any(|m| question.contains(m));
    if contient(&["ALEATOIRE", "au hasard", "pioche aléatoire"]) {
        Lecture::Hors(Raison::Hasard)
    } else if contient(&["ALLIE"]) {
        Lecture::Hors(Raison::Allies)
    } else if contient(&["SES PROPRES effets", "les effets de « Poudre »", "un malus et non un buff", "la Puissance est comptée"]) {
        Lecture::Hors(Raison::SansIncidence)
    } else if contient(&["PAR ENTITE", "PAR PM UTILISE", "ACHETE avec des PM", "portail", "CIBLES EN ZONE autour du lanceur"]) {
        Lecture::Hors(Raison::Placement)
    } else if contient(&["TANT QU'IL EST EN VIE", "BALISE"]) {
        Lecture::Hors(Raison::Invocation)
    } else {
        Lecture::Inconnue
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Carte {
    /// L'identifiant du sort dans les règles, pour sa fiche au survol.
    pub sort: String,
    pub icone: Option<u32>,
    pub nom: String,
    pub points: Vec<Point>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Hors {
    pub raison: &'static str,
    pub sorts: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Classe {
    pub id: u32,
    pub nom: String,
    pub points: usize,
    pub cartes: Vec<Carte>,
    pub hors: Vec<Hors>,
}

/// Un état et les sorts de la classe qui le posent ou le retirent.
struct EtatDeClasse {
    carte: usize,
    lu: EtatLu,
    poses: Vec<String>,
    retires: Vec<String>,
}

fn et(mots: &[String]) -> String {
    match mots {
        [] => String::new(),
        [seul] => seul.clone(),
        [debut @ .., dernier] => format!("{} et {dernier}", debut.join(", ")),
    }
}

/// « A, B, C et 7 autres sorts » : quatre noms au plus.
fn liste(noms: &[String], autres: u32) -> String {
    if autres == 0 && noms.len() <= 4 {
        return et(noms);
    }
    let montres = &noms[..noms.len().min(3)];
    let reste = (noms.len() - montres.len()) as u32 + autres;
    format!("{} et {reste} autres sorts", montres.join(", "))
}

/// Les points d'une classe, et ses questions qui sortent de la liste. Erreur
/// sur une question qu'aucune règle ne reconnaît.
pub fn classe(id: u32, nom: &str, regles: &[SpellDef], instantane: &[Spell]) -> Result<Classe, String> {
    let par_id: BTreeMap<u32, &Spell> = instantane.iter().map(|s| (s.id, s)).collect();
    let nom_de_sort = |i: u32| par_id.get(&i).map(|s| s.name.fr.clone());
    let invoquent: BTreeSet<u32> = crate::invocations::releve().invocations.iter().map(|i| i.sort).collect();
    let mut cartes: Vec<Carte> = Vec::new();
    let mut etats: Vec<EtatDeClasse> = Vec::new();
    let mut hors: BTreeMap<Raison, Vec<String>> = BTreeMap::new();
    for s in regles {
        if s.open_questions.is_empty() {
            continue;
        }
        let snap = s.dofusdb_id.and_then(|d| par_id.get(&d).copied());
        let reference = s.dofusdb_id.map_or_else(|| s.id.clone(), |d| d.to_string());
        let aleatoire = s.open_questions.iter().any(|q| q.contains("ALEATOIRE"));
        // Les créatures contrôlables, que la rotation ne joue pas, ne sont pas
        // dans le relevé des invocations : leur effet d'invocation les dit.
        let invoque = s.dofusdb_id.is_some_and(|d| invoquent.contains(&d))
            || snap.is_some_and(|sp| {
                sp.levels.last().is_some_and(|n| n.effect_ids.iter().any(|e| EFFETS_D_INVOCATION.contains(e)))
            });
        let effet = |e: u32| {
            snap.and_then(|sp| effets_secondaires::lire_un_effet(sp, &|x: &PlacedEffect| x.id == e, &nom_de_sort))
        };
        let mut points = Vec::new();
        let ici = cartes.len();
        for q in &s.open_questions {
            if let Some(lus) = etats_de(q) {
                for l in lus {
                    // L'état posé sur les seuls alliés, ou sur les invocations.
                    let pose = l.as_ref().ok().and_then(|lu| {
                        let v = i32::try_from(lu.id).ok();
                        snap.and_then(|sp| {
                            effets_secondaires::lire_un_effet(sp, &|x: &PlacedEffect| (950..=952).contains(&x.id) && x.value == v, &nom_de_sort)
                        })
                    });
                    let l = match (l, pose) {
                        (Ok(_), Some(p)) if !p.lanceur && !p.ennemis => {
                            Err(if p.invocations { Raison::Invocation } else { Raison::Allies })
                        }
                        (l, _) => l,
                    };
                    match l {
                        Err(r) => hors.entry(r).or_default().push(s.name.fr.clone()),
                        Ok(lu) => match etats.iter_mut().find(|e| e.lu.id == lu.id) {
                            Some(e) => {
                                let liste = if lu.retire { &mut e.retires } else { &mut e.poses };
                                if !liste.contains(&s.name.fr) {
                                    liste.push(s.name.fr.clone());
                                }
                            }
                            None => etats.push(EtatDeClasse {
                                carte: ici,
                                poses: if lu.retire { vec![] } else { vec![s.name.fr.clone()] },
                                retires: if lu.retire { vec![s.name.fr.clone()] } else { vec![] },
                                lu,
                            }),
                        },
                    }
                }
                continue;
            }
            match lire_question(q, aleatoire, &effet, invoque, &reference) {
                Lecture::Points(p) => points.extend(p),
                Lecture::Hors(r) => hors.entry(r).or_default().push(s.name.fr.clone()),
                Lecture::Inconnue => return Err(format!("{nom}, {} : question sans règle : {q}", s.name.fr)),
            }
        }
        cartes.push(Carte { sort: s.id.clone(), icone: s.dofusdb_id, nom: s.name.fr.clone(), points });
    }
    for e in etats {
        let lus = (e.lu.lu_par.clone(), e.lu.autres);
        let mut qui = Vec::new();
        if !e.poses.is_empty() && lus == (e.poses.clone(), 0) && e.retires.is_empty() {
            qui.push(format!("Posé et lu par {}", liste(&e.poses, 0)));
        } else {
            if !e.poses.is_empty() {
                qui.push(format!("Posé par {}", liste(&e.poses, 0)));
            }
            if !e.retires.is_empty() {
                let verbe = if qui.is_empty() { "Retiré" } else { "retiré" };
                qui.push(format!("{verbe} par {}", liste(&e.retires, 0)));
            }
            if !e.lu.lu_par.is_empty() {
                qui.push(format!("lu par {}", liste(&e.lu.lu_par, e.lu.autres)));
            }
        }
        let aide = format!("{}. Son effet n'est pas compté : décrivez ce qu'il change en jeu.", qui.join(", "));
        cartes[e.carte].points.push(Point {
            genre: Genre::Etat,
            texte: format!("L'état « {} »", e.lu.nom),
            aide,
            reference: format!("état {}", e.lu.id),
        });
    }
    for c in &mut cartes {
        c.points.sort_by_key(|p| p.genre as u8);
    }
    cartes.retain(|c| !c.points.is_empty());
    let hors = hors
        .into_iter()
        .map(|(r, mut sorts)| {
            sorts.dedup();
            Hors { raison: r.dire(), sorts }
        })
        .collect();
    Ok(Classe { id, nom: nom.to_string(), points: cartes.iter().map(|c| c.points.len()).sum(), cartes, hors })
}

/// Un objet qu'une note de mise à jour change sans donner ses chiffres.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Objet {
    pub id: u32,
    pub nom: String,
    pub note: String,
}

fn objets_selon(manifeste: &str) -> Vec<Objet> {
    let Ok(m) = serde_json::from_str::<serde_json::Value>(manifeste) else { return Vec::new() };
    let mut objets: Vec<Objet> = Vec::new();
    for note in m["patch_notes"].as_array().into_iter().flatten() {
        let titre = note["titre"].as_str().unwrap_or_default();
        for (id, nom) in note["objets_a_relever"].as_object().into_iter().flatten() {
            let (Ok(id), Some(nom)) = (id.parse::<u32>(), nom.as_str()) else { continue };
            if !objets.iter().any(|o| o.id == id) {
                objets.push(Objet { id, nom: nom.to_string(), note: titre.to_string() });
            }
        }
    }
    objets
}

fn calculer() -> Result<String, String> {
    let mut classes = Vec::new();
    for (id, _, nom) in crate::solve::CLASSES {
        let regles = crate::solve::load_ruleset(*id)?;
        let instantane = crate::solve::snapshot_for(*id)?;
        classes.push(classe(*id, nom, &regles.spells, &instantane.spells)?);
    }
    serde_json::to_string(&serde_json::json!({
        "version_du_jeu": crate::donnees::version_du_jeu(),
        "classes": classes,
        "objets": objets_selon(crate::donnees::VERSION),
    }))
    .map_err(|e| e.to_string())
}

/// La page, calculée une fois : la donnée est embarquée.
pub fn manquantes_json() -> Result<String, String> {
    static PAGE: OnceLock<Result<String, String>> = OnceLock::new();
    PAGE.get_or_init(calculer).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn charger(id: u32) -> Classe {
        let (_, _, nom) = crate::solve::CLASSES.iter().find(|c| c.0 == id).unwrap();
        let regles = crate::solve::load_ruleset(id).unwrap();
        let instantane = crate::solve::snapshot_for(id).unwrap();
        classe(id, nom, &regles.spells, &instantane.spells).unwrap()
    }

    fn points_de(c: &Classe, sort: &str) -> Vec<Point> {
        c.cartes.iter().find(|x| x.nom == sort).map(|x| x.points.clone()).unwrap_or_default()
    }

    fn sortis(c: &Classe, r: Raison) -> Vec<String> {
        c.hors.iter().find(|h| h.raison == r.dire()).map(|h| h.sorts.clone()).unwrap_or_default()
    }

    /// Chaque question des dix-neuf classes trouve sa règle : une nouvelle
    /// question sans règle ne disparaît pas de la page sans qu'on le décide.
    #[test]
    fn toute_question_trouve_sa_regle() {
        let page: serde_json::Value = serde_json::from_str(&manquantes_json().unwrap()).unwrap();
        assert_eq!(page["classes"].as_array().unwrap().len(), crate::solve::CLASSES.len());
    }

    /// Une question qu'aucune règle ne reconnaît arrête la page, au lieu d'en
    /// sortir sans bruit.
    #[test]
    fn une_question_sans_regle_est_une_erreur() {
        let mut regles = crate::solve::load_ruleset(1).unwrap().spells;
        let instantane = crate::solve::snapshot_for(1).unwrap();
        regles[0].open_questions.push("Une question que personne n'a prévue.".into());
        let e = classe(1, "Féca", &regles, &instantane.spells).unwrap_err();
        assert!(e.contains("Une question que personne n'a prévue."), "{e}");
    }

    /// L'état Psychopathe, que posent deux masques et que lisent Carnavalo et
    /// Transfiguration : un seul point, qui nomme les deux sorts.
    #[test]
    fn un_etat_pose_par_deux_sorts_fait_un_seul_point() {
        let p = points_de(&charger(14), "Masque du Psychopathe");
        assert_eq!(p.len(), 1, "{p:#?}");
        assert_eq!((p[0].genre, p[0].texte.as_str()), (Genre::Etat, "L'état « Psychopathe »"));
        assert_eq!(
            p[0].aide,
            "Posé par Masque du Psychopathe et Masque de l'Hystérique, lu par Carnavalo et Transfiguration. \
             Son effet n'est pas compté : décrivez ce qu'il change en jeu."
        );
    }

    /// Un effet non compté se dit comme dans la frise : le Tir Perçant du Crâ.
    #[test]
    fn un_effet_se_dit_comme_la_frise() {
        let p = points_de(&charger(9), "Tir Perçant");
        assert_eq!(p.len(), 1, "{p:#?}");
        assert_eq!(p[0].genre, Genre::Effet);
        assert_eq!(p[0].texte, "Augmente de 15 % les dommages subis par la cible");
        assert_eq!(p[0].reference, "sort 32471, effet 1163");
    }

    /// Ce que les règles comptent désormais quitte la page : les masques du
    /// Zobal, Berserk, Manigance, la Sentinelle, sauf le sous-sort qu'elle lance
    /// deux tours plus tard ; et le bonus des portails, qui dépend du placement.
    #[test]
    fn les_effets_modelises_quittent_la_page() {
        let zobal = charger(14);
        assert!(points_de(&zobal, "Masque du Couard").iter().all(|p| p.genre != Genre::Effet));
        assert!(points_de(&zobal, "Masque du Psychopathe").iter().all(|p| p.genre != Genre::Effet));
        assert!(points_de(&charger(11), "Berserk").is_empty());
        assert!(points_de(&charger(4), "Manigance").is_empty());
        let sentinelle = points_de(&charger(9), "Sentinelle");
        assert_eq!(sentinelle.len(), 1, "{sentinelle:#?}");
        assert_eq!(sentinelle[0].texte, "Lance un sort que la donnée ne décrit pas, dans 2 tours");
        let eliotrope = charger(16);
        assert!(points_de(&eliotrope, "Portail").is_empty() && points_de(&eliotrope, "Errance").is_empty());
        assert!(sortis(&eliotrope, Raison::Placement).contains(&"Portail".to_string()));
    }

    /// La Mise en Garde donne ses caractéristiques sous un état que la liste
    /// ne nomme pas : la phrase se dit sans lui, et la consigne le demande.
    #[test]
    fn une_condition_sans_nom_se_demande() {
        let p = points_de(&charger(1), "Mise en Garde");
        let force = p.iter().find(|x| x.reference.ends_with("effet 118")).unwrap();
        assert_eq!(force.texte, "Donne 200 Force au lanceur, pendant 2 tours");
        assert_eq!(force.aide, SOUS_UN_ETAT);
    }

    /// Le modificateur de la Varappe nomme le sort qu'il touche, et son
    /// identifiant ne passe pas pour une valeur.
    #[test]
    fn un_modificateur_nomme_le_sort_qu_il_touche() {
        let p = points_de(&charger(12), "Varappe");
        assert!(!p.is_empty(), "{p:#?}");
        for x in &p {
            assert!(x.texte.starts_with("Karcham\u{a0}: +") && !x.aide.contains("12787"), "{x:#?}");
            assert_eq!(x.aide, SOUS_UN_ETAT);
        }
    }

    /// Les valeurs que la donnée ne porte pas : la Rage de l'Ouginak, les
    /// lancers par tour des Esprits de l'Osamodas.
    #[test]
    fn les_valeurs_absentes_se_relevent() {
        let p = points_de(&charger(18), "Pelage Protecteur");
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].genre, p[0].texte.as_str()), (Genre::Valeur, "La Rage gagnée à chaque lancer"));
        let osamodas = charger(2);
        let p = points_de(&osamodas, "Esprit Glouton");
        assert_eq!(p.len(), 1, "{p:#?}");
        assert_eq!(p[0].aide, "La donnée n'en fixe aucun ; Krozties en compte 6 au plus.");
        // Sa relance, fixée à la mort du Gobgob, dépend de l'invocation.
        assert!(sortis(&osamodas, Raison::Invocation).contains(&"Esprit Glouton".to_string()), "{osamodas:#?}");
    }

    /// Ce qui ne touche que les alliés, dépend du hasard ou ne change pas les
    /// dégâts sort de la liste : le Bouclier Féca, la Roulette, la Fuite que
    /// Manigance vole.
    #[test]
    fn les_questions_hors_calcul_sortent_de_la_liste() {
        let feca = charger(1);
        assert!(points_de(&feca, "Bouclier Féca").is_empty());
        assert!(sortis(&feca, Raison::Allies).contains(&"Bouclier Féca".to_string()));
        let ecaflip = charger(6);
        assert!(points_de(&ecaflip, "Roulette").is_empty());
        let hasard = sortis(&ecaflip, Raison::Hasard);
        assert!(hasard.contains(&"Roulette".to_string()) && hasard.contains(&"Tromperie".to_string()));
        assert!(sortis(&charger(4), Raison::SansIncidence).contains(&"Manigance".to_string()));
    }

    /// Un état que le jeu n'affiche pas ne se demande pas : Glyphes déclenchés,
    /// de la Mise en Garde.
    #[test]
    fn un_etat_interne_ne_se_demande_pas() {
        let feca = charger(1);
        assert!(points_de(&feca, "Mise en Garde").iter().all(|p| !p.texte.contains("Glyphes")));
        assert!(sortis(&feca, Raison::Interne).contains(&"Mise en Garde".to_string()));
    }

    /// Les états d'une question : nom, posé ou retiré, et les sorts qui les
    /// lisent, « et 12 autre(s) » compris ; un drapeau sort de la liste.
    #[test]
    fn une_question_d_etats_se_decoupe() {
        let q = "Effets 950/951 : ce sort applique « Insoignable » (76), qui porte incurable, un drapeau qui \
                 change ce qu'une cible peut infliger ou subir ; applique « {{spell,24037,1::Sobre}} » (3531), lu \
                 par Absinthe, Alcoshu et 12 autre(s) ; retire « Prohibition » (3536), lu par Liqueur, Ribote. \
                 Ces etats-la portent une mecanique que ce calculateur n'ecrit pas encore : chacun demande une \
                 decision de modelisation.";
        let e = etats_de(q).unwrap();
        assert_eq!(e[0], Err(Raison::SansIncidence));
        let sobre = e[1].clone().unwrap();
        assert_eq!((sobre.id, sobre.nom.as_str(), sobre.retire, sobre.autres), (3531, "Sobre", false, 12));
        assert_eq!(sobre.lu_par, ["Absinthe", "Alcoshu"]);
        let prohibition = e[2].clone().unwrap();
        assert!(prohibition.retire);
        assert_eq!((prohibition.lu_par.as_slice(), prohibition.autres), (&["Liqueur".to_string(), "Ribote".to_string()][..], 0));
    }

    /// Quatre noms au plus, le reste compté.
    #[test]
    fn une_longue_liste_se_resume() {
        let noms: Vec<String> = ["A", "B", "C", "D", "E"].map(String::from).to_vec();
        assert_eq!(liste(&noms[..4], 0), "A, B, C et D");
        assert_eq!(liste(&noms, 0), "A, B, C et 2 autres sorts");
        assert_eq!(liste(&noms[..2], 12), "A, B et 12 autres sorts");
    }

    /// Les objets d'une note, une fois chacun, avec le titre de la note.
    #[test]
    fn les_objets_a_relever_viennent_des_notes() {
        let manifeste = r#"{"patch_notes": [
            {"titre": "Mise à jour 3.7", "objets_a_relever": {"123": "Coiffe du Bouftou"}},
            {"titre": "Mise à jour 3.8", "objets_a_relever": {"123": "Coiffe du Bouftou", "456": "Cape Rebelle"}}
        ]}"#;
        assert_eq!(
            objets_selon(manifeste),
            [
                Objet { id: 123, nom: "Coiffe du Bouftou".into(), note: "Mise à jour 3.7".into() },
                Objet { id: 456, nom: "Cape Rebelle".into(), note: "Mise à jour 3.8".into() },
            ]
        );
        assert!(objets_selon(crate::donnees::VERSION).is_empty());
    }
}
