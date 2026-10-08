//! Une ligne qui refrappe depuis chaque pose au sol.
//!
//! Vingt-huit sorts du jeu portent un effet 1160 qui pointe vers eux-mêmes,
//! ciblé sur les invocations du lanceur (la Lance du Forgelance, les poupées du
//! Sadida, les tourelles du Steamer) : le sort se relance depuis chacune, pour
//! la même fourchette, et `repeats_per` le compte. Le mécanisme sert aussi à
//! l'Eniripsa (peintures), au Huppermage (runes) et à l'Ouginak (Vertèbre) ; un
//! banc synthétique le porte, où seule la répétition change d'une mesure à
//! l'autre.

use dofus_damage::{Damage, DamageProfile, Element, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::Ruleset;

/// Un frappeur dont une ligne refrappe une fois par pose, et un poseur que le
/// budget `poses_par_tour` borne.
fn banc() -> Ruleset {
    Ruleset::from_yaml(
        r#"
schema_version: 1
game_version: "3.6.12"
class: test
name: { fr: "Test", en: "Test" }
resources:
  - id: poses
    scope: caster
    max: 3
    default: 0
    monotone: increasing
budgets:
  - id: poses_par_tour
    name: { fr: "Poses par tour", en: "Placements per turn" }
    default: 1
    max: 3
    note: Combien de poses le tour permet.
spells:
  - id: frappeur
    name: { fr: "Frappeur", en: "Striker" }
    ap_cost: { base: 3 }
    casts_per_turn: 2
    crit: { base_rate: 10 }
    lines:
      - element: air
        critical: [37, 42]
        normal: [30, 34]
      - element: air
        critical: [37, 42]
        normal: [30, 34]
        repeats_per: poses
    effects:
      - effect: damage
  - id: poseur
    name: { fr: "Poseur", en: "Setter" }
    ap_cost: { base: 2 }
    casts_per_turn: 3
    crit: { base_rate: 0, can_crit: false }
    lines: []
    effects:
      - effect: gain
        resource: poses
        budget: poses_par_tour
        at_cap: skip
"#,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn moteur(rs: &Ruleset, deck: &[&str]) -> Engine {
    moteur_budget(rs, deck, &[])
}

fn moteur_budget(rs: &Ruleset, deck: &[&str], budgets: &[(&str, u8)]) -> Engine {
    moteur_horizon(rs, deck, budgets, 3)
}

fn moteur_horizon(rs: &Ruleset, deck: &[&str], budgets: &[(&str, u8)], horizon: u8) -> Engine {
    let mut elements = [ElementStats {
        characteristic: 200,
        flat_damage: 20,
    }; 5];
    elements[Element::Air.index()] = ElementStats {
        characteristic: 900,
        flat_damage: 60,
    };
    let build = Build {
        name: "roublard air".into(),
        profile: DamageProfile {
            power: 150,
            flat_crit_damage: 100,
            elements,
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 30,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: 1,
        horizon,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: budgets
            .iter()
            .map(|(n, v)| ((*n).to_string(), *v))
            .collect(),
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(rs, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

/// Zero pose, une frappe. Une pose, deux. Trois poses, quatre.
#[test]
fn chaque_pose_ajoute_une_frappe_entiere() {
    let rs = banc();
    let table = moteur(&rs, &["frappeur"]).damage_table("frappeur");
    assert_eq!(
        table.len(),
        4,
        "un palier par nombre de poses, de zero a trois : {table:?}"
    );
    let base = table[0].normal.0;
    assert!(base > 0, "le frappeur sans pose doit deja frapper");
    for (i, r) in table.iter().enumerate() {
        // La ligne repetee porte les memes bornes que la premiere : n poses
        // valent donc n + 1 fois la frappe de base, a l'arrondi pres. Verifie
        // en rapport plutot qu'en valeur absolue, le build etant arbitraire.
        let attendu = base * (i as i64 + 1);
        assert!(
            (r.normal.0 - attendu).abs() <= 2,
            "a {i} pose(s) le frappeur vaut {} au lieu de {attendu} : {table:?}",
            r.normal.0
        );
    }
}

/// La répétition passe par le pipeline N fois, pas en multipliant le jet :
/// arrondir une fois un jet triple ne donne pas le même nombre qu'arrondir
/// trois fois le jet nominal, et c'est la seconde qui décrit le jeu.
#[test]
fn la_repetition_arrondit_a_chaque_instance() {
    let rs = banc();
    let table = moteur(&rs, &["frappeur"]).damage_table("frappeur");
    let base = table[0].normal.0;
    assert_eq!(
        table[3].normal.0,
        base * 4,
        "quatre instances arrondies separement doivent donner exactement quatre fois la premiere"
    );
}

/// Le solveur s'en sert : poser doit rapporter. La table de dégâts d'un sort se
/// précalcule par combinaison des ressources dont il dépend, `repeats_per`
/// compris : sans lui, la table resterait de taille un, lue à « zéro pose ».
#[test]
fn le_solveur_pose_pour_frapper_plus() {
    let rs = banc();
    let sans = moteur_budget(&rs, &["frappeur"], &[]).solve();
    let avec = moteur_budget(&rs, &["frappeur", "poseur"], &[("poses_par_tour", 1)]).solve();
    assert!(
        avec.total > sans.total,
        "poser doit rapporter : {} contre {}",
        avec.total,
        sans.total
    );
    // Et le budget borne vraiment : a zero pose permise, le poseur ne
    // rapporte plus rien et le total retombe sur celui du deck sans elle.
    let bride = moteur_budget(&rs, &["frappeur", "poseur"], &[("poses_par_tour", 0)]).solve();
    assert_eq!(
        bride.total, sans.total,
        "budget a zero : le poseur ne doit rien changer"
    );
}

/// Le solveur compte autant de frappes que de poses : la ligne répétée tire une
/// fois dès qu'une pose est là, même si la multiplication par leur nombre
/// sautait. On compare donc la valeur d'un lancer à celle du même lancer sans
/// pose.
#[test]
fn le_solveur_compte_une_frappe_par_pose() {
    let rs = banc();
    let nu = moteur_budget(&rs, &["frappeur"], &[]).solve();
    let base = nu.turns[0].casts[0].damage;

    // Un seul tour, pour que le nombre de poses soit exactement le
    // budget : sur plusieurs tours elles s'accumulent et saturent le plafond.
    //
    for bombes in 1..=3u8 {
        let s = moteur_horizon(
            &rs,
            &["frappeur", "poseur"],
            &[("poses_par_tour", bombes)],
            1,
        )
        .solve();
        let meilleure = s
            .turns
            .last()
            .expect("au moins un tour")
            .casts
            .iter()
            .filter(|c| c.id == "frappeur")
            .map(|c| c.damage)
            .max()
            .expect("le frappeur doit etre lance");
        let attendu = (0..=bombes).map(|_| base).fold(Damage::ZERO, |a, b| a + b);
        assert_eq!(
            meilleure,
            attendu,
            "a {bombes} pose(s) le frappeur doit valoir {} fois sa frappe nue",
            bombes + 1
        );
    }
}
