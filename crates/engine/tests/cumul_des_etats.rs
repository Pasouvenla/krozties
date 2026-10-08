//! Un état qui se cumule deux fois vaut deux fois, et le fichier doit le dire.
//!
//! La donnée porte `maxStack` sur chaque niveau de sort : le nombre de fois que
//! l'état tient. `modifiers` s'applique `res[i]` fois ; c'est le plafond qui
//! doit suivre `maxStack`, sans quoi la moitié du bonus serait inatteignable.

use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn regles(classe: &str, breed: u8) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

/// Le plafond écrit est celui du jeu, pour les cinq états concernés.
#[test]
fn les_cinq_etats_portent_le_plafond_du_jeu() {
    let attendu: &[(&str, u8, &[&str])] = &[
        (
            "sacrieur",
            11,
            &[
                "furie",
                "nervosite_critique",
                "douleur_cuisante",
                "vulnerabilite_decimation",
            ],
        ),
        ("sram", 4, &["truanderie"]),
    ];
    for (classe, breed, etats) in attendu {
        let rs = regles(classe, *breed);
        for id in *etats {
            let r = rs
                .resources
                .iter()
                .find(|r| &r.id == id)
                .unwrap_or_else(|| panic!("{classe} : l'état {id} a disparu du fichier"));
            assert_eq!(
                r.max, 2,
                "{classe}/{id} : DofusDB donne `maxStack` à 2, le fichier doit suivre"
            );
            assert!(
                !r.modifies_damage.is_empty(),
                "{classe}/{id} : sans modificateur, le plafond ne changerait rien"
            );
        }
    }
}

/// Le plafond écrit vient de l'instantané, pas d'une saisie à la main : le
/// test précédent passerait si quelqu'un montait les plafonds à 2 par erreur.
#[test]
fn le_plafond_se_retrouve_dans_l_instantane() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let attendu: &[(&str, u8, u32)] = &[
        ("sacrieur", 11, 12723),
        ("sacrieur", 11, 12727),
        ("sacrieur", 11, 12730),
        ("sacrieur", 11, 12731),
        ("sram", 4, 12902),
    ];
    for (classe, breed, dofusdb_id) in attendu {
        let brut =
            std::fs::read_to_string(format!("{root}/data/snapshots/breed-{breed}.json"))
                .unwrap();
        let v: serde_json::Value = serde_json::from_str(&brut).unwrap();
        let sort = v["spells"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"].as_u64() == Some(u64::from(*dofusdb_id)))
            .unwrap_or_else(|| {
                panic!("{classe} : le sort {dofusdb_id} n'est pas dans l'instantané")
            });
        let niveaux = sort["levels"].as_array().unwrap();
        let dernier = niveaux.last().unwrap();
        assert_eq!(
            dernier["max_stack"].as_u64(),
            Some(2),
            "{classe}/{dofusdb_id} : l'instantané doit porter `max_stack` à 2"
        );
    }
}

/// La Vertèbre de l'Ouginak se cumule deux fois : deux poisons qui tombent
/// chacun. Son poison se répète une fois par cran.
#[test]
fn la_vertebre_cumule_deux_poisons() {
    let rs = regles("ouginak", 18);
    let r = rs
        .resources
        .iter()
        .find(|r| r.id == "vertebre")
        .expect("la Vertèbre est au fichier");
    assert_eq!(r.max, 2, "deux cumuls, comme `maxStack`");
    assert!(
        r.while_present
            .iter()
            .flat_map(|e| &e.lines)
            .all(|l| l.repeats_per.iter().any(|x| x == "vertebre")),
        "le poison se répète une fois par cran"
    );
}
