//! Les effets d'un sort autres que ses dégâts, en phrases : ce que la frise
//! écrit sous son nom, « Retire 3 PM aux ennemis en zone, pendant 1 tour ».
//!
//! Tout vient de l'instantané : les effets du sort et ceux du piège ou du
//! glyphe qu'il pose, avec leurs valeurs, leur masque de cible (camp, état
//! exigé du lanceur ou de la cible), leur zone et leur durée. Un effet dont le
//! sens n'est pas établi ne s'écrit pas, pas plus que les effets d'un sous-sort,
//! dont la donnée ne porte pas les valeurs : tous deux vont dans `manques`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use dofus_ruleset::snapshot::{PlacedEffect, Spell};

/// Les effets d'un sort en phrases, et ce qui n'a pas pu s'écrire.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effets {
    pub phrases: Vec<String>,
    pub manques: Vec<String>,
}

/// Qui l'effet touche, d'après son masque de cible et sa zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Qui {
    Lanceur,
    Cible,
    Ennemis,
    Allie,
    Allies,
    Cibles,
    LanceurEtCible,
    LanceurEtAllie,
    LanceurEtAllies,
    Invocations,
}

impl Qui {
    /// « Repousse la cible ».
    fn le(self) -> &'static str {
        match self {
            Qui::Lanceur => "le lanceur",
            Qui::Cible => "la cible",
            Qui::Ennemis => "les ennemis en zone",
            Qui::Allie => "l'allié ciblé",
            Qui::Allies => "les alliés en zone",
            Qui::Cibles => "les cibles en zone",
            Qui::LanceurEtCible => "le lanceur et la cible",
            Qui::LanceurEtAllie => "le lanceur et l'allié ciblé",
            Qui::LanceurEtAllies => "le lanceur et les alliés en zone",
            Qui::Invocations => "les invocations du lanceur",
        }
    }

    /// « Retire 3 PM à la cible ».
    fn a(self) -> &'static str {
        match self {
            Qui::Lanceur => "au lanceur",
            Qui::Cible => "à la cible",
            Qui::Ennemis => "aux ennemis en zone",
            Qui::Allie => "à l'allié ciblé",
            Qui::Allies => "aux alliés en zone",
            Qui::Cibles => "aux cibles en zone",
            Qui::LanceurEtCible => "au lanceur et à la cible",
            Qui::LanceurEtAllie => "au lanceur et à l'allié ciblé",
            Qui::LanceurEtAllies => "au lanceur et aux alliés en zone",
            Qui::Invocations => "aux invocations du lanceur",
        }
    }

    /// « les dommages finaux du lanceur ».
    fn de(self) -> &'static str {
        match self {
            Qui::Lanceur => "du lanceur",
            Qui::Cible => "de la cible",
            Qui::Ennemis => "des ennemis en zone",
            Qui::Allie => "de l'allié ciblé",
            Qui::Allies => "des alliés en zone",
            Qui::Cibles => "des cibles en zone",
            Qui::LanceurEtCible => "du lanceur et de la cible",
            Qui::LanceurEtAllie => "du lanceur et de l'allié ciblé",
            Qui::LanceurEtAllies => "du lanceur et des alliés en zone",
            Qui::Invocations => "des invocations du lanceur",
        }
    }

    fn pluriel(self) -> bool {
        !matches!(self, Qui::Lanceur | Qui::Cible | Qui::Allie)
    }
}

/// Ce qu'une caractéristique devient : donnée, retirée, volée, rendue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sens {
    Donne,
    Retire,
    Vole,
    Rend,
}

impl Sens {
    fn verbe(self) -> &'static str {
        match self {
            Sens::Donne => "donne",
            Sens::Retire => "retire",
            Sens::Vole => "vole",
            Sens::Rend => "rend",
        }
    }
}

