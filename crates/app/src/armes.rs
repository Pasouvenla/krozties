//! L'arme du build, lancée comme un sort dans la rotation.
//!
//! Ses chiffres viennent du catalogue (`dofus_build::Weapon`), sa zone de son
//! type (`data/snapshots/types-d-arme.json`). Le calcul :
//!
//! * le critique de l'arme s'ajoute à celui du build ; une arme à 0 % ne
//!   critique pas ;
//! * sur un coup critique, chaque jet gagne le bonus critique de l'arme, avant
//!   la caractéristique ;
//! * la Maîtrise d'arme ajoute sa Puissance à celle du build : 300 par défaut,
//!   360 en maîtrise critique, 0 sans, réglée dans la Rotation ;
//! * les % de dommages d'armes remplacent ceux des sorts, à côté des % finaux
//!   et de ceux de mêlée ou de distance ;
//! * un bâton, une lance, un marteau ou une pelle qui frappe à distance perd
//!   10 % ;
//! * le vol frappe comme des dégâts ; le soin ne frappe pas.

use std::sync::OnceLock;

use dofus_build::{ArmePortee, Resolved, WeaponLineKind};
use dofus_engine::Reach;
use dofus_ruleset::SpellDef;

/// L'identifiant de l'arme dans le deck.
pub const ID: &str = "arme";

/// La Maîtrise d'arme quand le joueur ne dit rien : la normale.
pub const MAITRISE_PAR_DEFAUT: i32 = 300;

/// Bâton, lance, marteau et pelle : à distance, ils perdent 10 %.
const MALUS_A_DISTANCE: [u32; 4] = [4, 271, 7, 8];

#[derive(serde::Deserialize)]
struct Types {
    types: Vec<TypeDArme>,
}

#[derive(serde::Deserialize)]
struct TypeDArme {
    id: u32,
    name: String,
    zone: Zone,
}

#[derive(serde::Deserialize)]
struct Zone {
    shape: String,
    size: u8,
    size2: u8,
    cells: Option<u16>,
    falloff_percent: u8,
    falloff_steps: u8,
}

fn types() -> &'static [TypeDArme] {
    static TYPES: OnceLock<Vec<TypeDArme>> = OnceLock::new();
    TYPES.get_or_init(|| {
        serde_json::from_str::<Types>(crate::donnees::TYPES_D_ARME)
            .map(|t| t.types)
            .unwrap_or_default()
    })
}

fn type_d_arme(id: u32) -> Option<&'static TypeDArme> {
    types().iter().find(|t| t.id == id)
}

/// L'arme frappe-t-elle à distance, dans ce scénario : sa portée le décide,
/// sinon la position du joueur, comme pour un sort.
fn a_distance(portee: &ArmePortee, reach: Reach) -> bool {
    let [min, max] = portee.arme.range;
    dofus_engine::reach_for(Some((min, max)), reach) == Reach::Ranged
}

