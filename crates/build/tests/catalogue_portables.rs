//! Un objet portable entre au catalogue même sans statistique de dégâts, sinon un
//! build qui le porte s'importe avec un objet « absent du catalogue ». Le
//! représentant des objets portables sans statistique est un bouclier trophée,
//! qui ne porte qu'un titre.

use dofus_build::Catalogue;

fn catalogue() -> Catalogue {
    Catalogue::load(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/snapshots/items.json"
    )))
    .expect("catalogue")
}

#[test]
fn un_objet_portable_sans_statistique_est_au_catalogue() {
    let c = catalogue();
    let trophee = c
        .item(10159)
        .expect("Bouclier trophée du Moon absent du catalogue : le filtre a rebascule sur les seules statistiques");
    assert_eq!(trophee.name, "Bouclier trophée du Moon");
    // Le test ne vaut que si l'objet est bien depourvu de statistique connue :
    // sinon il passerait pour la mauvaise raison.
    assert!(
        trophee.stats.is_empty(),
        "le bouclier porte desormais des statistiques ({:?}) : ce test ne prouve plus rien sur \
         les objets portables SANS statistique, il faut lui trouver un autre representant",
        trophee.stats
    );
    assert!(
        !trophee.unmapped_effect_ids.is_empty(),
        "sans effet non mappe, le resolveur n'a rien a dire au joueur"
    );
}

/// Espryt, lue en entier : 20 de Retrait PA et PM, et le malus de 20 en Esquive
/// PA et PM qui va avec.
#[test]
fn espryt_porte_son_retrait_et_son_malus_d_esquive() {
    let c = catalogue();
    let espryt = c.item(22014).expect("Espryt au catalogue");
    for (nom, valeur) in [("ap_reduction", 20), ("mp_reduction", 20), ("ap_dodge", -20), ("mp_dodge", -20)] {
        assert_eq!(espryt.stats.get(nom).map(|r| r[1]), Some(valeur), "{nom} : {:?}", espryt.stats);
    }
    assert!(espryt.unmapped_effect_ids.is_empty(), "{:?}", espryt.unmapped_effect_ids);
}

#[test]
fn le_build_ougi_feu_se_resout_en_entier() {
    let c = catalogue();
    let porte = [
        22192, 17574, 14169, 22368, 22189, 22191, 14161, 14162, 22190, 22014, 29136, 7754, 7043,
        694, 739, 29431,
    ];
    let absents: Vec<u32> = porte.into_iter().filter(|i| c.item(*i).is_none()).collect();
    assert!(
        absents.is_empty(),
        "objets absents du catalogue : {absents:?}"
    );
}