/// Les caractéristiques qu'un effet donne, retire ou vole, et leur nom.
fn caracteristique(id: u32) -> Option<(Sens, &'static str)> {
    use Sens::*;
    Some(match id {
        111 => (Donne, "PA"),
        120 => (Rend, "PA"),
        128 => (Donne, "PM"),
        117 => (Donne, "Portée"),
        138 => (Donne, "Puissance"),
        112 => (Donne, "Dommages"),
        118 => (Donne, "Force"),
        119 => (Donne, "Agilité"),
        123 => (Donne, "Chance"),
        126 => (Donne, "Intelligence"),
        752 => (Donne, "Fuite"),
        753 => (Donne, "Tacle"),
        160 => (Donne, "Esquive PA"),
        161 => (Donne, "Esquive PM"),
        115 => (Donne, "% de Critique"),
        414 => (Donne, "Dommages Poussée"),
        416 => (Donne, "Résistances Poussée"),
        418 => (Donne, "Dommages Critiques"),
        420 => (Donne, "Résistances Critiques"),
        410 => (Donne, "Retrait PA"),
        412 => (Donne, "Retrait PM"),
        178 => (Donne, "Soins"),
        182 => (Donne, "Invocations"),
        2800 => (Donne, "% de Dommages mêlée"),
        2804 => (Donne, "% de Dommages distance"),
        2812 => (Donne, "% de Dommages aux sorts"),
        2803 => (Donne, "% de Résistance mêlée"),
        2807 => (Donne, "% de Résistance distance"),
        210 => (Donne, "% de Résistance Terre"),
        211 => (Donne, "% de Résistance Eau"),
        212 => (Donne, "% de Résistance Air"),
        213 => (Donne, "% de Résistance Feu"),
        214 => (Donne, "% de Résistance Neutre"),
        101 | 168 | 1079 => (Retire, "PA"),
        127 | 169 | 1080 => (Retire, "PM"),
        116 => (Retire, "Portée"),
        186 => (Retire, "Puissance"),
        145 => (Retire, "Dommages"),
        157 => (Retire, "Force"),
        154 => (Retire, "Agilité"),
        152 => (Retire, "Chance"),
        155 => (Retire, "Intelligence"),
        754 => (Retire, "Fuite"),
        755 => (Retire, "Tacle"),
        162 => (Retire, "Esquive PA"),
        163 => (Retire, "Esquive PM"),
        171 => (Retire, "% de Critique"),
        415 => (Retire, "Dommages Poussée"),
        417 => (Retire, "Résistances Poussée"),
        419 => (Retire, "Dommages Critiques"),
        421 => (Retire, "Résistances Critiques"),
        411 => (Retire, "Retrait PA"),
        413 => (Retire, "Retrait PM"),
        2805 => (Retire, "% de Dommages distance"),
        2802 => (Retire, "% de Résistance mêlée"),
        2806 => (Retire, "% de Résistance distance"),
        215 => (Retire, "% de Résistance Terre"),
        216 => (Retire, "% de Résistance Eau"),
        217 => (Retire, "% de Résistance Air"),
        218 => (Retire, "% de Résistance Feu"),
        219 => (Retire, "% de Résistance Neutre"),
        77 => (Vole, "PM"),
        84 => (Vole, "PA"),
        320 => (Vole, "Portée"),
        266 => (Vole, "Chance"),
        267 => (Vole, "Vitalité"),
        268 => (Vole, "Agilité"),
        269 => (Vole, "Intelligence"),
        270 => (Vole, "Sagesse"),
        271 => (Vole, "Force"),
        _ => return None,
    })
}

/// Ce qu'un effet modifie dans un autre sort du lanceur, dont l'identifiant
/// est `dice_num` et la valeur `value` : « Flèche Magique : +6 Portée
/// maximale ».
fn modificateur(id: u32) -> Option<&'static str> {
    Some(match id {
        280 => "Portée minimale",
        281 => "Portée maximale",
        285 => "PA de coût en moins",
        296 => "PA de coût en plus",
        289 => "ligne de vue désactivée",
        290 => "lancer par tour",
        291 => "lancer par cible",
        293 => "dégâts de base",
        1036 => "tour de relance en moins",
        1045 => "tour de relance fixé",
        _ => return None,
    })
}

/// La mécanique du sort, que son nom dit ou que ses dégâts comptent déjà :
/// invocations, poses de piège ou de glyphe (leurs effets s'écrivent depuis
/// `placed_effects`), portails, apparence, dégâts sans ligne. La téléportation
/// (4, et 1101 au tour suivant) aussi : son masque ne dit pas qui se déplace,
/// le lanceur (Transhumance) ou l'invocation (Rempotage).
const MECANIQUE: &[u32] = &[
    4, 82, 85, 86, 87, 88, 89, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 141, 180, 181, 279, 333, 335, 400, 401, 402,
    405, 406, 666, 1008, 1009, 1011, 1013, 1016, 1017, 1026, 1031, 1091, 1092, 1097,
    1101, 1118, 1165, 1181, 1182, 1183, 2018, 2022, 2023, 2184, 2796, 2822, 2828, 2832, 2935,
];

/// Les effets qui lancent un sous-sort : la donnée ne porte que la liste de
/// ses effets, sans leurs valeurs.
const SOUS_SORTS: &[u32] = &[792, 1019, 1160, 1406, 2160, 2794, 2795, 2960, 3792, 3793];

/// Ce qu'un sous-sort porte sans que la frise ait à l'écrire : ses dégâts et
/// vols (lignes déjà comptées), ses soins, ses états, l'effet vide (666) et
/// l'apparence (149).
fn muet_dans_un_sous_sort(id: u32) -> bool {
    matches!(id, 81 | 82 | 85..=100 | 108 | 149 | 202 | 666 | 765 | 950..=952 | 1020 | 1061 | 1078 | 1109 | 2998..=3002)
        || MECANIQUE.contains(&id)
}

/// Les états qui ne servent qu'au calcul du jeu, que le joueur ne lit pas.
const ETATS_INTERNES: &[&str] = &[
    "Hypoglyphe",
    "Glyphes déclenchés",
    "Évolution bloquée",
    "Activation tactique",
    "Activation offensive",
    "Activation défensive",
    "Croisement",
    "Croisement / Piège Magnétique",
    "Blocage Rémission",
];

