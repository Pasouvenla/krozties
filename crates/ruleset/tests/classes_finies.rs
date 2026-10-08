//! Les classes annoncées finies le sont, et le restent : chaque sort qui frappe
//! a ses chiffres et sa mécanique. Le test tient aussi dans l'autre sens, une
//! classe en chantier doit l'être pour de bon : une liste qui n'exclurait
//! personne ne prouverait rien.

use dofus_ruleset::{snapshot::Snapshot, Ruleset};

/// Les classes tenues pour finies, avec leur identifiant de classe.
const FINIES: &[(&str, u32)] = &[
    ("xelor", 5),
    ("iop", 8),
    ("enutrof", 3),
    ("eliotrope", 16),
    ("pandawa", 12),
    ("ouginak", 18),
    ("roublard", 13),
    ("osamodas", 2),
    ("huppermage", 17),
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

/// Les classes en chantier, avec ce qu'il leur reste. Presentes ici pour que le
/// test au-dessus ne puisse pas passer en declarant tout le monde fini.
const EN_CHANTIER: &[(&str, u32)] = &[
    // Des effets restent non modélisés : questions ouvertes de son fichier.
    ("ecaflip", 6),
];

fn couverture(nom: &str, breed: u32) -> dofus_ruleset::snapshot::Coverage {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{nom}.yaml"))
        .unwrap_or_else(|e| panic!("{nom}: {e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json"))
        .unwrap_or_else(|e| panic!("breed-{breed}: {e}"));
    rs.merge_snapshot(&snap);
    snap.coverage(&rs)
}

#[test]
fn les_classes_annoncees_finies_le_sont() {
    let mut fautes = Vec::new();
    for (nom, breed) in FINIES {
        let c = couverture(nom, *breed);
        if !c.is_complete() {
            fautes.push(format!(
                "{nom} : {} % complet, {} sort(s) partiel(s) {:?}, {} manquant(s)",
                c.percent_complete(),
                c.partial.len(),
                c.partial,
                c.missing.len()
            ));
        }
    }
    assert!(fautes.is_empty(), "{}", fautes.join("\n"));
}

/// Les classes que le pourcentage dit finies et que `classes_completes_verifiees.rs`
/// refuse : un sort dont tous les dégâts passent par une pose au sol n'a aucune
/// ligne dans l'instantané, et échappe au dénominateur. Les deux listes
/// partitionnent les dix-neuf classes : une classe qui se termine fait tomber le
/// test, et le maintenir revient à la déplacer.
const COMPLETES_EN_APPARENCE: &[&str] = &[];

#[test]
fn une_classe_en_chantier_en_est_vraiment_une() {
    let mut fautes = Vec::new();
    for (nom, breed) in EN_CHANTIER {
        let c = couverture(nom, *breed);
        if c.is_complete() && !COMPLETES_EN_APPARENCE.contains(nom) {
            fautes.push(format!(
                "{nom} est complet a {} % : le passer dans FINIES",
                c.percent_complete()
            ));
        }
    }
    assert!(fautes.is_empty(), "{}", fautes.join("\n"));

    // La partition, sans quoi vider EN_CHANTIER suffirait a faire taire le
    // test ci-dessus.
    let mut toutes: Vec<&str> = FINIES.iter().chain(EN_CHANTIER).map(|(n, _)| *n).collect();
    let combien = toutes.len();
    toutes.sort_unstable();
    toutes.dedup();
    assert_eq!(
        toutes.len(),
        combien,
        "une classe est dans les deux listes a la fois"
    );
    assert_eq!(
        combien, 19,
        "les deux listes doivent couvrir les dix-neuf classes, comptees {combien}"
    );
    assert!(
        !EN_CHANTIER.is_empty(),
        "aucune classe en chantier : verifier que ce n'est pas la liste qui a ete videe"
    );
}

/// Chaque classe est REFERENCEE en entier, meme celles en chantier : tous leurs
/// sorts qui frappent portent au moins leurs chiffres.
#[test]
fn aucun_sort_qui_frappe_n_est_absent() {
    let mut fautes = Vec::new();
    for (nom, breed) in FINIES.iter().chain(EN_CHANTIER) {
        let c = couverture(nom, *breed);
        if !c.missing.is_empty() {
            fautes.push(format!("{nom} : {:?}", c.missing));
        }
        if !c.unknown.is_empty() {
            fautes.push(format!("{nom}, dofusdb_id inconnus : {:?}", c.unknown));
        }
    }
    assert!(fautes.is_empty(), "{}", fautes.join("\n"));
}
