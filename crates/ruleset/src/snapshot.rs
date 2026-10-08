//! Vendored game numbers, and the merge that folds them into a ruleset. The
//! snapshot owns the numbers (AP cost, cast limits, relaunch interval, base
//! critical rate, damage ranges); the ruleset owns the meaning, which the data
//! cannot carry. A ruleset that states a number keeps it, and a disagreement with
//! the snapshot is reported, not resolved: a rebalance shows up as a conflict.

use std::collections::HashMap;

use dofus_damage::Element;
use serde::Deserialize;

use crate::{Effect, LineDef, Maybe, Ruleset};

#[derive(Clone, Debug, Deserialize)]
pub struct Snapshot {
    pub source: String,
    pub fetched_at: String,
    pub game_version: String,
    pub breed: Breed,
    pub spells: Vec<Spell>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Breed {
    pub id: u32,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Spell {
    pub id: u32,
    pub name: Name,
    /// What the spell does, in the game's own words. The ruleset says how it
    /// behaves; this says what a player reads in game, which is the thing worth
    /// showing them.
    #[serde(default)]
    pub description_fr: Option<String>,
    /// Two spells sharing a group are variants of one slot: a character can
    /// carry one or the other, never both. Ignoring this lets a deck ask for a
    /// rotation that cannot be played.
    #[serde(default)]
    pub variant_group: Option<u32>,
    /// -1 pour un sort sans icône : huit sorts d'objet, dont l'Assasindic.
    #[serde(default)]
    pub icon_id: Option<i32>,
    pub levels: Vec<Level>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Name {
    pub fr: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Level {
    pub grade: Option<u8>,
    pub ap_cost: Option<u8>,
    /// Zero means no per-turn limit, not "cannot be cast".
    pub max_cast_per_turn: Option<u8>,
    pub max_cast_per_target: Option<u8>,
    pub min_cast_interval: Option<u8>,
    pub base_crit_percent: Option<u8>,
    /// `[minRange, maxRange]`, either end possibly absent: a maximum of 1 means
    /// contact only, a minimum of 2 or more range only, anything else is up to the
    /// player's positioning.
    #[serde(default)]
    pub range: Option<[Option<u8>; 2]>,
    /// How the spell is cast: constraints a player lives with and that no
    /// damage figure conveys.
    #[serde(default)]
    pub cast: Option<CastRules>,
    /// La condition de lancement sur les états du lanceur (`statesCriterion`) :
    /// « `HS=3` » pour « porte l'état 3 ».
    #[serde(default)]
    pub states_criterion: Option<String>,
    pub normal_lines: Vec<Line>,
    pub critical_lines: Vec<Line>,
    /// Les identifiants d'effets que ce grade porte, lignes de dégâts exclues : ils
    /// reconnaissent les dégâts qui ne sont pas des lignes (2822, meilleur élément ;
    /// 2832, pire).
    #[serde(default)]
    pub effect_ids: Vec<u32>,
    /// Les effets du sort lui-même, valeurs comprises. Lus pour ses POUSSÉES
    /// (effet 5, distance dans `dice_num`), que comptent les dommages de
    /// poussée quand le joueur déclare que ses poussées butent.
    #[serde(default)]
    pub other_effects: Vec<PlacedEffect>,
    /// La zone de l'effet de pose : les cases sur lesquelles il faut marcher pour que
    /// le piège ou le glyphe parte. Ce n'est pas la zone des lignes posées, qui dit
    /// qui prend les dégâts : un piège du Sram peut partir d'une case et frapper sur
    /// treize.
    #[serde(default)]
    pub placed_trigger_zone: Option<Zone>,
    /// Les effets du sous-sort posé, valeurs comprises : la distance d'une poussée
    /// (effet 5) ou d'une attirance (6) est dans `dice_num`.
    #[serde(default)]
    pub placed_effects: Vec<PlacedEffect>,
    #[serde(default)]
    pub placed_lines: Vec<Line>,
    #[serde(default)]
    pub placed_critical_lines: Vec<Line>,
    /// Les identifiants d'effets du sous-sort posé : « dans le meilleur élément du
    /// lanceur » est un effet, pas une ligne (la Vendetta du Crâ, dont les
    /// `placed_lines` sont vides).
    #[serde(default)]
    pub placed_effect_ids: Vec<u32>,
}

/// Les effets qui portent des dégâts sans être des lignes, bornes dans `dice_num`
/// et `dice_side` : 2822 et 2832, dégâts du meilleur et du pire élément ; 2828,
/// vol du meilleur élément ; 89, 279 et 1118, au prorata d'une vie (PV, PV
/// manquants, PV érodés).
pub const DAMAGE_EFFECT_IDS: [u32; 6] = [2822, 2832, 2828, 89, 279, 1118];

#[derive(Clone, Debug, Deserialize)]
pub struct Line {
    pub element: String,
    pub range: (i32, i32),
    /// Le masque de cible du jeu, tel quel (`A`, `a,A`, `a,A,v50`). Il se compare et
    /// ne se décode pas : deux lignes de même zone aux masques différents sont des
    /// variantes, que les additionner doublerait.
    #[serde(default)]
    pub target: Option<String>,
    /// La zone que cette ligne couvre, quand la donnée du jeu en décrit une.
    #[serde(default)]
    pub zone: Option<Zone>,
}

/// La zone d'une ligne de dégâts, telle que le jeu la décrit. Pour le calcul
/// multi-cibles, c'est le plafond qui compte : un sort mono-cible ne frappe qu'une
/// fois.
#[derive(Clone, Copy, Debug, Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct Zone {
    /// La lettre du jeu : `P` une case, `C` un cercle, `X` une croix, etc.
    pub shape: char,
    pub size: Option<u8>,
    /// Le second paramètre, que seuls le rectangle et la croix en diagonale lisent,
    /// pour leur dessin.
    #[serde(default)]
    pub size2: u8,
    /// Combien de cases la zone couvre. `None` quand la forme n'est pas encore
    /// décodée : c'est alors dit, pas deviné.
    pub cells: Option<u16>,
    /// Dégressivité : ce pourcentage est retiré par cran d'éloignement de
    /// l'impact, au plus `falloff_steps` fois. Zéro veut dire non dégressif,
    /// ce que le texte de plusieurs sorts affirme explicitement.
    #[serde(default)]
    pub falloff_percent: u8,
    #[serde(default)]
    pub falloff_steps: u8,
}

/// Un effet du sous-sort posé, tel que la donnée le porte : `value`, `dice_num` et
/// `dice_side` changent de sens selon la famille d'effet, et ne s'interprètent pas
/// ici.
#[derive(Clone, Debug, Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct PlacedEffect {
    pub id: u32,
    #[serde(default)]
    pub value: Option<i32>,
    #[serde(default)]
    pub dice_num: Option<i32>,
    #[serde(default)]
    pub dice_side: Option<i32>,
    #[serde(default)]
    pub duration: Option<i32>,
    /// Le nombre de tours avant que l'effet ne s'applique : 0, tout de suite.
    #[serde(default)]
    pub delay: Option<i32>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub zone: Option<Zone>,
}

/// Une poussée de la donnée, si elle vise l'ennemi : sa distance, et ce que son
/// masque exige des états du lanceur (`*E3531` : Sobre). Un masque qui conditionne
/// la cible ou filtre des entités n'est pas lu, et la poussée n'est pas comptée.
fn poussee_sur_l_ennemi(e: &PlacedEffect) -> Option<crate::PushDef> {
    let cells = u8::try_from(e.dice_num?).ok().filter(|c| *c > 0)?;
    let mut ennemi = false;
    let mut conditions = Vec::new();
    for jeton in e.target.as_deref().unwrap_or("").split(',').map(str::trim) {
        match jeton {
            "A" => ennemi = true,
            "a" | "g" | "c" | "C" | "" => {}
            j if j.len() > 2 && (j.starts_with("*E") || j.starts_with("*e")) => {
                let etat: u32 = j[2..].parse().ok()?;
                conditions.push(crate::Critere::Etat {
                    etat,
                    present: j.starts_with("*E"),
                });
            }
            _ => return None,
        }
    }
    if !ennemi {
        return None;
    }
    let caster = match conditions.len() {
        0 => None,
        1 => conditions.pop(),
        _ => Some(crate::Critere::Et(conditions)),
    };
    Some(crate::PushDef { cells, caster })
}

/// Casting constraints from the game data: Xelor's Sablier can only be cast in a
/// straight line, but needs no line of sight.
#[derive(Clone, Copy, Debug, Default, Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct CastRules {
    #[serde(default)]
    pub in_line: bool,
    #[serde(default)]
    pub in_diagonal: bool,
    /// `false` is the notable case: the spell reaches through obstacles.
    #[serde(default = "yes_bool")]
    pub needs_line_of_sight: bool,
    #[serde(default = "yes_bool")]
    pub range_boostable: bool,
    #[serde(default)]
    pub needs_free_cell: bool,
    #[serde(default)]
    pub needs_taken_cell: bool,
    #[serde(default)]
    pub needs_cell_without_portal: bool,
    #[serde(default)]
    pub needs_free_trap_cell: bool,
}

fn yes_bool() -> bool {
    true
}

impl Snapshot {
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Snapshot, String> {
        let text = std::fs::read_to_string(path.as_ref())
            .map_err(|e| format!("cannot read snapshot {}: {e}", path.as_ref().display()))?;
        Snapshot::from_json(&text)
    }

