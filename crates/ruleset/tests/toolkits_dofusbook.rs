//! Ce que les toolkits DofusBook (`data/snapshots/dofusbook-*.json`) savent
//! contredire et que l'instantané ne porte pas : le titre de chaque groupe, où
//! la condition est écrite en toutes lettres, et la durée de chaque effet. Les
//! défauts gardés ici ne se voient dans aucun total : un plafond de compteur
//! au-delà des paliers affichés, un palier qui élargit une fourchette écrit en
//! bonus de base, un poison compté une fois quand il tombe plusieurs.

use dofus_ruleset::{BaseBonus, Maybe, Ruleset};
use std::collections::{BTreeMap, BTreeSet};

const CLASSES: &[(&str, u32)] = &[
    ("feca", 1),
    ("osamodas", 2),
    ("enutrof", 3),
    ("sram", 4),
    ("xelor", 5),
    ("ecaflip", 6),
    ("eniripsa", 7),
    ("iop", 8),
    ("cra", 9),
    ("sadida", 10),
    ("sacrieur", 11),
    ("pandawa", 12),
    ("roublard", 13),
    ("zobal", 14),
    ("steamer", 15),
    ("eliotrope", 16),
    ("huppermage", 17),
    ("ouginak", 18),
    ("forgelance", 20),
];

/// Un groupe du toolkit : son titre, et les effets qu'il porte.
struct Groupe {
    titre: String,
    effets: Vec<Effet>,
}

struct Effet {
    code: String,
    critique: bool,
    min: i64,
    max: i64,
    duree: i64,
}

fn ruleset(classe: &str) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
        .unwrap_or_else(|e| panic!("{classe}: {e}"))
}

/// Les sorts qu'une note de sortie a changés : les toolkits, relevés en 3.6.12,
/// ne les connaissent pas à leurs nouveaux chiffres.
fn changes() -> BTreeSet<u32> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let manifeste = std::fs::read_to_string(format!("{root}/data/version.json")).unwrap();
    dofus_ruleset::sorts_changes_depuis_le_releve(&manifeste)
}

/// Les toolkits d'une classe, indexés par `dofusdb_id`.
fn toolkits(breed: u32) -> BTreeMap<u32, Vec<Groupe>> {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let brut = std::fs::read_to_string(format!(
        "{root}/data/snapshots/dofusbook-{breed}.json"
    ))
    .unwrap_or_else(|e| panic!("dofusbook-{breed}.json: {e}"));
    let doc: serde_json::Value = serde_json::from_str(&brut).unwrap();
    let mut out = BTreeMap::new();
    for sort in doc["sorts"].as_array().into_iter().flatten() {
        let Some(id) = sort["id"].as_u64() else {
            continue;
        };
        let mut groupes = Vec::new();
        for g in sort["g"].as_array().into_iter().flatten() {
            let titre = g[0].as_str().unwrap_or_default().to_string();
            let mut effets = Vec::new();
            for e in g[2].as_array().into_iter().flatten() {
                effets.push(Effet {
                    code: e[0].as_str().unwrap_or_default().to_string(),
                    critique: e[1].as_i64() == Some(1),
                    min: e[2].as_i64().unwrap_or(0),
                    max: e[3].as_i64().unwrap_or(0),
                    duree: e[4].as_i64().unwrap_or(0),
                });
            }
            groupes.push(Groupe { titre, effets });
        }
        out.insert(id as u32, groupes);
    }
    out
}

/// `dt`, `vf`, `dme`, `dpe`… : la fin du code est l'élément, le préfixe dit
/// dégâts (`d`) ou vol de vie (`v`) ; `me` est le meilleur élément, `pe` le
/// pire. Les soins (`pdv`, `s`), les poussées (`pou`) et les pièges (`pi`)
/// n'entrent pas ici.
fn degats(code: &str) -> bool {
    if code == "dv" {
        return false; // dégâts en pourcentage de vie, pas une fourchette d'élément
    }
    let Some(reste) = code.strip_prefix('d').or_else(|| code.strip_prefix('v')) else {
        return false;
    };
    matches!(reste, "t" | "f" | "a" | "e" | "n" | "me" | "pe")
}

