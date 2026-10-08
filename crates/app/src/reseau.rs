//! Les réseaux de pièges du Sram : ce qu'une pose couvre, ce qu'elle coûte, et
//! ce que déclenche l'entrée d'un ennemi.
//!
//! Le module ne prévoit pas le tour de l'adversaire : le joueur dit où l'ennemi
//! entre, ou quel sort du Sram l'y fait entrer, et la chaîne suit pièges,
//! poussées et attirances. Il propose aussi un tour de poses, et conçoit un
//! réseau sur plusieurs tours.
//!
//! # Ce que la donnée du jeu établit
//!
//! * deux zones par piège : celle de l'effet de pose, où il faut marcher pour
//!   qu'il parte, et celle du sous-sort posé, qui dit qui prend les dégâts ;
//! * ses dégâts, son coût en PA, sa portée, ses lancers par tour et son
//!   intervalle minimal ;
//! * `needs_free_trap_cell`, vrai sur les quatorze poses de la classe : deux
//!   pièges ne partagent jamais leur case centrale. Leurs zones, elles, se
//!   recouvrent librement.

use crate::cartes::Plateau;
use crate::solve::{load_ruleset, resolve_build, snapshot_for};
use dofus_build::BuildInput;
use dofus_damage::{CastContext, DamageProfile, Delivery, Element, FinalMultiplier, Reach, Resistance, SpellLine};
use dofus_engine::Case;
use dofus_ruleset::snapshot::{Level, Spell as SpellSnap};

/// Un piège posé : le sort, la case où il tombe, et le tour où il est lancé.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PoseInput {
    pub sort: String,
    pub case: (i16, i16),
    /// ⚠️ LES LANCERS ET LES PA SE COMPTENT PAR TOUR. Un réseau se construit
    /// sur plusieurs tours : deux Sournois posés à deux tours d'écart sont
    /// permis, pas dans le même.
    #[serde(default = "premier_tour")]
    pub tour: u8,
}

fn premier_tour() -> u8 {
    1
}

/// Ce que le concepteur cherche, au choix du joueur. Les deux visent le plus
/// de dégâts ; ils diffèrent par le temps qu'on se donne.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Objectif {
    /// « Sur plusieurs tours (OS) » : le plus gros réseau posé en un à dix
    /// tours, l'ennemi amené ensuite sur la case d'entrée.
    Frappe,
    /// « En un tour » : les pièges et le sort qui fait entrer l'ennemi, dans
    /// les PA d'un seul tour, pour le plus de dégâts.
    UnTour,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RequeteReseau {
    #[serde(flatten)]
    pub build: BuildInput,
    #[serde(default)]
    pub poses: Vec<PoseInput>,
    /// Où se tient le Sram, pour juger la portée de chaque pose.
    #[serde(default)]
    pub lanceur: Option<(i16, i16)>,
    /// La case par laquelle un ennemi entre dans le réseau. Sans elle, la
    /// réponse s'arrête à la couverture et ne déroule aucune chaîne.
    #[serde(default)]
    pub entree: Option<(i16, i16)>,
    /// Les cases infranchissables : murs et décor.
    #[serde(default)]
    pub obstacles: Vec<(i16, i16)>,
    /// Les alliés du Sram sur le damier. Ses pièges les déplacent, et certains
    /// les frappent.
    #[serde(default)]
    pub allies: Vec<(i16, i16)>,
    /// Les ennemis autres que celui qui entre dans le réseau. Ils attendent
    /// qu'un piège les touche.
    #[serde(default)]
    pub ennemis: Vec<(i16, i16)>,
    #[serde(default)]
    pub ordre: Option<Ordre>,
    /// La cible porte la Concentration de Chakra : chaque dégât de piège
    /// qu'elle subit déclenche en plus un vol de vie dans le meilleur élément
    /// du lanceur.
    #[serde(default)]
    pub chakra: bool,
    /// Les PA dont le Sram dispose à chaque tour. Absents, aucune pose n'est
    /// refusée faute de PA.
    #[serde(default)]
    pub pa: Option<u8>,
    /// Le tour en cours, celui que le générateur complète. Absent, le dernier
    /// tour des poses, ou le premier.
    #[serde(default)]
    pub tour: Option<u8>,
    /// Demande au générateur de compléter le tour en cours : le plus de
    /// dégâts avec les PA qui lui restent.
    #[serde(default)]
    pub proposer: bool,
    /// Les cases des pièges posés à un tour d'après celui-ci : pas encore au
    /// sol, mais leur case est prise, et le générateur ne la propose pas.
    #[serde(default)]
    pub reservees: Vec<(i16, i16)>,
    /// La carte de boss, par son identifiant de relevé ; absente, le damier
    /// vide. Ses murs et ses trous arrêtent les déplacements, et un piège ne se
    /// pose que sur son sol : le générateur cherche donc sur cette carte.
    #[serde(default)]
    pub carte: Option<u16>,
    /// Le côté du damier vide. Il ne borne rien ici : le damier de KrozTrap
    /// reste un terrain ouvert, comme avant les cartes.
    #[serde(default)]
    pub damier: Option<u8>,
    /// Le sort qui fait entrer l'ennemi dans le réseau, lancé par le Sram à la
    /// fin du dernier tour. Présent, l'ennemi part de sa case et `entree` ne
    /// sert pas.
    #[serde(default)]
    pub declencheur: Option<DeclencheurInput>,
    /// Demande au concepteur un réseau entier, depuis la case `entree` : les
    /// poses tour par tour, et pour un réseau en un tour le sort qui fait
    /// entrer l'ennemi.
    #[serde(default)]
    pub concevoir: Option<Conception>,
}

/// Le sort qui fait entrer l'ennemi dans le réseau, et la case où l'ennemi se
/// tient avant.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DeclencheurInput {
    /// `sournoiserie`, `perquisition`, `guet_apens`, `meprise` ou `peur`.
    pub sort: String,
    pub depart: (i16, i16),
    /// La case que vise Peur : l'ennemi au contact du Sram y est poussé. Les
    /// autres sorts visent l'ennemi lui-même.
    #[serde(default)]
    pub vise: Option<(i16, i16)>,
}

/// Les sorts du Sram qui déplacent l'ennemi, et donc le font entrer dans un
/// réseau : Sournoiserie et Perquisition le repoussent, Guet-apens l'attire,
/// Méprise échange sa place avec celle du Sram, et Peur, sa variante, pousse
/// l'ennemi au contact du Sram jusqu'à la case visée (effet 783). Double n'y
/// est pas : il n'échange sa place qu'avec le Sram.
pub const DECLENCHEURS: [&str; 5] = ["sournoiserie", "perquisition", "guet_apens", "meprise", "peur"];

/// Un sort de déclenchement, lu dans la donnée, et ce qu'il inflige sur ce
/// build.
#[derive(Debug, Clone)]
struct Declencheur {
    sort: String,
    nom: String,
    dofusdb_id: u32,
    pa: u32,
    portee: (u16, u16),
    en_ligne: bool,
    vue: bool,
    mouvement: Mouvement,
    coup: Portees,
}

impl Declencheur {
    /// Lu dans la donnée : son coût, sa portée, s'il se lance en ligne et à
    /// vue, son déplacement (effets 5, 6 et 8, hors de la variante qui vise
    /// le double du Comploteur, `*E6052`) et ses dégâts sur ce build.
    fn de(
        ruleset: &dofus_ruleset::Ruleset,
        snap: &dofus_ruleset::snapshot::Snapshot,
        sort: &str,
        profile: &DamageProfile,
        distance: FinalMultiplier,
        melee: FinalMultiplier,
    ) -> Option<Declencheur> {
        let def = ruleset.spells.iter().find(|s| s.id == sort)?;
        let id = def.dofusdb_id?;
        let sp = snap.spells.iter().find(|s| s.id == id)?;
        let niveau = sp.levels.last()?;
        let mouvement = niveau
            .other_effects
            .iter()
            .filter(|e| !e.target.as_deref().unwrap_or("").contains("*E6052"))
            .find_map(|e| match e.id {
                5 => Some(Mouvement::Pousse(e.dice_num.unwrap_or(0))),
                6 => Some(Mouvement::Attire(e.dice_num.unwrap_or(0))),
                8 => Some(Mouvement::Echange),
                // « Pousse jusqu'à la case ciblée » : la distance vient de la
                // case visée, au lancer.
                783 => Some(Mouvement::JusquA(0)),
                _ => None,
            })?;
        let regles = niveau.cast.as_ref();
        let coup = |m: FinalMultiplier| {
            niveau.normal_lines.iter().fold((0i64, 0i64), |(a, b), l| {
                let ligne = SpellLine {
                    element: element_de(&l.element),
                    normal: l.range,
                    critical: l.range,
                };
                let (x, y) = dofus_damage::range(&ligne, profile, m, false, &Resistance::NONE);
                (a + x, b + y)
            })
        };
        Some(Declencheur {
            sort: def.id.clone(),
            nom: def.name.fr.clone(),
            dofusdb_id: id,
            pa: u32::from(niveau.ap_cost.unwrap_or(0)),
            portee: niveau
                .range
                .and_then(|[a, b]| Some((u16::from(a?), u16::from(b?))))
                .unwrap_or((0, 0)),
            en_ligne: regles.is_some_and(|r| r.in_line),
            vue: regles.is_none_or(|r| r.needs_line_of_sight),
            mouvement,
            coup: Portees {
                distance: coup(distance),
                melee: coup(melee),
            },
        })
    }

    /// Pourquoi le jeu refuserait ce lancer du Sram en `lanceur` sur l'ennemi
    /// en `depart`. `corps` : les autres entités, qui coupent la vue ;
    /// `tenues` : ce qui occupe une case, obstacles compris. `vise` : la case
    /// que vise Peur.
    fn refus(
        &self,
        lanceur: Case,
        depart: Case,
        vise: Option<Case>,
        corps: &[Case],
        tenues: &[Case],
        plateau: &Plateau,
    ) -> Option<&'static str> {
        if !plateau.est_sol(depart) {
            return Some("l'ennemi doit se tenir sur le sol");
        }
        // Peur vise une case, pas l'ennemi : la case libre où il le pousse,
        // en ligne et à portée, l'ennemi se tenant au contact du Sram de ce
        // côté-là. Elle ne demande pas de vue, la donnée le dit.
        if matches!(self.mouvement, Mouvement::JusquA(_)) {
            let Some(v) = vise else {
                return Some("visez la case où pousser l'ennemi");
            };
            let d = lanceur.distance(v);
            let meme_sens = |a: i16, b: i16| (a - lanceur.x).signum() == (b - lanceur.x).signum();
            let meme_sens_y = |a: i16, b: i16| (a - lanceur.y).signum() == (b - lanceur.y).signum();
            return if lanceur.distance(depart) != 1 {
                Some("l'ennemi doit se tenir au contact du Sram")
            } else if !plateau.est_sol(v) || tenues.contains(&v) || v == depart || v == lanceur {
                Some("la case visée doit être libre")
            } else if d < self.portee.0 || d > self.portee.1 {
                Some("la case visée est hors de portée du sort")
            } else if !((v.x == lanceur.x && depart.x == lanceur.x && meme_sens_y(v.y, depart.y))
                || (v.y == lanceur.y && depart.y == lanceur.y && meme_sens(v.x, depart.x)))
            {
                Some("le sort se lance en ligne, du côté de l'ennemi")
            } else {
                None
            };
        }
        let d = lanceur.distance(depart);
        if d < self.portee.0 || d > self.portee.1 {
            Some("l'ennemi est hors de portée du sort")
        } else if self.en_ligne && depart.x != lanceur.x && depart.y != lanceur.y {
            Some("le sort se lance en ligne")
        } else if self.vue && !plateau.en_vue(lanceur, depart, corps) {
            Some("le Sram ne voit pas l'ennemi")
        } else {
            None
        }
    }
}

/// Ce que chaque case reçoit : le rang du piège et les dégâts de la ligne qui
/// la couvre. Une case peut apparaître plusieurs fois pour un même piège quand
/// deux de ses lignes se recouvrent, ce qui est voulu.
type Couverture = std::collections::BTreeMap<(i16, i16), Vec<(usize, (i64, i64))>>;

/// Jusqu'où le tableau de couverture s'étend autour des pièges posés.
const RAYON_RENDU: u16 = 9;

/// La zone de déclenchement d'un piège et ses dégâts, tels que la donnée les
/// porte.
struct Piege<'a> {
    nom: &'a str,
    niveau: &'a Level,
    /// Le texte du jeu, seul endroit qui dise si le piège est mono-cellule.
    texte: &'a str,
    /// La paire de variantes du sort. ⚠️ LE JEU N'EN LAISSE ÉQUIPER QU'UNE, et
    /// on ne change pas de variante en combat : le Répulsif et l'Effroyable, le
    /// Scélérat et la Fragmentation, le Mortel et la Calamité, l'Insidieux et la
    /// Dérive ne se posent jamais dans un même réseau.
    variante: Option<u32>,
}

fn piege<'a>(snap: &'a dofus_ruleset::snapshot::Snapshot, dofusdb_id: u32) -> Option<Piege<'a>> {
    let spell: &SpellSnap = snap.spells.iter().find(|s| s.id == dofusdb_id)?;
    // Le grade modélisé est le plus haut, comme partout ailleurs dans le parc.
    let niveau = spell.levels.iter().max_by_key(|l| l.grade.unwrap_or(0))?;
    if niveau.placed_lines.is_empty() {
        return None;
    }
    Some(Piege {
        nom: spell.name.fr.as_str(),
        niveau,
        texte: spell.description_fr.as_deref().unwrap_or(""),
        variante: spell.variant_group,
    })
}

fn element_de(nom: &str) -> Element {
    match nom {
        "fire" => Element::Fire,
        "earth" => Element::Earth,
        "air" => Element::Air,
        "water" => Element::Water,
        _ => Element::Neutral,
    }
}

/// Les cases où un ennemi déclenche le piège, qui ne sont pas celles où il
/// frappe : la zone de l'effet de pose dit où il faut marcher, celle du
/// sous-sort posé qui prend les dégâts. Sept pièges du Sram déclenchent sur une
/// seule case et frappent sur cinq, treize ou vingt-cinq.
fn declenchement(niveau: &Level, centre: Case) -> Option<Vec<Case>> {
    let z = niveau.placed_trigger_zone?;
    dofus_grid::cases_de_zone(
        z.shape,
        u16::from(z.size.unwrap_or(0)),
        u16::from(z.size2),
        centre,
        (1, 0),
    )
}

/// Ce qu'un piège déplace : le sens et la distance, quand il en déplace un.
/// L'effet 5 pousse, le 6 attire, et la distance est dans `dice_num` ; six
/// pièges du Sram déplacent.
fn deplacement(niveau: &Level) -> Option<(&'static str, i32)> {
    niveau.placed_effects.iter().find_map(|e| match e.id {
        5 => Some(("pousse", e.dice_num.unwrap_or(0))),
        6 => Some(("attire", e.dice_num.unwrap_or(0))),
        _ => None,
    })
}

/// Le déplacement d'un piège posé en `centre`, avec sa zone et son masque.
/// Sans zone dans la donnée, on retombe sur le déclenchement, qui déplace au
/// moins celui qui a fait partir le piège ; les six pièges du Sram qui
/// déplacent portent tous leur zone.
fn deplacement_pose(niveau: &Level, centre: Case, declenchement: &[Case]) -> Option<Deplacement> {
    let e = niveau.placed_effects.iter().find(|e| e.id == 5 || e.id == 6)?;
    let zone = e
        .zone
        .and_then(|z| {
            dofus_grid::cases_de_zone(
                z.shape,
                u16::from(z.size.unwrap_or(0)),
                u16::from(z.size2),
                centre,
                (1, 0),
            )
        })
        .unwrap_or_else(|| declenchement.to_vec());
    Some(Deplacement {
        sens: if e.id == 5 { 1 } else { -1 },
        distance: e.dice_num.unwrap_or(0),
        zone,
        masque: e.target.clone().unwrap_or_default(),
    })
}

/// Les pièges dont une ligne de dégâts est un poison de fin de tour, et non un
/// coup au déclenchement : le sort, et combien de ses lignes le sont. Le Piège
/// Insidieux (12918) porte deux lignes Air 8-9, et son texte dit pourquoi :
/// « applique un poison Air de fin de tour sur les ennemis, leur occasionne des
/// dommages Air ». Déclaré plutôt que déduit : l'effet 3793 qui précède ces
/// lignes a d'autres sens ailleurs dans le parc.
const LIGNES_DE_POISON: &[(u32, usize)] = &[(12918, 1)];

/// Sépare les lignes qui frappent au déclenchement de celles qui empoisonnent
/// en fin de tour. Le poison ne tombe pas pendant la chaîne : il se montre à
/// part et n'entre pas dans son total.
fn separer_poison(mut lignes: Vec<LignePiege>, sort: u32) -> (Vec<LignePiege>, Vec<LignePiege>) {
    let combien = LIGNES_DE_POISON
        .iter()
        .find(|(id, _)| *id == sort)
        .map_or(0, |(_, n)| *n);
    let mut poison = Vec::new();
    for _ in 0..combien {
        // Les deux lignes sont identiques : on retire la dernière de la paire.
        if let Some(i) = lignes.iter().rposition(|l| {
            lignes
                .iter()
                .filter(|m| m.degats == l.degats && m.signature == l.signature && m.masque == l.masque)
                .count()
                > 1
        }) {
            poison.push(lignes.remove(i));
        }
    }
    (lignes, poison)
}

/// La somme d'une liste de lignes à distance, ou rien si elle est vide.
fn somme(lignes: &[LignePiege]) -> Option<(i64, i64)> {
    (!lignes.is_empty()).then(|| {
        lignes
            .iter()
            .fold((0i64, 0i64), |(a, b), l| (a + l.degats.0, b + l.degats.1))
    })
}

/// Le piège déplace-t-il avant de frapper ?
///
/// La donnée range les effets posés dans l'ordre où le jeu les applique : le
/// Répulsif porte la poussée (5) puis les dégâts Air (98), le Scélérat
/// l'attirance (6) puis les dégâts Eau (96), le Sournois les dégâts Feu (99)
/// puis l'attirance. On compare le rang du premier déplacement à celui du
/// premier dégât élémentaire.
fn deplace_avant(niveau: &Level) -> bool {
    let rang = |garder: &dyn Fn(u32) -> bool| niveau.placed_effects.iter().position(|e| garder(e.id));
    let deplacement = rang(&|id| id == 5 || id == 6);
    let degats = rang(&|id| (96..=100).contains(&id));
    match (deplacement, degats) {
        (Some(d), Some(g)) => d < g,
        _ => false,
    }
}

/// Une ligne d'un piège : sa zone propre, ses dégâts, et son masque de cible.
struct LignePiege {
    element: Element,
    /// Ce que la ligne rend quand la cible n'est pas au contact du Sram : les
    /// `% Dommages distance` s'y appliquent. C'est la valeur des tableaux.
    degats: (i64, i64),
    /// Et au contact, avec les `% Dommages mêlée` : la chaîne choisit l'une
    /// ou l'autre au moment du coup.
    degats_melee: (i64, i64),
    cases: Option<Vec<Case>>,
    signature: String,
    masque: String,
    /// La variante écartée, quand deux lignes de même zone se disputaient la
    /// place. Rendue et non perdue : le joueur doit voir ce qui l'attend quand
    /// la condition bascule.
    ecartee: Option<(i64, i64)>,
}

impl LignePiege {
    fn vide() -> Self {
        LignePiege {
            element: Element::Neutral,
            degats: (0, 0),
            degats_melee: (0, 0),
            cases: None,
            signature: String::new(),
            masque: String::new(),
            ecartee: None,
        }
    }
}

/// Les lignes d'un piège, chacune avec sa zone, passées par le pipeline complet.
///
/// Chaque ligne a sa propre zone : le Piège à Fragmentation porte quatre lignes
/// de zones `P`, `O1`, `O2` et `O3`, le centre puis trois anneaux, et une case
/// n'appartient qu'à l'un d'eux. Sans coup critique : le taux qui s'y applique
/// est celui du lanceur au moment de la pose, que ce module ne suit pas ; la
/// fourchette normale sous-estime donc légèrement.
fn lignes_du_piege(
    niveau: &Level,
    centre: Case,
    profile: &DamageProfile,
    distance: FinalMultiplier,
    melee: FinalMultiplier,
) -> Vec<LignePiege> {
    let brutes: Vec<LignePiege> = niveau
        .placed_lines
        .iter()
        .map(|l| {
            let e = element_de(&l.element);
            let ligne = SpellLine {
                element: e,
                normal: (l.range.0, l.range.1),
                critical: (l.range.0, l.range.1),
            };
            let cases = l.zone.and_then(|z| {
                dofus_grid::cases_de_zone(
                    z.shape,
                    u16::from(z.size.unwrap_or(0)),
                    u16::from(z.size2),
                    centre,
                    (1, 0),
                )
            });
            LignePiege {
                element: e,
                degats: dofus_damage::range(&ligne, profile, distance, false, &Resistance::NONE),
                degats_melee: dofus_damage::range(&ligne, profile, melee, false, &Resistance::NONE),
                cases,
                signature: l.zone.map_or_else(
                    || "-".into(),
                    |z| format!("{}{}", z.shape, z.size.unwrap_or(0)),
                ),
                masque: l.target.clone().unwrap_or_default(),
                ecartee: None,
            }
        })
        .collect();

    // Deux lignes de même zone et de masques différents sont des
    // variantes, pas deux coups : le Piège Mortel en porte deux, `a,A,v50`
    // et `a,A,V50`. Le texte du sort dit laquelle est laquelle : « Les
    // dommages sont plus importants sur les entités ayant moins de 50% de
    // leurs points de vie. » La plus basse, celle d'une cible pleine, est
    // retenue ; l'autre est rendue à part, avec la phrase du jeu.
    let mut retenues: Vec<LignePiege> = Vec::new();
    for ligne in brutes {
        match retenues
            .iter_mut()
            .find(|r| r.signature == ligne.signature && r.masque != ligne.masque)
        {
            Some(deja) => {
                let (basse, haute) = if ligne.degats.1 < deja.degats.1 {
                    (ligne, std::mem::replace(deja, LignePiege::vide()))
                } else {
                    (std::mem::replace(deja, LignePiege::vide()), ligne)
                };
                *deja = basse;
                deja.ecartee = Some(haute.degats);
            }
            None => retenues.push(ligne),
        }
    }
    retenues
}

/// Les lignes d'un piège qui frappent l'ennemi qui le déclenche seul, debout
/// sur son centre, en fourchettes de base : celles dont la zone couvre le
/// centre, une variante par zone (la plus basse, celle d'une cible pleine de
/// vie), sans les anneaux qui l'entourent, ses poisons de fin de tour à part.
/// C'est ce que la Rotation compte quand le joueur déclare que la cible
/// déclenche ses pièges ; les chaînes de plusieurs pièges restent à KrozTrap.
pub fn lignes_au_centre(
    snap: &dofus_ruleset::snapshot::Snapshot,
    dofusdb_id: u32,
) -> Option<(Vec<(Element, (i64, i64))>, Vec<(Element, (i64, i64))>)> {
    let pg = piege(snap, dofusdb_id)?;
    let centre = Case::new(0, 0);
    // Un profil nu et des multiplicateurs neutres : les dégâts rendus sont les
    // fourchettes de base, que le moteur montera lui-même.
    let lignes = lignes_du_piege(
        pg.niveau,
        centre,
        &DamageProfile::default(),
        FinalMultiplier::NEUTRAL,
        FinalMultiplier::NEUTRAL,
    );
    let (frappent, poisons) = separer_poison(lignes, dofusdb_id);
    let au_centre = |l: &LignePiege| l.cases.as_ref().is_none_or(|c| c.contains(&centre));
    let brutes = |v: Vec<LignePiege>| -> Vec<(Element, (i64, i64))> {
        v.into_iter().filter(|l| au_centre(l)).map(|l| (l.element, l.degats)).collect()
    };
    Some((brutes(frappent), brutes(poisons)))
}

/// Ce que le jeu exige d'une pose, lu dans la donnée du sort : la même règle
/// pour la main et pour le générateur.
#[derive(Debug, Clone, Copy)]
struct Exigences {
    pa: u32,
    portee: Option<(u16, u16)>,
    /// Zéro : pas de limite.
    lancers_par_tour: u8,
    /// La case doit être alignée avec le Sram, sur l'un des deux axes du
    /// repère du jeu. Le Piège Mortel l'exige.
    en_ligne: bool,
    /// Ni entité ni obstacle sur la case : tous les pièges du Sram l'exigent.
    case_libre: bool,
    /// Pas d'autre piège sur la case.
    sans_piege: bool,
}

impl Exigences {
    fn de(niveau: &Level) -> Self {
        let regles = niveau.cast.as_ref();
        Exigences {
            pa: u32::from(niveau.ap_cost.unwrap_or(0)),
            portee: niveau
                .range
                .and_then(|[a, b]| Some((u16::from(a?), u16::from(b?)))),
            lancers_par_tour: niveau.max_cast_per_turn.unwrap_or(0),
            en_ligne: regles.is_some_and(|r| r.in_line),
            case_libre: regles.is_some_and(|r| r.needs_free_cell),
            // Tous les pièges du Sram le portent ; sans la donnée, on garde la
            // règle qui s'appliquait avant qu'elle soit lue.
            sans_piege: regles.is_none_or(|r| r.needs_free_trap_cell),
        }
    }

    /// Pourquoi le jeu refuserait cette pose, s'il la refuse.
    ///
    /// `pieges` : les cases qui tiennent déjà un piège. `prises` : celles où se
    /// tient une entité ou un obstacle au moment de poser, l'entrée exceptée,
    /// puisque l'ennemi n'y arrive qu'ensuite. `lances` et `depenses` : ce que
    /// le tour a déjà lancé de ce sort et dépensé en PA.
    ///
    /// Sur une carte, un piège ne se pose que sur son sol ; le damier vide ne
    /// borne pas les poses. La ligne de vue n'est pas jugée : aucun des onze
    /// pièges du Sram ne l'exige. Sans Sram sur le damier, ni la portée ni
    /// l'alignement ne se jugent.
    #[allow(clippy::too_many_arguments)]
    fn refus(
        &self,
        case: Case,
        lanceur: Option<Case>,
        pieges: &[Case],
        prises: &[Case],
        lances: u8,
        depenses: u32,
        budget: Option<u32>,
        plateau: &Plateau,
    ) -> Option<&'static str> {
        let hors_de_portee = lanceur.is_some_and(|l| {
            let distance = l.distance(case);
            self.portee.is_some_and(|(min, max)| distance < min || distance > max)
        });
        if matches!(plateau, Plateau::Carte(_)) && !plateau.est_sol(case) {
            Some("la case n'est pas du sol : un mur ou un trou")
        } else if self.sans_piege && pieges.contains(&case) {
            Some("deux pièges ne partagent pas leur case")
        } else if self.case_libre && prises.contains(&case) {
            Some("la case doit être libre")
        } else if hors_de_portee {
            Some("hors de la portée du sort")
        } else if self.en_ligne && lanceur.is_some_and(|l| case.x != l.x && case.y != l.y) {
            Some("se lance en ligne")
        } else if self.lancers_par_tour > 0 && lances >= self.lancers_par_tour {
            Some("plus de lancers que le tour n'en permet")
        } else if budget.is_some_and(|b| depenses + self.pa > b) {
            Some("plus de PA que le tour n'en a")
        } else {
            None
        }
    }
}

/// Un piège prêt pour la chaîne, posé en `centre`.
fn piege_au_sol(
    pg: &Piege,
    id: u32,
    rang: usize,
    centre: Case,
    profile: &DamageProfile,
    distance: FinalMultiplier,
    melee: FinalMultiplier,
) -> PiegePose {
    let (lignes, poison) = separer_poison(lignes_du_piege(pg.niveau, centre, profile, distance, melee), id);
    // Où il faut poser le pied pour que le piège parte. La donnée le dit
    // depuis la passe `poses-detail` ; à défaut, on retombe sur la case du
    // piège, qui est le cas le plus restrictif et jamais une surestimation.
    let declenchement = declenchement(pg.niveau, centre).unwrap_or_else(|| vec![centre]);
    PiegePose {
        rang,
        nom: pg.nom.to_string(),
        centre,
        effets: lignes
            .iter()
            .filter_map(|l| {
                l.cases.clone().map(|zone| LigneDeDegats {
                    zone,
                    degats: Portees {
                        distance: l.degats,
                        melee: l.degats_melee,
                    },
                    masque: l.masque.clone(),
                })
            })
            .collect(),
        deplacement: deplacement_pose(pg.niveau, centre, &declenchement),
        deplace_avant: deplace_avant(pg.niveau),
        poison: somme(&poison),
        declenchement,
    }
}

/// Un piège de la palette tel que le générateur le pose partout : lu une fois,
/// posé en (0,0), puis déplacé sur chaque case essayée. Ses zones ont toutes
/// une forme symétrique, qui ne dépend pas de la direction du lancer.
struct Modele {
    sort: String,
    nom: String,
    dofusdb_id: u32,
    variante: Option<u32>,
    exigences: Exigences,
    gabarit: PiegePose,
}

