//! Les sorts temporaires des objets, au deck de la Rotation.
//!
//! Un objet peut ajouter un sort à la barre de son porteur, l'effet 722 de ses
//! effets possibles : la Pelle Fantomatique de l'Épée Nécronyx, l'Assasindic de
//! l'Attrape-Cauchemar, le Camouflage des catacombres du Dofus Verdoyant. Ses
//! chiffres viennent du relevé des sorts d'objet
//! (`data/snapshots/sorts-d-objets.json`), au grade que l'objet donne. Un sort
//! qui ne frappe pas au lancer et ne renforce pas son lanceur n'entre pas au
//! deck ; les remarques du calcul le disent.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use dofus_build::Resolved;
use dofus_ruleset::snapshot::{Level, Line, PlacedEffect, Snapshot, Spell};
use dofus_ruleset::{ResourceDef, Ruleset, SpellDef};
use serde_json::{json, Value};

/// Le début de l'identifiant de chaque sort d'objet, suivi de celui du sort.
pub const PREFIXE: &str = "sort_d_objet_";

fn releve() -> Option<&'static Snapshot> {
    static RELEVE: OnceLock<Option<Snapshot>> = OnceLock::new();
    RELEVE.get_or_init(|| Snapshot::from_json(crate::donnees::SORTS_D_OBJETS).ok()).as_ref()
}

/// Un sort du relevé, par son identifiant.
pub fn sort_du_releve(id: u32) -> Option<&'static Spell> {
    releve()?.spells.iter().find(|s| s.id == id)
}

/// L'identifiant d'un sort d'objet dans les règles.
pub fn id(sort: u32) -> String {
    format!("{PREFIXE}{sort}")
}

/// Un sort temporaire d'un objet porté, au grade que l'objet donne.
struct Porte {
    objet: u32,
    nom_objet: String,
    sort: &'static Spell,
    niveau: &'static Level,
    /// Le grade que l'objet demande, quand la donnée ne le porte pas : le plus
    /// haut grade inférieur le remplace. Le Dofus Verdoyant demande le grade 2
    /// du Camouflage des catacombres, qui n'en a qu'un.
    grade_absent: Option<u8>,
}

/// Ce qu'un sort pose sur son lanceur et qui change ses dégâts : le
/// modificateur, et la durée que le jeu donne.
fn renfort(e: &PlacedEffect) -> Option<(Value, u8)> {
    let sur_le_lanceur = e.target.as_deref().is_some_and(|t| t.split(',').any(|j| j == "C"));
    let duree = u8::try_from(e.duration.unwrap_or(0)).ok().filter(|d| *d > 0)?;
    if !sur_le_lanceur {
        return None;
    }
    let n = e.dice_num.unwrap_or(0);
    let modificateur = match e.id {
        115 => json!({ "kind": "critical_rate", "percent": n }),
        138 => json!({ "kind": "characteristic", "amount": n }),
        112 => json!({ "kind": "flat_damage", "amount": n }),
        1171 => json!({ "kind": "final_multiplier", "percent": 100 + n, "finaux": true }),
        414 => json!({ "kind": "push_damage", "amount": n }),
        _ => return None,
    };
    Some((modificateur, duree))
}

/// Ce que le sort lance fait, dans les mots du jeu, pour sa fiche.
fn libelle(e: &PlacedEffect) -> Option<String> {
    let n = e.dice_num.unwrap_or(0);
    let duree = e.duration.unwrap_or(0);
    let pendant = match duree {
        d if d > 1 => format!(" pendant {d} tours"),
        1 => " pendant 1 tour".to_string(),
        _ => String::new(),
    };
    Some(match e.id {
        115 => format!("+{n} % de critique{pendant}"),
        138 => format!("+{n} Puissance{pendant}"),
        112 => format!("+{n} Dommages{pendant}"),
        1171 => format!("+{n} % de dommages finaux{pendant}"),
        414 => format!("+{n} Dommages Poussée{pendant}"),
        128 => format!("+{n} PM{pendant}"),
        150 => format!("Rend invisible{pendant}"),
        _ => return None,
    })
}