/// Le plafond d'un compteur, contre le nombre de paliers que la source affiche.
/// Le champ `cu` du toolkit ne sert pas : il vient de la dernière
/// caractéristique du sort, qui ne porte pas forcément le bonus (la Flèche
/// Punitive affiche `cu` à 1 avec deux paliers). Seuls les titres « N charges »
/// comptent.
#[test]
fn un_plafond_ne_depasse_pas_les_paliers_de_la_source() {
    let mut fautes = Vec::new();
    let mut verifies = 0usize;

    for (classe, breed) in CLASSES {
        let rs = ruleset(classe);
        let tk = toolkits(*breed);
        let plafonds: BTreeMap<&str, u8> = rs
            .resources
            .iter()
            .map(|r| (r.id.as_str(), r.max))
            .collect();

        for sort in &rs.spells {
            let Some(groupes) = sort.dofusdb_id.filter(|d| !changes().contains(d)).and_then(|d| tk.get(&d)) else {
                continue;
            };
            // « 1 charge », « 2 charges »… en tête de titre : le nombre de crans
            // que la source reconnaît à ce sort.
            let paliers = groupes
                .iter()
                .filter_map(|g| {
                    let t = g.titre.trim();
                    let n: String = t.chars().take_while(char::is_ascii_digit).collect();
                    let reste = t[n.len()..].trim_start();
                    (reste.starts_with("charge")).then(|| n.parse::<u8>().ok())?
                })
                .max();
            let Some(paliers) = paliers else { continue };

            for line in &sort.lines {
                for b in &line.base_bonus {
                    let resource = match b {
                        BaseBonus::Steps { resource, .. }
                        | BaseBonus::PerResource { resource, .. }
                        // Le compteur, pas la garde : c'est lui qui porte les paliers.
                        | BaseBonus::PerResourceGated { resource, .. }
                        | BaseBonus::WhileResource { resource, .. } => resource,
                        // Ces deux-la lisent un reglage du scenario, pas une
                        // ressource : ils n'ont pas de plafond a confronter.
                        BaseBonus::PerExtraTarget { .. } | BaseBonus::PerMpUsed { .. } => continue,
                        // La ressource n'est ici qu'un drapeau de présence : son `cap`
                        // compte des ennemis, pas des paliers de charge.
                        BaseBonus::PerExtraTargetWhile { .. } => continue,
                    };
                    let Some(max) = plafonds.get(resource.as_str()) else {
                        continue;
                    };
                    verifies += 1;
                    if *max != paliers {
                        fautes.push(format!(
                            "{classe}: {} lit `{resource}` plafonné à {max}, \
                             quand DofusBook lui donne {paliers} palier(s) de charge",
                            sort.name.fr
                        ));
                    }
                }
            }
        }
    }

    assert!(
        verifies >= 10,
        "seulement {verifies} plafonds contrôlés : le test ne contrôle plus rien"
    );
    assert!(fautes.is_empty(), "{}", fautes.join("\n"));
}