impl Modele {
    fn poser(&self, rang: usize, centre: Case) -> PiegePose {
        let ici = |c: &Case| Case::new(c.x + centre.x, c.y + centre.y);
        let g = &self.gabarit;
        PiegePose {
            rang,
            nom: g.nom.clone(),
            centre,
            declenchement: g.declenchement.iter().map(ici).collect(),
            effets: g
                .effets
                .iter()
                .map(|l| LigneDeDegats {
                    zone: l.zone.iter().map(ici).collect(),
                    degats: l.degats,
                    masque: l.masque.clone(),
                })
                .collect(),
            deplacement: g.deplacement.as_ref().map(|d| Deplacement {
                sens: d.sens,
                distance: d.distance,
                zone: d.zone.iter().map(ici).collect(),
                masque: d.masque.clone(),
            }),
            deplace_avant: g.deplace_avant,
            poison: g.poison,
        }
    }
}

/// Ce qu'une chaîne rapporte : ses dégâts sur les ennemis, Chakra et
/// dommages de poussée compris.
fn total_ennemis(etapes: &[Etape], camps: &[Camp]) -> (i64, i64) {
    etapes.iter().fold((0, 0), |(a, b), e| match e {
        Etape::Degats { entite, degats, .. } if camps[*entite] == Camp::Ennemi => (a + degats.0, b + degats.1),
        Etape::Chakra { degats } => (a + degats.0, b + degats.1),
        Etape::Poussee { entite, degats, .. } if camps[*entite] == Camp::Ennemi => (a + degats, b + degats),
        Etape::Coup { degats } => (a + degats.0, b + degats.1),
        _ => (a, b),
    })
}

/// Les cases où une entité entre pendant la chaîne : l'entrée, puis chaque
/// case traversée par un déplacement. Un piège ne part que si sa zone de
/// déclenchement en couvre une : c'est là qu'une pose nouvelle change la
/// chaîne. Une pose qui ne compte qu'après une autre vient après elle dans la
/// recherche, et l'ordre de deux pièges qui ne partent pas ensemble ne change
/// rien.
fn cases_parcourues(arrivee: &Arrivee, etapes: &[Etape]) -> Vec<Case> {
    // Une cible amenée par un sort n'entre pas dans sa case de départ : elle
    // s'y tient déjà.
    let mut cases = match arrivee {
        Arrivee::Entree(entree) => vec![*entree],
        Arrivee::Sort { .. } => Vec::new(),
    };
    for e in etapes {
        if let Etape::Echange { cible, sram } = e {
            cases.push(*cible);
            cases.push(*sram);
        }
        if let Etape::Deplace { de, vers, .. } | Etape::Amene { de, vers, .. } = e {
            // Un pas va sur un axe ou en diagonale : avancer d'un cran sur
            // chaque écart non nul mène de l'un à l'autre.
            let (sx, sy) = ((vers.x - de.x).signum(), (vers.y - de.y).signum());
            let mut c = *de;
            while c != *vers && cases.len() < 4096 {
                c = Case::new(c.x + sx, c.y + sy);
                cases.push(c);
            }
        }
    }
    cases.sort_unstable();
    cases.dedup();
    cases
}

/// Ce que le générateur propose pour le tour en cours.
struct Proposition {
    /// Le modèle et la case, dans l'ordre de pose.
    poses: Vec<(usize, Case)>,
    pa: u32,
    avant: (i64, i64),
    total: (i64, i64),
    /// Combien de chaînes la recherche a déroulées.
    essais: usize,
}

/// Combien de réseaux partiels le générateur garde à chaque pose.
///
/// Une recherche par faisceau, pas exhaustive : à chaque pose, le générateur
/// ajoute un piège à chacun des meilleurs réseaux partiels, déroule la chaîne de
/// chaque candidat, et garde les meilleurs. Mesurée contre une largeur double
/// sur onze montages, celle-ci rend le même réseau sur dix et 3 % de moins sur
/// le onzième, en 90 à 540 ms par proposition.
const LARGEUR_DU_FAISCEAU: usize = 1600;

/// Le terrain d'une recherche : ce qui est au sol et ce que le tour permet
/// encore.
struct Recherche<'a> {
    modeles: &'a [Modele],
    au_sol: &'a [PiegePose],
    entree: Case,
    terrain: &'a Terrain<'a>,
    /// Les cases qui tiennent un piège.
    pieges: &'a [Case],
    /// Les cases où se tient une entité ou un obstacle, l'entrée exceptée.
    prises: &'a [Case],
    /// Ce que le tour a déjà lancé, sort par sort.
    lances: &'a std::collections::BTreeMap<String, u8>,
    /// Les paires de variantes déjà engagées au sol, et le sort retenu.
    variantes: &'a std::collections::BTreeMap<u32, String>,
    /// Les PA que le tour laisse.
    restants: u32,
    largeur: usize,
    /// Le damier vide ou la carte : un piège ne se pose que sur le sol d'une
    /// carte.
    plateau: Plateau,
}

/// Le générateur : les pièges, leurs cases et leur ordre de pose qui ajoutent
/// le plus de dégâts avec les PA du tour.
fn proposer(r: &Recherche) -> Proposition {
    #[derive(Clone)]
    struct Noeud {
        poses: Vec<(usize, Case)>,
        pa: u32,
        total: (i64, i64),
        parcourues: Vec<Case>,
    }

    let lanceur = r.terrain.lanceur;
    let camps: Vec<Camp> = entites(r.entree, r.terrain).iter().map(|e| e.camp).collect();
    // Les nouveaux pièges prennent les rangs qui suivent ceux du sol : ils
    // passent après eux dans l'ordre de pose.
    let premier_rang = r.au_sol.iter().map(|p| p.rang + 1).max().unwrap_or(0);
    let derouler = |poses: &[(usize, Case)]| -> Vec<Etape> {
        let mut tous: Vec<PiegePose> = r.au_sol.to_vec();
        tous.extend(
            poses
                .iter()
                .enumerate()
                .map(|(k, (m, c))| r.modeles[*m].poser(premier_rang + k, *c)),
        );
        chaine(&tous, r.entree, r.terrain)
    };

    let depart = derouler(&[]);
    let avant = total_ennemis(&depart, &camps);
    // Les fourchettes se comparent par leur milieu.
    let valeur = |n: &Noeud| -> f64 { ((n.total.0 + n.total.1) - (avant.0 + avant.1)) as f64 / 2.0 };

    let mut faisceau = vec![Noeud {
        poses: Vec::new(),
        pa: 0,
        total: avant,
        parcourues: cases_parcourues(&Arrivee::Entree(r.entree), &depart),
    }];
    let mut meilleur = faisceau[0].clone();
    let mut essais = 0usize;
    loop {
        // Rangés par l'ENSEMBLE de leurs poses : deux ordres qui mènent au même
        // réseau ne comptent qu'une fois, le meilleur des deux.
        let mut suivants: std::collections::BTreeMap<Vec<(usize, Case)>, Noeud> = std::collections::BTreeMap::new();
        for n in &faisceau {
            let pieges: Vec<Case> = r
                .pieges
                .iter()
                .copied()
                .chain(n.poses.iter().map(|(_, c)| *c))
                .collect();
            for (m, modele) in r.modeles.iter().enumerate() {
                let ex = modele.exigences;
                if n.pa + ex.pa > r.restants {
                    continue;
                }
                // Une seule variante par paire, sol compris.
                let prise_ailleurs = modele.variante.is_some_and(|v| {
                    r.variantes.get(&v).is_some_and(|s| *s != modele.sort)
                        || n.poses
                            .iter()
                            .any(|(u, _)| r.modeles[*u].variante == Some(v) && r.modeles[*u].sort != modele.sort)
                });
                if prise_ailleurs {
                    continue;
                }
                let lances = r.lances.get(&modele.sort).copied().unwrap_or(0)
                    + u8::try_from(n.poses.iter().filter(|(u, _)| *u == m).count()).unwrap_or(u8::MAX);
                // Les centres d'où sa zone de déclenchement couvre une case où
                // une entité entre.
                let mut centres: Vec<Case> = n
                    .parcourues
                    .iter()
                    .flat_map(|h| {
                        modele
                            .gabarit
                            .declenchement
                            .iter()
                            .map(move |o| Case::new(h.x - o.x, h.y - o.y))
                    })
                    .collect();
                centres.sort_unstable();
                centres.dedup();
                for c in centres {
                    if ex.refus(c, lanceur, &pieges, r.prises, lances, 0, None, &r.plateau).is_some() {
                        continue;
                    }
                    let mut poses = n.poses.clone();
                    poses.push((m, c));
                    let etapes = derouler(&poses);
                    essais += 1;
                    let noeud = Noeud {
                        pa: n.pa + ex.pa,
                        total: total_ennemis(&etapes, &camps),
                        parcourues: cases_parcourues(&Arrivee::Entree(r.entree), &etapes),
                        poses,
                    };
                    let mut cle = noeud.poses.clone();
                    cle.sort_unstable();
                    if suivants.get(&cle).is_none_or(|e| valeur(&noeud) > valeur(e)) {
                        suivants.insert(cle, noeud);
                    }
                }
            }
        }
        if suivants.is_empty() {
            break;
        }
        let mut rang: Vec<Noeud> = suivants.into_values().collect();
        // Tri stable : à valeur et PA égaux, l'ordre des clés départage, et la
        // proposition ne change pas d'un appel à l'autre.
        rang.sort_by(|a, b| valeur(b).total_cmp(&valeur(a)).then(a.pa.cmp(&b.pa)));
        if valeur(&rang[0]) > valeur(&meilleur) {
            meilleur = rang[0].clone();
        }
        rang.truncate(r.largeur);
        faisceau = rang;
    }
    Proposition {
        poses: meilleur.poses,
        pa: meilleur.pa,
        avant,
        total: meilleur.total,
        essais,
    }
}

// ---------------------------------------------------------------------------
// Le concepteur : un réseau entier, placé sur la carte.
// ---------------------------------------------------------------------------

/// Ce que le concepteur cherche, au choix du joueur.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct Conception {
    pub objectif: Objectif,
    /// Les tours de pose de la frappe maximale, de un à dix. Le réseau en un
    /// tour n'en prend qu'un, déclenchement compris : ce nombre ne le
    /// concerne pas.
    #[serde(default = "un_tour")]
    pub tours: u8,
}

fn un_tour() -> u8 {
    1
}

/// Les tours de pose qu'un plan peut demander.
pub const TOURS_MAX: u8 = 10;

/// Où le concepteur cherche la forme d'un réseau avant de le placer : un
/// damier assez grand pour qu'aucun bord n'arrête une poussée.
const DAMIER_OUVERT: u8 = 41;

/// Combien de réseaux partiels le concepteur garde à chaque pose : sur le
/// damier ouvert, où il cherche la forme ; puis sur le plateau, autour des
/// meilleurs emplacements.
const LARGEUR_DE_CONCEPTION: usize = 300;
const LARGEUR_D_AFFINAGE: usize = 300;

/// Combien de formes trouvées sur le damier ouvert le concepteur essaie de
/// placer sur le plateau, et autour de combien d'emplacements il affine.
const FORMES_A_PLACER: usize = 6;
const EMPLACEMENTS_A_AFFINER: usize = 3;

/// La place qu'un réseau prend : les cases du plus petit rectangle, dans le
/// repère du jeu, qui tient ses pièges et la case où l'ennemi entre. À dégâts
/// égaux, le plus petit l'emporte.
fn encombrement(centres: impl Iterator<Item = Case>, ennemi: Case) -> u32 {
    let (largeur, hauteur) = emprise(centres, ennemi);
    largeur * hauteur
}

/// Les deux côtés de ce rectangle.
fn emprise(centres: impl Iterator<Item = Case>, ennemi: Case) -> (u32, u32) {
    let (mut x0, mut x1, mut y0, mut y1) = (ennemi.x, ennemi.x, ennemi.y, ennemi.y);
    for c in centres {
        x0 = x0.min(c.x);
        x1 = x1.max(c.x);
        y0 = y0.min(c.y);
        y1 = y1.max(c.y);
    }
    ((x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32)
}

/// Un réseau conçu : ses poses dans l'ordre, chacune à son tour.
#[derive(Clone, Debug)]
struct Plan {
    /// Le modèle, la case et le tour de chaque pose, dans l'ordre de pose :
    /// les tours se suivent.
    poses: Vec<(usize, Case, u8)>,
    /// Les PA des pièges, et ceux du sort de déclenchement.
    pa: u32,
    total: (i64, i64),
    valeur: f64,
    encombrement: u32,
    /// Le dernier tour de pose ; zéro sans pose.
    tours: u8,
}

impl Plan {
    /// L'ordre du concepteur : les dégâts d'abord, puis la place, puis les PA,
    /// puis les tours.
    fn avant(&self, autre: &Plan) -> std::cmp::Ordering {
        autre
            .valeur
            .total_cmp(&self.valeur)
            .then(self.encombrement.cmp(&autre.encombrement))
            .then(self.pa.cmp(&autre.pa))
            .then(self.tours.cmp(&autre.tours))
    }
}

/// Ce que le concepteur sait d'une recherche : les pièges, le terrain, la
/// case d'entrée, et ce que les tours permettent. Ni Sram ni ennemi sur le
/// damier : le joueur se place pour poser et amène la cible, et le plan ne dit
/// que les poses et les entrées. Sans Sram, aucune portée ne se juge et tout
/// coup compte à distance.
struct Atelier<'a> {
    modeles: &'a [Modele],
    terrain: &'a Terrain<'a>,
    entree: Case,
    /// Les cases où se tient une entité ou un obstacle au moment de poser.
    prises: &'a [Case],
    plateau: Plateau,
    /// Les PA de chaque tour pour les pièges : ceux du build, moins le sort qui
    /// fait entrer la cible pour le réseau en un tour.
    budgets: Vec<u32>,
    /// Ce sort : ses PA et ses dégâts comptent.
    pa_du_sort: u32,
    coup_du_sort: (i64, i64),
    largeur: usize,
}

/// Le milieu d'une fourchette : c'est par lui que deux réseaux se comparent.
fn milieu(total: (i64, i64)) -> f64 {
    (total.0 + total.1) as f64 / 2.0
}

