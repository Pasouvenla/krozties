//! Des paliers qui changent de largeur, une ligne par cran.
//!
//! La Flèche Dévorante du Crâ vole 11-13 sans état, 23-27 sous le premier cran,
//! 34-38 sous le deuxième : l'amplitude passe de deux points à quatre. Un bonus
//! de dégâts de base décale une fourchette sans l'élargir : il faut une ligne
//! par cran, active à ce cran et à aucun autre.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn cra() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/cra.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    rs.merge_snapshot(
        &Snapshot::load(format!("{root}/data/snapshots/breed-9.json")).unwrap(),
    );
    rs
}

fn moteur(rs: &Ruleset, deck: &[&str]) -> Engine {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 900,
                flat_damage: 0,
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
    Engine::new(rs, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

/// Un palier par cran, et un seul a la fois.
///
/// Ce que le test verifie n'est pas que le sort monte, mais que les paliers ne
/// s'ADDITIONNENT pas : quatre lignes ecrites sur un meme sort frapperaient
/// toutes ensemble sans le filtre, et la Fleche Devorante vaudrait d'emblee la
/// somme de ses quatre crans.
#[test]
fn une_seule_ligne_frappe_par_cran() {
    let rs = cra();
    let table = moteur(&rs, &["fleche_devorante"]).damage_table("fleche_devorante");
    assert_eq!(table.len(), 4, "quatre crans, de zero a trois : {table:?}");

    // Les bornes de base valent 11-13, 23-27, 34-38, 34-38. Le rapport entre
    // deux crans est donc connu, et il ne depend pas du build : verifier des
    // rapports plutot que des valeurs absolues garde le test lisible si les
    // caracteristiques du banc changent.
    let bas: Vec<i64> = table.iter().map(|r| r.normal.0).collect();
    assert!(bas[0] > 0, "le cran zero doit frapper : {bas:?}");
    assert_eq!(
        bas[2], bas[3],
        "les crans II et III portent la meme fourchette, 34-38 : {bas:?}"
    );
    // Somme des quatre crans, ce que donnerait un filtre absent.
    let somme: i64 = bas.iter().sum();
    assert!(
        bas[3] < somme / 2,
        "un cran ne doit pas valoir la somme des quatre : {bas:?}"
    );
    // Et le rapport suit les fourchettes du jeu, 23 sur 11 puis 34 sur 23.
    let r1 = bas[1] as f64 / bas[0] as f64;
    let r2 = bas[2] as f64 / bas[1] as f64;
    assert!(
        (r1 - 23.0 / 11.0).abs() < 0.05,
        "le premier cran doit valoir 23/11 fois le socle, il vaut {r1:.3}"
    );
    assert!(
        (r2 - 34.0 / 23.0).abs() < 0.05,
        "le deuxieme cran doit valoir 34/23 fois le premier, il vaut {r2:.3}"
    );
}

/// Et le solveur s'en sert : relancer sur la meme cible doit rapporter.
#[test]
fn le_solveur_empile_les_crans() {
    let rs = cra();
    let s = moteur(&rs, &["fleche_devorante"]).solve();
    let lancers: Vec<i64> = s
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .map(|c| c.damage.0)
        .collect();
    assert!(lancers.len() >= 3, "au moins trois lancers : {lancers:?}");
    assert!(
        lancers[1] > lancers[0],
        "le deuxieme lancer sur la meme cible doit frapper plus fort que le premier : {lancers:?}"
    );
    assert!(
        lancers[2] > lancers[1],
        "et le troisieme plus que le deuxieme : {lancers:?}"
    );
}