/// L'arme du build en sort, prête pour le moteur, ou rien si le build n'en
/// porte pas, ou porte une arme qui ne frappe pas.
pub fn sort(resolu: &Resolved, maitrise: i32, reach: Reach) -> Option<SpellDef> {
    let portee = resolu.arme.as_ref()?;
    let a = &portee.arme;
    let frappent: Vec<_> = a.lines.iter().filter(|l| l.kind != WeaponLineKind::Heal).collect();
    if frappent.is_empty() {
        return None;
    }
    let malus = a_distance(portee, reach) && MALUS_A_DISTANCE.contains(&a.type_id);
    let zone = type_d_arme(a.type_id).map(|t| &t.zone).filter(|z| z.shape != "P");
    let lignes: Vec<serde_json::Value> = frappent
        .iter()
        .map(|l| {
            let mut ligne = serde_json::json!({
                "normal": [l.min, l.max],
                "critical": [l.min + a.crit_bonus, l.max + a.crit_bonus],
                "puissance_propre": maitrise,
            });
            if l.element == "best" {
                ligne["best_element"] = serde_json::json!(true);
            } else {
                ligne["element"] = serde_json::json!(l.element);
            }
            if malus {
                ligne["facteur"] = serde_json::json!(90);
            }
            if let Some(z) = zone {
                ligne["area"] = serde_json::json!({
                    "shape": z.shape, "size": z.size, "size2": z.size2, "max_targets": z.cells,
                    "falloff_percent": z.falloff_percent, "falloff_steps": z.falloff_steps,
                });
            }
            ligne
        })
        .collect();
    let poussees: Vec<serde_json::Value> = a
        .effects
        .iter()
        .filter(|e| e.kind == "push" && e.max > 0)
        .map(|e| serde_json::json!({ "cells": e.max }))
        .collect();
    let mut etiquettes = Vec::new();
    if a.effects.iter().any(|e| e.kind == "ap_removal") {
        etiquettes.push("removes_ap");
    }
    if a.effects.iter().any(|e| matches!(e.kind.as_str(), "mp_removal" | "mp_steal")) {
        etiquettes.push("removes_mp");
    }
    serde_json::from_value(serde_json::json!({
        "id": ID,
        "name": { "fr": portee.nom, "en": portee.nom },
        "ap_cost": { "base": a.ap },
        // Zéro dit « sans limite » ; les règles l'écrivent 6, les PA bornant
        // de toute façon le nombre de coups.
        "casts_per_turn": if a.casts_per_turn == 0 { 6 } else { a.casts_per_turn },
        "crit": { "base_rate": a.crit_rate, "can_crit": a.crit_rate > 0 },
        "range": [a.range[0], a.range[1]],
        // ⚠️ La portée d'une arme ne se modifie pas : la Portée du build ne
        // l'allonge pas.
        "cast": {
            "in_line": a.in_line, "in_diagonal": a.in_diagonal,
            "needs_line_of_sight": a.line_of_sight, "range_boostable": false,
        },
        "lines": lignes,
        "effects": [{ "effect": "damage" }],
        "pushes": poussees,
        "tags": etiquettes,
        "arme": true,
    }))
    .ok()
}

/// Ce que la Rotation dit de l'arme quand elle est au deck.
pub fn hypotheses(resolu: &Resolved, maitrise: i32, reach: Reach) -> Vec<String> {
    let Some(portee) = resolu.arme.as_ref() else { return Vec::new() };
    let mut notes = vec![if maitrise > 0 {
        format!("{} : Maîtrise d'arme comptée, {maitrise} Puissance sur ses coups", portee.nom)
    } else {
        format!("{} : sans Maîtrise d'arme", portee.nom)
    }];
    let type_d_arme = type_d_arme(portee.arme.type_id);
    if a_distance(portee, reach) && MALUS_A_DISTANCE.contains(&portee.arme.type_id) {
        let nom = type_d_arme.map_or("arme", |t| t.name.as_str());
        notes.push(format!("{} frappé à distance : 10 % de dégâts en moins", nom.to_lowercase()));
    }
    notes.extend(portee.forgemagie.iter().map(|f| format!("{} : {f}", portee.nom)));
    notes
}

