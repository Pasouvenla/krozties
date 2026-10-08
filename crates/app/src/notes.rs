//! Les notes de patch d'Ankama, lues pour la donnée du jeu.
//!
//! `dofus note <fichier>` applique à la donnée les lignes « champ : avant →
//! après » d'une note de sortie et dit ce qu'il n'a pas su appliquer. L'entrée
//! est la note exportée en JSON : ses nœuds dans l'ordre du message, chacun avec
//! sa balise, sa profondeur de liste et son texte propre, sans celui de ses
//! sous-listes. Le texte d'Ankama reste hors du dépôt.
//!
//! Ce module rattache chaque ligne à une classe et à un sort, lit le champ et ses
//! deux valeurs, et dit si le changement reste à appliquer, s'il l'est déjà, ou
//! si la donnée n'a ni l'avant ni l'après. Il n'écrit rien : c'est le mode à
//! blanc, le seul pour une note de bêta.
//!
//! Deux formes de note. En 3.7, une classe est un titre `h3` sous
//! « Équilibrage », ses sorts des lignes de liste et leurs changements des
//! sous-lignes. En 3.6, la classe est une ligne de liste en majuscules
//! (« HUPPERMAGE ») sous « Équilibrage de classes ». Une classe se reconnaît à
//! son nom, d'où qu'il vienne.
//!
//! La donnée ne garde que le plus haut grade : une valeur par grade, « 4 / 5 / 6
//! → 6 / 7 / 8 », se compare par sa dernière ; des dégâts donnés niveau par
//! niveau, par leur niveau le plus haut.

use serde::{Deserialize, Serialize};

use crate::solve::{load_ruleset, CLASSES};
use dofus_ruleset::{BaseBonus, Maybe, Ruleset};

pub mod appliquer;
pub mod beta;
mod json_ordonne;

/// Une note telle que le bot l'exporte.
#[derive(Debug, Clone, Deserialize)]
pub struct Note {
    pub id: String,
    pub titre: String,
    /// Le forum du sujet : « Patch notes » pour une sortie, « Beta … » pour
    /// une bêta, qui ne s'applique jamais.
    #[serde(default)]
    pub forum: String,
    #[serde(default)]
    pub url: String,
    pub noeuds: Vec<Noeud>,
}

/// Un titre, un paragraphe ou une ligne de liste, dans l'ordre du message.
#[derive(Debug, Clone, Deserialize)]
pub struct Noeud {
    pub balise: String,
    pub profondeur: u8,
    pub texte: String,
}

/// Ce qu'une ligne de note change, quand ce module sait le lire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Champ {
    CoutPa,
    Portee,
    PorteeMin,
    PorteeMax,
    LancersParTour,
    LancersParCible,
    Relance,
    Critique,
    LancerEnLigne,
    LancerEnDiagonale,
    LancerEnLigneEtDiagonale,
    LigneDeVue,
    PorteeModifiable,
    /// Les dégâts du sort, « Niveau 179 : 30 à 34 (36 à 41) → … » ou
    /// « Dommages : … ».
    Degats,
    BonusDegatsBase,
}

/// Où en est la donnée pour une ligne.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "etat")]
pub enum Etat {
    /// La donnée a l'avant : le changement reste à appliquer.
    AAppliquer,
    /// La donnée a déjà l'après.
    DejaAJour,
    /// La donnée n'a ni l'un ni l'autre : à regarder de près.
    Ecart { actuel: String },
    /// Des dégâts d'un grade plus bas que le plus haut que la note donne : la
    /// donnée ne garde que celui-là.
    AutreGrade,
    /// Un champ reconnu, mais pas sa valeur, ou pas sur ce sort.
    Illisible,
    /// Une phrase, pas un « avant → après » : à modéliser à la main.
    Prose,
}