/// `faire` sur chaque élément, les éléments répartis entre les cœurs. Les
/// résultats gardent l'ordre des éléments : ce qu'on en tire ne dépend pas
/// du cœur qui finit le premier.
fn en_parallele<T: Sync, R: Send>(elements: &[T], faire: impl Fn(&T) -> Vec<R> + Sync) -> Vec<R> {
    let fils = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let lot = elements.len().div_ceil(fils).max(1);
    let faire = &faire;
    std::thread::scope(|portee| {
        let taches: Vec<_> = elements
            .chunks(lot)
            .map(|morceau| portee.spawn(move || morceau.iter().flat_map(faire).collect::<Vec<R>>()))
            .collect();
        taches
            .into_iter()
            .flat_map(|t| t.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
            .collect()
    })
}

/// Les quatre voisines d'une case, celles d'où l'on y entre en marchant.
const VOISINES: [(i16, i16); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// Les cases des zones de déclenchement d'un réseau, rangées, sans doublon.
fn couvertes_par(pieges: &[PiegePose]) -> Vec<Case> {
    let mut cases: Vec<Case> = pieges.iter().flat_map(|p| p.declenchement.iter().copied()).collect();
    cases.sort_unstable();
    cases.dedup();
    cases
}

/// L'entrée se rejoint du large. Une voisine libre ne suffit pas : enfermée par
/// les zones des autres pièges, l'ennemi n'y arrive qu'en les traversant, et
/// chacun part avant l'heure. L'entrée doit toucher la plus grande étendue de
/// sol libre qu'aucune zone ne couvre, celle d'où le joueur amène la cible ; un
/// piège du bord l'attire ensuite à l'intérieur (l'Insidieux en diagonale, le
/// Sournois vers son centre). Deux étendues de même taille, le réseau coupant la
/// carte en deux, valent toutes deux.
struct Exterieur(Vec<Case>);

impl Exterieur {
    fn de(couvertes: &[Case], prises: &[Case], plateau: &Plateau) -> Exterieur {
        let mut sol: Vec<Case> = plateau
            .sol()
            .into_iter()
            .filter(|c| !prises.contains(c) && couvertes.binary_search(c).is_err())
            .collect();
        sol.sort_unstable();
        let mut vue = vec![false; sol.len()];
        let mut etendues: Vec<Vec<Case>> = Vec::new();
        for depart in 0..sol.len() {
            if vue[depart] {
                continue;
            }
            vue[depart] = true;
            let mut etendue = vec![sol[depart]];
            let mut k = 0;
            while k < etendue.len() {
                let c = etendue[k];
                k += 1;
                for (dx, dy) in VOISINES {
                    if let Ok(i) = sol.binary_search(&Case::new(c.x + dx, c.y + dy)) {
                        if !vue[i] {
                            vue[i] = true;
                            etendue.push(sol[i]);
                        }
                    }
                }
            }
            etendues.push(etendue);
        }
        let plus_grande = etendues.iter().map(Vec::len).max().unwrap_or(0);
        let mut large: Vec<Case> = etendues.into_iter().filter(|e| e.len() == plus_grande).flatten().collect();
        large.sort_unstable();
        Exterieur(large)
    }

    /// L'une des quatre voisines de cette case est au large.
    fn touche(&self, entree: Case) -> bool {
        VOISINES
            .iter()
            .any(|(dx, dy)| self.0.binary_search(&Case::new(entree.x + dx, entree.y + dy)).is_ok())
    }
}

/// Le tri rapide du faisceau, avant `Exterieur` sur les réseaux complets :
/// depuis l'entrée, sur du sol libre qu'aucune zone ne couvre, on sort du
/// rectangle des zones. Sur le damier ouvert, sans mur, c'est le test exact.
/// Une entrée enfermée le reste quoi qu'on ajoute, une zone ne faisant que
/// s'étendre : le faisceau l'écarte dès qu'elle l'est.
fn sort_du_reseau(entree: Case, couvertes: &[Case], prises: &[Case], plateau: &Plateau) -> bool {
    let (mut x0, mut x1, mut y0, mut y1) = (entree.x, entree.x, entree.y, entree.y);
    for c in couvertes {
        x0 = x0.min(c.x);
        x1 = x1.max(c.x);
        y0 = y0.min(c.y);
        y1 = y1.max(c.y);
    }
    let hauteur = (y1 - y0 + 1) as usize;
    let indice = |c: Case| (c.x - x0) as usize * hauteur + (c.y - y0) as usize;
    let libre = |c: Case| {
        c != entree && plateau.est_sol(c) && !prises.contains(&c) && couvertes.binary_search(&c).is_err()
    };
    let mut vues = vec![false; (x1 - x0 + 1) as usize * hauteur];
    let mut file = vec![entree];
    while let Some(c) = file.pop() {
        for (dx, dy) in VOISINES {
            let v = Case::new(c.x + dx, c.y + dy);
            if !libre(v) {
                continue;
            }
            if v.x < x0 || v.x > x1 || v.y < y0 || v.y > y1 {
                return true;
            }
            let i = indice(v);
            if !vues[i] {
                vues[i] = true;
                file.push(v);
            }
        }
    }
    false
}

/// La cible prend tous les pièges du réseau : un piège qui ne part pas
/// coûterait ses PA pour rien.
fn tous_partent(etapes: &[Etape], pieges: usize) -> bool {
    let mut partis = vec![false; pieges];
    for e in etapes {
        if let Etape::Declenche { piege, .. } = e {
            partis[*piege] = true;
        }
    }
    partis.iter().all(|p| *p)
}

/// Les cases d'où la cible prend tout le réseau, et ce que chacune rapporte, la
/// meilleure d'abord : une case qu'une zone de déclenchement couvre, libre,
/// qu'on rejoint du large, et d'où tous les pièges partent. `ici` départage deux
/// entrées qui rapportent autant : la plus proche d'abord.
fn entrees_du_reseau(
    pieges: &[PiegePose],
    terrain: &Terrain,
    prises: &[Case],
    plateau: &Plateau,
    ici: Case,
) -> Vec<(Case, (i64, i64))> {
    let cases = couvertes_par(pieges);
    let exterieur = Exterieur::de(&cases, prises, plateau);
    let camps: Vec<Camp> = entites(ici, terrain).iter().map(|e| e.camp).collect();
    let mut entrees: Vec<(Case, (i64, i64))> = cases
        .into_iter()
        .filter(|c| plateau.est_sol(*c) && !prises.contains(c) && exterieur.touche(*c))
        .filter_map(|c| {
            let etapes = chaine(pieges, c, terrain);
            tous_partent(&etapes, pieges.len()).then(|| (c, total_ennemis(&etapes, &camps)))
        })
        .collect();
    entrees.sort_by(|a, b| {
        milieu(b.1)
            .total_cmp(&milieu(a.1))
            .then(a.0.distance(ici).cmp(&b.0.distance(ici)))
            .then(a.0.cmp(&b.0))
    });
    entrees
}

/// Le concepteur : les pièges, leurs cases, leur ordre et leur tour de pose
/// qui font le plus de dégâts depuis une case d'entrée. Comme le générateur
/// de tour, une recherche par faisceau : à chaque pose, chaque réseau partiel
/// gagne un piège sur une case où sa zone couvre un passage de la cible, et
/// seuls les meilleurs continuent.
///
/// Rend les `garder` meilleurs réseaux dont tous les pièges partent et dont
/// l'entrée se rejoint du large, le meilleur d'abord, et le nombre de chaînes
/// déroulées.
fn concevoir_reseau(a: &Atelier, garder: usize) -> (Vec<Plan>, usize) {
    #[derive(Clone)]
    struct Noeud {
        plan: Plan,
        /// Les PA que chaque tour a déjà pris.
        depenses: Vec<u32>,
        parcourues: Vec<Case>,
        /// Les cases des zones de déclenchement, rangées.
        couvertes: Vec<Case>,
        complet: bool,
    }
    let tours = u8::try_from(a.budgets.len()).unwrap_or(u8::MAX);
    let camps: Vec<Camp> = entites(a.entree, a.terrain).iter().map(|e| e.camp).collect();
    let noeud = |poses: Vec<(usize, Case, u8)>, depenses: Vec<u32>, couvertes: Vec<Case>| -> Noeud {
        let tous: Vec<PiegePose> = poses
            .iter()
            .enumerate()
            .map(|(k, (m, c, _))| a.modeles[*m].poser(k, *c))
            .collect();
        let etapes = chaine(&tous, a.entree, a.terrain);
        let frappe = total_ennemis(&etapes, &camps);
        let total = (frappe.0 + a.coup_du_sort.0, frappe.1 + a.coup_du_sort.1);
        Noeud {
            complet: !poses.is_empty() && tous_partent(&etapes, poses.len()),
            parcourues: cases_parcourues(&Arrivee::Entree(a.entree), &etapes),
            plan: Plan {
                pa: depenses.iter().sum::<u32>() + a.pa_du_sort,
                total,
                valeur: milieu(total),
                encombrement: encombrement(poses.iter().map(|(_, c, _)| *c), a.entree),
                tours: poses.last().map_or(0, |p| p.2),
                poses,
            },
            depenses,
            couvertes,
        }
    };
    // Ce qu'un réseau partiel devient en gagnant un piège : chaque piège, sur
    // chaque case d'où sa zone couvre un passage de la cible, au tour où il
    // tient.
    let enfants = |n: &Noeud| -> Vec<Noeud> {
        let mut sortie = Vec::new();
        let poses = &n.plan.poses;
        let pieges: Vec<Case> = poses.iter().map(|(_, c, _)| *c).collect();
        // Les tours entamés, et le premier tour libre : les tours libres se
        // valent tous.
        let ouverts = n.plan.tours.saturating_add(1).min(tours);
        for (m, modele) in a.modeles.iter().enumerate() {
            let ex = modele.exigences;
            // Une seule variante par paire, sur tout le réseau.
            if modele.variante.is_some_and(|v| {
                poses
                    .iter()
                    .any(|(u, _, _)| a.modeles[*u].variante == Some(v) && a.modeles[*u].sort != modele.sort)
            }) {
                continue;
            }
            let tient = |t: u8| {
                let lances = poses.iter().filter(|(u, _, tt)| *u == m && *tt == t).count();
                n.depenses[usize::from(t - 1)] + ex.pa <= a.budgets[usize::from(t - 1)]
                    && (ex.lancers_par_tour == 0 || lances < usize::from(ex.lancers_par_tour))
            };
            // Le premier tour qui a la place, et le dernier. Le premier
            // comble un tour entamé, au lieu de passer toutes les poses au
            // tour d'après dès qu'un piège n'y tient plus. Le dernier garde
            // l'ordre de pose : un piège posé plus tôt part plus tôt quand
            // deux pièges partent ensemble, ce qui peut changer la chaîne.
            let possibles: Vec<u8> = (1..=ouverts).filter(|&t| tient(t)).collect();
            let (Some(&premier), Some(&dernier)) = (possibles.first(), possibles.last()) else {
                continue;
            };
            let essayes: &[u8] = if premier == dernier { &[premier] } else { &[premier, dernier] };
            let mut centres: Vec<Case> = n
                .parcourues
                .iter()
                .flat_map(|h| {
                    modele
                        .gabarit
                        .declenchement
                        .iter()
                        .map(move |o| Case::new(h.x - o.x, h.y - o.y))
                })
                .collect();
            centres.sort_unstable();
            centres.dedup();
            for c in centres {
                if ex.refus(c, None, &pieges, a.prises, 0, 0, None, &a.plateau).is_some() {
                    continue;
                }
                // Une zone ne fait que s'étendre : une entrée enfermée le reste.
                let mut couvertes = n.couvertes.clone();
                couvertes.extend(modele.gabarit.declenchement.iter().map(|o| Case::new(c.x + o.x, c.y + o.y)));
                couvertes.sort_unstable();
                couvertes.dedup();
                if !sort_du_reseau(a.entree, &couvertes, a.prises, &a.plateau) {
                    continue;
                }
                for &t in essayes {
                    // À la fin des poses de son tour, avant celles des tours
                    // suivants.
                    let mut suite = poses.clone();
                    suite.insert(poses.partition_point(|p| p.2 <= t), (m, c, t));
                    let mut depenses = n.depenses.clone();
                    depenses[usize::from(t - 1)] += ex.pa;
                    sortie.push(noeud(suite, depenses, couvertes.clone()));
                }
            }
        }
        sortie
    };

    if !Exterieur::de(&[], a.prises, &a.plateau).touche(a.entree) {
        return (Vec::new(), 0);
    }
    let racine = noeud(Vec::new(), vec![0; usize::from(tours)], Vec::new());
    let mut faisceau = vec![racine];
    let mut meilleurs: Vec<Plan> = Vec::new();
    let mut essais = 0usize;
    loop {
        // ⚠️ LES RÉSEAUX PARTIELS SE PARTAGENT ENTRE LES CŒURS : dérouler la
        // chaîne de chaque candidat est de loin le plus coûteux, et dix tours
        // en font des centaines de milliers.
        let candidats = en_parallele(&faisceau, &enfants);
        // Rangés par l'ENSEMBLE de leurs poses : deux ordres qui mènent au même
        // réseau ne comptent qu'une fois, le meilleur des deux.
        let mut suivants: std::collections::BTreeMap<Vec<(usize, Case)>, Noeud> = std::collections::BTreeMap::new();
        for suivant in candidats {
            essais += 1;
            let mut cle: Vec<(usize, Case)> = suivant.plan.poses.iter().map(|(m, c, _)| (*m, *c)).collect();
            cle.sort_unstable();
            if suivants
                .get(&cle)
                .is_none_or(|e| suivant.plan.avant(&e.plan) == std::cmp::Ordering::Less)
            {
                suivants.insert(cle, suivant);
            }
        }
        if suivants.is_empty() {
            break;
        }
        let mut rang: Vec<Noeud> = suivants.into_values().collect();
        // Tri stable : à égalité parfaite, l'ordre des clés départage, et le
        // plan ne change pas d'un appel à l'autre.
        rang.sort_by(|x, y| x.plan.avant(&y.plan));
        // Le test exact sur les réseaux complets : sur une carte, sortir du
        // rectangle des zones peut mener dans un cul-de-sac.
        meilleurs.extend(
            rang.iter()
                .filter(|n| n.complet && Exterieur::de(&n.couvertes, a.prises, &a.plateau).touche(a.entree))
                .take(garder)
                .map(|n| n.plan.clone()),
        );
        meilleurs.sort_by(Plan::avant);
        meilleurs.truncate(garder);
        rang.truncate(a.largeur);
        faisceau = rang;
    }
    (meilleurs, essais)
}

/// Les huit façons de tourner et retourner un réseau sur le damier : elles
/// gardent les distances et l'alignement.
const SYMETRIES: [(i16, i16, i16, i16); 8] = [
    (1, 0, 0, 1),
    (0, -1, 1, 0),
    (-1, 0, 0, -1),
    (0, 1, -1, 0),
    (1, 0, 0, -1),
    (-1, 0, 0, 1),
    (0, 1, 1, 0),
    (0, -1, -1, 0),
];

fn transformer(c: Case, g: (i16, i16, i16, i16), ancre: Case) -> Case {
    Case::new(ancre.x + g.0 * c.x + g.1 * c.y, ancre.y + g.2 * c.x + g.3 * c.y)
}

/// Ce que le concepteur rend : le plan, la case d'où il l'a conçu, et le sort
/// qui fait entrer la cible pour le réseau en un tour.
struct Concu {
    entree: Case,
    sort: Option<usize>,
    plan: Plan,
}

/// Le concepteur entier : la forme du réseau sur un damier ouvert, puis son
/// meilleur emplacement sur le plateau, tourné ou retourné, puis un affinage
/// sur place, qui profite des murs et des trous de la carte. Le joueur ne donne
/// pas d'entrée : le plan dit ensuite toutes celles d'où la cible prend le
/// réseau.
///
/// La frappe maximale se pose en autant de tours que le joueur en donne, et le
/// joueur amène l'ennemi à une entrée. Le réseau en un tour essaie chacun des
/// sorts qui y font entrer la cible (Sournoiserie, Perquisition, Guet-apens,
/// Méprise) : ses PA se prennent sur le tour, ses dégâts s'ajoutent, à
/// distance. Le joueur se place pour le lancer.
#[allow(clippy::too_many_arguments)]
fn concevoir(
    conception: Conception,
    modeles: &[Modele],
    declencheurs: &[Declencheur],
    plateau: Plateau,
    obstacles: &[Case],
    autres: &[Entite],
    ordre: Ordre,
    chakra: Option<Portees>,
    poussee: Poussee,
    pa: u32,
) -> (Option<Concu>, usize) {
    let tours = match conception.objectif {
        Objectif::Frappe => conception.tours.clamp(1, TOURS_MAX),
        Objectif::UnTour => 1,
    };
    // ⚠️ À PRIX ÉGAL, LE MÊME RÉSEAU : le sort d'entrée ne compte que pour ses
    // PA et ses dégâts. Une recherche par prix, avec le sort qui frappe le plus
    // ; les autres au même prix se proposent aussi, car le joueur lance celui
    // qu'il a équipé et qu'il peut placer (Peur ou Méprise, une paire).
    let sorts: Vec<Option<usize>> = match conception.objectif {
        Objectif::Frappe => vec![None],
        Objectif::UnTour => {
            let mut par_prix: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
            for (i, t) in declencheurs.iter().enumerate() {
                let retenu = par_prix.entry(t.pa).or_insert(i);
                if milieu(t.coup.distance) > milieu(declencheurs[*retenu].coup.distance) {
                    *retenu = i;
                }
            }
            par_prix.into_values().map(Some).collect()
        }
    };
    // Le sort part au dernier tour, avec ce que les pièges laissent.
    let budgets = |sort: Option<usize>| -> Vec<u32> {
        let mut b = vec![pa; usize::from(tours)];
        if let (Some(i), Some(dernier)) = (sort, b.last_mut()) {
            *dernier = dernier.saturating_sub(declencheurs[i].pa);
        }
        b
    };
    let pa_du_sort = |sort: Option<usize>| sort.map_or(0, |i| declencheurs[i].pa);
    let coup_du_sort = |sort: Option<usize>| sort.map_or((0, 0), |i| declencheurs[i].coup.distance);
    let mut essais = 0usize;

    // 1. Les formes, sur un damier ouvert.
    let ouvert = Plateau::Damier(DAMIER_OUVERT);
    let bords_ouverts = ouvert.bords();
    let terrain_ouvert = Terrain {
        obstacles: &bords_ouverts,
        lanceur: None,
        autres: &[],
        ordre,
        chakra,
        poussee,
    };
    let mut formes: Vec<(Option<usize>, Plan)> = Vec::new();
    for &sort in &sorts {
        let (plans, n) = concevoir_reseau(
            &Atelier {
                modeles,
                terrain: &terrain_ouvert,
                entree: Case::new(0, 0),
                prises: &[],
                plateau: ouvert,
                budgets: budgets(sort),
                pa_du_sort: pa_du_sort(sort),
                coup_du_sort: coup_du_sort(sort),
                largeur: LARGEUR_DE_CONCEPTION,
            },
            FORMES_A_PLACER,
        );
        essais += n;
        formes.extend(plans.into_iter().map(|p| (sort, p)));
    }
    formes.sort_by(|a, b| a.1.avant(&b.1));
    formes.truncate(FORMES_A_PLACER);

    // 2. Chaque forme, à chaque case libre du plateau, tournée ou retournée :
    // sa chaîne s'y déroule de nouveau, murs compris, tous ses pièges doivent
    // partir et son entrée rester accessible.
    let prises: Vec<Case> = obstacles.iter().copied().chain(autres.iter().map(|e| e.case)).collect();
    let libre = |c: Case| plateau.est_sol(c) && !prises.contains(&c);
    let terrain = Terrain {
        obstacles,
        lanceur: None,
        autres,
        ordre,
        chakra,
        poussee,
    };
    let camps: Vec<Camp> = entites(Case::new(0, 0), &terrain).iter().map(|e| e.camp).collect();
    let ancres: Vec<Case> = plateau.sol().into_iter().filter(|c| libre(*c)).collect();
    let placer = |ancre: &Case| -> Vec<Concu> {
        let ancre = *ancre;
        let mut sortie = Vec::new();
        for (sort, plan) in &formes {
            for g in SYMETRIES {
                let poses: Vec<(usize, Case, u8)> = plan
                    .poses
                    .iter()
                    .map(|(m, c, t)| (*m, transformer(*c, g, ancre), *t))
                    .collect();
                if poses.iter().any(|(_, c, _)| !libre(*c)) {
                    continue;
                }
                let tous: Vec<PiegePose> = poses
                    .iter()
                    .enumerate()
                    .map(|(k, (m, c, _))| modeles[*m].poser(k, *c))
                    .collect();
                let couvertes = couvertes_par(&tous);
                if !sort_du_reseau(ancre, &couvertes, &prises, &plateau) {
                    continue;
                }
                let etapes = chaine(&tous, ancre, &terrain);
                if !tous_partent(&etapes, tous.len()) {
                    continue;
                }
                if !Exterieur::de(&couvertes, &prises, &plateau).touche(ancre) {
                    continue;
                }
                let frappe = total_ennemis(&etapes, &camps);
                let coup = coup_du_sort(*sort);
                let total = (frappe.0 + coup.0, frappe.1 + coup.1);
                sortie.push(Concu {
                    entree: ancre,
                    sort: *sort,
                    plan: Plan {
                        valeur: milieu(total),
                        total,
                        poses,
                        pa: plan.pa,
                        encombrement: plan.encombrement,
                        tours: plan.tours,
                    },
                });
            }
        }
        sortie
    };
    let mut places = en_parallele(&ancres, &placer);
    essais += places.len();
    // À valeur et place égales, l'emplacement le plus proche du centre du sol :
    // sur un damier, tous se valent, et le premier parcouru était un coin.
    let n = ancres.len().max(1) as i64;
    let centre = Case::new(
        (ancres.iter().map(|c| i64::from(c.x)).sum::<i64>() / n) as i16,
        (ancres.iter().map(|c| i64::from(c.y)).sum::<i64>() / n) as i16,
    );
    let ordre_des_concus = |a: &Concu, b: &Concu| {
        a.plan
            .avant(&b.plan)
            .then(a.entree.distance(centre).cmp(&b.entree.distance(centre)))
    };
    places.sort_by(ordre_des_concus);

    // 3. Autour des meilleurs emplacements, la recherche reprend sur le plateau
    // même : un mur qui arrête une poussée peut servir le réseau, un recoin
    // peut le tenir mieux que la forme du damier ouvert.
    let mut a_affiner: Vec<(Case, Option<usize>)> = Vec::new();
    for c in &places {
        if a_affiner.len() >= EMPLACEMENTS_A_AFFINER {
            break;
        }
        if !a_affiner.contains(&(c.entree, c.sort)) {
            a_affiner.push((c.entree, c.sort));
        }
    }
    // ⚠️ FAUTE DE PLACE POUR LES FORMES, LES CASES LES PLUS DÉGAGÉES. Sur une
    // carte étroite, aucune forme du damier ouvert ne tient : la recherche part
    // alors des cases qui ont le plus de sol autour d'elles, écartées les unes
    // des autres.
    if a_affiner.len() < EMPLACEMENTS_A_AFFINER {
        let degage = |c: Case| ancres.iter().filter(|d| d.distance(c) <= 3).count();
        let mut par_espace = ancres.clone();
        par_espace.sort_by_key(|c| (std::cmp::Reverse(degage(*c)), c.distance(centre)));
        let mut choisies: Vec<Case> = a_affiner.iter().map(|(c, _)| *c).collect();
        for c in par_espace {
            if a_affiner.len() >= EMPLACEMENTS_A_AFFINER {
                break;
            }
            if choisies.iter().any(|d| d.distance(c) <= 3) {
                continue;
            }
            choisies.push(c);
            a_affiner.extend(sorts.iter().map(|&sort| (c, sort)));
        }
    }
    let mut affines: Vec<Concu> = Vec::new();
    for (entree, sort) in a_affiner {
        let (plans, n) = concevoir_reseau(
            &Atelier {
                modeles,
                terrain: &terrain,
                entree,
                prises: &prises,
                plateau,
                budgets: budgets(sort),
                pa_du_sort: pa_du_sort(sort),
                coup_du_sort: coup_du_sort(sort),
                largeur: LARGEUR_D_AFFINAGE,
            },
            1,
        );
        essais += n;
        affines.extend(plans.into_iter().map(|plan| Concu { entree, sort, plan }));
    }
    let meilleur = places.into_iter().take(1).chain(affines).min_by(ordre_des_concus);
    (meilleur, essais)
}

/// Ce que le build fait aux dégâts de piège, à distance puis au contact : chaque
/// dégât de piège passe par les dommages finaux, les dommages aux sorts, et la
/// mêlée ou la distance selon l'écart entre le Sram et la cible.
fn multiplicateurs(resolved: &dofus_build::Resolved) -> (FinalMultiplier, FinalMultiplier) {
    // Ceux d'un tour sur deux comptent au tour impair, comme dans KrozZone et
    // l'infobulle : le Rêve Nébuleux à +20 %, pas son −10 % des tours pairs.
    let finaux: Vec<u32> = resolved
        .damage_multipliers
        .iter()
        .filter(|(n, _)| resolved.tours.get(n) != Some(&dofus_build::Tours::Pairs))
        .map(|(_, p)| *p)
        .collect();
    let finaux = FinalMultiplier::from_percents(&finaux);
    let pour = |reach: Reach| {
        finaux.and(FinalMultiplier::for_cast(
            &resolved.profile,
            &Resistance::NONE,
            CastContext {
                delivery: Delivery::Spell,
                reach,
            },
        ))
    };
    (pour(Reach::Ranged), pour(Reach::Melee))
}

/// La Portée du build s'ajoute au maximum de chaque sort à portée modifiable,
/// sans le faire passer sous son minimum, comme dans KrozZone
/// (`grille::contraintes_du_sort`). KrozTrap lit ses portées dans
/// l'instantané, qui la reçoit donc ici, après les objets de classe : l'un
/// d'eux peut rendre une portée modifiable.
fn avec_la_portee_du_build(snap: &mut dofus_ruleset::snapshot::Snapshot, bonus: i32) {
    if bonus == 0 {
        return;
    }
    for niveau in snap.spells.iter_mut().flat_map(|s| s.levels.iter_mut()) {
        if !niveau.cast.is_none_or(|c| c.range_boostable) {
            continue;
        }
        if let Some([Some(min), Some(max)]) = niveau.range.as_mut() {
            *max = u8::try_from((i32::from(*max) + bonus).max(i32::from(*min))).unwrap_or(u8::MAX);
        }
    }
}

/// Les pièges de la classe, prêts à être posés partout par le générateur.
fn modeles_de(
    ruleset: &dofus_ruleset::Ruleset,
    snap: &dofus_ruleset::snapshot::Snapshot,
    profile: &DamageProfile,
    distance: FinalMultiplier,
    melee: FinalMultiplier,
) -> Vec<Modele> {
    ruleset
        .spells
        .iter()
        .filter_map(|def| {
            let id = def.dofusdb_id?;
            let pg = piege(snap, id)?;
            Some(Modele {
                sort: def.id.clone(),
                nom: pg.nom.to_string(),
                dofusdb_id: id,
                variante: pg.variante,
                exigences: Exigences::de(pg.niveau),
                gabarit: piege_au_sol(&pg, id, 0, Case::new(0, 0), profile, distance, melee),
            })
        })
        .collect()
}

pub fn reseau_json(requete: &RequeteReseau) -> Result<String, String> {
    let build = requete.build.normalise();
    let mut ruleset = load_ruleset(build.class)?;
    let mut snap = snapshot_for(build.class)?;
    let resolved = resolve_build(&build)?;
    crate::objets_de_classe::sur_les_regles(&mut ruleset, &resolved);
    crate::objets_de_classe::sur_l_instantane(&mut snap, &resolved);
    avec_la_portee_du_build(&mut snap, resolved.totals.get("range").copied().unwrap_or(0));
    // Le Sram, s'il est sur le damier : absent, le joueur se place où il veut,
    // aucune portée ne se juge et tout coup compte à distance.
    let lanceur: Option<Case> = requete.lanceur.map(|(x, y)| Case::new(x, y));
    let plateau = Plateau::de(requete.carte, requete.damier)?;
    // ⚠️ LES BORDS D'UNE CARTE SONT DES OBSTACLES. Ses murs et ses trous
    // arrêtent une poussée comme un mur posé à la main, et la font buter, avec
    // ses dommages de poussée. Seuls ceux qui touchent le sol comptent : un
    // déplacement va de case en case et bute sur le premier.
    let obstacles: Vec<Case> = requete
        .obstacles
        .iter()
        .map(|&(x, y)| Case::new(x, y))
        .chain(plateau.bords())
        .collect();

    let (mult_distance, mult_melee) = multiplicateurs(&resolved);

    let mut poses = Vec::new();
    let mut couverture: Couverture = std::collections::BTreeMap::new();
    let mut occupees: Vec<Case> = Vec::new();
    let mut au_sol: Vec<PiegePose> = Vec::new();
    let mut pa_total: u32 = 0;
    // Les lancers et les PA de chaque tour, et la variante retenue de chaque
    // paire.
    let mut lances: std::collections::BTreeMap<(u8, String), u8> = std::collections::BTreeMap::new();
    let mut depenses: std::collections::BTreeMap<u8, u32> = std::collections::BTreeMap::new();
    let mut variantes: std::collections::BTreeMap<u32, String> = std::collections::BTreeMap::new();
    let budget = requete.pa.map(u32::from);
    // Là où se tient une entité ou un obstacle au moment de poser. L'entrée
    // n'en fait pas partie : l'ennemi n'y arrive qu'une fois le réseau posé.
    let prises: Vec<Case> = obstacles
        .iter()
        .copied()
        .chain(lanceur)
        .chain(requete.allies.iter().chain(&requete.ennemis).map(|&(x, y)| Case::new(x, y)))
        .collect();

    for (rang, p) in requete.poses.iter().enumerate() {
        let Some(def) = ruleset.spells.iter().find(|s| s.id == p.sort) else {
            return Err(format!(
                "le sort `{}` n'est pas dans le fichier de règles",
                p.sort
            ));
        };
        let Some(id) = def.dofusdb_id else {
            return Err(format!("le sort `{}` n'a pas d'identifiant de jeu", p.sort));
        };
        let Some(pg) = piege(&snap, id) else {
            return Err(format!("`{}` ne pose pas de piège", def.name.fr));
        };
        let case = Case::new(p.case.0, p.case.1);

        // ⚠️ LE REFUS VIENT DE LA DONNÉE, PAS D'UNE PRÉFÉRENCE. Les quatorze
        // poses de la classe portent `needs_free_trap_cell` : le jeu refuse une
        // pose sur une case qui tient déjà un piège. Leurs zones, elles, se
        // recouvrent, et c'est justement ce qu'on cherche à construire.
        let exigences = Exigences::de(pg.niveau);
        let portee = exigences.portee;
        let deja = lances.get(&(p.tour, def.id.clone())).copied().unwrap_or(0);
        let depense = depenses.get(&p.tour).copied().unwrap_or(0);
        let autre_variante = pg
            .variante
            .and_then(|v| variantes.get(&v))
            .is_some_and(|s| *s != def.id);
        let refus = if autre_variante {
            Some("l'autre variante est déjà posée : le jeu n'en laisse équiper qu'une")
        } else {
            exigences.refus(case, lanceur, &occupees, &prises, deja, depense, budget, &plateau)
        };

        let (lignes, poison) =
            separer_poison(lignes_du_piege(pg.niveau, case, &resolved.profile, mult_distance, mult_melee), id);

        // Où il faut poser le pied pour que le piège parte. La donnée le dit
        // depuis la passe `poses-detail` ; à défaut, on retombe sur la case du
        // piège, qui est le cas le plus restrictif et jamais une surestimation.
        let declenchement: Vec<Case> = declenchement(pg.niveau, case).unwrap_or_else(|| vec![case]);
        let mono = declenchement.len() == 1;

        // Une pose refusée ne compte ni en couverture ni en PA : elle ne
        // partira pas. Elle reste dans la liste, avec son motif.
        if refus.is_none() {
            occupees.push(case);
            pa_total += exigences.pa;
            *lances.entry((p.tour, def.id.clone())).or_insert(0) += 1;
            *depenses.entry(p.tour).or_insert(0) += exigences.pa;
            if let Some(v) = pg.variante {
                variantes.insert(v, def.id.clone());
            }
            au_sol.push(piege_au_sol(
                &pg,
                id,
                rang,
                case,
                &resolved.profile,
                mult_distance,
                mult_melee,
            ));
            // La couverture répond à « si un ennemi entre ici » : le piège
            // part parce que la case est dans son déclenchement, et l'ennemi
            // y prend ce que la ligne dont l'effet couvre cette case lui
            // inflige. Les deux zones diffèrent sur les pièges mono-cellule,
            // où seule la case centrale déclenche.
            for c in &declenchement {
                let subi = lignes
                    .iter()
                    .filter(|l| l.cases.as_ref().is_some_and(|z| z.contains(c)))
                    .fold((0i64, 0i64), |(a, b), l| (a + l.degats.0, b + l.degats.1));
                couverture.entry((c.x, c.y)).or_default().push((rang, subi));
            }
        }

        // Ce qu'un ennemi prendrait sur la case même du piège, de ce piège
        // seul : le seul chiffre par pose qui ait un sens une fois que chaque
        // ligne a sa propre zone.
        let sur_le_centre = lignes
            .iter()
            .filter(|l| l.cases.as_ref().is_some_and(|c| c.contains(&case)))
            .fold((0i64, 0i64), |(a, b), l| (a + l.degats.0, b + l.degats.1));

        let toutes_les_cases: Option<Vec<[i16; 2]>> = if lignes.iter().all(|l| l.cases.is_none()) {
            None
        } else {
            let mut v: Vec<Case> = lignes
                .iter()
                .flat_map(|l| l.cases.iter().flatten().copied())
                .collect();
            v.sort_unstable();
            v.dedup();
            Some(v.iter().map(|c| [c.x, c.y]).collect())
        };

        poses.push(serde_json::json!({
            "sort": def.id,
            "nom": pg.nom,
            "dofusdb_id": def.dofusdb_id,
            "case": [case.x, case.y],
            "pa": pg.niveau.ap_cost,
            "portee": portee.map(|(a, b)| vec![a, b]),
            "lancers_par_tour": pg.niveau.max_cast_per_turn,
            "intervalle": pg.niveau.min_cast_interval,
            "mono_cellule": mono,
            "deplacement": deplacement(pg.niveau)
                .map(|(sens, n)| serde_json::json!({ "sens": sens, "cases": n })),
            // Le texte du jeu, rendu tel quel : c'est lui qui explique une
            // variante écartée, et le paraphraser serait s'en écarter.
            "texte": pg.texte,
            "declenchement": declenchement.iter().map(|c| [c.x, c.y]).collect::<Vec<_>>(),
            "lignes": lignes.iter().map(|l| serde_json::json!({
                "zone": l.signature,
                "masque": l.masque,
                "element": format!("{:?}", l.element),
                "degats": [l.degats.0, l.degats.1],
                "cases": l.cases.as_ref().map(|c| c.len()),
                "variante_ecartee": l.ecartee.map(|(a, b)| vec![a, b]),
            })).collect::<Vec<_>>(),
            "cases": toutes_les_cases,
            "degats": [sur_le_centre.0, sur_le_centre.1],
            // Le poison de fin de tour, à part : il ne tombe pas au déclenchement.
            "poison": somme(&poison).map(|(a, b)| vec![a, b]),
            "elements": lignes.iter().map(|l| format!("{:?}", l.element))
                .collect::<std::collections::BTreeSet<_>>(),
            "tour": p.tour,
            "refus": refus,
        }));
    }

    // La couverture, case par case : qui la couvre et ce qu'elle coûterait à un
    // ennemi qui y entrerait en déclenchant tout. Chaque entrée porte déjà les
    // dégâts de SA ligne, donc la somme est directe.
    let cases: Vec<serde_json::Value> = couverture
        .iter()
        .filter(|((x, y), _)| lanceur.is_none_or(|l| Case::new(*x, *y).distance(l) <= RAYON_RENDU + 6))
        .map(|((x, y), entrees)| {
            let total = entrees
                .iter()
                .fold((0i64, 0i64), |(a, b), (_, d)| (a + d.0, b + d.1));
            let mut rangs: Vec<usize> = entrees.iter().map(|(r, _)| *r).collect();
            rangs.dedup();
            serde_json::json!({
                "case": [x, y],
                "pieges": rangs,
                "degats": [total.0, total.1],
            })
        })
        .collect();

    let meilleure = cases
        .iter()
        .max_by_key(|c| c["degats"][1].as_i64().unwrap_or(0))
        .cloned();

    // La chaîne, quand une case d'entrée est donnée. Les pièges refusés n'y
    // entrent pas : ils ne sont pas au sol.
    let ordre = requete.ordre.unwrap_or_default();
    // Le vol de vie du Chakra passe par le build, comme tout dégât : 12 fixes
    // dans le meilleur élément du lanceur, effet 2828 du sort 12903.
    let chakra = requete.chakra.then(|| {
        let element = dofus_engine::meilleur_element(&resolved.profile);
        let ligne = SpellLine {
            element,
            normal: (12, 12),
            critical: (12, 12),
        };
        let vol = |m| dofus_damage::range(&ligne, &resolved.profile, m, false, &Resistance::NONE);
        (
            element,
            Portees {
                distance: vol(mult_distance),
                melee: vol(mult_melee),
            },
        )
    });
    // Le Sram est une entité : il occupe sa case et arrête une poussée comme un
    // mur, mais ses pièges le déplacent aussi, leurs déplacements visant `a,A`, et
    // la priorité horaire décide qui passe le premier. Une entité posée sur une
    // case déjà prise (mur, Sram, entrée, autre entité) est ignorée. Ce n'est pas
    // le même ensemble que `prises`, qui sert aux poses et laisse l'entrée libre.
    let mut tenues: Vec<Case> = obstacles.clone();
    tenues.extend(lanceur);
    // La cible part de sa case quand un sort l'amène, de l'entrée sinon.
    let cible_part: Option<Case> = requete
        .declencheur
        .as_ref()
        .map(|d| Case::new(d.depart.0, d.depart.1))
        .or(requete.entree.map(|(x, y)| Case::new(x, y)));
    tenues.extend(cible_part);
    let mut autres: Vec<Entite> = Vec::new();
    for (camp, liste) in [(Camp::Allie, &requete.allies), (Camp::Ennemi, &requete.ennemis)] {
        for &(x, y) in liste {
            let case = Case::new(x, y);
            if !tenues.contains(&case) {
                tenues.push(case);
                autres.push(Entite { camp, case });
            }
        }
    }
    // Le sort qui fait entrer l'ennemi, lancé à la fin du dernier tour avec les
    // PA qui lui restent. Refusé, il ne déroule rien.
    let tour_du_lancer = requete
        .tour
        .unwrap_or_else(|| requete.poses.iter().map(|p| p.tour).max().unwrap_or(1));
    let declencheur = match &requete.declencheur {
        None => None,
        Some(d) => {
            let t = Declencheur::de(&ruleset, &snap, &d.sort, &resolved.profile, mult_distance, mult_melee)
                .ok_or_else(|| format!("`{}` ne fait pas entrer l'ennemi dans un réseau", d.sort))?;
            let depart = Case::new(d.depart.0, d.depart.1);
            let corps: Vec<Case> = autres.iter().map(|e| e.case).collect();
            let depense = depenses.get(&tour_du_lancer).copied().unwrap_or(0);
            let vise = d.vise.map(|(x, y)| Case::new(x, y));
            let refus = if obstacles.contains(&depart) || Some(depart) == lanceur || corps.contains(&depart) {
                Some("la case de l'ennemi doit être libre")
            } else if budget.is_some_and(|b| depense + t.pa > b) {
                Some("plus de PA que le tour n'en a")
            } else {
                let tenues: Vec<Case> = obstacles.iter().chain(&corps).copied().collect();
                match lanceur {
                    Some(l) => t.refus(l, depart, vise, &corps, &tenues, &plateau),
                    None => Some("placez le Sram : le sort part de sa case"),
                }
            };
            if refus.is_none() {
                pa_total += t.pa;
                *depenses.entry(tour_du_lancer).or_insert(0) += t.pa;
            }
            // Peur pousse jusqu'à la case visée : sa distance au départ.
            let t = match (t.mouvement, vise) {
                (Mouvement::JusquA(_), Some(v)) => Declencheur {
                    mouvement: Mouvement::JusquA(i32::from(depart.distance(v))),
                    ..t
                },
                _ => t,
            };
            Some((t, depart, refus))
        }
    };
    let arrivee: Option<Arrivee> = match &declencheur {
        Some((t, depart, None)) => Some(Arrivee::Sort {
            depart: *depart,
            mouvement: t.mouvement,
            coup: t.coup,
        }),
        Some((_, _, Some(_))) => None,
        None => requete.entree.map(|(x, y)| Arrivee::Entree(Case::new(x, y))),
    };
    let nom_du_sort = declencheur.as_ref().map(|(t, _, _)| t.nom.clone()).unwrap_or_default();
    let chaine_json = arrivee.map(|arrivee| {
        let terrain = Terrain {
            obstacles: &obstacles,
            lanceur,
            autres: &autres,
            ordre,
            chakra: chakra.map(|(_, vol)| vol),
            poussee: Poussee {
                niveau: resolved.profile.level,
                do_poussee: resolved.profile.push_damage,
            },
        };
        let etapes = chaine_arrivee(&au_sol, arrivee, &terrain);
        let depart = entites(arrivee.depart(), &terrain);

        // Où chacun finit, et ce que chacun a pris.
        let mut arrivees: Vec<Case> = depart.iter().map(|e| e.case).collect();
        let mut subis = vec![(0i64, 0i64); depart.len()];
        for e in &etapes {
            match e {
                Etape::Deplace { entite, vers, .. } => arrivees[*entite] = *vers,
                Etape::Amene { vers, .. } => arrivees[0] = *vers,
                // Le Sram est la deuxième entité, toujours là ici.
                Etape::Echange { cible, sram } => {
                    arrivees[0] = *cible;
                    arrivees[1] = *sram;
                }
                Etape::Coup { degats } => {
                    subis[0].0 += degats.0;
                    subis[0].1 += degats.1;
                }
                Etape::Degats { entite, degats, .. } => {
                    subis[*entite].0 += degats.0;
                    subis[*entite].1 += degats.1;
                }
                Etape::Chakra { degats } => {
                    subis[0].0 += degats.0;
                    subis[0].1 += degats.1;
                }
                Etape::Poussee { entite, degats, .. } => {
                    subis[*entite].0 += degats;
                    subis[*entite].1 += degats;
                }
                _ => {}
            }
        }
        // Le total : tout ce que prennent les ennemis, seuls frappés.
        let total = subis.iter().fold((0i64, 0i64), |(a, b), d| (a + d.0, b + d.1));
        let (mut allies, mut ennemis) = (0u32, 0u32);
        let fiches: Vec<serde_json::Value> = depart
            .iter()
            .enumerate()
            .map(|(i, e)| {
                // Un numéro par camp, pour que la page dise « allié 2 ». La
                // cible et le Sram n'en ont pas besoin.
                let numero = match e.camp {
                    Camp::Allie => {
                        allies += 1;
                        Some(allies)
                    }
                    Camp::Ennemi if i > 0 => {
                        ennemis += 1;
                        Some(ennemis)
                    }
                    _ => None,
                };
                serde_json::json!({
                    "camp": e.camp,
                    "cible": i == 0,
                    "numero": numero,
                    "depart": [e.case.x, e.case.y],
                    "arrivee": [arrivees[i].x, arrivees[i].y],
                    "degats": [subis[i].0, subis[i].1],
                })
            })
            .collect();
        serde_json::json!({
            "ordre": format!("{ordre:?}").to_lowercase(),
            "total": [total.0, total.1],
            "entites": fiches,
            "etapes": etapes.iter().map(|e| match e {
                Etape::Declenche { piege, sur } => serde_json::json!({
                    "quoi": "declenche", "piege": piege,
                    "nom": au_sol[*piege].nom, "case": [sur.x, sur.y],
                }),
                Etape::Degats { piege, entite, degats } => serde_json::json!({
                    "quoi": "degats", "piege": piege, "entite": entite,
                    "nom": au_sol[*piege].nom, "degats": [degats.0, degats.1],
                }),
                Etape::Deplace { piege, entite, de, vers, voulu, fait, arretee_par_piege } => serde_json::json!({
                    "quoi": "deplace", "piege": piege, "entite": entite, "nom": au_sol[*piege].nom,
                    // Le SENS, pour que la page n'ait pas à le déduire de deux
                    // couples de coordonnées qui ne parlent à aucun joueur.
                    "sens": au_sol[*piege].deplacement.as_ref()
                        .map(|d| if d.sens >= 0 { "pousse" } else { "attire" }),
                    "de": [de.x, de.y], "vers": [vers.x, vers.y],
                    "voulu": voulu, "fait": fait,
                    // Une poussée qui bute occasionne des dommages de poussée :
                    // l'étape « poussee » qui suit les porte.
                    "bloquee": fait < voulu && !arretee_par_piege,
                    "arretee_par_piege": arretee_par_piege,
                }),
                Etape::Poison { piege, entite, degats } => serde_json::json!({
                    "quoi": "poison", "piege": piege, "entite": entite, "nom": au_sol[*piege].nom,
                    "degats": [degats.0, degats.1],
                }),
                Etape::Poussee { piege, entite, cases, percutee, degats } => serde_json::json!({
                    "quoi": "poussee", "piege": piege, "entite": entite,
                    "nom": piege.map_or_else(|| nom_du_sort.clone(), |p| au_sol[p].nom.clone()),
                    "cases": cases, "percutee": percutee, "degats": [degats, degats],
                }),
                Etape::Coup { degats } => serde_json::json!({
                    "quoi": "coup", "entite": 0, "nom": nom_du_sort, "degats": [degats.0, degats.1],
                }),
                // Le déplacement du sort se lit comme celui d'un piège : la
                // page le dessine et le raconte de la même façon.
                Etape::Amene { de, vers, voulu, fait, sens, arretee_par_piege } => serde_json::json!({
                    "quoi": "deplace", "piege": null, "entite": 0, "nom": nom_du_sort,
                    "sens": if *sens > 0 { "pousse" } else { "attire" },
                    "de": [de.x, de.y], "vers": [vers.x, vers.y],
                    "voulu": voulu, "fait": fait,
                    "bloquee": fait < voulu && !arretee_par_piege,
                    "arretee_par_piege": arretee_par_piege,
                }),
                Etape::Echange { cible, sram } => serde_json::json!({
                    "quoi": "echange", "nom": nom_du_sort,
                    "cible": [cible.x, cible.y], "sram": [sram.x, sram.y],
                }),
                Etape::Chakra { degats } => serde_json::json!({
                    "quoi": "chakra", "entite": 0, "degats": [degats.0, degats.1],
                    "element": chakra.map(|(e, _)| format!("{e:?}")),
                }),
                Etape::Fin { sur } => serde_json::json!({
                    "quoi": "fin", "case": [sur.x, sur.y],
                }),
                Etape::Coupee { apres } => serde_json::json!({
                    "quoi": "coupee", "apres": apres,
                }),
            }).collect::<Vec<_>>(),
        })
    });

    // Le générateur, quand on le lui demande : il complète le tour en cours avec
    // les PA qui lui restent, à partir de ce qui est déjà au sol.
    let tour = requete
        .tour
        .unwrap_or_else(|| requete.poses.iter().map(|p| p.tour).max().unwrap_or(1));
    let proposition = requete.proposer.then(|| {
        let Some((x, y)) = requete.entree else {
            return serde_json::json!({
                "tour": tour, "poses": [],
                "raison": "Placez d'abord la case d'entrée de l'ennemi.",
            });
        };
        let budget = budget.unwrap_or(u32::from(resolved.base_ap));
        let restants = budget.saturating_sub(depenses.get(&tour).copied().unwrap_or(0));
        let lances_du_tour: std::collections::BTreeMap<String, u8> = lances
            .iter()
            .filter(|((t, _), _)| *t == tour)
            .map(|((_, s), n)| (s.clone(), *n))
            .collect();
        let modeles = modeles_de(&ruleset, &snap, &resolved.profile, mult_distance, mult_melee);
        let terrain = Terrain {
            obstacles: &obstacles,
            lanceur,
            autres: &autres,
            ordre,
            chakra: chakra.map(|(_, vol)| vol),
            poussee: Poussee {
                niveau: resolved.profile.level,
                do_poussee: resolved.profile.push_damage,
            },
        };
        // Les pièges des tours d'après gardent leur case.
        let pieges: Vec<Case> = occupees
            .iter()
            .copied()
            .chain(requete.reservees.iter().map(|&(x, y)| Case::new(x, y)))
            .collect();
        let p = proposer(&Recherche {
            modeles: &modeles,
            au_sol: &au_sol,
            entree: Case::new(x, y),
            terrain: &terrain,
            pieges: &pieges,
            prises: &prises,
            lances: &lances_du_tour,
            variantes: &variantes,
            restants,
            largeur: LARGEUR_DU_FAISCEAU,
            plateau,
        });
        serde_json::json!({
            "tour": tour,
            "pa_du_tour": budget,
            "pa_restants": restants,
            "pa": p.pa,
            "avant": [p.avant.0, p.avant.1],
            "total": [p.total.0, p.total.1],
            "gain": [p.total.0 - p.avant.0, p.total.1 - p.avant.1],
            "essais": p.essais,
            "poses": p.poses.iter().map(|(m, c)| {
                let m = &modeles[*m];
                serde_json::json!({
                    "sort": m.sort, "nom": m.nom, "dofusdb_id": m.dofusdb_id,
                    "case": [c.x, c.y], "pa": m.exigences.pa,
                })
            }).collect::<Vec<_>>(),
        })
    });

    // Le concepteur, quand on le lui demande : un réseau entier, placé sur ce
    // plateau et avec ses murs, puis les cases d'où la cible le prend tout.
    let plan = requete.concevoir.map(|conception| {
        let modeles = modeles_de(&ruleset, &snap, &resolved.profile, mult_distance, mult_melee);
        let declencheurs: Vec<Declencheur> = DECLENCHEURS
            .iter()
            .filter_map(|s| Declencheur::de(&ruleset, &snap, s, &resolved.profile, mult_distance, mult_melee))
            .collect();
        // Les alliés et les autres ennemis restent où le joueur les a mis.
        let mut entites_posees: Vec<Entite> = Vec::new();
        for (camp, liste) in [(Camp::Allie, &requete.allies), (Camp::Ennemi, &requete.ennemis)] {
            for &(x, y) in liste {
                let case = Case::new(x, y);
                if !obstacles.contains(&case) && !entites_posees.iter().any(|e| e.case == case) {
                    entites_posees.push(Entite { camp, case });
                }
            }
        }
        let pa = budget.unwrap_or(u32::from(resolved.base_ap));
        let vol = chakra.map(|(_, vol)| vol);
        let poussee = Poussee {
            niveau: resolved.profile.level,
            do_poussee: resolved.profile.push_damage,
        };
        let (concu, essais) = concevoir(
            conception,
            &modeles,
            &declencheurs,
            plateau,
            &obstacles,
            &entites_posees,
            ordre,
            vol,
            poussee,
            pa,
        );
        let Some(c) = concu else {
            return serde_json::json!({
                "objectif": conception.objectif,
                "essais": essais,
                "raison": "Aucun réseau ne tient sur ce plateau avec ces PA : ajoutez des PA, ou retirez des obstacles.",
            });
        };
        let declencheur = c.sort.map(|i| &declencheurs[i]);
        // Les autres sorts d'entrée au même prix, qui frappent moins.
        let mut aussi: Vec<&Declencheur> = declencheur
            .map(|t| declencheurs.iter().filter(|u| u.pa == t.pa && u.sort != t.sort).collect())
            .unwrap_or_default();
        aussi.sort_by(|a, b| milieu(b.coup.distance).total_cmp(&milieu(a.coup.distance)));
        let fiche = |t: &Declencheur| {
            serde_json::json!({
                "sort": t.sort, "nom": t.nom, "dofusdb_id": t.dofusdb_id, "pa": t.pa,
                "degats": [t.coup.distance.0, t.coup.distance.1],
            })
        };
        // Les entrées : toutes les cases d'où la cible prend le réseau entier,
        // la meilleure d'abord. Celle d'où le concepteur l'a construit en est.
        let pieges: Vec<PiegePose> = c
            .plan
            .poses
            .iter()
            .enumerate()
            .map(|(k, (m, case, _))| modeles[*m].poser(k, *case))
            .collect();
        let terrain = Terrain {
            obstacles: &obstacles,
            lanceur: None,
            autres: &entites_posees,
            ordre,
            chakra: vol,
            poussee,
        };
        let tenues: Vec<Case> = obstacles
            .iter()
            .copied()
            .chain(entites_posees.iter().map(|e| e.case))
            .collect();
        let coup = declencheur.map_or((0, 0), |t| t.coup.distance);
        let entrees: Vec<(Case, (i64, i64))> = entrees_du_reseau(&pieges, &terrain, &tenues, &plateau, c.entree)
            .into_iter()
            .map(|(e, t)| (e, (t.0 + coup.0, t.1 + coup.1)))
            .collect();
        let (entree, total) = entrees.first().copied().unwrap_or((c.entree, c.plan.total));
        let (largeur, hauteur) = emprise(c.plan.poses.iter().map(|p| p.1), entree);
        // Les PA de chaque tour : ses pièges, et le sort au dernier.
        let mut pa_par_tour = vec![0u32; usize::from(c.plan.tours.max(1))];
        for (m, _, t) in &c.plan.poses {
            pa_par_tour[usize::from(*t - 1)] += modeles[*m].exigences.pa;
        }
        if let (Some(t), Some(dernier)) = (declencheur, pa_par_tour.last_mut()) {
            *dernier += t.pa;
        }
        serde_json::json!({
            "objectif": conception.objectif,
            "tours": c.plan.tours,
            "pa_du_tour": pa,
            "pa_par_tour": pa_par_tour,
            "entree": [entree.x, entree.y],
            "entrees": entrees.iter().map(|(e, t)| serde_json::json!({
                "case": [e.x, e.y], "total": [t.0, t.1],
            })).collect::<Vec<_>>(),
            "declencheur": declencheur.map(fiche),
            "aussi": aussi.into_iter().map(fiche).collect::<Vec<_>>(),
            "poses": c.plan.poses.iter().map(|(m, case, t)| {
                let m = &modeles[*m];
                serde_json::json!({
                    "sort": m.sort, "nom": m.nom, "dofusdb_id": m.dofusdb_id,
                    "case": [case.x, case.y], "pa": m.exigences.pa, "tour": t,
                })
            }).collect::<Vec<_>>(),
            "total": [total.0, total.1],
            "pa": c.plan.pa,
            "par_pa": if c.plan.pa > 0 { milieu(total) / f64::from(c.plan.pa) } else { 0.0 },
            "emprise": [largeur, hauteur],
            "encombrement": largeur * hauteur,
            "essais": essais,
        })
    });

    // Le catalogue des pièges de la classe, pour que la page ait de quoi
    // remplir sa palette sans un second aller-retour ni une liste en dur.
    let catalogue: Vec<serde_json::Value> = ruleset
        .spells
        .iter()
        .filter_map(|def| {
            let pg = piege(&snap, def.dofusdb_id?)?;
            // La palette montre la valeur À DISTANCE, le cas d'un piège qui
            // part loin du Sram. La chaîne, elle, choisit coup par coup.
            let (lignes, poison) = separer_poison(
                lignes_du_piege(pg.niveau, Case::new(0, 0), &resolved.profile, mult_distance, mult_melee),
                def.dofusdb_id?,
            );
            let d = declenchement(pg.niveau, Case::new(0, 0));
            let sur_place = lignes
                .iter()
                .filter(|l| {
                    l.cases
                        .as_ref()
                        .is_some_and(|z| z.contains(&Case::new(0, 0)))
                })
                .fold((0i64, 0i64), |(a, b), l| (a + l.degats.0, b + l.degats.1));
            Some(serde_json::json!({
                "sort": def.id,
                "nom": pg.nom,
                "dofusdb_id": def.dofusdb_id,
                "pa": pg.niveau.ap_cost,
                "portee": pg.niveau.range.and_then(|[a, b]| Some(vec![a?, b?])),
                "lancers_par_tour": pg.niveau.max_cast_per_turn,
                "declenchement": d.as_ref().map_or(1, Vec::len),
                // Les cases qui le déclenchent, autour de son centre : la page
                // les décale sous le curseur pour l'aperçu de pose. À défaut
                // de donnée, sa seule case, comme le fait la couverture.
                "gabarit": d.as_ref().map_or_else(
                    || vec![[0i16, 0i16]],
                    |v| v.iter().map(|c| [c.x, c.y]).collect(),
                ),
                "degats": [sur_place.0, sur_place.1],
                "poison": somme(&poison).map(|(a, b)| vec![a, b]),
                "elements": lignes.iter().map(|l| format!("{:?}", l.element))
                    .collect::<std::collections::BTreeSet<_>>(),
                "deplacement": deplacement(pg.niveau)
                    .map(|(sens, n)| serde_json::json!({ "sens": sens, "cases": n })),
                "texte": pg.texte,
            }))
        })
        .collect();

    let declencheur_json = declencheur.as_ref().map(|(t, depart, refus)| {
        serde_json::json!({
            "sort": t.sort, "nom": t.nom, "dofusdb_id": t.dofusdb_id, "pa": t.pa,
            "depart": [depart.x, depart.y],
            "mouvement": match t.mouvement {
                Mouvement::Pousse(n) => serde_json::json!({ "sens": "pousse", "cases": n }),
                Mouvement::Attire(n) => serde_json::json!({ "sens": "attire", "cases": n }),
                Mouvement::Echange => serde_json::json!({ "sens": "echange" }),
                Mouvement::JusquA(n) => serde_json::json!({ "sens": "pousse_jusqu_a", "cases": n }),
            },
            "degats": [t.coup.distance.0, t.coup.distance.1],
            "tour": tour_du_lancer,
            "refus": refus,
        })
    });

    Ok(serde_json::json!({
        "catalogue": catalogue,
        "declencheur": declencheur_json,
        "poses": poses,
        "cases": cases,
        "pa_total": pa_total,
        "meilleure_case": meilleure,
        "chaine": chaine_json,
        "tour": tour,
        "proposition": proposition,
        "plan": plan,
    })
    .to_string())
}

// ---------------------------------------------------------------------------
// La chaîne : ce qui part quand un ennemi entre dans le réseau.
// ---------------------------------------------------------------------------

/// Un piège tel que la chaîne le manipule, une fois la donnée lue.
#[derive(Debug, Clone, PartialEq)]
pub struct PiegePose {
    pub rang: usize,
    pub nom: String,
    pub centre: Case,
    pub declenchement: Vec<Case>,
    /// Les lignes de dégâts, chacune avec sa zone et ses cibles.
    pub effets: Vec<LigneDeDegats>,
    /// Ce que le piège déplace, quand il déplace.
    pub deplacement: Option<Deplacement>,
    /// ⚠️ Le piège déplace avant de frapper, dans l'ordre des effets de la
    /// donnée : le Répulsif pousse puis frappe, le Scélérat attire puis frappe,
    /// les autres frappent d'abord. Ce qu'un déplacement fait partir passe
    /// juste après lui, donc avant les dégâts du piège qui a poussé.
    pub deplace_avant: bool,
    /// Le poison de fin de tour que le piège applique avec ses dégâts, à
    /// distance. Il ne tombe pas pendant la chaîne et n'entre pas dans son
    /// total : la trace le signale, c'est tout.
    pub poison: Option<(i64, i64)>,
}

/// Une ligne de dégâts d'un piège posé.
#[derive(Debug, Clone, PartialEq)]
pub struct LigneDeDegats {
    pub zone: Vec<Case>,
    pub degats: Portees,
    /// Le masque de la donnée, dont les conditions comptent (`v50`, `V50`) ;
    /// quant au camp, un piège ne frappe que les ennemis. Voir [`frappe`].
    pub masque: String,
}

/// Le déplacement qu'un piège impose à ceux qu'il vise.
#[derive(Debug, Clone, PartialEq)]
pub struct Deplacement {
    /// `+1` pousse, `-1` attire.
    pub sens: i8,
    /// En cases.
    pub distance: i32,
    /// ⚠️ Sa zone propre, qui n'est pas celle du déclenchement. Le Scélérat
    /// part quand on entre sur sa seule case et attire sur trois cases dans
    /// chaque direction : il amène les AUTRES vers celui qui l'a déclenché.
    pub zone: Vec<Case>,
    pub masque: String,
}

/// Qui se tient sur le damier, vu du Sram qui a posé les pièges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Camp {
    Sram,
    Allie,
    Ennemi,
}

/// Une entité du damier, à sa case de départ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entite {
    pub camp: Camp,
    pub case: Case,
}

