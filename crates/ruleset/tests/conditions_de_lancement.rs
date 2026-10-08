//! Chaque condition de lancement de la donnée est tenue, ou déclarée.
//!
//! La donnée donne la condition de chaque sort (`statesCriterion`, `HS=3` : « le
//! lanceur est Porteur »), que le moteur lit sur les compteurs des états du jeu.
//! Chaque état cité est représenté par un compteur (`game_states`,
//! `game_states_when_zero`, `tier_states`), ou déclaré hors modèle avec sa
//! raison (`states_outside_model`) : un état nouveau fait tomber le test au lieu
//! de passer en silence.

use std::collections::BTreeSet;

use dofus_ruleset::{snapshot::Snapshot, Critere, Ruleset, SpellDef};

const CLASSES: [(&str, u32); 19] = [
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

fn racine() -> String {
    concat!(env!("CARGO_MANIFEST_DIR"), "/../..").to_string()
}

fn regles(classe: &str, breed: u32) -> Ruleset {
    let r = racine();
    let mut rs = Ruleset::load(format!("{r}/data/rulesets/{classe}.yaml"))
        .unwrap_or_else(|e| panic!("{classe} : {e}"));
    let snap = Snapshot::load(format!("{r}/data/snapshots/breed-{breed}.json")).unwrap();
    let rapport = rs.merge_snapshot(&snap);
    let illisibles: Vec<_> = rapport
        .unmatched
        .iter()
        .filter(|m| m.contains("condition de lancement"))
        .collect();
    assert!(illisibles.is_empty(), "{classe} : {illisibles:?}");
    rs
}

fn representes(rs: &Ruleset) -> BTreeSet<u32> {
    rs.resources
        .iter()
        .flat_map(|r| r.game_states.iter().chain(&r.game_states_when_zero))
        .chain(rs.tier_states.iter().flat_map(|t| &t.game_states))
        .copied()
        .collect()
}

fn conditions(rs: &Ruleset) -> Vec<(String, Critere)> {
    rs.spells
        .iter()
        .flat_map(SpellDef::alternatives)
        .filter_map(|s| Some((s.name.fr.clone(), s.cast_criterion?)))
        .collect()
}

/// Les sorts à condition de la donnée : identifiant, nom, condition.
fn de_la_donnee(breed: u32) -> Vec<(u32, String, Critere)> {
    let brut = std::fs::read_to_string(format!(
        "{}/data/snapshots/breed-{breed}.json",
        racine()
    ))
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&brut).unwrap();
    let mut out = Vec::new();
    for s in v["spells"].as_array().unwrap() {
        let textes: BTreeSet<&str> = s["levels"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|l| l["states_criterion"].as_str())
            .filter(|c| !c.trim().is_empty())
            .collect();
        let nom = s["name"]["fr"].as_str().unwrap().to_string();
        // Un sort dont la condition changerait d'un grade à l'autre serait lu
        // au seul grade le plus haut : aucun ne le fait dans la donnée.
        assert!(textes.len() <= 1, "{nom} : {textes:?}");
        if let Some(t) = textes.first() {
            let c = Critere::lire(t).unwrap_or_else(|e| panic!("{nom} : {e}"));
            out.push((u32::try_from(s["id"].as_u64().unwrap()).unwrap(), nom, c));
        }
    }
    out
}

/// Les 132 sorts qui portent une condition dans la donnée : chacun de ceux que
/// les rulesets modélisent la retrouve, lisible. La Téléportation et la Faille du
/// Xélor, absentes de son ruleset, ne peuvent pas être proposées.
#[test]
fn toutes_les_conditions_de_la_donnee_sont_importees() {
    let mut total = 0;
    let mut absents = Vec::new();
    for (classe, breed) in CLASSES {
        let donnee = de_la_donnee(breed);
        total += donnee.len();
        let rs = regles(classe, breed);
        for (id, nom, c) in &donnee {
            match rs.spells.iter().find(|s| s.dofusdb_id == Some(*id)) {
                None => absents.push(format!("{classe} : {nom}")),
                Some(s) => assert_eq!(s.cast_criterion.as_ref(), Some(c), "{classe} : {nom}"),
            }
        }
    }
    assert_eq!(total, 132, "le decompte de la donnee a change");
    absents.sort();
    assert_eq!(absents, ["xelor : Faille", "xelor : Téléportation"]);
}

/// Chaque état que cite la donnée est représenté par un compteur, ou déclaré
/// hors modèle, jamais les deux, et rien n'est déclaré pour rien. La donnée
/// entière, pour qu'un sort ajouté plus tard trouve ses états déjà tenus.
#[test]
fn chaque_etat_cite_est_represente_ou_declare() {
    for (classe, breed) in CLASSES {
        let rs = regles(classe, breed);
        let representes = representes(&rs);
        let declares: BTreeSet<u32> = rs.states_outside_model.iter().map(|e| e.state).collect();
        assert_eq!(
            declares.len(),
            rs.states_outside_model.len(),
            "{classe} : un etat declare deux fois"
        );
        let cites: BTreeSet<u32> = de_la_donnee(breed)
            .iter()
            .flat_map(|(_, _, c)| c.etats())
            .collect();
        for e in &cites {
            assert!(
                representes.contains(e) || declares.contains(e),
                "{classe} : l'etat {e} est cite par une condition, sans compteur ni declaration"
            );
        }
        for e in &declares {
            assert!(
                !representes.contains(e),
                "{classe} : l'etat {e} a un compteur ET une declaration hors modele"
            );
            assert!(
                cites.contains(e),
                "{classe} : l'etat {e} est declare hors modele sans qu'aucune condition le cite"
            );
        }
    }
}

/// Aucun sort n'est condamné : sa condition tient pour au moins une valeur des
/// états représentés, les états hors modèle étant absents. Un sort qui exige un
/// état que rien ne représente ne partirait jamais.
#[test]
fn aucun_sort_n_est_condamne() {
    for (classe, breed) in CLASSES {
        let rs = regles(classe, breed);
        let representes = representes(&rs);
        for (nom, c) in conditions(&rs) {
            let libres: Vec<u32> = c
                .etats()
                .into_iter()
                .filter(|e| representes.contains(e))
                .collect();
            let possible = (0u32..1 << libres.len()).any(|bits| {
                c.tient(&|e| {
                    libres
                        .iter()
                        .position(|l| *l == e)
                        .is_some_and(|i| bits & (1 << i) != 0)
                })
            });
            assert!(possible, "{classe} : {nom} ne peut jamais etre lance");
        }
    }
}