/// Le nom de chaque état, et s'il renvoie à son explication en jeu
/// (« {{spell,24278,1::Infecté}} »).
fn etats() -> &'static BTreeMap<u32, (String, bool)> {
    static ETATS: OnceLock<BTreeMap<u32, (String, bool)>> = OnceLock::new();
    ETATS.get_or_init(|| {
        let brut: BTreeMap<String, serde_json::Value> =
            serde_json::from_str(crate::donnees::ETATS).unwrap_or_default();
        brut.into_iter()
            .filter_map(|(id, v)| {
                let nom = v["nom"].as_str()?;
                let lie = nom.starts_with("{{spell,");
                let nom = match nom.split_once("::") {
                    Some((_, reste)) if lie => reste.trim_end_matches("}}"),
                    _ => nom,
                };
                Some((id.parse().ok()?, (nom.to_string(), lie)))
            })
            .collect()
    })
}

/// Un sous-sort : son nom et les effets de chacun de ses grades.
fn sous_sorts() -> &'static BTreeMap<u32, (String, Vec<Vec<u32>>)> {
    static SOUS: OnceLock<BTreeMap<u32, (String, Vec<Vec<u32>>)>> = OnceLock::new();
    SOUS.get_or_init(|| {
        let brut: BTreeMap<String, serde_json::Value> =
            serde_json::from_str(crate::donnees::SOUS_SORTS).unwrap_or_default();
        brut.into_iter()
            .filter_map(|(id, v)| {
                let grades = v["grades"]
                    .as_array()?
                    .iter()
                    .map(|g| {
                        g["effets"]
                            .as_array()
                            .map(|e| e.iter().filter_map(|x| x.as_u64()).map(|x| x as u32).collect())
                            .unwrap_or_default()
                    })
                    .collect();
                Some((id.parse().ok()?, (v["nom"].as_str().unwrap_or("").to_string(), grades)))
            })
            .collect()
    })
}

/// Ce qu'un effet de sous-sort fait, en un mot, pour la liste des manques.
fn libelle(id: u32) -> &'static str {
    match id {
        5 | 6 | 8 | 783 | 1041 | 1042 | 1099..=1106 => "un déplacement",
        1075 => "une réduction de durée",
        776 => "de l'érosion",
        1159 => "une réduction des soins reçus",
        1163 => "des dommages subis",
        1171 | 1172 => "des dommages finaux",
        2971 => "des soins finaux",
        1223 => "un renvoi de dommages",
        786 | 2973 => "un soin sur les dommages",
        _ => "un effet",
    }
}

/// Un état que le joueur ne lit pas : entre parenthèses, de la liste des
/// états internes, ou au nom du sort sans renvoyer à une explication.
fn etat_interne(nom: &str, lie: bool, sort: &str) -> bool {
    nom.contains('(') || ETATS_INTERNES.contains(&nom) || (!lie && nom.starts_with(sort))
}

/// Les états qui empêchent un déplacement : « cible sans Pesanteur » sur une
/// poussée ne dit rien de plus que la règle du jeu.
const IMMUNITES: &[&str] = &["Pesanteur", "Indéplaçable", "Enraciné", "Inébranlable"];

/// Ce que dit le masque de cible : qui l'effet touche, ce qu'il exige
/// (« lanceur sous Saoul », « cible sans Feuillu », « projeté dans un
/// portail »), et un état exigé que la liste des états ne nomme pas. Une
/// condition sur un état interne ou une immunité de déplacement ne s'écrit pas.
struct Masque {
    qui: Qui,
    conditions: Vec<String>,
    etat_sans_nom: Option<u32>,
}

