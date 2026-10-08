//! Toute forgemagie qui change un dégât est comptée ; les autres ne se signalent
//! pas.

use dofus_build::*;
use dofus_damage::Element;

fn catalogue() -> Catalogue {
    Catalogue::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/snapshots/items.json"
    ))
    .unwrap()
}

fn resoudre(fm: &str) -> Resolved {
    let input: BuildInput = serde_json::from_str(&format!(
        r#"{{ "v": 1, "src": "dofusbook", "class": 8, "level": 200, "items": [],
              "carac": {{}}, "fm": {{ "a1": {fm} }} }}"#
    ))
    .expect("le format de l'import doit être lu");
    resolve(&input, &catalogue())
}

#[test]
fn les_dommages_d_un_element_et_la_vitalite_sont_comptes() {
    let nu = resoudre("{}");
    let r = resoudre(r#"{"dff": 12, "dtf": 7, "vi": 100}"#);
    let fixe = |r: &Resolved, e: Element| r.profile.elements[e.index()].flat_damage;
    assert_eq!(fixe(&r, Element::Fire) - fixe(&nu, Element::Fire), 12);
    assert_eq!(fixe(&r, Element::Earth) - fixe(&nu, Element::Earth), 7);
    assert_eq!(fixe(&r, Element::Water), fixe(&nu, Element::Water));
    assert_eq!(r.profile.life - nu.profile.life, 100);
    assert!(r.assumptions.iter().all(|a| !a.contains("forgemagie")), "{:#?}", r.assumptions);
}

#[test]
fn ce_qui_ne_change_aucun_degat_ne_se_dit_pas() {
    let r = resoudre(r#"{"ii": 50, "rpm": 7, "po": 1, "re": 10, "epa": 5}"#);
    assert!(r.assumptions.iter().all(|a| !a.contains("forgemagie")), "{:#?}", r.assumptions);
}

#[test]
fn un_code_inconnu_se_signale() {
    let r = resoudre(r#"{"zz": 3}"#);
    assert!(r.assumptions.iter().any(|a| a.contains("inconnue (zz)")), "{:#?}", r.assumptions);
}

/// La Portée forgemagée entre au total : les zones de sorts dessinent avec elle
/// les cases d'où un sort part.
#[test]
fn la_portee_forgemagee_est_comptee() {
    let nu = resoudre("{}");
    let r = resoudre(r#"{"po": 1}"#);
    let portee = |r: &Resolved| r.totals.get("range").copied().unwrap_or(0);
    assert_eq!(portee(&r) - portee(&nu), 1);
}
