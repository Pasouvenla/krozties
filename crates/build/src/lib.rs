//! Turn an imported equipment list into the figures the damage pipeline needs.
//!
//! The importer sends seventeen item ids, the invested characteristics and the
//! forgemagic per slot; everything else is resolved here from the vendored
//! catalogue.
//!
//! # Effect 112
//!
//! "Dommages" across all elements (112) is separate from the per-element
//! 422/424/426/428 and must be added to each element exactly once: adding it on
//! top of a per-element figure that already includes it overstates a four-element
//! spell by about 6%.
//!
//! # Item values
//!
//! An item rolls somewhere inside a range, and the import only knows the range:
//! this takes the maximum, as build simulators do, and
//! [`Resolved::assumptions`] says so.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use dofus_damage::{DamageProfile, Element, ElementStats};
use serde::Deserialize;

/// Base action points before any equipment: six at level one, plus one gained
/// at level 100, so seven for anything this tool is aimed at.
const BASE_AP: i32 = 7;
/// Base movement points.
const BASE_MP: i32 = 3;

// ---------------------------------------------------------------------------
// Vendored catalogue
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
pub struct Catalogue {
    pub game_version: String,
    pub items: Vec<Item>,
    pub sets: Vec<Set>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Item {
    pub id: u32,
    pub name: String,
    /// Le niveau de l'objet, affiché sur la fiche de build.
    #[serde(default)]
    pub level: u32,
    pub set_id: Option<u32>,
    /// Stat name to `[min, max]`.
    pub stats: BTreeMap<String, [i32; 2]>,
    /// Effects this project does not interpret. An item whose whole point is one of
    /// them (the Dofus Nébuleux, effect 984) resolves to no statistics and must be
    /// flagged, not silently counted as nothing.
    pub unmapped_effect_ids: Vec<u32>,
    /// Ce que l'objet change aux sorts de sa classe : les objets de classe,
    /// effets 281 à 297 du jeu. Absent du catalogue sur tout autre objet.
    #[serde(default)]
    pub spell_modifiers: Vec<SpellModifier>,
    /// Ce que frappe l'objet s'il est une arme, et comment il se lance.
    #[serde(default)]
    pub weapon: Option<Weapon>,
    /// Les sorts que porte l'objet : son effet spécial (effet 1175) et les sorts
    /// temporaires qu'il ajoute (effet 722), chiffrés dans
    /// `data/snapshots/sorts-d-objets.json`.
    #[serde(default)]
    pub spells: Vec<ItemSpell>,
    /// Ce que l'objet fait hors du combat, tel que sa fiche le dit : « Lié au
    /// personnage », « Titre : … », « Attitude : … ».
    #[serde(default)]
    pub infos: Vec<String>,
}

/// Un sort porté par un objet, au grade que l'objet donne.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ItemSpell {
    pub spell: u32,
    pub grade: u8,
    /// Ajouté à la barre de sorts du porteur, qui le lance comme les siens.
    /// Sinon c'est l'effet spécial, qui se déclenche seul.
    pub temporary: bool,
}

/// Une arme : ses lignes (effets de catégorie 2) et ses champs de lancer.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Weapon {
    /// Le type d'objet, 7 pour un marteau : sa zone de frappe vient de son
    /// type, pas de l'objet.
    pub type_id: u32,
    pub ap: u8,
    pub range: [u8; 2],
    /// En pour cent, ajouté au critique du build. Zéro : elle ne critique pas.
    pub crit_rate: u8,
    /// Ce que gagne chaque jet sur un coup critique, avant la caractéristique.
    pub crit_bonus: i32,
    pub in_line: bool,
    pub in_diagonal: bool,
    pub line_of_sight: bool,
    /// Zéro : sans limite.
    pub casts_per_turn: u8,
    #[serde(default)]
    pub two_handed: bool,
    pub lines: Vec<WeaponLine>,
    #[serde(default)]
    pub effects: Vec<WeaponEffect>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WeaponLine {
    pub kind: WeaponLineKind,
    /// `fire`, `earth`, `water`, `air`, `neutral`, ou `best` pour le meilleur
    /// élément du porteur.
    pub element: String,
    pub min: i32,
    pub max: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WeaponLineKind {
    Damage,
    /// Frappe comme des dégâts, et rend la moitié en soin.
    Steal,
    /// Soigne la cible, sans frapper.
    Heal,
}

/// Ce que l'arme fait d'autre à sa cible : `push`, `pull`, `advance` (en
/// cases), `ap_removal`, `mp_removal`, `mp_steal`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct WeaponEffect {
    pub kind: String,
    pub min: i32,
    pub max: i32,
}

