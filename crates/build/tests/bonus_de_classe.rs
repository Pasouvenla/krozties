//! Un bonus de classe que le solveur ne sait pas lancer est compté ici. Compter
//! un bonus que le solveur produit aussi le compterait deux fois, mais la règle
//! ne vaut que si le solveur en est capable, ce que seul le ruleset sait :
//! `resolve_with_class_spells` reçoit les couples (sort, statistique) modélisés.

use dofus_build::*;
use std::collections::BTreeSet;

/// Le même équipement, avec un bonus de classe de 150 Puissance.
const PAYLOAD: &str = r#"{
  "v": 1, "src": "dofusbook", "id": 1, "class": 20, "level": 200,
  "items": [34330,31761,32234,24035,31762,17575,34332,32236,34331,13673,0,8698,7043,6980,739,7754,7115],
  "carac": {"base_ch":398},
  "boosts": [{"boostName":"Prélude au Fer","effectName":"pu","effectValue":150,"count":1,
              "classId":20,"effectId":"23841-1-3-1560-0","active":true}]
}"#;

fn catalogue() -> Catalogue {
    Catalogue::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/snapshots/items.json"
    ))
    .unwrap()
}

fn entree() -> BuildInput {
    serde_json::from_str(PAYLOAD).expect("le format doit être lu")
}

/// La Puissance que l'équipement seul apporte, bonus de classe écarté. Les
/// assertions portent sur l'ÉCART, l'équipement en donnant déjà trente.
fn puissance_nue() -> i32 {
    resolve_with_class_spells(&entree(), &catalogue(), None)
        .profile
        .power
}

/// L'identifiant du sort se lit dans `effectId`, premier segment.
#[test]
fn le_bonus_nomme_le_sort_dont_il_vient() {
    let input = entree();
    assert_eq!(
        input.boosts[0].spell_id(),
        Some(23841),
        "23841 est le Prélude au Fer, et c'est ce qui permet d'interroger le ruleset"
    );
}

/// Sort NON modélisé : le bonus est compté, sinon il n'est compté nulle part.
#[test]
fn un_bonus_de_classe_non_modelise_est_compte() {
    let vide = BTreeSet::new();
    let r = resolve_with_class_spells(&entree(), &catalogue(), Some(&vide));
    assert_eq!(
        r.profile.power,
        puissance_nue() + 150,
        "les 150 Puissance doivent s'ajouter à celle de l'équipement"
    );
    assert!(
        r.assumptions.iter().any(|a| a.contains("compté en permanence")),
        "et la résolution doit dire qu'elle les a comptés : {:#?}",
        r.assumptions
    );
}

/// Sort modélisé : le bonus est écarté, le solveur le produira lui-même.
#[test]
fn un_bonus_de_classe_modelise_reste_au_solveur() {
    let mut connus = BTreeSet::new();
    connus.insert((23841u32, "pu".to_string()));
    let r = resolve_with_class_spells(&entree(), &catalogue(), Some(&connus));
    assert_eq!(
        r.profile.power,
        puissance_nue(),
        "compter ici un bonus que le solveur lance aussi le compterait deux fois"
    );
    assert!(
        r.assumptions.iter().any(|a| a.contains("non compté ici")),
        "{:#?}",
        r.assumptions
    );
}

/// La statistique compte autant que le sort : un sort dont le multiplicateur est
/// modélisé mais pas la Puissance ne couvre pas un bonus de Puissance.
#[test]
fn la_statistique_doit_correspondre_elle_aussi() {
    let mut autre = BTreeSet::new();
    autre.insert((23841u32, "deg".to_string()));
    let r = resolve_with_class_spells(&entree(), &catalogue(), Some(&autre));
    assert_eq!(
        r.profile.power,
        puissance_nue() + 150,
        "le solveur modélise un multiplicateur, pas cette Puissance : elle se compte ici"
    );
}

/// Sans ruleset, l'ancien comportement : tous les bonus de classe sont écartés.
#[test]
fn sans_ruleset_rien_ne_change() {
    let r = resolve_with_class_spells(&entree(), &catalogue(), None);
    assert_eq!(r.profile.power, puissance_nue());
    assert!(
        r.assumptions.iter().any(|a| a.contains("non compté ici")),
        "{:#?}",
        r.assumptions
    );
}
