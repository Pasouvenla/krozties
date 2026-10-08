//! Les classes annoncées complètes le sont vraiment.
//!
//! Un pourcentage se calcule sur ce que le fichier déclare et ne voit pas ce
//! qu'il oublie. Ces tests croisent la donnée du jeu et le fichier, classe par
//! classe :
//!
//! 1. aucun sort crédité de dégâts par DofusBook n'est compté à zéro ;
//! 2. tout sort qui porte des lignes les résout ;
//! 3. aucune fourchette ne reste inconnue après la fusion ;
//! 4. les sorts absents du fichier ne frappent pas.
//!
//! Les générateurs de Téléfrag du Xélor restent hors du fichier : leur rendement
//! dépend des positions, et le budget `telefrag_per_turn` du joueur l'approche.

use dofus_ruleset::{snapshot::Snapshot, Maybe, Ruleset};
use std::collections::BTreeSet;

/// Les classes annoncées complètes, et leur identifiant de classe. Toute classe
/// ajoutée à `FINIES` dans `classes_finies.rs` entre ici aussi.
const COMPLETES: &[(&str, u32)] = &[
    ("xelor", 5),
    ("iop", 8),
    ("enutrof", 3),
    ("eliotrope", 16),
    ("pandawa", 12),
    ("ouginak", 18),
    ("roublard", 13),
    ("osamodas", 2),
    ("feca", 1),
    ("steamer", 15),
    ("forgelance", 20),
    ("sram", 4),
    ("sadida", 10),
    ("sacrieur", 11),
    ("zobal", 14),
    ("cra", 9),
    ("eniripsa", 7),
];

fn racine() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../..").to_string()
}

