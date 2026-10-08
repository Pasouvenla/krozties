//! Toute fourchette écrite à la main se retrouve chez DofusBook, sur le même
//! sort.
//!
//! La plupart des fourchettes s'écrivent `normal: unknown` et viennent de
//! l'instantané. Ce test garde les autres, tapées parce que l'instantané ne les
//! porte pas (dégâts passant par un effet, poisons posés sur un état). Il compare
//! sort par sort : une borne juste sur le mauvais sort reste une erreur. Une
//! fourchette portée par un état se rattache aux sorts qui l'appliquent.

use dofus_ruleset::{BaseBonus, Ruleset};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const TOUTES: &[(&str, u32)] = &[
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

fn racine() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../..").to_string()
}

/// Les sorts qu'une note de sortie a changés : les toolkits, relevés en 3.6.12,
/// ne les connaissent pas à leurs nouveaux chiffres.
fn changes() -> BTreeSet<u64> {
    let manifeste = std::fs::read_to_string(format!("{}/data/version.json", racine())).unwrap();
    dofus_ruleset::sorts_changes_depuis_le_releve(&manifeste).into_iter().map(u64::from).collect()
}

/// Les bornes que DofusBook publie, sort par sort. Une entrée vaut
/// `[code, critique, bas, haut, delai]` ; un `haut` à zéro signale une valeur
/// unique (les 35 % de la Punition), comparée à elle-même.
fn bornes_par_sort(breed: u32) -> BTreeMap<u64, BTreeSet<(i64, i64)>> {
    let brut = std::fs::read_to_string(format!(
        "{}/data/snapshots/dofusbook-{breed}.json",
        racine()
    ))
    .unwrap();
    let db: Value = serde_json::from_str(&brut).unwrap();
    let mut out = BTreeMap::new();
    for sort in db["sorts"].as_array().unwrap() {
        let mut s = BTreeSet::new();
        for groupe in sort["g"].as_array().unwrap_or(&Vec::new()) {
            for e in groupe[2].as_array().unwrap_or(&Vec::new()) {
                let Some(t) = e.as_array() else { continue };
                if t.len() < 4 {
                    continue;
                }
                let (lo, hi) = (t[2].as_i64().unwrap_or(0), t[3].as_i64().unwrap_or(0));
                s.insert((lo, hi));
                s.insert((lo, lo));
            }
        }
        out.insert(sort["id"].as_u64().unwrap_or(0), s);
    }
    out
}

/// Les `normal: [a, b]` et `critical: [a, b]` écrits en toutes lettres.
fn fourchettes(bloc: &str) -> Vec<(i64, i64)> {
    bloc.lines()
        .filter_map(|l| {
            let l = l.trim();
            let reste = l
                .strip_prefix("normal: [")
                .or_else(|| l.strip_prefix("critical: ["))?;
            let reste = reste.strip_suffix(']')?;
            let (a, b) = reste.split_once(',')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        })
        .collect()
}

#[test]
fn aucune_fourchette_ecrite_a_la_main_n_est_inventee() {
    let mut inventees = Vec::new();
    let mut examinees = 0usize;

    for (classe, breed) in TOUTES {
        let bornes = bornes_par_sort(*breed);
        let fichier =
            std::fs::read_to_string(format!("{}/data/rulesets/{classe}.yaml", racine()))
                .unwrap();
        let (tete, corps) = fichier.split_once("\nspells:").unwrap();

        // Quelles ressources chaque sort applique, pour rattacher une
        // fourchette d'état au sort qui la publie.
        let mut applique: BTreeMap<String, BTreeSet<u64>> = BTreeMap::new();
        let mut blocs: Vec<(String, u64)> = Vec::new();
        for bloc in corps.split("\n  - id: ").skip(1) {
            let Some(id) = id_dofusdb(bloc) else { continue };
            blocs.push((bloc.to_string(), id));
            for l in bloc.lines() {
                if let Some(r) = l.trim().strip_prefix("resource: ") {
                    applique.entry(r.trim().to_string()).or_default().insert(id);
                }
            }
        }

        for (bloc, id) in &blocs {
            if changes().contains(id) {
                continue;
            }
            let vide = BTreeSet::new();
            let attendues = bornes.get(id).unwrap_or(&vide);
            for f in fourchettes(bloc) {
                examinees += 1;
                if !attendues.contains(&f) {
                    inventees.push(format!(
                        "{classe} / sort {id} : [{}, {}] n'est nulle part dans ce que \
                         DofusBook publie pour ce sort",
                        f.0, f.1
                    ));
                }
            }
        }

        for bloc in tete.split("\n  - id: ").skip(1) {
            let nom = bloc.lines().next().unwrap_or("").trim().to_string();
            let vide = BTreeSet::new();
            let porteurs = applique.get(&nom).unwrap_or(&vide);
            for f in fourchettes(bloc) {
                examinees += 1;
                let publiee = porteurs
                    .iter()
                    .any(|id| bornes.get(id).is_some_and(|s| s.contains(&f)));
                if !publiee {
                    inventees.push(format!(
                        "{classe} / état `{nom}` : [{}, {}] n'est publiée par aucun des \
                         {} sort(s) qui l'appliquent",
                        f.0,
                        f.1,
                        porteurs.len()
                    ));
                }
            }
        }
    }

    assert!(
        examinees >= 250,
        "ce test n'a lu que {examinees} fourchette(s) : il ne prouve plus rien"
    );
    inventees.sort_unstable();
    assert!(inventees.is_empty(), "{}", inventees.join("\n"));
}