/// La fiche de l'arme pour l'interface, avec les champs d'un sort du catalogue
/// (`classes_json`) : le deck et l'infobulle la lisent comme un sort.
pub fn fiche(resolu: &Resolved) -> Option<serde_json::Value> {
    let portee = resolu.arme.as_ref()?;
    let def = sort(resolu, MAITRISE_PAR_DEFAUT, Reach::Melee)?;
    let a = &portee.arme;
    let nom_element = |e: &str| match e {
        "fire" => "Fire",
        "earth" => "Earth",
        "water" => "Water",
        "air" => "Air",
        "neutral" => "Neutral",
        _ => "best",
    };
    let frappent: Vec<_> = a.lines.iter().filter(|l| l.kind != WeaponLineKind::Heal).collect();
    let mut elements: Vec<&str> =
        frappent.iter().map(|l| nom_element(&l.element)).filter(|e| *e != "best").collect();
    elements.sort_unstable();
    elements.dedup();
    let icones: std::collections::BTreeMap<String, u32> =
        serde_json::from_str(crate::donnees::ICONES_D_OBJETS).unwrap_or_default();
    // Ce que l'arme fait d'autre, dans les mots du jeu.
    let mut description = vec![type_d_arme(a.type_id).map_or_else(|| "Arme".to_string(), |t| t.name.clone())];
    for l in a.lines.iter().filter(|l| l.kind == WeaponLineKind::Heal) {
        description.push(format!("Soigne de {} à {}", l.min, l.max));
    }
    for e in &a.effects {
        let valeur = if e.min == e.max { e.min.to_string() } else { format!("{} à {}", e.min, e.max) };
        let pluriel = if e.max > 1 { "s" } else { "" };
        description.push(match e.kind.as_str() {
            "push" => format!("Repousse de {valeur} case{pluriel}"),
            "pull" => format!("Attire de {valeur} case{pluriel}"),
            "advance" => format!("Avance de {valeur} case{pluriel}"),
            "ap_removal" => format!("Retire {valeur} PA"),
            "mp_removal" => format!("Retire {valeur} PM"),
            "mp_steal" => format!("Vole {valeur} PM"),
            autre => autre.to_string(),
        });
    }
    if let Some(z) = type_d_arme(a.type_id).map(|t| &t.zone).filter(|z| z.shape != "P") {
        description.push(format!("Frappe une zone de {} cases", z.cells.unwrap_or(1)));
    }
    if MALUS_A_DISTANCE.contains(&a.type_id) {
        description.push("Perd 10 % de dégâts à distance".to_string());
    }
    description.extend(portee.forgemagie.iter().map(|f| majuscule(f)));
    description.push(format!("Dégâts avec la Maîtrise d'arme normale ({MAITRISE_PAR_DEFAUT} Puissance)"));
    Some(serde_json::json!({
        "id": ID,
        "dofusdb_id": null,
        "icone_objet": icones.get(&portee.id.to_string()),
        "arme": true,
        "orientation": {
            "elements": elements,
            "meilleur": frappent.iter().any(|l| l.element == "best"),
            "agit": true,
        },
        "variant_group": null,
        "name": portee.nom,
        "ap": a.ap,
        "casts_per_turn": def.casts_per_turn,
        "cooldown": 0,
        "elements": elements,
        "open_questions": [],
        "assumptions": [],
        "description": description.join("\n"),
        "critical_effects": [],
        "crit_base": a.crit_rate,
        "can_crit": a.crit_rate > 0,
        "range": def.range,
        "cast": def.cast,
        "casts_per_target": null,
        "lines": frappent.iter().map(|l| serde_json::json!({
            "element": nom_element(&l.element),
            "normal": [l.min, l.max],
            "critical": [l.min + a.crit_bonus, l.max + a.crit_bonus],
        })).collect::<Vec<_>>(),
    }))
}

/// L'icône de l'objet arme, pour les lignes de la Rotation qui la nomment.
pub fn icone(resolu: &Resolved) -> Option<u32> {
    let portee = resolu.arme.as_ref()?;
    let icones: std::collections::BTreeMap<String, u32> =
        serde_json::from_str(crate::donnees::ICONES_D_OBJETS).ok()?;
    icones.get(&portee.id.to_string()).copied()
}

