//! Rejouer une rotation donnée, tour par tour : ce qu'un sort écarté vaudrait
//! à un instant se lit en rejouant la rotation avec lui. Les règles sont celles
//! de la recherche, lancer par lancer : rejouer la rotation que la recherche a
//! trouvée rend exactement ses chiffres, ce qui dit qu'aucune règle n'est
//! restée d'un seul côté.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn classe(fichier: &str, instantane: &str) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{fichier}.yaml")).unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/{instantane}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn moteur(rs: &Ruleset, deck: &[&str], horizon: u8) -> Engine {
    let build = Build {
        name: "rejeu".into(),
        profile: DamageProfile {
            power: 150,
            flat_crit_damage: 150,
            elements: [ElementStats { characteristic: 700, flat_damage: 80 }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 25,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: 1,
        horizon,
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
    Engine::new(rs, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

/// La rotation trouvée, rejouée lancer par lancer, rend les mêmes dégâts à
/// chaque tour et au total. Trois classes aux mécaniques différentes : le Iop
/// et ses buffs, le Xélor et ses charges différées, l'Osamodas et son Pacte.
#[test]
fn rejouer_la_rotation_trouvee_rend_ses_chiffres() {
    let cas: [(&str, &str, &[&str]); 3] = [
        ("iop", "breed-8", &["couperet", "rassemblement", "epee_destructrice", "epee_du_destin", "puissance", "sentence"]),
        ("xelor", "breed-5", &["aiguille", "rouage", "frappe_de_xelor", "horloge", "fuite_du_temps"]),
        ("osamodas", "breed-2", &["cri_du_corbac", "pics_du_prespic", "saute_granouille", "souffle_draconique", "pacte_bestial"]),
    ];
    for (fichier, instantane, deck) in cas {
        let rs = classe(fichier, instantane);
        let m = moteur(&rs, deck, 5);
        let trouvee = m.solve();
        let tours: Vec<Vec<String>> =
            trouvee.turns.iter().map(|t| t.casts.iter().map(|c| c.spell.clone()).collect()).collect();
        let rejouee = m.replay(&tours).unwrap_or_else(|e| panic!("{fichier} : {e}"));
        assert_eq!(rejouee.total, trouvee.total, "{fichier}");
        for (a, b) in rejouee.turns.iter().zip(&trouvee.turns) {
            assert_eq!((a.opening_damage, a.damage, a.ap_left), (b.opening_damage, b.damage, b.ap_left), "{fichier}, tour {}", a.turn);
        }
    }
}

/// Un lancer qui ne peut pas partir se nomme : le Pacte Bestial a un délai
/// initial d'un tour, il ne part pas au premier.
#[test]
fn un_lancer_impossible_se_nomme() {
    let rs = classe("osamodas", "breed-2");
    let m = moteur(&rs, &["cri_du_corbac", "pacte_bestial"], 2);
    let e = m.replay(&[vec!["cri_du_corbac".into(), "pacte_bestial".into()]]).unwrap_err();
    assert!(e.starts_with("tour 1, lancer 2 : « pacte_bestial »"), "{e}");
    let e = m.replay(&[vec!["fouet".into()]]).unwrap_err();
    assert!(e.contains("n'est pas dans le deck"), "{e}");
    // Au deuxième tour, il part, et ses PA arrivent au troisième.
    let s = m
        .replay(&[vec!["cri_du_corbac".into()], vec!["pacte_bestial".into()], vec![]])
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(s.turns[2].ap_left, 14, "douze PA et les deux du Pacte");
}

/// Ce que chaque sort vaudrait à la place d'un lancer : le sort joué y figure
/// avec ses dégâts de la rotation, la liste va du plus fort au plus faible,
/// et un sort qui ne peut pas partir à cet instant n'y est pas.
#[test]
fn les_remplacants_d_un_lancer() {
    let rs = classe("iop", "breed-8");
    let m = moteur(&rs, &["couperet", "rassemblement", "epee_destructrice", "epee_du_destin", "puissance", "sentence"], 3);
    let trouvee = m.solve();
    let tours: Vec<Vec<String>> =
        trouvee.turns.iter().map(|t| t.casts.iter().map(|c| c.spell.clone()).collect()).collect();
    let joue = &trouvee.turns[0].casts[0];
    let liste = m.remplacants(&tours, 0, 0).unwrap_or_else(|e| panic!("{e}"));
    let lui = liste.iter().find(|c| c.spell == joue.spell).expect("le sort joué est dans la liste");
    assert_eq!(lui.damage, joue.damage);
    assert!(liste.windows(2).all(|w| w[0].damage >= w[1].damage), "{liste:?}");
    assert!(liste.len() >= 2, "{liste:?}");
    // Au dernier lancer du premier tour, un sort déjà lancé à son plafond
    // n'est plus proposé : chaque nom n'y figure qu'une fois, et tous peuvent
    // partir.
    let dernier = tours[0].len() - 1;
    let fin = m.remplacants(&tours, 0, dernier).unwrap();
    for c in &fin {
        let mut script = tours[..0].to_vec();
        let mut t = tours[0][..dernier].to_vec();
        t.push(c.spell.clone());
        script.push(t);
        assert!(m.replay(&script).is_ok(), "{} ne peut pas partir là", c.spell);
    }
    assert!(m.remplacants(&tours, 9, 0).is_err());
    assert!(m.remplacants(&tours, 0, 99).is_err());
}