fn id_dofusdb(bloc: &str) -> Option<u64> {
    bloc.lines()
        .find_map(|l| l.trim().strip_prefix("dofusdb_id: ")?.trim().parse().ok())
}

/// Les deux endroits où DofusBook est moins complet que l'instantané, sans le
/// contredire : le Torrent Arcanique du Huppermage, dont il ne publie pas le rang
/// « zéro combinaison », et la seconde ligne de fin de tour du Feu de Brousse du
/// Sadida, dont il ne donne pas la critique.
const OMISSIONS_DOFUSBOOK: &[(&str, u64, i64, i64)] =
    &[("huppermage", 14342, 2, 2), ("sadida", 13568, 20, 23)];

/// Les fourchettes que la fusion ramène de l'instantané disent la même chose que
/// DofusBook : une divergence ferait tourner le solveur sur des chiffres que le
/// joueur ne retrouverait pas dans son encyclopédie.
#[test]
fn la_fusion_dit_la_meme_chose_que_dofusbook() {
    let mut ecarts = Vec::new();
    let mut examinees = 0usize;

    for (classe, breed) in TOUTES {
        let bornes = bornes_par_sort(*breed);
        let fichier =
            std::fs::read_to_string(format!("{}/data/rulesets/{classe}.yaml", racine()))
                .unwrap();
        // Seuls les sorts que le fichier référence : l'instantané en porte
        // d'autres, que le parc n'a pas repris.
        let references: BTreeSet<u64> = fichier
            .lines()
            .filter_map(|l| l.trim().strip_prefix("dofusdb_id: ")?.trim().parse().ok())
            .collect();

        let brut = std::fs::read_to_string(format!(
            "{}/data/snapshots/breed-{breed}.json",
            racine()
        ))
        .unwrap();
        let snap: Value = serde_json::from_str(&brut).unwrap();

        for sort in snap["spells"].as_array().unwrap() {
            let id = sort["id"].as_u64().unwrap_or(0);
            if !references.contains(&id) || changes().contains(&id) {
                continue;
            }
            let nom = sort["name"]["fr"].as_str().unwrap_or("?");
            let Some(niveau) = sort["levels"].as_array().unwrap().last() else {
                continue;
            };
            let vide = BTreeSet::new();
            let publiees = bornes.get(&id).unwrap_or(&vide);
            for clef in ["normal_lines", "critical_lines"] {
                for ligne in niveau[clef].as_array().unwrap_or(&Vec::new()) {
                    let Some(r) = ligne["range"].as_array() else {
                        continue;
                    };
                    let f = (r[0].as_i64().unwrap_or(0), r[1].as_i64().unwrap_or(0));
                    examinees += 1;
                    if publiees.contains(&f)
                        || OMISSIONS_DOFUSBOOK.contains(&(classe, id, f.0, f.1))
                    {
                        continue;
                    }
                    ecarts.push(format!(
                        "{classe} / {nom} : la fusion ramène [{}, {}] et DofusBook ne                          publie pas cette fourchette pour ce sort",
                        f.0, f.1
                    ));
                }
            }
        }
    }

    assert!(
        examinees >= 1000,
        "ce test n'a lu que {examinees} fourchette(s) : il ne prouve plus rien"
    );
    ecarts.sort_unstable();
    ecarts.dedup();
    assert!(ecarts.is_empty(), "{}", ecarts.join("\n"));
}

/// Les codes que DofusBook met devant une fourchette de dégâts.
const CODES: &[&str] = &[
    "dt", "de", "da", "df", "dn", "dme", "vt", "ve", "va", "vf", "vn", "vme", "ga", "ge", "gt",
    "gf", "gn", "pia", "pie", "pit", "pif", "pin", "dpe", "dv",
];