fn lire_masque(masque: Option<&str>, en_zone: bool, sort: &str) -> Masque {
    let (mut ennemis, mut allies, mut lanceur, mut monstres) = (false, false, false, false);
    let mut conditions = Vec::new();
    let mut etat_sans_nom = None;
    let mut etat = |id: u32, sans: bool| match etats().get(&id) {
        Some((nom, lie)) if etat_interne(nom, *lie, sort) => None,
        Some((nom, _)) if sans && IMMUNITES.contains(&nom.as_str()) => None,
        Some((nom, _)) => Some(nom.as_str()),
        None => {
            etat_sans_nom = Some(id);
            None
        }
    };
    for jeton in masque.unwrap_or("").split(',').map(str::trim).filter(|j| !j.is_empty()) {
        let (du_lanceur, j) = match jeton.strip_prefix('*') {
            Some(reste) => (true, reste),
            None => (false, jeton),
        };
        let mut lettres = j.chars();
        let Some(lettre) = lettres.next() else { continue };
        let nombre: Option<u32> = lettres.as_str().parse().ok();
        let qui = if du_lanceur { "lanceur" } else { "cible" };
        match (du_lanceur, lettre, nombre) {
            (false, 'A' | 'H', _) => ennemis = true,
            (false, 'a' | 'h' | 'g', _) => allies = true,
            (false, 'C' | 'c', _) => lanceur = true,
            (false, 'F', Some(_)) => monstres = true,
            (_, 'E', Some(e)) => conditions.extend(etat(e, false).map(|n| format!("{qui} sous {n}"))),
            (_, 'e', Some(e)) => conditions.extend(etat(e, true).map(|n| format!("{qui} sans {n}"))),
            (false, 'R', _) => conditions.push("projeté dans un portail".to_string()),
            (false, 'v', Some(p)) => conditions.push(format!("cible à plus de {p} % de vie")),
            (false, 'V', Some(p)) => conditions.push(format!("cible à {p} % de vie ou moins")),
            _ => {}
        }
    }
    let qui = match (lanceur, allies, ennemis) {
        _ if monstres && !ennemis => Qui::Invocations,
        (true, false, false) => Qui::Lanceur,
        (true, true, false) if en_zone => Qui::LanceurEtAllies,
        (true, true, false) => Qui::LanceurEtAllie,
        (true, _, true) if !en_zone => Qui::LanceurEtCible,
        (false, true, false) if en_zone => Qui::Allies,
        (false, true, false) => Qui::Allie,
        (false, false, true) if en_zone => Qui::Ennemis,
        _ if en_zone => Qui::Cibles,
        _ => Qui::Cible,
    };
    Masque { qui, conditions, etat_sans_nom }
}

/// « 3 », « 20 à 22 » : les bornes de l'effet, `None` à zéro.
fn valeur(e: &PlacedEffect) -> Option<String> {
    match (e.dice_num.unwrap_or(0), e.dice_side.unwrap_or(0)) {
        (0, _) => None,
        (n, s) if s > n => Some(format!("{n} à {s}")),
        (n, _) => Some(n.to_string()),
    }
}

fn pluriel(n: i32, mot: &str) -> String {
    format!("{n} {mot}{}", if n.abs() > 1 { "s" } else { "" })
}

/// « , pendant 2 tours », « , dans 1 tour ».
fn duree(e: &PlacedEffect) -> String {
    let mut s = match e.duration.unwrap_or(0) {
        d if d > 0 => format!(", pendant {}", pluriel(d, "tour")),
        d if d < 0 => ", sans limite de durée".to_string(),
        _ => String::new(),
    };
    if let Some(k) = e.delay.filter(|k| *k > 0) {
        s.push_str(&format!(", dans {}", pluriel(k, "tour")));
    }
    s
}