/// Le masque de cibles d'un effet retient-il une entité de ce camp ?
///
/// Le masque est une liste de jetons séparés par des virgules, chacun une
/// lettre suivie d'un nombre éventuel. Les lettres de camp forment un seul
/// groupe, dont une suffit : `a` et `h` le camp du Sram, Sram compris, `A` et
/// `H` le camp adverse, `g` les alliés hors le Sram, `c` et `C` le Sram
/// lui-même ; `i`, `I`, `j`, `J`, `s` et `S` n'écartent personne. Chaque
/// condition doit tenir à elle seule : `v50` plus de 50 % de vie, `V50` 50 % ou
/// moins, `e` un état porté. Toute autre lettre est sans effet.
///
/// ⚠️ Les entités de la chaîne sont à pleine vie et sans état. Les
/// déplacements des pièges visant tous `a,A`, un Sournois attire le Sram qui
/// se tient dans sa croix comme il attire un ennemi.
fn touche(masque: &str, camp: Camp) -> bool {
    const VIE: u32 = 100;
    let mut camp_exige = false;
    let mut camp_admis = false;
    for jeton in masque.split(',') {
        let mut lettres = jeton.chars();
        let Some(lettre) = lettres.next() else {
            continue;
        };
        let nombre: u32 = lettres.as_str().parse().unwrap_or(0);
        let admis = match lettre {
            'a' | 'h' => camp != Camp::Ennemi,
            'A' | 'H' => camp == Camp::Ennemi,
            'g' => camp == Camp::Allie,
            'c' | 'C' => camp == Camp::Sram,
            'i' | 'I' | 'j' | 'J' | 's' | 'S' => true,
            'v' if VIE <= nombre => return false,
            'V' if VIE > nombre => return false,
            'e' => return false,
            _ => continue,
        };
        camp_exige = true;
        camp_admis |= admis;
    }
    !camp_exige || camp_admis
}

/// Une ligne de dégâts de piège frappe-t-elle une entité de ce camp ?
///
/// ⚠️ Un piège ne frappe jamais le camp du Sram, même quand le masque de ses
/// dégâts porte `a,A` (Fangeux, Mortel, Funeste, anneaux du Piège à
/// Fragmentation). Les conditions du masque (`v50`, `V50`) comptent toujours.
/// Les déplacements, eux, visent bien `a,A` : voir [`touche`].
fn frappe(masque: &str, camp: Camp) -> bool {
    camp == Camp::Ennemi && touche(masque, camp)
}

/// Les dommages de poussée ne frappent, eux aussi, que les ennemis.
fn subit_la_poussee(camp: Camp) -> bool {
    camp == Camp::Ennemi
}

/// La place d'une case sur son anneau autour d'un centre, en tournant dans le
/// sens horaire de l'écran depuis la case du haut à droite. À `r` cases, la
/// case du haut à droite (0, -r) vaut 0, celle du bas à droite (r, 0) vaut r,
/// celle du bas à gauche (0, r) 2r et celle du haut à gauche (-r, 0) 3r.
fn index_horaire(dx: i16, dy: i16) -> i32 {
    let (x, y) = (i32::from(dx), i32::from(dy));
    let r = x.abs() + y.abs();
    if x >= 0 && y < 0 {
        x
    } else if x > 0 && y >= 0 {
        r + y
    } else if x <= 0 && y > 0 {
        2 * r - x
    } else if x < 0 && y <= 0 {
        3 * r - y
    } else {
        0
    }
}

/// Les entités qu'un effet touche, de la plus proche de son centre à la plus
/// lointaine, puis dans le sens horaire : la priorité de proximité, puis la
/// priorité horaire. Une poussée les prend à l'envers, sa priorité
/// contre-horaire : l'appelant retourne la liste.
fn dans_l_ordre(mut touchees: Vec<usize>, positions: &[Case], centre: Case) -> Vec<usize> {
    touchees.sort_by_key(|&i| {
        let p = positions[i];
        (centre.distance(p), index_horaire(p.x - centre.x, p.y - centre.y))
    });
    touchees
}

/// Les entités de la chaîne, dans l'ordre de leurs numéros : la cible qui
/// entre en `entree`, le Sram quand il est posé, puis les autres.
pub fn entites(entree: Case, terrain: &Terrain) -> Vec<Entite> {
    std::iter::once(Entite {
        camp: Camp::Ennemi,
        case: entree,
    })
    .chain(terrain.lanceur.map(|case| Entite {
        camp: Camp::Sram,
        case,
    }))
    .chain(terrain.autres.iter().copied())
    .collect()
}

/// Ce qu'un coup rend selon que la cible est au contact du Sram ou non.
///
/// Les `% Dommages mêlée` et les `% Dommages distance` ne s'appliquent pas au
/// même coup, et c'est l'écart entre le Sram et la cible AU MOMENT DU COUP qui
/// tranche : une case ou moins, mêlée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Portees {
    pub distance: (i64, i64),
    pub melee: (i64, i64),
}

impl Portees {
    /// La même valeur quelle que soit la portée, pour un build sans pourcentage
    /// de portée.
    pub fn uniques(degats: (i64, i64)) -> Self {
        Self {
            distance: degats,
            melee: degats,
        }
    }

    fn selon(self, contact: bool) -> (i64, i64) {
        if contact {
            self.melee
        } else {
            self.distance
        }
    }
}

/// Le terrain sur lequel la chaîne se déroule.
pub struct Terrain<'a> {
    /// Les cases infranchissables : murs et décor. Les entités n'y figurent
    /// pas, puisqu'elles bougent.
    pub obstacles: &'a [Case],
    /// Où se tient le Sram au départ. C'est l'écart entre lui et chaque entité
    /// frappée qui décide si le coup est de mêlée ou à distance, et ses pièges
    /// le déplacent comme n'importe qui. Absent, tout coup est à distance.
    pub lanceur: Option<Case>,
    /// Les autres entités du damier, alliés et ennemis.
    pub autres: &'a [Entite],
    pub ordre: Ordre,
    /// La cible porte la Concentration de Chakra, et voici son vol de vie.
    pub chakra: Option<Portees>,
    /// Ce que lisent les dommages de poussée.
    pub poussee: Poussee,
}

/// Le niveau du Sram et ses Dommages Poussée : tout ce que la formule des
/// dommages de poussée lit de lui, voir [`dofus_damage::degats_de_poussee`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Poussee {
    pub niveau: i32,
    pub do_poussee: i32,
}

/// Comment l'ennemi entre dans le réseau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrivee {
    /// Le joueur l'amène sur cette case, et il y entre.
    Entree(Case),
    /// Il se tient en `depart`, et un sort du Sram l'y fait entrer depuis la
    /// case du Sram : le sort le frappe (`coup`), puis le déplace. Sournoiserie,
    /// Perquisition, Guet-apens, Méprise.
    Sort {
        depart: Case,
        mouvement: Mouvement,
        coup: Portees,
    },
}

impl Arrivee {
    /// Où se tient la cible avant que rien ne parte.
    pub fn depart(&self) -> Case {
        match self {
            Arrivee::Entree(c) => *c,
            Arrivee::Sort { depart, .. } => *depart,
        }
    }
}

/// Ce que le sort de déclenchement fait à la cible, depuis la case du Sram :
/// la repousser ou l'attirer de tant de cases, ou prendre sa place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mouvement {
    Pousse(i32),
    Attire(i32),
    Echange,
    /// Peur : la repousser jusqu'à la case visée, à tant de cases. Elle
    /// s'arrête au premier piège qu'elle rencontre, comme toute poussée, et ne
    /// prend AUCUN dommage de poussée si elle bute avant.
    JusquA(i32),
}

/// Un effet de piège en attente de résolution.
///
/// ⚠️ Un effet, pas un piège : chaque effet d'un piège se résout à part, dans
/// l'ordre de la donnée, ce qui laisse ce qu'un déplacement déclenche passer
/// entre la poussée d'un Répulsif et ses propres dégâts.
#[derive(Debug, Clone, Copy)]
enum Action {
    /// Les dégâts d'un piège sur une entité. `case` est là où elle se tenait
    /// quand il est parti : c'est là que ses lignes la visent.
    Degats { piege: usize, entite: usize, case: Case },
    /// Le déplacement d'une entité, dans la direction fixée au départ du
    /// piège : de son centre vers `depuis`.
    Deplacement { piege: usize, entite: usize, depuis: Case },
}

/// Une étape de la chaîne, dans l'ordre où elle se produit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Etape {
    Declenche {
        piege: usize,
        sur: Case,
    },
    /// ⚠️ LA RÉSOLUTION A ÉTÉ COUPÉE. Ne doit jamais arriver : chaque effet
    /// résolu vient d'un piège parti, donc la chaîne se termine. Si cette étape
    /// paraît, c'est qu'une modification a rendu la résolution non bornée, et
    /// il vaut mieux une trace tronquée qui le DIT qu'un serveur qui tourne.
    Coupee {
        apres: usize,
    },
    /// `entite` suit la numérotation de [`entites`] : 0 est la cible.
    Degats {
        piege: usize,
        entite: usize,
        degats: (i64, i64),
    },
    Deplace {
        piege: usize,
        entite: usize,
        de: Case,
        vers: Case,
        voulu: i32,
        fait: i32,
        /// La poussée s'est arrêtée en ENTRANT dans la zone d'un piège, et non
        /// contre un obstacle. Les deux raccourcissent un déplacement, mais
        /// seul l'obstacle occasionne des dommages de poussée.
        arretee_par_piege: bool,
    },
    /// Les dommages de poussée d'une poussée qui a buté, sur l'entité poussée,
    /// ou sur celle qu'elle a percutée (`percutee`, divisés par huit). Sans
    /// piège, c'est le sort de déclenchement qui a poussé.
    Poussee {
        piege: Option<usize>,
        entite: usize,
        cases: i32,
        percutee: bool,
        degats: i64,
    },
    /// Le poison de fin de tour que le piège applique avec ses dégâts. Hors du
    /// total de la chaîne : il tombe plus tard.
    Poison {
        piege: usize,
        entite: usize,
        degats: (i64, i64),
    },
    /// Le vol de vie de la Concentration de Chakra, que déclenche le dégât de
    /// piège qui le précède. Seule la cible porte l'état.
    Chakra {
        degats: (i64, i64),
    },
    /// Plus rien ne bouge et plus rien ne part ; `sur` est la case où la
    /// cible finit.
    Fin {
        sur: Case,
    },
    /// Les dégâts du sort qui fait entrer la cible dans le réseau, avant
    /// qu'elle bouge.
    Coup {
        degats: (i64, i64),
    },
    /// Le déplacement que ce sort impose à la cible, depuis la case du Sram :
    /// `sens` positif la repousse, négatif l'attire.
    Amene {
        de: Case,
        vers: Case,
        voulu: i32,
        fait: i32,
        sens: i8,
        arretee_par_piege: bool,
    },
    /// La cible et le Sram échangent leurs places : la cible en `cible`, le
    /// Sram en `sram`.
    Echange {
        cible: Case,
        sram: Case,
    },
}

/// Dans quel ordre partent plusieurs pièges qui couvrent la même case.
///
/// ⚠️ L'ordre de pose décide : le premier posé résout le premier. On pose donc
/// les pièges dans l'ordre des déplacements voulus, et l'ordre de pose est le
/// scénario. Les deux autres ordres restent pour qu'un test montre que l'ordre
/// change vraiment le résultat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ordre {
    /// Du plus ancien posé au plus récent : la règle du jeu.
    #[default]
    Pose,
    /// Du plus récent au plus ancien.
    PoseInverse,
    /// Balayage horaire autour de la case d'entrée, en partant de l'est.
    Horaire,
}

