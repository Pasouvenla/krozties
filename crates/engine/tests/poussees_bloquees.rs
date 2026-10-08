//! « Vos poussées butent contre un obstacle » : chaque sort qui repousse
//! l'ennemi ajoute ses dommages de poussée, toute sa distance comptée bloquée,
//! par `(niveau / 2 + 32 + Dommages Poussée) × cases / 4`. Au niveau 200 avec
//! 50 de Dommages Poussée, deux cases valent (100 + 32 + 50) × 2 / 4 = 91.

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

fn frappes(rs: &Ruleset, deck: &[&str], ap: u8, bloquees: bool) -> Vec<(String, f64)> {
    frappes_sur(rs, deck, ap, bloquees, 1)
}

fn frappes_sur(rs: &Ruleset, deck: &[&str], ap: u8, bloquees: bool, horizon: u8) -> Vec<(String, f64)> {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 100,
                flat_damage: 0,
            }; 5],
            push_damage: 50,
            level: 200,
            ..Default::default()
        },
        base_ap: ap,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: bloquees,
        horizon,
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
    Engine::new(rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .flat_map(|t| t.casts.iter().map(|c| (c.id.clone(), c.damage.as_f64())))
        .collect()
}

/// Le Tibia repousse de deux cases : 91 de plus par lancer, rien sans la
/// déclaration.
#[test]
fn le_tibia_ajoute_ses_dommages_de_poussee() {
    let rs = regles("ouginak", 18);
    let tibia = rs.spells.iter().find(|s| s.id == "tibia").unwrap();
    assert_eq!(tibia.pushes.iter().map(|p| p.cells).collect::<Vec<_>>(), [2]);
    let sans = frappes(&rs, &["tibia"], 4, false);
    let avec = frappes(&rs, &["tibia"], 4, true);
    assert_eq!((sans.len(), avec.len()), (1, 1));
    assert!((avec[0].1 - sans[0].1 - 91.0).abs() < 0.01, "{sans:?} {avec:?}");
}

/// L'Épouvante repousse de deux cases le Pandawa Sobre, d'une le Pandawa
/// Saoul : la donnée le dit par deux effets à masque (`*E3531`, `*E498`).
#[test]
fn la_poussee_de_l_epouvante_depend_de_l_etat_du_lanceur() {
    use dofus_ruleset::{Critere, PushDef};
    let rs = regles("pandawa", 12);
    let epouvante = rs.spells.iter().find(|s| s.id == "epouvante").unwrap();
    assert_eq!(
        epouvante.pushes,
        vec![
            PushDef { cells: 2, caster: Some(Critere::Etat { etat: 3531, present: true }) },
            PushDef { cells: 1, caster: Some(Critere::Etat { etat: 498, present: true }) },
        ]
    );
    // Sobre au départ : deux cases, 91.
    let sobre = frappes(&rs, &["epouvante"], 2, true);
    assert!((sobre[0].1 - 91.0).abs() < 0.01, "{sobre:?}");
}

