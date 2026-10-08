//! Ce que les objets de classe changent aux sorts : les effets 281 à 297 de la
//! donnée du jeu (« Flèche Glacée : +4 dégâts de base », « Piège Sournois : +3
//! Portée maximale », « Lapement : +35 % Critique »), chacun pour une seule
//! classe. Ils s'appliquent aux règles de la classe une fois le build résolu :
//! la Rotation, KrozZone, KrozPortal, KrozTrap et les infobulles voient tous le
//! même sort. Un objet d'une autre classe ne trouve aucun de ses sorts.

use dofus_build::{Resolved, SpellModifier, SpellModifierKind as K};
use dofus_ruleset::snapshot::{Level, Snapshot};
use dofus_ruleset::{LineDef, Maybe, Ruleset, SpellDef};

/// Applique aux règles de la classe les modificateurs des objets portés.
pub fn sur_les_regles(regles: &mut Ruleset, resolu: &Resolved) {
    for (_, m) in &resolu.spell_modifiers {
        for sort in regles.spells.iter_mut().filter(|s| s.dofusdb_id == Some(m.spell)) {
            sur_le_sort(sort, m);
        }
    }
}

/// La même chose sur l'instantané, que KrozTrap lit directement : portée,
/// règles de lancer, limites, relance et critique. Pas les dégâts : KrozTrap ne
/// frappe que par les pièges du Sram, dont aucun objet ne change les dégâts
/// (`les_degats_des_pieges_ne_changent_pas`).
pub fn sur_l_instantane(instantane: &mut Snapshot, resolu: &Resolved) {
    for (_, m) in &resolu.spell_modifiers {
        for sort in instantane.spells.iter_mut().filter(|s| s.id == m.spell) {
            for niveau in &mut sort.levels {
                sur_le_niveau(niveau, m);
            }
        }
    }
}

/// Ce que les objets portés disent de ce sort, dans les mots du jeu : «
/// Coiffe de Robbie Capuche : +6 dégâts de base ».
pub fn sources(resolu: &Resolved, dofusdb_id: u32) -> Vec<String> {
    resolu
        .spell_modifiers
        .iter()
        .filter(|(_, m)| m.spell == dofusdb_id)
        .map(|(objet, m)| format!("{objet} : {}", m.libelle()))
        .collect()
}

/// Le nom d'un sort du jeu, quelle que soit sa classe : la fiche d'un build
/// montre ce qu'un objet de classe dit, même porté par une autre classe.
pub fn nom_du_sort(id: u32) -> Option<&'static str> {
    static NOMS: std::sync::OnceLock<std::collections::BTreeMap<u32, String>> = std::sync::OnceLock::new();
    NOMS.get_or_init(|| {
        crate::solve::CLASSES
            .iter()
            .filter_map(|(classe, _, _)| crate::solve::snapshot_for(*classe).ok())
            .flat_map(|i| i.spells.into_iter().map(|s| (s.id, s.name.fr)))
            .collect()
    })
    .get(&id)
    .map(String::as_str)
}

/// Le critique que les objets ajoutent au taux de base de ce sort.
pub fn critique_des_objets(resolu: &Resolved, dofusdb_id: u32) -> i32 {
    resolu
        .spell_modifiers
        .iter()
        .filter(|(_, m)| m.spell == dofusdb_id && m.kind == K::CriticalRate)
        .map(|(_, m)| m.value)
        .sum()
}

fn plus(x: u8, v: i32) -> u8 {
    u8::try_from((i32::from(x) + v).clamp(0, i32::from(u8::MAX))).unwrap_or(0)
}