    /// L'instantané depuis son texte : celui que l'application embarque.
    pub fn from_json(text: &str) -> Result<Snapshot, String> {
        serde_json::from_str(text).map_err(|e| format!("cannot parse snapshot: {e}"))
    }

    fn by_id(&self) -> HashMap<u32, &Spell> {
        self.spells.iter().map(|s| (s.id, s)).collect()
    }

    /// Les sorts qui entravent leur propre lanceur dans le tour : un retrait de PA, de
    /// PM ou de Portée sur le lanceur (`C`), sans délai, pour l'Œil du Cauchemar
    /// (Tirs Puissants du Crâ). Un retrait différé au tour suivant ne compte pas.
    pub fn sorts_qui_entravent_le_lanceur(&self) -> Vec<u32> {
        // -PA, -PO, -PM, puis -PA et -PM non esquivables.
        const RETRAITS: [u32; 5] = [101, 116, 127, 168, 169];
        self.spells
            .iter()
            .filter(|s| {
                s.top().is_some_and(|l| {
                    l.other_effects.iter().any(|e| {
                        RETRAITS.contains(&e.id)
                            && e.delay.unwrap_or(0) == 0
                            && e.target.as_deref().is_some_and(|t| t.split(',').any(|m| m == "C"))
                    })
                })
            })
            .map(|s| s.id)
            .collect()
    }

