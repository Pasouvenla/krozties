//! L'aperçu sur grille : pour chaque sort du deck, où tombe sa zone et qui elle
//! touche.
//!
//! Toute la géométrie est ici, pas dans la page : deux géométries à maintenir
//! divergeraient à la première correction de forme. L'aperçu ne cherche aucune
//! rotation : il répond à « si je lance ce sort d'ici vers là, qui prend quoi ».

use crate::cartes::Plateau;
use crate::solve::{load_ruleset, resolve_build, to_engine_build, Request};
use dofus_engine::{Case, Engine, Mode, Reach, Scenario};
use dofus_grid::tir::{visee_permise, Contraintes, Refus};
use dofus_ruleset::SpellDef;


/// La portée et l'axe d'un sort, ce que le jeu montre en bleu quand on le prend
/// en main ; `None` si la donnée n'en porte pas. Le bonus de Portée du build ne
/// vaut que pour une portée modifiable, et un malus ne la fait pas descendre sous
/// la portée minimale.
fn contraintes_du_sort(def: &SpellDef, bonus_portee: i32) -> Option<Contraintes> {
    let (min, max) = def.range?;
    let boostable = def.cast.is_none_or(|c| c.range_boostable);
    let max = if boostable {
        (i32::from(max) + bonus_portee).max(i32::from(min))
    } else {
        i32::from(max)
    };
    Some(Contraintes {
        portee_min: u16::from(min),
        portee_max: u16::try_from(max).unwrap_or(0),
        en_ligne: def.cast.is_some_and(|c| c.in_line),
        en_diagonale: def.cast.is_some_and(|c| c.in_diagonal),
    })
}

/// Pourquoi le sort ne part pas sur la case visée, ou `null` ; la page en
/// compose la phrase. Pour un sort qui exige la ligne de vue, les murs de la
/// carte la coupent, et les ennemis posés aussi. Hors du plateau, un mur ou un
/// trou ne se vise pas.
fn refus_de_visee(
    def: &SpellDef,
    contraintes: Option<Contraintes>,
    plateau: &Plateau,
    lanceur: Case,
    visee: Case,
    ennemis: &[Case],
) -> serde_json::Value {
    if !plateau.est_sol(visee) {
        return serde_json::json!({ "motif": "hors_sol" });
    }
    if let Some(c) = contraintes {
        match visee_permise(lanceur, visee, c) {
            Ok(()) => {}
            Err(Refus::TropPres { distance, minimum }) => {
                return serde_json::json!({ "motif": "trop_pres", "distance": distance, "limite": minimum });
            }
            Err(Refus::TropLoin { distance, maximum }) => {
                return serde_json::json!({ "motif": "trop_loin", "distance": distance, "limite": maximum });
            }
            Err(_) => return serde_json::json!({ "motif": "hors_axe" }),
        }
    }
    if exige_la_vue(def) && !plateau.en_vue(lanceur, visee, ennemis) {
        return serde_json::json!({ "motif": "hors_vue" });
    }
    let occupee = visee == lanceur || ennemis.contains(&visee);
    let regles = def.cast.unwrap_or_default();
    if regles.needs_taken_cell && !occupee {
        return serde_json::json!({ "motif": "case_vide" });
    }
    if regles.needs_free_cell && occupee {
        return serde_json::json!({ "motif": "case_occupee" });
    }
    serde_json::Value::Null
}

/// Le sort exige-t-il la ligne de vue ? La donnée le dit pour chaque sort ;
/// muette, elle vaut oui, le cas de presque tout le parc.
fn exige_la_vue(def: &SpellDef) -> bool {
    def.cast.is_none_or(|c| c.needs_line_of_sight)
}

/// La ligne dont la zone est la plus large : c'est elle qui décide du dessin.
fn la_plus_large<'a>(
    lignes: impl Iterator<Item = &'a dofus_ruleset::LineDef>,
) -> Option<&'a dofus_ruleset::LineDef> {
    lignes
        .filter(|l| l.area.is_some())
        .max_by_key(|l| l.area.as_ref().and_then(|a| a.max_targets).unwrap_or(0))
}

/// Au-delà, un plafond de cibles n'est plus un plafond mais un « sans limite »
/// écrit en chiffres.
const PLAFOND_LISIBLE: u16 = 20;

/// Le taux qu'une cible subit à cet éloignement. Recopie la règle du moteur,
/// dont `taux_par_cible` est privé : un test compare les deux chemins sur toutes
/// les zones du parc.
fn taux(eloignement: u16, percent: u8, steps: u8) -> u32 {
    if percent == 0 || steps == 0 {
        return 100;
    }
    // `min(steps)` ne mord que sur l'Afflux du Sacrieur, dont la croix en
    // diagonale a ses pointes à six cases de l'impact pour quatre crans. Un
    // test fixe la liste.
    let crans = u32::from(eloignement).min(u32::from(steps));
    100u32.saturating_sub(crans * u32::from(percent))
}

