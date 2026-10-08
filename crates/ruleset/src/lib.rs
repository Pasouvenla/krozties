//! Declarative spell mechanics.
//!
//! # Why this is data and not code
//!
//! The solver's speed comes from this data. Without executing anything, the
//! compiler works out which resources each spell reads and writes, hence which
//! casts commute within a turn (enumerated in one canonical order only), and the
//! domain of every state dimension, which lets the search pack and bound its
//! states. Effects are therefore a closed, typed vocabulary rather than a script:
//! a new mechanic is a new effect variant.
//!
//! # Unknowns are first-class
//!
//! A number the tooltip omits is written [`Maybe::Unknown`], never guessed, and
//! [`Ruleset::data_gaps`] lists every one of them.

#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::fmt;

use dofus_damage::Element;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Unknowns
// ---------------------------------------------------------------------------

/// A value that may not be known yet: the value itself, or the literal `unknown`
/// in YAML, so that "not measured" and "forgotten" do not look the same.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Maybe<T> {
    Known(T),
    Unknown(UnknownMarker),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum UnknownMarker {
    Unknown,
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Maybe<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml_ng::Value::deserialize(deserializer)?;
        if value.as_str() == Some("unknown") {
            return Ok(Maybe::Unknown(UnknownMarker::Unknown));
        }
        T::deserialize(value)
            .map(Maybe::Known)
            .map_err(serde::de::Error::custom)
    }
}

impl<T> Maybe<T> {
    pub fn known(&self) -> Option<&T> {
        match self {
            Maybe::Known(v) => Some(v),
            Maybe::Unknown(_) => None,
        }
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, Maybe::Unknown(_))
    }

    /// Ce que vaut une fourchette absente d'une ligne : inconnue.
    pub fn inconnue() -> Self {
        Maybe::Unknown(UnknownMarker::Unknown)
    }
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Localised {
    pub fr: String,
    pub en: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Caster,
    Target,
}

/// Whether more of a resource is ever worse. `Increasing` lets the search discard
/// a state that is componentwise no better than another; it is validated against
/// a prune-free search.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Monotone {
    Increasing,
    Decreasing,
    #[default]
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceDef {
    pub id: String,
    /// Le nom que le jeu donne à l'état, quand une phrase de la page doit le
    /// citer : KrozZone dit que l'Aiguille frappe « quand la cible perd l'état
    /// Téléfrag ». Absent, la page n'a pas de nom à citer.
    #[serde(default)]
    pub name: Option<Localised>,
    pub scope: Scope,
    /// Domain is `0..=max`.
    pub max: u8,
    /// Le plafond ne dépasse jamais le nombre d'ennemis du scénario, pour un
    /// compteur qui dénombre des ennemis (les cibles peintes par le Pinceau Tribal de
    /// l'Eniripsa).
    #[serde(default)]
    pub capped_by_targets: bool,
    /// À la fin du tour, la valeur de ce compteur passe dans celui-ci, et ce compteur
    /// repart de son défaut. Une chaîne de compteurs fait ainsi expirer chaque pose à
    /// son tour (une rune du Huppermage dure deux tours) ; le dernier de la chaîne
    /// perd sa valeur.
    #[serde(default)]
    pub shifts_into: Option<String>,
    /// Ce que ce compteur perd en fin de tour se retire aussi de ce total-là : le
    /// total porte l'effet, la chaîne fait vieillir chaque cumul (les dommages finaux
    /// du Pacte Bestial, trois tours par invocation sacrifiée, sans limite de cumul).
    /// Un total plutôt que plusieurs compteurs bonifiés, car la table de dégâts se
    /// calcule sur le produit des plafonds.
    #[serde(default)]
    pub drains: Option<String>,
    /// Des tirages au hasard : chaque point de ce compteur est une carte tirée
    /// uniformément parmi ces compteurs-là, invisible à la rotation ; une carte à son
    /// plafond ne s'ajoute pas. Les dégâts comptent la moyenne sur tous les tirages,
    /// et la rotation choisit sans connaître la carte (les pioches de l'Ecaflip).
    #[serde(default)]
    pub random_among: Vec<String>,
    #[serde(default)]
    pub default: u8,
    #[serde(default)]
    pub monotone: Monotone,
    #[serde(default)]
    pub duration: Option<DurationDef>,
    /// Damage this resource deals on its own, while it is present, without any
    /// spell being cast. A poison that ticks at the start of a turn, or a mark
    /// that answers when the target loses something.
    #[serde(default)]
    pub while_present: Vec<StateEffect>,
    /// How this state changes the damage of every spell while it is up, unlike a
    /// line's `base_bonus`, which only touches the declaring spell: a Iop's Puissance
    /// raises everything.
    #[serde(default)]
    pub modifies_damage: Vec<DamageModifier>,
    /// Gained automatically at the start of every turn, capped by `max`: "the
    /// spell's damage grows while it recharges" (Épée du Destin, Colère de Iop), the
    /// spell resetting it on cast.
    #[serde(default)]
    pub gain_per_turn: Option<u8>,
    /// Le compteur ne monte que tant que cet autre état est présent : c'est ce qui
    /// écrit une maturation, déclenchée N tours après un lancer. Le Mot Secret de
    /// l'Eniripsa : le lancer arme un drapeau et remet le compteur à zéro, le compteur
    /// monte d'un cran par tour tant que le drapeau tient, et un relancement ramène à
    /// zéro (effet 406). Une charge différée ne conviendrait pas : le moteur refuse
    /// de relancer un sort dont une charge est en vol.
    #[serde(default)]
    pub gain_per_turn_while: Option<String>,
    /// Ce compteur repart de son défaut au début de chaque tour où cet autre état est
    /// présent, après les effets de début de tour : les invocations du Cortège
    /// Sauvage de l'Osamodas jouent leur tour, puis meurent.
    #[serde(default)]
    pub reset_at_turn_start_while: Option<String>,
    /// Ce compteur perd un cran au début de chaque tour où cet autre état est
    /// présent, après les effets de début de tour : la tourelle de la Surtension du
    /// Steamer joue son tour à l'Évolution III, puis redescend.
    #[serde(default)]
    pub lose_at_turn_start_while: Option<String>,
    /// Ce compteur ne monte qu'une fois par tour, quel que soit le nombre de sorts
    /// qui le donnent (le Chœur Strident de l'Eniripsa). Un budget ne conviendrait
    /// pas : non déclaré, il vaut `u8::MAX`.
    #[serde(default)]
    pub gain_once_per_turn: bool,
    /// Ce compteur est renseigné par le joueur, pas gagné en combat : une quantité
    /// que le solveur ne connaît pas mais qui s'énumère palier par palier (ennemis au
    /// contact, tourelles posées). Il démarre à la valeur déclarée et n'en bouge plus ;
    /// la page en fait un champ. Seulement quand les paliers s'énumèrent : « si la
    /// cible est poussée » reste une question ouverte.
    #[serde(default)]
    pub declared_by_player: bool,
    /// Ce que la page affiche au-dessus du champ, quand le joueur le renseigne.
    #[serde(default)]
    pub player_label: Option<String>,
    /// Des PA donnés au début de chaque tour où l'état est porté (le Pacte Bestial
    /// de l'Osamodas : deux PA pendant trois tours), à distinguer de `ap_bonus`,
    /// remboursé sur le lancer. Compté par charge. Le plafond d'élagage en tient
    /// compte : un budget sous-estimé ferait tomber un sort qu'il fallait garder.
    #[serde(default)]
    pub grants_ap: Option<u8>,
    /// Des bonus qui dépendent de la valeur du compteur : `paliers[v - 1]`
    /// s'applique quand il vaut `v`. Ainsi s'écrivent les effets d'objet qui changent
    /// d'un tour à l'autre (la Surpryz : 100 % de critique au premier tour, 35 au
    /// deuxième, 15 au troisième), posés par `crates/app/src/effets_d_objets.rs`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paliers: Vec<Vec<DamageModifier>>,
    /// Les PA que donne chaque valeur du compteur, comme `paliers` : les
    /// Prycipithon au premier tour, le Diadème de Ganymède les tours pairs, et
    /// le PA qu'il retire les tours impairs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pa_par_valeur: Vec<i8>,
    /// Le compteur reboucle : à `max`, son gain de début de tour le ramène à
    /// 1. La Dofusteuse change de caractéristique chaque tour, sur quatre.
    #[serde(default)]
    pub cyclique: bool,
    /// Les poussées que ce compteur inflige à la cible à chaque tour où il est
    /// présent, en cases, au début du tour (les Bottes du Cul Botté : `[2, 2]`).
    /// Elles ne font de dégâts que si le joueur déclare ses poussées bloquées.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub poussees_par_tour: Vec<u8>,
    /// Gained at the END of every turn, so it is absent on turn one and present
    /// from turn two. That is how "everyone has moved by now" is said: on the
    /// opening turn nobody has a previous position to be sent back to.
    #[serde(default)]
    pub gain_at_turn_end: Option<u8>,

    /// The spell whose effect list carries this state's damage figures: a state
    /// deals damage without being cast.
    #[serde(default)]
    pub dofusdb_source: Option<u32>,
    /// Les états du jeu que le lanceur porte tant que ce compteur vaut au moins un :
    /// le pont entre la condition de lancement de la donnée (`HS=3`, `HS!7`) et ce que
    /// le moteur suit. Un état qu'aucun compteur ne représente, le lanceur ne l'a
    /// jamais : un sort qui l'exige n'est jamais proposé.
    #[serde(default)]
    pub game_states: Vec<u32>,
    /// Les états que le lanceur porte tant que ce compteur vaut zéro : Armé (3360)
    /// tant que la Lance du Forgelance n'est pas plantée, Désarmé (3361) dès qu'elle
    /// l'est.
    #[serde(default)]
    pub game_states_when_zero: Vec<u32>,
    /// Tant que ce compteur vaut au moins un, le lanceur ne lance que les sorts dont
    /// la condition exige l'un de ses `game_states` (drapeau `preventsSpellCast` : le
    /// Pandawa qui porte ne peut que jeter).
    #[serde(default)]
    pub prevents_spell_cast: bool,
    #[serde(default)]
    pub note: Option<String>,
}

/// La condition de lancement d'un sort, telle que la donnée l'écrit
/// (`statesCriterion`) : une formule sur les états du lanceur. `HS=3` : « porte
/// l'état 3 », `HS!7` : « ne porte pas l'état 7 », combinés par `&`, `|` et des
/// parenthèses (`HS=3|(HS=3531&HS!8)` : « Porteur, ou Sobre et pas Porté »).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Critere {
    Etat { etat: u32, present: bool },
    Et(Vec<Critere>),
    Ou(Vec<Critere>),
}