    /// Les sorts qui téléportent ou échangent leur lanceur, sans délai, pour la
    /// Ponctualité d'Henual. L'effet 4 téléporte toujours le lanceur ; l'effet 8
    /// l'échange avec sa cible ; les effets 1100, 1104, 1105 et 1106 seulement si leur
    /// masque le vise.
    pub fn sorts_qui_deplacent_le_lanceur(&self) -> Vec<u32> {
        self.spells
            .iter()
            .filter(|s| {
                s.top().is_some_and(|l| {
                    l.other_effects.iter().any(|e| {
                        let lanceur = e.target.as_deref().is_some_and(|t| t.split(',').any(|m| m == "C"));
                        e.delay.unwrap_or(0) == 0
                            && (matches!(e.id, 4 | 8) || (matches!(e.id, 1100 | 1104 | 1105 | 1106) && lanceur))
                    })
                })
            })
            .map(|s| s.id)
            .collect()
    }

    /// Les sorts qui tentent de retirer des PA ou des PM à l'ennemi, avec leur nombre
    /// de retraits (un pour des PA, un pour des PM, deux pour les deux) : retirés ou
    /// volés, esquivables ou non (101, 168, 1079, 84 ; 127, 169, 1080, 77), sur un
    /// masque qui vise les ennemis. Pour la Couronne de Brâm Barbe-Monde.
    pub fn retraits_sur_l_ennemi(&self) -> HashMap<u32, u8> {
        const PA: [u32; 4] = [101, 168, 1079, 84];
        const PM: [u32; 4] = [127, 169, 1080, 77];
        let vise = |e: &PlacedEffect| e.target.as_deref().is_some_and(|t| t.split(',').any(|m| m == "A"));
        self.spells
            .iter()
            .filter_map(|s| {
                let l = s.top()?;
                let retire = |ids: &[u32]| l.other_effects.iter().any(|e| ids.contains(&e.id) && vise(e));
                let n = u8::from(retire(&PA)) + u8::from(retire(&PM));
                (n > 0).then_some((s.id, n))
            })
            .collect()
    }
}

impl Spell {
    /// Highest grade available, which is the one a level 200 build casts.
    fn top(&self) -> Option<&Level> {
        self.levels.iter().max_by_key(|l| l.grade.unwrap_or(0))
    }
}

fn parse_element(name: &str) -> Option<Element> {
    match name {
        "fire" => Some(Element::Fire),
        "earth" => Some(Element::Earth),
        "air" => Some(Element::Air),
        "water" => Some(Element::Water),
        "neutral" => Some(Element::Neutral),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    pub path: String,
    pub authored: String,
    pub snapshot: String,
}

#[derive(Clone, Debug, Default)]
pub struct MergeReport {
    /// Values the snapshot supplied that the ruleset had marked unknown.
    pub filled: Vec<String>,
    /// Values present on both sides that disagree. Nothing is overwritten: an
    /// authored number outranks a fetched one, and a conflict (a rebalance or a typo)
    /// is for a human to settle.
    pub conflicts: Vec<Conflict>,
    /// Things the merge could not line up at all.
    pub unmatched: Vec<String>,
}

impl MergeReport {
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty() && self.unmatched.is_empty()
    }
}

impl Ruleset {
    /// Fill every unknown number from the snapshot, reporting what was filled,
    /// what disagreed, and what could not be matched.
    pub fn merge_snapshot(&mut self, snapshot: &Snapshot) -> MergeReport {
        let index = snapshot.by_id();
        let mut report = MergeReport::default();

        for resource in &mut self.resources {
            let Some(source_id) = resource.dofusdb_source else {
                continue;
            };
            let Some(level) = index.get(&source_id).and_then(|s| s.top()) else {
                report.unmatched.push(format!(
                    "{}: dofusdb source {source_id} is not in the snapshot",
                    resource.id
                ));
                continue;
            };
            let mut normal = level.normal_lines.clone();
            let mut critical = level.critical_lines.clone();
            for (i, effect) in resource.while_present.iter_mut().enumerate() {
                for (j, line) in effect.lines.iter_mut().enumerate() {
                    let path = format!("{}.while_present[{i}].lines[{j}]", resource.id);
                    fill_line(
                        line,
                        &path,
                        &mut normal,
                        &mut critical,
                        &mut report,
                        !level.critical_lines.is_empty(),
                    );
                    // La zone d'un poison ou d'un glyphe : là où le sort qui le
                    // pose frappe. Sans elle, il ne touchait qu'un ennemi.
                    completer_la_zone(line, &path, level, &mut report);
                }
            }
        }

        for spell in &mut self.spells {
            let Some(dofusdb_id) = spell.dofusdb_id else {
                report.unmatched.push(format!(
                    "{}: no dofusdb_id, numbers stay hand-authored",
                    spell.id
                ));
                continue;
            };
            let Some(source) = index.get(&dofusdb_id) else {
                report.unmatched.push(format!(
                    "{}: dofusdb id {dofusdb_id} is not in the snapshot",
                    spell.id
                ));
                continue;
            };
            let Some(level) = source.top() else {
                report
                    .unmatched
                    .push(format!("{}: snapshot spell has no levels", spell.id));
                continue;
            };

            if let Some(ap) = level.ap_cost {
                if ap != spell.ap_cost.base {
                    report.conflicts.push(Conflict {
                        path: format!("{}.ap_cost.base", spell.id),
                        authored: spell.ap_cost.base.to_string(),
                        snapshot: ap.to_string(),
                    });
                }
            }

            // Casting range, which decides melee vs ranged and therefore which
            // of the two percentages applies. Only filled when both ends are
            // present: half a range decides nothing.
            if spell.cast.is_none() {
                spell.cast = level.cast;
            }
            // La condition de lancement vient de la donnée, pour tous les sorts.
            if spell.cast_criterion.is_none() {
                // Une chaîne vide, que la donnée porte souvent, n'exige rien.
                if let Some(texte) = level.states_criterion.as_deref().filter(|t| !t.trim().is_empty()) {
                    match crate::Critere::lire(texte) {
                        Ok(c) => spell.cast_criterion = Some(c),
                        Err(e) => report
                            .unmatched
                            .push(format!("{}: condition de lancement illisible, {e}", spell.id)),
                    }
                }
            }
            // Les poussées sur l'ennemi : effet 5, distance dans `dice_num`.
            if spell.pushes.is_empty() {
                spell.pushes = level
                    .other_effects
                    .iter()
                    .filter(|e| e.id == 5)
                    .filter_map(poussee_sur_l_ennemi)
                    .collect();
            }
            if let (None, Some([Some(min), Some(max)])) = (spell.range, level.range) {
                spell.range = Some((min, max));
                report
                    .filled
                    .push(format!("{}.range = {min}-{max}", spell.id));
            }

            match (&spell.crit.base_rate, level.base_crit_percent) {
                (Maybe::Unknown(_), Some(rate)) => {
                    spell.crit.base_rate = Maybe::Known(rate);
                    report
                        .filled
                        .push(format!("{}.crit.base_rate = {rate}%", spell.id));
                }
                (Maybe::Known(authored), Some(rate)) if *authored != rate => {
                    report.conflicts.push(Conflict {
                        path: format!("{}.crit.base_rate", spell.id),
                        authored: authored.to_string(),
                        snapshot: rate.to_string(),
                    });
                }
                _ => {}
            }

            // Match by element rather than by position: a ruleset models one line per
            // element where the snapshot may repeat it per target, in another order.
            let mut normal = level.normal_lines.clone();
            let mut critical = level.critical_lines.clone();

            for (i, line) in spell.lines.iter_mut().enumerate() {
                fill_line(
                    line,
                    &format!("{}.lines[{i}]", spell.id),
                    &mut normal,
                    &mut critical,
                    &mut report,
                    spell.crit.can_crit,
                );
            }

            // A scheduled payload is a damage line listed alongside the immediate one
            // (Sablier: fire 13-15, then fire 28-30): immediate lines are matched first.
            for effect in &mut spell.effects {
                if let Effect::Schedule { id, payload, .. } = effect {
                    for (j, line) in payload.iter_mut().enumerate() {
                        fill_line(
                            line,
                            &format!("{}.schedule[{id}].payload[{j}]", spell.id),
                            &mut normal,
                            &mut critical,
                            &mut report,
                            spell.crit.can_crit,
                        );
                    }
                }
            }

            // Un mode puise d'abord dans ce que le mode principal a laissé, puis dans toute
            // la donnée quand l'élément y manque : la propagation du Pinceau Tribal prend la
            // seconde ligne Terre ; le Javelot-foudre visé sur un monstre, la même ligne Eau
            // que le mode qui plante.
            for (k, mode) in spell.modes.iter_mut().enumerate() {
                let mut reste_n = normal.clone();
                let mut reste_c = critical.clone();
                for (j, line) in mode.lines.iter_mut().enumerate() {
                    let puise = !(line.best_element
                        || line.worst_element
                        || line.active_at.is_some()
                        || line.differs_from_snapshot.is_some());
                    let present = |pool: &[Line]| {
                        pool.iter()
                            .any(|l| parse_element(&l.element) == line.element)
                    };
                    if puise && !present(&reste_n) && !present(&reste_c) {
                        reste_n = level.normal_lines.clone();
                        reste_c = level.critical_lines.clone();
                    }
                    fill_line(
                        line,
                        &format!("{}.modes[{k}].lines[{j}]", spell.id),
                        &mut reste_n,
                        &mut reste_c,
                        &mut report,
                        spell.crit.can_crit,
                    );
                }
            }

            // ⚠️ LA ZONE DE CHAQUE LIGNE, même de celles que l'appariement n'a
            // pas remplies : voir `completer_la_zone`.
            for (i, line) in spell.lines.iter_mut().enumerate() {
                completer_la_zone(line, &format!("{}.lines[{i}]", spell.id), level, &mut report);
            }
            for effect in &mut spell.effects {
                if let Effect::Schedule { id, payload, .. } = effect {
                    for (j, line) in payload.iter_mut().enumerate() {
                        let path = format!("{}.schedule[{id}].payload[{j}]", spell.id);
                        completer_la_zone(line, &path, level, &mut report);
                    }
                }
            }
            for (k, mode) in spell.modes.iter_mut().enumerate() {
                for (j, line) in mode.lines.iter_mut().enumerate() {
                    let path = format!("{}.modes[{k}].lines[{j}]", spell.id);
                    completer_la_zone(line, &path, level, &mut report);
                }
            }
        }

        // Un état sans source dans la donnée (un poison, le plus souvent) prend la zone
        // du sort qui le pose, comme pour son taux de critique.
        let spells = &self.spells;
        for resource in &mut self.resources {
            let Some(level) = spells
                .iter()
                .filter(|s| {
                    s.effects
                        .iter()
                        .any(|e| matches!(e, Effect::Gain { resource: r, .. } if *r == resource.id))
                })
                .find_map(|s| index.get(&s.dofusdb_id?).and_then(|x| x.top()))
            else {
                continue;
            };
            for (i, effect) in resource.while_present.iter_mut().enumerate() {
                for (j, line) in effect.lines.iter_mut().enumerate() {
                    let path = format!("{}.while_present[{i}].lines[{j}]", resource.id);
                    completer_la_zone(line, &path, level, &mut report);
                }
            }
        }

        report
    }
}

/// Complète la zone d'une ligne que l'appariement a laissée sans : sans zone, une
/// ligne frappe une seule cible et ne se dessine pas. La zone vient de ce que la
/// ligne représente dans la donnée (les lignes de son élément, ou les effets de
/// `DAMAGE_EFFECT_IDS`) : une seule zone, c'est la sienne ; plusieurs, celle des
/// effets à sa fourchette si elle est seule ; sinon le fichier de règles doit
/// l'écrire, et le rapport la réclame.
fn completer_la_zone(line: &mut LineDef, path: &str, level: &Level, report: &mut MergeReport) {
    if line.area.is_some() {
        return;
    }
    let effets: &[u32] = if line.best_element {
        &[2822, 2828]
    } else if line.worst_element {
        &[2832]
    } else {
        &[89, 279, 1118]
    };
    debug_assert!(effets.iter().all(|id| DAMAGE_EFFECT_IDS.contains(id)));
    let candidates: Vec<(Option<(i32, i32)>, Zone)> =
        if line.best_element || line.worst_element || line.percent_of_life.is_some() {
            level
                .other_effects
                .iter()
                .filter(|e| effets.contains(&e.id))
                .filter(|e| e.target.as_deref().is_some_and(|t| t.contains('A')))
                .filter_map(|e| {
                    // Une fourchette pour le meilleur élément, un pourcentage
                    // pour la vie : celui-là ne se compare pas.
                    let fourchette = line.percent_of_life.is_none().then(|| {
                        let bas = e.dice_num.unwrap_or(0);
                        (bas, e.dice_side.filter(|h| *h > 0).unwrap_or(bas))
                    });
                    Some((fourchette, e.zone?))
                })
                .collect()
        } else {
            let de = |lignes: &[Line]| -> Vec<(Option<(i32, i32)>, Zone)> {
                lignes
                    .iter()
                    .filter(|l| parse_element(&l.element) == line.element)
                    .filter_map(|l| Some((Some(l.range), l.zone?)))
                    .collect()
            };
            // Un glyphe ou un piège frappe par ce qu'il POSE : ses lignes
            // posées portent sa zone quand le sort n'a pas de ligne directe.
            let directes = de(&level.normal_lines);
            if directes.is_empty() {
                de(&level.placed_lines)
            } else {
                directes
            }
        };
    let seule = |zones: Vec<Zone>| -> Option<Zone> {
        let premiere = *zones.first()?;
        zones.iter().all(|z| *z == premiere).then_some(premiere)
    };
    let zone = seule(candidates.iter().map(|(_, z)| *z).collect()).or_else(|| {
        let Maybe::Known(fourchette) = line.normal else {
            return None;
        };
        seule(
            candidates
                .iter()
                .filter(|(r, _)| *r == Some(fourchette))
                .map(|(_, z)| *z)
                .collect(),
        )
    });
    match zone {
        Some(z) => {
            line.area = Some(crate::AreaDef {
                shape: z.shape,
                size: z.size,
                size2: z.size2,
                max_targets: z.cells,
                falloff_percent: z.falloff_percent,
                falloff_steps: z.falloff_steps,
            });
        }
        None => report.unmatched.push(format!(
            "{path}: zone à écrire, la donnée en porte {} pour cette ligne ({})",
            candidates.len(),
            candidates
                .iter()
                .map(|(_, z)| format!("{}{}", z.shape, z.size.unwrap_or(0)))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Fill one damage line from the snapshot pools, matching by element: a ruleset
/// models one line per element, the snapshot may repeat it per target, in another
/// order.
fn fill_line(
    line: &mut LineDef,
    path: &str,
    normal: &mut Vec<Line>,
    critical: &mut Vec<Line>,
    report: &mut MergeReport,
    can_crit: bool,
) {
    // Une ligne en meilleur ou pire élément ne se remplit pas depuis les lignes : ses
    // bornes vivent sur l'effet 2822 ou 2832, et son élément dépend du build.
    if line.best_element || line.worst_element {
        return;
    }
    // Une ligne en pourcentage de vie n'a pas de fourchette a remplir : ses
    // degats se lisent sur la vie du lanceur, pas dans le vivier.
    if line.sans_fourchette() {
        return;
    }
    // Une ligne de palier (`active_at`) ne se remplit pas depuis le vivier, consommé
    // dans l'ordre : elle se repère par son cran. `authored_magnitudes` vérifie que
    // sa fourchette existe sur le sort.
    if line.active_at.is_some() {
        return;
    }
    // Une ligne qui déclare différer de l'instantané ne se remplit pas, et son
    // désaccord n'est pas signalé : c'est ainsi qu'on écrit l'espérance d'un tirage
    // (le Topkaj).
    if line.differs_from_snapshot.is_some() {
        return;
    }
    let mut zone_vue: Option<Zone> = None;
    let mut take = |pool: &mut Vec<Line>| -> Option<(i32, i32)> {
        let at = pool
            .iter()
            .position(|l| parse_element(&l.element) == line.element)?;
        let l = pool.remove(at);
        // La zone vient de l'instantané comme les magnitudes : elle n'est
        // jamais écrite à la main, et une ligne qui la porte déjà n'est pas
        // écrasée.
        zone_vue = zone_vue.or(l.zone);
        Some(l.range)
    };

    match (take(critical), &line.critical) {
        (Some(range), Maybe::Unknown(_)) => {
            line.critical = Maybe::Known(range);
            report.filled.push(format!("{path}.critical = {range:?}"));
        }
        (Some(range), Maybe::Known(authored)) if *authored != range => {
            report.conflicts.push(Conflict {
                path: format!("{path}.critical"),
                authored: format!("{authored:?}"),
                snapshot: format!("{range:?}"),
            });
        }
        // Un sort qui ne peut pas critiquer n'a pas de ligne critique a
        // reclamer : l'absence est la donnee, pas un trou.
        (None, _) if can_crit => report.unmatched.push(format!(
            "{path}: snapshot has no {:?} critical line left",
            line.element
        )),
        (None, _) => {}
        _ => {}
    }

    let normal_pris = take(normal);
    if line.area.is_none() {
        line.area = zone_vue.map(|z| crate::AreaDef {
            shape: z.shape,
            size: z.size,
            size2: z.size2,
            max_targets: z.cells,
            falloff_percent: z.falloff_percent,
            falloff_steps: z.falloff_steps,
        });
    }
    match (normal_pris, &line.normal) {
        (Some(range), Maybe::Unknown(_)) => {
            line.normal = Maybe::Known(range);
            report.filled.push(format!("{path}.normal = {range:?}"));
        }
        (Some(range), Maybe::Known(authored)) if *authored != range => {
            report.conflicts.push(Conflict {
                path: format!("{path}.normal"),
                authored: format!("{authored:?}"),
                snapshot: format!("{range:?}"),
            });
        }
        (None, _) => report.unmatched.push(format!(
            "{path}: snapshot has no {:?} normal line left",
            line.element
        )),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Coverage
// ---------------------------------------------------------------------------

/// How much of a class is modelled, computed from the snapshot rather than
/// claimed: a solver that has not seen most spells returns a wrong rotation, not
/// an approximate one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Coverage {
    /// Spells in the snapshot that carry at least one damage line.
    pub damaging: usize,
    /// How many of those the ruleset REFERENCES. A referenced spell has its
    /// numbers; it does not necessarily have its mechanic.
    pub modelled: usize,
    /// How many are referenced and carry no declared gap (no open question, no
    /// unknown magnitude): the number a player should read.
    pub fully_modelled: usize,
    /// The referenced-but-incomplete spells, by name, with how many gaps each
    /// still carries. A list, not a number, so the missing part can be worked
    /// through rather than merely counted.
    pub partial: Vec<(String, usize)>,
    /// The ones it does not, by name, so the gap is a list and not a number.
    pub missing: Vec<String>,
    /// Les sorts qui frappent hors de la rotation, par décision, nommés à part (voir
    /// [`crate::SpellDef::outside_rotation`]).
    pub outside_rotation: Vec<String>,
    /// Spells the ruleset models that the snapshot does not know about. Usually
    /// a mistyped `dofusdb_id`.
    pub unknown: Vec<String>,
}

impl Coverage {
    /// True only when every damage-carrying spell is present and complete: a missing
    /// spell is a missing option, an undeclared mechanic a misjudged one.
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.partial.is_empty() && self.damaging > 0
    }

    /// Share of damaging spells that are merely PRESENT, floored. Display only.
    pub fn percent(&self) -> u32 {
        if self.damaging == 0 {
            return 0;
        }
        (self.modelled * 100 / self.damaging) as u32
    }

    /// Share of damaging spells present and complete, floored: the figure to show.
    pub fn percent_complete(&self) -> u32 {
        if self.damaging == 0 {
            return 0;
        }
        (self.fully_modelled * 100 / self.damaging) as u32
    }
}

impl Snapshot {
    /// Which of this class's damage-carrying spells the ruleset models, joined on
    /// `dofusdb_id`, the only stable key (names are localised).
    pub fn coverage(&self, ruleset: &crate::Ruleset) -> Coverage {
        let modelled: std::collections::BTreeSet<u32> =
            ruleset.spells.iter().filter_map(|s| s.dofusdb_id).collect();

        // Combien de trous chaque sort traîne encore. `data_gaps` prefixe chaque
        // chemin par l'identifiant du sort, ce qui suffit a les regrouper.
        let mut trous: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for gap in ruleset.data_gaps() {
            if let Some((prefixe, _)) = gap.path.split_once('.') {
                if let Some(spell) = ruleset.spells.iter().find(|s| s.id == prefixe) {
                    *trous.entry(spell.id.as_str()).or_default() += 1;
                }
            }
        }
        let par_dofusdb: std::collections::BTreeMap<u32, &crate::SpellDef> = ruleset
            .spells
            .iter()
            .filter_map(|s| s.dofusdb_id.map(|d| (d, s)))
            .collect();

        let mut coverage = Coverage::default();
        let mut known = std::collections::BTreeSet::new();

        for spell in &self.spells {
            known.insert(spell.id);
            // A spell counts as damaging if any of its grades carries damage: a line, a best
            // or worst element effect, or damage laid on the ground by a trap or a glyph.
            let damaging = spell.levels.iter().any(|l| {
                !l.normal_lines.is_empty()
                    || !l.critical_lines.is_empty()
                    || !l.placed_lines.is_empty()
                    || !l.placed_critical_lines.is_empty()
                    || l.effect_ids.iter().any(|e| DAMAGE_EFFECT_IDS.contains(e))
                    || l.placed_effect_ids
                        .iter()
                        .any(|e| DAMAGE_EFFECT_IDS.contains(e))
            });
            if !damaging {
                continue;
            }
            if par_dofusdb
                .get(&spell.id)
                .is_some_and(|d| d.outside_rotation.is_some())
            {
                coverage.outside_rotation.push(spell.name.fr.clone());
                continue;
            }
            coverage.damaging += 1;
            if modelled.contains(&spell.id) {
                coverage.modelled += 1;
                // Présent ne veut pas dire compris : un sort dont la mécanique reste déclarée
                // manquante a des dégâts justes et une valeur fausse.
                match par_dofusdb.get(&spell.id).map(|d| trous.get(d.id.as_str())) {
                    Some(Some(&n)) if n > 0 => {
                        coverage.partial.push((spell.name.fr.clone(), n));
                    }
                    _ => coverage.fully_modelled += 1,
                }
            } else {
                coverage.missing.push(spell.name.fr.clone());
            }
        }

        for spell in &ruleset.spells {
            match spell.dofusdb_id {
                Some(id) if !known.contains(&id) => coverage
                    .unknown
                    .push(format!("{} (dofusdb_id {id})", spell.id)),
                None => coverage
                    .unknown
                    .push(format!("{} (no dofusdb_id)", spell.id)),
                _ => {}
            }
        }
        coverage.missing.sort();
        coverage.unknown.sort();
        coverage.outside_rotation.sort();
        // Les plus troues d'abord : c'est par la qu'il faut commencer.
        coverage
            .partial
            .sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        coverage
    }
}
