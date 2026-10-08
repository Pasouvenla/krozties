//! Une ligne de dégâts dit son élément, et l'omission se voit : un défaut ferait
//! d'une ligne sans `element:` une ligne de Feu, en silence. Trois façons de le
//! dire, exactement une à la fois : le nommer, `best_element`, ou
//! `worst_element`.

use dofus_ruleset::Ruleset;

/// Le meme sort a quatre variantes, ecrites en clair : une substitution de
/// chaine sur un fragment de YAML se trompe d'indentation sans rien dire, ce
/// qui a fait passer deux de ces tests pour la mauvaise raison.
fn sort(ligne: &str) -> String {
    format!(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: {{ fr: "Test", en: "Test" }}
resources: []
spells:
  - id: coup
    name: {{ fr: "Coup", en: "Hit" }}
    ap_cost: {{ base: 3 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines:
      - critical: [10, 12]
        normal: [8, 10]
{ligne}    effects:
      - effect: damage
"#
    )
}

const NOMME: &str = "        element: water\n";
const AUCUN: &str = "";
const MEILLEUR: &str = "        best_element: true\n";
const LES_DEUX: &str = "        element: water\n        best_element: true\n";

#[test]
fn une_ligne_avec_son_element_se_charge() {
    Ruleset::from_yaml(&sort(NOMME)).expect("le cas nominal doit passer");
}

#[test]
fn une_ligne_sans_element_est_refusee() {
    let e = Ruleset::from_yaml(&sort(AUCUN))
        .expect_err("une ligne sans element doit etre refusee, pas lue en Feu");
    let texte = e.to_string();
    assert!(
        texte.contains("coup.lines[0]") && texte.contains("aucun élément"),
        "le refus doit nommer la ligne fautive : {texte}"
    );
}

#[test]
fn le_meilleur_element_dispense_de_le_nommer() {
    Ruleset::from_yaml(&sort(MEILLEUR)).expect("best_element doit suffire");
}

#[test]
fn declarer_l_element_deux_fois_est_refuse() {
    let e = Ruleset::from_yaml(&sort(LES_DEUX))
        .expect_err("un element nomme ET `best_element` est une contradiction");
    assert!(
        e.to_string().contains("deux fois"),
        "le refus doit dire laquelle : {e}"
    );
}

/// Et la regle vaut aussi pour les lignes qui ne sont pas celles d'un sort :
/// une charge differee et l'etat d'une ressource en portent aussi.
#[test]
fn la_regle_couvre_les_charges_et_les_etats() {
    let yaml = r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - id: marque
    scope: target
    max: 1
    default: 0
    monotone: increasing
    while_present:
      - trigger: turn_start
        lines:
          - critical: [10, 12]
            normal: [8, 10]
spells: []
"#;
    let e = Ruleset::from_yaml(yaml).expect_err("l'etat porte une ligne sans element");
    assert!(
        e.to_string().contains("marque.while_present[0].lines[0]"),
        "le refus doit nommer le chemin exact : {e}"
    );
}