/// Un sort sans portée ou sans règles de lancer n'en reçoit pas : le code ne
/// s'accorde pas sur ce que « rien » veut dire (sans règles, KrozTrap exige une
/// case sans piège, KrozZone non). Aucun sort qu'un objet modifie n'est dans ce
/// cas (`chaque_modificateur_trouve_son_sort`).
fn sur_le_sort(sort: &mut SpellDef, m: &SpellModifier) {
    let v = m.value;
    match m.kind {
        K::ApCost => sort.ap_cost.base = plus(sort.ap_cost.base, v),
        K::RangeMax => sort.range = sort.range.map(|(a, b)| (a, plus(b, v))),
        K::RangeMin => sort.range = sort.range.map(|(a, b)| (plus(a, v), b)),
        K::RangeBoostable => sort.cast.iter_mut().for_each(|c| c.range_boostable = true),
        K::NoCastInLine => sort.cast.iter_mut().for_each(|c| c.in_line = false),
        K::NoLineOfSight => sort.cast.iter_mut().for_each(|c| c.needs_line_of_sight = false),
        K::NoTakenCell => sort.cast.iter_mut().for_each(|c| c.needs_taken_cell = false),
        // L'intervalle entre deux lancers : à 1 comme à 0, le sort revient au
        // tour suivant.
        K::Cooldown => sort.cooldown_turns = plus(sort.cooldown_turns, v),
        K::CriticalRate => {
            if let Maybe::Known(base) = sort.crit.base_rate {
                sort.crit.base_rate = Maybe::Known(plus(base, v));
            }
        }
        // Zéro veut dire sans limite, et un lancer de plus n'y change rien.
        K::CastsPerTurn => {
            if sort.casts_per_turn > 0 {
                sort.casts_per_turn = plus(sort.casts_per_turn, v);
            }
        }
        K::CastsPerTarget => {
            if let Some(t) = sort.casts_per_target.filter(|t| *t > 0) {
                sort.casts_per_target = Some(plus(t, v));
            }
        }
        K::BaseDamage => lignes_du_sort(sort).for_each(|l| {
            let decaler = |r: Maybe<(i32, i32)>| match r {
                Maybe::Known((a, b)) => Maybe::Known((a + v, b + v)),
                inconnue => inconnue,
            };
            l.normal = decaler(l.normal);
            l.critical = decaler(l.critical);
        }),
        K::Damage => lignes_du_sort(sort).for_each(|l| l.dommages_fixes += v),
    }
}

/// Les lignes que le LANCER du sort inflige, dans tous ses modes : ni les
/// attaques d'invocation, qui ont leur propre profil, ni les lignes en
/// pourcentage de vie, qui n'ont pas de jet, ni ce que pose le sort au sol.
fn lignes_du_sort(sort: &mut SpellDef) -> impl Iterator<Item = &mut LineDef> {
    sort.lines
        .iter_mut()
        .chain(sort.modes.iter_mut().flat_map(|m| m.lines.iter_mut()))
        .filter(|l| l.invocation.is_none() && l.percent_of_life.is_none())
}

