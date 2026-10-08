//! Des cartes tirées au hasard (`random_among`), sur un banc synthétique.
//!
//! Quatre cartes, `a` à `d`, une chacune ; `hasard` compte les cartes tirées
//! au hasard que la rotation ne voit pas. Le coup `joue` frappe 100 si la Main
//! tient `a` ET `b`. Chaque tirage est uniforme sur les quatre cartes, et une
//! carte déjà en main ne s'ajoute pas (la donnée de l'Ecaflip : chaque carte
//! exige l'absence de son état). Les dégâts comptent la moyenne sur tous les
//! tirages possibles.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

fn banc() -> Ruleset {
    let sort = |id: &str, effets: &str| {
        format!(
            r#"
  - id: {id}
    name: {{ fr: "{id}", en: "{id}" }}
    ap_cost: {{ base: 0 }}
    casts_per_turn: 4
    crit: {{ base_rate: 0, can_crit: false }}
    lines: []
    effects:{effets}"#
        )
    };
    let gain = |r: &str| format!("\n      - effect: gain\n        resource: {r}");
    let remise = |r: &str| format!("\n      - effect: reset\n        resource: {r}");
    Ruleset::from_yaml(&format!(
        r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: {{ fr: "Test", en: "Test" }}
resources:
  - {{ id: a, scope: caster, max: 1, default: 0 }}
  - {{ id: b, scope: caster, max: 1, default: 0 }}
  - {{ id: c, scope: caster, max: 1, default: 0 }}
  - {{ id: d, scope: caster, max: 1, default: 0 }}
  - {{ id: hasard, scope: caster, max: 2, default: 0, random_among: [a, b, c, d] }}
spells:{}{}{}{}{}
  - id: joue
    name: {{ fr: "joue", en: "joue" }}
    ap_cost: {{ base: 0 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines:
      - element: water
        critical: [100, 100]
        normal: [100, 100]
        active_at:
          all_of:
            - sum_of: [a, b]
              exactly: 2
    effects:
      - effect: damage
"#,
        sort("pose_a", &gain("a")),
        sort("pose_b", &gain("b")),
        sort("pioche", &gain("hasard")),
        sort(
            "etoile",
            "\n      - effect: gain\n        resource: hasard\n        requires: { kind: none_of, none_of: [a, b, c, d, hasard] }"
        ),
        sort(
            "defausse",
            &[remise("a"), remise("b"), remise("c"), remise("d"), remise("hasard")].concat()
        ),
    ))
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Ce que frappe `joue`, lancé à la fin de ces lancers.
fn joue_apres(lancers: &[&str]) -> f64 {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats { characteristic: 0, flat_damage: 0 }; 5],
            ..Default::default()
        },
        base_ap: 6,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: ["pose_a", "pose_b", "pioche", "etoile", "defausse", "joue"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon: 1,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let tour: Vec<String> = lancers.iter().chain(&["joue"]).map(|s| s.to_string()).collect();
    let s = Engine::new(&banc(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .replay(&[tour])
        .unwrap_or_else(|e| panic!("{lancers:?} : {e}"));
    s.turns[0].casts.last().unwrap().damage.as_f64()
}

/// Sans hasard, rien ne change.
#[test]
fn les_cartes_connues_comptent_comme_avant() {
    assert_eq!(joue_apres(&["pose_a"]), 0.0);
    assert_eq!(joue_apres(&["pose_a", "pose_b"]), 100.0);
}

/// Une carte tirée avec `a` en main : `b` une fois sur quatre, 25. Deux cartes
/// tirées sur une Main vide : `a` puis `b` ou `b` puis `a`, deux tirages sur
/// seize, 12,5.
#[test]
fn une_carte_tiree_compte_sa_chance() {
    assert_eq!(joue_apres(&["pose_a", "pioche"]), 25.0);
    assert_eq!(joue_apres(&["pioche", "pioche"]), 12.5);
}

/// Une carte tirée déjà en main ne s'ajoute pas : `a` et `b` tenus, la
/// combinaison tient quel que soit le tirage.
#[test]
fn une_carte_deja_en_main_ne_s_ajoute_pas() {
    assert_eq!(joue_apres(&["pose_a", "pose_b", "pioche"]), 100.0);
}

/// `etoile` ne tire que sur une Main vide, cartes tirées comprises.
#[test]
fn aucune_carte_en_main() {
    assert_eq!(joue_apres(&["etoile", "pose_a"]), 25.0);
    assert_eq!(joue_apres(&["pose_a", "etoile"]), 0.0);
    assert_eq!(joue_apres(&["pioche", "etoile", "pose_a"]), 25.0);
}

/// La défausse vide aussi les cartes tirées.
#[test]
fn la_defausse_vide_les_cartes_tirees() {
    assert_eq!(joue_apres(&["pioche", "defausse", "pose_a"]), 0.0);
}

/// Sur la vraie Main de l'Ecaflip : trois Dames posées puis une Bonne Pioche,
/// la Rekop vaut la moyenne des seize Rekop où la quatrième carte est posée à
/// la main. Les cartes se posent par des sorts ajoutés au fichier pour le
/// test, à 0 PA.
#[test]
fn la_rekop_compte_la_moyenne_de_la_bonne_pioche() {
    use dofus_ruleset::snapshot::Snapshot;
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let cartes: Vec<String> = ["valet", "dame", "roi", "as"]
        .iter()
        .flat_map(|v| ["pique", "trefle", "coeur", "carreau"].map(|c| format!("carte_{v}_{c}")))
        .collect();
    let mut texte = std::fs::read_to_string(format!("{root}/data/rulesets/ecaflip.yaml")).unwrap();
    for c in &cartes {
        texte.push_str(&format!(
            r#"
  - id: pose_{c}
    name: {{ fr: "pose {c}", en: "pose {c}" }}
    ap_cost: {{ base: 0 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines: []
    effects:
      - effect: gain
        resource: {c}
"#
        ));
    }
    let mut rs = Ruleset::from_yaml(&texte).unwrap_or_else(|e| panic!("{e}"));
    rs.merge_snapshot(&Snapshot::load(format!("{root}/data/snapshots/breed-6.json")).unwrap());
    let rs = rs.avec_mecaniques(&["main_de_poker".to_string()]);
    let mut deck: Vec<String> = cartes.iter().map(|c| format!("pose_{c}")).collect();
    deck.extend(["bonne_pioche".to_string(), "redistribution".to_string(), "rekop".to_string()]);
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            elements: [ElementStats { characteristic: 100, flat_damage: 0 }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck,
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon: 1,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let moteur = Engine::new(&rs, build, sc).unwrap_or_else(|e| panic!("{e}"));
    let dames = ["pose_carte_dame_trefle", "pose_carte_dame_carreau", "pose_carte_dame_pique"];
    let rekop = |fin: &[String]| -> f64 {
        let tour: Vec<String> = dames.iter().map(|s| s.to_string()).chain(fin.iter().cloned()).collect();
        let s = moteur.replay(&[tour]).unwrap_or_else(|e| panic!("{fin:?} : {e}"));
        s.turns[0].casts.iter().filter(|c| c.id == "rekop").map(|c| c.damage.as_f64()).sum()
    };
    // Une Dame déjà en main tirée de nouveau ne change rien.
    let reference: f64 = cartes
        .iter()
        .map(|c| {
            let pose = format!("pose_{c}");
            if dames.contains(&pose.as_str()) {
                rekop(&["rekop".to_string()])
            } else {
                rekop(&[pose, "rekop".to_string()])
            }
        })
        .sum::<f64>()
        / 16.0;
    let tiree = rekop(&["bonne_pioche".to_string(), "rekop".to_string()]);
    assert!(reference > 0.0);
    assert!((tiree - reference).abs() < 0.01, "{tiree} contre {reference}");
    // Une Main entièrement tirée : la Main Gagnante passe pour posée tant que
    // des cartes tirées attendent, et la Rekop part pour sa moyenne.
    let s = moteur
        .replay(&[vec!["redistribution".to_string(), "rekop".to_string()]])
        .unwrap_or_else(|e| panic!("{e}"));
    let main_tiree = s.turns[0].casts[1].damage.as_f64();
    assert!(main_tiree > 0.0 && main_tiree < reference, "{main_tiree}");
}