impl Critere {
    /// Lit la formule de la donnée. Toute autre forme que `HS=`, `HS!`, `&`,
    /// `|` et les parenthèses est refusée plutôt que devinée.
    pub fn lire(texte: &str) -> Result<Critere, String> {
        let octets: Vec<char> = texte.chars().filter(|c| !c.is_whitespace()).collect();
        let mut pos = 0;
        let critere = Self::ou(&octets, &mut pos, texte)?;
        if pos != octets.len() {
            return Err(format!("« {texte} » : reste illisible après le caractère {pos}"));
        }
        Ok(critere)
    }

    fn ou(c: &[char], pos: &mut usize, texte: &str) -> Result<Critere, String> {
        let mut termes = vec![Self::et(c, pos, texte)?];
        while c.get(*pos) == Some(&'|') {
            *pos += 1;
            termes.push(Self::et(c, pos, texte)?);
        }
        Ok(if termes.len() == 1 { termes.remove(0) } else { Critere::Ou(termes) })
    }

    fn et(c: &[char], pos: &mut usize, texte: &str) -> Result<Critere, String> {
        let mut termes = vec![Self::atome(c, pos, texte)?];
        while c.get(*pos) == Some(&'&') {
            *pos += 1;
            termes.push(Self::atome(c, pos, texte)?);
        }
        Ok(if termes.len() == 1 { termes.remove(0) } else { Critere::Et(termes) })
    }

    fn atome(c: &[char], pos: &mut usize, texte: &str) -> Result<Critere, String> {
        if c.get(*pos) == Some(&'(') {
            *pos += 1;
            let dedans = Self::ou(c, pos, texte)?;
            if c.get(*pos) != Some(&')') {
                return Err(format!("« {texte} » : parenthèse non fermée"));
            }
            *pos += 1;
            return Ok(dedans);
        }
        if c.get(*pos) != Some(&'H') || c.get(*pos + 1) != Some(&'S') {
            return Err(format!("« {texte} » : seules les conditions d'état HS sont connues"));
        }
        let present = match c.get(*pos + 2) {
            Some('=') => true,
            Some('!') => false,
            _ => return Err(format!("« {texte} » : HS doit être suivi de = ou de !")),
        };
        *pos += 3;
        let debut = *pos;
        while c.get(*pos).is_some_and(char::is_ascii_digit) {
            *pos += 1;
        }
        let etat: u32 = c[debut..*pos]
            .iter()
            .collect::<String>()
            .parse()
            .map_err(|_| format!("« {texte} » : identifiant d'état manquant"))?;
        Ok(Critere::Etat { etat, present })
    }

    /// Les états que la formule cite, sans doublon.
    pub fn etats(&self) -> Vec<u32> {
        let mut v = Vec::new();
        self.collecter(&mut v);
        v.sort_unstable();
        v.dedup();
        v
    }

    fn collecter(&self, v: &mut Vec<u32>) {
        match self {
            Critere::Etat { etat, .. } => v.push(*etat),
            Critere::Et(t) | Critere::Ou(t) => t.iter().for_each(|c| c.collecter(v)),
        }
    }

    /// La formule exige-t-elle quelque part la présence de l'un de ces états ? Le
    /// Karcham (`HS=3|(HS=3531&…)`) se lance en portant comme à jeun : un état qui
    /// interdit de lancer ne le ferme pas.
    pub fn exige_un_de(&self, etats: &[u32]) -> bool {
        match self {
            Critere::Etat { etat, present } => *present && etats.contains(etat),
            Critere::Et(t) | Critere::Ou(t) => t.iter().any(|c| c.exige_un_de(etats)),
        }
    }

    /// La formule tient-elle, sachant quels états le lanceur porte ?
    pub fn tient(&self, porte: &dyn Fn(u32) -> bool) -> bool {
        match self {
            Critere::Etat { etat, present } => porte(*etat) == *present,
            Critere::Et(t) => t.iter().all(|c| c.tient(porte)),
            Critere::Ou(t) => t.iter().any(|c| c.tient(porte)),
        }
    }
}

/// A change a state makes to every damage line while it is present. Three forms,
/// the three stages a buff can feed, not interchangeable.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DamageModifier {
    /// Multiplie les dommages finaux, 120 valant +20 %. Les % de dommages finaux du
    /// lanceur (effet 1171) s'écrivent `finaux: true` et s'additionnent à ceux des
    /// Dofus et de l'équipement ; les dommages subis par la cible (effet 1163) sont un
    /// facteur à part, qui multiplie.
    FinalMultiplier {
        percent: Maybe<u32>,
        #[serde(default)]
        critical_percent: Option<u32>,
        #[serde(default)]
        finaux: bool,
    },
    /// Adds to the characteristic driving each line, inside the first floor.
    Characteristic {
        amount: Maybe<i32>,
        #[serde(default)]
        critical_amount: Option<i32>,
        /// L'élément dont la caractéristique monte ; absent, les cinq (la Puissance). Un
        /// vol en a un : la Fourberie du Sram vole de l'Intelligence, qui ne porte que le
        /// Feu. `earth` emporte le Neutre : les deux tirent de la Force.
        #[serde(default)]
        element: Option<Element>,
        /// Le montant vaut par ennemi touché (la Terre du Milieu du Forgelance : 50 de
        /// Puissance par ennemi). Le nombre de cibles est celui du scénario, connu à la
        /// compilation.
        #[serde(default)]
        per_target: bool,
        /// Le montant vaut par PM dépensé dans le tour (le Tunnel de Fortune de
        /// l'Enutrof : 20 de Puissance par PM), d'après le réglage « PM dépensés ».
        #[serde(default)]
        per_mp_used: bool,
        /// Combien de fois le bonus se cumule au plus, quand il vaut par cible : 4 pour
        /// la Terre du Milieu, soit 200 de Puissance au plus. Absent, il ne se cumule
        /// pas.
        #[serde(default)]
        per_target_cap: Option<u8>,
    },
    /// Adds to the base roll of each line, before any scaling. Not `flat_damage`:
    /// at 650 characteristic and 170 Power the roll is multiplied by 9.2, so a "+30"
    /// here is worth 276.
    BaseDamage {
        amount: Maybe<i32>,
        #[serde(default)]
        critical_amount: Option<i32>,
    },
    /// Dommages fixes, ajoutés après l'arrondi caractéristique, au même étage que les
    /// « Dommages » de l'équipement (effet 112).
    FlatDamage {
        amount: Maybe<i32>,
        #[serde(default)]
        critical_amount: Option<i32>,
    },
    /// Points de taux de coup critique, en pourcentage entier : 7 vaut +7 % (la
    /// Nervosité du Sacrieur, effet 115). Le seul modificateur qui touche au tirage ;
    /// sans effet sur un sort `can_crit: false`.
    CriticalRate {
        percent: Maybe<i32>,
        /// Le taux que donne un lancer critique du sort qui pose l'état, quand la donnée
        /// en porte un autre (le Yams de l'Ecaflip : 3 à 18 %, 21 fixes en critique).
        /// Choisi au seuil de [`CRITICAL_EFFECT_THRESHOLD`].
        #[serde(default)]
        critical_percent: Option<i32>,
    },
    /// Dommages Critiques fixes, ajoutés au bonus critique du build (le Kraps de
    /// l'Ecaflip, effet 418, bâti pour valoir les dégâts du sort).
    CriticalDamage {
        amount: Maybe<i32>,
        /// Sur un lancer critique (34 pour le Kraps), choisi au seuil de
        /// [`CRITICAL_EFFECT_THRESHOLD`].
        #[serde(default)]
        critical_amount: Option<i32>,
    },
    /// Résistance Critique retirée à la cible (la Griffe Joueuse de l'Ecaflip, effet
    /// 421) : elle se soustrait du bonus critique de l'attaquant, donc en retirer en
    /// rend.
    CriticalResistance {
        amount: Maybe<i32>,
        /// Sur un lancer critique : 42 pour la Griffe Joueuse, dans la donnée.
        /// Choisi au seuil de [`CRITICAL_EFFECT_THRESHOLD`].
        #[serde(default)]
        critical_amount: Option<i32>,
    },
    /// Dommages Poussée (effet 414), ajoutés à ceux du build dans la formule
    /// `(niveau / 2 + 32 + Dommages Poussée) × cases / 4` : sans effet tant que les
    /// poussées ne sont pas déclarées bloquées (la Puissance du Iop : 120, 140 en
    /// critique).
    PushDamage {
        amount: Maybe<i32>,
        #[serde(default)]
        critical_amount: Option<i32>,
    },
    /// Des % de dommages d'un domaine (mêlée ou distance, sorts ou armes), valables
    /// pour les lancers de ce domaine (la Pryximite en mêlée). Comptés comme un
    /// facteur à part au même arrondi, quand le jeu les ajoute aux % du même domaine
    /// de l'équipement : 2 % sur 10 % font 1,12 en jeu et 1,122 ici.
    DomainPercent {
        #[serde(default)]
        delivery: Option<dofus_damage::Delivery>,
        #[serde(default)]
        reach: Option<dofus_damage::Reach>,
        percent: i32,
    },
}

/// Les sorts que les notes de sortie appliquées ont changés depuis le relevé,
/// lus dans le manifeste (`data/version.json`) : `sorts` et `sorts_a_la_main`.
/// Une comparaison avec une source restée à la version du relevé les écarte.
pub fn sorts_changes_depuis_le_releve(manifeste: &str) -> BTreeSet<u32> {
    let v: serde_json::Value = serde_json::from_str(manifeste).unwrap_or_default();
    v["patch_notes"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|n| ["sorts", "sorts_a_la_main"].map(|cle| n[cle].as_array().cloned().unwrap_or_default()))
        .flatten()
        .filter_map(|id| id.as_u64().and_then(|i| u32::try_from(i).ok()))
        .collect()
}

/// Above this critical rate, a spell's critical figures are used for its
/// single-magnitude effects (such as the Puissance it grants): a cast either crits
/// or not, and averaging would invent a bonus. Damage lines carry both ranges and
/// are weighted by the actual rate.
pub const CRITICAL_EFFECT_THRESHOLD: i32 = 78;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateEffect {
    #[serde(flatten)]
    pub trigger: StateTrigger,
    pub lines: Vec<LineDef>,
    /// Fires at most once per turn however many times the event occurs.
    #[serde(default)]
    pub once_per_turn: bool,
    /// Le taux de critique de base de ces dégâts, quand la donnée en donne un autre
    /// que celui du sort qui pose l'état. Absent, l'état critique comme son sort (une
    /// hypothèse) ; le glyphe-aura du Pâturage porte 1 quand le Pâturage porte 10. Le
    /// bonus critique du build s'ajoute dans les deux cas.
    #[serde(default)]
    pub crit_base_percent: Option<u8>,
    /// Condition lue au moment où la détente partirait : fausse, elle ne part pas et
    /// l'état reste. Le coup de début de tour de la Flibuste et de la Noa représente
    /// une poussée infligée par quelqu'un d'autre : il ne part que si le joueur le
    /// déclare ; leurs propres poussées passent par [`StateTrigger::PushDamage`].
    #[serde(default)]
    pub requires: Option<Condition>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "trigger", rename_all = "snake_case")]