/// Ce que les états que ce sort pose infligent à chaque coup, ennemi par
/// ennemi : un poison, un glyphe, une Sentence. Le sort qui les pose n'a souvent
/// aucune ligne à lui. Une entrée par état, ses détentes réunies : elles portent
/// les mêmes lignes, seul leur moment change (un glyphe du Féca frappe au début
/// du tour et quand la Transhumance le déclenche). La zone est celle des lignes
/// de l'état.
fn coups_d_etat(
    ruleset: &dofus_ruleset::Ruleset,
    moteur: &Engine,
    sort: &str,
    lanceur: Case,
    visee: Case,
    ennemis: &[Case],
) -> Vec<serde_json::Value> {
    let mut etats: Vec<(String, Vec<dofus_engine::StateDamageRow>)> = Vec::new();
    for coup in moteur.state_damage_table(sort) {
        match etats.iter_mut().find(|(e, _)| *e == coup.state) {
            Some((_, coups)) => coups.push(coup),
            None => etats.push((coup.state.clone(), vec![coup])),
        }
    }
    etats
        .into_iter()
        .filter_map(|(etat, coups)| {
            let def = ruleset.resources.iter().find(|r| r.id == etat)?;
            let premier = &coups[0];
            let ligne = la_plus_large(def.while_present.get(premier.effect)?.lines.iter());
            let touches: Vec<serde_json::Value> = ligne
                .and_then(|l| l.area.as_ref())
                .and_then(|aire| {
                    let t = dofus_grid::tir::tir(
                        aire.shape,
                        u16::from(aire.size.unwrap_or(0)),
                        u16::from(aire.size2),
                        lanceur,
                        visee,
                        ennemis,
                        Contraintes::libre(0, u16::MAX),
                    )
                    .ok()?;
                    let plafond = aire.max_targets.map_or(usize::MAX, usize::from).max(1);
                    Some(
                        t.touches
                            .iter()
                            .take(plafond)
                            .map(|x| {
                                let taux =
                                    taux(x.eloignement, aire.falloff_percent, aire.falloff_steps);
                                let echelle = |(a, b): (i64, i64)| {
                                    [a * i64::from(taux) / 100, b * i64::from(taux) / 100]
                                };
                                serde_json::json!({
                                    "indice": x.indice,
                                    "case": [x.case.x, x.case.y],
                                    "eloignement": x.eloignement,
                                    "taux": taux,
                                    "normal": echelle(premier.normal),
                                    "critique": premier.critical.map(echelle),
                                })
                            })
                            .collect(),
                    )
                })
                .unwrap_or_default();
            let declencheurs: Vec<serde_json::Value> = coups
                .iter()
                .filter_map(|c| def.while_present.get(c.effect))
                .map(|w| declencheur(ruleset, def, w))
                .collect();
            Some(serde_json::json!({
                "etat": etat,
                "element": ligne.and_then(|l| l.element).map(|e| format!("{e:?}")),
                "critique": premier.crit_permille / 10,
                "declencheurs": declencheurs,
                "touches": touches,
            }))
        })
        .collect()
}

/// Quand une détente part, en données : la page en fait sa phrase.
fn declencheur(
    ruleset: &dofus_ruleset::Ruleset,
    etat: &dofus_ruleset::ResourceDef,
    effet: &dofus_ruleset::StateEffect,
) -> serde_json::Value {
    use dofus_ruleset::{Condition, Scope, StateTrigger as D};
    let porteur = |s: Scope| if s == Scope::Caster { "lanceur" } else { "cible" };
    // La condition d'une détente, quand c'est un réglage que le joueur
    // déclare dans la rotation : la page le cite par son libellé. Toute autre
    // condition se dit seulement présente.
    let (si, sous_condition) = match &effet.requires {
        None => (None, false),
        Some(Condition::CasterHas { resource } | Condition::TargetHas { resource }) => {
            let declare = ruleset
                .resources
                .iter()
                .find(|r| &r.id == resource)
                .filter(|r| r.declared_by_player)
                .and_then(|r| r.player_label.clone());
            let sous = declare.is_none();
            (declare, sous)
        }
        Some(_) => (None, true),
    };
    let mut d = match &effet.trigger {
        // Un état de d tours frappe d fois : le dernier coup part au moment
        // où il s'en va.
        D::TurnStart => serde_json::json!({
            "quand": "debut_de_tour",
            "coups": etat.duration.as_ref().map(|d| u16::from(d.turns) + 1),
            "porteur": porteur(etat.scope),
        }),
        D::SpellTagged { tag, .. } => serde_json::json!({
            "quand": "sort",
            "sorts": ruleset
                .spells
                .iter()
                .filter(|s| s.tags.contains(tag))
                .map(|s| s.name.fr.as_str())
                .collect::<Vec<_>>(),
        }),
        D::PushDamage { .. } => serde_json::json!({
            "quand": "poussee",
            "porteur": porteur(etat.scope),
        }),
        D::ResourceConsumed { resource } => {
            let perdu = ruleset.resources.iter().find(|r| &r.id == resource);
            serde_json::json!({
                "quand": "perte",
                "etat": perdu.and_then(|r| r.name.as_ref()).map(|n| n.fr.as_str()),
                "porteur": porteur(perdu.map_or(Scope::Target, |r| r.scope)),
            })
        }
    };
    d["une_fois_par_tour"] = serde_json::json!(effet.once_per_turn);
    d["si"] = serde_json::json!(si);
    d["sous_condition"] = serde_json::json!(sous_condition);
    d
}

/// L'aperçu de toutes les zones du deck, pour le placement de la requête.
pub fn zones_json(request: &Request) -> Result<String, String> {
    zones_avec_bonus(request, 0)
}