/// Les sorts temporaires des objets portés, et ceux que le relevé n'a pas,
/// avec l'objet qui les porte (un sort du Ménologium béni, 10141).
fn portes_et_manquants(resolu: &Resolved) -> (Vec<Porte>, Vec<(String, u32)>) {
    let mut vus = Vec::new();
    let mut out = Vec::new();
    let mut manquants = Vec::new();
    for (objet, nom_objet, s) in resolu.sorts_d_objets.iter().filter(|(_, _, s)| s.temporary) {
        if vus.contains(&s.spell) {
            continue;
        }
        vus.push(s.spell);
        if sort_du_releve(s.spell).is_none() {
            manquants.push((nom_objet.clone(), s.spell));
            continue;
        }
        out.extend(porte(*objet, nom_objet, s));
    }
    (out, manquants)
}

/// Un sort d'objet au grade que l'objet donne, ou au plus haut grade inférieur
/// que la donnée porte.
fn porte(objet: u32, nom_objet: &str, s: &dofus_build::ItemSpell) -> Option<Porte> {
    let sort = sort_du_releve(s.spell)?;
    let niveau = sort
        .levels
        .iter()
        .filter(|l| l.grade.is_some_and(|g| g <= s.grade))
        .max_by_key(|l| l.grade)
        .or_else(|| sort.levels.first())?;
    let grade_absent = (niveau.grade != Some(s.grade)).then_some(s.grade);
    Some(Porte { objet, nom_objet: nom_objet.to_string(), sort, niveau, grade_absent })
}

