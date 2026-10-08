//! Des correctifs relevés en jeu sur le serveur bêta, un par classe touchée.
//! La règle des durées et les deux formes de zone ont leurs propres fichiers
//! (`regle_des_durees.rs`, `crates/grid/tests`) ; ici, les mécaniques de sort.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn regles(classe: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn sort<'a>(rs: &'a Ruleset, id: &str) -> &'a dofus_ruleset::SpellDef {
    rs.spells
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("{id} absent"))
}

/// La Tromperie compte la moyenne de ses seize issues, au coût de base.
///
/// À 100 dans chaque élément, une fourchette double : 19-20 vaut 39 en
/// moyenne, 28-30 vaut 58, 37-40 vaut 77, 46-50 vaut 96, soit 67,5 ; les
/// critiques 60, 80, 100 et 120, soit 90. À 5 % de critique, 68,625 par
/// lancer. La fourchette affichée va du plus bas tirage au plus haut, sans
/// additionner les seize.
#[test]
fn la_tromperie_compte_la_moyenne_de_ses_seize_issues() {
    let rs = regles("ecaflip", 6);
    assert!(sort(&rs, "tromperie").outside_rotation.is_none());
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            elements: [ElementStats { characteristic: 100, flat_damage: 0 }; 5],
            ..Default::default()
        },
        base_ap: 2,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["tromperie".into()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 1,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let moteur = Engine::new(&rs, build, sc).unwrap_or_else(|e| panic!("{e}"));
    let sol = moteur.replay(&[vec!["tromperie".into()]]).unwrap_or_else(|e| panic!("{e}"));
    let lancer = sol.turns[0].casts[0].damage.as_f64();
    assert!((lancer - 68.625).abs() < 0.01, "{lancer}");
    let table = moteur.damage_table("tromperie");
    assert_eq!((table[0].normal, table[0].critical), ((38, 100), Some((60, 120))));
    assert_eq!(table[0].by_element.len(), 4);
    for e in &table[0].by_element {
        assert_eq!((e.normal, e.critical), ((38, 100), Some((60, 120))), "{:?}", e.element);
    }
}

/// Le cumul de Cadence et Shrapnel s'arrête à 2 et ne vaut que pour le retrait
/// de PA/PM : leur relance depuis les Bombes ne frappe plus. Le Plombage non
/// plus : lancé sur une Bombe, il redéclenche le mur, un mode que la rotation
/// lui greffe.
#[test]
fn la_cadence_et_le_shrapnel_ne_frappent_plus_depuis_les_bombes() {
    let rs = regles("roublard", 13);
    for id in ["cadence", "shrapnel", "plombage"] {
        assert!(
            sort(&rs, id).lines.iter().all(|l| l.repeats_per.is_empty()),
            "{id} frappe encore une fois par bombe"
        );
    }
}

/// « Ça cumule MAIS un seul lancé par cible, donc à relancer tous les tours pour
/// que la cible cumule bien 2 fois le poison. » Relancée au tour 2, la Vertèbre
/// fait tomber deux poisons au début du tour 3.
#[test]
fn deux_vertebres_font_deux_poisons() {
    let rs = regles("ouginak", 18);
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            elements: [ElementStats {
                characteristic: 100,
                flat_damage: 0,
            }; 5],
            ..Default::default()
        },
        base_ap: 4,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["vertebre".into()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 3,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let sol = Engine::new(&rs, build, sc).unwrap_or_else(|e| panic!("{e}")).solve();
    let ouvertures: Vec<f64> = sol.turns.iter().map(|t| t.opening_damage.as_f64()).collect();
    assert_eq!(ouvertures[0], 0.0, "{ouvertures:?}");
    assert!(ouvertures[1] > 0.0, "un poison au tour 2 : {ouvertures:?}");
    assert!(
        (ouvertures[2] / ouvertures[1] - 2.0).abs() < 0.02,
        "deux poisons au tour 3 : {ouvertures:?}"
    );
}

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

/// Un malus de dommages subis que la donnée pose avec une durée 0 ne dure que
/// le tour du lancer : 0 y veut dire « jusqu'au début du prochain tour du
/// lanceur » (le Bond et le Massacre du Iop, le Gibier et l'Acharnement de
/// l'Ouginak). Le contrôle porte sur les dix-neuf classes, pour qu'aucun état
/// ne le lise « permanent ».
#[test]
fn un_malus_a_duree_zero_ne_dure_que_le_tour_du_lancer() {
    use dofus_ruleset::{DamageModifier, Effect, Scope};
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut vus = 0;
    let mut fautes = Vec::new();
    for (classe, breed) in CLASSES {
        let rs = regles(classe, *breed);
        let brut: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(format!("{root}/data/snapshots/breed-{breed}.json"))
                .unwrap(),
        )
        .unwrap();
        let duree_zero = |dofusdb_id: u32| {
            brut["spells"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|s| s["id"].as_u64() == Some(u64::from(dofusdb_id)))
                .filter_map(|s| s["levels"].as_array().and_then(|l| l.last()))
                .flat_map(|n| n["other_effects"].as_array().cloned().unwrap_or_default())
                .any(|e| e["id"].as_u64() == Some(1163) && e["duration"].as_i64() == Some(0))
        };
        for r in rs.resources.iter().filter(|r| {
            r.scope == Scope::Target
                && r
                    .modifies_damage
                    .iter()
                    .any(|m| matches!(m, DamageModifier::FinalMultiplier { .. }))
        }) {
            let pose_a_zero = rs
                .spells
                .iter()
                .filter(|s| {
                    s.effects
                        .iter()
                        .any(|e| matches!(e, Effect::Gain { resource, .. } if *resource == r.id))
                })
                .filter_map(|s| s.dofusdb_id)
                .any(duree_zero);
            if !pose_a_zero {
                continue;
            }
            vus += 1;
            if r.duration.as_ref().map(|d| d.turns) != Some(0) {
                fautes.push(format!("{classe}/{}", r.id));
            }
        }
    }
    assert!(vus >= 4, "seulement {vus} malus contrôlés : le test ne contrôle plus rien");
    assert!(
        fautes.is_empty(),
        "malus posés pour le tour du lancer et qui durent plus longtemps : {fautes:?}"
    );
}
