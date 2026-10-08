//! Un compteur de Combinaisons, distinct des drapeaux de combinaison.
//!
//! Le Torrent Arcanique « augmente ses dommages pour chaque combinaison
//! élémentaire générée par le lanceur », deux points de dégâts de base par cran
//! et par ligne, six crans au plus, remis à zéro après le lancer. Les drapeaux
//! ne pouvaient pas le porter, un modificateur d'état valant son montant par
//! charge : six combinaisons auraient donné trois cents Puissance. D'où un
//! compteur à part, gagné aux mêmes endroits et sous les mêmes conditions que
//! les six drapeaux `comb_*`, un par combinaison : la Puissance se gagne « une
//! fois maximum par combinaison élémentaire différente », et une combinaison
//! consomme les deux états, celui du sort compris.

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn huppermage() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/huppermage.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-17.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

/// Ce que vaut chaque Torrent de la rotation, dans l'ordre.
fn torrents(deck: &[&str], ap: u8) -> Vec<f64> {
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
        base_ap: ap,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 6,
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
    Engine::new(&huppermage(), build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .filter(|c| c.id == "torrent_arcanique")
        .map(|c| c.damage.as_f64())
        .collect()
}

/// Les crans se gagnent en enchaînant deux éléments différents.
#[test]
fn le_torrent_monte_avec_les_combinaisons() {
    let seul = torrents(&["torrent_arcanique"], 12);
    let avec = torrents(&["lance_flamme", "stalagmite", "torrent_arcanique"], 12);
    assert!(!seul.is_empty() && !avec.is_empty(), "{seul:?} / {avec:?}");
    assert!(
        seul.windows(2).all(|f| (f[0] - f[1]).abs() < 0.51),
        "sans generateur le Torrent ne doit pas bouger, mesuré {seul:?}"
    );
    // Six crans de deux points sur QUATRE lignes, contre une fourchette nue de
    // 2 a 4 : le sort double et au-delà. Exiger l'AMPLEUR, une assertion
    // directionnelle passant avec un seul cran.
    let rapport = avec[0] / seul[0];
    assert!(
        rapport > 1.8,
        "le Torrent doit au moins doubler, mesuré {rapport:.2} sur {seul:?} contre {avec:?}"
    );
}

/// Le lancer remet le compte à zéro, et le suivant repart de rien. Une
/// combinaison consomme les deux états, celui du sort compris : en alternant
/// deux éléments, un lancer sur deux combine. À neuf PA, le premier Torrent
/// part avec peu de crans et le second, plus tard, avec davantage.
#[test]
fn le_lancer_remet_le_compte_a_zero() {
    let rs = huppermage();
    let torrent = rs
        .spells
        .iter()
        .find(|s| s.id == "torrent_arcanique")
        .expect("le Torrent est au fichier");
    assert!(
        torrent.effects.iter().any(|e| matches!(
            e,
            dofus_ruleset::Effect::Reset { resource, .. } if resource == "combinaisons_generees"
        )),
        "le Torrent doit remettre son compte à zéro"
    );
    let neuf = torrents(&["lance_flamme", "stalagmite", "torrent_arcanique"], 9);
    assert!(neuf.len() >= 2, "deux Torrents attendus sur six tours, mesuré {neuf:?}");
    assert!(
        neuf[0] + 0.51 < neuf[1],
        "le compte se reconstitue entre deux Torrents, mesuré {neuf:?}"
    );
}

/// Le motif d'effets des vingt-quatre sorts élémentaires, sur un banc à deux
/// éléments : une combinaison consomme les deux états, celui du sort compris,
/// et en alternant Feu et Eau, un lancer sur deux combine.
#[test]
fn en_alternant_deux_elements_un_lancer_sur_deux_combine() {
    let sort = |id: &str, mien: &str, autre: &str| {
        format!(
            r#"
  - id: {id}
    name: {{ fr: "{id}", en: "{id}" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 4
    crit: {{ base_rate: 0, can_crit: false }}
    lines: []
    effects:
      - {{ effect: gain, resource: compte, requires: {{ kind: target_has, resource: {autre} }} }}
      - {{ effect: gain, resource: ce_lancer, requires: {{ kind: target_has, resource: {autre} }} }}
      - {{ effect: gain, resource: {mien}, at_cap: refresh, requires: {{ kind: exactly, resource: ce_lancer, amount: 0 }} }}
      - {{ effect: reset, resource: {autre} }}
      - {{ effect: reset, resource: ce_lancer }}"#
        )
    };
    let yaml = format!(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: {{ fr: "Test", en: "Test" }}
resources:
  - {{ id: etat_feu, scope: target, max: 1, default: 0, monotone: none }}
  - {{ id: etat_eau, scope: target, max: 1, default: 0, monotone: none }}
  - {{ id: ce_lancer, scope: caster, max: 1, default: 0, monotone: none }}
  - {{ id: compte, scope: caster, max: 6, default: 0, monotone: increasing }}
spells:{}{}
  - id: lire
    name: {{ fr: "Lire", en: "Read" }}
    ap_cost: {{ base: 1 }}
    casts_per_turn: 1
    crit: {{ base_rate: 0, can_crit: false }}
    lines:
      - {{ element: fire, normal: [10, 10], critical: [10, 10], repeats_per: [compte] }}
    effects:
      - effect: damage
"#,
        sort("feu", "etat_feu", "etat_eau"),
        sort("eau", "etat_eau", "etat_feu"),
    );
    let rs = Ruleset::from_yaml(&yaml).unwrap_or_else(|e| panic!("{e}"));
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile::default(),
        base_ap: 5,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: vec!["feu".into(), "eau".into(), "lire".into()],
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
    let sol = Engine::new(&rs, build, sc).unwrap_or_else(|e| panic!("{e}")).solve();
    // Quatre lancers élémentaires puis la lecture : deux combinaisons, vingt.
    assert_eq!(sol.total.as_f64(), 20.0, "{sol}");
}

/// Les six combinaisons, chacune sa Puissance, et l'Éruption sur la cible :
/// 50 Puissance, ×1,15 aux dommages subis pour l'Éruption (sous-sort 23876).
#[test]
fn chaque_paire_d_elements_donne_sa_combinaison() {
    use dofus_ruleset::{DamageModifier, Effect};
    let rs = huppermage();
    let paires = [
        ("feu", ["eruption", "ebullition", "carbonisation"], ["terre", "eau", "air"]),
        ("eau", ["enlisement", "ebullition", "cristallisation"], ["terre", "feu", "air"]),
        ("terre", ["eruption", "enlisement", "assechement"], ["feu", "eau", "air"]),
        ("air", ["carbonisation", "cristallisation", "assechement"], ["feu", "eau", "terre"]),
    ];
    let mut sorts_vus = 0;
    for s in &rs.spells {
        let Some((element, combos, autres)) = paires.iter().find(|(e, ..)| {
            s.effects.iter().any(|x| {
                matches!(x, Effect::Gain { resource, .. } if *resource == format!("etat_{e}"))
            })
        }) else {
            continue;
        };
        sorts_vus += 1;
        // Son propre état, seulement sans combinaison : elle consomme les deux.
        let pose_sans_combiner = s.effects.iter().any(|x| matches!(x,
            Effect::Gain { resource, requires: Some(dofus_ruleset::Condition::Exactly { resource: r, amount: 0 }), .. }
                if *resource == format!("etat_{element}") && r == "combinaison_ce_lancer"));
        assert!(pose_sans_combiner, "{} repose son état après une combinaison", s.id);
        for (combo, autre) in combos.iter().zip(autres) {
            let gagne = s.effects.iter().any(|x| matches!(x,
                Effect::Gain { resource, requires: Some(dofus_ruleset::Condition::TargetHas { resource: r }), .. }
                    if *resource == format!("comb_{combo}") && *r == format!("etat_{autre}")));
            assert!(gagne, "{} ({element}) sur {autre} doit donner comb_{combo}", s.id);
        }
    }
    assert_eq!(sorts_vus, 24, "vingt-quatre sorts élémentaires attendus");
    let puissance = |id: &str| {
        let r = rs.resource(id).unwrap_or_else(|| panic!("{id} absent"));
        assert_eq!(r.max, 1, "{id} : une fois au plus");
        r.modifies_damage
            .iter()
            .any(|m| matches!(m, DamageModifier::Characteristic { amount, .. } if amount.known() == Some(&50)))
    };
    for c in ["eruption", "enlisement", "carbonisation", "ebullition", "cristallisation", "assechement"] {
        assert!(puissance(&format!("comb_{c}")), "comb_{c} doit porter 50 Puissance");
    }
    let eruption = rs.resource("eruption").expect("l'Éruption est au fichier");
    assert_eq!(eruption.duration.as_ref().map(|d| d.turns), Some(0));
    assert!(eruption
        .modifies_damage
        .iter()
        .any(|m| matches!(m, DamageModifier::FinalMultiplier { percent, .. } if percent.known() == Some(&115))));
}

/// Les six Puissances courent chacune de leur côté, et leurs minuteurs
/// multiplieraient les états entre tours sans rien changer à ce qui se joue
/// dans le tour : le moteur ne cherche qu'une fois les fins de tour de deux
/// états qui ne diffèrent que par leurs minuteurs.
#[test]
fn les_minuteurs_ne_multiplient_pas_les_recherches() {
    let build = Build {
        name: "Huppermage".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 200,
                flat_damage: 0,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        deck: [
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
        .map(|s| s.to_string())
        .collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 7,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: true,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let moteur = Engine::new(&huppermage(), build, sc).unwrap_or_else(|e| panic!("{e}"));
    moteur.solve();
    let recherches = moteur.stats().turn_searches;
    // Le graphe entre tours compte chaque état une fois, minuteurs compris :
    // c'est ce que coûtait la recherche quand sa clé les gardait (2 432 ici,
    // contre 760 recherches).
    let etats = moteur.steady_state().states as u64;
    assert!(
        recherches * 2 < etats,
        "{recherches} recherches pour {etats} états : les minuteurs multiplient de nouveau le travail"
    );
}

/// `solve` ne cherche les fins de tour que des états qu'il visite, ceux de son
/// horizon ; le reste du graphe atteignable n'est demandé que par la boucle.
#[test]
fn solve_ne_cherche_que_son_horizon() {
    let rs = huppermage();
    let build = Build {
        name: "Huppermage".into(),
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
        deck: rs.spells.iter().take(5).map(|s| s.id.clone()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 2,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: false,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    let moteur = Engine::new(&rs, build, sc).unwrap_or_else(|e| panic!("{e}"));
    let sol = moteur.solve();
    let recherches = moteur.stats().turn_searches;
    assert!(
        recherches <= sol.inter_turn_states as u64,
        "{recherches} recherches pour {} états visités : solve cherche au-delà de son horizon",
        sol.inter_turn_states
    );
}