fn sur_le_niveau(niveau: &mut Level, m: &SpellModifier) {
    let v = m.value;
    match m.kind {
        K::ApCost => niveau.ap_cost = niveau.ap_cost.map(|c| plus(c, v)),
        K::RangeMax => {
            if let Some([_, Some(max)]) = niveau.range.as_mut() {
                *max = plus(*max, v);
            }
        }
        K::RangeMin => {
            if let Some([Some(min), _]) = niveau.range.as_mut() {
                *min = plus(*min, v);
            }
        }
        K::RangeBoostable => niveau.cast.iter_mut().for_each(|c| c.range_boostable = true),
        K::NoCastInLine => niveau.cast.iter_mut().for_each(|c| c.in_line = false),
        K::NoLineOfSight => niveau.cast.iter_mut().for_each(|c| c.needs_line_of_sight = false),
        K::NoTakenCell => niveau.cast.iter_mut().for_each(|c| c.needs_taken_cell = false),
        K::Cooldown => niveau.min_cast_interval = niveau.min_cast_interval.map(|r| plus(r, v)),
        K::CriticalRate => niveau.base_crit_percent = niveau.base_crit_percent.map(|c| plus(c, v)),
        K::CastsPerTurn => niveau.max_cast_per_turn = niveau.max_cast_per_turn.map(|n| if n > 0 { plus(n, v) } else { n }),
        K::CastsPerTarget => {
            niveau.max_cast_per_target = niveau.max_cast_per_target.map(|n| if n > 0 { plus(n, v) } else { n });
        }
        K::BaseDamage | K::Damage => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solve::{load_ruleset, resolve_build, snapshot_for};
    use dofus_build::{BuildInput, Catalogue};

    fn porte(classe: u32, objets: &[u32]) -> Resolved {
        let mut build = BuildInput { class: classe, level: 200, ..Default::default() };
        build.items = objets.to_vec();
        resolve_build(&build).expect("le build")
    }

    fn sort<'a>(regles: &'a Ruleset, id: &str) -> &'a SpellDef {
        regles.spells.iter().find(|s| s.id == id).unwrap_or_else(|| panic!("{id}"))
    }

    /// La Coiffe de Robbie Capuche : +6 dégâts de base à la Flèche
    /// Ralentissante, normale et critique, avant toute caractéristique.
    #[test]
    fn les_degats_de_base_decalent_le_jet() {
        let nu = load_ruleset(9).unwrap();
        let mut avec = load_ruleset(9).unwrap();
        sur_les_regles(&mut avec, &porte(9, &[8636]));
        let (avant, apres) = (sort(&nu, "fleche_ralentissante"), sort(&avec, "fleche_ralentissante"));
        let jets = |s: &SpellDef| (s.lines[0].normal, s.lines[0].critical);
        let ((n0, c0), (n1, c1)) = (jets(avant), jets(apres));
        let (Maybe::Known(n0), Maybe::Known(c0), Maybe::Known(n1), Maybe::Known(c1)) = (n0, c0, n1, c1) else {
            panic!("fourchettes inconnues");
        };
        assert_eq!(n1, (n0.0 + 6, n0.1 + 6));
        assert_eq!(c1, (c0.0 + 6, c0.1 + 6));
    }

    /// Le Chapeau Leufère : +35 % de critique sur Lapement, un lancer de plus
    /// par cible sur Réflexes, sa variante.
    #[test]
    fn le_critique_et_les_lancers_par_cible_montent() {
        let nu = load_ruleset(6).unwrap();
        let mut avec = load_ruleset(6).unwrap();
        sur_les_regles(&mut avec, &porte(6, &[8628]));
        let taux = |r: &Ruleset| sort(r, "lapement").crit.base_rate.known().copied();
        assert!(taux(&nu).is_some(), "un taux inconnu ferait passer le test à vide");
        assert_eq!(taux(&avec), taux(&nu).map(|t| t + 35));
        let cible = |r: &Ruleset| sort(r, "reflexes").casts_per_target;
        assert!(cible(&nu).is_some_and(|t| t > 0), "sans limite par cible, le test passerait à vide");
        assert_eq!(cible(&avec), cible(&nu).map(|t| t + 1));
    }

    /// Le Casque Keutumedi : Agitation coûte 1 PA de moins, 1 au lieu de 2. Ce
    /// « -1 PA » n'est que dans les effets possibles de l'objet.
    #[test]
    fn le_cout_en_pa_baisse() {
        let nu = load_ruleset(8).unwrap();
        let mut avec = load_ruleset(8).unwrap();
        sur_les_regles(&mut avec, &porte(8, &[8619]));
        assert_eq!(sort(&nu, "agitation").ap_cost.base, 2);
        assert_eq!(sort(&avec, "agitation").ap_cost.base, 1);
    }

    /// Un objet d'une autre classe ne change rien : le Chapeau Leufère de
    /// l'Ecaflip sur un Iop.
    #[test]
    fn l_objet_d_une_autre_classe_ne_change_rien() {
        let nu = load_ruleset(8).unwrap();
        let mut avec = load_ruleset(8).unwrap();
        sur_les_regles(&mut avec, &porte(8, &[8628]));
        assert_eq!(format!("{:?}", nu.spells), format!("{:?}", avec.spells));
    }

    /// Chaque modificateur du catalogue trouve son sort, et le champ qu'il
    /// change existe : un sort sans portée ou sans règles de lancer n'en
    /// recevrait pas. Seuls quatorze sorts du Xélor qui ne frappent pas sont
    /// hors des règles.
    #[test]
    fn chaque_modificateur_trouve_son_sort() {
        let catalogue = Catalogue::from_json(crate::donnees::OBJETS).unwrap();
        let sorts: std::collections::BTreeMap<u32, SpellDef> = crate::solve::CLASSES
            .iter()
            .flat_map(|(id, _, _)| load_ruleset(*id).unwrap().spells)
            .filter_map(|s| Some((s.dofusdb_id?, s)))
            .collect();
        let mut hors_regles = std::collections::BTreeSet::new();
        let mut vus = 0;
        for objet in &catalogue.items {
            for m in &objet.spell_modifiers {
                vus += 1;
                let Some(s) = sorts.get(&m.spell) else {
                    hors_regles.insert(m.spell);
                    continue;
                };
                match m.kind {
                    K::RangeMax | K::RangeMin => assert!(s.range.is_some(), "{} : {}", objet.name, s.id),
                    K::RangeBoostable | K::NoCastInLine | K::NoLineOfSight | K::NoTakenCell => {
                        assert!(s.cast.is_some(), "{} : {}", objet.name, s.id);
                    }
                    K::CriticalRate => assert!(s.crit.base_rate.known().is_some() && s.crit.can_crit, "{}", s.id),
                    _ => {}
                }
            }
        }
        assert_eq!(vus, 760);
        // Quatorze sorts du Xélor qui ne frappent pas, de Rembobinage à
        // Espace-temps : aucune ligne de dégâts.
        assert_eq!(
            hors_regles.into_iter().collect::<Vec<_>>(),
            [13243, 13246, 13247, 13249, 13250, 13255, 13258, 13282, 13287, 13291, 13293, 13295, 13296, 13297]
        );
    }

    /// L'instantané que lit KrozTrap prend les portées et les règles de
    /// lancer : l'Anneau Hell rend le Piège Mortel lançable hors ligne et donne
    /// 3 de portée au Piège Insidieux.
    #[test]
    fn kroztrap_voit_les_portees_et_le_lancer_en_ligne() {
        let mut instantane = snapshot_for(4).unwrap();
        sur_l_instantane(&mut instantane, &porte(4, &[8722]));
        let niveau = |id: u32| instantane.spells.iter().find(|s| s.id == id).and_then(|s| s.levels.last()).unwrap();
        assert!(!niveau(12921).cast.unwrap().in_line, "Piège Mortel");
        assert_eq!(niveau(12918).range, Some([Some(1), Some(9)]), "Piège Insidieux");
    }

    /// Aucun objet ne change les dégâts d'un sort du Sram : l'instantané de
    /// KrozTrap peut donc s'en passer.
    #[test]
    fn les_degats_des_pieges_ne_changent_pas() {
        let catalogue = Catalogue::from_json(crate::donnees::OBJETS).unwrap();
        let sram: Vec<u32> = snapshot_for(4).unwrap().spells.iter().map(|s| s.id).collect();
        let fautifs: Vec<&str> = catalogue
            .items
            .iter()
            .filter(|o| {
                o.spell_modifiers
                    .iter()
                    .any(|m| sram.contains(&m.spell) && matches!(m.kind, K::BaseDamage | K::Damage))
            })
            .map(|o| o.name.as_str())
            .collect();
        assert!(fautifs.is_empty(), "{fautifs:?}");
    }
}