fn majuscule(texte: &str) -> String {
    let mut c = texte.chars();
    c.next().map_or_else(String::new, |p| p.to_uppercase().chain(c).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solve::resolve_build;
    use dofus_build::BuildInput;

    fn porte(classe: u32, arme: u32, fm: Option<&str>) -> Resolved {
        let mut build = BuildInput { class: classe, level: 200, ..Default::default() };
        build.items = vec![0; 17];
        build.items[8] = arme;
        build.fm_weapon = fm.map(str::to_string);
        resolve_build(&build).expect("le build")
    }

    /// La Hachebarde de Guerre devient un sort : 5 PA, portée 1 à 2, 15 % de
    /// critique, la Maîtrise sur ses lignes, les % d'armes. En 3.7, ses deux lignes
    /// frappent dans le meilleur élément, 37 à 43 et un vol de vie de 10 à 13,
    /// chacune +10 en critique.
    #[test]
    fn la_hachebarde_devient_un_sort() {
        let resolu = porte(18, 22368, None);
        let s = sort(&resolu, 300, Reach::Melee).expect("l'arme");
        assert!(s.arme);
        assert_eq!((s.ap_cost.base, s.range, s.casts_per_turn), (5, Some((1, 2)), 1));
        assert_eq!(s.crit.base_rate.known().copied(), Some(15));
        let lignes: Vec<String> = s
            .lines
            .iter()
            .map(|l| format!("{:?} {} {:?} {:?}", l.element, l.best_element, l.normal, l.critical))
            .collect();
        assert_eq!(lignes, ["None true Known((37, 43)) Known((47, 53))", "None true Known((10, 13)) Known((20, 23))"]);
        assert!(s.lines.iter().all(|l| l.puissance_propre == 300));
        assert!(s.lines[0].area.is_none(), "une hache frappe une case");
    }

    /// Un marteau frappe en croix, cinq cases à 10 % de dégressivité.
    #[test]
    fn le_marteau_frappe_en_croix() {
        let catalogue = dofus_build::Catalogue::from_json(crate::donnees::OBJETS).unwrap();
        let marteau = catalogue.items.iter().find(|o| o.weapon.as_ref().is_some_and(|w| w.type_id == 7 && !w.lines.is_empty())).unwrap();
        let resolu = porte(8, marteau.id, None);
        let zone = sort(&resolu, 300, Reach::Melee).unwrap().lines[0].area.expect("la zone du marteau");
        assert_eq!((zone.shape, zone.size, zone.max_targets, zone.falloff_percent), ('X', Some(1), Some(5), 10));
    }

    /// La forgemagie d'arme d'un build importé : « df-85 » passe les dégâts Neutre
    /// en Feu, arrondis comme `Math.round`. En 3.7, une potion à 85 % convertit à
    /// 100 %, une à 50 % à 10 % : l'Épée de Boisaille frappe 8 à 10 Neutre, soit 8
    /// à 10 Feu, ou 1 à 1 Eau (0,8 et 1 arrondis).
    #[test]
    fn la_forgemagie_d_arme_change_l_element() {
        let resolu = porte(8, 44, Some("df-85"));
        let ligne = &resolu.arme.as_ref().unwrap().arme.lines[0];
        assert_eq!((ligne.element.as_str(), ligne.min, ligne.max), ("fire", 8, 10));
        let resolu = porte(8, 44, Some("de-50"));
        let ligne = &resolu.arme.as_ref().unwrap().arme.lines[0];
        assert_eq!((ligne.element.as_str(), ligne.min, ligne.max), ("water", 1, 1));
        let resolu = porte(8, 44, Some("de-100"));
        let ligne = &resolu.arme.as_ref().unwrap().arme.lines[0];
        assert_eq!((ligne.element.as_str(), ligne.min, ligne.max), ("water", 8, 10));
    }

    /// Un bâton qui frappe à distance perd 10 % ; au corps à corps, rien.
    /// Quatre-vingt-treize bâtons portent à deux cases ou plus.
    #[test]
    fn le_baton_a_distance_perd_dix_pour_cent() {
        let catalogue = dofus_build::Catalogue::from_json(crate::donnees::OBJETS).unwrap();
        let baton = catalogue
            .items
            .iter()
            .find(|o| o.weapon.as_ref().is_some_and(|w| w.type_id == 4 && w.range == [1, 2] && !w.lines.is_empty()))
            .expect("un bâton qui porte de 1 à 2 cases");
        let resolu = porte(8, baton.id, None);
        let facteur = |r| sort(&resolu, 300, r).unwrap().lines[0].facteur;
        assert_eq!(facteur(Reach::Ranged), Some(90), "{}", baton.name);
        assert_eq!(facteur(Reach::Melee), None, "{}", baton.name);
    }
}
