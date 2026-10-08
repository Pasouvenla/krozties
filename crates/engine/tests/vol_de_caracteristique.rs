//! Un vol de caractéristique profite au lanceur, et à un élément seulement.
//!
//! Quatre sorts du Sram et le Drain Élémentaire du Huppermage volent cent ou
//! deux cents points de caractéristique pendant trois tours. La donnée n'écrit
//! que ce que l'adversaire perd ; le lanceur gagne autant, pour la durée
//! inscrite.
//!
//! L'élément est la moitié de la réponse : l'Intelligence ne porte que le Feu,
//! et un vol qui lèverait les cinq lignes surestimerait le sort sur un build
//! bâti sur un autre élément. La Terre emporte le Neutre avec elle, les deux
//! tirant de la Force.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn sram() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/sram.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-4.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

/// Ce que chaque lancer du sort nommé vaut, dans l'ordre.
fn lancers(deck: &[&str], sort: &str) -> Vec<f64> {
    let build = Build {
        name: "Sram".into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 630,
                flat_damage: 56,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon: 4,
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
    Engine::new(&sram(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == sort)
        .map(|c| c.damage.as_f64())
        .collect()
}

/// L'Extorsion vole de la Force, et la Force lève ses propres lignes Terre.
#[test]
fn le_vol_leve_les_lignes_de_son_element() {
    let v = lancers(&["extorsion", "sournoiserie"], "extorsion");
    assert!(
        v.len() >= 2,
        "au moins deux Extorsions attendues, mesuré {v:?}"
    );
    let (premier, dernier) = (v[0], *v.last().unwrap());
    assert!(
        dernier > premier,
        "le premier lancer se paie sans le vol, les suivants avec : {v:?}"
    );
    // Cent points sur un facteur de 800 : environ douze pour cent sur le jet,
    // moins une fois les dommages fixes ajoutés. Exiger l'AMPLEUR plutôt qu'un
    // simple sens, une assertion directionnelle passant avec un point volé.
    let ecart = (dernier - premier) / premier;
    assert!(
        ecart > 0.05,
        "l'écart doit valoir plusieurs points, mesuré {:.1} % sur {v:?}",
        ecart * 100.0
    );
}

/// Et il ne lève AUCUNE ligne d'un autre élément.
///
/// C'est le contrôle qui compte. Sans le champ `element`, la Sournoiserie
/// monterait elle aussi, pour un vol de Force dont un sort de Feu ne tire rien.
#[test]
fn le_vol_ne_leve_pas_les_autres_elements() {
    let avec = lancers(&["extorsion", "sournoiserie"], "sournoiserie");
    let sans = lancers(&["sournoiserie"], "sournoiserie");
    assert!(!avec.is_empty() && !sans.is_empty(), "{avec:?} / {sans:?}");
    assert!(
        (avec[0] - sans[0]).abs() < 0.51,
        "un vol de Force ne doit rien faire à une ligne de Feu, \
         mesuré {} accompagné contre {} seul",
        avec[0],
        sans[0]
    );
    assert!(
        avec.iter().all(|d| (d - avec[0]).abs() < 0.51),
        "et la Sournoiserie ne doit pas bouger d'un lancer à l'autre : {avec:?}"
    );
}

/// La Terre emporte le Neutre : les deux tirent de la Force.
#[test]
fn un_vol_de_force_leve_aussi_le_neutre() {
    let avec = lancers(&["extorsion", "meprise"], "meprise");
    let sans = lancers(&["meprise"], "meprise");
    assert!(!avec.is_empty() && !sans.is_empty(), "{avec:?} / {sans:?}");
    // ⚠️ Témoin FAIBLE, et c'est voulu qu'on le sache : la Méprise roule 1-1,
    // donc cent points de caractéristique n'y valent qu'un point de dégâts.
    // L'écart est exact, pas grand, et une assertion d'amplitude n'aurait ici
    // aucun sens.
    assert!(
        avec[0] > sans[0],
        "le Neutre doit monter avec la Force, mesuré {} contre {}",
        avec[0],
        sans[0]
    );
}

// -- et le vol CONDITIONNEL du Huppermage -------------------------------------

/// Le Huppermage, plus une sonde qui frappe dans les quatre éléments sans poser
/// aucun état : la Surcharge Runique frappe dans l'élément de ses runes et ne
/// peut pas servir de témoin. Un sort fictif garde le test sur le vrai fichier
/// de la classe, Drain Élémentaire compris.
const SONDE: &str = r#"
  - id: sonde
    name: { fr: "Sonde", en: "Probe" }
    ap_cost: { base: 3 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      - { element: water, critical: unknown, normal: [10, 10] }
      - { element: fire, critical: unknown, normal: [10, 10] }
      - { element: air, critical: unknown, normal: [10, 10] }
      - { element: earth, critical: unknown, normal: [10, 10] }
    effects:
      - effect: damage
"#;

fn huppermage() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let texte = std::fs::read_to_string(format!("{root}/data/rulesets/huppermage.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    // `spells:` est la dernière clef du fichier : la sonde s'ajoute à la fin.
    let mut rs = Ruleset::from_yaml(&format!("{}\n{SONDE}", texte.trim_end()))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-17.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

/// Ce que vaut la sonde, qui frappe dans les QUATRE éléments et n'applique
/// aucun état : un vol mal conditionné s'y verrait quatre fois plutôt qu'une.
fn sonde(deck: &[&str]) -> f64 {
    let build = Build {
        name: "Huppermage".into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 630,
                flat_damage: 56,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon: 4,
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
    Engine::new(&huppermage(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == "sonde")
        .map(|c| c.damage.as_f64())
        .fold(0.0, f64::max)
}

/// Sur une cible SANS état élémentaire, le Drain ne vole rien du tout.
#[test]
fn le_drain_ne_vole_rien_sans_etat_sur_la_cible() {
    let avec = sonde(&["drain_elementaire", "sonde"]);
    let sans = sonde(&["sonde"]);
    assert!(sans > 0.0, "la sonde doit frapper, mesuré {sans}");
    assert!(
        (avec - sans).abs() < 0.51,
        "aucun des quatre vols ne doit partir sur une cible nue, \
         mesuré {avec} avec le Drain contre {sans} sans lui"
    );
}

/// Avec un état, UN SEUL des quatre vols part, et il ne lève qu'une ligne.
///
/// La sonde frappe dans les quatre éléments. Un vol bien conditionné lui
/// vaut un quart de ce que quatre vols lui vaudraient : c'est le plafond
/// ci-dessous qui porte le test, pas le plancher.
#[test]
fn un_seul_des_quatre_vols_part_a_la_fois() {
    let nue = sonde(&["sonde"]);
    let feu = sonde(&["lance_flamme", "drain_elementaire", "sonde"]);
    assert!(
        feu > nue,
        "le vol d'Intelligence doit lever la ligne Feu, {nue} contre {feu}"
    );
    let ecart = (feu - nue) / nue;
    assert!(
        ecart < 0.10,
        "une ligne sur quatre, pas quatre : l'écart doit rester petit, \
         mesuré {:.1} % entre {nue} et {feu}",
        ecart * 100.0
    );
}