/// L'angle d'un piège vu depuis la case où l'ennemi vient d'entrer, en
/// huitièmes de tour, pour trier en sens horaire depuis l'est.
///
/// Un piège dont le centre EST la case d'entrée n'a pas d'angle : il passe en
/// premier, faute de direction pour le classer.
fn angle_horaire(centre: Case, depuis: Case) -> i32 {
    let (dx, dy) = (centre.x - depuis.x, centre.y - depuis.y);
    if dx == 0 && dy == 0 {
        return -1;
    }
    // L'écran tourne le repère de 45 degrés ; l'angle se mesure donc dans le
    // repère du jeu, où les quatre directions sont les axes.
    match (dx.signum(), dy.signum()) {
        (1, 0) => 0,
        (1, 1) => 1,
        (0, 1) => 2,
        (-1, 1) => 3,
        (-1, 0) => 4,
        (-1, -1) => 5,
        (0, -1) => 6,
        _ => 7,
    }
}

/// Ce qui se produit quand un ennemi entre sur `entree`.
///
/// ⚠️ Un piège ne part qu'une fois : déclenché, il disparaît. Chaque effet
/// résolu vient d'un piège parti, ce qui borne la chaîne à la taille du réseau
/// et assure qu'elle se termine.
///
/// Tous les pièges qui couvrent une case partent avant que leurs effets ne se
/// résolvent. Une cible déplacée traverse les cases une à une et déclenche ce
/// qu'elle croise.
///
/// ⚠️ Un piège arrête un déplacement : la cible qui entre dans la zone de
/// déclenchement d'un piège encore posé s'y arrête, et ce piège part. Le groupe
/// rencontré passe avant le reste du groupe précédent.
///
/// Une poussée qui bute sur un obstacle ou une entité s'arrête, et la trace le
/// dit en comparant `voulu` et `fait`.
pub fn chaine(pieges: &[PiegePose], entree: Case, terrain: &Terrain) -> Vec<Etape> {
    chaine_arrivee(pieges, Arrivee::Entree(entree), terrain)
}

/// La chaîne, quelle que soit la façon dont l'ennemi entre dans le réseau.
pub fn chaine_arrivee(pieges: &[PiegePose], arrivee: Arrivee, terrain: &Terrain) -> Vec<Etape> {
    chaine_arrivee_bornee(pieges, arrivee, terrain, pieges.len() * 8 + 64)
}

/// La même, avec son plafond en clair : un paramètre, pour qu'un test vérifie
/// le garde-fou sans provoquer de chaîne infinie.
pub fn chaine_bornee(pieges: &[PiegePose], entree: Case, terrain: &Terrain, plafond: usize) -> Vec<Etape> {
    chaine_arrivee_bornee(pieges, Arrivee::Entree(entree), terrain, plafond)
}

fn chaine_arrivee_bornee(pieges: &[PiegePose], arrivee: Arrivee, terrain: &Terrain, plafond: usize) -> Vec<Etape> {
    let depart = entites(arrivee.depart(), terrain);
    let camps: Vec<Camp> = depart.iter().map(|e| e.camp).collect();
    let mut chaine = Resolution {
        pieges,
        terrain,
        sram: camps.iter().position(|c| *c == Camp::Sram),
        camps,
        positions: depart.iter().map(|e| e.case).collect(),
        partis: vec![false; pieges.len()],
        trace: Vec::new(),
        resolus: 0,
        plafond,
    };
    let effets = match arrivee {
        Arrivee::Entree(entree) => chaine.declencher(entree),
        Arrivee::Sort { depart, mouvement, coup } => chaine.amener(depart, mouvement, coup),
    };
    // ⚠️ Pas de `Fin` après une coupe : une trace coupée qui finirait « sur
    // telle case » se lirait comme une chaîne résolue.
    if chaine.resoudre(effets) {
        chaine.trace.push(Etape::Fin { sur: chaine.positions[0] });
    }
    chaine.trace
}

/// Une chaîne en cours : où chacun se tient, ce qui est parti, et la trace de
/// ce qui s'est produit.
struct Resolution<'a> {
    pieges: &'a [PiegePose],
    terrain: &'a Terrain<'a>,
    camps: Vec<Camp>,
    positions: Vec<Case>,
    sram: Option<usize>,
    partis: Vec<bool>,
    trace: Vec<Etape>,
    /// Les effets résolus, et combien la chaîne en admet avant de se couper :
    /// un garde-fou, la chaîne se terminant d'elle-même.
    resolus: usize,
    plafond: usize,
}

impl Resolution<'_> {
    /// Les pièges encore posés qui couvrent `case` partent ensemble, dans
    /// l'ordre choisi ; leurs effets suivent, piège après piège.
    fn declencher(&mut self, case: Case) -> Vec<Action> {
        let pieges = self.pieges;
        let mut partants: Vec<usize> = (0..pieges.len())
            .filter(|&i| !self.partis[i] && pieges[i].declenchement.contains(&case))
            .collect();
        match self.terrain.ordre {
            Ordre::Pose => partants.sort_by_key(|&i| pieges[i].rang),
            Ordre::PoseInverse => partants.sort_by_key(|&i| std::cmp::Reverse(pieges[i].rang)),
            Ordre::Horaire => partants.sort_by_key(|&i| (angle_horaire(pieges[i].centre, case), pieges[i].rang)),
        }
        for &i in &partants {
            self.partis[i] = true;
            self.trace.push(Etape::Declenche { piege: i, sur: case });
        }
        partants.into_iter().flat_map(|i| self.effets_du_piege(i)).collect()
    }

    /// Les effets d'un piège qui part, dans l'ordre de la donnée, leurs cibles
    /// prises là où chacun se tient à cet instant : ses dégâts sur chaque
    /// entité qu'une de ses lignes frappe, son déplacement sur chaque entité
    /// que sa zone vise. Une entité posée sur le centre n'a pas de direction et
    /// ne bouge pas.
    fn effets_du_piege(&self, piege: usize) -> Vec<Action> {
        let p = &self.pieges[piege];
        let positions = &self.positions;
        let frappees = dans_l_ordre(
            (0..positions.len())
                .filter(|&j| {
                    p.effets
                        .iter()
                        .any(|l| l.zone.contains(&positions[j]) && frappe(&l.masque, self.camps[j]))
                })
                .collect(),
            positions,
            p.centre,
        );
        let deplacees = p.deplacement.as_ref().map_or_else(Vec::new, |d| {
            let mut visees = dans_l_ordre(
                (0..positions.len())
                    .filter(|&j| {
                        positions[j] != p.centre && d.zone.contains(&positions[j]) && touche(&d.masque, self.camps[j])
                    })
                    .collect(),
                positions,
                p.centre,
            );
            if d.sens > 0 {
                visees.reverse();
            }
            visees
        });
        let degats = frappees
            .into_iter()
            .map(|entite| Action::Degats { piege, entite, case: positions[entite] });
        let deplacements = deplacees
            .into_iter()
            .map(|entite| Action::Deplacement { piege, entite, depuis: positions[entite] });
        if p.deplace_avant {
            deplacements.chain(degats).collect()
        } else {
            degats.chain(deplacements).collect()
        }
    }

    /// Un sort du Sram fait entrer la cible dans le réseau : il la frappe là
    /// où elle se tient, puis la déplace depuis la case du Sram. Rend les
    /// effets des pièges qu'elle fait partir.
    fn amener(&mut self, depart: Case, mouvement: Mouvement, coup: Portees) -> Vec<Action> {
        let lanceur = self.sram.map(|s| self.positions[s]);
        let contact = lanceur.is_some_and(|l| l.distance(depart) <= 1);
        self.trace.push(Etape::Coup {
            degats: coup.selon(contact),
        });
        match (mouvement, lanceur, self.sram) {
            (Mouvement::Pousse(voulu) | Mouvement::Attire(voulu) | Mouvement::JusquA(voulu), Some(l), _) => {
                let sens: i8 = if matches!(mouvement, Mouvement::Attire(_)) { -1 } else { 1 };
                let t = trajet(depart, l, depart, sens, voulu, &self.barrees(0), &self.zones_posees());
                self.positions[0] = t.vers;
                let arretee_par_piege = t.arretee_par_piege(voulu);
                self.trace.push(Etape::Amene {
                    de: depart,
                    vers: t.vers,
                    voulu,
                    fait: t.fait,
                    sens,
                    arretee_par_piege,
                });
                // Peur ne fait pas de dommages de poussée.
                if matches!(mouvement, Mouvement::Pousse(_)) && t.restantes > 0 && !arretee_par_piege {
                    self.dommages_de_poussee(None, 0, t.restantes, t.devant);
                }
                t.croisees.into_iter().flat_map(|c| self.declencher(c)).collect()
            }
            (Mouvement::Echange, Some(l), Some(s)) => {
                // ⚠️ À vérifier en jeu : la cible amenée par échange déclenche
                // les pièges de sa nouvelle case, comme une cible qui y entre ;
                // le Sram, ceux de la sienne.
                self.positions[0] = l;
                self.positions[s] = depart;
                self.trace.push(Etape::Echange { cible: l, sram: depart });
                let mut effets = self.declencher(l);
                effets.extend(self.declencher(depart));
                effets
            }
            // Sans Sram sur le damier, rien ne déplace la cible.
            _ => Vec::new(),
        }
    }

    /// Résout des effets dans l'ordre, et ce qu'un déplacement fait partir
    /// avant tout ce qui reste, les effets suivants du piège qui a poussé
    /// compris. Rend `false` quand la chaîne a été coupée.
    fn resoudre(&mut self, effets: Vec<Action>) -> bool {
        for effet in effets {
            self.resolus += 1;
            if self.resolus > self.plafond {
                self.trace.push(Etape::Coupee { apres: self.plafond });
                return false;
            }
            let partis = match effet {
                Action::Degats { piege, entite, case } => {
                    self.infliger(piege, entite, case);
                    Vec::new()
                }
                Action::Deplacement { piege, entite, depuis } => self.deplacer(piege, entite, depuis),
            };
            if !partis.is_empty() && !self.resoudre(partis) {
                return false;
            }
        }
        true
    }

    /// Les dégâts d'un piège sur une entité, lus sur la case où elle se tenait
    /// quand il est parti : on n'échappe pas à un piège en passant dessus vite.
    /// Mêlée ou distance se juge au moment du coup, sur l'écart entre le Sram et
    /// l'entité là où ils sont : un Répulsif qui pousse avant de frapper juge la
    /// portée après la poussée.
    fn infliger(&mut self, piege: usize, entite: usize, case: Case) {
        let pieges = self.pieges;
        let p = &pieges[piege];
        let contact = self
            .sram
            .is_some_and(|s| self.positions[s].distance(self.positions[entite]) <= 1);
        let subi = p
            .effets
            .iter()
            .filter(|l| l.zone.contains(&case) && frappe(&l.masque, self.camps[entite]))
            .map(|l| l.degats.selon(contact))
            .fold((0i64, 0i64), |(a, b), d| (a + d.0, b + d.1));
        if subi == (0, 0) {
            return;
        }
        self.trace.push(Etape::Degats {
            piege,
            entite,
            degats: subi,
        });
        // La Concentration de Chakra vole de la vie à chaque dégât de piège que
        // subit la cible, seule à porter l'état.
        if let Some(vol) = self.terrain.chakra.filter(|_| entite == 0) {
            self.trace.push(Etape::Chakra {
                degats: vol.selon(contact),
            });
        }
        if let Some(degats) = p.poison {
            self.trace.push(Etape::Poison { piege, entite, degats });
        }
    }

    /// Le déplacement qu'un piège impose à une entité ; rend les effets des
    /// pièges qu'il fait partir. N'importe qui déclenche ce qu'il traverse, un
    /// allié ou le Sram compris.
    fn deplacer(&mut self, piege: usize, entite: usize, depuis: Case) -> Vec<Action> {
        let pieges = self.pieges;
        let p = &pieges[piege];
        let Some(d) = &p.deplacement else {
            return Vec::new();
        };
        let de = self.positions[entite];
        let t = trajet(de, p.centre, depuis, d.sens, d.distance, &self.barrees(entite), &self.zones_posees());
        self.positions[entite] = t.vers;
        let arretee_par_piege = t.arretee_par_piege(d.distance);
        self.trace.push(Etape::Deplace {
            piege,
            entite,
            de,
            vers: t.vers,
            voulu: d.distance,
            fait: t.fait,
            arretee_par_piege,
        });
        // Une poussée qui bute sur un obstacle ou une entité frappe ; une
        // attirance ou une poussée qu'un piège arrête, non.
        if d.sens > 0 && t.restantes > 0 && !arretee_par_piege {
            self.dommages_de_poussee(Some(piege), entite, t.restantes, t.devant);
        }
        t.croisees.into_iter().flat_map(|c| self.declencher(c)).collect()
    }

    /// Les dommages de poussée d'une poussée qui a buté, pour `cases` cases
    /// manquées : sur l'entité poussée, puis, divisés par huit, sur celle
    /// qu'elle a percutée en `devant`.
    fn dommages_de_poussee(&mut self, piege: Option<usize>, entite: usize, cases: i32, devant: Option<Case>) {
        let poussee = self.terrain.poussee;
        let formule = |percutee| {
            i64::from(dofus_damage::degats_de_poussee(
                poussee.niveau,
                poussee.do_poussee,
                0,
                cases,
                percutee,
            ))
        };
        if subit_la_poussee(self.camps[entite]) {
            self.trace.push(Etape::Poussee {
                piege,
                entite,
                cases,
                percutee: false,
                degats: formule(false),
            });
        }
        let percutee = devant.and_then(|c| (0..self.positions.len()).find(|&j| j != entite && self.positions[j] == c));
        if let Some(autre) = percutee.filter(|&a| subit_la_poussee(self.camps[a])) {
            self.trace.push(Etape::Poussee {
                piege,
                entite: autre,
                cases,
                percutee: true,
                degats: formule(true),
            });
        }
    }

    /// Ce qui barre le chemin de `entite` : les obstacles, et toute autre
    /// entité là où elle se tient.
    fn barrees(&self, entite: usize) -> Vec<Case> {
        self.terrain
            .obstacles
            .iter()
            .copied()
            .chain(
                self.positions
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != entite)
                    .map(|(_, c)| *c),
            )
            .collect()
    }

    /// Les zones de déclenchement des pièges encore posés : ceux déjà partis
    /// n'arrêtent plus rien.
    fn zones_posees(&self) -> Vec<Case> {
        (0..self.pieges.len())
            .filter(|&j| !self.partis[j])
            .flat_map(|j| self.pieges[j].declenchement.iter().copied())
            .collect()
    }
}

/// Ce qu'un déplacement a fait.
struct Trajet {
    vers: Case,
    /// La distance parcourue, en cases, comme le sort l'annonce.
    fait: i32,
    /// Les cases traversées, dans l'ordre.
    croisees: Vec<Case>,
    /// Arrêté en entrant dans la zone d'un piège encore posé.
    par_piege: bool,
    /// Ce qui reste à parcourir quand le chemin est barré, en cases : deux par
    /// pas diagonal.
    restantes: i32,
    /// La case qui barre le chemin.
    devant: Option<Case>,
}

impl Trajet {
    /// Un piège n'a arrêté le déplacement que s'il l'a raccourci.
    fn arretee_par_piege(&self, voulu: i32) -> bool {
        self.par_piege && self.fait < voulu
    }
}