/// [`zones_json`], avec `bonus` pour cent de dommages finaux en plus sur
/// chaque lancer : celui des portails de l'Éliotrope, que KrozPortal applique
/// au sort qui ressort du portail de sortie.
pub fn zones_avec_bonus(request: &Request, bonus: u32) -> Result<String, String> {
    let Some(placement) = request.placement.as_ref() else {
        return Err("aucun placement dans la requête".into());
    };
    let build = request.build.normalise();
    let mut ruleset = load_ruleset(build.class)?;
    let resolved = resolve_build(&build)?;
    crate::objets_de_classe::sur_les_regles(&mut ruleset, &resolved);
    // ⚠️ LE MÊME CHEMIN QUE LE SOLVEUR, pour que l'aperçu et le total ne
    // puissent pas compter des ennemis différents. Le plafond de dix vit dans
    // `en_placement`, une seule fois.
    let pose = placement.en_placement();
    let (lanceur, visee, ennemis) = (pose.lanceur, pose.visee, pose.ennemis);
    let plateau = Plateau::de(placement.carte, placement.damier)?;

    // Un moteur mono-cible, uniquement pour lire les fourchettes du sort telles
    // qu'elles sortent du pipeline complet. Les mêmes chiffres que l'infobulle,
    // par construction : c'est le même appel.
    let mut build_moteur = to_engine_build(request, &resolved);
    if bonus > 0 {
        build_moteur.modifiers.push(dofus_engine::BuildModifier {
            id: "portail".into(),
            percent: 100 + bonus,
            when: dofus_engine::When::Always,
        });
    }
    let moteur = Engine::new(
        &ruleset,
        build_moteur,
        Scenario {
            poussees_bloquees: false,
            horizon: 1,
            pm_depenses: 0,
            etalement: 0,
            placement: None,
            etats_declares: vec![],
            targets: 1,
            starting_turn_is_odd: true,
            resistance: dofus_damage::Resistance::NONE,
            prune_spells: false,
            budgets: Vec::new(),
            dominance: true,
            mode: Mode::Expected,
            reach: Reach::Ranged,
        },
    )
    .map_err(|e| e.to_string())?;

    // Le bonus de Portée du build, objets, panoplies et forgemagie. Zéro pour
    // le niveau 200 sans équipement.
    let bonus_portee = resolved.totals.get("range").copied().unwrap_or(0);

    let mut sorts = Vec::new();
    for id in &request.deck {
        let Some(def) = ruleset.spells.iter().find(|s| &s.id == id) else {
            continue;
        };
        // La fourchette du palier de base : celle d'un lancer sec, sans charge
        // accumulée. C'est le seul palier qui ne dépende pas d'un état, donc le
        // seul qu'un aperçu statique puisse afficher sans mentir.
        let table = moteur.damage_table(id);
        let base = table
            .iter()
            .find(|r| r.charges == 0)
            .or_else(|| table.first());
        // Un sort à charges le dit sous ses dégâts, « sans charge accumulée » ;
        // sur les autres, la mention ne dirait rien.
        let a_des_charges = table.iter().any(|r| r.charges > 0);
        // Le taux de critique de ce sort sur ce build, règle du moteur
        // comprise : à 78 de critique ou plus, tout coup qui peut critiquer
        // critique.
        let critique = match def.crit.base_rate.known() {
            Some(b) if def.crit.can_crit => {
                dofus_engine::taux_critique(i32::from(*b), resolved.crit_bonus_percent).permille()
                    / 10
            }
            _ => 0,
        };

        // La ligne dont la zone est la plus large décide de la portée du dessin :
        // les autres lignes du sort frappent dans une zone incluse. On garde la
        // ligne, pas seulement son aire, pour lire aussi son élément. Un sort qui
        // ne frappe pas au lancer frappe par l'état qu'il pose (poison, glyphe,
        // Sentence) : sa zone est alors celle des lignes de cet état.
        let ligne = la_plus_large(def.lines.iter()).or_else(|| {
            la_plus_large(
                def.effects
                    .iter()
                    .filter_map(|e| match e {
                        dofus_ruleset::Effect::Gain { resource, .. } => {
                            ruleset.resources.iter().find(|r| &r.id == resource)
                        }
                        _ => None,
                    })
                    .flat_map(|r| r.while_present.iter().flat_map(|w| w.lines.iter())),
            )
        });
        let aire = ligne.and_then(|l| l.area.as_ref());
        // Les cases d'où le sort part, vues de la case du lanceur, et ce qui
        // l'empêche de partir sur la case visée. À portée mais hors de vue,
        // une case est « masquée » : le jeu la montre d'un bleu plus pâle.
        let contraintes = contraintes_du_sort(def, bonus_portee);
        let vue = exige_la_vue(def);
        let (lancables, masquees): (Vec<Case>, Vec<Case>) = contraintes
            .map(|c| {
                plateau
                    .sol()
                    .into_iter()
                    .filter(|&v| visee_permise(lanceur, v, c).is_ok())
                    .partition(|&v| !vue || plateau.en_vue(lanceur, v, &ennemis))
            })
            .unwrap_or_default();
        let portee = serde_json::json!({
            "bornes": contraintes.map(|c| [c.portee_min, c.portee_max]),
            "bonus": if def.cast.is_none_or(|c| c.range_boostable) { bonus_portee } else { 0 },
            "cases": lancables.iter().map(|c| [c.x, c.y]).collect::<Vec<_>>(),
            "masquees": masquees.iter().map(|c| [c.x, c.y]).collect::<Vec<_>>(),
            "ligne_de_vue": vue,
            "visee": refus_de_visee(def, contraintes, &plateau, lanceur, visee, &ennemis),
        });
        let Some(aire) = aire else {
            sorts.push(serde_json::json!({
                "sort": id, "nom": def.name.fr, "forme": "P", "cases": [],
                "refus": "sans zone déclarée", "portee": portee,
            }));
            continue;
        };
        // La zone qu'on obtiendrait en visant chacune de ces cases : la page
        // la dessine au survol, AVANT le clic, comme le jeu. Calculée ici et
        // non dans la page, pour que la géométrie reste en un seul endroit.
        let survol: serde_json::Map<String, serde_json::Value> = lancables
            .iter()
            .filter_map(|&v| {
                let t = dofus_grid::tir::tir(
                    aire.shape,
                    u16::from(aire.size.unwrap_or(0)),
                    u16::from(aire.size2),
                    lanceur,
                    v,
                    &[],
                    Contraintes::libre(0, u16::MAX),
                )
                .ok()?;
                let cases: Vec<[i16; 2]> =
                    t.cases.iter().filter(|c| plateau.est_sol(**c)).map(|c| [c.x, c.y]).collect();
                Some((format!("{},{}", v.x, v.y), serde_json::json!(cases)))
            })
            .collect();
        let tir = dofus_grid::tir::tir(
            aire.shape,
            u16::from(aire.size.unwrap_or(0)),
            u16::from(aire.size2),
            lanceur,
            visee,
            &ennemis,
            Contraintes::libre(0, u16::MAX),
        );
        let (cases, touches, refus) = match tir {
            Ok(t) => {
                let plafond = aire.max_targets.map_or(usize::MAX, usize::from).max(1);
                let touches: Vec<serde_json::Value> = t
                    .touches
                    .iter()
                    .take(plafond)
                    .map(|x| {
                        let taux = taux(x.eloignement, aire.falloff_percent, aire.falloff_steps);
                        serde_json::json!({
                            // Le rang de l'ennemi dans la liste posée, que la
                            // page dessine sur le damier et reprend dans son
                            // tableau.
                            "indice": x.indice,
                            "case": [x.case.x, x.case.y],
                            "eloignement": x.eloignement,
                            "taux": taux,
                            "normal": base.map(|r| {
                                [r.normal.0 * i64::from(taux) / 100,
                                  r.normal.1 * i64::from(taux) / 100]
                            }),
                            "critique": base.and_then(|r| r.critical).map(|c| {
                                [c.0 * i64::from(taux) / 100, c.1 * i64::from(taux) / 100]
                            }),
                        })
                    })
                    .collect();
                // Les cases sont écrêtées au plateau : un cercle de
                // rayon 63, la façon dont le jeu écrit « partout »,
                // ferait huit mille paires de coordonnées. Le nombre
                // annoncé reste celui de la zone entière.
                let proches: Vec<[i16; 2]> = t
                    .cases
                    .iter()
                    .filter(|c| plateau.est_sol(**c))
                    .map(|c| [c.x, c.y])
                    .collect();
                (proches, touches, None)
            }
            // Une forme non dessinée se dit. Le silence ferait croire que le
            // sort ne touche personne, ce qui est faux : il touche, on ne sait
            // simplement pas où.
            Err(e) => (Vec::new(), Vec::new(), Some(format!("{e:?}"))),
        };
        // L'élément de cette ligne, pour la couleur de l'étiquette de dégâts.
        // Une ligne en meilleur ou pire élément n'en a pas d'écrit : il dépend
        // du build.
        let element = ligne.and_then(|l| l.element).map(|e| format!("{e:?}"));
        let etats = coups_d_etat(&ruleset, &moteur, id, lanceur, visee, &ennemis);
        sorts.push(serde_json::json!({
            "sort": id,
            "nom": def.name.fr,
            "element": element,
            "forme": aire.shape.to_string(),
            "taille": aire.size,
            // Un plafond de plusieurs milliers est la façon dont le jeu écrit
            // « sans limite » ; l'afficher tel quel ferait « frappe au plus
            // 8 065 cibles », une phrase que personne ne peut lire.
            "plafond": aire.max_targets.filter(|n| *n <= PLAFOND_LISIBLE),
            "degressive": aire.falloff_percent > 0 && aire.falloff_steps > 0,
            "charges": a_des_charges,
            "cases": cases,
            "touches": touches,
            // Le taux de critique du sort sur ce build, en pour cent.
            "critique": critique,
            "refus": refus,
            "portee": portee,
            "survol": survol,
            // Les coups des états que le sort pose, poison ou glyphe, à côté
            // de ceux du lancer.
            "etats": etats,
        }));
    }
    // Le critique du build pris par le calcul, Dofus écartés compris : la page
    // dit la règle du critique sûr quand il l'atteint.
    Ok(serde_json::json!({
        "sorts": sorts,
        "critique_du_build": resolved.crit_bonus_percent,
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requete(deck: &[&str], ennemis: Vec<(i16, i16)>) -> Request {
        serde_json::from_value(serde_json::json!({
            "class": 8,
            "level": 200,
            "invested": { "strength": 400 },
            "deck": deck,
            "horizon": 1,
            "placement": {
                "lanceur": [-6, 0],
                "visee": [0, 0],
                "ennemis": ennemis,
            },
        }))
        .expect("la requête de test doit se lire")
    }

    fn apercu(r: &Request) -> serde_json::Value {
        serde_json::from_str(&zones_json(r).expect("l'aperçu doit se calculer"))
            .expect("l'aperçu doit être du JSON")
    }

    /// Le tableau `[x, y]` du JSON, en case.
    fn case(v: &serde_json::Value) -> Case {
        Case::new(
            i16::try_from(v[0].as_i64().unwrap()).unwrap(),
            i16::try_from(v[1].as_i64().unwrap()).unwrap(),
        )
    }

    /// La zone se dessine avant qu'on pose un ennemi : la réponse porte la zone, la
    /// portée et le survol, et personne n'est touché.
    #[test]
    fn la_zone_et_la_portee_se_dessinent_sans_ennemi() {
        let vu = apercu(&requete(&["epee_de_iop"], vec![]));
        let s = &vu["sorts"][0];
        assert!(!s["cases"].as_array().unwrap().is_empty(), "{s}");
        assert!(s["touches"].as_array().unwrap().is_empty(), "{s}");
        let bornes = &s["portee"]["bornes"];
        let (min, max) = (bornes[0].as_u64().unwrap(), bornes[1].as_u64().unwrap());
        let lanceur = Case::new(-6, 0);
        let cases: Vec<Case> = s["portee"]["cases"].as_array().unwrap().iter().map(case).collect();
        assert!(!cases.is_empty());
        for c in &cases {
            let d = u64::from(lanceur.distance(*c));
            assert!((min..=max).contains(&d), "{c:?} à {d} cases hors de {min}..{max}");
        }
        // Toute case du damier à bonne distance y est : la portée n'en oublie pas.
        let attendues = (-6..=6i16)
            .flat_map(|y| (-6..=6i16).map(move |x| Case::new(x, y)))
            .filter(|c| (min..=max).contains(&u64::from(lanceur.distance(*c))))
            .count();
        let axe = &s["portee"];
        if axe["bornes"].is_array() && cases.len() != attendues {
            // Un sort en ligne ou en diagonale en garde moins, jamais plus.
            assert!(cases.len() < attendues);
        }
    }

    /// Le survol d'une case montre exactement la zone qu'on obtient en la
    /// visant : c'est le même calcul, et le clic ne doit rien changer au
    /// dessin qu'on vient de voir.
    #[test]
    fn le_survol_montre_la_zone_du_clic() {
        let vu = apercu(&requete(&["epee_de_iop"], vec![]));
        let survol = vu["sorts"][0]["survol"].as_object().unwrap().clone();
        assert!(!survol.is_empty());
        for cle in ["-3,0", "-2,1", "-6,3"] {
            let Some(vue) = survol.get(cle) else { continue };
            let (x, y) = cle.split_once(',').unwrap();
            let mut r = requete(&["epee_de_iop"], vec![]);
            r.placement.as_mut().unwrap().visee = (x.parse().unwrap(), y.parse().unwrap());
            let clic = apercu(&r);
            let mut attendu: Vec<Case> = clic["sorts"][0]["cases"]
                .as_array()
                .unwrap()
                .iter()
                .map(case)
                .filter(|c| Plateau::de(None, None).unwrap().est_sol(*c))
                .collect();
            let mut montre: Vec<Case> = vue.as_array().unwrap().iter().map(case).collect();
            attendu.sort_by_key(|c| (c.x, c.y));
            montre.sort_by_key(|c| (c.x, c.y));
            assert_eq!(montre, attendu, "survol de {cle}");
        }
    }

    /// Une visée hors de portée se dit, chiffres compris ; le bonus de Portée
    /// du build l'allonge. L'Épée de Iop part de 0 à 8 cases, en ligne.
    #[test]
    fn la_visee_hors_de_portee_se_dit_et_la_portee_du_build_compte() {
        let visant = |visee: (i16, i16), portee_du_build: i32| {
            let mut r = requete(&["epee_de_iop"], vec![(0, 0)]);
            r.placement.as_mut().unwrap().visee = visee;
            if portee_du_build != 0 {
                r.build.invested.insert("range".into(), portee_du_build);
            }
            apercu(&r)["sorts"][0].clone()
        };
        let s = visant((3, 0), 0);
        assert_eq!(s["portee"]["bornes"], serde_json::json!([0, 8]), "{s}");
        assert_eq!(s["portee"]["visee"]["motif"], "trop_loin", "{s}");
        assert_eq!(s["portee"]["visee"]["distance"], 9);
        assert_eq!(s["portee"]["visee"]["limite"], 8);
        // Hors de la ligne, à portée pourtant.
        let s = visant((-4, 2), 0);
        assert_eq!(s["portee"]["visee"]["motif"], "hors_axe", "{s}");

        // Sa portée ne se modifie pas : le bonus du build n'y change rien.
        let s = visant((3, 0), 6);
        assert_eq!(s["portee"]["bonus"], 0, "{s}");
        assert_eq!(s["portee"]["bornes"], serde_json::json!([0, 8]));
    }

    /// Le critique se lit sur la carte comme dans la rotation : à 78 de
    /// critique au build, le sort critique à coup sûr et sa fourchette
    /// critique arrive avec la normale.
    #[test]
    fn la_zone_donne_le_critique_et_son_taux() {
        let critique_au_build = |crit: i32| {
            let mut r = requete(&["epee_de_iop"], vec![(0, 0)]);
            r.build.invested.insert("critical_rate".into(), crit);
            apercu(&r)["sorts"][0].clone()
        };
        let sur = critique_au_build(80);
        assert_eq!(sur["critique"], 100, "{sur}");
        let t = &sur["touches"][0];
        let (n, c) = (&t["normal"], &t["critique"]);
        assert!(c[0].as_i64().unwrap() > n[0].as_i64().unwrap(), "{t}");
        assert!(c[1].as_i64().unwrap() > n[1].as_i64().unwrap(), "{t}");
        // Sous le seuil, le taux du sort plus celui du build, et pas cent.
        let bas = critique_au_build(10);
        let taux = bas["critique"].as_i64().unwrap();
        assert!(taux > 10 && taux < 100, "{bas}");
    }

    /// La réponse porte le critique du build pris par le calcul : KrozZone y
    /// lit si la règle du critique sûr s'applique, pour la dire.
    #[test]
    fn la_zone_dit_le_critique_du_build() {
        for crit in [10, 80] {
            let mut r = requete(&["epee_de_iop"], vec![(0, 0)]);
            r.build.invested.insert("critical_rate".into(), crit);
            assert_eq!(apercu(&r)["critique_du_build"], crit);
        }
    }

    /// Sur un sort dont la portée se modifie, le bonus du build l'allonge et
    /// les cases bleues suivent.
    #[test]
    fn la_portee_du_build_allonge_un_sort_modifiable() {
        let rs = load_ruleset(8).unwrap();
        let def = rs
            .spells
            .iter()
            .find(|s| {
                s.lines.iter().any(|l| l.area.is_some())
                    && s.cast.is_some_and(|c| c.range_boostable && !c.in_line && !c.in_diagonal)
                    && s.range.is_some_and(|(min, max)| min <= 1 && max <= 6)
            })
            .expect("un sort de Iop à zone et à portée modifiable");
        let (min, max) = def.range.unwrap();
        let portee = |bonus: i32| {
            let mut r = requete(&[def.id.as_str()], vec![]);
            if bonus != 0 {
                r.build.invested.insert("range".into(), bonus);
            }
            apercu(&r)["sorts"][0]["portee"].clone()
        };
        let nu = portee(0);
        let long = portee(2);
        assert_eq!(nu["bornes"], serde_json::json!([min, max]), "{}", def.id);
        assert_eq!(long["bornes"], serde_json::json!([min, max + 2]), "{}", def.id);
        assert_eq!(long["bonus"], 2);
        assert!(
            long["cases"].as_array().unwrap().len() > nu["cases"].as_array().unwrap().len(),
            "{}",
            def.id
        );
    }

    /// Le test qui justifie la recopie de `taux` : le rapport des deux totaux du
    /// moteur, avec les ennemis posés et avec un seul sur l'impact, vaut la somme des
    /// taux que l'aperçu annonce. Les deux chemins ne partagent aucune ligne de code.
    #[test]
    fn les_taux_de_l_apercu_sont_ceux_que_le_moteur_applique() {
        // L'Épée de Iop : croix de rayon 3. Trois ennemis dedans, à zéro, une
        // et trois cases, plus un quatrième en diagonale que la croix rate.
        let r = requete(&["epee_de_iop"], vec![(0, 0), (0, 1), (0, 3), (1, 1)]);
        let vu = apercu(&r);
        let touches = vu["sorts"][0]["touches"].as_array().unwrap();
        assert_eq!(
            touches.len(),
            3,
            "la diagonale n'est pas sur la croix : {touches:?}"
        );
        let somme: u64 = touches.iter().map(|t| t["taux"].as_u64().unwrap()).sum();
        assert_eq!(somme, 100 + 90 + 70, "les taux annoncés");

        let solde = |req: &Request| {
            crate::solve::run(req)
                .expect("le solveur doit tourner")
                .solution
                .total
                .as_f64()
        };
        let seul = requete(&["epee_de_iop"], vec![(0, 0)]);
        let attendu = solde(&seul) * somme as f64 / 100.0;
        let mesure = solde(&r);
        assert!(
            (mesure - attendu).abs() < 2.0,
            "le moteur doit appliquer exactement les taux annoncés : il rend \
             {mesure:.1} là où l'aperçu promet {attendu:.1}"
        );
    }

    /// La fourche : sept cases pour le Trident de la Mer.
    #[test]
    fn la_fourche_du_trident_se_dessine() {
        let mut r = requete(&["epee_de_iop"], vec![(0, 0)]);
        r.build.class = 20; // Forgelance
        r.deck = vec!["trident_de_la_mer".into()];
        let vu = apercu(&r);
        let sort = &vu["sorts"][0];
        assert!(sort["refus"].is_null(), "{sort}");
        assert_eq!(sort["cases"].as_array().unwrap().len(), 7, "{sort}");
    }

    /// Le plafond de crans mord sur un seul sort du parc. Le carré, l'étoile et la
    /// croix en diagonale portent des cases à deux fois leur taille de l'impact :
    /// comparer le rayon déclaré au nombre de crans ne suffit pas, il faut dessiner
    /// les zones. Reste l'Afflux du Sacrieur, croix en diagonale de taille 3 dont les
    /// pointes sont à six cases de l'impact pour quatre crans : un ennemi sur une
    /// pointe y prend le même 60 % qu'à quatre cases.
    #[test]
    fn le_plafond_de_crans_mord_sur_l_afflux_et_sur_lui_seul() {
        let mut degressives = 0;
        let mut non_dessinees = 0;
        let mut mordues: Vec<(String, char, u16, u8)> = Vec::new();
        for classe in 1..=20u32 {
            let Ok(snap) = crate::solve::snapshot_for(classe) else {
                continue;
            };
            for spell in &snap.spells {
                for niveau in &spell.levels {
                    let lignes = niveau
                        .normal_lines
                        .iter()
                        .chain(niveau.critical_lines.iter())
                        .chain(niveau.placed_lines.iter())
                        .chain(niveau.placed_critical_lines.iter());
                    for ligne in lignes {
                        let Some(z) = ligne.zone else { continue };
                        if z.falloff_steps == 0 || z.falloff_percent == 0 {
                            continue;
                        }
                        degressives += 1;
                        let Some(cases) = dofus_grid::cases_de_zone(
                            z.shape,
                            u16::from(z.size.unwrap_or(0)),
                            u16::from(z.size2),
                            Case::new(0, 0),
                            (1, 0),
                        ) else {
                            non_dessinees += 1;
                            continue;
                        };
                        let loin = cases
                            .iter()
                            .map(|c| c.distance(Case::new(0, 0)))
                            .max()
                            .unwrap_or(0);
                        if loin > u16::from(z.falloff_steps) {
                            let e = (spell.name.fr.clone(), z.shape, loin, z.falloff_steps);
                            if !mordues.contains(&e) {
                                mordues.push(e);
                            }
                        }
                    }
                }
            }
        }
        // Les Carreaux Destructeurs en 3.7, dégressifs : leur fourche porte à 6
        // cases, et le plafond de 4 crans y mord (10 % par case et 4 crans, la
        // règle des autres zones, la note ne les chiffrant pas).
        assert_eq!(
            mordues,
            vec![("Carreaux Destructeurs".to_string(), 'F', 6, 4), ("Afflux".to_string(), 'Q', 6, 4)],
            "la liste des zones où le plafond mord a changé : s'il s'en ajoute une, \
             elle doit être regardée, et s'il n'en reste aucune le plafond redevient \
             du code que rien ne visite"
        );
        assert!(
            degressives >= 500 && non_dessinees < degressives / 10,
            "contrôle trop maigre : {degressives} zones dégressives, dont \
             {non_dessinees} non dessinées"
        );

        // Et la conséquence, chiffrée : sur cette zone-là, s'éloigner au-delà
        // de quatre cases ne coûte plus rien.
        assert_eq!(taux(4, 10, 4), 60);
        assert_eq!(taux(5, 10, 4), 60);
        assert_eq!(taux(6, 10, 4), 60);
        // Sans le plafond, la pointe tomberait à 40 %, et une pointe plus loin
        // encore passerait sous zéro.
        assert_eq!(100u32.saturating_sub(6 * 10), 40);
    }

    /// Le plafond d'ennemis tient aussi hors de la page : il vit dans
    /// `en_placement`, que l'aperçu et le solveur traversent tous les deux. Le test
    /// se lit sur `MAX_ENNEMIS`, quelle que soit sa valeur. La coupe garde les
    /// premiers posés : le plafond entier hors de la zone, puis trois ennemis dedans,
    /// et le sort ne touche personne.
    #[test]
    fn le_plafond_d_ennemis_ne_depend_pas_de_la_page() {
        let plafond = usize::from(crate::solve::MAX_ENNEMIS);
        // Les cases à deux coordonnées non nulles ne sont sur aucune branche
        // d'une croix centrée en zéro : de quoi remplir le plafond sans qu'un
        // seul de ces ennemis soit touché.
        let hors_zone: Vec<(i16, i16)> = (1..=8)
            .flat_map(|x| (1..=8).map(move |y| (x as i16, y as i16)))
            .take(plafond)
            .collect();
        assert_eq!(hors_zone.len(), plafond, "il faut de quoi remplir le plafond");

        let dans_la_zone = [(0, 1), (0, 2), (0, 3)];
        let trop: Vec<(i16, i16)> = hors_zone
            .iter()
            .copied()
            .chain(dans_la_zone)
            .collect();
        let vu = apercu(&requete(&["epee_de_iop"], trop));
        let touches = vu["sorts"][0]["touches"].as_array().unwrap();
        assert!(
            touches.is_empty(),
            "les trois ennemis de la zone sont posés APRÈS le plafond, ils doivent \
             tomber : {touches:?}"
        );

        // Et le plafond ne mord pas en deçà : les trois mêmes, posés alors
        // qu'il reste de la place, sont bien tous les trois touchés.
        let assez: Vec<(i16, i16)> = hors_zone
            .iter()
            .copied()
            .take(plafond - dans_la_zone.len())
            .chain(dans_la_zone)
            .collect();
        let vu = apercu(&requete(&["epee_de_iop"], assez));
        assert_eq!(
            vu["sorts"][0]["touches"].as_array().unwrap().len(),
            dans_la_zone.len(),
            "sous le plafond, aucun ennemi ne doit disparaître"
        );
    }

    /// Le rayon de rendu écrête les cases sans toucher au reste : un cercle de rayon
    /// 63 renvoyé entier ferait huit mille paires de coordonnées.
    #[test]
    fn une_zone_immense_ne_deverse_pas_ses_milliers_de_cases() {
        let r = requete(&["epee_de_iop"], vec![(0, 0)]);
        let vu = apercu(&r);
        let cases = vu["sorts"][0]["cases"].as_array().unwrap();
        let damier = Plateau::de(None, None).unwrap();
        for c in cases {
            let (x, y) = (c[0].as_i64().unwrap() as i16, c[1].as_i64().unwrap() as i16);
            assert!(damier.est_sol(Case::new(x, y)), "case {x},{y} hors du damier");
        }
        // Et la croix de rayon 3 tient entièrement dedans : l'écrêtage ne doit
        // pas mordre sur ce que la page affiche vraiment.
        assert_eq!(cases.len(), 13);
    }

    /// Une requête posée sur une carte de boss, pour une classe et un sort.
    fn sur_carte(
        class: u32,
        sort: &str,
        lanceur: (i16, i16),
        visee: (i16, i16),
        ennemis: Vec<(i16, i16)>,
        carte: Option<u16>,
    ) -> Request {
        serde_json::from_value(serde_json::json!({
            "class": class, "level": 200, "deck": [sort], "horizon": 1,
            "placement": { "lanceur": lanceur, "visee": visee, "ennemis": ennemis, "carte": carte },
        }))
        .expect("la requête de test doit se lire")
    }

    fn cases_de(v: &serde_json::Value) -> Vec<Case> {
        v.as_array().expect("une liste de cases").iter().map(case).collect()
    }

    /// Sur une carte, on ne vise que le sol en vue. Klime : sa ligne 10 porte un mur
    /// en colonne 17, soit (1, -6), entre (-2, -6) et (3, -6). La Flèche Explosive,
    /// qui exige la vue, n'y part pas ; toute case qu'elle montre est du sol, en vue
    /// si elle est bleue, hors de vue si elle est masquée.
    #[test]
    fn sur_une_carte_le_mur_masque_la_visee() {
        let klime = Plateau::de(Some(62), None).unwrap();
        assert!(klime.murs().contains(&Case::new(1, -6)));
        let (lanceur, visee) = ((-2, -6), (3, -6));
        let vu = apercu(&sur_carte(9, "fleche_explosive", lanceur, visee, vec![visee], Some(62)));
        let s = &vu["sorts"][0];
        assert_eq!(s["portee"]["visee"]["motif"], "hors_vue", "{}", s["portee"]);
        let (bleues, masquees) = (cases_de(&s["portee"]["cases"]), cases_de(&s["portee"]["masquees"]));
        assert!(masquees.contains(&Case::new(3, -6)) && !bleues.contains(&Case::new(3, -6)));
        let depuis = Case::new(lanceur.0, lanceur.1);
        assert!(bleues.iter().all(|c| klime.est_sol(*c) && klime.en_vue(depuis, *c, &[])));
        assert!(masquees.iter().all(|c| klime.est_sol(*c) && !klime.en_vue(depuis, *c, &[])));
        assert!(cases_de(&s["cases"]).iter().all(|c| klime.est_sol(*c)));
        // Sur le damier vide, la même visée part : c'est bien le mur.
        let vide = apercu(&sur_carte(9, "fleche_explosive", lanceur, visee, vec![visee], None));
        assert!(vide["sorts"][0]["portee"]["visee"].is_null(), "{}", vide["sorts"][0]["portee"]);
    }

    /// Un corps coupe la vue, sur le damier vide aussi : l'ennemi posé entre
    /// le lanceur et la visée la masque, et la case derrière lui avec. Retiré,
    /// la visée part, et seules restent masquées les cases que l'ennemi de la
    /// visée cache à son tour.
    #[test]
    fn un_ennemi_entre_deux_masque_la_visee() {
        let (lanceur, visee) = ((-6, 0), (0, 0));
        let vu = apercu(&sur_carte(8, "epee_de_iop", lanceur, visee, vec![(-3, 0), visee], None));
        let portee = &vu["sorts"][0]["portee"];
        assert_eq!(portee["visee"]["motif"], "hors_vue", "{portee}");
        assert!(cases_de(&portee["masquees"]).contains(&Case::new(0, 0)));
        assert!(cases_de(&portee["cases"]).contains(&Case::new(-3, 0)), "on vise l'ennemi lui-même");
        let libre = apercu(&sur_carte(8, "epee_de_iop", lanceur, visee, vec![visee], None));
        assert!(libre["sorts"][0]["portee"]["visee"].is_null(), "{}", libre["sorts"][0]["portee"]);
        let derriere = cases_de(&libre["sorts"][0]["portee"]["masquees"]);
        assert!(!derriere.is_empty());
        assert!(derriere
            .iter()
            .all(|c| dofus_grid::vue::traversees(Case::new(-6, 0), *c).contains(&Case::new(0, 0))));
    }

    /// Le Sablier du Xélor n'exige pas la vue, la donnée le dit : il part
    /// par-dessus le mur de Klime, et rien n'est masqué.
    #[test]
    fn un_sort_sans_ligne_de_vue_passe_le_mur() {
        let (lanceur, visee) = ((-2, -6), (3, -6));
        let vu = apercu(&sur_carte(5, "sablier", lanceur, visee, vec![visee], Some(62)));
        let portee = &vu["sorts"][0]["portee"];
        assert_eq!(portee["ligne_de_vue"], false);
        assert!(portee["visee"].is_null(), "{portee}");
        assert!(cases_de(&portee["masquees"]).is_empty());
        assert!(cases_de(&portee["cases"]).contains(&Case::new(3, -6)));
    }

    /// Un mur ou un trou ne se vise pas : la réponse le dit.
    #[test]
    fn un_mur_ne_se_vise_pas() {
        let vu = apercu(&sur_carte(9, "fleche_explosive", (-2, -6), (1, -6), vec![], Some(62)));
        assert_eq!(vu["sorts"][0]["portee"]["visee"]["motif"], "hors_sol");
    }

    /// Un sort qui frappe par l'état qu'il pose se dessine aussi, par la zone de ce
    /// poison ou de ce glyphe : Terre Brûlée du Féca pose un glyphe carré de 25
    /// cases, les Miasmes du Sadida un poison en cercle de 13, l'Arsenic du Sram un
    /// poison sur une case.
    #[test]
    fn un_sort_qui_frappe_par_son_etat_se_dessine() {
        for (classe, sort, forme, cases) in
            [(1, "terre_brulee", "G", 25), (10, "miasmes", "C", 13), (4, "arsenic", "P", 1)]
        {
            let vu = apercu(&sur_carte(classe, sort, (-3, 0), (0, 0), vec![(0, 0)], None));
            let s = &vu["sorts"][0];
            assert!(s["refus"].is_null(), "{sort} : {}", s["refus"]);
            assert_eq!(s["forme"], forme, "{sort}");
            assert_eq!(s["cases"].as_array().unwrap().len(), cases, "{sort}");
        }
    }

    /// Un poison se chiffre ennemi par ennemi. Le Vent Empoisonné du Sadida dure
    /// deux tours, donc frappe deux fois, au début des tours de la cible.
    #[test]
    fn un_poison_se_chiffre_ennemi_par_ennemi() {
        let vu = apercu(&sur_carte(10, "vent_empoisonne", (-3, 0), (0, 0), vec![(0, 0)], None));
        let etat = &vu["sorts"][0]["etats"][0];
        assert_eq!(etat["etat"], "vent_empoisonne", "{vu}");
        let d = &etat["declencheurs"][0];
        assert_eq!(
            (d["quand"].as_str(), d["coups"].as_u64(), d["porteur"].as_str()),
            (Some("debut_de_tour"), Some(2), Some("cible")),
            "{d}"
        );
        let coup = &etat["touches"][0];
        let (a, b) = (coup["normal"][0].as_i64().unwrap(), coup["normal"][1].as_i64().unwrap());
        assert!(a > 0 && b >= a, "{coup}");
        assert_eq!(coup["taux"], 100, "{coup}");
    }

    /// Les détentes d'un même état se réunissent, et chacune dit quand elle
    /// part : le glyphe au début des tours ET à la Transhumance, l'Aiguille au
    /// début du tour ET quand la cible perd son Téléfrag.
    #[test]
    fn chaque_detente_dit_quand_elle_part() {
        let vu = apercu(&sur_carte(1, "terre_brulee", (-3, 0), (0, 0), vec![(0, 0)], None));
        let d = &vu["sorts"][0]["etats"][0]["declencheurs"];
        assert_eq!(d.as_array().map(Vec::len), Some(2), "{d}");
        assert_eq!((&d[0]["quand"], &d[0]["coups"]), (&serde_json::json!("debut_de_tour"), &serde_json::json!(2)));
        assert_eq!(d[1]["quand"], "sort", "{d}");
        assert_eq!(d[1]["sorts"], serde_json::json!(["Transhumance"]), "{d}");
        assert_eq!(d[1]["une_fois_par_tour"], true, "{d}");
        let vu = apercu(&sur_carte(5, "aiguille", (-3, 0), (0, 0), vec![(0, 0)], None));
        let d = &vu["sorts"][0]["etats"][0]["declencheurs"];
        assert_eq!((&d[1]["quand"], &d[1]["etat"]), (&serde_json::json!("perte"), &serde_json::json!("Téléfrag")), "{d}");
    }

    /// Aucun sort qui frappe par un état ne reste sans chiffre : sur les
    /// dix-neuf classes, chaque état qui frappe se chiffre sur les ennemis
    /// posés dans sa zone, et chacune de ses détentes se dit.
    #[test]
    fn tout_etat_qui_frappe_se_chiffre() {
        let mut vus = 0;
        for (classe, _, _) in crate::solve::CLASSES {
            let rs = load_ruleset(*classe).expect("les règles de la classe");
            let frappe = |id: &str| {
                rs.resources.iter().any(|r| r.id == id && !r.while_present.is_empty())
            };
            for sort in rs.spells.iter().filter(|s| {
                s.effects.iter().any(|e| {
                    matches!(e, dofus_ruleset::Effect::Gain { resource, .. } if frappe(resource))
                })
            }) {
                let autour = vec![(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)];
                let vu = apercu(&sur_carte(*classe, &sort.id, (-3, 0), (0, 0), autour, None));
                let etats = vu["sorts"][0]["etats"].as_array().cloned().unwrap_or_default();
                assert!(!etats.is_empty(), "{} : aucun état chiffré, {vu}", sort.id);
                for e in &etats {
                    let touches = e["touches"].as_array().unwrap();
                    assert!(
                        !touches.is_empty()
                            && touches.iter().all(|t| t["normal"][0].as_i64().unwrap_or(0) > 0),
                        "{} : {e}",
                        sort.id
                    );
                    for d in e["declencheurs"].as_array().unwrap() {
                        let dit = match d["quand"].as_str() {
                            Some("sort") => !d["sorts"].as_array().unwrap().is_empty(),
                            Some("perte") => d["etat"].is_string(),
                            Some("debut_de_tour" | "poussee") => true,
                            _ => false,
                        };
                        assert!(dit, "{} : {d}", sort.id);
                    }
                    vus += 1;
                }
            }
        }
        assert!(vus >= 32, "{vus} états chiffrés seulement");
    }

    /// Le Glas monte par charges, l'Épée de Iop non : seul le premier dit
    /// « sans charge accumulée » sous ses dégâts.
    #[test]
    fn seul_un_sort_a_charges_le_dit() {
        let iop = apercu(&requete(&["epee_de_iop"], vec![(0, 0)]));
        assert_eq!(iop["sorts"][0]["charges"], false, "{iop}");
        let mut r = requete(&["glas"], vec![(0, 0)]);
        r.build.class = 5; // Xélor
        let glas = apercu(&r);
        assert_eq!(glas["sorts"][0]["charges"], true, "{glas}");
    }
}