/// Une ligne de la note, rattachée à sa classe et, s'il y en a un, à son sort.
#[derive(Debug, Clone, Serialize)]
pub struct Ligne {
    pub classe: u32,
    /// Les sorts de la donnée que la ligne vise : plusieurs quand la note les
    /// réunit (« Cataracte, Lances Telluriques, Tison, Onde Céleste »).
    pub sorts: Vec<String>,
    pub texte: String,
    pub champ: Option<Champ>,
    /// Le niveau d'une ligne de dégâts donnée niveau par niveau.
    pub niveau: Option<u16>,
    pub avant: Option<String>,
    pub apres: Option<String>,
    #[serde(flatten)]
    pub etat: Etat,
}

/// Le texte ramené à ce qui se compare : minuscules, sans accents, une seule
/// sorte d'apostrophe, des espaces simples, sans deux-points final.
pub fn normaliser(texte: &str) -> String {
    let mut sortie = String::with_capacity(texte.len());
    for c in texte.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' | 'á' => sortie.push('a'),
            'é' | 'è' | 'ê' | 'ë' => sortie.push('e'),
            'î' | 'ï' | 'í' => sortie.push('i'),
            'ô' | 'ö' | 'ó' => sortie.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => sortie.push('u'),
            'ç' => sortie.push('c'),
            'œ' => sortie.push_str("oe"),
            'æ' => sortie.push_str("ae"),
            '’' | '‘' | '`' => sortie.push('\''),
            '\u{a0}' | '\u{202f}' => sortie.push(' '),
            c => sortie.push(c),
        }
    }
    let simple = sortie.split_whitespace().collect::<Vec<_>>().join(" ");
    simple.trim_end_matches([':', ' ', '.']).to_string()
}

/// La classe dont ce texte est le nom, ou qu'un titre nomme à sa fin
/// (« Refonte du Crâ »).
fn classe_nommee(texte: &str, titre: bool) -> Option<u32> {
    let t = normaliser(texte);
    CLASSES.iter().find_map(|(id, _, nom)| {
        let n = normaliser(nom);
        (t == n || (titre && t.ends_with(&format!(" {n}")))).then_some(*id)
    })
}

/// Les sorts d'une classe, par nom ramené.
struct Repertoire {
    classe: u32,
    regles: Ruleset,
    noms: Vec<(String, String)>,
}

impl Repertoire {
    fn de(classe: u32, regles: Ruleset) -> Repertoire {
        let noms = regles.spells.iter().map(|s| (normaliser(&s.name.fr), s.id.clone())).collect();
        Repertoire { classe, regles, noms }
    }

    /// Les sorts que ce texte nomme : un seul nom, ou plusieurs réunis par des
    /// virgules, « & » ou « et ». Rien si un seul des noms est inconnu.
    fn sorts_nommes(&self, texte: &str) -> Vec<String> {
        let t = normaliser(texte);
        if let Some((_, id)) = self.noms.iter().find(|(n, _)| *n == t) {
            return vec![id.clone()];
        }
        let morceaux: Vec<&str> = t
            .split([',', '&'])
            .flat_map(|m| m.split(" et "))
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .collect();
        if morceaux.len() < 2 {
            return Vec::new();
        }
        let ids: Vec<String> = morceaux
            .iter()
            .filter_map(|m| self.noms.iter().find(|(n, _)| n == m).map(|(_, id)| id.clone()))
            .collect();
        if ids.len() == morceaux.len() {
            ids
        } else {
            Vec::new()
        }
    }
}