/// La condition se lit sur l'état du lanceur AVANT le lancer. Banc d'essai :
/// `poser` met le lanceur dans l'état 7, et `pousser` ne repousse que si le
/// lanceur le porte.
#[test]
fn une_poussee_conditionnelle_ne_part_que_sous_son_etat() {
    let rs = Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - { id: etat, scope: caster, max: 1, default: 0, monotone: none, game_states: [7] }
spells:
  - id: poser
    name: { fr: "Poser", en: "Set" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - { effect: gain, resource: etat }
  - id: pousser
    name: { fr: "Pousser", en: "Push" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    pushes:
      - { cells: 2, caster: !etat { etat: 7, present: true } }
    lines: []
    effects: []
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let seul = frappes(&rs, &["pousser"], 1, true);
    assert!(seul.iter().all(|(_, d)| *d == 0.0), "sans l'état, rien : {seul:?}");
    let deux = frappes(&rs, &["poser", "pousser"], 2, true);
    let p = deux.iter().find(|(id, _)| id == "pousser").expect("poussé");
    assert!((p.1 - 91.0).abs() < 0.01, "{deux:?}");
    assert_eq!(deux[0].0, "poser", "{deux:?}");
}

/// Une poussée qui ne vise que les alliés ne blesse aucun ennemi. L'Odyssée de
/// l'Eliotrope repousse d'une case, masque `a` : rien à compter.
#[test]
fn une_poussee_sur_les_allies_ne_compte_pas() {
    let rs = regles("eliotrope", 16);
    let odyssee = rs.spells.iter().find(|s| s.id == "odyssee").unwrap();
    assert!(odyssee.pushes.is_empty(), "{:?}", odyssee.pushes);
}

/// Tout le plan : chaque tour, ses dégâts d'ouverture et ses lancers.
fn plan(
    rs: &Ruleset,
    deck: &[&str],
    ap: u8,
    bloquees: bool,
    horizon: u8,
    declares: &[&str],
) -> Vec<(f64, Vec<(String, f64)>)> {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 100,
                flat_damage: 0,
            }; 5],
            push_damage: 50,
            level: 200,
            ..Default::default()
        },
        base_ap: ap,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: bloquees,
        horizon,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: declares.iter().map(|d| (d.to_string(), 1)).collect(),
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .map(|t| {
            (
                t.opening_damage.as_f64(),
                t.casts.iter().map(|c| (c.id.clone(), c.damage.as_f64())).collect(),
            )
        })
        .collect()
}

/// Banc : `marquer` pose un état que les dommages de poussée consomment, et
/// qu'une poussée déclarée ferait partir au début du tour suivant.
const BANC_MARQUE: &str = r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - { id: poussee_subie, scope: caster, max: 1, default: 0, monotone: increasing, declared_by_player: true }
  - id: marque
    scope: target
    max: 1
    default: 0
    monotone: increasing
    duration: { turns: 0, refresh: on_apply, on_expire: reset_to_default }
    while_present:
      - trigger: turn_start
        requires: { kind: caster_has, resource: poussee_subie }
        lines:
          - { element: fire, normal: [10, 10], critical: [10, 10] }
      - trigger: push_damage
        lines:
          - { element: fire, normal: [10, 10], critical: [10, 10] }