/// Ce qu'un effet devient en phrase.
enum Lecture {
    Caracteristique(Sens, &'static str),
    Modificateur(&'static str),
    Phrase(String),
    Muet,
    SousSort(u32, usize),
    /// Un état que la liste des états ne nomme pas.
    EtatSansNom,
    Inconnu,
}

fn lire(e: &PlacedEffect, qui: Qui, sort: &str) -> Lecture {
    let n = e.dice_num.unwrap_or(0);
    let cases = |n: i32| pluriel(n, "case");
    if let Some((sens, nom)) = caracteristique(e.id) {
        return Lecture::Caracteristique(sens, nom);
    }
    if let Some(nom) = modificateur(e.id) {
        return Lecture::Modificateur(nom);
    }
    if MECANIQUE.contains(&e.id) {
        return Lecture::Muet;
    }
    if SOUS_SORTS.contains(&e.id) {
        let id = if n > 1000 { n } else { e.value.unwrap_or(0) };
        let grade = usize::try_from(e.dice_side.unwrap_or(1).max(1)).unwrap_or(1);
        return Lecture::SousSort(u32::try_from(id).unwrap_or(0), grade);
    }
    let sa = if qui.pluriel() { "leur" } else { "sa" };
    Lecture::Phrase(match e.id {
        5 | 1103 => format!("repousse {} de {}", qui.le(), cases(n)),
        6 => format!("attire {} de {}", qui.le(), cases(n)),
        1041 => format!("éloigne le lanceur de {}", cases(n)),
        1042 => format!("rapproche le lanceur de {}", cases(n)),
        8 => "échange de position avec la cible".to_string(),
        783 => "pousse la cible jusqu'à la case ciblée".to_string(),
        1100 => format!("téléporte {} à {sa} position précédente", qui.le()),
        1099 => format!("téléporte {} à {sa} position de début de tour", qui.le()),
        1104 => "téléporte le lanceur symétriquement par rapport à la cible".to_string(),
        1105 => format!("téléporte {} symétriquement par rapport au lanceur", qui.le()),
        1106 => format!("téléporte {} symétriquement par rapport au centre de la zone", qui.le()),
        50 => "porte la cible".to_string(),
        51 => "jette la cible".to_string(),
        950..=952 => {
            let Some((nom, lie)) = u32::try_from(e.value.unwrap_or(0)).ok().and_then(|v| etats().get(&v)) else {
                return Lecture::EtatSansNom;
            };
            if etat_interne(nom, *lie, sort) {
                return Lecture::Muet;
            }
            match e.id {
                950 => format!("applique l'état {nom} {}", qui.a()),
                951 => format!("retire l'état {nom} {}", qui.de()),
                _ => format!("désactive l'état {nom} {}", qui.de()),
            }
        }
        1075 => format!("réduit de {} la durée des effets sur {}", pluriel(n, "tour"), qui.le()),
        776 => format!("applique {n} % d'érosion {}", qui.a()),
        1159 => format!("réduit de {n} % les soins reçus par {}", qui.le()),
        1163 if n > 100 => format!("augmente de {} % les dommages subis par {}", n - 100, qui.le()),
        1163 if n > 0 && n < 100 => format!("réduit de {} % les dommages subis par {}", 100 - n, qui.le()),
        1163 => return Lecture::Muet,
        1171 => format!("augmente de {n} % les dommages finaux {}", qui.de()),
        1172 => format!("réduit de {n} % les dommages finaux {}", qui.de()),
        2971 => format!("augmente de {n} % les soins finaux {}", qui.de()),
        1078 => format!("augmente de {n} % la Vitalité {}", qui.de()),
        1223 => {
            let son = if qui.pluriel() { "leur" } else { "son" };
            format!("renvoie {n} % des dommages subis par {} à {son} attaquant", qui.le())
        }
        786 => {
            let part = e.value.unwrap_or(0);
            format!("soigne de {part} % des dommages occasionnés ceux qui attaquent {}", qui.le())
        }
        2973 => format!("soigne les alliés de {n} % des dommages occasionnés"),
        1020 | 1039 => format!("applique un bouclier {}", qui.a()),
        81 | 108 | 1109 | 2998..=3002 => format!("soigne {}", qui.le()),
        150 => format!("rend {} invisible{}", qui.le(), if qui.pluriel() { "s" } else { "" }),
        202 => "dévoile les invisibles en zone".to_string(),
        781 => format!("minimise les effets aléatoires {}", qui.de()),
        1061 => "partage les dommages subis".to_string(),
        765 => format!("intercepte les dommages subis par {}", qui.le()),
        265 => format!("réduit les dommages reçus par {}", qui.le()),
        1048 => "sacrifie une partie de la vie du lanceur".to_string(),
        90 | 2020 => format!("transfère une partie de la vie du lanceur {}", qui.a()),
        1033 => "réduit la Vitalité maximale du lanceur".to_string(),
        _ => return Lecture::Inconnu,
    })
}

/// Ce qu'un sous-sort porte que la frise ne peut pas écrire, faute de valeurs :
/// caractéristiques, déplacements, pourcentages, effets au sens inconnu, sauf
/// ceux que le sort écrit déjà (`deja`), et ceux d'un sous-sort qu'il lance à
/// son tour.
fn manque_du_sous_sort(id: u32, grade: usize, sort: &str, deja: &[u32], profondeur: u8) -> Option<String> {
    let Some((nom, grades)) = sous_sorts().get(&id) else {
        return Some(format!("sous-sort {id}, non relevé"));
    };
    let effets = grades.get(grade - 1).or(grades.last())?;
    let parlants: Vec<String> = effets
        .iter()
        .filter(|&&x| !muet_dans_un_sous_sort(x) && !deja.contains(&x))
        .filter_map(|&x| {
            let vide = PlacedEffect {
                id: x,
                value: None,
                dice_num: None,
                dice_side: None,
                duration: None,
                delay: None,
                target: None,
                zone: None,
            };
            match lire(&vide, Qui::Cible, sort) {
                Lecture::Caracteristique(sens, c) => Some(format!("{} {c}", sens.verbe())),
                Lecture::Modificateur(m) => Some(m.to_string()),
                Lecture::Phrase(_) => Some(libelle(x).to_string()),
                Lecture::Inconnu | Lecture::EtatSansNom => Some(format!("effet {x}")),
                // Son contenu n'est pas relevé : la donnée n'en porte que la
                // liste, et pas l'identifiant du sous-sort imbriqué.
                Lecture::SousSort(..) => None,
                Lecture::Muet => None,
            }
        })
        .collect();
    let _ = profondeur;
    let nom = nom.split_once("::").map_or(nom.as_str(), |(_, n)| n.trim_end_matches("}}"));
    (!parlants.is_empty()).then(|| format!("sous-sort « {nom} » : {}", parlants.join(", ")))
}

/// Une phrase à écrire, ou un groupe qui se complète au fil des effets.
enum Sortie {
    Phrase(String),
    Caracteristiques(usize),
    Modificateurs(usize),
}

/// Les effets du dernier grade d'un sort, en phrases. `nom_de_sort` nomme les
/// sorts que ses modificateurs touchent.
pub fn du_sort(sort: &Spell, nom_de_sort: &dyn Fn(u32) -> Option<String>) -> Effets {
    let Some(niveau) = sort.levels.last() else {
        return Effets::default();
    };
    let mut sorties: Vec<Sortie> = Vec::new();
    // Les caractéristiques d'un même sens, d'une même cible, sous les mêmes
    // conditions et pour la même durée : une seule phrase.
    let mut caracs: Vec<((Sens, Qui, Vec<String>, String), Vec<String>)> = Vec::new();
    let mut modifs: Vec<((&'static str, i32, Vec<String>, String), Vec<String>)> = Vec::new();
    let mut manques = Vec::new();
    let mut sous: Vec<(u32, usize)> = Vec::new();
    let critiques = &niveau.critical_other_effects;
    let effets = niveau
        .other_effects
        .iter()
        .enumerate()
        .map(|(i, e)| (e, critiques.get(i).filter(|c| c.id == e.id)))
        .chain(niveau.placed_effects.iter().map(|e| (e, None)));
    for (e, critique) in effets {
        let en_zone = e.zone.is_some_and(|z| z.cells != Some(1));
        let Masque { qui, conditions, etat_sans_nom } = lire_masque(e.target.as_deref(), en_zone, &sort.name.fr);
        let suite = duree(e);
        let lecture = lire(e, qui, &sort.name.fr);
        // Une condition qu'on ne sait pas nommer : la phrase serait fausse.
        if let (Some(etat), Lecture::Caracteristique(..) | Lecture::Modificateur(_) | Lecture::Phrase(_)) =
            (etat_sans_nom, &lecture)
        {
            manques.push(format!("effet {} sous un état {etat} sans nom", e.id));
            continue;
        }
        match lecture {
            Lecture::Caracteristique(sens, nom) => {
                let Some(v) = valeur(e) else { continue };
                let crit = critique
                    .and_then(valeur)
                    .filter(|c| *c != v)
                    .map_or_else(String::new, |c| format!(" ({c} en coup critique)"));
                let cle = (sens, qui, conditions, suite);
                let texte = format!("{v} {nom}{crit}");
                match caracs.iter_mut().position(|(k, _)| *k == cle) {
                    Some(i) => caracs[i].1.push(texte),
                    None => {
                        sorties.push(Sortie::Caracteristiques(caracs.len()));
                        caracs.push((cle, vec![texte]));
                    }
                }
            }
            Lecture::Modificateur(nom) => {
                let v = e.value.unwrap_or(0);
                let id = u32::try_from(e.dice_num.unwrap_or(0)).unwrap_or(0);
                let cible = if id == sort.id {
                    CE_SORT.to_string()
                } else if let Some(nom) = nom_de_sort(id) {
                    nom
                } else {
                    manques.push(format!("effet {} sur le sort {id}, non relevé", e.id));
                    continue;
                };
                let cle = (nom, v, conditions, suite);
                match modifs.iter_mut().position(|(k, _)| *k == cle) {
                    Some(i) => modifs[i].1.push(cible),
                    None => {
                        sorties.push(Sortie::Modificateurs(modifs.len()));
                        modifs.push((cle, vec![cible]));
                    }
                }
            }
            Lecture::Phrase(texte) => sorties.push(Sortie::Phrase(avec_conditions(&conditions, &texte, &suite))),
            Lecture::SousSort(id, grade) => sous.push((id, grade)),
            Lecture::Inconnu => manques.push(format!("effet {}", e.id)),
            Lecture::EtatSansNom => manques.push(format!("état {} sans nom", e.value.unwrap_or(0))),
            Lecture::Muet => {}
        }
    }
    // Les sous-sorts en dernier : ce que le sort écrit déjà ne manque pas.
    let deja: Vec<u32> = niveau.other_effects.iter().chain(&niveau.placed_effects).map(|e| e.id).collect();
    for (id, grade) in sous {
        manques.extend(manque_du_sous_sort(id, grade, &sort.name.fr, &deja, 0));
    }
    let mut phrases: Vec<String> = Vec::new();
    for s in sorties {
        let phrase = match s {
            Sortie::Phrase(p) => p,
            Sortie::Caracteristiques(i) => {
                let ((sens, qui, conditions, suite), textes) = &caracs[i];
                let texte = format!("{} {} {}", sens.verbe(), et(textes), qui.a());
                avec_conditions(conditions, &texte, suite)
            }
            Sortie::Modificateurs(i) => {
                let ((nom, v, conditions, suite), sorts) = &modifs[i];
                let effet = match *nom {
                    "ligne de vue désactivée" => (*nom).to_string(),
                    "lancer par tour" | "lancer par cible" => {
                        let lancers = if *v > 1 { "lancers" } else { "lancer" };
                        format!("+{v} {}", nom.replacen("lancer", lancers, 1))
                    }
                    "tour de relance fixé" => format!("relance fixée à {}", pluriel(*v, "tour")),
                    "tour de relance en moins" => format!("-{} de relance", pluriel(*v, "tour")),
                    _ => format!("+{v} {nom}"),
                };
                let texte = match sorts.len() {
                    1 if sorts[0] == CE_SORT => format!("{effet} pour ce sort"),
                    1 | 2 => format!("{}\u{a0}: {effet}", et(sorts)),
                    n => format!("{effet} sur {n} sorts"),
                };
                avec_conditions(conditions, &texte, suite)
            }
        };
        let phrase = majuscule(&phrase);
        if !phrases.contains(&phrase) {
            phrases.push(phrase);
        }
    }
    manques.dedup();
    Effets { phrases, manques }
}

/// Un effet du sort, seul, tel que la page des données manquantes le lit : sa
/// phrase, ce qu'il est et qui il touche.
#[derive(Clone, Debug)]
pub(crate) struct EffetLu {
    pub id: u32,
    /// La valeur du dé, les 10 de « 10 % de Dommages mêlée ».
    pub n: i32,
    /// Sa phrase dans la frise, s'il se dit. Sans la condition d'un état que la
    /// liste ne nomme pas : `condition_sans_nom` le dit.
    pub phrase: Option<String>,
    pub condition_sans_nom: bool,
    pub nature: Nature,
    pub lanceur: bool,
    /// La cible ou les ennemis : un effet sans camp précis peut toucher un ennemi.
    pub ennemis: bool,
    pub invocations: bool,
    /// « 70 », « 20 à 22 » : la valeur que porte un effet qui ne se dit pas.
    pub valeur: Option<String>,
    /// « pendant 2 tours », « dans 1 tour », ou rien.
    pub duree: String,
    pub delai: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Nature {
    /// Une caractéristique, avec son verbe : « donne », « retire », « vole »,
    /// « rend ».
    Caracteristique { verbe: &'static str, nom: &'static str },
    /// Ce qu'il change à un sort du lanceur.
    Modificateur(&'static str),
    Phrase,
    /// Un sous-sort, par son nom.
    SousSort(String),
    /// Ce que le nom du sort dit déjà : invocation, pose, portail, téléportation.
    Mecanique,
    Inconnue,
}

/// Le premier effet du dernier grade du sort, ou de ce qu'il pose, que `choisir`
/// retient.
pub(crate) fn lire_un_effet(
    sort: &Spell,
    choisir: &dyn Fn(&PlacedEffect) -> bool,
    nom_de_sort: &dyn Fn(u32) -> Option<String>,
) -> Option<EffetLu> {
    let niveau = sort.levels.last()?;
    let e = niveau.other_effects.iter().chain(&niveau.placed_effects).find(|e| choisir(e))?;
    let id = e.id;
    let en_zone = e.zone.is_some_and(|z| z.cells != Some(1));
    let masque = lire_masque(e.target.as_deref(), en_zone, &sort.name.fr);
    let qui = masque.qui;
    let nature = match lire(e, qui, &sort.name.fr) {
        Lecture::Caracteristique(sens, nom) => Nature::Caracteristique { verbe: sens.verbe(), nom },
        Lecture::Modificateur(nom) => Nature::Modificateur(nom),
        Lecture::Phrase(_) => Nature::Phrase,
        Lecture::SousSort(s, _) => Nature::SousSort(
            sous_sorts()
                .get(&s)
                .map(|(nom, _)| nom.split_once("::").map_or(nom.as_str(), |(_, n)| n.trim_end_matches("}}")).to_string())
                .unwrap_or_default(),
        ),
        Lecture::Muet => Nature::Mecanique,
        Lecture::EtatSansNom | Lecture::Inconnu => Nature::Inconnue,
    };
    // La phrase de la frise, l'effet seul dans le sort, avec le masque donné. Un
    // sort que la classe n'a pas reste « un autre sort ».
    let nommer = |i: u32| nom_de_sort(i).or_else(|| Some("un autre sort".to_string()));
    let phrase_avec = |masque: Option<&str>| {
        let mut seul = sort.clone();
        let n = seul.levels.last_mut()?;
        // Le coup critique change le dé, pas l'effet.
        let meme = |x: &PlacedEffect| x.id == e.id && x.value == e.value;
        let garder = |x: &PlacedEffect| meme(x) && x.dice_num == e.dice_num;
        n.other_effects.retain(garder);
        n.critical_other_effects.retain(meme);
        n.placed_effects.retain(garder);
        for x in n.other_effects.iter_mut().chain(n.placed_effects.iter_mut()) {
            x.target = masque.map(str::to_string);
        }
        du_sort(&seul, &nommer).phrases.into_iter().next()
    };
    let mut phrase = phrase_avec(e.target.as_deref());
    // Une condition sur un état sans nom tait la phrase entière : elle se dit
    // sans cette condition, que la page signale.
    let condition_sans_nom = phrase.is_none() && masque.etat_sans_nom.is_some();
    if let (true, Some(sans)) = (condition_sans_nom, masque.etat_sans_nom) {
        let reste: Vec<&str> = e
            .target
            .as_deref()
            .unwrap_or("")
            .split(',')
            .filter(|j| {
                let j = j.trim().trim_start_matches('*');
                !(j.starts_with(['E', 'e']) && j[1..].parse::<u32>() == Ok(sans))
            })
            .collect();
        phrase = phrase_avec(Some(&reste.join(",")));
    }
    Some(EffetLu {
        id,
        n: e.dice_num.unwrap_or(0),
        phrase,
        condition_sans_nom,
        nature,
        lanceur: matches!(qui, Qui::Lanceur | Qui::LanceurEtCible | Qui::LanceurEtAllie | Qui::LanceurEtAllies),
        ennemis: matches!(qui, Qui::Cible | Qui::Ennemis | Qui::Cibles | Qui::LanceurEtCible),
        invocations: qui == Qui::Invocations,
        valeur: valeur(e),
        duree: duree(e).trim_start_matches(", ").to_string(),
        delai: e.delay.unwrap_or(0),
    })
}

/// Le nom d'un état, et s'il ne sert qu'au calcul du jeu, qui ne l'affiche
/// pas. `None` pour un état que la liste ne nomme pas.
pub(crate) fn etat(id: u32) -> Option<(String, bool)> {
    etats().get(&id).map(|(nom, _)| (nom.clone(), nom.contains('(') || ETATS_INTERNES.contains(&nom.as_str())))
}

/// Un modificateur du sort sur lui-même : « +5 dégâts de base pour ce sort ».
const CE_SORT: &str = "ce sort";

/// « a », « a et b », « a, b et c ».
fn et(mots: &[String]) -> String {
    match mots {
        [] => String::new(),
        [seul] => seul.clone(),
        [debut @ .., dernier] => format!("{} et {dernier}", debut.join(", ")),
    }
}

fn avec_conditions(conditions: &[String], texte: &str, suite: &str) -> String {
    if conditions.is_empty() {
        format!("{texte}{suite}")
    } else {
        format!("{}\u{a0}: {texte}{suite}", conditions.join(", "))
    }
}

fn majuscule(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |p| p.to_uppercase().chain(c).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use dofus_ruleset::snapshot::Snapshot;

    fn sort(classe: u32, nom: &str) -> Spell {
        let s = Snapshot::from_json(crate::donnees::instantane(classe).unwrap()).unwrap();
        s.spells.into_iter().find(|x| x.name.fr == nom).unwrap_or_else(|| panic!("{nom}"))
    }

    fn phrases(classe: u32, nom: &str) -> Vec<String> {
        du_sort(&sort(classe, nom), &|_| None).phrases
    }

    /// Le retrait de PM d'Herbes Folles, avec sa cible, sa zone et sa durée.
    #[test]
    fn herbes_folles_retire_trois_pm_en_zone() {
        assert_eq!(phrases(10, "Herbes Folles"), ["Retire 3 PM aux ennemis en zone, pendant 1 tour"]);
    }

    /// Deux retraits d'une même cible et d'une même durée font une phrase.
    #[test]
    fn retour_du_baton_retire_force_et_fuite_en_une_phrase() {
        assert_eq!(phrases(1, "Retour du Bâton"), ["Retire 100 Force et 25 Fuite à la cible, pendant 3 tours"]);
    }

    /// La condition de lancer du masque précède l'effet : le Pandawa Saoul.
    #[test]
    fn ribote_ne_retire_la_fuite_que_saoul() {
        let p = phrases(12, "Ribote");
        let attendue = "Lanceur sous Saoul\u{a0}: retire 40 Fuite à la cible, pendant 1 tour";
        assert!(p.iter().any(|x| x == attendue), "{p:?}");
    }

    /// Le glyphe de la Barrière : les effets qu'il pose s'écrivent aussi.
    #[test]
    fn barriere_donne_de_la_resistance_distance_en_zone() {
        let p = phrases(1, "Barrière");
        let attendue = "Donne 30 % de Résistance distance aux alliés en zone, pendant 1 tour";
        assert!(p.iter().any(|x| x == attendue), "{p:?}");
    }

    /// La condition nomme l'état : le bouclier de Cerbère tombe sous Forme
    /// Bestiale.
    #[test]
    fn cerbere_ne_protege_que_hors_de_la_forme_bestiale() {
        let p = phrases(18, "Cerbère");
        let attendue = "Lanceur sans Forme Bestiale\u{a0}: applique un bouclier au lanceur, pendant 1 tour";
        assert!(p.iter().any(|x| x == attendue), "{p:?}");
    }

    /// Un effet au sens inconnu ne s'écrit pas : il va dans les manques.
    #[test]
    fn un_effet_inconnu_ne_s_ecrit_pas() {
        let mut s = sort(10, "Herbes Folles");
        let niveau = s.levels.last_mut().unwrap();
        niveau.other_effects[0].id = 99_999;
        let e = du_sort(&s, &|_| None);
        assert!(e.phrases.is_empty(), "{e:?}");
        assert_eq!(e.manques, ["effet 99999"]);
    }
}