/// Le champ qu'un libellé de note désigne, et le niveau d'une ligne de dégâts
/// donnée niveau par niveau.
fn champ_de(libelle: &str) -> Option<(Champ, Option<u16>)> {
    let l = normaliser(libelle);
    if let Some(n) = l.strip_prefix("niveau ").and_then(|n| n.parse().ok()) {
        return Some((Champ::Degats, Some(n)));
    }
    let champ = match l.as_str() {
        "cout en pa" | "cout" => Champ::CoutPa,
        "portee" => Champ::Portee,
        "portee minimale" | "portee min" => Champ::PorteeMin,
        "portee maximale" | "portee max" => Champ::PorteeMax,
        "lancers par tour" | "nombre de lancers par tour" => Champ::LancersParTour,
        "lancers par cible" | "nombre de lancers par cible" => Champ::LancersParCible,
        "relance" | "intervalle de relance" | "delai de relance" => Champ::Relance,
        "critique" | "coup critique" | "probabilite de coup critique" => Champ::Critique,
        "lancer en ligne" => Champ::LancerEnLigne,
        "lancer en diagonale" => Champ::LancerEnDiagonale,
        "lancer en ligne et en diagonale" => Champ::LancerEnLigneEtDiagonale,
        "ligne de vue" => Champ::LigneDeVue,
        "portee modifiable" => Champ::PorteeModifiable,
        "dommages" | "degats" => Champ::Degats,
        "bonus de degats de base" | "bonus de dommages de base" => Champ::BonusDegatsBase,
        _ => return None,
    };
    Some((champ, None))
}

/// Une valeur, telle qu'une note la donne et que la donnée la porte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Valeur {
    Nombre(i64),
    /// Une portée, du minimum au maximum.
    Paire(i64, i64),
    /// Une fourchette de dégâts, et sa fourchette critique quand elle est dite.
    Degats((i64, i64), Option<(i64, i64)>),
}

impl Valeur {
    fn dire(self) -> String {
        match self {
            Valeur::Nombre(n) => n.to_string(),
            Valeur::Paire(a, b) => format!("{a} à {b}"),
            Valeur::Degats((a, b), None) => format!("{a} à {b}"),
            Valeur::Degats((a, b), Some((c, d))) => format!("{a} à {b} ({c} à {d})"),
        }
    }

    /// La note et la donnée disent-elles la même chose ? Une fourchette
    /// critique que l'une ne dit pas ne compte pas.
    fn concorde(self, donnee: Valeur) -> bool {
        match (self, donnee) {
            (Valeur::Degats(n, c), Valeur::Degats(m, d)) => n == m && (c.is_none() || d.is_none() || c == d),
            (a, b) => a == b,
        }
    }
}

/// Les nombres d'un texte, hors de ses parenthèses.
fn nombres(texte: &str) -> Vec<i64> {
    let mut hors = String::with_capacity(texte.len());
    let mut dedans = 0usize;
    for c in texte.chars() {
        match c {
            '(' => dedans += 1,
            ')' => dedans = dedans.saturating_sub(1),
            c if dedans == 0 => hors.push(c),
            _ => {}
        }
    }
    hors.split(|c: char| !c.is_ascii_digit())
        .filter(|m| !m.is_empty())
        .filter_map(|m| m.parse().ok())
        .collect()
}

/// Une valeur de note pour ce champ, ramenée au plus haut grade : « 4 / 5 / 6 »
/// vaut 6, « 3 tours » 3, « 20 % » 20, « oui » 1 et « non » 0, « 2 à 7 » la
/// portée de 2 à 7, « 30 à 34 (36 à 41) » des dégâts et leur critique.
fn valeur(champ: Champ, texte: &str) -> Option<Valeur> {
    let t = normaliser(texte);
    match champ {
        Champ::Portee => {
            let n = nombres(&t);
            (n.len() >= 2).then(|| Valeur::Paire(n[n.len() - 2], n[n.len() - 1]))
        }
        Champ::Degats => {
            // Le critique est entre parenthèses : « (36 à 41) », « (7 en
            // critique) », « (17 à 19 en CC) ».
            let (normal, critique) = match t.split_once('(') {
                Some((avant, apres)) => (avant, Some(apres)),
                None => (t.as_str(), None),
            };
            let fourchette = |s: &str| {
                let n = nombres(s);
                match n.as_slice() {
                    [a] => Some((*a, *a)),
                    [a, b] => Some((*a, *b)),
                    _ => None,
                }
            };
            let n = fourchette(normal)?;
            let c = critique.and_then(|c| fourchette(c.trim_end_matches(')')));
            Some(Valeur::Degats(n, c))
        }
        _ => match t.as_str() {
            "oui" => Some(Valeur::Nombre(1)),
            "non" => Some(Valeur::Nombre(0)),
            _ => nombres(&t).last().map(|n| Valeur::Nombre(*n)),
        },
    }
}