/// Déplacer une entité depuis `position` : une poussée l'écarte de `centre`
/// dans la direction de `depuis`, une attirance (`sens` négatif) l'en
/// rapproche.
///
/// ⚠️ La direction se fige au départ du piège, de son centre vers la case où
/// la cible se tenait, puis s'applique depuis là où la cible est rendue : deux
/// impulsions s'additionnent. Une cible qui se tenait sur le centre n'a pas de
/// direction et ne bouge pas.
///
/// ⚠️ À écarts égaux, le déplacement est diagonal (l'Insidieux attire en
/// diagonale) : un pas couvre deux cases, et les deux cases d'angle doivent
/// être libres. Sinon il suit l'axe du plus grand écart.
///
/// La cible entre dans une case piégée puis s'y arrête ; devant ce qui barre le
/// chemin, elle s'arrête sans y entrer.
fn trajet(
    position: Case,
    centre: Case,
    depuis: Case,
    sens: i8,
    distance: i32,
    barrees: &[Case],
    pieges_poses: &[Case],
) -> Trajet {
    let mut t = Trajet {
        vers: position,
        fait: 0,
        croisees: Vec::new(),
        par_piege: false,
        restantes: 0,
        devant: None,
    };
    let (dx, dy) = (depuis.x - centre.x, depuis.y - centre.y);
    if dx == 0 && dy == 0 {
        return t;
    }
    let diagonal = dx.abs() == dy.abs();
    let (mut ux, mut uy) = if diagonal {
        (dx.signum(), dy.signum())
    } else if dx.abs() > dy.abs() {
        (dx.signum(), 0)
    } else {
        (0, dy.signum())
    };
    if sens < 0 {
        (ux, uy) = (-ux, -uy);
    }
    let distance = distance.max(0);
    let largeur = if diagonal { 2 } else { 1 };
    let pas_voulus = (distance + largeur - 1) / largeur;
    let libre = |c: Case| !barrees.contains(&c);
    while (t.croisees.len() as i32) < pas_voulus {
        let ici = t.vers;
        let suivante = Case::new(ici.x + ux, ici.y + uy);
        let angles_libres = !diagonal || (libre(Case::new(suivante.x, ici.y)) && libre(Case::new(ici.x, suivante.y)));
        if !libre(suivante) || !angles_libres {
            t.devant = Some(suivante);
            break;
        }
        t.vers = suivante;
        t.croisees.push(suivante);
        if pieges_poses.contains(&suivante) {
            t.par_piege = true;
            break;
        }
    }
    let faits = t.croisees.len() as i32;
    t.fait = if faits == pas_voulus { distance } else { faits * largeur };
    t.restantes = (pas_voulus - faits) * largeur;
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requete(poses: &[(&str, (i16, i16))]) -> RequeteReseau {
        serde_json::from_value(serde_json::json!({
            "class": 4,
            "level": 200,
            // Plusieurs caractéristiques : avec la seule Agilité, les lignes
            // Terre et Feu des pièges resteraient à leur valeur de base, et un
            // test qui compare des dégâts ne verrait pas qu'elles ne sont pas résolues.
            "invested": { "agility": 300, "strength": 300, "intelligence": 200, "chance": 200 },
            "lanceur": [0, 0],
            "poses": poses.iter()
                .map(|(s, c)| serde_json::json!({ "sort": s, "case": [c.0, c.1] }))
                .collect::<Vec<_>>(),
        }))
        .expect("la requête de test doit se lire")
    }

    fn reseau(r: &RequeteReseau) -> serde_json::Value {
        serde_json::from_str(&reseau_json(r).expect("le réseau doit se calculer")).unwrap()
    }

    /// La règle de la case unique vient de la donnée, et ce test l'y lit : les poses
    /// de la classe portent toutes `needs_free_trap_cell`. Le jour où une pose ne le
    /// porterait plus, ce test tombe et la règle doit être revue.
    #[test]
    fn aucune_pose_de_la_classe_n_accepte_une_case_deja_piegee() {
        let snap = snapshot_for(4).expect("l'instantané du Sram");
        let mut vues = 0;
        for spell in &snap.spells {
            for niveau in &spell.levels {
                if niveau.placed_lines.is_empty() {
                    continue;
                }
                let Some(cast) = niveau.cast else { continue };
                vues += 1;
                assert!(
                    cast.needs_free_trap_cell,
                    "{} pose sans exiger une case libre : la règle de la case \
                     unique ne vaut plus pour tout le monde",
                    spell.name.fr
                );
            }
        }
        assert!(vues >= 20, "contrôle trop maigre : {vues} poses lues");
    }

    #[test]
    fn deux_pieges_sur_la_meme_case_le_second_est_refuse_et_ne_compte_pas() {
        let seul = reseau(&requete(&[("piege_sournois", (2, 0))]));
        let double = reseau(&requete(&[
            ("piege_sournois", (2, 0)),
            ("piege_mortel", (2, 0)),
        ]));
        assert!(double["poses"][0]["refus"].is_null());
        assert_eq!(
            double["poses"][1]["refus"].as_str(),
            Some("deux pièges ne partagent pas leur case")
        );
        // ⚠️ ET SURTOUT : il ne compte nulle part. Une pose refusée qui
        // alimenterait la couverture donnerait des dégâts qu'aucun tour ne peut
        // produire.
        assert_eq!(double["pa_total"], seul["pa_total"]);
        assert_eq!(
            double["meilleure_case"]["degats"], seul["meilleure_case"]["degats"],
            "le piège refusé ne doit rien ajouter à la meilleure case"
        );
    }

    /// Les dégâts d'une case sont la somme de ceux des pièges qui la couvrent.
    ///
    /// Recalculés ici depuis la liste des poses, sans relire le total que le
    /// module annonce : deux chemins pour le même nombre.
    #[test]
    fn les_degats_d_une_case_sont_la_somme_des_pieges_qui_la_couvrent() {
        // Trois pièges qui ne déclenchent pas que sur leur centre, pour que
        // leurs zones se recouvrent ; l'Insidieux et la Dérive portent une
        // croix diagonale.
        let v = reseau(&requete(&[
            ("piege_sournois", (2, 0)),
            ("piege_repulsif", (3, 0)),
            ("piege_insidieux", (2, 1)),
        ]));
        // Recalculé depuis les LIGNES, pas depuis le total par pose : chaque
        // ligne a sa zone, et ces trois pièges n'en ont qu'une chacune.
        let par_pose: Vec<(i64, i64)> = v["poses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                p["lignes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .fold((0i64, 0i64), |(a, b), l| {
                        (
                            a + l["degats"][0].as_i64().unwrap(),
                            b + l["degats"][1].as_i64().unwrap(),
                        )
                    })
            })
            .collect();
        let mut verifiees = 0;
        for c in v["cases"].as_array().unwrap() {
            let attendu = c["pieges"]
                .as_array()
                .unwrap()
                .iter()
                .fold((0i64, 0i64), |(a, b), r| {
                    let (lo, hi) = par_pose[r.as_u64().unwrap() as usize];
                    (a + lo, b + hi)
                });
            assert_eq!(
                c["degats"][0].as_i64().unwrap(),
                attendu.0,
                "case {}",
                c["case"]
            );
            assert_eq!(
                c["degats"][1].as_i64().unwrap(),
                attendu.1,
                "case {}",
                c["case"]
            );
            verifiees += 1;
        }
        assert!(verifiees >= 10, "seulement {verifiees} cases vérifiées");

        // Et le recouvrement existe VRAIMENT : sans case couverte par plus d'un
        // piège, le test ci-dessus passerait sur un réseau sans intérêt.
        let multiples = v["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["pieges"].as_array().unwrap().len() > 1)
            .count();
        assert!(
            multiples >= 3,
            "seulement {multiples} cases couvertes plusieurs fois"
        );
    }

    /// La Portée du build allonge les pièges, tous à portée modifiable : le
    /// Piège Sournois porte à 8 cases, à 10 avec 2 de Portée.
    #[test]
    fn la_portee_du_build_allonge_les_pieges() {
        let refus = |r: &RequeteReseau| reseau(r)["poses"][0]["refus"].as_str().map(str::to_string);
        let mut sournois = requete(&[("piege_sournois", (10, 0))]);
        assert_eq!(refus(&sournois).as_deref(), Some("hors de la portée du sort"));
        sournois.build.invested.insert("range".into(), 2);
        assert_eq!(refus(&sournois), None);
    }

    /// Sournoiserie n'est pas à portée modifiable et garde ses 8 cases, sauf
    /// sous la Cape Routh, qui la rend modifiable : l'objet de classe passe
    /// avant la Portée du build.
    #[test]
    fn la_portee_du_build_attend_une_portee_modifiable() {
        let portee = |objets: &[u32]| {
            let mut build = dofus_build::BuildInput { class: 4, level: 200, ..Default::default() };
            build.items = objets.to_vec();
            let resolu = resolve_build(&build).unwrap();
            let mut snap = snapshot_for(4).unwrap();
            crate::objets_de_classe::sur_l_instantane(&mut snap, &resolu);
            avec_la_portee_du_build(&mut snap, 2);
            let niveau = |id: u32| snap.spells.iter().find(|s| s.id == id).and_then(|s| s.levels.last()).unwrap().range;
            (niveau(12904), niveau(12906))
        };
        assert_eq!(portee(&[]), (Some([Some(1), Some(8)]), Some([Some(1), Some(10)])), "Sournoiserie, Piège Sournois");
        assert_eq!(portee(&[8641]).0, Some([Some(1), Some(10)]), "Sournoiserie sous la Cape Routh");
    }

    #[test]
    fn une_pose_hors_de_portee_est_refusee() {
        // Le Piège Sournois porte de 1 à 8 cases. À douze, il ne part pas.
        let v = reseau(&requete(&[("piege_sournois", (12, 0))]));
        assert_eq!(
            v["poses"][0]["refus"].as_str(),
            Some("hors de la portée du sort")
        );
        assert_eq!(v["pa_total"], 0);
        assert!(v["cases"].as_array().unwrap().is_empty());
    }

    /// La meilleure case est bien la plus chère du tableau.
    #[test]
    fn la_meilleure_case_est_le_maximum_du_tableau() {
        let v = reseau(&requete(&[
            ("piege_sournois", (2, 0)),
            ("piege_effroyable", (3, 0)),
            ("piege_scelerat", (2, 1)),
        ]));
        let max = v["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["degats"][1].as_i64().unwrap())
            .max()
            .unwrap();
        assert_eq!(v["meilleure_case"]["degats"][1].as_i64().unwrap(), max);
    }

    /// Deux lignes de même zone et de masques différents ne s'additionnent pas : le
    /// Piège Mortel porte deux lignes Terre sur la même case, 39-43 sous `a,A,v50`
    /// et 49-54 sous `a,A,V50`, deux variantes selon la vie de la cible. La plus
    /// basse, celle d'une cible pleine, est retenue.
    #[test]
    fn le_piege_mortel_retient_sa_variante_basse_et_ne_somme_pas() {
        let v = reseau(&requete(&[("piege_mortel", (2, 0))]));
        let lignes = v["poses"][0]["lignes"].as_array().unwrap();
        assert_eq!(
            lignes.len(),
            1,
            "une seule des deux variantes est retenue : {lignes:?}"
        );

        // La retenue est la plus basse. On refait le calcul depuis
        // l'instantané, pour ne pas se contenter de relire le chiffre rendu.
        let snap = snapshot_for(4).unwrap();
        let spell = snap
            .spells
            .iter()
            .find(|s| s.name.fr == "Piège Mortel")
            .expect("le Piège Mortel");
        let niveau = spell
            .levels
            .iter()
            .max_by_key(|l| l.grade.unwrap_or(0))
            .unwrap();
        let bases: Vec<i32> = niveau.placed_lines.iter().map(|l| l.range.1).collect();
        assert_eq!(
            bases.len(),
            2,
            "la donnée porte bien deux lignes : {bases:?}"
        );
        assert!(bases[0] != bases[1], "et elles diffèrent : {bases:?}");

        let rendu = lignes[0]["degats"][1].as_i64().unwrap();
        let somme = v["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["degats"][1].as_i64().unwrap())
            .max()
            .unwrap();
        assert_eq!(
            rendu, somme,
            "la case du piège prend la variante, pas la somme"
        );

        // Et c'est la plus basse, pas simplement l'une des deux : les
        // assertions précédentes passeraient aussi avec la haute.
        let resolved = resolve_build(&requete(&[]).build.normalise()).unwrap();
        let resoudre = |base: i32| {
            dofus_damage::range(
                &SpellLine {
                    element: Element::Earth,
                    normal: (base, base),
                    critical: (base, base),
                },
                &resolved.profile,
                FinalMultiplier::NEUTRAL,
                false,
                &Resistance::NONE,
            )
            .1
        };
        let possibles: Vec<i64> = bases.iter().map(|b| resoudre(*b)).collect();
        assert_eq!(
            rendu,
            *possibles.iter().min().unwrap(),
            "la variante retenue doit être la plus BASSE des deux : {possibles:?}"
        );
        assert!(
            possibles.iter().max() > possibles.iter().min(),
            "les deux variantes doivent vraiment différer : {possibles:?}"
        );
        // Et l'écartée est rendue, pas perdue : une cible sous la moitié de
        // sa vie prend davantage.
        assert_eq!(
            lignes[0]["variante_ecartee"][1].as_i64().unwrap(),
            *possibles.iter().max().unwrap()
        );
        assert!(
            v["poses"][0]["texte"]
                .as_str()
                .unwrap_or("")
                .contains("moins de 50%"),
            "le texte du jeu doit accompagner la variante"
        );
    }

    /// Les anneaux du Piège à Fragmentation ne s'additionnent pas non plus : quatre
    /// lignes de zones `P`, `O1`, `O2` et `O3`, le centre puis trois anneaux, et une
    /// case n'appartient qu'à l'un d'eux.
    #[test]
    fn les_anneaux_du_piege_a_fragmentation_ne_se_cumulent_pas() {
        let v = reseau(&requete(&[("piege_a_fragmentation", (5, 0))]));
        let pose = &v["poses"][0];
        let lignes = pose["lignes"].as_array().unwrap();
        assert_eq!(lignes.len(), 4, "centre plus trois anneaux");
        let somme_des_quatre: i64 = lignes
            .iter()
            .map(|l| l["degats"][1].as_i64().unwrap())
            .sum();

        // Sa zone d'EFFET couvre le centre et les trois anneaux.
        assert_eq!(pose["cases"].as_array().unwrap().len(), 25);
        // Son DÉCLENCHEMENT, lui, tient sur une case : le sort se dit
        // mono-cellule, et le confondre avec la zone d'effet le rendait
        // vingt-cinq fois plus facile à declencher qu'il ne l'est.
        assert_eq!(pose["mono_cellule"], true);
        assert_eq!(pose["declenchement"].as_array().unwrap().len(), 1);

        let cases = v["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 1, "une seule case déclenche : {cases:?}");
        let d = cases[0]["degats"][1].as_i64().unwrap();
        assert!(
            d < somme_des_quatre,
            "la case centrale prend {d}, soit les quatre lignes cumulées"
        );
        // Et ce qu'elle prend est la ligne du CENTRE, pas un anneau.
        let au_centre = lignes.iter().find(|l| l["zone"] == "P1").unwrap()["degats"][1]
            .as_i64()
            .unwrap();
        assert_eq!(d, au_centre);
    }

    /// Un piège mono-cellule ne se déclenche que sur sa case, et ses effets
    /// s'étendent ensuite sur sa zone : quatre pièges du Sram le disent dans leur
    /// texte.
    #[test]
    fn un_piege_mono_cellule_ne_se_declenche_que_sur_sa_case() {
        let v = reseau(&requete(&[("piege_effroyable", (2, 0))]));
        let pose = &v["poses"][0];
        assert_eq!(pose["mono_cellule"], true);
        assert_eq!(
            pose["cases"].as_array().unwrap().len(),
            13,
            "sa zone d'effet reste la croix de rayon 3"
        );
        assert_eq!(
            pose["declenchement"].as_array().unwrap().len(),
            1,
            "mais une seule case le fait partir"
        );
        assert_eq!(v["cases"].as_array().unwrap().len(), 1);

        // Le Piège Sournois, lui, ne se dit pas mono-cellule : sa croix de
        // rayon 1 déclenche sur ses cinq cases.
        let v = reseau(&requete(&[("piege_sournois", (2, 0))]));
        let pose = &v["poses"][0];
        assert_eq!(pose["mono_cellule"], false);
        assert_eq!(pose["declenchement"].as_array().unwrap().len(), 5);
        assert_eq!(v["cases"].as_array().unwrap().len(), 5);
    }

    /// Le texte du jeu et la donnée disent la même chose : le mot « mono-cellule »
    /// de la description, écrit pour le joueur, et la zone de l'effet de pose, lue
    /// par le moteur du jeu. S'ils divergent, un rééquilibrage a touché l'un sans
    /// l'autre.
    #[test]
    fn le_texte_et_la_donnee_s_accordent_sur_le_declenchement() {
        let snap = snapshot_for(4).unwrap();
        let mut confrontes = 0;
        for spell in &snap.spells {
            let texte = spell.description_fr.as_deref().unwrap_or("").to_lowercase();
            if !texte.contains("mono-cellule") {
                continue;
            }
            for niveau in &spell.levels {
                let Some(z) = niveau.placed_trigger_zone else {
                    continue;
                };
                assert_eq!(
                    z.cells,
                    Some(1),
                    "{} se dit mono-cellule et la donnée lui donne {:?} cases",
                    spell.name.fr,
                    z.cells
                );
                confrontes += 1;
            }
        }
        assert!(confrontes >= 4, "seulement {confrontes} niveaux confrontés");
    }

    /// Les pièges à déclenchement mono-case, tels que la donnée les donne : plus que
    /// ceux dont le texte le dit, les autres frappant eux aussi sur une seule case.
    /// La Fosse Commune n'y est pas : elle déclenche sur une case mais pose un état
    /// sans frapper, et ce test ne parcourt que les pièges qui portent des dégâts.
    #[test]
    fn six_pieges_du_sram_qui_frappent_declenchent_sur_une_seule_case() {
        let snap = snapshot_for(4).unwrap();
        let mut monos: Vec<&str> = Vec::new();
        let mut poseurs = 0;
        for spell in &snap.spells {
            let Some(niveau) = spell.levels.iter().max_by_key(|l| l.grade.unwrap_or(0)) else {
                continue;
            };
            if niveau.placed_lines.is_empty() {
                continue;
            }
            poseurs += 1;
            if niveau.placed_trigger_zone.and_then(|z| z.cells) == Some(1) {
                monos.push(spell.name.fr.as_str());
            }
        }
        monos.sort_unstable();
        assert_eq!(
            monos,
            vec![
                "Piège Effroyable",
                "Piège Fangeux",
                "Piège Funeste",
                "Piège Mortel",
                "Piège Scélérat",
                "Piège à Fragmentation",
            ],
            "la liste des pièges à déclenchement mono-case a changé"
        );
        assert!(poseurs >= 10, "seulement {poseurs} pièges qui frappent");
    }

    /// Ce qu'un piège déplace, et de combien : la distance est dans `dice_num`, pas
    /// dans `value`, qui vaut zéro sur ces effets.
    #[test]
    fn six_pieges_du_sram_deplacent_et_la_donnee_dit_de_combien() {
        let snap = snapshot_for(4).unwrap();
        let mut vus: Vec<(String, &str, i32)> = Vec::new();
        for spell in &snap.spells {
            let Some(niveau) = spell.levels.iter().max_by_key(|l| l.grade.unwrap_or(0)) else {
                continue;
            };
            if let Some((sens, n)) = deplacement(niveau) {
                vus.push((spell.name.fr.clone(), sens, n));
            }
        }
        vus.sort();
        assert_eq!(
            vus,
            vec![
                ("Piège Insidieux".to_string(), "attire", 1),
                ("Piège Répulsif".to_string(), "pousse", 2),
                ("Piège Scélérat".to_string(), "attire", 3),
                ("Piège Sournois".to_string(), "attire", 1),
                ("Piège de Dérive".to_string(), "pousse", 2),
                ("Piège Effroyable".to_string(), "pousse", 2),
            ]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>(),
            "la liste des pièges qui déplacent a changé"
        );
        // ⚠️ ET AUCUNE DISTANCE N'EST NULLE. Lire la magnitude dans `value`
        // rendait zéro partout, ce qui ressemble à « ne déplace pas ».
        assert!(vus.iter().all(|(_, _, n)| *n > 0), "{vus:?}");
    }

    fn avec_chaine(
        poses: &[(&str, (i16, i16))],
        entree: (i16, i16),
        obstacles: &[(i16, i16)],
    ) -> serde_json::Value {
        let mut r = requete(poses);
        r.entree = Some(entree);
        r.obstacles = obstacles.to_vec();
        reseau(&r)["chaine"].clone()
    }

    fn etapes(c: &serde_json::Value, quoi: &str) -> Vec<serde_json::Value> {
        c["etapes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["quoi"] == quoi)
            .cloned()
            .collect()
    }

    /// Les dommages de poussée d'une poussée qui bute : `(niveau / 2 + 32 +
    /// Dommages Poussée) × cases restantes / 4`. Niveau 200 sans Dommages Poussée :
    /// 132 par tranche de quatre cases, 66 pour deux cases bloquées net.
    #[test]
    fn une_poussee_qui_bute_occasionne_des_dommages_de_poussee() {
        let poussee = |c: &serde_json::Value| -> Vec<(i64, i64, bool)> {
            etapes(c, "poussee")
                .iter()
                .map(|e| {
                    (
                        e["entite"].as_i64().unwrap(),
                        e["degats"][0].as_i64().unwrap(),
                        e["percutee"].as_bool().unwrap(),
                    )
                })
                .collect()
        };
        // Le Répulsif de (2,0) pousse de deux la cible entrée en (3,0).
        let net = avec_chaine(&[("piege_repulsif", (2, 0))], (3, 0), &[(4, 0)]);
        assert_eq!(poussee(&net), vec![(0, 66, false)], "deux cases bloquées net");
        let une = avec_chaine(&[("piege_repulsif", (2, 0))], (3, 0), &[(5, 0)]);
        assert_eq!(poussee(&une), vec![(0, 33, false)], "une case faite, une bloquée");
        let libre = avec_chaine(&[("piege_repulsif", (2, 0))], (3, 0), &[]);
        assert!(poussee(&libre).is_empty(), "rien ne bute");
        // Le total de la chaîne et la fiche de la cible les comptent.
        let degats_piege: i64 = etapes(&net, "degats")
            .iter()
            .filter(|e| e["entite"] == 0)
            .map(|e| e["degats"][0].as_i64().unwrap())
            .sum();
        assert_eq!(net["total"][0].as_i64().unwrap(), degats_piege + 66);
    }

    /// Le gabarit du catalogue, décalé sur la case d'une pose, est exactement
    /// la zone de déclenchement que cette pose dessine : l'aperçu de pose
    /// montre ce que le clic posera.
    #[test]
    fn le_gabarit_du_catalogue_est_la_zone_posee() {
        let v = reseau(&requete(&[("piege_repulsif", (2, 1)), ("piege_mortel", (-1, 3))]));
        for pose in v["poses"].as_array().unwrap() {
            let modele = v["catalogue"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["sort"] == pose["sort"])
                .expect("au catalogue");
            let (px, py) = (pose["case"][0].as_i64().unwrap(), pose["case"][1].as_i64().unwrap());
            let mut decale: Vec<(i64, i64)> = modele["gabarit"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| (c[0].as_i64().unwrap() + px, c[1].as_i64().unwrap() + py))
                .collect();
            let mut pose_zone: Vec<(i64, i64)> = pose["declenchement"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| (c[0].as_i64().unwrap(), c[1].as_i64().unwrap()))
                .collect();
            decale.sort_unstable();
            pose_zone.sort_unstable();
            assert_eq!(decale, pose_zone, "{}", pose["sort"]);
            assert_eq!(modele["declenchement"].as_u64(), Some(pose_zone.len() as u64));
        }
    }

    /// Sur une carte, ses murs et ses trous arrêtent la poussée. Klime : sa ligne
    /// y = 0 est du sol de x = -1 à 6, avec un mur en x = 7 et un trou en x = -2.
    /// Poussée de deux vers le mur depuis (5,0), la cible fait une case et bute :
    /// 33. Poussée vers le trou depuis (-1,0), elle bute net : 66. Sur le damier
    /// vide, rien ne l'arrête.
    #[test]
    fn sur_une_carte_les_murs_et_les_trous_arretent_la_poussee() {
        let chaine = |pose: (i16, i16), entree: (i16, i16), carte: Option<u16>| {
            let mut r = requete(&[("piege_repulsif", pose)]);
            r.lanceur = Some((3, 2));
            r.entree = Some(entree);
            r.carte = carte;
            reseau(&r)["chaine"].clone()
        };
        let buter = |c: &serde_json::Value| -> Vec<i64> {
            etapes(c, "poussee").iter().map(|e| e["degats"][0].as_i64().unwrap()).collect()
        };
        assert_eq!(buter(&chaine((4, 0), (5, 0), Some(62))), vec![33], "contre le mur");
        assert_eq!(buter(&chaine((0, 0), (-1, 0), Some(62))), vec![66], "contre le trou");
        assert!(buter(&chaine((4, 0), (5, 0), None)).is_empty(), "le damier vide n'arrête rien");
    }

    /// Sur une carte, un piège ne se pose que sur son sol, à la main comme par
    /// le générateur, qui cherche donc sur la carte choisie.
    #[test]
    fn sur_une_carte_un_piege_ne_se_pose_que_sur_le_sol() {
        let mut r = requete(&[("piege_repulsif", (7, 0))]);
        r.lanceur = Some((3, 2));
        r.carte = Some(62);
        assert_eq!(
            reseau(&r)["poses"][0]["refus"].as_str(),
            Some("la case n'est pas du sol : un mur ou un trou")
        );
        let mut r = requete(&[]);
        r.lanceur = Some((3, 2));
        r.entree = Some((6, 0));
        r.pa = Some(8);
        r.carte = Some(62);
        r.proposer = true;
        let p = reseau(&r)["proposition"].clone();
        let klime = Plateau::de(Some(62), None).unwrap();
        let poses = p["poses"].as_array().unwrap();
        assert!(!poses.is_empty(), "{p}");
        for x in poses {
            let c = |i: usize| i16::try_from(x["case"][i].as_i64().unwrap()).unwrap();
            assert!(klime.est_sol(Case::new(c(0), c(1))), "{x}");
        }
    }

    /// L'entité percutée prend la même formule divisée par huit, si c'est un
    /// ennemi : un allié ou le Sram ne prennent rien, comme pour les dégâts de
    /// piège.
    #[test]
    fn l_entite_percutee_prend_la_moitie() {
        let mut r = requete(&[("piege_repulsif", (2, 0))]);
        r.entree = Some((3, 0));
        r.ennemis = vec![(4, 0)];
        let c = reseau(&r)["chaine"].clone();
        let poussee: Vec<(i64, i64, bool)> = etapes(&c, "poussee")
            .iter()
            .map(|e| (e["entite"].as_i64().unwrap(), e["degats"][0].as_i64().unwrap(), e["percutee"].as_bool().unwrap()))
            .collect();
        assert_eq!(poussee.len(), 2, "{poussee:?}");
        assert_eq!((poussee[0].1, poussee[0].2), (66, false));
        assert_eq!((poussee[1].1, poussee[1].2), (33, true));

        let mut r = requete(&[("piege_repulsif", (2, 0))]);
        r.entree = Some((3, 0));
        r.allies = vec![(4, 0)];
        let c = reseau(&r)["chaine"].clone();
        let poussee = etapes(&c, "poussee");
        assert_eq!(poussee.len(), 1, "l'allié percuté ne prend rien : {poussee:?}");
        assert_eq!(poussee[0]["entite"], 0);
    }

    /// Un piège ne part qu'une fois : c'est ce qui borne une chaîne, sans quoi un
    /// Répulsif et un Sournois face à face se renverraient la cible sans fin.
    #[test]
    fn un_piege_ne_se_declenche_qu_une_fois() {
        // Le Répulsif pousse de 2, le Sournois attire de 1 : posés en vis-à-vis,
        // ils se renverraient la cible indéfiniment si un piège persistait.
        let c = avec_chaine(
            &[("piege_repulsif", (2, 0)), ("piege_sournois", (6, 0))],
            (3, 0),
            &[],
        );
        let declenches = etapes(&c, "declenche");
        let mut vus: Vec<u64> = declenches
            .iter()
            .map(|e| e["piege"].as_u64().unwrap())
            .collect();
        let avant = vus.len();
        vus.sort_unstable();
        vus.dedup();
        assert_eq!(
            vus.len(),
            avant,
            "un piège est parti deux fois : {declenches:?}"
        );
        assert!(avant <= 2, "il n'y a que deux pièges au sol");
        // Et la chaîne se termine, ce qui n'est pas rien : une boucle infinie
        // ferait tourner le serveur, pas échouer le test.
        assert_eq!(
            c["etapes"].as_array().unwrap().last().unwrap()["quoi"],
            "fin"
        );
    }

    /// On n'échappe pas à un piège en passant dessus vite : une cible poussée
    /// déclenche ce qu'elle croise, et les dégâts se lisent sur la case qui a
    /// déclenché, pas sur sa position courante.
    #[test]
    fn un_piege_traverse_pendant_une_poussee_frappe_quand_meme() {
        // Répulsif en (2,0), qui pousse de 2 depuis (3,0) jusqu'à (5,0), en
        // passant par (4,0) où se tient un Piège Mortel mono-case.
        let c = avec_chaine(
            &[("piege_repulsif", (2, 0)), ("piege_mortel", (4, 0))],
            (3, 0),
            &[],
        );
        let noms: Vec<String> = etapes(&c, "degats")
            .iter()
            .map(|e| e["nom"].as_str().unwrap().to_string())
            .collect();
        assert!(
            noms.iter().any(|n| n == "Piège Mortel"),
            "le piège traversé doit frapper : {noms:?}"
        );
        // Le total EST la somme des étapes de dégâts, et pas autre chose.
        let somme = etapes(&c, "degats").iter().fold((0i64, 0i64), |(a, b), e| {
            (
                a + e["degats"][0].as_i64().unwrap(),
                b + e["degats"][1].as_i64().unwrap(),
            )
        });
        assert_eq!(c["total"][0].as_i64().unwrap(), somme.0);
        assert_eq!(c["total"][1].as_i64().unwrap(), somme.1);
    }

    /// Une poussée qui bute s'arrête, et la trace le dit.
    #[test]
    fn une_poussee_bloquee_s_arrete_et_se_signale() {
        let libre = avec_chaine(&[("piege_repulsif", (2, 0))], (3, 0), &[]);
        let mur = avec_chaine(&[("piege_repulsif", (2, 0))], (3, 0), &[(5, 0)]);

        let d = &etapes(&libre, "deplace")[0];
        assert_eq!(d["fait"], 2);
        assert_eq!(d["bloquee"], false);
        assert_eq!(d["vers"], serde_json::json!([5, 0]));

        let d = &etapes(&mur, "deplace")[0];
        assert_eq!(d["fait"], 1, "elle s'arrête devant le mur");
        assert_eq!(d["voulu"], 2);
        assert_eq!(d["bloquee"], true);
        assert_eq!(d["vers"], serde_json::json!([4, 0]));
    }

    /// Les trois lectures de l'ordre donnent des traces différentes : sinon le
    /// réglage serait décoratif.
    #[test]
    fn les_trois_ordres_de_declenchement_ne_disent_pas_la_meme_chose() {
        // Trois pièges dont les zones de déclenchement couvrent toutes (3,0),
        // et qui ne sont pas au même endroit : l'ordre horaire les classe
        // autrement que l'ordre de pose.
        let poses = &[
            ("piege_sournois", (3, 1)),
            ("piege_repulsif", (3, -1)),
            ("piege_de_derive", (4, 1)),
        ];
        let ordres = ["pose", "pose_inverse", "horaire"];
        let mut suites = Vec::new();
        for o in ordres {
            let mut r = requete(poses);
            r.entree = Some((3, 0));
            r.ordre = Some(serde_json::from_value(serde_json::json!(o)).unwrap());
            let c = reseau(&r)["chaine"].clone();
            let suite: Vec<String> = etapes(&c, "declenche")
                .iter()
                .map(|e| e["nom"].as_str().unwrap().to_string())
                .collect();
            assert_eq!(suite.len(), 3, "les trois pièges partent, ordre {o}");
            suites.push(suite);
        }
        assert_ne!(suites[0], suites[1], "l'ordre de pose et son inverse");
        assert!(
            suites[2] != suites[0] || suites[2] != suites[1],
            "l'ordre horaire doit différer d'au moins un des deux : {suites:?}"
        );
    }

    /// Le plafond existe et ne sert jamais : une chaîne se termine parce qu'un piège
    /// ne part qu'une fois.
    #[test]
    fn la_chaine_se_termine_sans_jamais_toucher_son_plafond() {
        // Quatre pièges qui se renvoient la cible : le cas le plus long qu'on
        // puisse construire avec ce que la classe a.
        let c = avec_chaine(
            &[
                ("piege_repulsif", (2, 0)),
                ("piege_sournois", (6, 0)),
                ("piege_de_derive", (4, 2)),
                ("piege_insidieux", (4, -2)),
            ],
            (3, 0),
            &[],
        );
        let suite = c["etapes"].as_array().unwrap();
        assert_eq!(
            suite.last().unwrap()["quoi"],
            "fin",
            "la chaîne se termine d'elle-même"
        );
        assert!(
            !suite.iter().any(|e| e["quoi"] == "coupee"),
            "le plafond n'a pas à servir : {suite:?}"
        );
        // Et elle a vraiment tourné, sinon le test ne dirait rien.
        assert!(
            etapes(&c, "declenche").len() >= 2,
            "la chaîne doit enchaîner au moins deux pièges"
        );
    }

    /// Le garde-fou fonctionne : la branche « coupée », que seule une chaîne sans
    /// fin visiterait, se teste par un plafond passé en paramètre.
    #[test]
    fn le_plafond_coupe_la_trace_au_lieu_de_boucler() {
        use dofus_engine::Case;
        let piege = |rang: usize, x: i16| PiegePose {
            rang,
            nom: format!("essai {rang}"),
            centre: Case::new(x, 0),
            declenchement: vec![Case::new(x, 0)],
            effets: vec![LigneDeDegats {
                zone: vec![Case::new(x, 0)],
                degats: Portees::uniques((10, 10)),
                masque: "A".into(),
            }],
            deplacement: None,
            deplace_avant: false,
            poison: None,
        };
        let pieges: Vec<PiegePose> = (0..3).map(|i| piege(i, i as i16)).collect();
        let terrain = Terrain {
            obstacles: &[],
            lanceur: None,
            autres: &[],
            ordre: Ordre::Pose,
            chakra: None,
            poussee: Poussee::default(),
        };
        let coupee = chaine_bornee(&pieges, Case::new(0, 0), &terrain, 0);
        assert_eq!(
            coupee.last().unwrap(),
            &Etape::Coupee { apres: 0 },
            "à plafond nul, la trace est coupée dès le premier tour"
        );
        // Et au plafond normal, elle ne l'est pas.
        let entiere = chaine(&pieges, Case::new(0, 0), &terrain);
        assert!(!entiere.iter().any(|e| matches!(e, Etape::Coupee { .. })));
    }

    /// Une poussée s'arrête sur le premier piège qu'elle rencontre. Le Répulsif de
    /// (2,0) veut pousser la cible de (3,0) à (5,0) ; le Mortel de (4,0) l'arrête
    /// en chemin, et il part. Ce n'est pas un blocage : seul un obstacle occasionne
    /// des dommages de poussée, et la trace les distingue.
    #[test]
    fn une_poussee_s_arrete_sur_le_premier_piege_qu_elle_rencontre() {
        let c = avec_chaine(
            &[("piege_repulsif", (2, 0)), ("piege_mortel", (4, 0))],
            (3, 0),
            &[],
        );
        let pousse = &etapes(&c, "deplace")[0];
        assert_eq!(pousse["voulu"], 2);
        assert_eq!(pousse["fait"], 1, "arrêtée sur le Mortel : {pousse}");
        assert_eq!(pousse["vers"], serde_json::json!([4, 0]));
        assert_eq!(pousse["arretee_par_piege"], true);
        assert_eq!(pousse["bloquee"], false, "un piège n'est pas un mur");
        assert!(
            etapes(&c, "declenche").iter().any(|e| e["nom"] == "Piège Mortel"),
            "le Mortel doit partir"
        );
    }

    /// Un groupe rencontré en chemin interrompt le groupe en cours : la mise en
    /// attente. Les pièges rencontrés pendant un déplacement prennent la priorité
    /// sur ceux du groupe en cours, même posés après eux.
    ///
    /// Le montage : la cible entre en (3,0), couverte par un Répulsif posé en
    /// premier et un Sournois posé en second. Le Répulsif la pousse ; le Mortel de
    /// (4,0) l'arrête et part. Ses dégâts tombent avant l'attirance du Sournois,
    /// pourtant déclenché le premier.
    #[test]
    fn un_groupe_rencontre_en_chemin_interrompt_le_groupe_en_cours() {
        let c = avec_chaine(
            &[
                ("piege_repulsif", (2, 0)),
                ("piege_sournois", (3, 1)),
                ("piege_mortel", (4, 0)),
            ],
            (3, 0),
            &[],
        );
        let ordre: Vec<String> = c["etapes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["quoi"] == "degats" || e["quoi"] == "deplace")
            .map(|e| format!("{} {}", e["quoi"].as_str().unwrap(), e["nom"].as_str().unwrap()))
            .collect();
        let rang = |quoi: &str| {
            ordre
                .iter()
                .position(|e| e == quoi)
                .unwrap_or_else(|| panic!("« {quoi} » absent de {ordre:?}"))
        };
        assert!(
            rang("degats Piège Mortel") < rang("deplace Piège Sournois"),
            "le Mortel, rencontré en chemin, passe devant le Sournois en attente : {ordre:?}"
        );
        // Et le Sournois finit bien par partir : la mise en attente retarde,
        // elle n'efface pas.
        assert!(rang("deplace Piège Répulsif") < rang("degats Piège Mortel"));
    }

    /// La Concentration de Chakra vole une fois par dégât de piège : 12 fixes par
    /// dégât, passés par le build, dans son meilleur élément.
    #[test]
    fn la_concentration_de_chakra_vole_apres_chaque_degat_de_piege() {
        let mut r = requete(&[("piege_repulsif", (2, 0)), ("piege_mortel", (4, 0))]);
        r.entree = Some((3, 0));
        let sans = reseau(&r)["chaine"].clone();
        r.chakra = true;
        let avec = reseau(&r)["chaine"].clone();

        let degats = etapes(&avec, "degats").len();
        let vols = etapes(&avec, "chakra");
        assert!(degats >= 2, "le montage doit faire frapper deux pièges");
        assert_eq!(vols.len(), degats, "un vol par dégât de piège");
        assert!(etapes(&sans, "chakra").is_empty(), "sans l'état, aucun vol");

        // Chaque vol suit le dégât qui le déclenche.
        let suite = avec["etapes"].as_array().unwrap();
        for (i, e) in suite.iter().enumerate() {
            if e["quoi"] == "chakra" {
                assert_eq!(suite[i - 1]["quoi"], "degats", "{suite:?}");
            }
        }

        // Le vol passe par le build : au-dessus de ses 12 de base.
        let vol = vols[0]["degats"][0].as_i64().unwrap();
        assert!(vol > 12, "12 de base multipliés par le build, mesuré {vol}");

        // Et le total en tient compte, exactement.
        let total = |c: &serde_json::Value| c["total"][0].as_i64().unwrap();
        assert_eq!(total(&avec) - total(&sans), vol * degats as i64);
    }

    /// L'Insidieux attire en diagonale, jusqu'à son centre. Sa zone est la croix
    /// diagonale : la cible qui y entre est en diagonale du centre, et ramenée sur
    /// un seul axe elle atterrirait à côté, hors de portée des Répulsifs qui
    /// couvrent le centre.
    #[test]
    fn l_insidieux_attire_en_diagonale_jusqu_a_son_centre() {
        let c = avec_chaine(
            &[
                ("piege_insidieux", (4, 0)),
                ("piege_repulsif", (5, 0)),
                ("piege_repulsif", (4, 1)),
            ],
            (3, -1),
            &[],
        );
        let attire = etapes(&c, "deplace")
            .into_iter()
            .find(|e| e["nom"] == "Piège Insidieux")
            .expect("l'Insidieux doit attirer");
        assert_eq!(attire["vers"], serde_json::json!([4, 0]), "en diagonale, jusqu'au centre : {attire}");
        let frappent = etapes(&c, "degats")
            .iter()
            .filter(|e| e["nom"] == "Piège Répulsif")
            .count();
        assert_eq!(frappent, 2, "les deux Répulsifs couvrent le centre et doivent frapper : {c}");
    }

    /// Le Sram arrête une poussée : il occupe sa case, et le blocage occasionne des
    /// dommages de poussée, comme un mur.
    #[test]
    fn le_sram_arrete_une_poussee_comme_un_mur() {
        // Le Sram en (0,0), le Répulsif en (3,0) : la cible entrée en (2,0) est
        // poussée vers (0,0) et doit s'arrêter en (1,0).
        let c = avec_chaine(&[("piege_repulsif", (3, 0))], (2, 0), &[]);
        let pousse = &etapes(&c, "deplace")[0];
        assert_eq!(pousse["vers"], serde_json::json!([1, 0]), "arrêtée devant le Sram : {pousse}");
        assert_eq!(pousse["bloquee"], true, "une entité bloque, comme un mur");
        assert_eq!(pousse["arretee_par_piege"], false);
    }

    /// La direction d'une poussée se fige au déclenchement. L'Insidieux amène la
    /// cible en (4,0), où deux Répulsifs partent ensemble. Le premier, en (5,0), la
    /// pousse de 2 vers (2,0). Le second, en (4,1), la pousse depuis là dans sa
    /// direction de départ, de (4,1) vers (4,0), soit vers les y négatifs : elle
    /// finit en (2,-2). Recalculée depuis la position courante, elle partirait vers
    /// les x négatifs et buterait sur le Sram en (1,0).
    #[test]
    fn la_direction_d_une_poussee_se_fige_au_declenchement() {
        let c = avec_chaine(
            &[
                ("piege_insidieux", (4, 0)),
                ("piege_repulsif", (5, 0)),
                ("piege_repulsif", (4, 1)),
            ],
            (3, -1),
            &[],
        );
        let fin = etapes(&c, "fin");
        assert_eq!(fin[0]["case"], serde_json::json!([2, -2]), "{c}");
    }

    /// En diagonale, un pas vaut deux cases. Le Piège de Dérive pousse de 2 et
    /// déclenche sur la croix diagonale : un pas diagonal, et la cible entrée en
    /// (3,-1) finit en (2,-2), poussée complète.
    #[test]
    fn en_diagonale_un_pas_vaut_deux_cases() {
        let c = avec_chaine(&[("piege_de_derive", (4, 0))], (3, -1), &[]);
        let pousse = &etapes(&c, "deplace")[0];
        assert_eq!(pousse["vers"], serde_json::json!([2, -2]), "{pousse}");
        assert_eq!(pousse["fait"], 2, "complète, en cases");
        assert_eq!(pousse["bloquee"], false);
    }

    /// Un pas diagonal exige ses deux cases d'angle libres : avec un obstacle en
    /// (2,-1), le pas de (3,-1) vers (2,-2) est refusé, et la cible reste, bloquée.
    #[test]
    fn un_pas_diagonal_exige_ses_deux_angles_libres() {
        let c = avec_chaine(&[("piege_de_derive", (4, 0))], (3, -1), &[(2, -1)]);
        let pousse = &etapes(&c, "deplace")[0];
        assert_eq!(pousse["vers"], serde_json::json!([3, -1]), "{pousse}");
        assert_eq!(pousse["fait"], 0);
        assert_eq!(pousse["bloquee"], true, "un angle occupé bloque le pas");
    }

    /// Les pourcentages du build s'appliquent selon la portée du coup, jugée sur
    /// l'écart entre le Sram et la cible au moment du coup. Ici 50 % à distance et
    /// 100 % en mêlée : un Mortel loin du Sram frappe à ×1,5, le même au contact
    /// à ×2.
    #[test]
    fn les_pourcentages_du_build_s_appliquent_selon_la_portee() {
        let degats = |fm: serde_json::Value, piege: (i16, i16)| -> i64 {
            let mut r = requete(&[("piege_mortel", piege)]);
            r.build = serde_json::from_value(serde_json::json!({
                "class": 4,
                "level": 200,
                "invested": { "agility": 300, "strength": 300, "intelligence": 200, "chance": 200 },
                "fmGlobal": fm,
            }))
            .unwrap();
            r.entree = Some(piege);
            etapes(&reseau(&r)["chaine"], "degats")[0]["degats"][0].as_i64().unwrap()
        };
        let pourcents = serde_json::json!({ "dd": 50, "dm": 100 });
        let nu = serde_json::json!({});

        // Loin du Sram, posé en (0,0) : les dommages distance.
        let (base, loin) = (degats(nu.clone(), (4, 0)), degats(pourcents.clone(), (4, 0)));
        assert!((loin - base * 3 / 2).abs() <= 1, "à distance, ×1,5 : {base} puis {loin}");
        // Au contact du Sram : les dommages mêlée.
        let (base, pres) = (degats(nu, (1, 0)), degats(pourcents, (1, 0)));
        assert!((pres - base * 2).abs() <= 1, "au contact, ×2 : {base} puis {pres}");
    }

    /// Le Répulsif pousse avant de frapper, et ce que sa poussée déclenche passe
    /// entre les deux : c'est l'ordre des effets dans la donnée. Le Mortel sur
    /// lequel la poussée s'arrête frappe avant les dégâts du Répulsif.
    #[test]
    fn le_repulsif_pousse_avant_de_frapper() {
        let c = avec_chaine(&[("piege_repulsif", (2, 0)), ("piege_mortel", (4, 0))], (3, 0), &[]);
        let ordre: Vec<String> = c["etapes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["quoi"] == "degats" || e["quoi"] == "deplace")
            .map(|e| format!("{} {}", e["quoi"].as_str().unwrap(), e["nom"].as_str().unwrap()))
            .collect();
        assert_eq!(
            ordre,
            vec![
                "deplace Piège Répulsif",
                "degats Piège Mortel",
                "degats Piège Répulsif",
            ],
            "{c}"
        );
    }

    /// L'Insidieux frappe 8-9 au déclenchement, et empoisonne pour 8-9 en fin de
    /// tour : son texte dit « applique un poison Air de fin de tour sur les
    /// ennemis, leur occasionne des dommages Air ». Les additionner donnerait 16-18
    /// à un coup qui en vaut 8-9.
    #[test]
    fn l_insidieux_frappe_une_fois_et_empoisonne_a_part() {
        // Sans build, les valeurs de base : 8-9 exactement.
        let mut r: RequeteReseau = serde_json::from_value(serde_json::json!({
            "class": 4,
            "level": 200,
            "lanceur": [0, 0],
            "poses": [{ "sort": "piege_insidieux", "case": [4, 0] }],
        }))
        .unwrap();
        let catalogue = reseau(&r)["catalogue"].clone();
        let insidieux = catalogue
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["sort"] == "piege_insidieux")
            .unwrap()
            .clone();
        assert_eq!(insidieux["degats"], serde_json::json!([8, 9]), "le coup seul");
        assert_eq!(insidieux["poison"], serde_json::json!([8, 9]), "le poison à part");

        r.entree = Some((3, -1));
        let c = reseau(&r)["chaine"].clone();
        let coup = &etapes(&c, "degats")[0];
        assert_eq!(coup["degats"], serde_json::json!([8, 9]));
        assert_eq!(etapes(&c, "poison").len(), 1, "le poison se signale");
        assert_eq!(c["total"], serde_json::json!([8, 9]), "et n'entre pas dans le total");
    }

    /// Le défaut est FIFO, l'ordre de pose, règle du jeu : un défaut qui
    /// basculerait sur une autre lecture rendrait des traces fausses sans qu'aucun
    /// autre test ne bronche.
    #[test]
    fn l_ordre_par_defaut_est_celui_de_pose() {
        assert_eq!(Ordre::default(), Ordre::Pose);

        // Et il traverse vraiment la requête : une requête muette doit rendre
        // la même trace qu'une requête qui demande `pose`, et une autre qu'une
        // requête qui demande autre chose.
        let poses = &[
            ("piege_sournois", (3, 1)),
            ("piege_repulsif", (3, -1)),
            ("piege_de_derive", (4, 1)),
        ];
        let suite = |ordre: Option<&str>| -> Vec<String> {
            let mut r = requete(poses);
            r.entree = Some((3, 0));
            r.ordre = ordre.map(|o| serde_json::from_value(serde_json::json!(o)).unwrap());
            etapes(&reseau(&r)["chaine"], "declenche")
                .iter()
                .map(|e| e["nom"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(suite(None), suite(Some("pose")));
        assert_ne!(suite(None), suite(Some("pose_inverse")));
        assert_eq!(
            reseau(&{
                let mut r = requete(poses);
                r.entree = Some((3, 0));
                r
            })["chaine"]["ordre"],
            "pose",
            "et la réponse dit lequel a servi"
        );
    }

    /// La chaîne d'un montage où d'autres entités que la cible se tiennent.
    fn avec_entites(
        poses: &[(&str, (i16, i16))],
        entree: (i16, i16),
        lanceur: (i16, i16),
        allies: &[(i16, i16)],
        ennemis: &[(i16, i16)],
    ) -> serde_json::Value {
        let mut r = requete(poses);
        r.entree = Some(entree);
        r.lanceur = Some(lanceur);
        r.allies = allies.to_vec();
        r.ennemis = ennemis.to_vec();
        reseau(&r)["chaine"].clone()
    }

    /// Les étapes d'un genre qui concernent une entité, par son numéro : 0 la
    /// cible, 1 le Sram, puis les alliés et les autres ennemis.
    fn de(c: &serde_json::Value, quoi: &str, entite: u64) -> Vec<serde_json::Value> {
        etapes(c, quoi)
            .into_iter()
            .filter(|e| e["entite"] == entite)
            .collect()
    }

    /// L'horloge, dans notre repère : 0, 3, 6 et 9 sur les quatre directions à
    /// trois cases, en partant du haut à droite de l'écran et dans le sens des
    /// aiguilles d'une montre. (0,-3) est en haut à droite de l'écran, (3,0) en bas
    /// à droite, (0,3) en bas à gauche, (-3,0) en haut à gauche.
    #[test]
    fn l_horloge_vaut_0_3_6_9_sur_les_quatre_directions() {
        assert_eq!(
            [
                index_horaire(0, -3),
                index_horaire(3, 0),
                index_horaire(0, 3),
                index_horaire(-3, 0),
            ],
            [0, 3, 6, 9]
        );
        // Et chaque anneau est numéroté sans doublon, de 0 à 4a - 1 : deux
        // entités à même distance ne sont jamais à égalité.
        for a in 1i16..=4 {
            let mut vus: Vec<i32> = (-a..=a)
                .flat_map(|dx| {
                    let reste = a - dx.abs();
                    let mut v = vec![index_horaire(dx, reste)];
                    if reste != 0 {
                        v.push(index_horaire(dx, -reste));
                    }
                    v
                })
                .collect();
            vus.sort_unstable();
            assert_eq!(vus, (0..4 * i32::from(a)).collect::<Vec<_>>(), "anneau {a}");
        }
    }

    /// ⚠️ PRIORITÉ HORAIRE : une attirance prend d'abord l'entité la plus tôt
    /// sur l'horloge.
    ///
    /// Le Sournois de (4,0) attire de 1. La cible entre en (5,0), en bas à
    /// droite du piège, un allié se tient en (4,-1), en haut à droite. À même
    /// distance, l'allié vient avant sur l'horloge : il est attiré le premier,
    /// prend le centre, et la cible, attirée ensuite, bute sur lui.
    #[test]
    fn une_attirance_prend_d_abord_l_entite_la_plus_tot_sur_l_horloge() {
        let c = avec_entites(&[("piege_sournois", (4, 0))], (5, 0), (0, 0), &[(4, -1)], &[]);
        let allie = &de(&c, "deplace", 2)[0];
        assert_eq!(allie["vers"], serde_json::json!([4, 0]), "l'allié prend le centre : {c}");
        let cible = &de(&c, "deplace", 0)[0];
        assert_eq!(cible["fait"], 0, "la cible bute sur l'allié : {c}");
        let ordre: Vec<u64> = etapes(&c, "deplace")
            .iter()
            .map(|e| e["entite"].as_u64().unwrap())
            .collect();
        assert_eq!(ordre, vec![2, 0], "l'allié part le premier");
    }

    /// Priorité contre-horaire : une poussée prend la liste à l'envers, la plus
    /// lointaine d'abord.
    ///
    /// L'Effroyable de (4,0) pousse de 2 tout ce qui est dans sa croix de 3. La
    /// cible entre sur son centre et ne bouge pas ; un ennemi en (5,0) et un allié
    /// en (6,0) sont sur le même bras. L'allié, plus loin, part le premier et
    /// dégage le chemin : l'ennemi le suit jusqu'en (7,0). Dans l'ordre direct,
    /// l'ennemi buterait sur l'allié. Le plus proche est saisi en dernier exprès :
    /// dans l'autre sens, une liste non triée tomberait déjà dans le bon ordre.
    #[test]
    fn une_poussee_prend_d_abord_l_entite_la_plus_lointaine() {
        // Au centre du damier : son bord arrête les poussées, et l'allié
        // poussé en (8, 0) serait sorti d'un damier de 15.
        let c = avec_entites(&[("piege_effroyable", (0, 0))], (0, 0), (-4, 0), &[(2, 0)], &[(1, 0)]);
        assert_eq!(de(&c, "deplace", 2)[0]["vers"], serde_json::json!([4, 0]), "{c}");
        assert_eq!(de(&c, "deplace", 3)[0]["vers"], serde_json::json!([3, 0]), "{c}");
        assert!(de(&c, "deplace", 0).is_empty(), "sur le centre, la cible ne bouge pas");
    }

    /// Le bord du damier vide arrête une poussée comme un mur, et elle y frappe de
    /// ses dommages de poussée. Le Répulsif de (5,0) pousse la cible entrée en
    /// (6,0) de deux cases ; le damier de quinze s'arrête en 7.
    #[test]
    fn le_bord_du_damier_arrete_la_poussee() {
        let c = avec_entites(&[("piege_repulsif", (5, 0))], (6, 0), (0, 0), &[], &[]);
        let pousse = &de(&c, "deplace", 0)[0];
        assert_eq!(pousse["vers"], serde_json::json!([7, 0]), "{c}");
        assert_eq!((&pousse["fait"], &pousse["bloquee"]), (&serde_json::json!(1), &serde_json::json!(true)), "{c}");
        assert!(!de(&c, "poussee", 0).is_empty(), "les dommages de poussée contre le bord : {c}");
    }

    /// ⚠️ LE SRAM EST DÉPLACÉ PAR SES PROPRES PIÈGES.
    ///
    /// Les déplacements visent `a,A`, et `a` compte le lanceur. Le Répulsif de
    /// (2,0) pousse le Sram qui se tient en (1,0) comme il pousse la cible
    /// entrée en (3,0) : chacun s'écarte du centre de deux cases.
    #[test]
    fn le_sram_est_deplace_par_ses_propres_pieges() {
        let c = avec_entites(&[("piege_repulsif", (2, 0))], (3, 0), (1, 0), &[], &[]);
        assert_eq!(de(&c, "deplace", 1)[0]["vers"], serde_json::json!([-1, 0]), "{c}");
        assert_eq!(de(&c, "deplace", 0)[0]["vers"], serde_json::json!([5, 0]), "{c}");
        assert_eq!(c["entites"][1]["arrivee"], serde_json::json!([-1, 0]));
    }

    /// Un piège ne frappe jamais le camp du Sram. Les anneaux du Piège à
    /// Fragmentation portent `a,A` dans la donnée. La cible entre sur son centre ;
    /// le Sram et un allié à deux cases, dans l'anneau 2, ne prennent rien ; un
    /// second ennemi à trois cases prend l'anneau 3, et le total est celui des deux
    /// ennemis. Le Sournois attire l'allié pris dans sa croix, sans le blesser : les
    /// déplacements, eux, visent tout le monde.
    #[test]
    fn les_pieges_ne_frappent_jamais_le_camp_du_sram() {
        let c = avec_entites(&[("piege_a_fragmentation", (4, 0))], (4, 0), (6, 0), &[(4, 2)], &[(4, 3)]);
        let subi = |i: u64| {
            let d = &de(&c, "degats", i);
            assert_eq!(d.len(), 1, "l'entité {i} est frappée une fois : {c}");
            (d[0]["degats"][0].as_i64().unwrap(), d[0]["degats"][1].as_i64().unwrap())
        };
        let (cible, ennemi) = (subi(0), subi(3));
        assert!(de(&c, "degats", 1).is_empty(), "le Sram n'est pas frappé : {c}");
        assert!(de(&c, "degats", 2).is_empty(), "l'allié n'est pas frappé : {c}");
        assert_eq!(c["total"], serde_json::json!([cible.0 + ennemi.0, cible.1 + ennemi.1]));

        let c = avec_entites(&[("piege_sournois", (4, 0))], (5, 0), (0, 0), &[(3, 0)], &[]);
        assert!(de(&c, "degats", 2).is_empty(), "le Sournois ne frappe pas l'allié : {c}");
        assert_eq!(de(&c, "deplace", 2).len(), 1, "mais il l'attire : {c}");
    }

    /// ⚠️ LE MASQUE SE LIT LIGNE PAR LIGNE, même sur une case que plusieurs
    /// lignes couvrent.
    ///
    /// Le test construit un piège de trois lignes sur les mêmes cases : `A`,
    /// `A,V50` et `a,A`. L'ennemi, à pleine vie, prend la première et la
    /// troisième, pas celle qui exige 50 % de vie ou moins ; l'allié ne prend
    /// rien, `a` ne suffisant pas à ce qu'un piège frappe son camp.
    #[test]
    fn le_masque_se_lit_ligne_par_ligne() {
        use dofus_engine::Case;
        let ligne = |degats: i64, masque: &str| LigneDeDegats {
            zone: vec![Case::new(0, 0), Case::new(1, 0)],
            degats: Portees::uniques((degats, degats)),
            masque: masque.into(),
        };
        let piege = PiegePose {
            rang: 0,
            nom: "essai".into(),
            centre: Case::new(0, 0),
            declenchement: vec![Case::new(0, 0)],
            effets: vec![ligne(10, "A"), ligne(5, "A,V50"), ligne(7, "a,A")],
            deplacement: None,
            deplace_avant: false,
            poison: None,
        };
        let autres = [Entite {
            camp: Camp::Allie,
            case: Case::new(1, 0),
        }];
        let terrain = Terrain {
            obstacles: &[],
            lanceur: None,
            autres: &autres,
            ordre: Ordre::Pose,
            chakra: None,
            poussee: Poussee::default(),
        };
        let subis: Vec<(usize, (i64, i64))> = chaine(&[piege], Case::new(0, 0), &terrain)
            .into_iter()
            .filter_map(|e| match e {
                Etape::Degats { entite, degats, .. } => Some((entite, degats)),
                _ => None,
            })
            .collect();
        assert_eq!(subis, vec![(0, (17, 17))]);
    }

    /// ⚠️ N'IMPORTE QUELLE ENTITÉ DÉPLACÉE DÉCLENCHE CE QU'ELLE RENCONTRE.
    ///
    /// Le Répulsif de (2,0) pousse la cible entrée en (3,0) et un second ennemi
    /// posé en (2,1), dans sa croix, qui prend aussi ses dégâts. Celui-ci
    /// s'arrête en (2,2) sur le Piège Mortel, qui part et le frappe lui : les
    /// deux coups entrent dans le total. Le Sram se tient en (0,2), dans la
    /// ligne du Mortel, qui se lance en ligne.
    #[test]
    fn un_ennemi_deplace_declenche_les_pieges_qu_il_rencontre() {
        let c = avec_entites(
            &[("piege_repulsif", (2, 0)), ("piege_mortel", (2, 2))],
            (3, 0),
            (0, 2),
            &[],
            &[(2, 1)],
        );
        assert_eq!(de(&c, "deplace", 2)[0]["vers"], serde_json::json!([2, 2]), "{c}");
        let noms: Vec<String> = de(&c, "degats", 2)
            .iter()
            .map(|e| e["nom"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(noms, vec!["Piège Mortel", "Piège Répulsif"], "le Mortel passe devant, mis en attente : {c}");
        let somme = de(&c, "degats", 2).iter().fold((0i64, 0i64), |(a, b), e| {
            (a + e["degats"][0].as_i64().unwrap(), b + e["degats"][1].as_i64().unwrap())
        });
        assert_eq!(c["entites"][2]["degats"], serde_json::json!([somme.0, somme.1]));
        assert_eq!(de(&c, "deplace", 0)[0]["vers"], serde_json::json!([5, 0]));
    }

    /// ⚠️ LE SCÉLÉRAT AMÈNE LES AUTRES VERS CELUI QUI L'A DÉCLENCHÉ.
    ///
    /// Il part quand on entre sur sa seule case et attire de 3 sur les quatre
    /// bras de sa croix. La cible, sur le centre, ne bouge pas ; l'allié posé
    /// en (6,2), sur un bras, vient d'un pas diagonal et bute contre elle en
    /// (5,1).
    #[test]
    fn le_scelerat_amene_les_autres_vers_celui_qui_l_a_declenche() {
        let c = avec_entites(&[("piege_scelerat", (4, 0))], (4, 0), (0, 0), &[(6, 2)], &[]);
        assert!(de(&c, "deplace", 0).is_empty(), "la cible reste sur le centre : {c}");
        let allie = &de(&c, "deplace", 2)[0];
        assert_eq!(allie["vers"], serde_json::json!([5, 1]), "{c}");
        assert_eq!(etapes(&c, "fin")[0]["case"], serde_json::json!([4, 0]));
    }

    /// ⚠️ SEULE LA CIBLE PORTE LA CONCENTRATION DE CHAKRA.
    ///
    /// Le Piège à Fragmentation frappe la cible et un second ennemi : un seul
    /// vol, après le coup de la cible.
    #[test]
    fn seule_la_cible_porte_le_chakra() {
        let mut r = requete(&[("piege_a_fragmentation", (4, 0))]);
        r.entree = Some((4, 0));
        r.ennemis = vec![(4, 3)];
        r.chakra = true;
        let c = reseau(&r)["chaine"].clone();
        assert_eq!(de(&c, "degats", 2).len(), 1, "le second ennemi est frappé : {c}");
        assert_eq!(etapes(&c, "chakra").len(), 1, "un seul vol : {c}");
    }
    /// Le réseau qu'un sort du Sram déclenche : le Sram en (0,0), l'ennemi en
    /// `depart`.
    fn avec_declencheur(poses: &[(&str, (i16, i16))], sort: &str, depart: (i16, i16)) -> serde_json::Value {
        let mut r = requete(poses);
        r.declencheur = Some(DeclencheurInput {
            sort: sort.to_string(),
            depart,
            vise: None,
        });
        reseau(&r)
    }

    /// Peur lancée du Sram en (0,0) sur la case `vise`, l'ennemi en `depart`.
    fn avec_peur(poses: &[(&str, (i16, i16))], depart: (i16, i16), vise: Option<(i16, i16)>) -> RequeteReseau {
        let mut r = requete(poses);
        r.declencheur = Some(DeclencheurInput {
            sort: "peur".to_string(),
            depart,
            vise,
        });
        r
    }

    /// Peur pousse l'ennemi au contact du Sram jusqu'à la case visée, et s'arrête au
    /// premier piège qu'il rencontre, comme toute poussée ; s'il bute avant, aucun
    /// dommage de poussée. Le Sram en (0,0), l'ennemi en (1,0), la case visée en
    /// (5,0).
    #[test]
    fn peur_pousse_l_ennemi_jusqu_a_la_case_visee() {
        let deplacement = |c: &serde_json::Value| etapes(c, "deplace")[0].clone();
        // Rien en chemin : il arrive sur la case visée, sans un dégât.
        let r = reseau(&avec_peur(&[], (1, 0), Some((5, 0))));
        assert!(r["declencheur"]["refus"].is_null(), "{}", r["declencheur"]);
        assert_eq!(r["declencheur"]["mouvement"], serde_json::json!({ "sens": "pousse_jusqu_a", "cases": 4 }));
        let d = deplacement(&r["chaine"]);
        assert_eq!((&d["nom"], &d["vers"], &d["fait"]), (&serde_json::json!("Peur"), &serde_json::json!([5, 0]), &serde_json::json!(4)));
        assert_eq!(r["chaine"]["total"], serde_json::json!([0, 0]), "{}", r["chaine"]);
        assert_eq!(r["pa_total"], 2);
        // La zone du Piège Sournois de (4,0) l'arrête en (3,0), où il part.
        let r = reseau(&avec_peur(&[("piege_sournois", (4, 0))], (1, 0), Some((5, 0))));
        let d = deplacement(&r["chaine"]);
        assert_eq!((&d["vers"], &d["arretee_par_piege"]), (&serde_json::json!([3, 0]), &serde_json::json!(true)));
        assert_eq!(etapes(&r["chaine"], "declenche")[0]["case"], serde_json::json!([3, 0]), "{}", r["chaine"]);
        // Un obstacle en (4,0) : il bute en (3,0), sans dommages de poussée.
        let mut bute = avec_peur(&[], (1, 0), Some((5, 0)));
        bute.obstacles = vec![(4, 0)];
        let r = reseau(&bute);
        let d = deplacement(&r["chaine"]);
        assert_eq!((&d["vers"], &d["bloquee"]), (&serde_json::json!([3, 0]), &serde_json::json!(true)), "{}", r["chaine"]);
        assert!(etapes(&r["chaine"], "poussee").is_empty(), "{}", r["chaine"]);
    }

    /// Peur vise une case libre, en ligne, de 2 à 8 cases du Sram, du côté de
    /// l'ennemi, qui se tient au contact. Sans ligne de vue : la donnée le dit.
    #[test]
    fn peur_vise_une_case_libre_en_ligne_l_ennemi_au_contact() {
        let refus = |r: RequeteReseau| reseau(&r)["declencheur"]["refus"].clone();
        assert_eq!(refus(avec_peur(&[], (2, 0), Some((5, 0)))), "l'ennemi doit se tenir au contact du Sram");
        assert_eq!(refus(avec_peur(&[], (1, 0), None)), "visez la case où pousser l'ennemi");
        // Le Sram au bord du damier : la case visée à neuf cases, sur le sol.
        let mut loin = avec_peur(&[], (-6, 0), Some((2, 0)));
        loin.lanceur = Some((-7, 0));
        assert_eq!(refus(loin), "la case visée est hors de portée du sort");
        assert_eq!(refus(avec_peur(&[], (1, 0), Some((3, 1)))), "le sort se lance en ligne, du côté de l'ennemi");
        assert_eq!(refus(avec_peur(&[], (1, 0), Some((-3, 0)))), "le sort se lance en ligne, du côté de l'ennemi");
        let mut prise = avec_peur(&[], (1, 0), Some((5, 0)));
        prise.allies = vec![(5, 0)];
        assert_eq!(refus(prise), "la case visée doit être libre");
        // Un obstacle entre l'ennemi et la case visée ne coupe rien : pas de vue.
        let mut cache = avec_peur(&[], (1, 0), Some((5, 0)));
        cache.obstacles = vec![(3, 0)];
        assert!(refus(cache).is_null());
    }

    /// Sournoiserie repousse l'ennemi dans le réseau. Lancée sur l'ennemi en (1,0),
    /// elle le frappe puis le repousse de trois cases ; la zone du Piège Sournois de
    /// (4,0) l'arrête en (3,0), où le piège part et l'attire sur son centre. Le
    /// total compte le coup du sort.
    #[test]
    fn sournoiserie_repousse_l_ennemi_dans_le_reseau() {
        let r = avec_declencheur(&[("piege_sournois", (4, 0))], "sournoiserie", (1, 0));
        assert!(r["declencheur"]["refus"].is_null(), "{}", r["declencheur"]);
        assert_eq!(r["declencheur"]["mouvement"], serde_json::json!({ "sens": "pousse", "cases": 3 }));
        let c = &r["chaine"];
        let coup = etapes(c, "coup");
        assert_eq!(coup.len(), 1, "{c}");
        let amene = &etapes(c, "deplace")[0];
        assert_eq!(
            (&amene["nom"], &amene["de"], &amene["vers"], &amene["fait"], &amene["arretee_par_piege"]),
            (&serde_json::json!("Sournoiserie"), &serde_json::json!([1, 0]), &serde_json::json!([3, 0]),
             &serde_json::json!(2), &serde_json::json!(true)),
            "{c}"
        );
        assert_eq!(etapes(c, "declenche")[0]["case"], serde_json::json!([3, 0]), "{c}");
        let sans_piege = avec_declencheur(&[], "sournoiserie", (1, 0));
        let coup_seul = sans_piege["chaine"]["total"][0].as_i64().unwrap();
        assert_eq!(coup_seul, coup[0]["degats"][0].as_i64().unwrap(), "{}", sans_piege["chaine"]);
        assert!(c["total"][0].as_i64().unwrap() > coup_seul, "{c}");
        // Les PA du tour comptent le sort : 2 pour le piège, 3 pour lui.
        assert_eq!(r["pa_total"], 5);
    }

    /// Le lancer suit les règles du sort : Sournoiserie se lance en ligne.
    #[test]
    fn un_declencheur_hors_de_ses_regles_est_refuse() {
        let r = avec_declencheur(&[("piege_sournois", (4, 0))], "sournoiserie", (1, 1));
        assert_eq!(r["declencheur"]["refus"], "le sort se lance en ligne");
        assert!(r["chaine"].is_null());
    }

    /// ⚠️ MÉPRISE : L'ENNEMI PREND LA CASE DU SRAM, et y déclenche les pièges
    /// qui la couvrent (à vérifier en jeu). Le Piège Sournois de (1,0) couvre
    /// (0,0) : l'ennemi parti de (2,0) y arrive, et le piège part.
    #[test]
    fn meprise_amene_l_ennemi_sur_la_case_du_sram() {
        let r = avec_declencheur(&[("piege_sournois", (1, 0))], "meprise", (2, 0));
        assert!(r["declencheur"]["refus"].is_null(), "{}", r["declencheur"]);
        let c = &r["chaine"];
        let echange = &etapes(c, "echange")[0];
        assert_eq!((&echange["cible"], &echange["sram"]), (&serde_json::json!([0, 0]), &serde_json::json!([2, 0])));
        assert_eq!(etapes(c, "declenche")[0]["case"], serde_json::json!([0, 0]), "{c}");
    }

    /// Guet-apens attire l'ennemi de deux cases vers le Sram, dans le réseau.
    #[test]
    fn guet_apens_attire_l_ennemi_dans_le_reseau() {
        let r = avec_declencheur(&[("piege_sournois", (2, 0))], "guet_apens", (4, 0));
        let c = &r["chaine"];
        let amene = &etapes(c, "deplace")[0];
        assert_eq!((&amene["sens"], &amene["vers"]), (&serde_json::json!("attire"), &serde_json::json!([3, 0])), "{c}");
        assert_eq!(etapes(c, "declenche")[0]["case"], serde_json::json!([3, 0]), "{c}");
    }

    /// Le plan du concepteur pour cette consigne, sur ce plateau, sans Sram
    /// sur le damier.
    fn plan(objectif: Objectif, tours: u8, pa: u8, carte: Option<u16>) -> serde_json::Value {
        let mut r = requete(&[]);
        r.lanceur = None;
        r.pa = Some(pa);
        r.carte = carte;
        r.concevoir = Some(Conception { objectif, tours });
        reseau(&r)["plan"].clone()
    }

    /// Le plan reposé à la main, tel que le simulateur le lit : ses poses à
    /// leur tour et la case d'entrée, sans Sram sur le damier.
    fn reposer(p: &serde_json::Value, pa: u8, carte: Option<u16>) -> serde_json::Value {
        let c = |v: &serde_json::Value| (v[0].as_i64().unwrap() as i16, v[1].as_i64().unwrap() as i16);
        let mut r = requete(&[]);
        r.poses = p["poses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| PoseInput {
                sort: x["sort"].as_str().unwrap().to_string(),
                case: c(&x["case"]),
                tour: x["tour"].as_u64().unwrap() as u8,
            })
            .collect();
        r.lanceur = None;
        r.entree = Some(c(&p["entree"]));
        r.pa = Some(pa);
        r.carte = carte;
        reseau(&r)
    }

    /// Le plan reposé passe toutes les règles du simulateur, et sa chaîne rend
    /// le total annoncé, sort de déclenchement compris.
    fn se_repose_tel_quel(p: &serde_json::Value, pa: u8, carte: Option<u16>) {
        let rep = reposer(p, pa, carte);
        assert!(rep["poses"].as_array().unwrap().iter().all(|x| x["refus"].is_null()), "{}", rep["poses"]);
        let sort = |i: usize| p["declencheur"]["degats"][i].as_i64().unwrap_or(0);
        let total = |i: usize| rep["chaine"]["total"][i].as_i64().unwrap() + sort(i);
        assert_eq!(serde_json::json!([total(0), total(1)]), p["total"], "{p}");
        let pa_du_sort = p["declencheur"]["pa"].as_u64().unwrap_or(0);
        assert_eq!(rep["pa_total"].as_u64().unwrap() + pa_du_sort, p["pa"].as_u64().unwrap(), "{p}");
    }

    /// Le plan dit ses entrées, sans que le joueur en donne : chaque case listée est
    /// libre, une voisine sans zone y mène, et la cible qui y entre prend tous les
    /// pièges, pour le total listé. La meilleure vient d'abord, et c'est celle du
    /// plan.
    #[test]
    fn le_plan_dit_ses_entrees() {
        let p = plan(Objectif::Frappe, 2, 6, None);
        let entrees = p["entrees"].as_array().unwrap();
        assert!(!entrees.is_empty(), "{p}");
        assert_eq!(entrees[0]["case"], p["entree"], "{p}");
        assert_eq!(entrees[0]["total"], p["total"], "{p}");
        let milieux: Vec<i64> = entrees
            .iter()
            .map(|e| e["total"][0].as_i64().unwrap() + e["total"][1].as_i64().unwrap())
            .collect();
        assert!(milieux.windows(2).all(|w| w[0] >= w[1]), "{milieux:?}");
        let pieges = p["poses"].as_array().unwrap().len();
        for e in entrees {
            let mut ici = p.clone();
            ici["entree"] = e["case"].clone();
            let rep = reposer(&ici, 6, None);
            let mut partis: Vec<u64> = etapes(&rep["chaine"], "declenche")
                .iter()
                .map(|x| x["piege"].as_u64().unwrap())
                .collect();
            partis.sort_unstable();
            partis.dedup();
            assert_eq!(partis.len(), pieges, "depuis {} : {}", e["case"], rep["chaine"]);
            assert_eq!(rep["chaine"]["total"], e["total"], "depuis {}", e["case"]);
        }
    }

    /// ⚠️ LE PLAN SE POSE TEL QUEL, et la chaîne rend le total annoncé : le
    /// concepteur et la main jouent au même jeu. La frappe maximale en un tour,
    /// l'ennemi amené à l'entrée.
    #[test]
    fn le_plan_d_une_frappe_se_pose_tel_quel() {
        let p = plan(Objectif::Frappe, 1, 6, None);
        assert!(p["raison"].is_null(), "{p}");
        assert!(p["declencheur"].is_null(), "{p}");
        assert!(p.get("lanceur").is_none(), "le plan ne place pas le Sram : {p}");
        se_repose_tel_quel(&p, 6, None);
        assert!(p["pa"].as_u64().unwrap() <= 6);
    }

    /// Le réseau en un tour compte le sort qui fait entrer la cible par la case
    /// d'entrée : ses PA, ses dégâts. Le joueur se place pour le lancer.
    #[test]
    fn le_plan_en_un_tour_compte_le_sort_qui_fait_entrer_la_cible() {
        let p = plan(Objectif::UnTour, 3, 7, None);
        assert!(p["raison"].is_null(), "{p}");
        let sort = p["declencheur"]["sort"].as_str().unwrap();
        assert!(DECLENCHEURS.contains(&sort), "{p}");
        assert!(p["declencheur"].get("depart").is_none(), "le plan ne place pas l'ennemi : {p}");
        assert!(p["poses"].as_array().unwrap().iter().all(|x| x["tour"] == 1), "un seul tour : {p}");
        se_repose_tel_quel(&p, 7, None);
        assert!(p["pa"].as_u64().unwrap() <= 7);
    }

    /// À prix égal, les autres sorts d'entrée se proposent aussi : le joueur lance
    /// celui qu'il a équipé et qu'il peut placer. À quatre PA, un sort de trois
    /// n'en laisse qu'un, trop peu pour un piège : le plan prend un sort de deux,
    /// Perquisition, qui frappe, et propose Peur, qui ne frappe pas.
    #[test]
    fn en_un_tour_les_sorts_de_meme_prix_se_proposent_aussi() {
        let p = plan(Objectif::UnTour, 1, 4, None);
        assert_eq!(p["declencheur"]["sort"], "perquisition", "{p}");
        let aussi: Vec<&str> = p["aussi"].as_array().unwrap().iter().map(|t| t["sort"].as_str().unwrap()).collect();
        assert_eq!(aussi, ["peur"], "{p}");
        // Au total d'un plan, chaque sort du même prix, le meilleur d'abord.
        let p = plan(Objectif::UnTour, 1, 12, None);
        let prix = p["declencheur"]["pa"].as_u64().unwrap();
        let mut tous: Vec<serde_json::Value> = vec![p["declencheur"].clone()];
        tous.extend(p["aussi"].as_array().unwrap().iter().cloned());
        assert!(tous.iter().all(|t| t["pa"].as_u64() == Some(prix)), "{p}");
        let milieux: Vec<i64> = tous.iter().map(|t| t["degats"][0].as_i64().unwrap() + t["degats"][1].as_i64().unwrap()).collect();
        assert!(milieux.windows(2).all(|w| w[0] >= w[1]), "{milieux:?}");
        let attendus = DECLENCHEURS.len() - if prix == 2 { 3 } else { 2 };
        assert_eq!(tous.len(), attendus, "{p}");
    }

    /// En un tour, tous les PA : les pièges et le sort qui fait entrer la cible
    /// prennent le tour entier, et il en reste moins que le prix du moins cher des
    /// pièges, deux PA.
    #[test]
    fn en_un_tour_le_plan_prend_tous_les_pa() {
        for pa in [7u8, 12] {
            let p = plan(Objectif::UnTour, 1, pa, None);
            let pris = p["pa"].as_u64().unwrap();
            assert!(pris <= u64::from(pa) && u64::from(pa) - pris < 2, "{pa} PA : {p}");
            assert_eq!(p["pa_par_tour"], serde_json::json!([pris]), "{p}");
        }
    }

    /// Chaque tour prend ses PA : un tour entamé se comble avant qu'un autre ne
    /// s'ouvre.
    #[test]
    fn chaque_tour_prend_ses_pa() {
        let p = plan(Objectif::Frappe, 3, 12, None);
        let par_tour: Vec<u64> = p["pa_par_tour"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
        assert_eq!(par_tour.len(), 3, "{p}");
        assert!(par_tour.iter().all(|pa| *pa <= 12 && 12 - pa < 2), "{par_tour:?}");
        se_repose_tel_quel(&p, 12, None);
    }

    /// Sur plusieurs tours, chaque tour tient dans ses PA et ses lancers, et
    /// les poses suivent l'ordre des tours.
    #[test]
    fn le_plan_se_construit_tour_par_tour() {
        let p = plan(Objectif::Frappe, 2, 4, None);
        assert!(p["raison"].is_null(), "{p}");
        let tours: Vec<u64> = p["poses"].as_array().unwrap().iter().map(|x| x["tour"].as_u64().unwrap()).collect();
        assert!(tours.contains(&1) && tours.contains(&2), "{p}");
        assert!(tours.windows(2).all(|w| w[0] <= w[1]), "{tours:?}");
        se_repose_tel_quel(&p, 4, None);
        // Deux tours valent davantage qu'un : le réseau grandit.
        let un = plan(Objectif::Frappe, 1, 4, None);
        assert!(p["total"][0].as_i64().unwrap() > un["total"][0].as_i64().unwrap(), "{p} contre {un}");
    }

    /// Dix tours au plus, et ils tiennent : le plan se repose sans refus, au total
    /// annoncé. Au-delà de dix, dix.
    #[test]
    fn le_plan_va_jusqu_a_dix_tours() {
        let p = plan(Objectif::Frappe, 12, 4, None);
        let tours = p["tours"].as_u64().unwrap();
        assert!((4..=10).contains(&tours), "{p}");
        assert_eq!(p["pa_par_tour"].as_array().unwrap().len() as u64, tours, "{p}");
        se_repose_tel_quel(&p, 4, None);
    }

    /// Sur une carte, le plan ne pose rien hors du sol, et le concepteur
    /// choisit l'emplacement : le joueur ne donne pas d'entrée.
    #[test]
    fn le_plan_tient_sur_la_carte() {
        let carte = crate::cartes::cartes().first().map(|c| c.id);
        let p = plan(Objectif::Frappe, 1, 6, carte);
        assert!(p["raison"].is_null(), "{p}");
        se_repose_tel_quel(&p, 6, carte);
    }

    /// En entrant par la case d'entrée, la cible prend tous les pièges, et cette
    /// case reste accessible : l'une de ses quatre voisines est du sol qu'aucune
    /// zone ne couvre.
    #[test]
    fn la_cible_entree_par_l_entree_prend_tous_les_pieges() {
        for (objectif, tours, pa) in [(Objectif::Frappe, 2, 6), (Objectif::UnTour, 1, 8)] {
            let p = plan(objectif, tours, pa, None);
            let rep = reposer(&p, pa, None);
            let mut partis: Vec<u64> = etapes(&rep["chaine"], "declenche")
                .iter()
                .map(|e| e["piege"].as_u64().unwrap())
                .collect();
            partis.sort_unstable();
            partis.dedup();
            assert_eq!(partis.len(), p["poses"].as_array().unwrap().len(), "{objectif:?} : {}", rep["chaine"]);
            let couvertes: Vec<serde_json::Value> =
                rep["cases"].as_array().unwrap().iter().map(|c| c["case"].clone()).collect();
            let (x, y) = (p["entree"][0].as_i64().unwrap(), p["entree"][1].as_i64().unwrap());
            let accessible = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
                let v = serde_json::json!([x + dx, y + dy]);
                (-7..=7).contains(&(x + dx)) && (-7..=7).contains(&(y + dy)) && !couvertes.contains(&v)
            });
            assert!(accessible, "{objectif:?} : entrée fermée {p}");
        }
    }

    /// Les entrées d'un plan, vues du large : en partant du sol libre au-delà
    /// du rectangle des zones de déclenchement, élargi de deux cases, et en
    /// marchant de voisine en voisine sans poser le pied sur aucune zone, on
    /// atteint une voisine de chaque entrée. Rend les entrées qu'on n'atteint
    /// pas.
    fn entrees_enfermees(p: &serde_json::Value, pa: u8, carte: Option<u16>) -> Vec<serde_json::Value> {
        let rep = reposer(p, pa, carte);
        let plateau = crate::cartes::Plateau::de(carte, None).unwrap();
        let mut zones: Vec<Case> = rep["poses"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|x| x["declenchement"].as_array().unwrap().iter())
            .map(|c| Case::new(c[0].as_i64().unwrap() as i16, c[1].as_i64().unwrap() as i16))
            .collect();
        zones.sort_unstable();
        zones.dedup();
        let (x0, x1) = (zones.iter().map(|c| c.x).min().unwrap() - 2, zones.iter().map(|c| c.x).max().unwrap() + 2);
        let (y0, y1) = (zones.iter().map(|c| c.y).min().unwrap() - 2, zones.iter().map(|c| c.y).max().unwrap() + 2);
        let libre = |c: &Case| plateau.est_sol(*c) && zones.binary_search(c).is_err();
        let mut atteintes: std::collections::BTreeSet<Case> = plateau
            .sol()
            .into_iter()
            .filter(|c| libre(c) && (c.x < x0 || c.x > x1 || c.y < y0 || c.y > y1))
            .collect();
        assert!(!atteintes.is_empty(), "aucun sol au large du réseau : {p}");
        let mut file: Vec<Case> = atteintes.iter().copied().collect();
        while let Some(c) = file.pop() {
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let v = Case::new(c.x + dx, c.y + dy);
                if libre(&v) && atteintes.insert(v) {
                    file.push(v);
                }
            }
        }
        p["entrees"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| {
                let (x, y) = (e["case"][0].as_i64().unwrap() as i16, e["case"][1].as_i64().unwrap() as i16);
                ![(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| atteintes.contains(&Case::new(x + dx, y + dy)))
            })
            .cloned()
            .collect()
    }

    /// Une entrée se rejoint du large : une voisine libre ne suffit pas si les zones
    /// des autres pièges l'enferment. Sur la carte de l'Arbre de Moon, en trois et
    /// quatre tours de 12 PA avec la Concentration de Chakra, chaque entrée listée
    /// se rejoint du large.
    #[test]
    fn chaque_entree_se_rejoint_du_large() {
        let carte = Some(151); // Arbre de moon
        for tours in [3u8, 4] {
            let mut r = requete(&[]);
            r.lanceur = None;
            r.pa = Some(12);
            r.carte = carte;
            r.chakra = true;
            r.concevoir = Some(Conception { objectif: Objectif::Frappe, tours });
            let p = reseau(&r)["plan"].clone();
            assert!(p["raison"].is_null(), "{p}");
            assert!(!p["entrees"].as_array().unwrap().is_empty(), "{p}");
            let enfermees = entrees_enfermees(&p, 12, carte);
            assert!(enfermees.is_empty(), "{tours} tours : entrées enfermées {enfermees:?} dans {p}");
        }
    }

    /// Un piège qui ne part pas fait échouer « tous partent » : le concepteur
    /// ne rend jamais un réseau qui en porte un. Le Mortel posé sur l'entrée
    /// part ; le Sournois posé au loin, jamais.
    #[test]
    fn un_piege_qui_ne_part_pas_est_refuse() {
        let modeles = modeles_du_sram();
        let modele = |sort: &str| modeles.iter().find(|m| m.sort == sort).unwrap();
        let entree = Case::new(3, 0);
        let (terrain, _) = duel(entree);
        let pieges = vec![
            modele("piege_mortel").poser(0, entree),
            modele("piege_sournois").poser(1, Case::new(-6, 5)),
        ];
        assert!(tous_partent(&chaine(&pieges[..1], entree, &terrain), 1));
        assert!(!tous_partent(&chaine(&pieges, entree, &terrain), 2));
    }

    /// À dégâts égaux, le plus compact, puis le moins de PA, puis le moins de
    /// tours. Les dégâts passent d'abord : un réseau plus large mais meilleur reste
    /// devant.
    #[test]
    fn a_valeur_egale_le_reseau_le_plus_compact() {
        let un = |valeur: f64, encombrement: u32, tours: u8| Plan {
            poses: Vec::new(),
            pa: 6,
            total: (0, 0),
            valeur,
            encombrement,
            tours,
        };
        assert_eq!(un(100.0, 4, 2).avant(&un(100.0, 9, 1)), std::cmp::Ordering::Less);
        assert_eq!(un(100.0, 9, 1).avant(&un(101.0, 4, 1)), std::cmp::Ordering::Greater);
        assert_eq!(un(100.0, 4, 1).avant(&un(100.0, 4, 2)), std::cmp::Ordering::Less);
        assert_eq!(
            encombrement([Case::new(1, 0), Case::new(3, 2)].into_iter(), Case::new(0, 0)),
            12,
            "un rectangle de quatre sur trois"
        );
    }

    /// La palette du Sram, comme le générateur la voit, avec le build des
    /// tests.
    fn modeles_du_sram() -> Vec<Modele> {
        let build = requete(&[]).build.normalise();
        let resolved = resolve_build(&build).expect("le build de test");
        let (distance, melee) = multiplicateurs(&resolved);
        modeles_de(
            &load_ruleset(4).expect("les règles du Sram"),
            &snapshot_for(4).expect("l'instantané du Sram"),
            &resolved.profile,
            distance,
            melee,
        )
    }

    fn duel(entree: Case) -> (Terrain<'static>, Vec<Camp>) {
        let terrain = Terrain {
            obstacles: &[],
            lanceur: Some(Case::new(0, 0)),
            autres: &[],
            ordre: Ordre::Pose,
            chakra: None,
            poussee: Poussee::default(),
        };
        let camps = entites(entree, &terrain).iter().map(|e| e.camp).collect();
        (terrain, camps)
    }

    fn refus(r: &RequeteReseau) -> Vec<serde_json::Value> {
        reseau(r)["poses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["refus"].clone())
            .collect()
    }

    /// ⚠️ UN MODÈLE DÉPLACÉ REND LE PIÈGE POSÉ SUR PLACE.
    ///
    /// Le générateur lit chaque piège une fois, en (0,0), puis le déplace sur
    /// des centaines de cases. Ce n'est juste que si ses zones ne dépendent pas
    /// de la direction du lancer : le test le vérifie sur toute la palette.
    #[test]
    fn un_modele_deplace_rend_le_piege_pose_sur_place() {
        let build = requete(&[]).build.normalise();
        let resolved = resolve_build(&build).unwrap();
        let (distance, melee) = multiplicateurs(&resolved);
        let snap = snapshot_for(4).unwrap();
        let modeles = modeles_du_sram();
        assert!(modeles.len() >= 11, "la palette du Sram : {}", modeles.len());
        for modele in &modeles {
            let pg = piege(&snap, modele.dofusdb_id).unwrap();
            for centre in [Case::new(3, -2), Case::new(-4, 5)] {
                assert_eq!(
                    modele.poser(7, centre),
                    piege_au_sol(&pg, modele.dofusdb_id, 7, centre, &resolved.profile, distance, melee),
                    "{}",
                    modele.nom
                );
            }
        }
    }

    /// ⚠️ UNE POSE HORS DES CASES PARCOURUES NE CHANGE RIEN, et c'est ce qui
    /// permet au générateur de ne chercher que là.
    ///
    /// Le Répulsif de (2,0) pousse la cible entrée en (3,0) jusqu'au Sournois de
    /// (6,0), qui l'attire sur son centre : elle entre en (3,0), (4,0), (5,0) et
    /// (6,0). Tout piège dont la zone de déclenchement évite ces cases laisse la
    /// trace identique, où qu'il soit posé.
    #[test]
    fn une_pose_hors_des_cases_parcourues_ne_change_rien() {
        let modeles = modeles_du_sram();
        let entree = Case::new(3, 0);
        let (terrain, _) = duel(entree);
        let modele = |sort: &str| modeles.iter().find(|m| m.sort == sort).unwrap();
        let au_sol = vec![
            modele("piege_repulsif").poser(0, Case::new(2, 0)),
            modele("piege_sournois").poser(1, Case::new(6, 0)),
        ];
        let trace = chaine(&au_sol, entree, &terrain);
        let parcourues = cases_parcourues(&Arrivee::Entree(entree), &trace);
        assert_eq!(
            parcourues,
            vec![Case::new(3, 0), Case::new(4, 0), Case::new(5, 0), Case::new(6, 0)]
        );
        let mut essayees = 0;
        for m in &modeles {
            for x in -2..=9 {
                for y in -5..=6 {
                    let c = Case::new(x, y);
                    let pose = m.poser(2, c);
                    if au_sol.iter().any(|p| p.centre == c) || pose.declenchement.iter().any(|d| parcourues.contains(d)) {
                        continue;
                    }
                    let mut tous = au_sol.clone();
                    tous.push(pose);
                    assert_eq!(chaine(&tous, entree, &terrain), trace, "{} en {c:?}", m.nom);
                    essayees += 1;
                }
            }
        }
        assert!(essayees > 1000, "{essayees} poses essayées");
    }

    /// ⚠️ LA PROPOSITION SE POSE TELLE QUELLE, et la chaîne rend le total
    /// annoncé.
    ///
    /// Reposées à la main au même tour, les poses proposées passent toutes les
    /// règles du simulateur : le générateur et la main jouent au même jeu.
    #[test]
    fn la_proposition_se_pose_telle_quelle_et_tient_ses_chiffres() {
        let mut r = requete(&[]);
        r.entree = Some((3, 0));
        r.pa = Some(8);
        r.proposer = true;
        let p = reseau(&r)["proposition"].clone();
        let poses: Vec<(String, (i16, i16))> = p["poses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| {
                let c = |i: usize| i16::try_from(x["case"][i].as_i64().unwrap()).unwrap();
                (x["sort"].as_str().unwrap().to_string(), (c(0), c(1)))
            })
            .collect();
        assert!(poses.len() >= 2, "huit PA posent plusieurs pièges : {p}");
        assert!(p["pa"].as_u64().unwrap() <= 8);

        let mut main = requete(&poses.iter().map(|(s, c)| (s.as_str(), *c)).collect::<Vec<_>>());
        main.entree = Some((3, 0));
        main.pa = Some(8);
        let rep = reseau(&main);
        assert!(
            rep["poses"].as_array().unwrap().iter().all(|x| x["refus"].is_null()),
            "{}",
            rep["poses"]
        );
        assert_eq!(rep["chaine"]["total"], p["total"]);
        assert_eq!(rep["pa_total"], p["pa"]);
    }

    /// ⚠️ LE GÉNÉRATEUR COMPLÈTE LE TOUR EN COURS, avec ce qui est au sol.
    ///
    /// Un Mortel posé à la main sur l'entrée prend 3 des 6 PA du tour : il en
    /// reste 3, et la chaîne compte déjà le Mortel avant toute proposition.
    #[test]
    fn le_generateur_complete_le_tour_en_cours() {
        let mut r = requete(&[("piege_mortel", (3, 0))]);
        r.entree = Some((3, 0));
        r.pa = Some(6);
        r.proposer = true;
        let p = reseau(&r)["proposition"].clone();
        assert_eq!(p["pa_restants"], 3, "{p}");
        assert!(p["pa"].as_u64().unwrap() <= 3, "{p}");
        assert!(p["avant"][0].as_i64().unwrap() > 0, "le Mortel compte déjà : {p}");
        assert!(!p["poses"].as_array().unwrap().is_empty(), "3 PA suffisent à un piège : {p}");
    }

    /// ⚠️ UNE SEULE VARIANTE PAR PAIRE, sol compris.
    ///
    /// Le Répulsif et l'Effroyable, le Mortel et la Calamité, le Scélérat et la
    /// Fragmentation, l'Insidieux et la Dérive : le jeu n'en laisse équiper
    /// qu'un de chaque paire, et on ne change pas de variante en combat.
    #[test]
    fn le_generateur_ne_pose_qu_une_variante_par_paire() {
        let snap = snapshot_for(4).unwrap();
        let ruleset = load_ruleset(4).unwrap();
        let paire = |sort: &str| {
            let id = ruleset.spells.iter().find(|s| s.id == sort).unwrap().dofusdb_id.unwrap();
            snap.spells.iter().find(|s| s.id == id).unwrap().variant_group
        };
        for sol in [vec![], vec![("piege_effroyable", (2, 0))]] {
            let mut r = requete(&sol);
            r.entree = Some((3, 0));
            r.pa = Some(10);
            r.proposer = true;
            let p = reseau(&r)["proposition"].clone();
            let mut sorts: Vec<String> = sol.iter().map(|(s, _)| (*s).to_string()).collect();
            sorts.extend(p["poses"].as_array().unwrap().iter().map(|x| x["sort"].as_str().unwrap().to_string()));
            for a in &sorts {
                for b in &sorts {
                    assert!(
                        a == b || paire(a).is_none() || paire(a) != paire(b),
                        "{a} et {b} sont deux variantes d'une même paire : {sorts:?}"
                    );
                }
            }
        }
    }

    /// Le générateur laisse leur case aux pièges des tours suivants : au tour 1
    /// d'un plan, la page n'envoie que ce qui est au sol, et proposer les cases des
    /// tours 2 et 3 ferait refuser ces pièges-là une fois le tour posé.
    #[test]
    fn le_generateur_laisse_leur_case_aux_pieges_des_tours_suivants() {
        let mut r = requete(&[]);
        r.entree = Some((3, 0));
        r.pa = Some(8);
        r.proposer = true;
        let cases = |r: &RequeteReseau| -> Vec<(i16, i16)> {
            reseau(r)["proposition"]["poses"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| (x["case"][0].as_i64().unwrap() as i16, x["case"][1].as_i64().unwrap() as i16))
                .collect()
        };
        let libres = cases(&r);
        assert!(!libres.is_empty());
        r.reservees = libres.clone();
        let autres = cases(&r);
        assert!(!autres.is_empty(), "d'autres cases restent");
        assert!(autres.iter().all(|c| !libres.contains(c)), "{autres:?} reprend {libres:?}");
    }

    /// ⚠️ UNE POSE EXIGE UNE CASE LIBRE : ni le Sram, ni un allié, ni un
    /// ennemi qui s'y tient, ni un obstacle. L'entrée, elle, reste permise :
    /// l'ennemi n'y arrive qu'une fois le réseau posé.
    #[test]
    fn une_pose_exige_une_case_libre() {
        let mut r = requete(&[
            ("piege_repulsif", (2, 1)),
            ("piege_scelerat", (3, 2)),
            ("piege_de_derive", (1, 2)),
            ("piege_fangeux", (0, 0)),
            ("piege_mortel", (4, 0)),
        ]);
        r.allies = vec![(2, 1)];
        r.ennemis = vec![(3, 2)];
        r.obstacles = vec![(1, 2)];
        r.entree = Some((4, 0));
        let libre = serde_json::json!("la case doit être libre");
        assert_eq!(
            refus(&r),
            vec![libre.clone(), libre.clone(), libre.clone(), libre, serde_json::Value::Null]
        );
    }

    /// ⚠️ LE MORTEL SE LANCE EN LIGNE, ce que dit la donnée du sort.
    #[test]
    fn le_mortel_se_lance_en_ligne() {
        let r = requete(&[("piege_mortel", (2, 1)), ("piege_mortel", (0, 3))]);
        assert_eq!(
            refus(&r),
            vec![serde_json::json!("se lance en ligne"), serde_json::Value::Null]
        );
    }

    /// ⚠️ LES LANCERS ET LES PA SE COMPTENT PAR TOUR.
    ///
    /// Un Sournois par tour : le second du tour 1 est refusé, celui du tour 2
    /// passe. Quatre PA par tour : le Répulsif qui suit un Mortel au tour 1 ne
    /// tient pas, le même au tour 2 si.
    #[test]
    fn les_lancers_et_les_pa_se_comptent_par_tour() {
        let rien = serde_json::Value::Null;
        let mut r = requete(&[
            ("piege_sournois", (3, 0)),
            ("piege_sournois", (4, 2)),
            ("piege_sournois", (5, -1)),
        ]);
        r.poses[2].tour = 2;
        assert_eq!(
            refus(&r),
            vec![rien.clone(), serde_json::json!("plus de lancers que le tour n'en permet"), rien.clone()]
        );

        let mut r = requete(&[
            ("piege_mortel", (3, 0)),
            ("piege_repulsif", (2, 2)),
            ("piege_repulsif", (2, 3)),
        ]);
        r.pa = Some(4);
        r.poses[2].tour = 2;
        assert_eq!(
            refus(&r),
            vec![rien.clone(), serde_json::json!("plus de PA que le tour n'en a"), rien]
        );
    }

    /// ⚠️ L'AUTRE VARIANTE D'UNE PAIRE EST REFUSÉE, à la main comme au
    /// générateur.
    #[test]
    fn l_autre_variante_d_une_paire_est_refusee() {
        let r = requete(&[
            ("piege_repulsif", (2, 0)),
            ("piege_effroyable", (4, 1)),
            ("piege_repulsif", (5, 2)),
        ]);
        assert_eq!(
            refus(&r),
            vec![
                serde_json::Value::Null,
                serde_json::json!("l'autre variante est déjà posée : le jeu n'en laisse équiper qu'une"),
                serde_json::Value::Null,
            ]
        );
    }
}