fn regles(classe: &str, breed: u32) -> Ruleset {
    let r = racine();
    let mut rs = Ruleset::load(format!("{r}/data/rulesets/{classe}.yaml"))
        .unwrap_or_else(|e| panic!("{classe} : {e}"));
    let snap = Snapshot::load(format!("{r}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn dofusbook(breed: u32) -> serde_json::Value {
    let r = racine();
    let brut = std::fs::read_to_string(format!("{r}/data/snapshots/dofusbook-{breed}.json"))
        .unwrap();
    serde_json::from_str(&brut).unwrap()
}

/// Les codes que DofusBook met devant une fourchette de dégâts ou de vol de vie :
/// `g*` les dégâts d'un glyphe, `p*` ceux d'un piège, `dpe` une fourchette dans
/// le pire élément, `dv` des dégâts en pourcentage de vie. Restent dehors
/// les soins et les déplacements : `ppv`, `pdv*`, `sme`, `pou`, `pct`.
const CODES: &[&str] = &[
    "dt", "de", "da", "df", "dn", "dme", "vt", "ve", "va", "vf", "vn", "vme", "ga", "ge", "gt",
    "gf", "gn", "pia", "pie", "pit", "pif", "pin", "dpe", "dv",
];

/// Les codes d'une pose au sol, glyphe ou piège : ce sont les dégâts propres du
/// sort, pas ceux d'une invocation, d'où leur place dans le filtre ci-dessous.
const CODES_AU_SOL: &[&str] = &[
    "ga", "ge", "gt", "gf", "gn", "pia", "pie", "pit", "pif", "pin",
];

/// Les codes de dégâts que DofusBook met devant les fourchettes de ce sort.
fn codes_de_degats(sort: &serde_json::Value) -> Vec<String> {
    let vide: Vec<serde_json::Value> = Vec::new();
    let mut out = Vec::new();
    for g in sort["g"].as_array().unwrap_or(&vide) {
        for e in g[2].as_array().unwrap_or(&vide) {
            if let Some(c) = e[0].as_str() {
                if CODES.contains(&c) && !out.iter().any(|x| x == c) {
                    out.push(c.to_string());
                }
            }
        }
    }
    out
}

/// Aucun sort crédité de dégâts par DofusBook n'est compté à zéro sans le dire.
#[test]
fn aucun_sort_offensif_n_est_compte_a_zero() {
    for (classe, breed) in COMPLETES {
        verifier_aucun_zero(classe, *breed);
    }
}

fn verifier_aucun_zero(classe: &str, breed: u32) {
    let rs = regles(classe, breed);
    let db = dofusbook(breed);
    let vide: Vec<serde_json::Value> = Vec::new();
    let mut muets = Vec::new();
    for sort in db["sorts"].as_array().unwrap_or(&vide) {
        let codes = codes_de_degats(sort);
        if codes.is_empty() {
            continue;
        }
        let id = sort["id"].as_u64().unwrap_or(0) as u32;
        let Some(s) = rs.spells.iter().find(|s| s.dofusdb_id == Some(id)) else {
            // Absent du fichier : couvert par le test du périmètre, pas par
            // celui-ci.
            continue;
        };
        // Hors rotation : ses dégâts ne sont pas comptés, exprès, et la
        // couverture le nomme à part.
        if s.outside_rotation.is_some() {
            continue;
        }
        // Les invocations frappent à leur propre tour, pas au lancer, et ne
        // comptent pas ici. Le signal est le texte du sort, pas son nom (le
        // Mot d'Amitié de l'Eniripsa invoque un Lapino sans le dire). L'excuse
        // ne vaut pas pour une pose au sol : un glyphe frappe pour le compte du
        // lanceur, même quand le sort invoque aussi.
        let au_sol = codes.iter().any(|c| CODES_AU_SOL.contains(&c.as_str()));
        // Un piège ou un glyphe que la cible déclenche : le fichier ne lui
        // écrit ni ligne ni effet, l'application les tire de la donnée quand le
        // joueur déclare `pieges_declenches` (`tout_piege_declare_recoit_ses_lignes`).
        // La Concentration de Chakra vole sur chacun de ces pièges, par des lignes
        // que l'application leur ajoute.
        let chakra = s.effects.iter().any(|e| {
            matches!(e, dofus_ruleset::Effect::Gain { resource, .. } if resource == "concentration_de_chakra")
        });
        if rs.resources.iter().any(|r| r.id == "pieges_declenches")
            && s.lines.is_empty()
            && (chakra || (s.effects.is_empty() && (au_sol || !s.trap_lines.is_empty())))
        {
            continue;
        }
        let texte = s.note.clone().unwrap_or_default();
        if !au_sol
            && (texte.contains("Invoque")
                || texte.contains("invoque")
                || texte.contains("Pose une Tourelle"))
        {
            continue;
        }
        let differe = s
            .effects
            .iter()
            .any(|e| matches!(e, dofus_ruleset::Effect::Schedule { .. }));
        let porteur = rs.resources.iter().any(|r| {
            !r.while_present.is_empty()
                && s.effects.iter().any(|e| {
                    matches!(
                        e,
                        dofus_ruleset::Effect::Gain { resource, .. } if *resource == r.id
                    )
                })
        });
        if s.lines.is_empty() && !differe && !porteur {
            muets.push(s.name.fr.clone());
        }
    }
    assert!(
        muets.is_empty(),
        "{classe} : DofusBook leur donne des dégâts et le fichier les compte à zéro : {muets:?}"
    );
}

/// Tout sort du fichier qui porte des lignes les résout : une ligne sans
/// `effect: damage` ne frappe jamais. Un sort sans ligne ni effet n'est pas un
/// défaut : c'est un sort utilitaire, gardé pour qu'un build puisse le lister.
#[test]
fn aucun_sort_ne_porte_de_lignes_sans_les_resoudre() {
    let mut souci = Vec::new();
    for (classe, breed) in COMPLETES {
        let rs = regles(classe, *breed);
        for s in &rs.spells {
            let frappe = s
                .effects
                .iter()
                .any(|e| matches!(e, dofus_ruleset::Effect::Damage));
            if !s.lines.is_empty() && !frappe {
                souci.push(format!(
                    "{classe} / {} : des lignes, aucun `effect: damage`",
                    s.name.fr
                ));
            }
        }
    }
    assert!(souci.is_empty(), "{souci:#?}");
}

/// Aucune magnitude ne reste inconnue une fois l'instantané fusionné : le
/// solveur traiterait une fourchette `unknown` comme un zéro.
#[test]
fn aucune_fourchette_ne_reste_inconnue() {
    let mut trous = BTreeSet::new();
    for (classe, breed) in COMPLETES {
        let rs = regles(classe, *breed);
        for s in &rs.spells {
            // Une ligne en pourcentage de vie n'a pas de fourchette, par nature.
            for l in s.lines.iter().filter(|l| !l.sans_fourchette()) {
                if matches!(l.normal, Maybe::Unknown(_)) {
                    trous.insert(format!("{classe} / {} : normal", s.name.fr));
                }
                if s.crit.can_crit && matches!(l.critical, Maybe::Unknown(_)) {
                    trous.insert(format!("{classe} / {} : critique", s.name.fr));
                }
            }
        }
    }
    assert!(
        trous.is_empty(),
        "des fourchettes que la fusion n'a pas su remplir : {trous:?}"
    );
}

/// Le fichier couvre tous les sorts offensifs de la classe. Le dénominateur se
/// prend sur l'instantané : le toolkit DofusBook d'une classe liste aussi les
/// sorts d'objets qu'elle peut porter (« Marteau de Moon »).
#[test]
fn les_sorts_absents_du_fichier_ne_frappent_pas() {
    for (classe, breed) in COMPLETES {
        verifier_absents(classe, *breed);
    }
}

fn verifier_absents(classe: &str, breed: u32) {
    let rs = regles(classe, breed);
    let db = dofusbook(breed);
    let vide: Vec<serde_json::Value> = Vec::new();
    let ecrits: BTreeSet<u32> = rs.spells.iter().filter_map(|s| s.dofusdb_id).collect();
    // Les sorts de la classe, tels que l'instantané les rattache à elle.
    let brut = std::fs::read_to_string(format!(
        "{}/data/snapshots/breed-{breed}.json",
        racine()
    ))
    .unwrap();
    let snap: serde_json::Value = serde_json::from_str(&brut).unwrap();
    let de_la_classe: BTreeSet<u32> = snap["spells"]
        .as_array()
        .unwrap_or(&vide)
        .iter()
        .filter_map(|s| s["id"].as_u64().map(|i| i as u32))
        .collect();
    let mut absents_qui_frappent = Vec::new();
    for sort in db["sorts"].as_array().unwrap_or(&vide) {
        let id = sort["id"].as_u64().unwrap_or(0) as u32;
        if ecrits.contains(&id) || !de_la_classe.contains(&id) {
            continue;
        }
        if !codes_de_degats(sort).is_empty() {
            absents_qui_frappent.push(sort["n"].as_str().unwrap_or("?").to_string());
        }
    }
    assert!(
        absents_qui_frappent.is_empty(),
        "{classe} : absents du fichier alors qu'ils frappent : {absents_qui_frappent:?}"
    );
}
