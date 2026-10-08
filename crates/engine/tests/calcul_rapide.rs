//! Écarter une mécanique coûteuse, ce que fait la case « calcul rapide » : le
//! total est plus bas, jamais plus haut. Le joueur échange de l'exactitude
//! contre du temps, et doit savoir de quel côté penche l'erreur.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn classe(nom: &str, breed: u32) -> Ruleset {
    let racine = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{racine}/data/rulesets/{nom}.yaml"))
        .unwrap_or_else(|e| panic!("{nom}: {e}"));
    // Les fourchettes vivent dans l'instantané, pas dans le fichier de règles.
    let snap =
        Snapshot::load(format!("{racine}/data/snapshots/breed-{breed}.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

fn resoudre(rs: &Ruleset, deck: &[&str], pa: u8, cibles: u8) -> Solution {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 100,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 300,
                flat_damage: 0,
            }; 5],
            ..Default::default()
        },
        base_ap: pa,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| (*s).to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        etats_declares: vec![],
        horizon: 4,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        targets: cibles,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: rs
            .budgets
            .iter()
            .map(|b| (b.id.clone(), b.default))
            .collect(),
        dominance: true,
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
}

/// Les trois classes qui en déclarent une, et le sens de l'échange.
///
/// Le Huppermage perd le bonus de caractéristique de ses runes, le Xélor la
/// remise de PA de sa Pétrification : les deux doivent frapper MOINS fort. Le
/// total allégé ne peut jamais dépasser le total complet.
#[test]
fn ecarter_une_mecanique_couteuse_ne_fait_jamais_monter_le_total() {
    let cas: &[(&str, u32, &[&str], u8, u8)] = &[
        (
            "huppermage",
            17,
            &["lance_flamme", "runification", "stalagmite", "orage"],
            12,
            3,
        ),
        ("xelor", 5, &["gelure", "petrification", "clepsydre", "glas"], 12, 3),
        (
            "ecaflip",
            6,
            &["jass", "belote", "blakjak", "rekop", "griffe_joueuse"],
            12,
            1,
        ),
    ];
    for (nom, breed, deck, pa, cibles) in cas {
        let rs = classe(nom, *breed);
        assert!(!rs.costly.is_empty(), "{nom} ne déclare aucune mécanique coûteuse");
        let complet = resoudre(&rs, deck, *pa, *cibles).total.as_f64();
        let allege = resoudre(&rs.sans_mecaniques_couteuses(), deck, *pa, *cibles)
            .total
            .as_f64();
        assert!(
            allege <= complet + 0.01,
            "{nom} : le calcul rapide rend {allege:.2}, au-dessus des {complet:.2} du calcul complet"
        );
    }
}

/// Et la case sert à quelque chose : sur le Huppermage, le bonus de
/// caractéristique des runes change le chiffre.
#[test]
fn le_calcul_rapide_change_vraiment_le_total_du_huppermage() {
    let rs = classe("huppermage", 17);
    let deck = &["lance_flamme", "runification", "stalagmite", "orage"];
    let complet = resoudre(&rs, deck, 12, 3).total.as_f64();
    let allege = resoudre(&rs.sans_mecaniques_couteuses(), deck, 12, 3)
        .total
        .as_f64();
    assert!(
        complet - allege > 1.0,
        "le bonus des runes devrait peser : {complet:.2} contre {allege:.2}"
    );
}

/// Un compteur écarté ne coûte plus rien à la recherche, ni valeur ni durée :
/// sinon il resterait une dimension d'état valant toujours zéro, et le calcul
/// « rapide » ne le serait pas.
#[test]
fn un_compteur_ecarte_perd_sa_valeur_et_sa_duree() {
    for (nom, breed) in [("huppermage", 17), ("xelor", 5), ("ecaflip", 6)] {
        let rs = classe(nom, breed);
        let ecartees: Vec<String> = rs
            .costly
            .iter()
            .flat_map(|c| c.resources.clone())
            .collect();
        let allege = rs.sans_mecaniques_couteuses();
        for id in &ecartees {
            let r = allege
                .resource(id)
                .unwrap_or_else(|| panic!("{nom}: `{id}` a disparu au lieu d'être neutralisé"));
            assert_eq!(r.max, 0, "{nom}: `{id}` garde un plafond");
            assert!(r.duration.is_none(), "{nom}: `{id}` garde sa durée");
            assert!(
                r.modifies_damage.is_empty(),
                "{nom}: `{id}` garde un modificateur de dégâts"
            );
        }
        // Et les remises de coût qui les lisaient sont parties avec.
        for sort in &allege.spells {
            if let Some(cr) = &sort.ap_cost.reduced_by {
                assert!(
                    !ecartees.contains(&cr.resource),
                    "{nom}: {} garde une remise sur `{}`",
                    sort.id,
                    cr.resource
                );
            }
        }
    }
}