/// Ce qu'un objet de classe change à un sort : « Flèche Glacée : +4 dégâts de
/// base ».
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct SpellModifier {
    /// Le sort, par son identifiant dans la donnée du jeu (le `dofusdb_id`
    /// des règles).
    pub spell: u32,
    pub kind: SpellModifierKind,
    /// Signée : -1 pour « -1 de relance », -5 pour « -5 Portée minimale ». Un
    /// effet sans valeur, comme « ligne de vue désactivée », vaut 1.
    pub value: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpellModifierKind {
    /// « -1 PA » : le coût du sort baisse.
    ApCost,
    RangeMax,
    RangeMin,
    RangeBoostable,
    NoCastInLine,
    NoLineOfSight,
    NoTakenCell,
    Cooldown,
    CriticalRate,
    CastsPerTurn,
    CastsPerTarget,
    /// Ajoutés au jet, avant la caractéristique.
    BaseDamage,
    /// Des Dommages fixes pour ce seul sort, au même étage que ceux de
    /// l'équipement.
    Damage,
}

impl SpellModifier {
    /// Le texte du jeu sans le nom du sort : « +2 Portée maximale ».
    pub fn libelle(&self) -> String {
        use SpellModifierKind as K;
        let v = self.value;
        let lancers = |cible: &str| format!("{v:+} lancer{} par {cible}", if v.abs() > 1 { "s" } else { "" });
        match self.kind {
            K::ApCost => format!("{v:+} PA"),
            K::RangeMax => format!("{v:+} Portée maximale"),
            K::RangeMin => format!("{v:+} Portée minimale"),
            K::RangeBoostable => "Portée modifiable".into(),
            K::NoCastInLine => "lancer en ligne désactivé".into(),
            K::NoLineOfSight => "ligne de vue désactivée".into(),
            K::NoTakenCell => "case occupée nécessaire désactivée".into(),
            K::Cooldown => format!("{v:+} de relance"),
            K::CriticalRate => format!("{v:+} % Critique"),
            K::CastsPerTurn => lancers("tour"),
            K::CastsPerTarget => lancers("cible"),
            K::BaseDamage => format!("{v:+} dégâts de base"),
            K::Damage => format!("{v:+} Dommages"),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Set {
    pub id: u32,
    pub name: String,
    /// `bonuses[n - 1]` is the bonus for holding n pieces.
    pub bonuses: Vec<BTreeMap<String, [i32; 2]>>,
}

impl Catalogue {
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Catalogue, String> {
        let text = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("cannot read catalogue {}: {e}", path.as_ref().display()))?;
        Catalogue::from_json(&text)
    }

    /// Le catalogue depuis son texte : celui que l'application embarque.
    pub fn from_json(text: &str) -> Result<Catalogue, String> {
        serde_json::from_str(text).map_err(|e| format!("cannot parse catalogue: {e}"))
    }

    pub fn item(&self, id: u32) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    fn set(&self, id: u32) -> Option<&Set> {
        self.sets.iter().find(|s| s.id == id)
    }
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// What the importer sends: DofusBook's own payload, verbatim. The bookmarklet
/// translates nothing, since its code is copied into the bookmark at install time
/// and never refreshed: interpretation belongs here, where it can be corrected.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct RawStuff {
    pub id: u32,
    #[serde(default)]
    pub name: String,
    pub character_class: u32,
    pub character_level: u32,
    /// Slot to DofusBook item id.
    #[serde(default, rename = "stuffItem")]
    pub stuff_item: BTreeMap<String, Option<u32>>,
    #[serde(default, rename = "stuffCarac")]
    pub stuff_carac: BTreeMap<String, i32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RawItem {
    /// DofusBook's internal id, which is what `stuff_item` refers to.
    pub id: u32,
    /// The Ankama id, which is what every other source keys on.
    pub official: u32,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct BuildInput {
    #[serde(default)]
    pub class: u32,
    #[serde(default)]
    pub level: u32,
    /// Ankama item ids, one per slot, zero for an empty slot. Derived from the
    /// raw payload when one is present.
    #[serde(default)]
    pub items: Vec<u32>,
    /// Characteristics the player spent points on, by this project's names.
    #[serde(default)]
    pub invested: BTreeMap<String, i32>,
    /// The same thing as DofusBook sends it (`base_ch`, `scroll_ch`…, invested points
    /// and scrolls apart), folded into `invested` by [`BuildInput::normalise`].
    #[serde(default)]
    pub carac: BTreeMap<String, i32>,
    /// Forgemagic per slot, as stat name to amount.
    #[serde(default)]
    pub forgemagic: BTreeMap<String, BTreeMap<String, i32>>,
    /// DofusBook's name for the same field.
    #[serde(default)]
    pub fm: BTreeMap<String, BTreeMap<String, i32>>,
    /// The payload as the importer sends it, untranslated. When present it
    /// supersedes the fields above, which exist for hand-written inputs.
    #[serde(default, alias = "stuff")]
    pub raw_stuff: Option<RawStuff>,
    #[serde(default, alias = "items_table")]
    pub raw_items: Vec<RawItem>,
    #[serde(default, rename = "fmItems")]
    pub fm_items: BTreeMap<String, BTreeMap<String, i32>>,
    /// Forgemagic totals, which are not the sum of `fm_items`: some builds leave the
    /// per-slot breakdown empty and only fill the total. Taking the larger of the two
    /// per statistic recovers them without double counting.
    #[serde(default, rename = "fmGlobal")]
    pub fm_global: BTreeMap<String, i32>,
    /// La forgemagie élémentaire de l'arme, comme DofusBook la stocke : « df-85 »
    /// fait passer ses dégâts Neutre en Feu avec une potion à 85 % (voir
    /// `forgemager`).
    #[serde(default, rename = "fmWeapon")]
    pub fm_weapon: Option<String>,
    /// La même chose pour son vol Neutre (une gravure) : « vf-85 ».
    #[serde(default, rename = "fmStealWeapon")]
    pub fm_steal_weapon: Option<String>,
    /// Active Dofus bonuses. Their magnitude comes from DofusBook, since the game
    /// data does not carry it; their condition comes from nowhere (nothing says the
    /// Nébuleux bonus applies only on odd turns).
    #[serde(default)]
    pub boosts: Vec<Boost>,
    /// La fiche que DofusBook a calculée : ses totaux (`stats`) et la provenance de
    /// chaque ligne (`details`), sous ses codes ; absente pour une charge écrite à la
    /// main. C'est la référence d'affichage, non typée pour qu'un champ ajouté par
    /// DofusBook ne casse rien.
    #[serde(default)]
    pub dofusbook: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Boost {
    /// DofusBook sends `boostName`; a hand-written input may say `name`.
    #[serde(default, alias = "boostName")]
    pub name: String,
    /// The short code. Three families, not interchangeable: `deg` is a final-damage
    /// percentage, `dmg` flat damage across all elements, `pu` Power, a
    /// characteristic. Reading a `pu` of 350 as a percentage would multiply damage by
    /// 4.5.
    #[serde(default, alias = "effectName")]
    pub stat: String,
    /// The resolved amount. Absent from DofusBook's own payload, where it is
    /// `effectValue` times `count`.
    #[serde(default)]
    pub percent: i32,
    #[serde(default, alias = "effectValue")]
    pub effect_value: Option<i32>,
    #[serde(default)]
    pub count: Option<i32>,
    /// Set when the boost is one of a class's own spells rather than an item bonus:
    /// the solver casts those itself, so counting them here too would count them
    /// twice. Only when the solver models that spell, see `spell_id`.
    #[serde(default, alias = "classId")]
    pub class_id: Option<u32>,
    /// L'identifiant du sort, premier segment de `effectId` (« 23841-1-3-1560-0 »
    /// pour le Prélude au Fer) : il dit si le ruleset modélise ce sort avant de
    /// laisser le solveur s'en charger.
    #[serde(default, alias = "effectId")]
    pub effect_id: Option<String>,
    /// DofusBook marks inactive boosts rather than omitting them.
    #[serde(default)]
    pub active: Option<bool>,
}

impl Boost {
    /// The amount, whichever shape it arrived in.
    pub fn amount(&self) -> i32 {
        if self.percent != 0 {
            return self.percent;
        }
        self.effect_value.unwrap_or(0) * self.count.unwrap_or(1)
    }

    pub fn is_active(&self) -> bool {
        self.active.unwrap_or(true)
    }

    /// Le sort dont ce bonus est l'effet, quand la source le dit.
    pub fn spell_id(&self) -> Option<u32> {
        self.effect_id
            .as_deref()?
            .split('-')
            .next()?
            .parse::<u32>()
            .ok()
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Resolved {
    pub profile: DamageProfile,
    pub base_ap: u8,
    pub base_mp: u8,
    pub crit_bonus_percent: i32,
    /// Everything the resolution assumed rather than knew.
    pub assumptions: Vec<String>,
    /// Items carrying effects this project cannot interpret. Their mechanical
    /// contribution is missing from the profile and has to be declared by hand
    /// as a build modifier, the way a Dofus Nebuleux is.
    pub uninterpreted: Vec<String>,
    /// Ankama ids the catalogue did not know. They contribute nothing to the
    /// profile: the build is resolved from an incomplete stuff and must say so.
    pub items_missing: Vec<u32>,
    pub sets_active: Vec<(String, usize)>,
    pub totals: BTreeMap<String, i32>,
    /// Final-damage bonuses, as whole percentages for
    /// `FinalMultiplier::from_percents` (120 for a +20% Dofus). Unconditional unless
    /// `tours` names them; each keeps its name so a conditional one can be declared
    /// by hand.
    pub damage_multipliers: Vec<(String, u32)>,
    /// Les multiplicateurs qui ne valent qu'un tour sur deux, par leur nom :
    /// ceux du Rêve Nébuleux, que le jeu décrit tour par tour.
    pub tours: BTreeMap<String, Tours>,
    /// Les PA qu'un effet de Dofus donne à chaque tour (le Jaune Ocre du Dofus
    /// Ocre), hors de `base_ap`, que la fiche compare à celle de DofusBook : le
    /// moteur les ajoute lui-même.
    pub pa_des_dofus: Vec<(String, u8)>,
    /// Les PM qu'un effet de Dofus donne à chaque tour, hors de la fiche comme les
    /// PA.
    pub pm_des_dofus: Vec<(String, u8)>,
    /// Les PM qu'un bonus donne quand le joueur l'écarte : l'Abyssal, qui donne
    /// 1 PM plutôt qu'1 PA sans ennemi au contact.
    pub pm_si_ecarte: Vec<(String, u8)>,
    /// Les dommages fixes ajoutés pour un Dofus porté dont le bonus n'est pas actif
    /// dans DofusBook (l'Harmonie de Pandala du Tacheté, +20). Ils sont dans
    /// `profile` mais pas dans la fiche de DofusBook : la comparaison des fiches les
    /// retire.
    pub dommages_des_dofus_portes: i32,
    /// La Puissance par PM dépensé dans le tour : le Garde Champêtre du Dofus
    /// Sylvestre, 8 par PM.
    pub puissance_par_pm: i32,
    /// La Puissance d'un tour où l'on inflige ou subit des dommages de
    /// poussée : l'Éternel Cauchemar, 100.
    pub puissance_si_poussee: i32,
    /// Les bonus de Dofus qui dépendent du combat, que le joueur peut écarter du
    /// calcul dans l'onglet Rotation.
    pub bonus_conditionnels: Vec<BonusConditionnel>,
    /// Ce que les objets portés changent aux sorts, avec le nom de l'objet qui
    /// le porte. Le solveur l'applique aux règles de la classe : un objet d'une
    /// autre classe ne trouve aucun de ses sorts.
    pub spell_modifiers: Vec<(String, SpellModifier)>,
    /// L'arme du build, sa forgemagie élémentaire appliquée.
    pub arme: Option<ArmePortee>,
    /// Les sorts que portent les objets du build, avec l'objet qui les porte :
    /// son identifiant et son nom.
    pub sorts_d_objets: Vec<(u32, String, ItemSpell)>,
}

/// L'arme portée : l'objet, et ses lignes telles que le build les frappe.
#[derive(Clone, Debug)]
pub struct ArmePortee {
    pub id: u32,
    pub nom: String,
    pub arme: Weapon,
    /// Ce que la forgemagie a changé, pour le dire au joueur : « dégâts Neutre
    /// forgemagés en Feu à 85 % ».
    pub forgemagie: Vec<String>,
}

/// La forgemagie élémentaire d'une arme : « df-85 » fait passer les lignes de
/// dégâts Neutre en Feu, « vf-85 » le vol Neutre, à un pourcentage de leurs
/// bornes arrondies. Le nombre est celui de la potion : une potion à 85 %
/// convertit à 100 %, une à 50 % à 10 %.
fn forgemager(arme: &mut Weapon, potion: Option<&str>, gravure: Option<&str>) -> Vec<String> {
    let mut notes = Vec::new();
    for (code, nature) in [(potion, WeaponLineKind::Damage), (gravure, WeaponLineKind::Steal)] {
        let Some(code) = code.filter(|c| !c.is_empty()) else { continue };
        let (tete, pourcent) = code.split_once('-').unwrap_or((code, "85"));
        let pourcent = match pourcent.parse().unwrap_or(85) {
            85 => 100,
            50 => 10,
            autre => autre,
        };
        let element = match tete.get(1..) {
            Some("t") => ("earth", "Terre"),
            Some("f") => ("fire", "Feu"),
            Some("e") => ("water", "Eau"),
            Some("a") => ("air", "Air"),
            _ => continue,
        };
        // `Math.round` sur des bornes positives : l'entier le plus proche, la
        // demie vers le haut.
        let arrondi = |v: i32| (v * pourcent + 50).div_euclid(100);
        let mut change = false;
        for l in arme.lines.iter_mut().filter(|l| l.kind == nature && l.element == "neutral") {
            l.element = element.0.to_string();
            l.min = arrondi(l.min);
            l.max = arrondi(l.max);
            change = true;
        }
        if change {
            let quoi = if nature == WeaponLineKind::Damage { "dégâts" } else { "vol" };
            notes.push(format!("{quoi} Neutre forgemagés en {} à {pourcent} %", element.1));
        }
    }
    notes
}

/// Un bonus de Dofus qui ne vaut qu'à une condition de combat.
#[derive(Clone, Debug)]
pub struct BonusConditionnel {
    /// Le nom du bonus, celui des multiplicateurs et des PA.
    pub nom: String,
    /// Ce qu'il donne : « +10 % de dommages finaux », « +1 PA ».
    pub effet: String,
    /// Ce dont il dépend en combat.
    pub condition: &'static str,
    /// Sa case est-elle cochée d'office. Non pour un bonus qu'un ennemi doit
    /// déclencher, comme l'Œil du Cauchemar quand il entrave le porteur.
    pub coche: bool,
}

/// L'Œil du Cauchemar : +10 % de dommages finaux sur les coups qui suivent une
/// poussée, si le porteur est aussi désenvoûté ou entravé dans le tour. Sa case
/// déclare un ennemi qui désenvoûte ou entrave, décochée d'office ; les sorts qui
/// entravent leur propre lanceur (Tirs Puissants du Crâ), la rotation les compte
/// seule.
pub const OEIL_DU_CAUCHEMAR: &str = "Œil du Cauchemar";

/// Le tour où un multiplicateur vaut, quand il n'en vaut qu'un sur deux.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tours {
    Impairs,
    Pairs,
}

/// Un effet spécial de Dofus que ce calculateur compte, seulement si son Dofus
/// est dans le build, bonus actif ou non. L'effet vit dans un sort à part : le
/// Dofus ne porte que le code 984.
struct EffetDeDofus {
    /// Le Dofus, par son identifiant Ankama.
    dofus: u32,
    /// Le sort qui décrit l'effet dans la donnée du jeu.
    sort: u32,
    /// Le nom du bonus chez DofusBook.
    nom: &'static str,
    /// Sa statistique chez DofusBook : `deg` (% de dommages finaux), `dmg`
    /// (dommages fixes), `pa`.
    stat: &'static str,
    /// Ce qu'il vaut quand le Dofus est porté sans bonus actif ni forgemagie : les
    /// effets qui ne dépendent que du tour, ou d'une condition que la cible passive
    /// assure. `None` pour ceux qui se cumulent en combat, dont le cumul se règle dans
    /// DofusBook.
    porte: Option<i32>,
    /// La condition de combat dont il dépend : elle lui vaut une case dans
    /// l'onglet Rotation, pour l'écarter du calcul. `None`, il vaut toujours,
    /// à ses règles près.
    condition: Option<&'static str>,
    /// Les PM qu'il donne quand sa condition manque, sa case décochée : l'Abyssal
    /// donne 1 PA au contact d'un ennemi, 1 PM sinon.
    sinon_pm: u8,
}

const PAS_TAPE: &str = "si vous n'êtes pas tapé entre deux tours";

/// Le bonus de Dofus ne joue pas au premier tour : « s'il n'a subi aucune
/// attaque ennemie depuis son précédent tour de jeu », et au premier tour il
/// n'y a pas de tour précédent. Le Jaune Ocre et le Rouge Vermeil.
pub fn des_le_deuxieme_tour(nom: &str) -> bool {
    EFFETS_DE_DOFUS.iter().any(|e| e.nom == nom && e.condition == Some(PAS_TAPE))
}

const EFFETS_DE_DOFUS: &[EffetDeDofus] = &[
    // +20 % les tours impairs, −10 % les tours pairs (sort 5454).
    EffetDeDofus {
        dofus: 8698,
        sort: SORT_REVE_NEBULEUX,
        nom: REVE_NEBULEUX,
        stat: "deg",
        porte: Some(20),
        condition: None,
        sinon_pm: 0,
    },
    // « Le porteur du Tacheté gagne +20 dmg dès la première attaque pendant un
    // tour » ; ses alliés porteurs du Dorigami ou du Domakuro aussi.
    EffetDeDofus {
        dofus: 7112,
        sort: 18888,
        nom: HARMONIE_DE_PANDALA,
        stat: "dmg",
        porte: Some(20),
        condition: None,
        sinon_pm: 0,
    },
    // Le Vulbis et l'Ocre, « si le personnage joué n'est pas tapé entre 2 tours
    // de jeu » : la cible passive l'assure à chaque tour.
    EffetDeDofus {
        dofus: 6980,
        sort: 8396,
        nom: "Rouge Vermeil",
        stat: "deg",
        porte: Some(10),
        condition: Some(PAS_TAPE),
        sinon_pm: 0,
    },
    EffetDeDofus {
        dofus: 7754,
        sort: 8394,
        nom: "Jaune Ocre",
        stat: "pa",
        porte: Some(1),
        condition: Some(PAS_TAPE),
        sinon_pm: 0,
    },
    EffetDeDofus {
        dofus: 739,
        sort: 5952,
        nom: "Bleu Turquoise",
        stat: "deg",
        porte: None,
        condition: Some("cumul réglé dans DofusBook"),
        sinon_pm: 0,
    },
    EffetDeDofus {
        dofus: 694,
        sort: 8395,
        nom: "Pourpre Profond",
        stat: "deg",
        porte: None,
        condition: Some("cumul réglé dans DofusBook"),
        sinon_pm: 0,
    },
    EffetDeDofus {
        dofus: 20286,
        sort: 18672,
        nom: "Promesse d'Argent",
        stat: "deg",
        porte: None,
        condition: Some("une fois par combat, sous 20 % de vie"),
        sinon_pm: 0,
    },
    // +2 % de dommages à distance après une attaque en mêlée, et inversement (5
    // cumuls) : compté seulement par un bonus de DofusBook, au cumul réglé.
    EffetDeDofus {
        dofus: 7114,
        sort: 18629,
        nom: "Noir Ébène",
        stat: "dd",
        porte: None,
        condition: None,
        sinon_pm: 0,
    },
    // « À chaque début de tour, le porteur gagne 1 PM si aucun ennemi n'est à
    // son contact. Sinon, il gagne 1 PA » : la rotation ne tient pas les
    // positions, la case dit lequel.
    EffetDeDofus {
        dofus: 18043,
        sort: 6828,
        nom: "Descente aux Abysses",
        stat: "pa",
        porte: Some(1),
        condition: Some("si un ennemi est à votre contact en début de tour ; sinon, +1 PM"),
        sinon_pm: 1,
    },
    // « Le porteur gagne 8 de Puissance pendant 2 tours pour chaque PM qu'il
    // utilise » : les PM dépensés de la rotation le règlent.
    EffetDeDofus {
        dofus: 29136,
        sort: 28516,
        nom: "Garde Champêtre",
        stat: "pu_par_pm",
        porte: Some(8),
        condition: None,
        sinon_pm: 0,
    },
    // « Lorsque le porteur occasionne ou subit des dommages de poussée, il
    // gagne 100 de Puissance pour 1 tour » : la case « Vos poussées butent
    // contre un obstacle » de la rotation le règle.
    EffetDeDofus {
        dofus: 26066,
        sort: 20981,
        nom: "Éternel Cauchemar",
        stat: "pu_si_poussee",
        porte: Some(100),
        condition: None,
        sinon_pm: 0,
    },
];

/// L'effet spécial de ce Dofus est-il compté, par les bonus de DofusBook ou
/// parce qu'il est porté ? La fiche de l'objet le dit.
pub fn effet_de_dofus_compte(dofus: u32) -> bool {
    EFFETS_DE_DOFUS.iter().any(|e| e.dofus == dofus)
}

/// L'effet de Dofus dont un bonus de DofusBook est le reflet, par son sort ou
/// par son nom.
fn effet_de(boost: &Boost) -> Option<&'static EffetDeDofus> {
    EFFETS_DE_DOFUS.iter().find(|e| boost.spell_id() == Some(e.sort) || boost.name == e.nom)
}

/// Le bonus du Dofus Nébuleux, par le nom que DofusBook lui donne et par son
/// sort dans la donnée du jeu.
const REVE_NEBULEUX: &str = "Rêve Nébuleux";
const SORT_REVE_NEBULEUX: u32 = 5454;
/// Le bonus du Dofus Tacheté, `dmg` chez DofusBook : « 20 de Dommages »
/// (effet 112), sort 18888 de la donnée.
const HARMONIE_DE_PANDALA: &str = "Harmonie de Pandala";
/// L'effet spécial d'un Dofus : la donnée ne porte que ce code.
const EFFET_SPECIAL_DE_DOFUS: u32 = 984;

fn characteristic_for(element: Element) -> &'static str {
    match element {
        Element::Fire => "intelligence",
        Element::Earth => "strength",
        Element::Air => "agility",
        Element::Water => "chance",
        // Neutral scales off Strength, like Earth. Its own line is the flat
        // damage and the resistance, not the characteristic.
        Element::Neutral => "strength",
    }
}

fn damage_for(element: Element) -> &'static str {
    match element {
        Element::Fire => "damage_fire",
        Element::Earth => "damage_earth",
        Element::Air => "damage_air",
        Element::Water => "damage_water",
        Element::Neutral => "damage_neutral",
    }
}

/// Les statistiques qui ne changent aucun dégât, en codes de forgemagie de
/// DofusBook : elles ne se signalent pas.
fn sans_effet_sur_les_degats(code: &str) -> bool {
    matches!(
        code,
        // Initiative, Prospection, Pods, Invocations, Soins, Sagesse.
        "ii" | "pp" | "pd" | "pod" | "pods" | "ic" | "so" | "sa"
        // Tacle, Fuite, Esquives et Retraits de PA et de PM.
        | "ta" | "fu" | "epa" | "epm" | "rpa" | "rpm"
        // Les résistances, fixes et en pourcentage.
        | "rn" | "rt" | "rf" | "re" | "ra" | "rc" | "rm" | "rw" | "rp" | "rd"
        | "rnp" | "rtp" | "rfp" | "rep" | "rap"
    )
}

/// Le nom d'une statistique tel que le joueur le lit en jeu. Il vit ici, avec
/// les noms internes qu'il traduit ; un nom inconnu ressort tel quel.
pub fn libelle_stat(nom: &str) -> &str {
    match nom {
        "vitality" => "Vitalité",
        "wisdom" => "Sagesse",
        "strength" => "Force",
        "intelligence" => "Intelligence",
        "agility" => "Agilité",
        "chance" => "Chance",
        "power" => "Puissance",
        "ap" => "PA",
        "mp" => "PM",
        "range" => "Portée",
        "initiative" => "Initiative",
        "prospecting" => "Prospection",
        "flee" => "Fuite",
        "tackle" => "Tacle",
        "critical_rate" => "Coup critique",
        "critical_damage" => "Dommages critiques",
        "critical_resist" => "Résistance critique",
        "damage_all" => "Dommages",
        "damage_fire" => "Dommages Feu",
        "damage_earth" => "Dommages Terre",
        "damage_air" => "Dommages Air",
        "damage_water" => "Dommages Eau",
        "damage_neutral" => "Dommages Neutre",
        // Résistances en pourcentage et fixes ont chacune leur ligne : le « % » les
        // distingue, comme en jeu.
        "resist_fire" => "% Résistance Feu",
        "resist_earth" => "% Résistance Terre",
        "resist_air" => "% Résistance Air",
        "resist_water" => "% Résistance Eau",
        "resist_neutral" => "% Résistance Neutre",
        "resist_fixed_fire" => "Résistance Feu",
        "resist_fixed_earth" => "Résistance Terre",
        "resist_fixed_air" => "Résistance Air",
        "resist_fixed_water" => "Résistance Eau",
        "resist_fixed_neutral" => "Résistance Neutre",
        "resist_percent_melee" => "Résistance mêlée",
        "resist_percent_ranged" => "Résistance distance",
        "percent_spell" => "% Dommages sorts",
        "percent_weapon" => "% Dommages armes",
        "percent_melee" => "% Dommages mêlée",
        "percent_ranged" => "% Dommages distance",
        "push_damage" => "Dommages Poussée",
        "push_resist" => "Résistance Poussée",
        "summons" => "Invocations",
        "heals" => "Soins",
        "ap_dodge" => "Esquive PA",
        "mp_dodge" => "Esquive PM",
        "ap_reduction" => "Retrait PA",
        "mp_reduction" => "Retrait PM",
        "pods" => "Pods",
        "trap_damage" => "Dommages Pièges",
        "trap_power" => "Puissance Pièges",
        "damage_reflect" => "Dommages renvoyés",
        autre => autre,
    }
}

/// La famille d'une statistique, pour grouper la fiche : celles du jeu, dans
/// l'ordre d'affichage.
pub fn famille_stat(nom: &str) -> (u8, &'static str) {
    match nom {
        "vitality" | "wisdom" | "strength" | "intelligence" | "chance" | "agility" | "power" => {
            (0, "Caractéristiques")
        }
        "ap" | "mp" | "range" | "initiative" | "prospecting" | "tackle" | "flee"
        | "critical_rate" | "summons" | "heals" | "ap_dodge" | "mp_dodge" | "ap_reduction"
        | "mp_reduction" | "pods" => (1, "Combat"),
        "damage_all"
        | "damage_fire"
        | "damage_earth"
        | "damage_air"
        | "damage_water"
        | "damage_neutral"
        | "critical_damage"
        | "push_damage"
        | "percent_spell"
        | "percent_weapon"
        | "percent_melee"
        | "percent_ranged"
        | "trap_damage"
        | "trap_power"
        | "damage_reflect" => (2, "Dommages"),
        "resist_fire"
        | "resist_earth"
        | "resist_air"
        | "resist_water"
        | "resist_neutral"
        | "resist_fixed_fire"
        | "resist_fixed_earth"
        | "resist_fixed_air"
        | "resist_fixed_water"
        | "resist_fixed_neutral"
        | "critical_resist"
        | "push_resist"
        | "resist_percent_melee"
        | "resist_percent_ranged" => (3, "Résistances"),
        // Une statistique ajoutée au catalogue tombe ici et SE VOIT, au lieu de
        // disparaître d'une fiche qui n'affiche que ce qu'elle connaît.
        _ => (4, "Autres"),
    }
}

/// La section de la fiche où se range un emplacement, et son rang : celles que le
/// jeu montre.
pub fn section_emplacement(slot: &str) -> (u8, &'static str) {
    match slot {
        "ch" | "am" | "ca" | "ce" | "bo" | "a1" | "a2" => (0, "Équipement"),
        "ar" | "br" => (1, "Main"),
        "fa" | "mo" => (2, "Compagnons"),
        _ => (3, "Dofus et trophées"),
    }
}

/// Le nom de l'emplacement, dans l'ordre de [`BuildInput::SLOTS`] : `ch` porte
/// une coiffe, `ca` une cape, `br` un bouclier, `ar` une arme.
pub fn libelle_emplacement(slot: &str) -> &'static str {
    match slot {
        "ch" => "Coiffe",
        "am" => "Amulette",
        "a1" | "a2" => "Anneau",
        "ce" => "Ceinture",
        "bo" => "Bottes",
        "ca" => "Cape",
        "br" => "Bouclier",
        "ar" => "Arme",
        "fa" => "Familier",
        "mo" => "Monture",
        _ => "Dofus",
    }
}

/// Le libellé d'une ligne de forgemagie (codes de DofusBook), et si elle compte
/// dans le profil : la Portée change où l'on lance, pas ce qu'on inflige.
pub fn libelle_forgemagie(code: &str) -> (&str, bool) {
    if let Some(stat) = forgemagic_stat(code) {
        return (libelle_stat(stat), true);
    }
    // Reconnu mais hors profil : on le nomme quand même, sinon la fiche laisse
    // croire à une forgemagie absente.
    if let Some(stat) = characteristic_code(code) {
        return (libelle_stat(stat), false);
    }
    match code {
        // Un bonus de Dofus sur un Dofus, des % de dommages finaux ailleurs.
        "deg" => ("% Dommages finaux", true),
        "po" => ("Portée", false),
        "rpa" => ("Retrait PA", false),
        "rpm" => ("Retrait PM", false),
        "re" => ("Résistance Eau", false),
        "rm" => ("% Résistance mêlée", false),
        autre => (autre, false),
    }
}

fn forgemagic_stat(code: &str) -> Option<&'static str> {
    match code {
        "pa" => Some("ap"),
        "pm" => Some("mp"),
        "cc" => Some("critical_rate"),
        "dc" => Some("critical_damage"),
        "fo" => Some("strength"),
        "in" => Some("intelligence"),
        "ch" => Some("chance"),
        "ag" => Some("agility"),
        "pu" => Some("power"),
        // Flat damage on every element, DofusBook's "Dommages".
        "dmg" | "do" => Some("damage_all"),
        // The cast-context percentages, each confirmed on a real build that carries it.
        // The weapon one stays unmapped until a build shows its code, so the resolver
        // says it ignored it.
        "ds" => Some("percent_spell"),
        "dm" => Some("percent_melee"),
        "dd" => Some("percent_ranged"),
        // Les dommages d'un élément, les dommages d'arme, et la Vitalité, qui fait les
        // points de vie que lisent les sorts frappant en pourcentage de vie.
        "dnf" => Some("damage_neutral"),
        "dtf" => Some("damage_earth"),
        "dff" => Some("damage_fire"),
        "def" => Some("damage_water"),
        "daf" => Some("damage_air"),
        "dw" => Some("percent_weapon"),
        "vi" => Some("vitality"),
        "dp" => Some("push_damage"),
        // La Portée ne change aucun dégât, mais les zones de sorts dessinent d'où un
        // sort part.
        "po" => Some("range"),
        _ => None,
    }
}

/// DofusBook's two-letter characteristic codes.
fn characteristic_code(code: &str) -> Option<&'static str> {
    match code {
        "fo" => Some("strength"),
        "in" => Some("intelligence"),
        "ch" => Some("chance"),
        "ag" => Some("agility"),
        "vi" => Some("vitality"),
        "sa" => Some("wisdom"),
        _ => None,
    }
}

impl BuildInput {
    /// The seventeen slots, in the order the resolver expects them.
    pub const SLOTS: [&'static str; 17] = [
        "ch", "am", "a1", "a2", "ce", "bo", "ca", "br", "ar", "fa", "mo", "d1", "d2", "d3", "d4",
        "d5", "d6",
    ];

    pub fn normalise(&self) -> BuildInput {
        let mut out = self.clone();

        // The importer's untranslated payload wins when it is there: it is the
        // only form that cannot go stale, because nothing in it was interpreted
        // by code frozen inside a bookmark.
        if let Some(raw) = &self.raw_stuff {
            let official: BTreeMap<u32, u32> =
                self.raw_items.iter().map(|i| (i.id, i.official)).collect();
            out.class = raw.character_class;
            out.level = raw.character_level;
            out.items = Self::SLOTS
                .iter()
                .map(|slot| {
                    raw.stuff_item
                        .get(*slot)
                        .copied()
                        .flatten()
                        .and_then(|id| official.get(&id).copied())
                        .unwrap_or(0)
                })
                .collect();
            out.carac = raw.stuff_carac.clone();
        }
        if out.forgemagic.is_empty() && !self.fm_items.is_empty() {
            out.forgemagic = self.fm_items.clone();
        }

        // Sur la charge brute, `carac` vient d'être repris de `stuff`, donc on
        // parcourt la valeur effective et non celle reçue.
        let carac = out.carac.clone();
        for (key, value) in &carac {
            // `base_ch` and `scroll_ch` both feed Chance.
            let Some(code) = key
                .strip_prefix("base_")
                .or_else(|| key.strip_prefix("scroll_"))
            else {
                continue;
            };
            if let Some(name) = characteristic_code(code) {
                *out.invested.entry(name.to_string()).or_insert(0) += value;
            }
        }
        if out.forgemagic.is_empty() {
            out.forgemagic = self.fm.clone();
        }

        // Idempotente : les sources brutes sont consommées une fois lues, sinon un
        // second appel ajouterait de nouveau les points investis.
        out.carac.clear();
        out.raw_stuff = None;
        out.raw_items.clear();
        out.fm_items.clear();
        out
    }
}

pub fn resolve(input: &BuildInput, catalogue: &Catalogue) -> Resolved {
    resolve_with_class_spells(input, catalogue, None)
}

/// La résolution, en sachant quels sorts de classe le solveur modélise.
/// `modelles` porte des couples (identifiant du sort, statistique) : un bonus de
/// classe n'est écarté que si son couple y figure, le solveur le produisant alors
/// en payant le coût du lancer ; sinon il est compté ici. `None` écarte tous les
/// bonus de classe (tests sans ruleset).
pub fn resolve_with_class_spells(
    input: &BuildInput,
    catalogue: &Catalogue,
    modelles: Option<&std::collections::BTreeSet<(u32, String)>>,
) -> Resolved {
    let input = &input.normalise();
    let mut totals: BTreeMap<String, i32> = BTreeMap::new();
    let mut add = |name: &str, value: i32| {
        *totals.entry(name.to_string()).or_insert(0) += value;
    };

    let mut assumptions = Vec::new();
    let mut uninterpreted = Vec::new();
    let mut missing = Vec::new();
    let mut set_counts: BTreeMap<u32, usize> = BTreeMap::new();
    let mut spell_modifiers = Vec::new();
    let mut sorts_d_objets = Vec::new();

    for (name, value) in &input.invested {
        add(name, *value);
    }

    // ⚠️ La forgemagie `deg` est des % de dommages finaux. Sur l'emplacement d'un
    // Dofus qui en donne, c'est ce bonus avec ses règles (le Rêve Nébuleux tour par
    // tour), compté une fois s'il est aussi actif dans les bonus ; ailleurs, des %
    // de dommages finaux permanents.
    let mut bonus_saisis: Vec<Boost> = Vec::new();
    let mut dommages_des_dofus_portes = 0;
    let mut deg_libre = 0;
    let mut deg_par_emplacement = 0;
    let bonus_actif = |sort: u32, nom: &str| {
        input.boosts.iter().any(|b| b.is_active() && (b.spell_id() == Some(sort) || b.name == nom))
    };
    for (emplacement, lignes) in &input.forgemagic {
        let Some(&n) = lignes.get("deg") else { continue };
        if n == 0 {
            continue;
        }
        deg_par_emplacement += n;
        let objet = BuildInput::SLOTS
            .iter()
            .position(|s| s == emplacement)
            .and_then(|i| input.items.get(i).copied());
        match objet.and_then(|o| EFFETS_DE_DOFUS.iter().find(|e| e.stat == "deg" && e.dofus == o)) {
            Some(e) => {
                let (sort, nom) = (e.sort, e.nom);
                let dofus = catalogue.item(e.dofus).map_or("son Dofus", |i| i.name.as_str());
                if bonus_actif(sort, nom) {
                    assumptions.push(format!(
                        "{n:+} % de dommages finaux forgemagés sur le {dofus}, comptés une seule \
                         fois avec son bonus {nom}"
                    ));
                } else {
                    assumptions.push(format!(
                        "{n:+} % de dommages finaux forgemagés sur le {dofus}, comptés comme son \
                         bonus {nom}"
                    ));
                    bonus_saisis.push(Boost {
                        name: nom.to_string(),
                        stat: "deg".into(),
                        percent: n,
                        effect_id: Some(sort.to_string()),
                        ..Default::default()
                    });
                }
            }
            None => deg_libre += n,
        }
    }
    // Le total de DofusBook, quand il dépasse ce que les emplacements portent :
    // l'excédent n'a pas d'emplacement, donc pas de Dofus.
    deg_libre += input.fm_global.get("deg").copied().unwrap_or(0).saturating_sub(deg_par_emplacement).max(0);

    for e in EFFETS_DE_DOFUS {
        let Some(montant) = e.porte else { continue };
        let deja = bonus_actif(e.sort, e.nom) || bonus_saisis.iter().any(|b| b.name == e.nom);
        if input.items.contains(&e.dofus) && !deja {
            // Un bonus sous condition a déjà sa phrase, avec sa case dans
            // l'onglet Rotation : une seconde ligne répéterait la première.
            if e.condition.is_none() {
                let porte = catalogue.item(e.dofus).map_or("son Dofus", |i| i.name.as_str());
                assumptions.push(format!(
                    "{porte} porté, son bonus {} compte même inactif dans DofusBook",
                    e.nom
                ));
            }
            bonus_saisis.push(Boost {
                name: e.nom.to_string(),
                stat: e.stat.to_string(),
                percent: montant,
                effect_id: Some(e.sort.to_string()),
                ..Default::default()
            });
            if e.stat == "dmg" {
                dommages_des_dofus_portes += montant;
            }
        }
    }

    for id in input.items.iter().copied().filter(|i| *i != 0) {
        let Some(item) = catalogue.item(id) else {
            missing.push(id);
            continue;
        };
        for (name, range) in &item.stats {
            add(name, range[1]);
        }
        spell_modifiers.extend(item.spell_modifiers.iter().map(|m| (item.name.clone(), m.clone())));
        sorts_d_objets.extend(item.spells.iter().map(|s| (item.id, item.name.clone(), s.clone())));
        if item.stats.is_empty() && !item.unmapped_effect_ids.is_empty() {
            // Un Dofus sans caractéristique n'est pas ignoré : son effet spécial arrive par
            // les bonus.
            let special = item.unmapped_effect_ids == [EFFET_SPECIAL_DE_DOFUS];
            let couvert = special
                && EFFETS_DE_DOFUS.iter().any(|e| {
                    e.dofus == id
                        && (bonus_actif(e.sort, e.nom) || bonus_saisis.iter().any(|b| b.name == e.nom))
                });
            if !couvert {
                uninterpreted.push(if special {
                    format!(
                        "{} sans caractéristique fixe, son effet ne compte que s'il est actif \
                         dans DofusBook",
                        item.name
                    )
                } else {
                    format!("{} n'apporte aucune caractéristique comptée", item.name)
                });
            }
        }
        if let Some(set_id) = item.set_id {
            *set_counts.entry(set_id).or_insert(0) += 1;
        }
    }

    // L'arme : l'objet de l'emplacement « ar », s'il en est une.
    let arme = BuildInput::SLOTS
        .iter()
        .position(|s| *s == "ar")
        .and_then(|i| input.items.get(i).copied())
        .and_then(|id| catalogue.item(id))
        .and_then(|objet| {
            let mut arme = objet.weapon.clone()?;
            let forgemagie = forgemager(&mut arme, input.fm_weapon.as_deref(), input.fm_steal_weapon.as_deref());
            Some(ArmePortee { id: objet.id, nom: objet.name.clone(), arme, forgemagie })
        });

    let mut sets_active = Vec::new();
    for (set_id, count) in &set_counts {
        let Some(set) = catalogue.set(*set_id) else {
            continue;
        };
        if *count < 2 {
            continue;
        }
        sets_active.push((set.name.clone(), *count));
        if let Some(bonus) = set.bonuses.get(count - 1) {
            for (name, range) in bonus {
                add(name, range[1]);
            }
        }
    }

    let mut forgemagic: BTreeMap<String, i32> = BTreeMap::new();
    for slot in input.forgemagic.values() {
        for (code, amount) in slot {
            *forgemagic.entry(code.clone()).or_insert(0) += amount;
        }
    }
    for (code, total) in &input.fm_global {
        let entry = forgemagic.entry(code.clone()).or_insert(0);
        *entry = (*entry).max(*total);
    }
    for (code, amount) in forgemagic.iter().filter(|(c, _)| c.as_str() != "deg") {
        match forgemagic_stat(code) {
            Some(name) => add(name, *amount),
            None if sans_effet_sur_les_degats(code) => {}
            None => assumptions.push(format!("forgemagie inconnue ({code}), non comptée")),
        }
    }

    if !missing.is_empty() {
        assumptions.push(if missing.len() > 1 {
            format!("{} objets inconnus, ignorés", missing.len())
        } else {
            "1 objet inconnu, ignoré".to_string()
        });
    }
    assumptions.push("jets d'objets pris au maximum".into());

    let mut damage_multipliers: Vec<(String, u32)> = Vec::new();
    let mut tours: BTreeMap<String, Tours> = BTreeMap::new();
    let mut pa_des_dofus: Vec<(String, u8)> = Vec::new();
    let mut pm_des_dofus: Vec<(String, u8)> = Vec::new();
    let mut pm_si_ecarte: Vec<(String, u8)> = Vec::new();
    let mut puissance_par_pm = 0;
    let mut puissance_si_poussee = 0;
    if deg_libre != 0 {
        damage_multipliers.push((
            "% dommages finaux (forgemagie)".to_string(),
            (100 + deg_libre).max(0) as u32,
        ));
        assumptions.push(format!(
            "{deg_libre:+} % de dommages finaux forgemagés hors d'un Dofus, comptés à chaque tour"
        ));
    }
    for boost in input.boosts.iter().filter(|b| b.is_active()).chain(bonus_saisis.iter()) {
        let effet = effet_de(boost);
        if let Some(e) = effet.filter(|e| !input.items.contains(&e.dofus)) {
            let dofus = catalogue.item(e.dofus).map_or("son Dofus", |i| i.name.as_str());
            assumptions.push(format!("bonus {} non compté sans le {dofus}", boost.name));
            continue;
        }
        // La phrase d'un bonus qui dépend du combat : sa case dans l'onglet
        // Rotation le retire du calcul.
        let sous_condition = |effet_texte: String| {
            effet.and_then(|e| e.condition).map(|c| {
                format!("bonus {} compté à {effet_texte} à chaque tour, {c}", boost.name)
            })
        };
        if boost.class_id.is_some() {
            // Le solveur ne s'en charge que s'il en est capable. Sans cette
            // verification, un bonus de classe dont la mecanique manque au
            // ruleset se perdait des deux cotes.
            let couvert = match (modelles, boost.spell_id()) {
                (None, _) => true,
                (Some(set), Some(id)) => set.contains(&(id, boost.stat.clone())),
                (Some(_), None) => false,
            };
            if couvert {
                assumptions.push(format!(
                    "bonus {} non compté ici, la rotation lance ce sort elle-même",
                    boost.name
                ));
                continue;
            }
            assumptions.push(format!(
                "bonus {} compté en permanence, sans le coût en PA de son lancer",
                boost.name
            ));
        }
        match boost.stat.as_str() {
            // ⚠️ Le Rêve Nébuleux ne vaut pas à chaque tour : +20 % de dommages finaux les
            // tours impairs, −10 % les tours pairs (sort 5454).
            "deg" if boost.name == REVE_NEBULEUX || boost.spell_id() == Some(SORT_REVE_NEBULEUX) => {
                let impairs = format!("{}, tours impairs", boost.name);
                let pairs = format!("{}, tours pairs", boost.name);
                damage_multipliers.push((impairs.clone(), (100 + boost.amount()).max(0) as u32));
                damage_multipliers.push((pairs.clone(), 90));
                tours.insert(impairs, Tours::Impairs);
                tours.insert(pairs, Tours::Pairs);
            }
            // Les % de dommages finaux des Dofus, gagnés cumul après cumul en combat (un
            // coup critique pour le Bleu Turquoise, une attaque subie pour le Pourpre
            // Profond) : le cumul est celui réglé dans DofusBook.
            "deg" => {
                damage_multipliers.push((boost.name.clone(), (100 + boost.amount()).max(0) as u32));
                assumptions.push(
                    sous_condition(format!("{:+} % de dommages finaux", boost.amount())).unwrap_or_else(
                        || {
                            format!(
                                "bonus {} compté à {:+} % de dommages finaux à chaque tour",
                                boost.name,
                                boost.amount()
                            )
                        },
                    ),
                );
            }
            // Flat damage on every element: DofusBook displays it folded into the
            // per-element figures, which are computed here, so it is added here.
            "dmg" => {
                add("damage_all", boost.amount());
                assumptions.push(if boost.name == HARMONIE_DE_PANDALA {
                    format!(
                        "bonus {} compté à {:+} dommages sur chaque coup, gagné à la première attaque \
                         du tour",
                        boost.name,
                        boost.amount()
                    )
                } else {
                    format!("bonus {} compté à {:+} dommages sur chaque coup", boost.name, boost.amount())
                });
            }
            // Power, a characteristic. Not a percentage.
            "pu" => add("power", boost.amount()),
            // Des PA à chaque tour : le Jaune Ocre du Dofus Ocre.
            "pa" => {
                pa_des_dofus.push((boost.name.clone(), u8::try_from(boost.amount()).unwrap_or(0)));
                if let Some(pm) = effet.map(|e| e.sinon_pm).filter(|pm| *pm > 0) {
                    pm_si_ecarte.push((boost.name.clone(), pm));
                }
                if let Some(note) = sous_condition(format!("+{} PA", boost.amount())) {
                    assumptions.push(note);
                }
            }
            "pm" => pm_des_dofus.push((boost.name.clone(), u8::try_from(boost.amount()).unwrap_or(0))),
            "pu_par_pm" => {
                puissance_par_pm += boost.amount();
                assumptions.push(format!(
                    "bonus {} compté à {:+} Puissance par PM dépensé sur deux tours, limite de cumul \
                     inconnue",
                    boost.name,
                    boost.amount()
                ));
            }
            "pu_si_poussee" => {
                puissance_si_poussee += boost.amount();
                assumptions.push(format!(
                    "bonus {} compté à {:+} Puissance après une poussée, les tours où vos poussées \
                     butent contre un obstacle",
                    boost.name,
                    boost.amount()
                ));
            }
            // Une autre statistique, lue comme la forgemagie la lit : les % de
            // distance ou de mêlée du Noir Ébène, au cumul réglé dans DofusBook.
            other => match forgemagic_stat(other) {
                Some(stat) => {
                    add(stat, boost.amount());
                    assumptions.push(format!(
                        "bonus {} compté à {:+} {} (cumul réglé dans DofusBook)",
                        boost.name,
                        boost.amount(),
                        libelle_stat(stat)
                    ));
                }
                None => assumptions.push(format!("bonus {} ignoré, effet non pris en charge", boost.name)),
            },
        }
    }

    let get = |name: &str| totals.get(name).copied().unwrap_or(0);
    let damage_all = get("damage_all");

    // `% Dommages finaux` carried by equipment is a multiplier like any other,
    // not a profile statistic: it composes with the solver's conditional ones
    // rather than sitting in the cast context.
    if get("percent_final") != 0 {
        damage_multipliers.push((
            "% dommages finaux (équipement)".to_string(),
            (100 + get("percent_final")).max(0) as u32,
        ));
    }

    let mut elements = [ElementStats::default(); 5];
    for element in Element::ALL {
        elements[element.index()] = ElementStats {
            characteristic: get(characteristic_for(element)),
            // Added once per element, never twice.
            flat_damage: get(damage_for(element)) + damage_all,
        };
    }

    let bonus_conditionnels = EFFETS_DE_DOFUS
        .iter()
        .filter_map(|e| Some((e.nom, e.condition?)))
        .filter_map(|(nom, condition)| {
            let effet = if let Some((_, p)) = damage_multipliers.iter().find(|(n, _)| n == nom) {
                format!("{:+} % de dommages finaux", i64::from(*p) - 100)
            } else {
                let (_, pa) = pa_des_dofus.iter().find(|(n, _)| n == nom)?;
                format!("+{pa} PA")
            };
            Some(BonusConditionnel { nom: nom.to_string(), effet, condition, coche: true })
        })
        .chain((puissance_si_poussee > 0).then(|| BonusConditionnel {
            nom: OEIL_DU_CAUCHEMAR.to_string(),
            effet: "+10 % de dommages finaux après une poussée".to_string(),
            condition: "si un ennemi vous désenvoûte ou vous entrave dans le tour",
            coche: false,
        }))
        .collect();

    Resolved {
        dommages_des_dofus_portes,
        damage_multipliers,
        tours,
        pa_des_dofus,
        pm_des_dofus,
        pm_si_ecarte,
        puissance_par_pm,
        puissance_si_poussee,
        bonus_conditionnels,
        profile: DamageProfile {
            power: get("power"),
            flat_crit_damage: get("critical_damage"),
            elements,
            percent_spell: get("percent_spell"),
            percent_weapon: get("percent_weapon"),
            percent_melee: get("percent_melee"),
            percent_ranged: get("percent_ranged"),
            // La formule de la fiche de build : 50, plus 5 par niveau, plus la Vitalité.
            life: 50 + 5 * i32::try_from(input.level).unwrap_or(0) + get("vitality"),
            push_damage: get("push_damage"),
            level: i32::try_from(input.level).unwrap_or(0),
        },
        base_ap: (BASE_AP + get("ap")).clamp(0, 255) as u8,
        base_mp: (BASE_MP + get("mp")).clamp(0, 255) as u8,
        crit_bonus_percent: get("critical_rate"),
        assumptions,
        uninterpreted,
        items_missing: missing,
        sets_active,
        totals,
        spell_modifiers,
        arme,
        sorts_d_objets,
    }
}
