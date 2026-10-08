//! Les sorts qui frappent hors de la rotation.
//!
//! La rotation suppose une cible immobile. Ce qui ne frappe que si l'adversaire
//! bouge ou frappe (pièges du Sram, Barrière du Féca, Vendetta du Crâ, Holmgang,
//! Parade du Forgelance) compte sous un réglage que le joueur déclare (« la cible
//! déclenche N pièges par tour », « la cible vous frappe en mêlée N fois par
//! tour »), et rien sans lui. Un sort hors rotation sort du dénominateur et reste
//! nommé, jamais en silence.

use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn couverture(nom: &str, breed: u32) -> dofus_ruleset::snapshot::Coverage {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{nom}.yaml"))
        .unwrap_or_else(|e| panic!("{nom}: {e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json"))
        .unwrap_or_else(|e| panic!("breed-{breed}: {e}"));
    rs.merge_snapshot(&snap);
    snap.coverage(&rs)
}

/// Les onze pièges et la Concentration de Chakra du Sram comptent : 36 sorts
/// qui frappent, tous au dénominateur, aucun manquant.
#[test]
fn les_pieges_du_sram_comptent() {
    let c = couverture("sram", 4);
    assert!(c.outside_rotation.is_empty(), "{:?}", c.outside_rotation);
    assert_eq!(c.damaging, 36);
    for nom in ["Piège Sournois", "Calamité", "Concentration de Chakra"] {
        assert!(!c.missing.iter().any(|n| n == nom), "{nom} : {:?}", c.missing);
    }
}

/// La Barrière du Féca aussi.
#[test]
fn la_barriere_du_feca_compte() {
    let c = couverture("feca", 1);
    assert!(c.outside_rotation.is_empty(), "{:?}", c.outside_rotation);
    assert_eq!(c.damaging, 30);
}

/// Et Vendetta du Crâ, le Holmgang et la Parade du Forgelance.
#[test]
fn ce_qui_attend_un_geste_de_l_adversaire_compte_aussi() {
    for (nom, breed) in [("cra", 9), ("forgelance", 20)] {
        let c = couverture(nom, breed);
        assert!(c.outside_rotation.is_empty(), "{nom} : {:?}", c.outside_rotation);
        assert!(c.missing.is_empty(), "{nom} : {:?}", c.missing);
    }
}