/// Un bonus de dégâts de base décale une fourchette, il ne l'élargit jamais :
/// deux paliers de largeurs différentes sont deux lignes, pas un bonus.
#[test]
fn un_palier_ecrit_en_bonus_garde_une_largeur_constante() {
    let mut fautes = Vec::new();
    let mut verifies = 0usize;

    for (classe, breed) in CLASSES {
        let rs = ruleset(classe);
        let tk = toolkits(*breed);

        for sort in &rs.spells {
            let Some(groupes) = sort.dofusdb_id.filter(|d| !changes().contains(d)).and_then(|d| tk.get(&d)) else {
                continue;
            };
            // Un sort qui déclare son trou n'est pas une dérive silencieuse.
            if !sort.open_questions.is_empty() {
                continue;
            }
            let porte_un_bonus = sort.lines.iter().any(|l| {
                l.base_bonus
                    .iter()
                    .any(|b| matches!(b, BaseBonus::Steps { .. } | BaseBonus::PerResource { .. }))
            });
            if !porte_un_bonus {
                continue;
            }
            // Les fourchettes normales de ce sort, par élément. Une valeur fixe
            // s'écrit min sans max, de largeur nulle.
            let mut par_element: BTreeMap<&str, BTreeSet<(i64, i64)>> = BTreeMap::new();
            for g in groupes {
                for e in &g.effets {
                    if e.critique || !degats(&e.code) {
                        continue;
                    }
                    let element = &e.code[e.code.len() - 1..];
                    let haut = if e.max == 0 { e.min } else { e.max };
                    par_element
                        .entry(element)
                        .or_default()
                        .insert((e.min, haut));
                }
            }
            for (element, fourchettes) in par_element {
                if fourchettes.len() < 2 {
                    continue;
                }
                verifies += 1;
                let largeurs: BTreeSet<i64> = fourchettes.iter().map(|(a, b)| b - a).collect();
                if largeurs.len() > 1 {
                    fautes.push(format!(
                        "{classe}: {} écrit ses paliers {element} en bonus de dégâts de base, \
                         mais leurs largeurs sont {largeurs:?} : {fourchettes:?}. \
                         Un bonus décale une fourchette, il ne l'élargit pas ; \
                         il faut une ligne `active_at` par cran.",
                        sort.name.fr
                    ));
                }
            }
        }
    }

    assert!(
        verifies >= 10,
        "seulement {verifies} sorts à paliers contrôlés : le test ne contrôle plus rien"
    );
    assert!(fautes.is_empty(), "{}", fautes.join("\n"));
}

/// Les états qui frappent sur une poussée puis se retirent : la durée de leur
/// source dit combien de temps ils l'attendent, pas combien de fois ils frappent
/// (pour la décharge de la Flèche Tyrannique, c'est celle du poison qu'elle
/// retire). Les autres sont des poisons, dont la durée décide combien de fois ils
/// tombent : une durée d s'écrit `turns: d - 1` et tombe d fois.
const ETATS_CONSOMMES: &[&str] = &["flibuste", "fleche_tyrannique_decharge"];

#[test]
fn la_duree_d_un_etat_qui_frappe_est_celle_de_la_source() {
    let mut fautes = Vec::new();
    let mut verifies = 0usize;

    for (classe, breed) in CLASSES {
        let rs = ruleset(classe);
        let tk = toolkits(*breed);

        for res in &rs.resources {
            if res.while_present.is_empty() || ETATS_CONSOMMES.contains(&res.id.as_str()) {
                continue;
            }
            // La source du chiffre : déclarée, ou le sort de même identifiant,
            // ou celui qui applique l'état. C'est l'ordre qu'emploie le reste
            // du dépôt.
            let source = res
                .dofusdb_source
                .or_else(|| {
                    rs.spells
                        .iter()
                        .find(|s| s.id == res.id)
                        .and_then(|s| s.dofusdb_id)
                })
                .or_else(|| {
                    rs.spells
                        .iter()
                        .find(|s| {
                            s.effects.iter().any(|e| {
                                matches!(e, dofus_ruleset::Effect::Gain { resource, .. }
                                            if *resource == res.id)
                            })
                        })
                        .and_then(|s| s.dofusdb_id)
                });
            let Some(groupes) = source.and_then(|d| tk.get(&d)) else {
                continue;
            };
            // Les durées que la source attache aux lignes de dégâts de ce sort.
            // Aucune pour un état à déclencheur (l'Éboulement de l'Enutrof
            // répond à un retrait de Portée) : rien à comparer.
            let durees: BTreeSet<i64> = groupes
                .iter()
                .flat_map(|g| &g.effets)
                .filter(|e| degats(&e.code) && e.duree > 0)
                .map(|e| e.duree)
                .collect();
            if durees.is_empty() {
                continue;
            }
            let Some(duration) = &res.duration else {
                fautes.push(format!(
                    "{classe}: l'état `{}` frappe tout seul et n'a AUCUNE durée, \
                     quand DofusBook lui donne {durees:?}",
                    res.id
                ));
                continue;
            };
            verifies += 1;
            if !durees.contains(&(i64::from(duration.turns) + 1)) {
                fautes.push(format!(
                    "{classe}: l'état `{}` dure {} tour(s) ici (`turns: {}`), {durees:?} chez \
                     DofusBook. Un poison de durée N tombe N fois : l'écart est un facteur, pas \
                     un détail.",
                    res.id,
                    duration.turns + 1,
                    duration.turns
                ));
            }
        }
    }

    assert!(
        verifies >= 8,
        "seulement {verifies} durées contrôlées : le test ne contrôle plus rien"
    );
    assert!(fautes.is_empty(), "{}", fautes.join("\n"));
}

