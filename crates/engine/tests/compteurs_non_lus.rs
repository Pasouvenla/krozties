//! Un compteur que rien ne lit ne doit pas démultiplier les états.
//!
//! Chaque sort élémentaire du Huppermage compte sa rune pour la Surcharge
//! Runique, inutilement sans la Surcharge au deck : le moteur retire les
//! écritures des compteurs que rien ne lit, sous le drapeau `dominance` comme
//! ses autres réductions.
//!
//! Deux garanties :
//!
//! 1. la réduction ne change aucun total, sur aucune classe ;
//! 2. elle retire ce qu'elle doit : le Huppermage sans Surcharge parcourt
//!    exactement les états d'un fichier dont les comptes de runes auraient été
//!    effacés à la main.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

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

fn racine() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../..")
}

fn fusionner(mut rs: Ruleset, breed: u32) -> Ruleset {
    let snap = Snapshot::load(format!("{}/data/snapshots/breed-{breed}.json", racine()))
        .unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn texte(classe: &str) -> String {
    std::fs::read_to_string(format!("{}/data/rulesets/{classe}.yaml", racine()))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn resoudre(rs: &Ruleset, deck: &[String], horizon: u8, reduire: bool) -> Option<Solution> {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 100,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 400,
                flat_damage: 20,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 20,
        modifiers: vec![],
        deck: deck.to_vec(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: vec![],
        horizon,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: reduire,
        prune_spells: reduire,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(rs, build, sc).ok().map(|e| e.solve())
}

/// Sur chaque classe, les cinq premiers sorts donnent le même total avec et
/// sans réduction.
#[test]
fn la_reduction_ne_change_aucun_total() {
    let mut verifiees = 0;
    for (classe, breed) in CLASSES {
        let rs = fusionner(Ruleset::from_yaml(&texte(classe)).unwrap(), *breed);
        let deck: Vec<String> = rs.spells.iter().take(5).map(|s| s.id.clone()).collect();
        let (Some(reduit), Some(complet)) = (
            resoudre(&rs, &deck, 2, true),
            resoudre(&rs, &deck, 2, false),
        ) else {
            continue;
        };
        assert!(
            (reduit.total.as_f64() - complet.total.as_f64()).abs() < 0.01,
            "{classe} : {:.2} réduit contre {:.2} complet",
            reduit.total.as_f64(),
            complet.total.as_f64()
        );
        verifiees += 1;
    }
    assert!(verifiees >= 15, "{verifiees} classes seulement ont pu se résoudre");
}

/// Sans Surcharge, les comptes de runes ne coûtent plus rien : mêmes états et
/// même total que si le fichier ne les avait jamais écrits.
#[test]
fn sans_surcharge_les_comptes_de_runes_disparaissent() {
    let original = texte("huppermage");
    let efface = original
        .replace(
            "      # Et compte parmi les runes vivantes, pour la Surcharge Runique.\n",
            "",
        )
        .replace("      - effect: gain\n        resource: runes_ce_tour\n", "");
    assert_ne!(original, efface, "le fichier a changé : ce test n'efface plus rien");
    let deck: Vec<String> = [
        "lance_flamme",
        "stalagmite",
        "onde_sismique",
        "ether",
        "orage",
        "glacier",
        "rafale",
        "trait_ardent",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    let avec = resoudre(
        &fusionner(Ruleset::from_yaml(&original).unwrap(), 17),
        &deck,
        4,
        true,
    )
    .unwrap();
    let sans = resoudre(
        &fusionner(Ruleset::from_yaml(&efface).unwrap(), 17),
        &deck,
        4,
        true,
    )
    .unwrap();
    assert_eq!(avec.inter_turn_states, sans.inter_turn_states);
    assert!((avec.total.as_f64() - sans.total.as_f64()).abs() < 0.01);
}

/// Un compteur qu'un autre remplit en glissant compte quand même dans les
/// tables de dégâts.
///
/// `b` ne reçoit rien d'un sort : c'est `a` qui lui passe sa valeur à la fin du
/// tour, et c'est `b` qui porte les cent points de Force. Le tenir pour
/// inatteignable ferait lire à jamais son entrée « zéro » et le bonus
/// disparaîtrait sans un mot.
#[test]
fn un_compteur_nourri_par_glissement_reste_dans_les_tables() {
    let rs = Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - { id: a, scope: caster, max: 1, default: 0, monotone: increasing, shifts_into: b }
  - id: b
    scope: caster
    max: 1
    default: 0
    monotone: increasing
    modifies_damage:
      - kind: characteristic
        amount: 100
        element: earth
spells:
  - id: pose
    name: { fr: "Pose", en: "Lay" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - { effect: gain, resource: a }
  - id: frappe
    name: { fr: "Frappe", en: "Strike" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      - { element: earth, critical: [10, 10], normal: [10, 10] }
    effects:
      - effect: damage
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let deck: Vec<String> = ["pose", "frappe"].iter().map(|s| (*s).to_string()).collect();
    let sol = resoudre(&rs, &deck, 2, true).unwrap();
    // Caractéristique 400 au banc : un jet de 10 vaut 50 nu, 60 avec les cent
    // points que `b` porte. Tour 1 : pose et frappe nue. Tour 2 : frappe
    // bonifiée, et poser ne sert plus à rien.
    let frappes: Vec<f64> = sol
        .turns
        .iter()
        .flat_map(|t| &t.casts)
        .filter(|c| c.id == "frappe")
        .map(|c| c.damage.as_f64())
        .collect();
    assert_eq!(frappes.len(), 2, "{sol}");
    assert!(
        frappes[1] > frappes[0],
        "le bonus doit arriver au tour 2 : {frappes:?}"
    );
}