/// La valeur actuelle d'un champ dans la donnée, au grade le plus haut. Rien
/// quand la donnée ne la porte pas, ou pas d'une seule façon (un sort à
/// plusieurs lignes de dégâts).
fn actuelle(regles: &Ruleset, sort: &str, champ: Champ) -> Option<Valeur> {
    let s = regles.spells.iter().find(|s| s.id == sort)?;
    let oui = |b: bool| Valeur::Nombre(i64::from(b));
    let nombre = |n: u8| Valeur::Nombre(i64::from(n));
    Some(match champ {
        Champ::CoutPa => nombre(s.ap_cost.base),
        Champ::Portee => {
            let (a, b) = s.range?;
            Valeur::Paire(i64::from(a), i64::from(b))
        }
        Champ::PorteeMin => nombre(s.range?.0),
        Champ::PorteeMax => nombre(s.range?.1),
        Champ::LancersParTour => nombre(s.casts_per_turn),
        Champ::LancersParCible => nombre(s.casts_per_target?),
        Champ::Relance => nombre(s.cooldown_turns),
        Champ::Critique => match s.crit.base_rate {
            Maybe::Known(t) => nombre(t),
            Maybe::Unknown(_) => return None,
        },
        Champ::LancerEnLigne => {
            let c = s.cast.as_ref()?;
            oui(c.in_line && !c.in_diagonal)
        }
        Champ::LancerEnDiagonale => {
            let c = s.cast.as_ref()?;
            oui(c.in_diagonal && !c.in_line)
        }
        Champ::LancerEnLigneEtDiagonale => {
            let c = s.cast.as_ref()?;
            oui(c.in_line && c.in_diagonal)
        }
        Champ::LigneDeVue => oui(s.cast.as_ref()?.needs_line_of_sight),
        Champ::PorteeModifiable => oui(s.cast.as_ref()?.range_boostable),
        Champ::Degats => {
            let mut toutes = s.lines.iter().filter_map(|l| match (l.normal, l.critical) {
                (Maybe::Known((a, b)), c) => Some(Valeur::Degats(
                    (i64::from(a), i64::from(b)),
                    match c {
                        Maybe::Known((c, d)) => Some((i64::from(c), i64::from(d))),
                        Maybe::Unknown(_) => None,
                    },
                )),
                _ => None,
            });
            let premiere = toutes.next()?;
            // Plusieurs lignes qui disent la même chose (un sort « dans le
            // meilleur élément ») valent une ; des lignes différentes, aucune.
            if toutes.any(|v| v != premiere) {
                return None;
            }
            premiere
        }
        Champ::BonusDegatsBase => {
            let montants: Vec<i32> = s
                .lines
                .iter()
                .flat_map(|l| l.base_bonus.iter())
                .filter_map(|b| match b {
                    BaseBonus::PerResource { amount, .. }
                    | BaseBonus::PerResourceGated { amount, .. }
                    | BaseBonus::WhileResource { amount, .. }
                    | BaseBonus::PerMpUsed { amount, .. }
                    | BaseBonus::PerExtraTarget { amount, .. }
                    | BaseBonus::PerExtraTargetWhile { amount, .. } => Some(*amount),
                    BaseBonus::Steps { .. } => None,
                })
                .collect();
            match montants.as_slice() {
                [m, reste @ ..] if reste.iter().all(|r| r == m) => Valeur::Nombre(i64::from(*m)),
                _ => return None,
            }
        }
    })
}