/// Le texte du jeu, sans le nom du sort qu'il répète en tête, une ligne par
/// puce : « Jaune Ocre : • À chaque début de tour… » devient « À chaque début
/// de tour… ».
fn texte_du_jeu(nom: &str, texte: &str) -> String {
    let texte = texte.trim();
    let texte = texte.strip_prefix(&format!("{nom} :")).unwrap_or(texte);
    texte.split('•').map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

/// Ce que la fiche d'un objet dit de ses sorts : son effet spécial et le sort
/// qu'il ajoute à la barre, leur texte, et ce que la Rotation en fait avec les
/// objets `portes` du build.
pub fn sur_la_fiche(item: &dofus_build::Item, portes: &[u32]) -> Vec<Value> {
    item.spells
        .iter()
        .map(|s| {
            let releve = sort_du_releve(s.spell);
            let nom = releve.map_or_else(|| format!("sort {}", s.spell), |r| r.name.fr.clone());
            let mut texte = texte_du_jeu(&nom, releve.and_then(|r| r.description_fr.as_deref()).unwrap_or(""));
            if s.temporary {
                let p = porte(item.id, &item.name, s);
                let au_deck = p.as_ref().is_some_and(|p| regles_du_sort(p).is_some());
                for l in p.iter().flat_map(|p| p.niveau.other_effects.iter().filter_map(libelle)) {
                    if !texte.is_empty() {
                        texte.push('\n');
                    }
                    texte.push_str(&l);
                }
                json!({
                    "genre": "sort_ajoute", "nom": nom, "texte": texte, "compte": au_deck,
                    "statut": if au_deck { "au deck de la Rotation" } else { "hors du deck" },
                })
            } else {
                let compte =
                    crate::effets_d_objets::modelise(item.id, portes) || dofus_build::effet_de_dofus_compte(item.id);
                // Sans texte dans la donnée, ce que les remarques du calcul en
                // disent.
                if texte.is_empty() {
                    texte = crate::effets_d_objets::sans_effet(item.id).unwrap_or_default();
                }
                json!({
                    "genre": "effet_special", "nom": nom, "texte": texte, "compte": compte,
                    "statut": if compte { "compté dans la Rotation" } else { "sans effet sur vos dégâts" },
                })
            }
        })
        .collect()
}

fn portes(resolu: &Resolved) -> Vec<Porte> {
    portes_et_manquants(resolu).0
}

fn ligne(normale: &Line, critique: Option<&Line>) -> Value {
    let mut l = json!({
        "element": normale.element,
        "normal": [normale.range.0, normale.range.1],
        "critical": critique.map_or([normale.range.0, normale.range.1], |c| [c.range.0, c.range.1]),
    });
    if let Some(z) = normale.zone.filter(|z| z.shape != 'P') {
        l["area"] = json!({
            "shape": z.shape, "size": z.size, "size2": z.size2, "max_targets": z.cells,
            "falloff_percent": z.falloff_percent, "falloff_steps": z.falloff_steps,
        });
    }
    l
}

/// Le sort et le compteur de son renfort, s'il en a un ; rien s'il ne fait
/// rien dans le modèle.
fn regles_du_sort(p: &Porte) -> Option<(SpellDef, Option<ResourceDef>)> {
    let n = p.niveau;
    let lignes: Vec<Value> = n
        .normal_lines
        .iter()
        .enumerate()
        .map(|(i, l)| ligne(l, n.critical_lines.get(i)))
        .collect();
    let renforts: Vec<(Value, u8)> = n.other_effects.iter().filter_map(renfort).collect();
    if lignes.is_empty() && renforts.is_empty() {
        return None;
    }
    let id = id(p.sort.id);
    let mut effets = Vec::new();
    if !lignes.is_empty() {
        effets.push(json!({ "effect": "damage" }));
    }
    let compteur = if renforts.is_empty() {
        None
    } else {
        // La règle des durées : un effet de `d` tours lancé au tour N
        // tient jusqu'au tour N + d - 1.
        let duree = renforts.iter().map(|(_, d)| *d).max().unwrap_or(1);
        let etat = format!("{id}_effet");
        effets.push(json!({ "effect": "gain", "resource": etat }));
        serde_json::from_value(json!({
            "id": etat, "scope": "caster", "max": 1, "default": 0,
            "duration": { "turns": duree - 1, "refresh": "on_apply", "on_expire": "reset_to_default" },
            "modifies_damage": renforts.iter().map(|(m, _)| m.clone()).collect::<Vec<_>>(),
        }))
        .ok()
    };
    let [min, max] = n.range.unwrap_or([Some(0), Some(0)]);
    let par_tour = n.max_cast_per_turn.unwrap_or(0);
    let sort = serde_json::from_value(json!({
        "id": id,
        "name": { "fr": p.sort.name.fr, "en": p.sort.name.fr },
        "ap_cost": { "base": n.ap_cost.unwrap_or(0) },
        // Zéro dit « sans limite » ; les règles l'écrivent 6, comme pour
        // l'arme, les PA bornant de toute façon le nombre de lancers.
        "casts_per_turn": if par_tour == 0 { 6 } else { par_tour },
        "casts_per_target": n.max_cast_per_target.filter(|c| *c > 0),
        "cooldown_turns": n.min_cast_interval.unwrap_or(0),
        "crit": { "base_rate": n.base_crit_percent.unwrap_or(0), "can_crit": !n.critical_lines.is_empty() },
        "range": [min.unwrap_or(0), max.unwrap_or(0)],
        "cast": n.cast,
        "lines": lignes,
        "effects": effets,
    }))
    .ok()?;
    Some((sort, compteur))
}

/// Greffe aux règles les sorts temporaires des objets portés, et rend les
/// remarques du calcul.
pub fn greffer(regles: &mut Ruleset, resolu: &Resolved) -> Vec<String> {
    let mut remarques = Vec::new();
    let (portes, manquants) = portes_et_manquants(resolu);
    // Par objet : les sorts qui ne font rien ici, puis ceux que le relevé n'a
    // pas.
    let mut dehors: BTreeMap<String, (Vec<String>, Vec<u32>)> = BTreeMap::new();
    for (objet, sort) in manquants {
        dehors.entry(objet).or_default().1.push(sort);
    }
    for p in portes {
        let Some((sort, compteur)) = regles_du_sort(&p) else {
            let noms = &mut dehors.entry(p.nom_objet.clone()).or_default().0;
            if !noms.contains(&p.sort.name.fr) {
                noms.push(p.sort.name.fr.clone());
            }
            continue;
        };
        if let Some(c) = compteur {
            if regles.resources.len() >= dofus_engine::MAX_RESOURCES {
                remarques.push(format!(
                    "{} n'a pas trouvé de place sous le plafond de compteurs du moteur : la rotation s'en passe",
                    p.sort.name.fr
                ));
                continue;
            }
            regles.resources.push(c);
        }
        if let Some(g) = p.grade_absent {
            remarques.push(format!(
                "{} : {} demandé au grade {g}, que la donnée n'a pas ; le grade {} est compté",
                p.nom_objet,
                p.sort.name.fr,
                p.niveau.grade.unwrap_or(1)
            ));
        }
        regles.spells.push(sort);
    }
    for (objet, (noms, absents)) in dehors {
        let mut phrase = match noms.len() {
            0 => String::new(),
            1 => format!("{} ne frappe pas au lancer et ne renforce pas son lanceur", noms[0]),
            _ => format!("{} ne frappent pas au lancer et ne renforcent pas leur lanceur", noms.join(", ")),
        };
        if !absents.is_empty() {
            let liste = absents.iter().map(u32::to_string).collect::<Vec<_>>().join(", ");
            let pluriel = if absents.len() > 1 { "s" } else { "" };
            let suite = format!("sort{pluriel} {liste} absent{pluriel} du relevé");
            phrase = if phrase.is_empty() { suite } else { format!("{phrase} ; {suite}") };
        }
        remarques.push(format!("{objet} : {phrase}, hors du deck"));
    }
    remarques
}

/// Les sorts d'objet seuls, prêts pour le moteur, sans leurs compteurs : pour
/// les noms et les tables de dégâts.
pub fn sorts(resolu: &Resolved) -> Vec<SpellDef> {
    portes(resolu).iter().filter_map(regles_du_sort).map(|(s, _)| s).collect()
}

fn icones() -> &'static BTreeMap<String, u32> {
    static ICONES: OnceLock<BTreeMap<String, u32>> = OnceLock::new();
    ICONES.get_or_init(|| serde_json::from_str(crate::donnees::ICONES_D_OBJETS).unwrap_or_default())
}

