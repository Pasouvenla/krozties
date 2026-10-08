//! Chaque glyphe du Féca dure ce que dit sa pose, l'effet 401 (402 pour un
//! glyphe de fin de tour) : 2 pour les quatre glyphes de début de tour, 1 pour
//! la Défiance, écrit `turns: d - 1` (un effet de durée d posé au tour N agit
//! jusqu'au tour N+d-1 inclus). Les trois glyphes-auras font exception : leur
//! pose porte 2, mais ils ne vivent que le tour de leur pose.
//!
//! Le contrôle porte sur la donnée : le solveur ne le montre pas toujours, un
//! Pâturage relancé à chaque tour rafraîchissant son aura.

use dofus_ruleset::Ruleset;

/// Là où le jeu contredit la donnée, sur la parole : la Transhumance ne
/// déclenche une aura que dans le tour de sa pose, après elle.
const REGLE_DE_TIM: &[(&str, u64)] = &[
    ("glyphe_paturage", 0),
    ("glyphe_refuge", 0),
    ("glyphe_verglas", 0),
];

#[test]
fn chaque_glyphe_dure_ce_que_sa_pose_dit() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let rs = Ruleset::load(format!("{root}/data/rulesets/feca.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let brut = std::fs::read_to_string(format!("{root}/data/snapshots/breed-1.json"))
        .expect("instantané du Féca");
    let snap: serde_json::Value = serde_json::from_str(&brut).unwrap();

    let mut vus = 0;
    for r in rs.resources.iter().filter(|r| r.id.starts_with("glyphe_")) {
        let source = r
            .dofusdb_source
            .unwrap_or_else(|| panic!("{} : aucun `dofusdb_source`", r.id));
        let sort = snap["spells"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|s| s["id"].as_u64() == Some(u64::from(source)))
            .unwrap_or_else(|| panic!("{} : sort {source} absent de l'instantané", r.id));
        let niveau = sort["levels"]
            .as_array()
            .and_then(|l| l.last())
            .expect("au moins un grade");
        let durees: Vec<u64> = niveau["other_effects"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| matches!(e["id"].as_u64(), Some(401 | 402)))
            .filter_map(|e| e["duration"].as_u64())
            .collect();
        assert_eq!(
            durees.len(),
            1,
            "{} : une pose de glyphe attendue sur le sort {source}, trouvé {durees:?}",
            r.id
        );
        let turns = r
            .duration
            .as_ref()
            .unwrap_or_else(|| panic!("{} : aucune durée", r.id))
            .turns;
        let attendu = REGLE_DE_TIM
            .iter()
            .find(|(id, _)| *id == r.id)
            .map_or(durees[0] - 1, |(_, t)| *t);
        assert_eq!(
            u64::from(turns),
            attendu,
            "{} porte `turns: {turns}`, il en faut {attendu} (la pose du sort {source} dit {} tours)",
            r.id,
            durees[0]
        );
        vus += 1;
    }
    assert_eq!(vus, 8, "huit glyphes attendus, {vus} contrôlés");
}

/// Pas de glyphe, pas d'aura : le bonus de dommages finaux du Pâturage vit
/// exactement aussi longtemps que son glyphe.
#[test]
fn le_bonus_du_paturage_tombe_avec_son_glyphe() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let rs = Ruleset::load(format!("{root}/data/rulesets/feca.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let duree = |id: &str| {
        rs.resource(id)
            .unwrap_or_else(|| panic!("`{id}` absent du fichier"))
            .duration
            .as_ref()
            .unwrap_or_else(|| panic!("`{id}` sans durée"))
            .turns
    };
    assert_eq!(
        duree("aura_paturage"),
        duree("glyphe_paturage"),
        "le bonus du Pâturage doit tomber avec son glyphe"
    );
}