pub enum StateTrigger {
    /// Before anything is cast.
    TurnStart,
    /// When the holder loses the named resource.
    ResourceConsumed { resource: String },
    /// When any spell carrying this tag resolves on the holder: the Enutrof's three
    /// marks (Éboulement on a Range removal, Monnaie Sonnante on an AP removal
    /// attempt, Orpaillage on an MP one). The attempt counts, not its success. A tag
    /// rather than a spell list: the answering state has no business knowing which
    /// spells remove AP.
    SpellTagged {
        tag: String,
        /// Charges spent per trigger. A mark with four charges vanishes on the
        /// fourth answer, which is what "déclenchables 4 fois, l'état est
        /// retiré dès la limite atteinte" means.
        #[serde(default = "one_u8_plain")]
        spends: u8,
    },
    /// Quand un sort du lanceur inflige des dommages de poussée au porteur, ce qui
    /// suppose des poussées déclarées bloquées : la Flibuste du Steamer et la Noa du
    /// Forgelance retirent l'état si la cible subit des dommages de poussée.
    PushDamage {
        /// Charges retirées à chaque déclenchement, comme pour
        /// [`StateTrigger::SpellTagged`].
        #[serde(default = "one_u8_plain")]
        spends: u8,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DurationDef {
    /// Les tours après celui de la pose : un effet que le jeu fait durer `d` tours
    /// s'écrit `turns: d - 1` (lancé au tour N pour trois tours, il dure jusqu'au tour
    /// N+2 inclus).
    pub turns: u8,
    #[serde(default)]
    pub refresh: Refresh,
    #[serde(default)]
    pub on_expire: OnExpire,
    /// L'état que la fin de celui-ci pose, pour le tour qui suit : Saoul du Pandawa
    /// pose en retombant l'état 3577 (« sorti de Saoul »), que lit la Gueule de Bois.
    #[serde(default)]
    pub then_gain: Option<String>,
    /// Les sorts dont la relance retombe à zéro quand cet état s'achève de lui-même
    /// (le Pacte Bestial de l'Osamodas). Un état qu'un lancer a retiré ne remet rien :
    /// voir [`Effect::ResetCooldowns`].
    #[serde(default)]
    pub then_reset_cooldowns: Vec<String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// À quel instant la condition d'une charge différée se lit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiresAt {
    /// Au moment du lancer, avant les effets du sort lui-même.
    #[default]
    Cast,
    /// Une fois le tour joué, sur l'état final. Ce que le sort a consommé en
    /// se lançant compte donc contre lui, et ce qu'un lancer ultérieur a
    /// réappliqué compte pour lui.
    TurnEnd,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refresh {
    /// Every application resets the timer, INCLUDING one that changes nothing
    /// because the resource is already at its cap. Casting a cost-reduction
    /// spell at 2 of 2 stacks still buys three more turns of the discount.
    #[default]
    OnApply,
    Never,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnExpire {
    /// Every stack drops at once.
    #[default]
    ResetToDefault,
    /// One stack falls off per turn.
    DecrementOne,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CritSpec {
    /// Base critical rate in whole percent, before the build's critical bonus: per
    /// spell, from 0 to 30 or more.
    pub base_rate: Maybe<u8>,
    /// `false` for spells that cannot critical at all, which is a different
    /// thing from a base rate of zero: a rate of zero still benefits from the
    /// build's critical bonus.
    #[serde(default = "yes")]
    pub can_crit: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LineDef {
    /// L'élément de la ligne, obligatoire sauf quand `best_element` ou
    /// `worst_element` le désigne : `problems()` exige qu'une ligne déclare son
    /// élément d'une des trois façons.
    #[serde(default)]
    pub element: Option<Element>,
    /// La ligne frappe dans le meilleur élément du lanceur (effet 2822, bornes dans
    /// `diceNum` et `diceSide`), décidé sur la caractéristique du build : résolu à la
    /// compilation du sort.
    #[serde(default)]
    pub best_element: bool,
    /// La ligne frappe dans le pire élément du lanceur (effet 2832 : la Supernova du
    /// Huppermage, qui frappe dans les deux).
    #[serde(default)]
    pub worst_element: bool,
    /// La ligne frappe une fois par point de cette ressource, zéro fois quand elle
    /// est absente : un sort qui se relance depuis chacune de ses poses (la Cadence
    /// du Roublard depuis ses bombes, le Forgelance depuis sa Lance, le Sadida depuis
    /// ses poupées, le Steamer depuis ses tourelles). Chaque instance traverse
    /// l'arrondi pour son compte. La ressource se remplit sous budget, le joueur
    /// déclarant ce que son terrain permet. Un nom, ou une liste de noms qui
    /// s'additionnent : la Surcharge Runique frappe une fois par rune vivante.
    #[serde(
        default,
        deserialize_with = "un_ou_plusieurs",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub repeats_per: Vec<String>,
    /// Le nombre de cibles que cette ligne frappe est plafonné par la somme de ces
    /// compteurs : la Runification lancée sur soi frappe une fois par rune vivante,
    /// jamais plus qu'il n'y a d'ennemis. Un nom, ou une liste, comme
    /// [`Self::repeats_per`].
    #[serde(
        default,
        deserialize_with = "un_ou_plusieurs",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub targets_capped_by: Vec<String>,
    /// La ligne vaut au prorata des PM qu'il reste au lanceur (le Zénith du Iop : la
    /// ligne de base sans PM, 52-58 de plus à 100 %). Entre les deux bornes, la règle
    /// est linéaire, ce qui est une lecture.
    #[serde(default)]
    pub scales_with_mp_left: bool,
    /// La zone que cette ligne couvre, remplie depuis l'instantané comme les
    /// magnitudes. Absente sur une ligne mono-cible sans zone déclarée.
    #[serde(default)]
    pub area: Option<AreaDef>,
    /// La ligne est un rebond : elle frappe un seul ennemi que les autres lignes du
    /// sort n'ont pas atteint, à cette distance au plus de la case visée ; sans
    /// ennemi disponible, elle ne frappe pas (le Javelot-foudre).
    #[serde(default)]
    pub rebound_within: Option<u8>,
    /// La ligne frappe chaque ennemi du scénario, sans zone à dessiner (la
    /// Runification lancée sur soi : une rune par cible déclarée).
    #[serde(default)]
    pub every_target: bool,
    /// La ligne ne frappe qu'à ce nombre exact de charges, pour les paliers qui
    /// changent de largeur (la Flèche Dévorante du Crâ : 11-13, 23-27, 34-38) : un
    /// bonus décale une fourchette, il ne l'élargit pas.
    #[serde(default)]
    pub active_at: Option<ActiveAt>,
    /// Cette ligne diffère de l'instantané exprès : la fusion ne la remplit pas
    /// depuis le vivier, et le désaccord n'est plus signalé pour elle seule (le Topkaj
    /// de l'Ecaflip, dont l'espérance 19 est l'une de ses trois lignes de Feu).
    /// `authored_magnitudes` exige toujours que la fourchette écrite existe sur ce
    /// sort.
    #[serde(default)]
    pub differs_from_snapshot: Option<String>,
    /// Absentes, elles sont inconnues, et la ligne compte comme un trou. Seule
    /// une ligne en pourcentage de vie s'en passe, n'ayant pas de fourchette.
    #[serde(default = "Maybe::inconnue")]
    pub critical: Maybe<(i32, i32)>,
    #[serde(default = "Maybe::inconnue")]
    pub normal: Maybe<(i32, i32)>,
    /// Additions to the base roll, applied before characteristic scaling.
    #[serde(default)]
    pub base_bonus: Vec<BaseBonus>,
    /// Un facteur propre à la ligne, en pour cent, qui multiplie avec les
    /// autres % au même arrondi. C'est le combo d'un mur de bombes du
    /// Roublard, que Plombage redéclenche : 150 pour 50 % de combos.
    #[serde(default)]
    pub facteur: Option<u32>,
    /// Des Dommages fixes propres à ce sort, au même étage que ceux de l'équipement,
    /// posés par un objet de classe à la résolution du build (« Main de Pandawa : +5
    /// Dommages »), jamais par un fichier de règles.
    #[serde(default, skip_serializing_if = "est_nul")]
    pub dommages_fixes: i32,
    /// De la Puissance propre à cette ligne, qui s'ajoute à celle du build :
    /// la Maîtrise d'arme, 300 sur les coups d'arme par défaut, que le joueur
    /// règle dans la Rotation. Posée par le solveur, comme `dommages_fixes`.
    #[serde(default, skip_serializing_if = "est_nul")]
    pub puissance_propre: i32,
    /// La ligne ne critique jamais, même quand le sort critique : le mur que
    /// Plombage redéclenche n'a pas de valeur critique, comme dans KrozBoom.
    #[serde(default)]
    pub sans_critique: bool,
    /// Une attaque d'invocation, et la part en pour cent de la caractéristique, de la
    /// Puissance et des dommages de l'élément que l'invocateur lui transmet (100, ou
    /// 50 à 100 chez l'Osamodas selon le palier). Aucun % de dommages ni dommage
    /// critique ne s'y applique.
    #[serde(default)]
    pub invocation: Option<u32>,
    /// Le taux de critique de la ligne, en pour cent, déjà composé : il
    /// remplace celui du sort. Une invocation qui enchaîne Béco-béco (5 %) et
    /// Bisou Béco (10 %) n'a pas un taux pour ses deux attaques.
    #[serde(default)]
    pub taux_critique: Option<u8>,
    /// Des dégâts en pourcentage d'une vie du lanceur, bruts : seules les
    /// résistances de l'élément les réduisent. La vie se lit sur `vie_restante` et
    /// `erosion`, que la classe déclare, et sur les points de vie du build ; pas de
    /// fourchette.
    #[serde(default)]
    pub percent_of_life: Option<PercentOfLife>,
    /// Le tirage auquel la ligne appartient : les lignes d'un même tirage sont des
    /// issues également probables, une seule jouée par lancer. Le solveur compte leur
    /// moyenne ; la fourchette affichée va de la plus basse à la plus haute (la
    /// Tromperie de l'Ecaflip).
    #[serde(default)]
    pub tirage: Option<String>,
    /// L'issue du tirage à laquelle la ligne appartient, quand une issue en
    /// porte plusieurs : le tour entier d'un monstre invoqué. Absente, chaque
    /// ligne du tirage est une issue à elle seule.
    #[serde(default)]
    pub issue: Option<u8>,
    /// La chance de cette issue, en pour cent ; absente, les issues sont également
    /// probables (l'Invocation de l'Arakne : 80 % l'Arakne, 20 % l'Arakne Majeure).
    #[serde(default)]
    pub chance: Option<u8>,
}

impl LineDef {
    /// Une ligne sans fourchette par nature : ses dégâts se lisent ailleurs.
    pub fn sans_fourchette(&self) -> bool {
        self.percent_of_life.is_some()
    }
}

/// Des dégâts en pourcentage d'une vie du lanceur.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PercentOfLife {
    /// 10 pour « 10 % des PV du lanceur ».
    pub percent: u8,
    pub of: LifeShare,
}

/// Quelle vie du lanceur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifeShare {
    /// La vie qui lui reste, effet 89.
    Remaining,
    /// La plus grande de la vie restante et de la vie manquante (la Mascarade, qui
    /// permute sous 50 % de vie).
    Larger,
    /// La vie érodée (effet 1118) : l'érosion déclarée appliquée à la vie perdue.
    Eroded,
}

/// Un nom seul ou une liste de noms, lus tous deux comme une liste.
fn un_ou_plusieurs<'de, D>(d: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum UnOuPlusieurs {
        Un(String),
        Plusieurs(Vec<String>),
    }
    Ok(match UnOuPlusieurs::deserialize(d)? {
        UnOuPlusieurs::Un(nom) => vec![nom],
        UnOuPlusieurs::Plusieurs(noms) => noms,
    })
}

/// Le cran auquel une ligne frappe, et elle seule.
///
/// ```yaml
/// active_at: { resource: devorante, exactly: 2 }        # un seul compteur
/// active_at:                                            # plusieurs, et un vainqueur
///   group: main_gagnante
///   tier: carre_as
///   all_of:
///     - { resource: cartes_as, exactly: 4 }
/// ```
///
/// `group` sert aux paliers qui se chevauchent (une Main du Poker de l'Ecaflip
/// peut être à la fois une Suite Royale et un Carré Couleurs) : le premier palier
/// déclaré dont les conditions tiennent gagne, et toutes ses lignes frappent.
/// L'ordre de déclaration est la priorité.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveAt {
    /// Forme courte : un seul compteur, à une valeur exacte.
    #[serde(default)]
    pub resource: Option<String>,
    /// Le nombre exact de charges. Zéro désigne la ligne de base, celle qui
    /// frappe quand rien n'est encore accumulé.
    #[serde(default)]
    pub exactly: Option<u8>,
    /// Forme longue : toutes ces conditions à la fois.
    #[serde(default)]
    pub all_of: Vec<ResourceIs>,
    /// Le groupe de paliers qui s'excluent. Absent, la ligne frappe dès que ses
    /// conditions tiennent, sans regarder les autres.
    #[serde(default)]
    pub group: Option<String>,
    /// Le palier auquel cette ligne appartient, au sein du groupe.
    #[serde(default)]
    pub tier: Option<String>,
}

impl ActiveAt {
    /// Les conditions, quelle que soit l'écriture employée.
    pub fn conditions(&self) -> Vec<ResourceIs> {
        match (&self.resource, self.exactly) {
            (Some(r), Some(n)) => vec![ResourceIs {
                resource: Some(r.clone()),
                sum_of: Vec::new(),
                exactly: n,
            }],
            _ => self.all_of.clone(),
        }
    }
}

/// Un compteur, ou une somme de compteurs, à une valeur exacte.
///
/// ```yaml
/// - { resource: devorante, exactly: 2 }
/// - { sum_of: [carte_as_pique, carte_as_trefle, carte_as_coeur, carte_as_carreau], exactly: 3 }
/// ```
///
/// La somme sert aux Mains de l'Ecaflip : chaque carte est un compteur, et un
/// Brelan d'As veut dire « trois As, n'importe lesquels ».
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResourceIs {
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub sum_of: Vec<String>,
    pub exactly: u8,
}

impl ResourceIs {
    /// Les compteurs à additionner, l'écriture courte valant somme d'un seul.
    pub fn resources(&self) -> Vec<&str> {
        match &self.resource {
            Some(r) => vec![r.as_str()],
            None => self.sum_of.iter().map(String::as_str).collect(),
        }
    }
}

/// Ce qu'une ligne de dégâts atteint au-delà de sa cible, pour le calcul sur X
/// cibles : sans plafond, un sort mono-cible serait multiplié par X.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct AreaDef {
    /// La lettre du jeu : `P` une case, `C` un cercle, `X` une croix.
    pub shape: char,
    /// Le rayon, la longueur ou le côté, selon la forme ; absent quand la donnée n'en
    /// porte pas. Il faut pour dessiner la zone.
    #[serde(default)]
    pub size: Option<u8>,
    /// Le second paramètre : la profondeur du rectangle, la distance de départ
    /// des bras de la croix en diagonale. Zéro pour toutes les autres formes.
    #[serde(default)]
    pub size2: u8,
    /// Combien de cibles au plus. `None` quand la forme n'est pas décodée : la ligne
    /// suit alors le nombre d'ennemis annoncé, ce qui surestime au-delà de la vraie
    /// zone.
    #[serde(default)]
    pub max_targets: Option<u16>,
    /// Le jeu retire ce pourcentage par cran d'éloignement de l'impact, au plus
    /// `falloff_steps` fois. Zéro veut dire non dégressif.
    #[serde(default)]
    pub falloff_percent: u8,
    #[serde(default)]
    pub falloff_steps: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BaseBonus {
    /// Bonus par palier, quand chaque cran ne vaut pas le même : la Flèche Punitive du
    /// Crâ ajoute 24 puis 32 (effet 293), cumulés.
    Steps {
        resource: String,
        /// Bonus apporté par le cran n°1, n°2, etc. Cumulatifs : à deux crans le
        /// sort gagne `steps[0] + steps[1]`. Au-delà de la liste, le dernier
        /// palier se répète.
        steps: Vec<i32>,
    },
    /// `amount` per point of the resource. Glas gains 6 base damage per stack.
    PerResource {
        resource: String,
        amount: i32,
        /// Value on a critical hit, when the game gives a different one (Crâ's Tirs
        /// Puissants: 250 Puissance, 300 on a crit). One value per cast: the critical one
        /// when the spell's critical rate on this build reaches
        /// [`crate::CRITICAL_EFFECT_THRESHOLD`].
        #[serde(default)]
        critical_amount: Option<i32>,
    },
    /// `PerResource`, mais seulement quand une autre ressource est présente : la
    /// Muselière de l'Ouginak ne compte ses 22 points par ennemi au contact que si la
    /// cible est la Proie (état 516).
    PerResourceGated {
        resource: String,
        amount: i32,
        /// La ressource qui ouvre le bonus. Absente, le bonus vaut zéro et la
        /// ligne garde sa fourchette nue.
        gated_by: String,
        #[serde(default)]
        critical_amount: Option<i32>,
    },
    /// `amount` par PM dépensé dans le tour, plafonné à `cap` crans (la Tourbière de
    /// l'Enutrof : cinq points par PM, jusqu'à six). Le chiffre vient du réglage
    /// « PM dépensés », pas d'une simulation.
    PerMpUsed {
        amount: i32,
        cap: u8,
        #[serde(default)]
        critical_amount: Option<i32>,
    },
    /// `amount` par ennemi touché au-delà du premier, plafonné à `cap` crans (le
    /// Muspel du Forgelance : +12 par cran, Cumul 4). La fourchette de base correspond
    /// à un ennemi touché. Résolu à la compilation depuis `Scenario::targets`.
    PerExtraTarget {
        amount: i32,
        /// Combien de crans au plus. Quatre sur le Muspel.
        cap: u8,
        #[serde(default)]
        critical_amount: Option<i32>,
    },
    /// `PerExtraTarget`, mais seulement tant que la ressource est présente (la
    /// Distillation du Pandawa : rien au premier lancer, aucun poison n'ayant encore
    /// été déclenché). Le produit se range dans les termes `while_resource`.
    PerExtraTargetWhile {
        resource: String,
        amount: i32,
        cap: u8,
        #[serde(default)]
        critical_amount: Option<i32>,
    },
    /// `amount` flat while the resource is present at all.
    WhileResource {
        resource: String,
        amount: i32,
        #[serde(default)]
        critical_amount: Option<i32>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApCost {
    pub base: u8,
    #[serde(default)]
    pub reduced_by: Option<CostReduction>,
    /// Le symétrique : un surcoût porté par un compteur (la Paume Explosive du
    /// Pandawa : 2 PA, puis 3).
    #[serde(default)]
    pub increased_by: Option<CostIncrease>,
}

/// A cost reduction driven by a stacking resource, read before the cast's own
/// effects apply: a spell does not discount itself on the cast that grants its
/// stack.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtraCasts {
    pub resource: String,
    /// Casts granted per point held.
    #[serde(default = "one_u8_plain")]
    pub per_stack: u8,
    /// Ceiling on what this can add, whatever the stack.
    #[serde(default = "one_u8_plain")]
    pub cap: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CostReduction {
    pub resource: String,
    #[serde(default = "one_u8")]
    pub per_stack: Maybe<u8>,
    /// A cast can never cost less than this.
    #[serde(default = "one_u8_plain")]
    pub floor: u8,
}

/// Un surcoût porté par un compteur, lu comme la réduction ci-dessus : AVANT
/// que les effets du lancer ne s'appliquent, si bien qu'un sort qui pose
/// lui-même le compteur ne se surtaxe pas au lancer qui le pose.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CostIncrease {
    pub resource: String,
    /// PA ajoutés par point du compteur.
    #[serde(default = "one_u8_plain")]
    pub per_stack: u8,
    /// Ce que le surcoût ne dépasse jamais, quel que soit le compteur ; un par
    /// défaut (la Paume Explosive : « cumulable une fois »).
    #[serde(default = "one_u8_plain")]
    pub ceiling: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Condition {
    /// The target holds at least one of the named resource.
    TargetHas { resource: String },
    /// The caster holds at least one of the named resource.
    CasterHas { resource: String },
    AtLeast {
        scope: Scope,
        resource: String,
        amount: u8,
    },
    /// Le compteur vaut exactement cette valeur, pour les compteurs qui codent autre
    /// chose qu'une quantité (l'élément de la rune du Huppermage).
    Exactly { resource: String, amount: u8 },
    /// Le compteur vaut AU PLUS cette valeur.
    AtMost { resource: String, amount: u8 },
    /// Toutes ces conditions à la fois, trois au plus, sans `all` imbriqué (les
    /// tourelles du Steamer).
    All { all: Vec<Condition> },
    /// Aucun de ces compteurs n'est posé : tous valent zéro (la Bonne Étoile de
    /// l'Ecaflip, « si la Main est vide »).
    NoneOf { none_of: Vec<String> },
}

impl Condition {
    /// Les compteurs que cette condition lit.
    pub fn resources(&self) -> Vec<&str> {
        match self {
            Condition::TargetHas { resource }
            | Condition::CasterHas { resource }
            | Condition::AtLeast { resource, .. }
            | Condition::Exactly { resource, .. }
            | Condition::AtMost { resource, .. } => vec![resource.as_str()],
            Condition::All { all } => all.iter().flat_map(Condition::resources).collect(),
            Condition::NoneOf { none_of } => none_of.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum Effect {
    /// Resolve every damage line of this spell.
    Damage,
    Gain {
        resource: String,
        #[serde(default = "one_u8_plain")]
        amount: u8,
        /// AP granted by the act of applying this resource.
        #[serde(default)]
        ap_bonus: Option<ApBonus>,
        /// Names a scenario parameter capping how many times per turn this may fire
        /// across all spells: a positional prerequisite the player asserts, respected by
        /// the solver instead of simulating the grid.
        #[serde(default)]
        budget: Option<String>,
        /// What happens when the resource is already at its cap.
        #[serde(default)]
        at_cap: AtCap,
        /// Condition on the gain itself, not on casting the spell: Poussière always deals
        /// its damage but only generates a Telefrag when there is one to move.
        #[serde(default)]
        requires: Option<Condition>,
        /// Le compteur monte jusqu'au nombre d'ennemis que la zone du sort atteint, au
        /// lieu de s'augmenter de `amount` (la propagation du Pinceau Tribal).
        #[serde(default)]
        up_to_zone: bool,
        /// Le gain n'a lieu que sur un coup critique (la Plume de Buhorado), c'est-à-dire
        /// quand le taux atteint le seuil de [`CRITICAL_EFFECT_THRESHOLD`].
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        on_critical: bool,
    },
    Consume {
        resource: String,
        #[serde(default = "one_u8_plain")]
        amount: u8,
        /// Condition sur le retrait lui-même, lue au moment où il s'applique : le
        /// Sabotage du Steamer ne fait pas descendre une tourelle sous l'Évolution I.
        #[serde(default)]
        requires: Option<Condition>,
    },
    /// Retire `amount` en puisant dans ces compteurs, dans l'ordre, chacun jusqu'à
    /// zéro : la Runification se retire des runes posées ce tour, puis des plus
    /// anciennes.
    ConsumeAcross {
        resources: Vec<String>,
        #[serde(default = "one_u8_plain")]
        amount: u8,
        /// La quantité vaut le nombre d'ennemis que la zone du sort atteint :
        /// lancée sur soi, la Runification déclenche une rune par cible.
        #[serde(default)]
        by_zone: bool,
    },
    StealAp {
        amount: u8,
        #[serde(default)]
        requires: Option<Condition>,
    },
    CarryAp {
        amount: u8,
        cap: u8,
        #[serde(default)]
        requires: Option<Condition>,
    },
    Reset {
        resource: String,
        /// Condition sur la remise elle-même, lue au moment où elle s'applique : le
        /// troisième cran de Rage de l'Ouginak fait passer en Forme Bestiale et remet la
        /// Rage à zéro.
        #[serde(default)]
        requires: Option<Condition>,
    },
    /// Bascule un compteur à un seul cran, zéro devient un et un devient zéro : la
    /// Picole et la Bombance du Pandawa, qui lisent l'état avant le lancer.
    Toggle { resource: String },
    /// Remet à zéro la relance de ces sorts, qui repartent dès ce tour dans la limite
    /// de leurs lancers (le Pacte Bestial relancé, effet 1045).
    ResetCooldowns {
        spells: Vec<String>,
        /// Lue au moment où la remise s'applique, comme celle d'un gain.
        #[serde(default)]
        requires: Option<Condition>,
    },
    /// Termine le tour du lanceur : plus rien ne part après ce lancer, et ses PA
    /// restants sont perdus (le Holmgang du Forgelance).
    EndTurn,
    /// Une ressource qui arrive au début d'un tour à venir (la Fuite du Temps du
    /// Xélor, effet 1100 avec `delay: 1`) : le Téléfrag n'existe pas dans le tour du
    /// lancer. Distinct de `Schedule`, qui porte des dégâts.
    ScheduleGain {
        resource: String,
        /// Tours à attendre. Un pour « au tour suivant ».
        #[serde(default = "one_u8_plain")]
        delay: u8,
        #[serde(default = "one_u8_plain")]
        amount: u8,
        /// Lu au moment du LANCER, comme pour `Schedule` : sans position
        /// précédente il n'y a rien à téléporter, donc rien à armer.
        #[serde(default)]
        requires: Option<Condition>,
    },
    /// Damage that lands later, on a countdown other events can shorten. The
    /// countdown becomes a state dimension of its own, which multiplies the search
    /// space.
    Schedule {
        id: String,
        /// Quand la condition `requires` se lit, au lancer par défaut. Le Gousset du
        /// Xélor exige `turn_end` : il pose une glyphe en fin de tour si la cible est
        /// alors Téléfrag, après avoir consommé lui-même le Téléfrag en se lançant.
        #[serde(default)]
        requires_at: RequiresAt,
        /// Turns to wait before the payload resolves at the start of a turn.
        delay: Maybe<u8>,
        /// Events that bring the countdown forward.
        #[serde(default)]
        accelerated_by: Vec<Acceleration>,
        payload: Vec<LineDef>,
        /// Condition checked when the spell is cast, not when the payload resolves;
        /// unmet, nothing is armed. Gousset always deals its Air damage but only schedules
        /// its larger hit if the target is Telefrag.
        #[serde(default)]
        requires: Option<Condition>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Acceleration {
    /// The only event kind so far. A closed vocabulary is what lets the
    /// compiler work out which casts interact.
    pub on_consumed: String,
    /// Turns removed per occurrence. Sablier de Xelor's tooltip says the delay
    /// "est reduit pour chaque Telefrag consomme" and never says by how much,
    /// which is why this is a `Maybe`.
    pub reduce_by: Maybe<u8>,
    /// Whether reaching zero resolves the payload there and then, or waits for
    /// the next turn to start. Two defensible readings of the same sentence,
    /// and they give different rotations.
    #[serde(default = "yes")]
    pub resolve_immediately_at_zero: bool,
}

/// What an application does when the resource is already at its maximum: a
/// cost-reduction stack at 2 of 2 still refreshes its duration, while a Telefrag
/// on a target that holds one does nothing, so the spell is cast without
/// generating.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtCap {
    /// The application still counts: durations refresh, bonuses are granted.
    #[default]
    Refresh,
    /// The application does not happen. The cast still resolves everything else.
    Skip,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApBonus {
    pub amount: u8,
    /// The bonus is claimable once per spell per turn: casting the same
    /// generator twice pays once, casting two different generators pays twice.
    #[serde(default)]
    pub once_per_spell_per_turn: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpellDef {
    pub id: String,
    /// Joins this spell to the vendored snapshot, which owns every number (AP cost,
    /// cast limits, relaunch interval, base critical rate, damage ranges). The YAML
    /// owns what the spell means.
    #[serde(default)]
    pub dofusdb_id: Option<u32>,
    pub name: Localised,
    pub ap_cost: ApCost,
    /// Cap across all targets (`maxCastPerTurn`), zero meaning unlimited.
    pub casts_per_turn: u8,
    /// Cap on a single target (`maxCastPerTarget`), distinct from the per-turn cap
    /// and usually lower: Gelure is 4 per turn but 2 per target.
    #[serde(default)]
    pub casts_per_target: Option<u8>,
    #[serde(default)]
    pub cooldown_turns: u8,
    /// Tours à attendre au début du combat avant le premier lancer
    /// (`initialCooldown`), distincts de `cooldown_turns` : Elding a un délai initial
    /// de 1 et un intervalle de 2.
    #[serde(default)]
    pub initial_cooldown: u8,
    /// `requires` ne vaut qu'à distance : les sorts du Forgelance qui parlent de la
    /// Lance changent de cible plutôt que d'être interdits. Armé, le sort frappe
    /// autour du lanceur ; désarmé, depuis la Lance. Au contact, les deux touchent ; à
    /// distance, il faut la Lance plantée.
    #[serde(default)]
    pub requires_only_at_range: bool,
    pub crit: CritSpec,
    /// `[minimum, maximum]` casting range in cells. It decides between `% Dommages
    /// mêlée` and `% Dommages distance`: a maximum of 1 forces contact, a minimum of 2
    /// forces range, anything else follows the scenario.
    #[serde(default)]
    pub range: Option<(u8, u8)>,
    /// Casting constraints, merged from the snapshot: line only, no line of
    /// sight needed, and so on. What a player has to respect to cast at all.
    #[serde(default)]
    pub cast: Option<crate::snapshot::CastRules>,
    #[serde(default)]
    pub lines: Vec<LineDef>,
    #[serde(default)]
    pub requires: Option<Condition>,
    /// La condition de lancement que la donnée du jeu porte, remplie à la
    /// fusion avec l'instantané. Voir [`Critere`] et
    /// [`ResourceDef::game_states`].
    #[serde(default)]
    pub cast_criterion: Option<Critere>,
    /// Les poussées que le sort inflige à l'ennemi, lues dans la donnée
    /// (effet 5) : la distance, et l'état que le LANCEUR doit porter ou non
    /// pour qu'elle parte (« Sobre : la poussée est plus importante »).
    #[serde(default)]
    pub pushes: Vec<PushDef>,
    /// Ce sort est interdit tant que le lanceur porte cette ressource, l'inverse de
    /// `requires` : les glyphes élémentaires du Féca (`HS!238`) s'appliquent l'état
    /// 238 en se lançant, d'où un seul par tour.
    #[serde(default)]
    pub blocked_by: Option<String>,
    /// Ce sort est interdit tant que cette ressource est à son plafond (le Pinceau
    /// Tribal, une fois tous les ennemis peints).
    #[serde(default)]
    pub blocked_at_max: Option<String>,
    /// Ce sort exige qu'un compteur dépasse un autre : la propagation du Pinceau
    /// Tribal vise un ennemi peint pas encore visé dans le tour.
    #[serde(default)]
    pub requires_more_than: Option<Surplus>,
    /// Ce que la case visée doit porter pour que ce sort parte, lu seulement avec un
    /// placement : le Javelot-foudre ne plante sa Lance que sur une case vide, et ne
    /// rebondit que visé sur un monstre.
    #[serde(default)]
    pub aim: Option<Aim>,
    /// Applied in order. Ordering is load-bearing: a spell that reads a resource
    /// and then consumes it must list the read first.
    pub effects: Vec<Effect>,
    /// Le sort vise le lanceur, et lui seul : sa limite par cible ne se multiplie pas
    /// par le nombre d'ennemis. Voir [`ModeDef::on_self`].
    #[serde(default)]
    pub on_self: bool,
    /// Le nom du mode que décrivent les champs du sort lui-même, obligatoire dès que
    /// `modes` n'est pas vide.
    #[serde(default)]
    pub mode_name: Option<Localised>,
    /// Les autres façons de lancer ce sort. Voir [`ModeDef`].
    #[serde(default)]
    pub modes: Vec<ModeDef>,
    /// Things about this spell nobody has settled: not a missing magnitude (that is
    /// `Maybe::Unknown`) but a question about what the spell does. They surface
    /// through `data_gaps`.
    #[serde(default)]
    pub open_questions: Vec<String>,
    /// Extra casts this spell earns within the turn, driven by a resource (the
    /// Xelor's Régulateur, effect 290). Read before the cast's own effects, like a
    /// cost reduction.
    #[serde(default)]
    pub extra_casts: Option<ExtraCasts>,
    /// What this spell does to the target beyond damage that another spell's state
    /// might answer, matched against [`StateTrigger::SpellTagged`]: `removes_range`,
    /// `removes_ap`, `removes_mp`. The removal itself is not modelled.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Modelling choices taken deliberately on this spell, and why: unlike an open
    /// question, the matter is settled (the Sram's Attaque Mortelle against a healthy
    /// target uses its lower range).
    #[serde(default)]
    pub assumptions: Vec<String>,
    /// Ce sort frappe, mais pas dans une rotation : ses dégâts dépendent de ce que
    /// fait l'adversaire (un piège du Sram, le glyphe de la Barrière du Féca). La
    /// couverture le sort du dénominateur et le nomme à part ; le texte dit où ces
    /// dégâts se calculent.
    #[serde(default)]
    pub outside_rotation: Option<String>,
    /// Ce que le piège ou le glyphe que pose ce sort inflige à l'ennemi qui le
    /// déclenche, quand la donnée ne le porte pas en lignes posées (Vendetta du Crâ).
    /// Compté seulement si le joueur déclare combien de pièges et de glyphes la cible
    /// déclenche par tour.
    #[serde(default)]
    pub trap_lines: Vec<LineDef>,
    /// Ce que le piège fait d'autre quand il part, aux mêmes conditions que ses
    /// dégâts (Vendetta du Crâ : ×110 % de dommages subis pour un tour).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trap_effects: Vec<Effect>,
    #[serde(default)]
    pub note: Option<String>,
    /// Ce « sort » est l'ARME du build : les % de dommages d'armes s'y
    /// appliquent, pas ceux des sorts. Aucun fichier de règles ne l'écrit, le
    /// solveur la greffe depuis l'équipement (`crates/app/src/armes.rs`).
    #[serde(default)]
    pub arme: bool,
}

/// Une autre façon de lancer le même sort, qui en partage les limites :
/// l'Accumulation du Iop frappe un ennemi ou charge le lanceur, jamais les deux.
/// Ce qui n'est pas repris vaut pour tous les modes (coût, relance, portée,
/// critique) ; les lancers par tour sont partagés, la limite par cible se compte
/// mode par mode.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModeDef {
    /// Ce que le joueur lit à côté du nom du sort : « sur soi ».
    pub name: Localised,
    /// Ce mode vise le lanceur. Voir [`SpellDef::on_self`].
    #[serde(default)]
    pub on_self: bool,
    #[serde(default)]
    pub lines: Vec<LineDef>,
    #[serde(default)]
    pub requires: Option<Condition>,
    #[serde(default)]
    pub blocked_by: Option<String>,
    #[serde(default)]
    pub blocked_at_max: Option<String>,
    #[serde(default)]
    pub requires_more_than: Option<Surplus>,
    #[serde(default)]
    pub aim: Option<Aim>,
    pub effects: Vec<Effect>,
}

/// Un compteur qui doit en dépasser un autre. Voir
/// [`SpellDef::requires_more_than`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Surplus {
    pub resource: String,
    pub than: String,
}

/// Ce que la case visée doit porter. Voir [`SpellDef::aim`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aim {
    EmptyCell,
    Enemy,
}

impl SpellDef {
    /// Le sort tel que le moteur le compile : lui seul, ou une définition par mode, la
    /// sienne en premier. Chaque définition garde l'identifiant du sort ; le nom porte
    /// le mode.
    pub fn alternatives(&self) -> Vec<SpellDef> {
        let mut base = self.clone();
        base.modes = Vec::new();
        base.mode_name = None;
        if self.modes.is_empty() {
            return vec![base];
        }
        let nommer = |mode: &Localised| Localised {
            fr: format!("{} ({})", self.name.fr, mode.fr),
            en: format!("{} ({})", self.name.en, mode.en),
        };
        let mut out = Vec::with_capacity(self.modes.len() + 1);
        for mode in &self.modes {
            let mut alt = base.clone();
            alt.name = nommer(&mode.name);
            alt.on_self = mode.on_self;
            alt.lines = mode.lines.clone();
            alt.requires = mode.requires.clone();
            alt.blocked_by = mode.blocked_by.clone();
            alt.blocked_at_max = mode.blocked_at_max.clone();
            alt.requires_more_than = mode.requires_more_than.clone();
            alt.aim = mode.aim;
            alt.effects = mode.effects.clone();
            out.push(alt);
        }
        if let Some(nom) = &self.mode_name {
            base.name = nommer(nom);
        }
        out.insert(0, base);
        out
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ruleset {
    pub schema_version: u32,
    pub game_version: String,
    pub class: String,
    pub name: Localised,
    #[serde(default)]
    pub note: Option<String>,
    pub resources: Vec<ResourceDef>,
    /// Limits the player asserts, standing in for what the grid would decide: how
    /// often a positional prerequisite can realistically be met. Each carries a
    /// label, a default and a reason.
    #[serde(default)]
    pub budgets: Vec<BudgetDef>,
    /// Les mécaniques qui coûtent cher à résoudre pour ce qu'elles rapportent,
    /// et que le joueur peut donc écarter.
    #[serde(default)]
    pub costly: Vec<CostlyDef>,
    /// Les états du jeu que porte un palier de groupe plutôt qu'un compteur.
    #[serde(default)]
    pub tier_states: Vec<TierStateDef>,
    /// Les états que lisent des conditions de lancement et qu'aucun compteur
    /// ne représente, chacun avec la raison pour laquelle la rotation ne le
    /// produit pas.
    #[serde(default)]
    pub states_outside_model: Vec<StateOutsideModel>,
    pub spells: Vec<SpellDef>,
}

/// Une poussée d'un sort sur l'ennemi : sa distance en cases, et la condition
/// sur les états du lanceur que porte son masque (`*E3531`, `*e100`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushDef {
    pub cells: u8,
    #[serde(default)]
    pub caster: Option<Critere>,
}

/// Un état du jeu présent quand l'un des paliers d'un groupe tient : la Main
/// Gagnante de l'Ecaflip (5554), que la Rekop exige, se lit sur les paliers du
/// groupe `main_gagnante`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TierStateDef {
    pub group: String,
    pub game_states: Vec<u32>,
    #[serde(default)]
    pub note: Option<String>,
}

/// Un état qu'une condition de lancement lit et que la rotation ne produit pas :
/// le moteur le tient pour absent (`HS=X` faux, `HS!X` vrai), et `note` dit
/// pourquoi c'est juste.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateOutsideModel {
    pub state: u32,
    pub name: String,
    pub note: String,
}

/// Une mécanique que le joueur peut écarter pour résoudre plus vite : son coût en
/// temps et son gain en dégâts sont mesurés dans `note`, que la page montre.
/// L'écarter donne un plancher.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CostlyDef {
    pub id: String,
    pub name: Localised,
    /// Comptée sans que le joueur l'ait demandé ? Faux par défaut : l'écran propose
    /// le calcul le plus court.
    #[serde(default)]
    pub default_on: bool,
    /// Les compteurs à neutraliser. Leur plafond tombe à zéro et leurs
    /// modificateurs de dégâts disparaissent : rien ne peut plus les remplir,
    /// tout ce qui les lit lit zéro, et la recherche n'a plus à les distinguer.
    pub resources: Vec<String>,
    /// Le prix et le gain, mesurés, tels que la page les affiche.
    pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BudgetDef {
    pub id: String,
    pub name: Localised,
    pub default: u8,
    pub max: u8,
    pub note: String,
}

fn yes() -> bool {
    true
}
fn one_u8() -> Maybe<u8> {
    Maybe::Known(1)
}
fn one_u8_plain() -> u8 {
    1
}
#[allow(clippy::trivially_copy_pass_by_ref)]
fn est_nul(n: &i32) -> bool {
    *n == 0
}

// ---------------------------------------------------------------------------
// Loading and validation
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Parse(serde_yaml_ng::Error),
    Invalid(Vec<String>),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "cannot read ruleset: {e}"),
            LoadError::Parse(e) => write!(f, "cannot parse ruleset: {e}"),
            LoadError::Invalid(problems) => {
                writeln!(f, "ruleset is not valid:")?;
                for p in problems {
                    writeln!(f, "  - {p}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// One number nobody has measured yet, with enough context to go and measure it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataGap {
    pub path: String,
    pub what: String,
}

impl fmt::Display for DataGap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.what)
    }
}

impl Ruleset {
    pub fn from_yaml(text: &str) -> Result<Ruleset, LoadError> {
        let rs: Ruleset = serde_yaml_ng::from_str(text).map_err(LoadError::Parse)?;
        let problems = rs.problems();
        if problems.is_empty() {
            Ok(rs)
        } else {
            Err(LoadError::Invalid(problems))
        }
    }

    /// La même classe sans les mécaniques coûteuses de `costly` : leur plafond tombe
    /// à zéro et leurs modificateurs partent, le sort retrouve son comportement nu.
    /// Le total qui en sort est un plancher.
    pub fn sans_mecaniques_couteuses(&self) -> Ruleset {
        self.avec_mecaniques(&[])
    }

    /// La même classe, en ne gardant que les mécaniques coûteuses nommées ; un
    /// identifiant inconnu est ignoré.
    pub fn avec_mecaniques(&self, gardees: &[String]) -> Ruleset {
        let ecartees: std::collections::BTreeSet<&str> = self
            .costly
            .iter()
            .filter(|c| !gardees.iter().any(|g| g == &c.id))
            .flat_map(|c| c.resources.iter().map(String::as_str))
            .collect();
        let mut allege = self.clone();
        for res in &mut allege.resources {
            if ecartees.contains(res.id.as_str()) {
                res.max = 0;
                res.modifies_damage = Vec::new();
                // La durée part avec, sinon la recherche distingue encore le compte à rebours
                // d'un compteur qui vaut toujours zéro.
                res.duration = None;
            }
        }
        // Une remise de coût qui lit un compteur écarté ne retire plus rien :
        // elle part, plutôt que de rester en lisant zéro. Ce n'est pas la même
        // chose pour le moteur, qui garde sinon une dimension de coût par sort.
        for sort in &mut allege.spells {
            if sort
                .ap_cost
                .reduced_by
                .as_ref()
                .is_some_and(|c| ecartees.contains(c.resource.as_str()))
            {
                sort.ap_cost.reduced_by = None;
            }
            if sort
                .ap_cost
                .increased_by
                .as_ref()
                .is_some_and(|c| ecartees.contains(c.resource.as_str()))
            {
                sort.ap_cost.increased_by = None;
            }
        }
        allege
    }

    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Ruleset, LoadError> {
        let text = std::fs::read_to_string(path).map_err(LoadError::Io)?;
        Ruleset::from_yaml(&text)
    }

    pub fn resource(&self, id: &str) -> Option<&ResourceDef> {
        self.resources.iter().find(|r| r.id == id)
    }

    /// Structural problems: duplicate ids, dangling references, impossible
    /// domains. These make a ruleset unloadable, as opposed to incomplete.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        // Une ligne de dégâts dit son élément d'une des trois façons, sans défaut : un
        // élément nommé, le meilleur du lanceur ou le pire.
        let controler = |chemin: String, l: &LineDef, out: &mut Vec<String>| match (
            l.element.is_some(),
            l.best_element,
            l.worst_element,
        ) {
            (false, false, false) => out.push(format!(
                "{chemin}: aucun élément déclaré, ni `element:`, ni `best_element`, \
                     ni `worst_element`"
            )),
            (true, true, _) | (true, _, true) | (false, true, true) => out.push(format!(
                "{chemin}: l'élément est déclaré deux fois, il faut choisir"
            )),
            _ => {}
        };
        for s in &self.spells {
            for (i, l) in s.lines.iter().enumerate() {
                controler(format!("{}.lines[{i}]", s.id), l, &mut out);
                // Une ligne en pourcentage de vie lit des réglages que la classe
                // doit déclarer : sans eux le moteur prendrait ses défauts, et
                // le joueur n'aurait aucun champ où dire sa vie.
                if let Some(p) = l.percent_of_life {
                    let mut exiges = vec!["vie_restante"];
                    if p.of == LifeShare::Eroded {
                        exiges.push("erosion");
                    }
                    for nom in exiges {
                        if !self
                            .resources
                            .iter()
                            .any(|r| r.id == nom && r.declared_by_player)
                        {
                            out.push(format!(
                                "{}.lines[{i}]: des dégâts en pourcentage de vie sans le \
                                 réglage déclaré `{nom}`",
                                s.id
                            ));
                        }
                    }
                }
            }
            for e in &s.effects {
                if let Effect::Schedule { id, payload, .. } = e {
                    for (i, l) in payload.iter().enumerate() {
                        controler(format!("{}.schedule[{id}].payload[{i}]", s.id), l, &mut out);
                    }
                }
            }
            for (k, m) in s.modes.iter().enumerate() {
                for (i, l) in m.lines.iter().enumerate() {
                    controler(format!("{}.modes[{k}].lines[{i}]", s.id), l, &mut out);
                }
            }
        }
        for r in &self.resources {
            for (j, e) in r.while_present.iter().enumerate() {
                for (i, l) in e.lines.iter().enumerate() {
                    controler(
                        format!("{}.while_present[{j}].lines[{i}]", r.id),
                        l,
                        &mut out,
                    );
                }
            }
        }
        for r in &self.resources {
            if let Some(garde) = &r.gain_per_turn_while {
                if !self.resources.iter().any(|x| &x.id == garde) {
                    out.push(format!(
                        "resource `{}` climbs while unknown resource `{garde}` is held",
                        r.id
                    ));
                }
                if r.gain_per_turn.is_none() {
                    out.push(format!(
                        "resource `{}` names `gain_per_turn_while` without any `gain_per_turn`                          to condition",
                        r.id
                    ));
                }
            }
        }
        let mut seen = BTreeSet::new();
        for r in &self.resources {
            for e in &r.while_present {
                if let StateTrigger::ResourceConsumed { resource } = &e.trigger {
                    if !self.resources.iter().any(|x| &x.id == resource) {
                        out.push(format!(
                            "resource `{}` triggers on unknown resource `{resource}`",
                            r.id
                        ));
                    }
                }
                for garde in e.requires.iter().flat_map(condition_resources) {
                    if !self.resources.iter().any(|x| x.id == garde) {
                        out.push(format!(
                            "resource `{}` triggers only while unknown resource `{garde}` holds",
                            r.id
                        ));
                    }
                }
            }
            if !seen.insert(&r.id) {
                out.push(format!("duplicate resource id `{}`", r.id));
            }
            if r.default > r.max {
                out.push(format!(
                    "resource `{}` defaults to {} but its maximum is {}",
                    r.id, r.default, r.max
                ));
            }
        }

        let known: BTreeSet<&str> = self.resources.iter().map(|r| r.id.as_str()).collect();
        for r in &self.resources {
            if let Some(t) = &r.shifts_into {
                if !known.contains(t.as_str()) {
                    out.push(format!("resource `{}` shifts into unknown resource `{t}`", r.id));
                }
            }
            if let Some(t) = r.duration.as_ref().and_then(|d| d.then_gain.as_ref()) {
                if !known.contains(t.as_str()) {
                    out.push(format!("resource `{}` expires into unknown resource `{t}`", r.id));
                }
            }
            if let Some(t) = &r.drains {
                if !known.contains(t.as_str()) {
                    out.push(format!("resource `{}` drains unknown resource `{t}`", r.id));
                }
                // Seul le bout d'une chaîne perd sa valeur en fin de tour.
                if r.shifts_into.is_some()
                    || !self.resources.iter().any(|x| x.shifts_into.as_deref() == Some(r.id.as_str()))
                {
                    out.push(format!("resource `{}` drains without ending a shift chain", r.id));
                }
            }
            if let Some(t) = &r.reset_at_turn_start_while {
                if !known.contains(t.as_str()) {
                    out.push(format!("resource `{}` resets while unknown resource `{t}` is held", r.id));
                }
            }
            if let Some(t) = &r.lose_at_turn_start_while {
                if !known.contains(t.as_str()) {
                    out.push(format!("resource `{}` loses a stack while unknown resource `{t}` is held", r.id));
                }
            }
            for s in r.duration.iter().flat_map(|d| &d.then_reset_cooldowns) {
                if !self.spells.iter().any(|x| &x.id == s) {
                    out.push(format!("resource `{}` resets the cooldown of unknown spell `{s}`", r.id));
                }
            }
            for (i, e) in r.while_present.iter().enumerate() {
                verifier_les_tirages(&e.lines, &format!("resource `{}`.while_present[{i}]", r.id), &mut out);
            }
            for t in &r.random_among {
                match self.resources.iter().find(|x| &x.id == t) {
                    None => out.push(format!("resource `{}` draws among unknown resource `{t}`", r.id)),
                    // Un tirage parmi des tirages ne se moyenne pas en une passe.
                    Some(x) if !x.random_among.is_empty() => {
                        out.push(format!("resource `{}` draws among `{t}`, itself a draw", r.id))
                    }
                    Some(_) => {}
                }
            }
        }
        // Les compteurs dont la perte déclenche quelque chose.
        let ecoutees: BTreeSet<&str> = self
            .resources
            .iter()
            .flat_map(|r| &r.while_present)
            .filter_map(|e| match &e.trigger {
                StateTrigger::ResourceConsumed { resource } => Some(resource.as_str()),
                _ => None,
            })
            .chain(self.spells.iter().flat_map(|s| {
                s.effects.iter().flat_map(|e| match e {
                    Effect::Schedule { accelerated_by, .. } => accelerated_by
                        .iter()
                        .map(|a| a.on_consumed.as_str())
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                })
            }))
            .collect();
        let check = |resource: &str, context: &str, out: &mut Vec<String>| {
            if !known.contains(resource) {
                out.push(format!("{context} refers to unknown resource `{resource}`"));
            }
        };

        // Une mécanique écartable nomme des compteurs qui existent, sinon la
        // case à cocher de la page ne neutraliserait rien du tout, en silence.
        for c in &self.costly {
            let ctx = format!("costly `{}`", c.id);
            for r in &c.resources {
                check(r, &ctx, &mut out);
            }
            if c.resources.is_empty() {
                out.push(format!("{ctx}: aucune ressource à écarter"));
            }
        }

        let mut spell_ids = BTreeSet::new();
        for s in &self.spells {
            if !spell_ids.insert(&s.id) {
                out.push(format!("duplicate spell id `{}`", s.id));
            }
            if !s.modes.is_empty() && s.mode_name.is_none() {
                out.push(format!(
                    "spell `{}` a des modes mais ne nomme pas le sien : il faut `mode_name`",
                    s.id
                ));
            }
        }
        // Chaque mode se contrôle comme un sort entier : une ressource mal
        // nommée dans un mode casse le chargement autant qu'ailleurs. Les
        // champs communs se relisent une fois par mode, d'où le dédoublonnage
        // en sortie.
        let deplies: Vec<SpellDef> = self.spells.iter().flat_map(SpellDef::alternatives).collect();
        for s in &deplies {
            if s.casts_per_turn == 0 {
                out.push(format!("spell `{}` can never be cast", s.id));
            }
            let ctx = format!("spell `{}`", s.id);
            if let Some(cr) = &s.ap_cost.reduced_by {
                check(&cr.resource, &ctx, &mut out);
            }
            if let Some(ci) = &s.ap_cost.increased_by {
                check(&ci.resource, &ctx, &mut out);
            }
            if let Some(ec) = &s.extra_casts {
                check(&ec.resource, &ctx, &mut out);
            }
            if let Some(b) = &s.blocked_by {
                check(b, &ctx, &mut out);
            }
            if let Some(b) = &s.blocked_at_max {
                check(b, &ctx, &mut out);
            }
            if let Some(surplus) = &s.requires_more_than {
                check(&surplus.resource, &ctx, &mut out);
                check(&surplus.than, &ctx, &mut out);
            }
            verifier_les_tirages(&s.lines, &ctx, &mut out);
            for line in &s.lines {
                for r in &line.repeats_per {
                    check(r, &ctx, &mut out);
                }
                for r in &line.targets_capped_by {
                    check(r, &ctx, &mut out);
                }
                // Deux façons de compter les ennemis touchés, qui s'excluent :
                // les cumuler laisserait le moteur choisir en silence.
                if line.every_target && line.rebound_within.is_some() {
                    out.push(format!(
                        "{}: une ligne ne peut pas être à la fois `every_target` et un rebond",
                        s.id
                    ));
                }
            }
            for line in &s.lines {
                for b in &line.base_bonus {
                    match b {
                        BaseBonus::PerResource { resource, .. }
                        | BaseBonus::WhileResource { resource, .. }
                        | BaseBonus::PerExtraTargetWhile { resource, .. }
                        | BaseBonus::Steps { resource, .. } => check(resource, &ctx, &mut out),
                        // Deux noms a verifier, pas un : la garde est une
                        // ressource comme le compteur.
                        BaseBonus::PerResourceGated {
                            resource, gated_by, ..
                        } => {
                            check(resource, &ctx, &mut out);
                            check(gated_by, &ctx, &mut out);
                        }
                        // Ne nomment aucune ressource : ils lisent un réglage
                        // du scénario, pas un état du combat.
                        BaseBonus::PerExtraTarget { .. } | BaseBonus::PerMpUsed { .. } => {}
                    }
                }
                if let Some(a) = &line.active_at {
                    let conditions = a.conditions();
                    if conditions.is_empty() {
                        out.push(format!(
                            "{}: active_at ne dit a quoi la ligne repond : il faut soit \
                             `resource` et `exactly`, soit `all_of`",
                            s.id
                        ));
                    }
                    for c in &conditions {
                        if c.resources().is_empty() {
                            out.push(format!(
                                "{}: une condition de active_at ne nomme ni `resource` \
                                 ni `sum_of`",
                                s.id
                            ));
                        }
                        for r in c.resources() {
                            check(r, &ctx, &mut out);
                        }
                    }
                    // Un palier sans groupe frappe des que ses conditions
                    // tiennent ; un groupe sans palier ne saurait pas quelles
                    // lignes font partie du meme cran. Les deux vont ensemble.
                    if a.group.is_some() != a.tier.is_some() {
                        out.push(format!(
                            "{}: active_at porte `group` sans `tier` ou l'inverse ; \
                             les deux vont ensemble",
                            s.id
                        ));
                    }
                }
            }
            for c in s
                .requires
                .iter()
                .chain(s.effects.iter().filter_map(|e| match e {
                    Effect::StealAp { requires, .. }
                    | Effect::CarryAp { requires, .. }
                    | Effect::Reset { requires, .. }
                    | Effect::Consume { requires, .. }
                    | Effect::Gain { requires, .. }
                    | Effect::ResetCooldowns { requires, .. } => requires.as_ref(),
                    _ => None,
                }))
            {
                for r in condition_resources(c) {
                    check(r, &ctx, &mut out);
                }
                // Une conjonction plate, de trois clauses au plus : le moteur
                // n'en compile pas davantage.
                if let Condition::All { all } = c {
                    if all.len() > 3 || all.iter().any(|x| matches!(x, Condition::All { .. })) {
                        out.push(format!("{ctx}: `all` porte au plus trois conditions, sans `all` imbrique"));
                    }
                }
                let vide = |c: &Condition| matches!(c, Condition::NoneOf { none_of } if none_of.is_empty());
                if vide(c) || matches!(c, Condition::All { all } if all.iter().any(vide)) {
                    out.push(format!("{ctx}: `none_of` ne nomme aucun compteur"));
                }
            }
            for e in &s.effects {
                if let Effect::ResetCooldowns { spells, .. } = e {
                    for x in spells {
                        if !self.spells.iter().any(|y| &y.id == x) {
                            out.push(format!("{ctx}: `reset_cooldowns` names unknown spell `{x}`"));
                        }
                    }
                }
            }
            for e in &s.effects {
                match e {
                    Effect::Gain { resource, .. }
                    | Effect::Consume { resource, .. }
                    | Effect::ScheduleGain { resource, .. }
                    | Effect::Reset { resource, .. } => check(resource, &ctx, &mut out),
                    // Basculer n'a de sens que sur un seul cran. Une durée est permise : Saoul dure
                    // deux tours, et la bascule relance son compte à rebours.
                    Effect::Toggle { resource } => {
                        check(resource, &ctx, &mut out);
                        if let Some(r) = self.resources.iter().find(|r| &r.id == resource) {
                            if r.max != 1 {
                                out.push(format!(
                                    "{}: `toggle` sur `{resource}`, qui doit valoir au plus un",
                                    s.id
                                ));
                            }
                        }
                    }
                    // Ce retrait ne déclenche ni état ni charge différée,
                    // contrairement à `consume` : un compteur qu'on écoute ne
                    // doit pas passer par lui.
                    Effect::ConsumeAcross { resources, .. } => {
                        for r in resources {
                            check(r, &ctx, &mut out);
                            if ecoutees.contains(r.as_str()) {
                                out.push(format!(
                                    "{}: `consume_across` retire `{r}`, qu'un état ou une \
                                     charge écoute : il faut `consume`",
                                    s.id
                                ));
                            }
                        }
                    }
                    Effect::Schedule {
                        accelerated_by,
                        payload,
                        id,
                        ..
                    } => {
                        for a in accelerated_by {
                            check(&a.on_consumed, &ctx, &mut out);
                        }
                        if payload.is_empty() {
                            out.push(format!(
                                "scheduled effect `{id}` on `{}` has no payload",
                                s.id
                            ));
                        }
                    }
                    _ => {}
                }
            }
            if s.effects.iter().any(|e| matches!(e, Effect::Damage)) && s.lines.is_empty() {
                out.push(format!("spell `{}` deals damage but has no lines", s.id));
            }
        }
        // Un état porté par un groupe sans palier ne serait jamais présent, et
        // le sort qui l'exige jamais lancé, sans que rien ne le dise.
        for t in &self.tier_states {
            let connu = self.spells.iter().flat_map(SpellDef::alternatives).any(|s| {
                s.lines.iter().any(|l| {
                    l.active_at.as_ref().and_then(|a| a.group.as_deref()) == Some(t.group.as_str())
                })
            });
            if !connu {
                out.push(format!("tier_states : aucun palier du groupe `{}`", t.group));
            }
        }
        let mut vus = BTreeSet::new();
        out.retain(|m| vus.insert(m.clone()));
        out
    }

    /// Every number this ruleset admits it does not know.
    ///
    /// This is the data-entry list, and it is also the verification queue: run a
    /// sensitivity sweep over these and the ones that change the rotation are
    /// the ones worth an in-game test.
    pub fn data_gaps(&self) -> Vec<DataGap> {
        let mut out = Vec::new();
        for r in &self.resources {
            for (i, m) in r.modifies_damage.iter().enumerate() {
                let unknown = match m {
                    DamageModifier::FinalMultiplier { percent, .. } => percent.is_unknown(),
                    DamageModifier::Characteristic { amount, .. }
                    | DamageModifier::BaseDamage { amount, .. }
                    | DamageModifier::FlatDamage { amount, .. } => amount.is_unknown(),
                    DamageModifier::CriticalRate { percent, .. } => percent.is_unknown(),
                    DamageModifier::CriticalDamage { amount, .. }
                    | DamageModifier::CriticalResistance { amount, .. }
                    | DamageModifier::PushDamage { amount, .. } => amount.is_unknown(),
                    DamageModifier::DomainPercent { .. } => false,
                };
                if unknown {
                    out.push(DataGap {
                        path: format!("{}.modifies_damage[{i}]", r.id),
                        what: "magnitude of this buff; cast a spell with and without it".into(),
                    });
                }
            }
            for (i, e) in r.while_present.iter().enumerate() {
                for (j, line) in e.lines.iter().enumerate() {
                    if line.normal.is_unknown() {
                        out.push(DataGap {
                            path: format!("{}.while_present[{i}].lines[{j}].normal", r.id),
                            what: format!("non-critical {:?} damage of this state", line.element),
                        });
                    }
                }
            }
        }
        for s in &self.spells {
            for q in &s.open_questions {
                out.push(DataGap {
                    path: format!("{}.open_question", s.id),
                    what: q.clone(),
                });
            }
            for e in &s.effects {
                if let Effect::Schedule {
                    id,
                    delay,
                    accelerated_by,
                    ..
                } = e
                {
                    if delay.is_unknown() {
                        out.push(DataGap {
                            path: format!("{}.schedule[{id}].delay", s.id),
                            what: "turns before the payload lands".into(),
                        });
                    }
                    for a in accelerated_by {
                        if a.reduce_by.is_unknown() {
                            out.push(DataGap {
                                path: format!("{}.schedule[{id}].accelerated_by.reduce_by", s.id),
                                what: format!(
                                    "turns removed per `{}` consumed; cast it, consume one, count the turns",
                                    a.on_consumed
                                ),
                            });
                        }
                    }
                }
            }
            if s.crit.base_rate.is_unknown() && s.crit.can_crit {
                out.push(DataGap {
                    path: format!("{}.crit.base_rate", s.id),
                    what: "base critical rate, in percent, off the tooltip".into(),
                });
            }
            for (i, line) in s.lines.iter().enumerate() {
                if line.sans_fourchette() {
                    continue;
                }
                if line.normal.is_unknown() {
                    out.push(DataGap {
                        path: format!("{}.lines[{i}].normal", s.id),
                        what: format!("non-critical {:?} base damage range", line.element),
                    });
                }
                if line.critical.is_unknown() && s.crit.can_crit {
                    out.push(DataGap {
                        path: format!("{}.lines[{i}].critical", s.id),
                        what: format!("critical {:?} base damage range", line.element),
                    });
                }
            }
            for (k, m) in s.modes.iter().enumerate() {
                for (i, line) in m.lines.iter().enumerate() {
                    if line.normal.is_unknown() {
                        out.push(DataGap {
                            path: format!("{}.modes[{k}].lines[{i}].normal", s.id),
                            what: format!("non-critical {:?} base damage range", line.element),
                        });
                    }
                    if line.critical.is_unknown() && s.crit.can_crit {
                        out.push(DataGap {
                            path: format!("{}.modes[{k}].lines[{i}].critical", s.id),
                            what: format!("critical {:?} base damage range", line.element),
                        });
                    }
                }
            }
            if let Some(cr) = &s.ap_cost.reduced_by {
                if cr.per_stack.is_unknown() {
                    out.push(DataGap {
                        path: format!("{}.ap_cost.reduced_by.per_stack", s.id),
                        what: format!(
                            "AP removed per stack of `{}`; cast it at 0, 1 and 2 stacks and read the cost",
                            cr.resource
                        ),
                    });
                }
            }
        }
        out
    }
}

fn condition_resources(c: &Condition) -> Vec<&str> {
    c.resources()
}

/// Les tirages d'une liste de lignes (`LineDef::tirage`) : des chances
/// partout ou nulle part, une seule par issue, cent pour cent en tout.
fn verifier_les_tirages(lines: &[LineDef], ctx: &str, out: &mut Vec<String>) {
    let mut noms: Vec<&str> = lines.iter().filter_map(|l| l.tirage.as_deref()).collect();
    noms.sort_unstable();
    noms.dedup();
    for nom in noms {
        let du_tirage: Vec<(usize, &LineDef)> =
            lines.iter().enumerate().filter(|(_, l)| l.tirage.as_deref() == Some(nom)).collect();
        if du_tirage.iter().any(|(_, l)| l.chance.is_some()) != du_tirage.iter().all(|(_, l)| l.chance.is_some()) {
            out.push(format!("{ctx}: tirage `{nom}` : des chances sur certaines issues seulement"));
            continue;
        }
        let mut issues: Vec<(usize, Option<u8>)> = Vec::new();
        for (i, l) in &du_tirage {
            let issue = l.issue.map_or(256 + i, usize::from);
            match issues.iter().find(|(k, _)| *k == issue) {
                Some((_, c)) if *c != l.chance => {
                    out.push(format!("{ctx}: tirage `{nom}` : deux chances pour une même issue"));
                }
                Some(_) => {}
                None => issues.push((issue, l.chance)),
            }
        }
        let total: u32 = issues.iter().filter_map(|(_, c)| c.map(u32::from)).sum();
        if issues.iter().all(|(_, c)| c.is_some()) && total != 100 {
            out.push(format!("{ctx}: tirage `{nom}` : ses chances font {total} %, et non 100"));
        }
    }
}

// ---------------------------------------------------------------------------
// Merging vendored numbers
// ---------------------------------------------------------------------------

pub mod snapshot;
