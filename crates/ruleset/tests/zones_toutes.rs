//! Chaque ligne de dégâts a sa zone, et une zone écrite à la main existe dans
//! la donnée de son sort. Sans zone, une ligne frappe une seule cible et ne se
//! dessine pas : les lignes à palier, en meilleur élément ou en pourcentage de
//! vie y sont les plus exposées.

use dofus_ruleset::{snapshot::Snapshot, Effect, LineDef, Ruleset};

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

/// Les lignes d'un sort : les siennes, sa charge différée, ses modes.
fn lignes(s: &dofus_ruleset::SpellDef) -> Vec<(String, &LineDef)> {
    let mut out: Vec<(String, &LineDef)> =
        s.lines.iter().enumerate().map(|(i, l)| (format!("{}.lines[{i}]", s.id), l)).collect();
    for e in &s.effects {
        if let Effect::Schedule { id, payload, .. } = e {
            for (j, l) in payload.iter().enumerate() {
                out.push((format!("{}.schedule[{id}].payload[{j}]", s.id), l));
            }
        }
    }
    for (k, m) in s.modes.iter().enumerate() {
        for (j, l) in m.lines.iter().enumerate() {
            out.push((format!("{}.modes[{k}].lines[{j}]", s.id), l));
        }
    }
    out
}

fn charger(classe: &str, breed: u32) -> (Ruleset, Ruleset, Snapshot) {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let ecrit = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
        .unwrap_or_else(|e| panic!("{classe}: {e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json"))
        .unwrap_or_else(|e| panic!("{classe}: {e}"));
    let mut fusionne = ecrit.clone();
    let rapport = fusionne.merge_snapshot(&snap);
    let a_ecrire: Vec<&String> = rapport.unmatched.iter().filter(|u| u.contains("zone à écrire")).collect();
    assert!(a_ecrire.is_empty(), "{classe} : {a_ecrire:#?}");
    (ecrit, fusionne, snap)
}

/// Les lignes d'un état : ce que frappent un poison, un glyphe, une marque.
fn lignes_d_etat(r: &dofus_ruleset::ResourceDef) -> Vec<(String, &LineDef)> {
    r.while_present
        .iter()
        .enumerate()
        .flat_map(|(j, e)| {
            e.lines
                .iter()
                .enumerate()
                .map(move |(i, l)| (format!("{}.while_present[{j}].lines[{i}]", r.id), l))
        })
        .collect()
}

#[test]
fn chaque_ligne_de_degats_a_sa_zone() {
    let mut sans = Vec::new();
    let mut comptees = 0usize;
    for (classe, breed) in CLASSES {
        let (_, fusionne, _) = charger(classe, *breed);
        for s in &fusionne.spells {
            for (ou, l) in lignes(s) {
                comptees += 1;
                if l.area.is_none() {
                    sans.push(format!("{classe}: {ou}"));
                }
            }
        }
        for r in &fusionne.resources {
            for (ou, l) in lignes_d_etat(r) {
                comptees += 1;
                if l.area.is_none() {
                    sans.push(format!("{classe}: {ou}"));
                }
            }
        }
    }
    assert!(comptees > 600, "seulement {comptees} lignes : le test ne contrôle plus rien");
    assert!(sans.is_empty(), "{} ligne(s) sans zone :\n{}", sans.len(), sans.join("\n"));
}

/// Une zone a sa taille : sans elle, la géométrie dessine la forme à la taille
/// zéro (le poison de la Distillation, `{ shape: G, max_targets: 9 }`, sur une
/// seule case quand la rotation lui compte neuf cibles). Seul un point s'en
/// passe.
#[test]
fn chaque_zone_a_sa_taille() {
    let mut sans = Vec::new();
    let mut zones = 0usize;
    for (classe, breed) in CLASSES {
        let (_, fusionne, _) = charger(classe, *breed);
        let toutes = fusionne
            .spells
            .iter()
            .flat_map(lignes)
            .chain(fusionne.resources.iter().flat_map(lignes_d_etat));
        for (ou, l) in toutes {
            if let Some(a) = &l.area {
                zones += 1;
                if a.size.is_none() && a.shape != 'P' {
                    sans.push(format!("{classe}: {ou} ({})", a.shape));
                }
            }
        }
    }
    assert!(zones > 600, "seulement {zones} zones : le test ne contrôle plus rien");
    assert!(sans.is_empty(), "{} zone(s) sans taille :\n{}", sans.len(), sans.join("\n"));
}

/// Les zones lues sur un SOUS-SORT que l'instantané de la classe ne porte pas,
/// chacune nommée avec sa source. Une de plus fait tomber le test ci-dessous :
/// il faut alors venir ici et dire d'où elle vient.
const ZONES_DE_SOUS_SORTS: &[(&str, &str)] = &[(
    "forgelance: eclipse.schedule[disque_de_sigel].payload[0]",
    "Disque de Sigel, sort 23836 grade 6, effet 2822 : cercle de taille 3",
)];

/// ⚠️ UNE ZONE ÉCRITE N'EST PAS INVENTÉE : elle existe parmi celles que la
/// donnée porte pour ce sort, lignes de dégâts ou autres effets. La fusion ne
/// l'écrase pas, la même règle que pour les fourchettes.
#[test]
fn une_zone_ecrite_existe_dans_la_donnee() {
    let mut fautes = Vec::new();
    let mut ecrites = 0usize;
    let mut sous_sorts = 0usize;
    for (classe, breed) in CLASSES {
        let (ecrit, _, snap) = charger(classe, *breed);
        for s in &ecrit.spells {
            let Some(source) = s.dofusdb_id.and_then(|id| snap.spells.iter().find(|x| x.id == id)) else {
                continue;
            };
            let zones: Vec<(char, Option<u8>, u8)> = source
                .levels
                .iter()
                .flat_map(|lv| {
                    lv.normal_lines
                        .iter()
                        .chain(&lv.critical_lines)
                        .filter_map(|l| l.zone)
                        .chain(lv.other_effects.iter().filter_map(|e| e.zone))
                })
                .map(|z| (z.shape, z.size, z.size2))
                .collect();
            for (ou, l) in lignes(s) {
                let Some(a) = &l.area else { continue };
                ecrites += 1;
                let chemin = format!("{classe}: {ou}");
                if ZONES_DE_SOUS_SORTS.iter().any(|(c, _)| *c == chemin) {
                    sous_sorts += 1;
                    continue;
                }
                if !zones.contains(&(a.shape, a.size, a.size2)) {
                    fautes.push(format!(
                        "{classe}: {ou} écrit {}{:?}/{} que la donnée du sort ne porte pas : {zones:?}",
                        a.shape, a.size, a.size2
                    ));
                }
            }
        }
    }
    assert!(ecrites > 0);
    assert_eq!(sous_sorts, ZONES_DE_SOUS_SORTS.len(), "une zone de sous-sort nommée ici n'est plus écrite");
    assert!(fautes.is_empty(), "{} zone(s) écrite(s) hors donnée :\n{}", fautes.len(), fautes.join("\n"));
}