/// Une ligne « champ : avant → après », découpée ; l'avant manque quand la
/// note ne donne que la nouvelle valeur.
fn avant_apres(texte: &str) -> Option<(String, Option<String>, String)> {
    let (libelle, reste) = texte.split_once(':')?;
    let (avant, apres) = match reste.split_once('→').or_else(|| reste.split_once("->")) {
        Some((a, b)) => (Some(a.trim().to_string()), b.trim().to_string()),
        None => (None, reste.trim().to_string()),
    };
    Some((libelle.trim().to_string(), avant, apres))
}

/// La note lue : chaque ligne rattachée à une classe de la donnée, et son
/// état. Les lignes hors des sections de classe (monstres, objets, quêtes) ne
/// sont pas rendues.
pub fn lire(note: &Note) -> Vec<Ligne> {
    lire_avec(note, &|c| load_ruleset(c).ok())
}

/// La même lecture, contre des règles chargées autrement que de la donnée
/// compilée : depuis les fichiers d'un dossier `data`, pour vérifier ce
/// qu'une application vient d'y écrire.
pub fn lire_avec(note: &Note, charger: &dyn Fn(u32) -> Option<Ruleset>) -> Vec<Ligne> {
    let mut repertoires: Vec<Repertoire> = Vec::new();
    let mut sortie = Vec::new();
    // La classe en cours, et la profondeur de la ligne qui l'a nommée quand ce
    // n'est pas un titre : une ligne aussi peu profonde la referme.
    let mut classe: Option<(u32, Option<u8>)> = None;
    // Les sorts en cours et la profondeur de leur ligne.
    let mut sorts: Option<(Vec<String>, u8)> = None;
    // Seules les sections d'équilibrage nomment des classes : une liste de
    // corrections de bugs peut citer une classe et ses sorts. Une section de
    // premier rang ouvre la lecture si elle commence par « Équilibrage » ou
    // nomme une classe (« Refonte du Crâ ») ; une note sans section se lit en
    // entier.
    let mut equilibrage = true;
    for n in &note.noeuds {
        if n.balise.starts_with('h') {
            sorts = None;
            let nommee = classe_nommee(&n.texte, true);
            if matches!(n.balise.as_str(), "h1" | "h2") {
                equilibrage = normaliser(&n.texte).starts_with("equilibrage") || nommee.is_some();
            }
            classe = if equilibrage { nommee.map(|c| (c, None)) } else { None };
            continue;
        }
        if !equilibrage {
            continue;
        }
        if let Some(c) = classe_nommee(&n.texte, false) {
            sorts = None;
            classe = Some((c, Some(n.profondeur)));
            continue;
        }
        let Some((id_classe, profondeur_classe)) = classe else {
            continue;
        };
        if profondeur_classe.is_some_and(|p| n.profondeur <= p) {
            classe = None;
            sorts = None;
            continue;
        }
        if !repertoires.iter().any(|r| r.classe == id_classe) {
            if let Some(r) = charger(id_classe) {
                repertoires.push(Repertoire::de(id_classe, r));
            }
        }
        let Some(rep) = repertoires.iter().find(|r| r.classe == id_classe) else {
            continue;
        };
        if sorts.as_ref().is_some_and(|(_, p)| n.profondeur <= *p) {
            sorts = None;
        }
        let nommes = rep.sorts_nommes(&n.texte);
        if !nommes.is_empty() {
            sorts = Some((nommes, n.profondeur));
            continue;
        }
        let vises: Vec<String> = sorts.as_ref().map(|(s, _)| s.clone()).unwrap_or_default();
        sortie.push(ligne(rep, vises, &n.texte));
    }
    marquer_les_autres_grades(&mut sortie);
    sortie
}