/// L'icône de l'objet qui porte ce sort, pour les lignes de la Rotation.
pub fn icone(resolu: &Resolved, id_du_sort: &str) -> Option<u32> {
    let sort: u32 = id_du_sort.strip_prefix(PREFIXE)?.parse().ok()?;
    let (objet, _, _) = resolu.sorts_d_objets.iter().find(|(_, _, s)| s.spell == sort)?;
    icones().get(&objet.to_string()).copied()
}

/// Les fiches des sorts d'objet pour l'interface, avec les champs d'un sort du
/// catalogue (`classes_json`) : le deck et l'infobulle les lisent comme un
/// sort, comme l'arme.
pub fn fiches(resolu: &Resolved) -> Vec<Value> {
    let nom_element = |e: &str| match e {
        "fire" => "Fire",
        "earth" => "Earth",
        "water" => "Water",
        "air" => "Air",
        _ => "Neutral",
    };
    portes(resolu)
        .iter()
        .filter_map(|p| {
            let (def, _) = regles_du_sort(p)?;
            let n = p.niveau;
            let mut elements: Vec<&str> = n.normal_lines.iter().map(|l| nom_element(&l.element)).collect();
            elements.sort_unstable();
            elements.dedup();
            let mut description = vec![format!("Sort de l'objet {}", p.nom_objet)];
            description.extend(p.sort.description_fr.iter().filter(|d| !d.is_empty()).cloned());
            description.extend(n.other_effects.iter().filter_map(libelle));
            Some(json!({
                "id": def.id,
                "dofusdb_id": null,
                "icone_objet": icones().get(&p.objet.to_string()),
                "objet": p.objet,
                "orientation": { "elements": elements, "meilleur": false, "agit": true },
                "variant_group": null,
                "name": p.sort.name.fr,
                "ap": def.ap_cost.base,
                "casts_per_turn": def.casts_per_turn,
                "cooldown": def.cooldown_turns,
                "elements": elements,
                "open_questions": [],
                "assumptions": [],
                "description": description.join("\n"),
                "critical_effects": [],
                "crit_base": n.base_crit_percent.unwrap_or(0),
                "can_crit": def.crit.can_crit,
                "range": def.range,
                "cast": def.cast,
                "casts_per_target": def.casts_per_target,
                "lines": n.normal_lines.iter().enumerate().map(|(i, l)| {
                    let c = n.critical_lines.get(i).unwrap_or(l);
                    json!({
                        "element": nom_element(&l.element),
                        "normal": [l.range.0, l.range.1],
                        "critical": [c.range.0, c.range.1],
                    })
                }).collect::<Vec<_>>(),
            }))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solve::resolve_build;
    use dofus_build::BuildInput;

    fn porte(classe: u32, objet: u32, place: usize) -> Resolved {
        let mut build = BuildInput { class: classe, level: 200, ..Default::default() };
        build.items = vec![0; 17];
        build.items[place] = objet;
        resolve_build(&build).expect("le build")
    }

    /// L'Épée Nécronyx donne la Pelle Fantomatique au grade 1 : 17 à 19 Feu,
    /// 19 à 21 en critique, 3 PA, à 4 cases exactement, deux fois par tour et
    /// une par cible.
    #[test]
    fn l_epee_necronyx_donne_la_pelle_fantomatique() {
        let resolu = porte(8, 19482, 8);
        let mut regles = crate::solve::load_ruleset(8).unwrap();
        let remarques = greffer(&mut regles, &resolu);
        assert!(remarques.is_empty(), "{remarques:?}");
        let pelle = regles.spells.iter().find(|s| s.id == id(2056)).expect("la Pelle");
        assert_eq!(pelle.ap_cost.base, 3);
        assert_eq!(pelle.range, Some((4, 4)));
        assert_eq!((pelle.casts_per_turn, pelle.casts_per_target), (2, Some(1)));
        assert!(pelle.crit.can_crit);
        let l = &pelle.lines[0];
        assert_eq!((l.normal.known().copied(), l.critical.known().copied()), (Some((17, 19)), Some((19, 21))));
    }

    /// L'Assasindic ne critique pas : sa donnée n'a aucune ligne critique.
    #[test]
    fn l_assasindic_ne_critique_pas() {
        let resolu = porte(8, 21878, 8);
        let s = sorts(&resolu);
        let a = s.iter().find(|s| s.id == id(12404)).expect("l'Assasindic");
        assert!(!a.crit.can_crit);
        assert_eq!((a.ap_cost.base, a.range, a.casts_per_turn), (4, Some((1, 1)), 1));
    }

    /// Le Camouflage des catacombres pose +10 % de critique pour trois tours
    /// (deux après le tour du lancer), relancé tous les cinq tours ; le grade
    /// 2 que le Dofus Verdoyant demande n'existe pas, le 1 le remplace. Les
    /// sorts du Ménologium béni ne font rien au lancer.
    #[test]
    fn le_camouflage_renforce_et_le_menologium_reste_dehors() {
        let resolu = porte(8, 29135, 15);
        let mut regles = crate::solve::load_ruleset(8).unwrap();
        let avant = regles.resources.len();
        let remarques = greffer(&mut regles, &resolu);
        assert_eq!(regles.resources.len(), avant + 1);
        let etat = regles.resources.last().unwrap();
        assert_eq!(etat.duration.as_ref().map(|d| d.turns), Some(2));
        let camouflage = regles.spells.iter().find(|s| s.id == id(10169)).expect("le Camouflage");
        assert_eq!((camouflage.ap_cost.base, camouflage.cooldown_turns), (2, 5));
        assert!(remarques.iter().any(|r| r.contains("grade 2")), "{remarques:?}");

        let menologium = porte(8, 29133, 15);
        let mut regles = crate::solve::load_ruleset(8).unwrap();
        let remarques = greffer(&mut regles, &menologium);
        assert!(regles.spells.iter().all(|s| !s.id.starts_with(PREFIXE)));
        assert!(remarques.iter().any(|r| r.contains("hors du deck")), "{remarques:?}");
    }

    /// Un Crâ qui porte l'objet en `place` rejoue ces tours : les dégâts de
    /// chaque lancer du premier tour.
    fn rejoue(objet: u32, place: usize, deck: &[&str], tour: &[&str]) -> Vec<f64> {
        let mut items = vec![0; 17];
        items[place] = objet;
        let requete: crate::solve::Request = serde_json::from_value(json!({
            "class": 9, "level": 200, "items": items,
            "invested": { "intelligence": 300, "agility": 300 },
            "deck": deck, "horizon": 1,
        }))
        .unwrap();
        let tours = vec![tour.iter().map(|s| s.to_string()).collect::<Vec<_>>()];
        crate::solve::rejouer(&requete, &tours).unwrap().turns[0].casts.iter().map(|c| c.damage.as_f64()).collect()
    }

    /// La Pelle Fantomatique frappe dans la rotation, une fois par cible.
    #[test]
    fn la_pelle_frappe_dans_la_rotation() {
        let pelle = id(2056);
        let d = rejoue(19482, 8, &[&pelle], &[&pelle]);
        assert!(d[0] > 0.0, "{d:?}");
    }

    /// Le Camouflage des catacombres lancé avant la Flèche Évasive lui donne
    /// 10 % de critique de plus.
    #[test]
    fn le_camouflage_releve_les_coups_qui_suivent() {
        let camouflage = id(10169);
        let sans = rejoue(29135, 15, &[&camouflage, "fleche_evasive"], &["fleche_evasive"]);
        let avec = rejoue(29135, 15, &[&camouflage, "fleche_evasive"], &[&camouflage, "fleche_evasive"]);
        assert!(avec[1] > sans[0], "{sans:?} {avec:?}");
    }

    fn fiche(objet: u32) -> Vec<Value> {
        let catalogue = dofus_build::Catalogue::from_json(crate::donnees::OBJETS).unwrap();
        sur_la_fiche(catalogue.item(objet).expect("l'objet"), &[objet])
    }

    /// La fiche du Dofus Ocre : son effet spécial dans les mots du jeu, sans
    /// le nom du sort qu'il répète en tête, et compté dans la Rotation.
    #[test]
    fn la_fiche_dit_l_effet_special_et_ce_qu_il_compte() {
        let f = fiche(7754);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!((f[0]["genre"].as_str(), f[0]["nom"].as_str()), (Some("effet_special"), Some("Jaune Ocre")));
        assert_eq!(f[0]["statut"], "compté dans la Rotation");
        assert!(f[0]["texte"].as_str().unwrap().starts_with("À chaque début de tour"), "{}", f[0]["texte"]);
    }

    /// La Guerre de Positions de la Hachebarde de Guerre ne compte qu'avec la
    /// Couronne de Brâm portée : sa fiche le dit selon le build.
    #[test]
    fn la_fiche_suit_les_objets_portes() {
        let catalogue = dofus_build::Catalogue::from_json(crate::donnees::OBJETS).unwrap();
        let hachebarde = catalogue.item(22368).unwrap();
        assert_eq!(sur_la_fiche(hachebarde, &[22368])[0]["statut"], "sans effet sur vos dégâts");
        assert_eq!(sur_la_fiche(hachebarde, &[22368, 20359])[0]["statut"], "compté dans la Rotation");
    }

    /// Sans texte dans la donnée, la fiche dit ce que disent les remarques :
    /// la Langue de Feu ne vise que des monstres de quête.
    #[test]
    fn un_effet_sans_texte_reprend_la_remarque() {
        let f = fiche(18853);
        assert_eq!(f[0]["statut"], "sans effet sur vos dégâts");
        assert_eq!(f[0]["texte"], "Son effet spécial ne vise que des monstres de quête");
    }

    /// Le Dofus Verdoyant : le Camouflage des catacombres au deck, avec son
    /// renfort, et son effet spécial sans effet ; le Ménologium béni, ses
    /// sorts ajoutés hors du deck.
    #[test]
    fn la_fiche_dit_le_sort_ajoute() {
        let f = fiche(29135);
        let camouflage = f.iter().find(|e| e["genre"] == "sort_ajoute").expect("le Camouflage");
        assert_eq!(camouflage["statut"], "au deck de la Rotation");
        assert!(camouflage["texte"].as_str().unwrap().contains("+10 % de critique pendant 3 tours"), "{camouflage}");
        assert!(f.iter().any(|e| e["genre"] == "effet_special" && e["statut"] == "sans effet sur vos dégâts"));
        let menologium = fiche(29133);
        assert!(menologium.iter().filter(|e| e["genre"] == "sort_ajoute").all(|e| e["statut"] == "hors du deck"));
    }

    /// Chaque fiche porte l'icône de son objet.
    #[test]
    fn la_fiche_porte_l_icone_de_l_objet() {
        let resolu = porte(8, 19482, 8);
        let f = fiches(&resolu);
        assert_eq!(f.len(), 1);
        assert!(f[0]["icone_objet"].is_number(), "{}", f[0]);
        assert_eq!(icone(&resolu, &id(2056)), f[0]["icone_objet"].as_u64().map(|n| n as u32));
    }
}