/// Une fourchette écrite à la main existe dans les toolkits : le pendant de
/// `authored_magnitudes`, sur l'autre source. Une valeur absente de l'une des
/// deux est une valeur lue au mauvais endroit de la fiche.
#[test]
fn aucune_fourchette_ecrite_ne_manque_a_dofusbook() {
    let mut fautes = Vec::new();
    let mut verifiees = 0usize;

    for (classe, breed) in CLASSES {
        let rs = ruleset(classe);
        let tk = toolkits(*breed);

        let mut controler = |quoi: &str, id: u32, element: Option<String>, r: (i32, i32)| {
            let Some(groupes) = tk.get(&id) else { return };
            let mut connues = BTreeSet::new();
            for g in groupes {
                for e in &g.effets {
                    if !degats(&e.code) {
                        continue;
                    }
                    let haut = if e.max == 0 { e.min } else { e.max };
                    connues.insert((e.code[e.code.len() - 1..].to_string(), e.min, haut));
                }
            }
            if connues.is_empty() {
                return;
            }
            verifiees += 1;
            let (a, b) = (i64::from(r.0), i64::from(r.1));
            let lettre = element.as_deref().map(|e| match e {
                "earth" => "t",
                "fire" => "f",
                "air" => "a",
                "water" => "e",
                _ => "n",
            });
            let trouve = match lettre {
                Some(l) => connues.contains(&(l.to_string(), a, b)),
                None => connues.iter().any(|(_, x, y)| *x == a && *y == b),
            };
            if !trouve {
                fautes.push(format!(
                    "{classe}: {quoi} porte [{a}, {b}], que DofusBook ne donne pas au sort {id} \
                     dont les fourchettes sont {connues:?}"
                ));
            }
        };

        let nom_element = |l: &dofus_ruleset::LineDef| -> Option<String> {
            if l.best_element || l.worst_element {
                None
            } else {
                Some(format!("{:?}", l.element?).to_lowercase())
            }
        };

        for s in &rs.spells {
            let Some(id) = s.dofusdb_id else { continue };
            if changes().contains(&id) {
                continue;
            }
            for l in &s.lines {
                if let Maybe::Known(r) = &l.normal {
                    controler(&s.id, id, nom_element(l), *r);
                }
            }
        }

        // Les lignes des états aussi, pas seulement celles des sorts.
        for res in &rs.resources {
            // Le sort source, declare ou bien celui du meme nom : un etat
            // s'ecrit a cote du sort qui l'applique, et les deux portent le
            // meme identifiant par convention.
            let id = res.dofusdb_source.or_else(|| {
                rs.spells
                    .iter()
                    .find(|s| s.id == res.id)
                    .and_then(|s| s.dofusdb_id)
            });
            let Some(id) = id.filter(|i| !changes().contains(i)) else { continue };
            for eff in &res.while_present {
                for l in &eff.lines {
                    if let Maybe::Known(r) = &l.normal {
                        controler(&res.id, id, nom_element(l), *r);
                    }
                }
            }
        }
    }

    assert!(
        verifiees > 100,
        "seulement {verifiees} fourchettes contrôlées : le test ne contrôle plus rien"
    );
    assert!(fautes.is_empty(), "{}", fautes.join("\n"));
}