/// Des dégâts donnés niveau par niveau : seul le plus haut niveau d'un sort se
/// compare, la donnée ne gardant que ce grade.
fn marquer_les_autres_grades(lignes: &mut [Ligne]) {
    let plus_haut = |sorts: &[String], lignes: &[Ligne]| {
        lignes
            .iter()
            .filter(|l| l.sorts == sorts && l.champ == Some(Champ::Degats))
            .filter_map(|l| l.niveau)
            .max()
    };
    let hauts: Vec<Option<u16>> = lignes.iter().map(|l| plus_haut(&l.sorts, lignes)).collect();
    for (l, haut) in lignes.iter_mut().zip(hauts) {
        if l.champ == Some(Champ::Degats) && l.niveau.is_some() && l.niveau != haut {
            l.etat = Etat::AutreGrade;
        }
    }
}

/// Une ligne de note, son champ, ses valeurs, et ce que la donnée en dit.
fn ligne(rep: &Repertoire, sorts: Vec<String>, texte: &str) -> Ligne {
    let mut l = Ligne {
        classe: rep.classe,
        sorts,
        texte: texte.to_string(),
        champ: None,
        niveau: None,
        avant: None,
        apres: None,
        etat: Etat::Prose,
    };
    let Some((libelle, avant, apres)) = avant_apres(texte) else {
        return l;
    };
    let champ = champ_de(&libelle);
    // Sans flèche, une ligne n'est un changement que pour des dégâts donnés
    // niveau par niveau, la note n'en donnant que la nouvelle valeur. Un
    // en-tête vide, « Dommages : », annonce ses sous-lignes.
    if avant.is_none() && (apres.is_empty() || !champ.is_some_and(|(c, _)| c == Champ::Degats)) {
        return l;
    }
    l.avant = avant.clone();
    l.apres = Some(apres.clone());
    l.champ = champ.map(|(c, _)| c);
    l.niveau = champ.and_then(|(_, n)| n);
    let Some((champ, _)) = champ else {
        l.etat = Etat::Illisible;
        return l;
    };
    let a = avant.as_deref().map(|t| valeur(champ, t));
    let (Some(b), false) = (valeur(champ, &apres), l.sorts.is_empty()) else {
        l.etat = Etat::Illisible;
        return l;
    };
    if a.is_some_and(|v| v.is_none()) {
        l.etat = Etat::Illisible;
        return l;
    }
    let a = a.flatten();
    // Plusieurs sorts réunis : chacun doit être dans le même état.
    let actuelles: Vec<Option<Valeur>> = l.sorts.iter().map(|s| actuelle(&rep.regles, s, champ)).collect();
    l.etat = if actuelles.iter().all(|v| v.is_some_and(|v| b.concorde(v))) {
        Etat::DejaAJour
    } else if a.is_some_and(|a| actuelles.iter().all(|v| v.is_some_and(|v| a.concorde(v)))) {
        Etat::AAppliquer
    } else if actuelles.iter().all(Option::is_some) {
        Etat::Ecart {
            actuel: actuelles.iter().flatten().map(|v| v.dire()).collect::<Vec<_>>().join(", "),
        }
    } else {
        Etat::Illisible
    };
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noeud(balise: &str, profondeur: u8, texte: &str) -> Noeud {
        Noeud { balise: balise.into(), profondeur, texte: texte.into() }
    }

    fn note(noeuds: Vec<Noeud>) -> Note {
        Note { id: "0".into(), titre: "essai".into(), forum: String::new(), url: String::new(), noeuds }
    }

    /// Le Crâ de la donnée : les valeurs actuelles de ses sorts, pour écrire
    /// des lignes d'essai qui disent vrai ou faux à coup sûr.
    fn cra() -> Ruleset {
        load_ruleset(9).unwrap()
    }

    #[test]
    fn normaliser_ramene_casse_accents_et_apostrophes() {
        assert_eq!(normaliser("  Flèche d’Immobilisation : "), "fleche d'immobilisation");
        assert_eq!(normaliser("HUPPERMAGE"), "huppermage");
        assert_eq!(normaliser("Cœur"), "coeur");
    }

    #[test]
    fn une_valeur_se_prend_au_plus_haut_grade() {
        let n = |t| valeur(Champ::CoutPa, t);
        assert_eq!(n("4 / 5 / 6"), Some(Valeur::Nombre(6)));
        assert_eq!(n("3 tours"), Some(Valeur::Nombre(3)));
        assert_eq!(n("20 %"), Some(Valeur::Nombre(20)));
        assert_eq!(n("oui"), Some(Valeur::Nombre(1)));
        assert_eq!(n("non"), Some(Valeur::Nombre(0)));
        assert_eq!(n("aucune"), None);
        // Le cumul entre parenthèses n'est pas la valeur.
        assert_eq!(n("+6 / +7 / +8 (cumul maximal : 1)"), Some(Valeur::Nombre(8)));
        assert_eq!(valeur(Champ::Portee, "2 à 7"), Some(Valeur::Paire(2, 7)));
    }

    #[test]
    fn des_degats_se_lisent_avec_leur_critique() {
        let d = |t| valeur(Champ::Degats, t);
        assert_eq!(d("30 à 34 (36 à 41)"), Some(Valeur::Degats((30, 34), Some((36, 41)))));
        assert_eq!(d("14 à 16 (17 à 19 en CC)"), Some(Valeur::Degats((14, 16), Some((17, 19)))));
        assert_eq!(d("6 (7 en critique)"), Some(Valeur::Degats((6, 6), Some((7, 7)))));
        assert_eq!(d("12 à 14"), Some(Valeur::Degats((12, 14), None)));
        assert_eq!(champ_de("Niveau 179"), Some((Champ::Degats, Some(179))));
    }

    /// La forme 3.7 : la classe en titre, une phrase, puis le sort et ses
    /// changements. Une ligne dont la donnée a l'après est déjà à jour, une
    /// dont elle a l'avant reste à appliquer, une autre fait un écart.
    #[test]
    fn la_forme_37_rattache_le_changement_a_son_sort() {
        let r = cra();
        let s = r.spells.iter().find(|s| s.id == "fleche_de_recul").expect("Flèche de Recul");
        let pa = i64::from(s.ap_cost.base);
        let max = i64::from(s.range.unwrap().1);
        let lignes = lire(&note(vec![
            noeud("h2", 0, "Équilibrage"),
            noeud("h3", 0, "Crâ"),
            noeud("li", 1, "Une phrase d'introduction."),
            noeud("li", 2, &s.name.fr),
            noeud("li", 3, &format!("Coût en PA : {} → {pa}", pa + 1)),
            noeud("li", 3, &format!("Portée maximale : 1 / 2 / {max} → 1 / 2 / {}", max + 1)),
            noeud("li", 3, "Lancers par tour : 7 → 9"),
            noeud("li", 3, "Le sort fait autre chose."),
            noeud("h3", 0, "Monstres"),
            noeud("li", 1, "Portée maximale : 1 → 2"),
        ]));
        let etats: Vec<(&str, &Etat)> = lignes.iter().map(|l| (l.texte.as_str(), &l.etat)).collect();
        assert_eq!(lignes.len(), 5, "{etats:?}");
        assert_eq!(lignes[0].etat, Etat::Prose, "la phrase avant le sort");
        assert!(lignes[0].sorts.is_empty());
        assert_eq!(lignes[1].etat, Etat::DejaAJour, "{etats:?}");
        assert_eq!(lignes[1].sorts, ["fleche_de_recul"]);
        assert_eq!(lignes[2].etat, Etat::AAppliquer, "{etats:?}");
        assert!(matches!(lignes[3].etat, Etat::Ecart { .. }), "{etats:?}");
        assert_eq!(lignes[4].etat, Etat::Prose);
    }

    /// Des dégâts niveau par niveau : le plus haut niveau se compare à la
    /// donnée, les autres sont d'un autre grade. Une ligne sans flèche donne
    /// la nouvelle valeur seulement.
    #[test]
    fn les_degats_du_plus_haut_niveau_se_comparent() {
        let r = cra();
        let s = r
            .spells
            .iter()
            .find(|s| {
                s.lines.iter().filter(|l| matches!(l.normal, Maybe::Known(_))).count() == 1
                    && s.lines.iter().any(|l| matches!(l.critical, Maybe::Known(_)))
            })
            .expect("un sort du Crâ à une seule ligne de dégâts");
        let l = s.lines.iter().find(|l| matches!(l.normal, Maybe::Known(_))).unwrap();
        let (Maybe::Known((a, b)), Maybe::Known((c, d))) = (l.normal, l.critical) else {
            unreachable!()
        };
        let lignes = lire(&note(vec![
            noeud("h3", 0, "Crâ"),
            noeud("li", 1, &s.name.fr),
            noeud("li", 2, "Les dommages sont réduits."),
            noeud("li", 3, "Niveau 1 : 1 à 2 (3 à 4) → 1 à 2 (3 à 4)"),
            noeud("li", 3, &format!("Niveau 200 : {a} à {b} ({c} à {d}) → {} à {} ({} à {})", a - 1, b - 1, c - 1, d - 1)),
            noeud("li", 2, &format!("Dommages : {a} à {b} ({c} à {d} en CC)")),
        ]));
        let etats: Vec<&Etat> = lignes.iter().map(|l| &l.etat).collect();
        assert_eq!(etats, [&Etat::Prose, &Etat::AutreGrade, &Etat::AAppliquer, &Etat::DejaAJour], "{lignes:?}");
        assert_eq!(lignes[2].niveau, Some(200));
    }

    /// La forme 3.6 : la classe en ligne de liste, en majuscules, et la
    /// suivante qui la referme. Plusieurs sorts réunis sur une ligne.
    #[test]
    fn la_forme_36_lit_la_classe_en_ligne_et_les_sorts_reunis() {
        let r = cra();
        let noms: Vec<&str> = r.spells.iter().take(2).map(|s| s.name.fr.as_str()).collect();
        let lignes = lire(&note(vec![
            noeud("h2", 0, "Équilibrage de classes"),
            noeud("li", 1, "CRÂ"),
            noeud("li", 2, &format!("{}, {}", noms[0], noms[1])),
            noeud("li", 3, "Une phrase sur les deux."),
            noeud("li", 1, "IOP"),
            noeud("li", 2, "Un sort qui n'existe pas"),
        ]));
        assert_eq!(lignes[0].classe, 9);
        assert_eq!(lignes[0].sorts.len(), 2, "{lignes:?}");
        assert_eq!(lignes[1].classe, 8, "la ligne IOP ouvre l'Iop : {lignes:?}");
    }

    /// Une classe nommée dans les corrections de bugs n'ouvre rien ; la section
    /// d'équilibrage qui suit se lit normalement.
    #[test]
    fn les_corrections_de_bugs_ne_nomment_pas_de_classe() {
        let r = cra();
        let s = &r.spells[0];
        let lignes = lire(&note(vec![
            noeud("h2", 0, "Corrections de bugs"),
            noeud("h3", 0, "Classes / Compagnons"),
            noeud("li", 1, "Les sorts suivants ne poussent pas dans la bonne direction."),
            noeud("li", 2, "Crâ"),
            noeud("li", 3, &s.name.fr),
            noeud("li", 3, "Une attaque d'invocation"),
            noeud("h2", 0, "Équilibrage"),
            noeud("h3", 0, "Crâ"),
            noeud("li", 1, &s.name.fr),
            noeud("li", 2, "Une phrase sur le sort."),
        ]));
        assert_eq!(lignes.len(), 1, "{lignes:?}");
        assert_eq!(lignes[0].texte, "Une phrase sur le sort.");
        assert_eq!(lignes[0].sorts, [s.id.clone()]);
    }

    #[test]
    fn un_titre_qui_finit_par_une_classe_l_ouvre() {
        assert_eq!(classe_nommee("Refonte du Crâ", true), Some(9));
        assert_eq!(classe_nommee("Refonte du Crâ", false), None);
        assert_eq!(classe_nommee("Classes / Compagnons", true), None);
    }
}