spells:
  - id: marquer
    name: { fr: "Marquer", en: "Mark" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - { effect: gain, resource: marque }
  - id: pousser
    name: { fr: "Pousser", en: "Push" }
    ap_cost: { base: 1 }
    casts_per_turn: 2
    crit: { base_rate: 0, can_crit: false }
    pushes:
      - { cells: 2 }
    lines: []
    effects: []
"#;

/// Une poussée bloquée consomme l'état et déclenche sa ligne : la marque
/// vaut dix, doublés par les cent de caractéristique du banc, et une seule
/// fois, la seconde poussée ne trouvant plus rien.
#[test]
fn une_poussee_bloquee_consomme_l_etat_et_le_declenche() {
    let rs = Ruleset::from_yaml(BANC_MARQUE).unwrap_or_else(|e| panic!("{e}"));
    let tour = &plan(&rs, &["marquer", "pousser"], 3, true, 1, &[])[0].1;
    let poussees: Vec<f64> = tour.iter().filter(|(id, _)| id == "pousser").map(|(_, d)| *d).collect();
    assert_eq!(tour[0].0, "marquer", "la marque d'abord : {tour:?}");
    assert_eq!(poussees.len(), 2, "{tour:?}");
    assert!((poussees[0] - 91.0 - 20.0).abs() < 0.01, "poussée et marque : {tour:?}");
    assert!((poussees[1] - 91.0).abs() < 0.01, "la marque est partie : {tour:?}");
    // Sans la déclaration, ni poussée ni marque.
    let libre = plan(&rs, &["marquer", "pousser"], 3, false, 1, &[]);
    let total: f64 = libre[0].1.iter().map(|(_, d)| d).sum();
    assert_eq!(total, 0.0, "{libre:?}");
}

/// Le coup de début de tour ne part que si le joueur déclare que la cible
/// subit une poussée à chaque tour.
#[test]
fn le_coup_de_debut_de_tour_attend_la_poussee_declaree() {
    let rs = Ruleset::from_yaml(BANC_MARQUE).unwrap_or_else(|e| panic!("{e}"));
    let muet = plan(&rs, &["marquer"], 1, false, 2, &[]);
    assert_eq!(muet[1].0, 0.0, "rien ne pousse la cible : {muet:?}");
    let declare = plan(&rs, &["marquer"], 1, false, 2, &["poussee_subie"]);
    assert!((declare[1].0 - 20.0).abs() < 0.01, "{declare:?}");
}

/// La Flibuste du Steamer, consommée par le Sonar qui la repousse de deux
/// cases contre un obstacle : dommages de poussée et coup de la Flibuste, dans
/// le tour. Sept PA et un seul Sonar par cible : la Flibuste puis le Sonar,
/// rien d'autre ne tient.
#[test]
fn le_sonar_fait_partir_la_flibuste() {
    let rs = regles("steamer", 15);
    let sonar = |bloquees| {
        let tour = plan(&rs, &["flibuste", "sonar"], 7, bloquees, 1, &[]).remove(0).1;
        assert_eq!(
            tour.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            ["flibuste", "sonar"],
            "{tour:?}"
        );
        tour[1].1
    };
    // 23-25 Air à 1 % de critique, doublés par la caractéristique : environ
    // 48 de plus que les 91 de la poussée.
    let ecart = sonar(true) - sonar(false);
    assert!(
        (ecart - 91.0 - 48.0).abs() < 2.0,
        "le Sonar doit ajouter sa poussée et la Flibuste, mesuré {ecart:.1}"
    );
}

/// Un buff de Dommages Poussée entre dans la formule : 100 de plus portent
/// deux cases de (100 + 32 + 50) × 2 / 4 = 91 à (100 + 32 + 150) × 2 / 4 = 141.
/// Sa valeur critique ne compte qu'au seuil du critique : ici 0 en normal et
/// 100 en critique, donc 91 sans critique au build et 141 à 78.
#[test]
fn un_buff_de_dommages_poussee_entre_dans_la_formule() {
    let banc = |modificateur: &str| {
        Ruleset::from_yaml(&format!(
            r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: {{ fr: "Test", en: "Test" }}
resources:
  - id: elan
    scope: caster
    max: 1
    default: 0
    monotone: increasing
    modifies_damage:
      - {modificateur}
spells:
  - id: poser
    name: {{ fr: "Poser", en: "Set" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines: []
    effects:
      - {{ effect: gain, resource: elan }}
  - id: pousser
    name: {{ fr: "Pousser", en: "Push" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0 }}
    pushes:
      - {{ cells: 2 }}
    lines: []
    effects: []
"#
        ))
        .unwrap_or_else(|e| panic!("{e}"))
    };
    let poussee = |rs: &Ruleset, critique: i32| {
        let build = Build {
            name: "banc".into(),
            profile: DamageProfile {
                power: 0,
                flat_crit_damage: 0,
                elements: [ElementStats {
                    characteristic: 100,
                    flat_damage: 0,
                }; 5],
                push_damage: 50,
                level: 200,
                ..Default::default()
            },
            base_ap: 2,
            base_mp: 3,
            crit_bonus_percent: critique,
            modifiers: vec![],
            deck: vec!["poser".into(), "pousser".into()],
        };
        let sc = Scenario {
            poussees_bloquees: true,
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
        let sol = Engine::new(rs, build, sc).unwrap_or_else(|e| panic!("{e}")).solve();
        sol.turns[0]
            .casts
            .iter()
            .find(|c| c.id == "pousser")
            .map(|c| c.damage.as_f64())
            .unwrap_or_else(|| panic!("{sol}"))
    };
    let simple = banc("{ kind: push_damage, amount: 100 }");
    assert!((poussee(&simple, 0) - 141.0).abs() < 0.01, "{}", poussee(&simple, 0));
    let critique = banc("{ kind: push_damage, amount: 0, critical_amount: 100 }");
    assert!((poussee(&critique, 0) - 91.0).abs() < 0.01, "{}", poussee(&critique, 0));
    assert!((poussee(&critique, 78) - 141.0).abs() < 0.01, "{}", poussee(&critique, 78));
}
