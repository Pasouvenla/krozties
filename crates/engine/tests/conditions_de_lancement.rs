//! Les conditions de lancement de la donnée, tenues par le moteur.
//!
//! La donnée donne la condition de chaque sort (`statesCriterion`) ; cent
//! trente-deux sorts en portent une, dans seize classes. Le moteur la lit sur
//! les compteurs qui disent chaque état du jeu, et les vérifications passent par
//! `Engine::lancable`, qui appelle la fonction même de la recherche : ce qu'elle
//! refuse ici, le solveur ne le propose pas.

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

fn moteur(rs: &Ruleset, deck: &[&str], ap: u8, horizon: u8) -> Engine {
    moteur_a(rs, deck, ap, horizon, Reach::Ranged)
}

fn moteur_a(rs: &Ruleset, deck: &[&str], ap: u8, horizon: u8, reach: Reach) -> Engine {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 0,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 100,
                flat_damage: 0,
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
        reach,
    };
    Engine::new(rs, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

fn lances(sol: &Solution) -> Vec<String> {
    sol.turns
        .iter()
        .flat_map(|t| t.casts.iter().map(|c| c.id.clone()))
        .collect()
}

// ------------------------------------------------------------------
// Les mécanismes, sur des bancs d'essai
// ------------------------------------------------------------------

/// `porter` fait porter, `jeter` exige de porter et rend la main, et
/// `coup_porte` frappe dix fois plus fort tant qu'on porte.
///
/// L'état interdit de lancer ce qui ne l'exige pas : sans cette règle, le
/// solveur glisserait `coup_porte` entre les deux, pour 220. Avec elle, il ne
/// reste que le jet, 20.
fn banc_porteur() -> Ruleset {
    Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - { id: porte, scope: caster, max: 1, default: 0, monotone: none, game_states: [3], prevents_spell_cast: true }
spells:
  - id: porter
    name: { fr: "Porter", en: "Carry" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    requires: { kind: exactly, resource: porte, amount: 0 }
    lines: []
    effects:
      - { effect: gain, resource: porte }
  - id: jeter
    name: { fr: "Jeter", en: "Throw" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    cast_criterion: !etat { etat: 3, present: true }
    lines:
      - { element: water, critical: [10, 10], normal: [10, 10] }
    effects:
      - effect: damage
      - { effect: consume, resource: porte }
  - id: coup_porte
    name: { fr: "Coup porte", en: "Carried blow" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    lines:
      - { element: water, critical: [100, 100], normal: [100, 100], active_at: { resource: porte, exactly: 1 } }
    effects:
      - effect: damage
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn un_etat_qui_interdit_de_lancer_ne_laisse_que_ce_qui_l_exige() {
    let rs = banc_porteur();
    let m = moteur(&rs, &["porter", "jeter", "coup_porte"], 3, 1);
    assert_eq!(m.lancable("coup_porte", &[("porte", 0)]), Some(true));
    assert_eq!(m.lancable("coup_porte", &[("porte", 1)]), Some(false));
    assert_eq!(m.lancable("jeter", &[("porte", 0)]), Some(false));
    assert_eq!(m.lancable("jeter", &[("porte", 1)]), Some(true));
    let sol = m.solve();
    assert!(
        (sol.total.as_f64() - 20.0).abs() < 0.5,
        "seul le jet frappe : {} ({:?})",
        sol.total.as_f64(),
        lances(&sol)
    );
}

/// Un état porté par un palier de groupe : présent dès que l'un des paliers
/// tient, comme la Main Gagnante de l'Ecaflip.
fn banc_main() -> Ruleset {
    Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.1.4"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - { id: carte, scope: caster, max: 3, default: 0, monotone: none }
tier_states:
  - { group: main, game_states: [5554] }
spells:
  - id: piocher
    name: { fr: "Piocher", en: "Draw" }
    ap_cost: { base: 1 }
    casts_per_turn: 3
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - { effect: gain, resource: carte }
  - id: rekop
    name: { fr: "Rekop", en: "Rekop" }
    ap_cost: { base: 1 }
    casts_per_turn: 1
    crit: { base_rate: 0, can_crit: false }
    cast_criterion: !etat { etat: 5554, present: true }
    lines:
      - { element: water, critical: [50, 50], normal: [50, 50] }
      - element: water
        critical: [100, 100]
        normal: [100, 100]
        active_at: { group: main, tier: paire, all_of: [{ resource: carte, exactly: 2 }] }
      - element: water
        critical: [300, 300]
        normal: [300, 300]
        active_at: { group: main, tier: brelan, all_of: [{ resource: carte, exactly: 3 }] }
    effects:
      - effect: damage
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn un_etat_porte_par_un_palier_se_lit_sur_le_palier() {
    let rs = banc_main();
    let m = moteur(&rs, &["piocher", "rekop"], 4, 1);
    assert_eq!(m.lancable("rekop", &[("carte", 0)]), Some(false));
    assert_eq!(m.lancable("rekop", &[("carte", 1)]), Some(false));
    assert_eq!(m.lancable("rekop", &[("carte", 2)]), Some(true));
    assert_eq!(m.lancable("rekop", &[("carte", 3)]), Some(true));
    // Un PA : sans Main, la ligne de base seule ne suffit pas à la lancer.
    let seul = moteur(&rs, &["piocher", "rekop"], 1, 1).solve();
    assert!(!lances(&seul).contains(&"rekop".to_string()), "{:?}", lances(&seul));
}

// ------------------------------------------------------------------
// Les classes
// ------------------------------------------------------------------

/// La Cascade jette ce que le Pandawa porte : sans rien porter, elle ne part
/// pas.
#[test]
fn le_pandawa_ne_jette_que_ce_qu_il_porte() {
    let rs = regles("pandawa", 12);
    let deck = [
        "karcham", "chamrak", "cascade", "propulsion", "eau_de_vie", "brancard",
        "pandikulation", "pandatak", "picole", "prohibition", "stabilisation", "souillure",
    ];
    let m = moteur(&rs, &deck, 12, 1);
    for jet in ["cascade", "propulsion", "eau_de_vie", "brancard", "pandikulation"] {
        assert_eq!(m.lancable(jet, &[("porteur", 0)]), Some(false), "{jet} sans rien porter");
        assert_eq!(m.lancable(jet, &[("porteur", 1)]), Some(true), "{jet} en portant");
    }
    // Porter exige d'etre Sobre, et de ne rien porter deja.
    assert_eq!(m.lancable("karcham", &[("saoul", 0)]), Some(true));
    assert_eq!(m.lancable("karcham", &[("saoul", 1)]), Some(false));
    assert_eq!(m.lancable("chamrak", &[("porteur", 1)]), Some(false));
    // En portant, rien d'autre ne part (drapeau preventsSpellCast).
    assert_eq!(m.lancable("pandatak", &[("porteur", 0)]), Some(true));
    assert_eq!(m.lancable("pandatak", &[("porteur", 1)]), Some(false));
    // Sobre et Saoul.
    assert_eq!(m.lancable("stabilisation", &[("saoul", 0)]), Some(true));
    assert_eq!(m.lancable("stabilisation", &[("saoul", 1)]), Some(false));
    assert_eq!(m.lancable("souillure", &[("saoul", 1)]), Some(true));
    assert_eq!(m.lancable("souillure", &[("saoul", 0)]), Some(false));
    // La Prohibition ferme la Picole.
    assert_eq!(m.lancable("picole", &[("prohibition", 0)]), Some(true));
    assert_eq!(m.lancable("picole", &[("prohibition", 1)]), Some(false));

    // Seule au deck, la Cascade ne part jamais.
    let seule = moteur(&rs, &["cascade"], 12, 2).solve();
    assert!(lances(&seule).is_empty(), "{:?}", lances(&seule));

    // Avec de quoi porter, chaque jet suit un Karcham, sans rien entre eux.
    let sol = moteur(&rs, &["karcham", "cascade", "eau_de_vie", "pandatak"], 12, 2).solve();
    let seq = lances(&sol);
    // Le jet rend la main : un Karcham et un jet a chaque tour, 3 PA, a
    // cote de deux Pandatak, 8 PA. Un jet qui ne la rendrait pas n'en
    // laisserait qu'un sur les deux tours.
    assert_eq!(
        seq.iter().filter(|id| *id == "cascade" || *id == "eau_de_vie").count(),
        2,
        "{seq:?}"
    );
    assert!(seq.iter().any(|id| id == "pandatak"), "{seq:?}");
    let mut porte = false;
    for id in &seq {
        match id.as_str() {
            "karcham" => {
                assert!(!porte, "{seq:?}");
                porte = true;
            }
            "cascade" | "eau_de_vie" => {
                assert!(porte, "jet sans rien porter : {seq:?}");
                porte = false;
            }
            _ => assert!(!porte, "{id} lance en portant : {seq:?}"),
        }
    }
}

#[test]
fn le_zobal_porte_un_masque_a_la_fois() {
    let rs = regles("zobal", 14);
    let deck = [
        "masque_du_psychopathe", "masque_de_l_intrepide", "masque_du_pleutre", "furia",
        "parafuso", "martelo",
    ];
    let m = moteur(&rs, &deck, 12, 1);
    // Furia exige le Psychopathe (99), Parafuso l'Intrepide (98), Martelo
    // l'Intrepide ou le Pleutre (100).
    assert_eq!(m.lancable("furia", &[]), Some(false));
    assert_eq!(m.lancable("furia", &[("psychopathe", 1)]), Some(true));
    assert_eq!(m.lancable("furia", &[("masque_intrepide", 1)]), Some(false));
    assert_eq!(m.lancable("parafuso", &[("masque_intrepide", 1)]), Some(true));
    assert_eq!(m.lancable("parafuso", &[("masque_pleutre", 1)]), Some(false));
    assert_eq!(m.lancable("martelo", &[("masque_pleutre", 1)]), Some(true));
    assert_eq!(m.lancable("martelo", &[("psychopathe", 1)]), Some(false));

    // Sans masque au deck, rien ne part.
    let nu = moteur(&rs, &["furia", "parafuso"], 12, 2).solve();
    assert!(lances(&nu).is_empty(), "{:?}", lances(&nu));

    // Un masque en remplace un autre : on rejoue la sequence.
    let sol = moteur(&rs, &deck, 12, 3).solve();
    let seq = lances(&sol);
    let mut masque = "";
    for id in &seq {
        match id.as_str() {
            "masque_du_psychopathe" => masque = "psychopathe",
            "masque_de_l_intrepide" => masque = "intrepide",
            "masque_du_pleutre" => masque = "pleutre",
            "furia" => assert_eq!(masque, "psychopathe", "{seq:?}"),
            "parafuso" => assert_eq!(masque, "intrepide", "{seq:?}"),
            "martelo" => assert!(masque == "intrepide" || masque == "pleutre", "{seq:?}"),
            _ => {}
        }
    }
}

/// Deux Tibias montent la Rage à deux ; le troisième fait passer en Forme
/// Bestiale, qui vaut +20 % de dommages finaux, et le quatrième en profite.
#[test]
fn la_troisieme_rage_fait_passer_en_forme_bestiale() {
    let rs = regles("ouginak", 18);
    let m = moteur(&rs, &["tibia", "arcanin", "ferocite"], 12, 1);
    assert_eq!(m.lancable("arcanin", &[("rage", 0)]), Some(false));
    assert_eq!(m.lancable("arcanin", &[("rage", 1)]), Some(true));
    assert_eq!(m.lancable("ferocite", &[("forme_bestiale", 0)]), Some(true));
    assert_eq!(m.lancable("ferocite", &[("forme_bestiale", 1)]), Some(false));

    let sol = moteur(&rs, &["tibia"], 12, 2).solve();
    let coups: Vec<Vec<f64>> = sol
        .turns
        .iter()
        .map(|t| t.casts.iter().map(|c| c.damage.as_f64()).collect())
        .collect();
    assert_eq!(coups.len(), 2, "{coups:?}");
    assert_eq!(coups[0].len(), 2, "{coups:?}");
    assert_eq!(coups[1].len(), 2, "{coups:?}");
    let base = coups[0][0];
    assert!((coups[0][1] / base - 1.0).abs() < 0.01, "{coups:?}");
    // Le troisieme declenche la Forme Bestiale apres sa frappe.
    assert!((coups[1][0] / base - 1.0).abs() < 0.01, "{coups:?}");
    assert!((coups[1][1] / base - 1.2).abs() < 0.01, "{coups:?}");
}

/// La Forme Bestiale court le tour de son déclenchement et le suivant, N et
/// N+1, pas N+2.
#[test]
fn la_forme_bestiale_court_les_tours_n_et_n_plus_1() {
    let rs = regles("ouginak", 18);
    let sol = moteur(&rs, &["tibia"], 12, 5).solve();
    let coups: Vec<Vec<f64>> = sol
        .turns
        .iter()
        .map(|t| t.casts.iter().map(|c| c.damage.as_f64()).collect())
        .collect();
    let base = coups[0][0];
    let rapports: Vec<Vec<f64>> = coups
        .iter()
        .map(|t| t.iter().map(|d| (d / base * 100.0).round() / 100.0).collect())
        .collect();
    // Transformation au premier Tibia du tour 2 (N), toujours la au tour 3,
    // partie au tour 4, ou la Rage remonte : ses deux Tibias et le premier du
    // tour 5 donnent les trois crans d'une nouvelle transformation.
    assert_eq!(
        rapports,
        [
            vec![1.0, 1.0],
            vec![1.0, 1.2],
            vec![1.2, 1.2],
            vec![1.0, 1.0],
            vec![1.0, 1.2]
        ]
    );
}

#[test]
fn l_eclipse_desarme_et_ferme_vingt_cinq_sorts() {
    let rs = regles("forgelance", 20);
    let deck = ["eclipse", "parade", "jormun", "lance_a_incendie", "muspel"];
    let m = moteur(&rs, &deck, 12, 1);
    // Arme (lance a zero) ou Desarme (Lance plantee).
    assert_eq!(m.lancable("lance_a_incendie", &[("lance", 0)]), Some(true));
    assert_eq!(m.lancable("lance_a_incendie", &[("lance", 1)]), Some(false));
    assert_eq!(m.lancable("muspel", &[("lance", 1)]), Some(true));
    assert_eq!(m.lancable("muspel", &[("lance", 0)]), Some(false));
    // Sous Eclipse, ni le Jormun ni le Muspel.
    assert_eq!(m.lancable("jormun", &[("eclipse_en_cours", 1)]), Some(false));
    assert_eq!(
        m.lancable("muspel", &[("lance", 1), ("eclipse_en_cours", 1)]),
        Some(false)
    );
    // Sous Parade, pas d'Eclipse.
    assert_eq!(m.lancable("eclipse", &[("parade_posee", 0)]), Some(true));
    assert_eq!(m.lancable("eclipse", &[("parade_posee", 1)]), Some(false));

    // L'Eclipse rappelle la Lance mais desarme : elle ne rend pas la main a
    // une seconde Lance a Incendie. Quand elle le faisait, le tour valait deux
    // Lances autour d'une Eclipse, 3 + 5 + 3 PA.
    let sol = moteur(&rs, &["lance_a_incendie", "eclipse"], 12, 1).solve();
    let seq = lances(&sol);
    assert_eq!(
        seq.iter().filter(|id| *id == "lance_a_incendie").count(),
        1,
        "{seq:?}"
    );
}

/// L'Éclipse tombe au tour suivant, où le Forgelance récupère la Lance et
/// lance Jormun dans le même tour : l'état Éclipse ne court que le tour du
/// lancer.
#[test]
fn le_jormun_repart_au_tour_ou_l_eclipse_tombe() {
    let rs = regles("forgelance", 20);
    let sol = moteur(&rs, &["eclipse", "jormun"], 12, 3).solve();
    let tours: Vec<Vec<String>> = sol
        .turns
        .iter()
        .map(|t| t.casts.iter().map(|c| c.id.clone()).collect())
        .collect();
    let k = tours
        .iter()
        .position(|t| t.contains(&"eclipse".to_string()))
        .unwrap_or_else(|| panic!("pas d'Eclipse : {tours:?}"));
    // Apres l'Eclipse, plus de Jormun dans le meme tour...
    let apres = &tours[k][tours[k].iter().position(|id| id == "eclipse").unwrap()..];
    assert!(!apres.contains(&"jormun".to_string()), "{tours:?}");
    // ... mais des le tour ou elle tombe.
    assert!(
        tours.get(k + 1).is_some_and(|t| t.contains(&"jormun".to_string())),
        "{tours:?}"
    );
}

/// Le Prélude au Fer ne donne sa Puissance au lanceur que s'il est Armé : Lance
/// plantée, il part de la Lance, et à distance le lanceur n'est pas dans sa
/// zone (masque « a,*E3360 » de la donnée). Sans aucun sort qui rappelle la
/// Lance, elle reste plantée dès le premier Javelot-foudre : un Prélude lancé
/// ensuite ne pose plus rien, ce que le moteur note « sans génération ».
#[test]
fn le_prelude_au_fer_ne_donne_rien_lance_plantee() {
    let rs = regles("forgelance", 20);
    let sol = moteur(&rs, &["prelude_au_fer", "javelot_foudre", "terre_du_milieu"], 12, 4).solve();
    let mut plantee = false;
    let mut preludes = 0;
    for c in sol.turns.iter().flat_map(|t| &t.casts) {
        match (c.id.as_str(), c.spell.contains("sur un monstre")) {
            ("prelude_au_fer", _) => {
                preludes += 1;
                if plantee {
                    assert!(
                        c.notes.iter().any(|n| n.contains("sans génération")),
                        "Prélude au Fer Lance plantée, et sa Puissance posée : {:?}",
                        lances(&sol)
                    );
                }
            }
            ("javelot_foudre", false) => plantee = true,
            _ => {}
        }
    }
    assert!(preludes > 0, "{:?}", lances(&sol));
}

/// L'Éclipse part dès le premier tour, et le Jormun aussi, sans délai initial,
/// comme le dit la donnée.
#[test]
fn l_eclipse_et_le_jormun_partent_au_premier_tour() {
    // Rejoués, et non cherchés : la recherche peut aussi bien les lancer au
    // deuxième tour, au même total (l'Éclipse frappe au tour qui suit son
    // lancer, et ne part qu'une fois sur trois tours). Seul le rejeu dit
    // qu'ils PEUVENT partir au premier.
    let rs = regles("forgelance", 20);
    let eclipse = moteur(&rs, &["eclipse"], 12, 3).replay(&[vec!["eclipse".into()]]);
    assert!(eclipse.is_ok(), "{eclipse:?}");
    // Au contact, le Jormun se passe de la Lance.
    let jormun = moteur_a(&rs, &["jormun"], 12, 2, Reach::Melee).replay(&[vec!["jormun".into()]]);
    assert!(jormun.is_ok(), "{jormun:?}");
}

#[test]
fn le_glas_et_les_glyphes_lisent_leur_etat() {
    let xelor = regles("xelor", 5);
    let m = moteur(&xelor, &["glas"], 12, 1);
    assert_eq!(m.lancable("glas", &[("glas_stacks", 0)]), Some(false));
    assert_eq!(m.lancable("glas", &[("glas_stacks", 1)]), Some(true));
    let feca = regles("feca", 1);
    let m = moteur(&feca, &["prairie"], 12, 1);
    assert_eq!(m.lancable("prairie", &[("hypoglyphe", 0)]), Some(true));
    assert_eq!(m.lancable("prairie", &[("hypoglyphe", 1)]), Some(false));
}
