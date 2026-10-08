//! Aucun chiffre écrit à la main ne contredit l'instantané du jeu.
//!
//! La fusion n'écrase jamais une valeur écrite et range le désaccord dans un
//! rapport : sans ce test, un rééquilibrage arriverait dans l'instantané et le
//! solveur continuerait sur l'ancienne valeur. Seuls les conflits sont une
//! faute : un `unmatched` dit que la fusion n'avait rien à verser, ce qui est
//! normal pour une ligne aux bornes déjà écrites (la seconde ligne des sorts à
//! rebond du Roublard).

use dofus_ruleset::{snapshot::Snapshot, Ruleset};

const CLASSES: &[(&str, u32)] = &[
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

#[test]
fn aucune_classe_ne_contredit_son_instantane() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut fautes = Vec::new();
    let mut verses = 0usize;

    for (classe, breed) in CLASSES {
        let mut rs = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
            .unwrap_or_else(|e| panic!("{classe}: {e}"));
        let snap = Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json"))
            .unwrap_or_else(|e| panic!("{classe}: {e}"));
        let rapport = rs.merge_snapshot(&snap);
        verses += rapport.filled.len();
        for c in &rapport.conflicts {
            fautes.push(format!(
                "{classe}: {} vaut {} ici et {} dans l'instantané. La fusion garde \
                 l'écrit sans rien dire : soit le jeu a rééquilibré, soit c'est une faute \
                 de frappe, et il faut trancher.",
                c.path, c.authored, c.snapshot
            ));
        }
    }

    assert!(
        verses > 100,
        "seulement {verses} valeurs versées par la fusion : le test ne contrôle plus rien"
    );
    assert!(
        fautes.is_empty(),
        "{} désaccord(s) entre un chiffre écrit et la donnée du jeu :\n{}",
        fautes.len(),
        fautes.join("\n")
    );
}

/// L'échappatoire `differs_from_snapshot` lève l'appariement par position sur
/// une ligne : le Topkaj porte l'espérance de son tirage là où la donnée range
/// trois lignes de Feu. Elle se compte, et une déclaration de plus fait tomber ce
/// test, exprès. Elle lève la position, pas la valeur : `authored_magnitudes`
/// exige toujours que chaque fourchette écrite existe sur son sort.
#[test]
fn le_desaccord_assume_reste_rare_et_motive() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut declarations = Vec::new();
    for (classe, _) in CLASSES {
        let rs = Ruleset::load(format!("{root}/data/rulesets/{classe}.yaml"))
            .unwrap_or_else(|e| panic!("{classe}: {e}"));
        // Les trois endroits où une ligne peut vivre : un sort, la charge
        // différée d'un sort, un état.
        let mut relever = |ou: String, l: &dofus_ruleset::LineDef| {
            if let Some(raison) = &l.differs_from_snapshot {
                declarations.push((ou, raison.clone()));
            }
        };
        for s in &rs.spells {
            for (i, l) in s.lines.iter().enumerate() {
                relever(format!("{classe}: {}.lines[{i}]", s.id), l);
            }
            for e in &s.effects {
                if let dofus_ruleset::Effect::Schedule { id, payload, .. } = e {
                    for (i, l) in payload.iter().enumerate() {
                        relever(format!("{classe}: {}.schedule[{id}].payload[{i}]", s.id), l);
                    }
                }
            }
        }
        for r in &rs.resources {
            for (j, e) in r.while_present.iter().enumerate() {
                for (i, l) in e.lines.iter().enumerate() {
                    relever(
                        format!("{classe}: {}.while_present[{j}].lines[{i}]", r.id),
                        l,
                    );
                }
            }
        }
    }
    for (ou, raison) in &declarations {
        assert!(
            raison.trim().len() > 80,
            "{ou} déclare un désaccord sans le motiver : « {raison} »"
        );
    }
    // Les lignes qui ont le droit de différer de l'instantané, nommées pour
    // qu'une autre ne s'ajoute pas en silence :
    //
    // * `ecaflip: topkaj` : l'instantané porte une fourchette que le jeu ne
    //   sert pas ;
    // * `iop: zenith.lines[1]` et `cra: fleche_du_jugement.lines[1]` : la
    //   seconde ligne, au prorata des PM restants, que l'instantané ne porte
    //   pas. Leurs bornes viennent de DofusBook, et le contrôle des magnitudes
    //   exige qu'elles existent sur ce sort.
    let attendues = [
        "ecaflip: topkaj",
        "iop: zenith.lines[1]",
        "cra: fleche_du_jugement.lines[1]",
    ];
    assert_eq!(
        declarations.len(),
        attendues.len(),
        "seules ces lignes ont le droit de différer de l'instantané : {attendues:?}. \
         Si une autre le mérite, relever ce compte ET dire ici pourquoi. Trouvé : {:#?}",
        declarations.iter().map(|(o, _)| o).collect::<Vec<_>>()
    );
    for attendue in attendues {
        assert!(
            declarations.iter().any(|(o, _)| o.starts_with(attendue)),
            "{attendue} ne déclare plus de désaccord, trouvé {:#?}",
            declarations.iter().map(|(o, _)| o).collect::<Vec<_>>()
        );
    }
}