/// Les bornes basses des groupes d'un sort, dans l'ordre où l'encyclopédie les
/// affiche, jet normal seulement.
fn bornes_basses(breed: u32) -> BTreeMap<u64, Vec<i64>> {
    let brut = std::fs::read_to_string(format!(
        "{}/data/snapshots/dofusbook-{breed}.json",
        racine()
    ))
    .unwrap();
    let db: Value = serde_json::from_str(&brut).unwrap();
    let mut out = BTreeMap::new();
    for sort in db["sorts"].as_array().unwrap() {
        let mut suite = Vec::new();
        for groupe in sort["g"].as_array().unwrap_or(&Vec::new()) {
            let bas: Vec<i64> = groupe[2]
                .as_array()
                .unwrap_or(&Vec::new())
                .iter()
                .filter_map(|e| {
                    let t = e.as_array()?;
                    let code = t[0].as_str()?;
                    (CODES.contains(&code) && t[1].as_i64() == Some(0)).then(|| t[2].as_i64())?
                })
                .collect();
            if let Some(m) = bas.into_iter().min() {
                suite.push(m);
            }
        }
        out.insert(sort["id"].as_u64().unwrap_or(0), suite);
    }
    out
}

/// Les montants qu'un bonus de base ajoute, par cran.
fn montants(b: &BaseBonus) -> Vec<i32> {
    match b {
        BaseBonus::Steps { steps, .. } => steps.clone(),
        BaseBonus::PerResource { amount, .. }
        | BaseBonus::PerResourceGated { amount, .. }
        | BaseBonus::PerMpUsed { amount, .. }
        | BaseBonus::PerExtraTarget { amount, .. }
        | BaseBonus::PerExtraTargetWhile { amount, .. }
        | BaseBonus::WhileResource { amount, .. } => vec![*amount],
    }
}

/// Ce qu'un cran ajoute se vérifie aussi : l'encyclopédie publie les paliers
/// (Muselière de l'Ouginak : 37-41, puis 59-63, 81-85…), et l'écart entre deux
/// paliers est le montant que ce test recalcule.
#[test]
fn ce_qu_un_cran_ajoute_vient_des_paliers_de_dofusbook() {
    let mut faux = Vec::new();
    let mut examines = 0usize;

    for (classe, breed) in TOUTES {
        let bornes = bornes_basses(*breed);
        let regles = Ruleset::load(format!("{}/data/rulesets/{classe}.yaml", racine()))
            .unwrap_or_else(|e| panic!("{classe} : {e}"));

        let ecarts_de = |id: u32| -> BTreeSet<i32> {
            bornes
                .get(&u64::from(id))
                .map(|suite| {
                    suite
                        .windows(2)
                        .filter(|p| p[1] > p[0])
                        .map(|p| (p[1] - p[0]) as i32)
                        .collect()
                })
                .unwrap_or_default()
        };

        for sort in &regles.spells {
            let Some(id) = sort.dofusdb_id else { continue };
            if changes().contains(&u64::from(id)) {
                continue;
            }
            let ecarts = ecarts_de(id);
            for ligne in &sort.lines {
                for bonus in &ligne.base_bonus {
                    for m in montants(bonus) {
                        examines += 1;
                        if !ecarts.contains(&m) {
                            faux.push(format!(
                                "{classe} / {} : un cran vaut {m} dans le fichier, et les \
                                 paliers de DofusBook donnent {ecarts:?}",
                                sort.name.fr
                            ));
                        }
                    }
                }
            }
        }

        // Les lignes des états aussi : un bonus peut vivre sur la ligne du
        // sort et sur celle du poison qu'il applique (la Distillation du
        // Pandawa). Une fourchette d'état se rattache aux sorts qui l'appliquent.
        for etat in &regles.resources {
            let porteurs: Vec<u32> = regles
                .spells
                .iter()
                .filter(|s| {
                    s.effects.iter().any(|e| {
                        matches!(e, dofus_ruleset::Effect::Gain { resource, .. } if *resource == etat.id)
                    })
                })
                .filter_map(|s| s.dofusdb_id)
                .collect();
            for effet in &etat.while_present {
                for ligne in &effet.lines {
                    for bonus in &ligne.base_bonus {
                        for m in montants(bonus) {
                            examines += 1;
                            if !porteurs.iter().any(|id| ecarts_de(*id).contains(&m)) {
                                faux.push(format!(
                                    "{classe} / état `{}` : un cran vaut {m}, et aucun des \
                                     {} sort(s) qui l'appliquent n'a cet écart chez DofusBook",
                                    etat.id,
                                    porteurs.len()
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    assert!(
        examines >= 15,
        "ce test n'a lu que {examines} montant(s) : il ne prouve plus rien"
    );
    faux.sort_unstable();
    assert!(faux.is_empty(), "{}", faux.join("\n"));
}
