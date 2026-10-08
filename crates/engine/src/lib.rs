//! Multi-turn rotation solver.
//!
//! Two nested searches:
//!
//! * within a turn, enumerate reachable end-of-turn states under the AP budget,
//!   cast limits, cooldowns and resource rules;
//! * across turns, dynamic programming over the states that survive a turn.
//!
//! Cost is roughly `horizon * |inter-turn states| * |within-turn states|`, and
//! each state count is a product of per-mechanic domains: the wall is mechanic
//! count rather than spell count, and one spell carrying a persistent state
//! costs more than six stateless ones.

#![forbid(unsafe_code)]

mod howard;

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

pub use dofus_damage::Reach;

use dofus_damage::{
    compat_midpoint_crit, expected_line, raw_line, CastContext, CritRate, Damage, DamageProfile,
    Delivery, Element, FinalMultiplier, Resistance, SpellLine, SCALE,
};
pub use dofus_grid::Case;
use dofus_ruleset::{
    AreaDef, Aim, AtCap, BaseBonus, Condition, DamageModifier, Effect, LineDef, Maybe, Monotone,
    OnExpire, Refresh, RequiresAt, Ruleset, SpellDef, StateTrigger as StateTriggerDef,
};

/// A small multiply-xor hasher for solver states.
///
/// The standard library defaults to SipHash, which is the right choice for maps
/// keyed on untrusted input. These maps are keyed on states the solver itself
/// produced, and they are probed millions of times per solve, so the
/// cryptographic strength is pure cost.
#[derive(Default, Clone, Copy)]
pub struct StateHasher(u64);

impl std::hash::Hasher for StateHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0 ^ value).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 ^= self.0 >> 29;
    }
    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }
    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }
    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
    fn write_i16(&mut self, value: i16) {
        self.write_u64(value as u64);
    }
}

#[derive(Default, Clone, Copy)]
pub struct StateHash;

impl std::hash::BuildHasher for StateHash {
    type Hasher = StateHasher;
    fn build_hasher(&self) -> StateHasher {
        StateHasher(0)
    }
}

type Map<K, V> = HashMap<K, V, StateHash>;

/// Le plafond dur : `applied` et `gain_proc` sont des u32 où chaque ressource
/// prend un bit. Au-delà, il faut changer leur type, pas ce chiffre.
pub const MAX_RESOURCES: usize = 32;
/// Spells per engine; the masks alongside it are u32, which is the hard ceiling.
pub const MAX_SPELLS: usize = 32;
pub const MAX_BUDGETS: usize = 4;
pub const MAX_SCHEDULES: usize = 4;
pub const MAX_STATE_TRIGGERS: usize = 16;

/// Whether stealing AP from the target is assumed to work. It is not: a steal
/// is opposed by the target's AP-removal resistance, and endgame monsters carry
/// enough of it that the attempt fails, so counting the stolen AP would spend
/// action points the turn never has. A named constant rather than a deletion
/// from the rulesets, because the game data does say the spell steals AP; the
/// spells concerned declare the assumption so the player reads it too.
pub const AP_THEFT_LANDS: bool = false;

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Always,
    OddTurns,
    EvenTurns,
}

/// Le lanceur, la case visée et les ennemis, en cases de grille. Une seule
/// visée pour tout le tour : le solveur ne déplace personne entre deux sorts,
/// comme `pm_depenses` déclare une dépense sans simuler le trajet.
#[derive(Clone, Debug)]
pub struct Placement {
    pub lanceur: Case,
    pub visee: Case,
    pub ennemis: Vec<Case>,
}

/// Où se tiennent les ennemis, pour la seule chose qui en dépende : combien de
/// fois une ligne de zone frappe, et à quel taux.
///
/// * `Declarees` : le joueur annonce un nombre d'ennemis et un étalement, et la
///   cible `i` se tient à `i * etalement` cases de l'impact.
/// * `Placees` porte des positions sur la grille : le moteur dessine la zone de
///   chaque ligne, regarde qui tombe dedans, et lit la distance.
///
/// Une forme que la grille ne sait pas dessiner retombe sur le régime déclaré,
/// et `placement_ignore` la nomme.
#[derive(Clone, Debug)]
pub enum Cibles {
    Declarees {
        combien: u8,
        etalement: u8,
    },
    Placees {
        lanceur: Case,
        visee: Case,
        ennemis: Vec<Case>,
    },
}

impl Default for Cibles {
    fn default() -> Self {
        Cibles::Declarees {
            combien: 1,
            etalement: 0,
        }
    }
}

impl Cibles {
    /// Ce que le scénario met en jeu : le placement s'il y en a un, sinon la
    /// déclaration.
    pub fn depuis(scenario: &Scenario) -> Cibles {
        match &scenario.placement {
            Some(p) => Cibles::Placees {
                lanceur: p.lanceur,
                visee: p.visee,
                ennemis: p.ennemis.clone(),
            },
            None => Cibles::Declarees {
                combien: scenario.targets,
                etalement: scenario.etalement,
            },
        }
    }

    /// Combien d'ennemis le scénario met en jeu, toutes zones confondues : non le
    /// nombre de cibles d'une ligne, bornée par sa propre zone, mais le plafond
    /// commun que lisent les effets qui comptent des ennemis sans dessiner de zone.
    pub fn nombre(&self) -> u8 {
        match self {
            Cibles::Declarees { combien, .. } => (*combien).max(1),
            Cibles::Placees { ennemis, .. } => {
                u8::try_from(ennemis.len()).unwrap_or(u8::MAX).max(1)
            }
        }
    }

    /// Les lignes dont la zone n'a pas pu être dessinée, retombées sur le régime
    /// déclaré : le seul moyen pour un appelant de savoir que le placement n'a pas
    /// servi sur ces sorts-là.
    pub fn placement_ignore(&self, ruleset: &Ruleset) -> Vec<String> {
        let Cibles::Placees { lanceur, visee, .. } = self else {
            return Vec::new();
        };
        let mut out = Vec::new();
        // Les modes d'un sort ont leurs propres lignes, et leur nom dit lequel.
        for spell in ruleset.spells.iter().flat_map(SpellDef::alternatives) {
            for (i, line) in spell.lines.iter().enumerate() {
                // Ni le rebond ni la ligne qui frappe chaque ennemi ne lisent
                // leur zone.
                if line.every_target || line.rebound_within.is_some() {
                    continue;
                }
                let Some(area) = &line.area else { continue };
                if cases_de_la_zone(area, *lanceur, *visee).is_none() {
                    out.push(format!(
                        "{} (ligne {i}, forme {})",
                        spell.name.fr, area.shape
                    ));
                }
            }
        }
        out
    }
}

/// Les cases qu'une zone couvre, ou `None` quand la grille ne sait pas la
/// dessiner.
///
/// Le cas `l` est à part et c'est la grille qui le dit : la ligne depuis le
/// lanceur n'a pas de longueur propre, la sienne est la distance de tir.
fn cases_de_la_zone(area: &AreaDef, lanceur: Case, visee: Case) -> Option<Vec<Case>> {
    let taille = u16::from(area.size.unwrap_or(0));
    let taille2 = u16::from(area.size2);
    dofus_grid::tir::tir(
        area.shape,
        taille,
        taille2,
        lanceur,
        visee,
        &[],
        dofus_grid::tir::Contraintes::libre(0, u16::MAX),
    )
    .ok()
    .map(|t| t.cases)
}

/// A multiplicative final-damage modifier that comes from the build, not from
/// any spell. Turn-parity bonuses are invisible in spell data and live here.
#[derive(Clone, Debug)]
pub struct BuildModifier {
    pub id: String,
    pub percent: u32,
    pub when: When,
}

#[derive(Clone, Debug)]
pub struct Build {
    pub name: String,
    pub profile: DamageProfile,
    pub base_ap: u8,
    /// Les PM du build. Sert à convertir « PM dépensés » en « fraction de PM
    /// restants », ce que le Zénith du Iop lit. Zéro vaut « inconnu », et la
    /// fraction est alors prise à un.
    pub base_mp: u8,
    pub crit_bonus_percent: i32,
    pub modifiers: Vec<BuildModifier>,
    /// Spell ids in play. This is where the elemental path and the variant
    /// selection live: one ruleset serves every path of a class.
    pub deck: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Crit-weighted expectation over the roll distribution, after resistance.
    /// The objective anyone should actually optimise.
    Expected,
    /// Midpoint of the critical range, every hit critical, no resistance: the
    /// assumptions of the original prototypes, kept so the engine can be compared
    /// with them cast for cast. It overstates every spell by
    /// `(1 - p) * (critical - normal)` per line.
    PrototypeCompat,
}

#[derive(Clone, Debug)]
pub struct Scenario {
    pub horizon: u8,
    /// La valeur de départ des compteurs que le joueur renseigne lui-même.
    ///
    /// Indexée comme les ressources du ruleset. Un compteur marqué
    /// `declared_by_player` démarre le combat ici et n'en bouge plus.
    pub etats_declares: Vec<(String, u8)>,
    /// Combien de PM le joueur compte dépenser dans le tour : une déclaration,
    /// comme « Ennemis touchés », pas une simulation du déplacement. Quatre sorts du
    /// parc en dépendent : la Tourbière de l'Enutrof monte de 5 par PM utilisé
    /// jusqu'à six, le Zénith du Iop ajoute une ligne entière proportionnelle aux PM
    /// restants.
    pub pm_depenses: u8,
    /// Combien de cases séparent deux ennemis touchés par une même zone, déclaré
    /// par le joueur : le jeu retire `falloff_percent` par cran d'éloignement de
    /// l'impact. À zéro, tout le monde est collé au centre et prend cent pour cent,
    /// ce qui est un plafond ; à un, la deuxième cible est une case plus loin, la
    /// troisième deux, et ainsi de suite.
    pub etalement: u8,
    /// Où se tiennent le lanceur et les ennemis, quand le joueur les a posés sur
    /// une grille. Présent, il l'emporte sur `targets` et `etalement` pour toute
    /// ligne dont la zone se dessine ; les deux champs déclarés restent le repli,
    /// pour les formes que la grille ne dessine pas et pour tout appelant qui ne
    /// place rien.
    #[allow(clippy::doc_markdown)]
    pub placement: Option<Placement>,
    /// Turn parity matters when the build carries a parity-conditional bonus,
    /// and a player bursting on combat turn 3 has the opposite parity to one
    /// opening on turn 1.
    pub starting_turn_is_odd: bool,
    pub resistance: Resistance,
    /// Drop end-of-turn outcomes that another outcome beats on every dimension
    /// surviving the turn.
    ///
    /// Lossless if, and only if, the monotonicity the ruleset declares is true.
    /// A false declaration does not crash anything: it silently returns a
    /// suboptimal rotation, which is the worst way for a solver to be wrong.
    /// Turning this off gives the prune-free search to check against, which is
    /// what `dominance_changes_nothing` does.
    pub dominance: bool,
    /// Skip spells proved never worth casting. Separate from `dominance` so the
    /// two prunings can be measured, and disproved, one at a time: folding them
    /// into one flag would have let either one take credit for the other's gain.
    ///
    /// `dominance: false` overrides this: the exhaustive search is exhaustive.
    pub prune_spells: bool,
    /// Player-asserted limits standing in for positional prerequisites, such as
    /// how many Telefrags are realistically reachable in a turn.
    pub budgets: Vec<(String, u8)>,
    /// Les poussées de la rotation butent contre un obstacle : chaque sort qui
    /// repousse l'ennemi ajoute ses dommages de poussée, toute sa distance comptée
    /// bloquée. Une déclaration du joueur : le solveur ne sait pas où sont les
    /// murs.
    pub poussees_bloquees: bool,
    /// Combien d'ennemis le joueur suppose toucher, pour voir comment la
    /// rotation change quand ils sont plusieurs.
    ///
    /// Chaque ligne de dégâts frappe au plus `min(targets, sa zone)` fois : un
    /// sort mono-cible reste à une frappe quel que soit ce nombre, ce qui est
    /// tout l'intérêt du réglage. Sans ce plafond le classement des sorts ne
    /// bougerait pas d'un cran, la valeur de chacun étant simplement
    /// multipliée par le même facteur.
    ///
    /// Un au minimum ; le solveur traite zéro comme un.
    pub targets: u8,
    pub mode: Mode,
    /// Where the player fights, for the spells whose own range does not settle
    /// it. A spell capped at range 1 is always melee and a spell starting at
    /// range 2 is always ranged whatever this says; everything in between
    /// follows it. This is a real choice with real consequences on a build
    /// carrying `% Dommages mêlée` or `% Dommages distance`, so it is asked
    /// rather than assumed.
    pub reach: Reach,
}

/// The reach a spell is actually cast at, given its own range and the player's
/// positioning.
pub fn reach_for(range: Option<(u8, u8)>, preference: Reach) -> Reach {
    match range {
        Some((_, max)) if max <= 1 => Reach::Melee,
        Some((min, _)) if min >= 2 => Reach::Ranged,
        _ => preference,
    }
}

// ---------------------------------------------------------------------------
// Compiled form
// ---------------------------------------------------------------------------

/// What a state does to every damage line while it is up.
///
/// Distinct from a line's own bonus, which touches only the spell declaring it.
/// Horloge raises its own base damage; a Iop's Puissance raises everything, and
/// no per-line bonus can express that.
#[derive(Clone, Copy, Debug)]
enum Modifier {
    /// Le pourcentage, sa valeur critique quand le jeu en donne une différente,
    /// et s'il s'agit de % de dommages finaux du lanceur, qui s'additionnent,
    /// plutôt que de dommages subis. Le solveur applique une valeur par lancer,
    /// celle du critique au seuil de [`dofus_ruleset::CRITICAL_EFFECT_THRESHOLD`].
    FinalMultiplier(u32, Option<u32>, bool),
    /// Le troisieme membre est l'element dont la caracteristique monte. Absent,
    /// les cinq montent : c'est la Puissance, qui n'a pas d'element. Present,
    /// c'est un vol, qui en a un.
    Characteristic(i32, Option<i32>, Option<Element>),
    BaseDamage(i32, Option<i32>),
    /// Dommages fixes, ajoutés après l'arrondi caractéristique.
    FlatDamage(i32, Option<i32>),
    /// Points de taux de coup critique. Le seul modificateur qui ne touche pas
    /// un étage du calcul mais le TIRAGE lui-même. Le second membre, la valeur
    /// d'un lancer critique, comme pour les autres.
    CritRate(i32, Option<i32>),
    /// Dommages Critiques fixes, ajoutés au bonus du build.
    CritDamage(i32, Option<i32>),
    /// Résistance Critique retirée à la CIBLE. Elle se soustrait du bonus
    /// critique de l'attaquant, donc en retirer en rend.
    CritResistance(i32, Option<i32>),
    /// Dommages Poussée : ils ne touchent aucune ligne, seulement la formule
    /// de poussée, que le lancer recalcule quand l'un d'eux est actif.
    PushDamage(i32, Option<i32>),
    /// Des % de dommages d'un domaine, un facteur à part : voir
    /// `DamageModifier::DomainPercent`. Ne vaut que pour un lancer de ce
    /// domaine.
    DomainPercent(Option<Delivery>, Option<Reach>, i32),
}

#[derive(Clone, Debug)]
struct Resource {
    /// L'identifiant du fichier, pour pouvoir NOMMER l'état dans la sortie.
    label: String,
    max: u8,
    default: u8,
    /// Declared in the ruleset. Licenses Pareto pruning, and is checked
    /// against a prune-free search rather than trusted.
    monotone: Monotone,
    duration: Option<(u8, Refresh, OnExpire)>,
    modifiers: Vec<Modifier>,
    /// Voir `ResourceDef::paliers` : les bonus de la valeur `v` sont
    /// `paliers[v - 1]`, appliqués une fois.
    paliers: Vec<Vec<Modifier>>,
    /// Voir `ResourceDef::pa_par_valeur`.
    pa_par_valeur: Vec<i8>,
    /// Voir `ResourceDef::cyclique`.
    cyclique: bool,
    /// Voir `ResourceDef::poussees_par_tour` : les cases de chaque poussée,
    /// vide quand le joueur ne déclare pas ses poussées bloquées.
    poussees: Vec<i32>,
    gain_per_turn: u8,
    /// Le compteur ne monte que tant que cet autre etat est present. Sans lui,
    /// `gain_per_turn` monte des le premier tour et sans qu'on ait rien lance.
    gain_per_turn_while: Option<usize>,
    gain_at_turn_end: u8,
    /// Ce compteur ne monte qu'une fois par tour, quel que soit le nombre de
    /// sorts qui le donnent.
    gain_once_per_turn: bool,
    /// Des PA donnes au debut de chaque tour ou l'etat est porte, par charge.
    grants_ap: u8,
    /// Le compteur ou passe cette valeur a la fin du tour. Voir
    /// [`dofus_ruleset::ResourceDef::shifts_into`].
    glisse_vers: Option<usize>,
    /// L'etat que l'expiration de celui-ci pose pour le tour qui suit. Voir
    /// [`dofus_ruleset::DurationDef::then_gain`].
    puis: Option<usize>,
    /// Ce compteur retombe au debut de chaque tour ou cet etat-la tient, apres
    /// les effets de debut de tour. Voir
    /// [`dofus_ruleset::ResourceDef::reset_at_turn_start_while`].
    remis_sous: Option<usize>,
    /// Ce compteur perd, au debut de chaque tour, autant de crans que cet
    /// etat-la en porte, apres les effets de debut de tour. Voir
    /// [`dofus_ruleset::ResourceDef::lose_at_turn_start_while`].
    perd_sous: Option<usize>,
    /// Le total dont se retire ce que ce compteur perd en fin de tour. Voir
    /// [`dofus_ruleset::ResourceDef::drains`].
    vide: Option<usize>,
    /// Les sorts compiles dont la relance retombe a zero quand cet etat
    /// s'acheve de lui-meme, en masque. Voir
    /// [`dofus_ruleset::DurationDef::then_reset_cooldowns`].
    relances_a_l_expiration: u32,
    /// Les compteurs parmi lesquels chaque point de celui-ci est une carte
    /// tirée au hasard. Voir [`dofus_ruleset::ResourceDef::random_among`].
    tirage_parmi: Vec<usize>,
}

#[derive(Clone, Debug)]
struct Line {
    element: Element,
    normal: Option<(i32, i32)>,
    critical: Option<(i32, i32)>,
    /// Voir `LineDef::facteur` : multiplie avec les autres %.
    facteur: Option<u32>,
    /// Voir `LineDef::dommages_fixes` : les Dommages d'un objet de classe,
    /// pour ce seul sort.
    dommages_fixes: i32,
    /// Voir `LineDef::puissance_propre` : la Maîtrise d'arme.
    puissance_propre: i32,
    /// Voir `LineDef::sans_critique`.
    sans_critique: bool,
    /// Voir `LineDef::invocation` : la part que l'invocateur transmet.
    invocation: Option<u32>,
    /// Voir `LineDef::taux_critique` : le taux propre de la ligne.
    crit: Option<CritRate>,
    per_resource: Vec<(usize, i32)>,
    /// `(compteur, garde, montant)` : ne compte que tant que la garde est
    /// presente. Vide partout sauf sur la Muselière de l'Ouginak, dont le
    /// bonus par ennemi au contact demande que la cible soit la Proie.
    per_resource_gated: Vec<(usize, usize, i32)>,
    /// Bonus par palier, quand chaque cran ne vaut pas le meme. Cumulatifs : a
    /// deux crans, la somme des deux premiers paliers. Au-dela de la liste, le
    /// dernier se repete.
    steps: Vec<(usize, Vec<i32>)>,
    while_resource: Vec<(usize, i32)>,
    /// La ligne frappe une fois par point de ces ressources, additionnees.
    /// Vide, elle frappe une fois tout court.
    repeats_per: Vec<usize>,
    /// Combien de fois cette ligne frappe sur le nombre de cibles du scénario,
    /// et à quel taux chacune. Calculé une fois à la compilation : le scénario
    /// ne change pas en cours de recherche.
    par_cible: Vec<u32>,
    /// Le nombre de cibles frappées est plafonné par la somme de ces compteurs.
    /// Vide, toutes les cibles de `par_cible` sont frappées.
    ///
    /// La Runification sur soi déclenche ses runes, pas ses ennemis : à deux
    /// runes vivantes et trois ennemis, elle frappe deux fois.
    plafond_cibles: Vec<usize>,
    /// La ligne ne frappe qu'à ces valeurs EXACTES, toutes à la fois. Chaque
    /// condition somme un ou plusieurs compteurs. Sert aux paliers qui changent
    /// de largeur et ne peuvent donc pas s'écrire en bonus de base.
    active_at: Vec<(Vec<usize>, u8)>,
    /// Le groupe de paliers qui s'excluent, et le palier de cette ligne, tous
    /// deux ramenés à un indice à la compilation. Un seul palier par groupe
    /// frappe : le premier déclaré dont les conditions tiennent.
    ///
    /// Sans lui, la Main de l'Ecaflip paierait deux fois. Quatre cartes d'une
    /// couleur chacune sont un Carré Couleurs ; si ce sont aussi Valet, Dame,
    /// Roi et As, c'est en plus une Suite Royale Couleurs. Le jeu n'en joue
    /// qu'une, la meilleure.
    groupe: Option<(usize, usize)>,
    /// Des dégâts en pourcentage de vie, déjà chiffrés : la valeur BRUTE, que
    /// seules les résistances réduisent. Aucune fourchette ne s'applique alors.
    brut: Option<i32>,
    /// Voir `LineDef::tirage`.
    tirage: Option<Tirage>,
}

/// La part d'une ligne dans son tirage (`LineDef::tirage`).
#[derive(Clone, Copy, Debug)]
struct Tirage {
    /// L'indice du tirage parmi ceux de la liste.
    groupe: usize,
    /// L'issue de la ligne, propre au tirage : les lignes d'une même issue
    /// partent ensemble.
    issue: usize,
    /// La chance de l'issue, en fraction : la ligne compte pour cette part.
    part: (u32, u32),
}

/// Les tirages d'une liste de lignes, ligne par ligne. Une ligne sans
/// `issue` est une issue à elle seule ; une issue sans `chance` partage
/// également le tirage avec les autres.
fn tirages(defs: &[LineDef]) -> Vec<Option<Tirage>> {
    let mut noms: Vec<&str> = Vec::new();
    // L'issue d'une ligne : la sienne, ou une à elle seule, après les autres.
    let issue_de = |i: usize, d: &LineDef| d.issue.map_or(256 + i, usize::from);
    defs.iter()
        .enumerate()
        .map(|(i, d)| {
            let nom = d.tirage.as_deref()?;
            let groupe = noms.iter().position(|n| *n == nom).unwrap_or_else(|| {
                noms.push(nom);
                noms.len() - 1
            });
            let mut issues: Vec<usize> = defs
                .iter()
                .enumerate()
                .filter(|(_, e)| e.tirage.as_deref() == Some(nom))
                .map(|(j, e)| issue_de(j, e))
                .collect();
            issues.sort_unstable();
            issues.dedup();
            let part = match d.chance {
                Some(c) => (u32::from(c), 100),
                None => (1, issues.len() as u32),
            };
            Some(Tirage { groupe, issue: issue_de(i, d), part })
        })
        .collect()
}

/// La vie du lanceur, pour les dégâts en pourcentage de vie : ses points de
/// vie, et les deux réglages que la classe déclare, en pour cent. Constante
/// toute la rotation : le % de vie restante déclaré est une intention de jeu,
/// soins et régénération compris, et le sacrifice du Châtiment ne le fait pas
/// baisser.
#[derive(Clone, Copy, Debug)]
struct Vie {
    max: i32,
    restante: u8,
    erosion: u8,
}

impl Vie {
    /// La valeur brute d'une ligne, arrondie à l'inférieur.
    fn degats(&self, p: &dofus_ruleset::PercentOfLife) -> i32 {
        let max = i64::from(self.max.max(0));
        let r = i64::from(self.restante.min(100));
        // La vie visée, en « points de vie × pour cent ».
        let part = match p.of {
            dofus_ruleset::LifeShare::Remaining => max * r,
            dofus_ruleset::LifeShare::Larger => max * r.max(100 - r),
            // L'érosion porte sur la vie perdue, toute perte venant de dommages
            // reçus : à 60 % de vie et 20 % d'érosion, 8 % de la vie max.
            dofus_ruleset::LifeShare::Eroded => {
                max * i64::from(self.erosion.min(100)) * (100 - r) / 100
            }
        };
        i32::try_from(part * i64::from(p.percent) / 10_000).unwrap_or(i32::MAX)
    }
}

#[derive(Clone, Debug)]
enum Eff {
    Damage,
    Gain {
        resource: usize,
        amount: u8,
        ap_bonus: Option<(u8, bool)>,
        budget: Option<usize>,
        at_cap: AtCap,
        requires: Option<Exigence>,
        /// Le compteur monte JUSQU'À `amount`, qui vaut alors le nombre
        /// d'ennemis que la zone du sort atteint.
        jusqu_a: bool,
        /// Seulement sur un coup critique : voir `Effect::Gain::on_critical`.
        si_critique: bool,
    },
    Consume {
        resource: usize,
        amount: u8,
        requires: Option<Exigence>,
    },
    /// Retire `amount` en puisant dans ces compteurs, dans l'ordre. Quand
    /// `zone` est vrai, `amount` vaut le nombre d'ennemis que la zone du sort
    /// atteint, posé par `compile_spell`.
    ConsumeAcross {
        resources: Vec<usize>,
        amount: u8,
        zone: bool,
    },
    StealAp {
        amount: u8,
        requires: Option<Exigence>,
    },
    CarryAp {
        amount: u8,
        cap: u8,
        requires: Option<Exigence>,
    },
    Reset {
        resource: usize,
        requires: Option<Exigence>,
    },
    Toggle {
        resource: usize,
    },
    Schedule {
        index: usize,
        /// Checked when the spell is cast. Unmet, nothing is armed.
        requires: Option<Exigence>,
    },
    /// Le tour du lanceur s'arrete : plus aucun PA.
    FinDuTour,
    /// La relance de ces sorts retombe a zero. Le masque des sorts compiles se
    /// resout une fois tout le deck compile : un sort peut en nommer un qui le
    /// suit.
    RemiseDesRelances {
        sorts: Vec<String>,
        masque: u32,
        requires: Option<Exigence>,
    },
}

/// Damage that lands later, on a countdown other events can shorten, as
/// compiled from the ruleset: the final-damage multipliers are not resolved
/// until the build is in hand.
#[derive(Clone, Debug)]
struct ScheduleSpec {
    label: String,
    /// Le sort qui arme la charge, à qui ses dégâts sont attribués.
    source: String,
    delay: u8,
    acceleration: Vec<(usize, u8, bool)>,
    lines: Vec<Line>,
    crit_rate: CritRate,
    /// La ressource qui apparaît à l'échéance, s'il y en a une. Une charge de
    /// la Fuite du Temps ne frappe pas : elle pose un Téléfrag au tour suivant.
    grants: Option<(usize, u8)>,
    requires_at_turn_end: Option<Exigence>,
}

#[derive(Clone, Debug)]
struct Schedule {
    label: String,
    source: String,
    delay: u8,
    /// (resource consumed, turns removed, resolve on reaching zero).
    acceleration: Vec<(usize, u8, bool)>,
    damage_odd: Damage,
    damage_even: Damage,
    grants: Option<(usize, u8)>,
    /// Condition relue une fois le tour joue, pour les charges qui s'arment a
    /// ce moment-la. `None` pour celles qui s'arment au lancer.
    requires_at_turn_end: Option<Exigence>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TriggerKind {
    TurnStart,
    Consumed(usize),
    /// Un sort portant cette étiquette vient de se résoudre.
    Tagged(u16),
    /// Un sort du lanceur vient d'infliger des dommages de poussée au porteur.
    Poussee,
}

/// Damage a state deals on its own, with no spell being cast.
#[derive(Clone, Debug)]
struct StateTrigger {
    label: String,
    /// Le sort qui applique l'état. Ses dégâts lui reviennent, pas au lancer
    /// qui a déclenché la détente : l'Aiguille pose l'état, la Réfraction ne
    /// fait que le consommer.
    source: String,
    state: usize,
    /// La place de cette détente parmi les `while_present` de l'état.
    effect: usize,
    /// Ses lignes compilées et leur taux de critique, pour les fourchettes
    /// que KrozZone affiche : `damage_odd` n'en garde que l'espérance.
    lines: Vec<Line>,
    crit_rate: CritRate,
    kind: TriggerKind,
    /// Charges de l'état dépensées à chaque déclenchement. Zéro pour une
    /// détente qui ne s'use pas, comme le poison de début de tour.
    spends: u8,
    once_per_turn: bool,
    /// La condition sans laquelle la détente ne part pas. Voir
    /// `StateEffect::requires`.
    requires: Option<Exigence>,
    /// Le compteur dont depend le montant, quand les lignes portent des
    /// paliers. `None` pour une detente qui frappe toujours pour la meme
    /// chose, ce qui est le cas de tous les poisons du parc sauf les Toxines
    /// du Sram, dont le poison monte de six crans.
    palier: Option<usize>,
    /// Un montant par valeur du compteur, indexe par elle. Longueur 1 quand il
    /// n'y a pas de palier, et le montant est alors une constante comme avant.
    ///
    /// Une table plutot qu'un calcul : `turn_start` tourne une fois par etat
    /// atteignable et n'a pas le droit d'allouer, alors que le domaine du
    /// compteur tient en quelques valeurs.
    damage_odd: Vec<Damage>,
    damage_even: Vec<Damage>,
}

impl StateTrigger {
    /// Le montant que cette detente vaut dans CET etat.
    fn damage(&self, res: &[u8], odd: bool) -> Damage {
        let table = if odd {
            &self.damage_odd
        } else {
            &self.damage_even
        };
        let k = self.palier.map_or(0, |r| usize::from(res[r]));
        // Le compteur ne depasse jamais son `max`, dont la table a la taille.
        // Le `min` est une ceinture, pas une conversion.
        table[k.min(table.len() - 1)]
    }
}

#[derive(Clone, Debug)]
struct Spell {
    id: String,
    name: String,
    /// Les étiquettes de ce sort, en masque : `removes_ap`, `removes_mp`,
    /// `removes_range`. Un masque plutôt que des chaînes, la comparaison
    /// tombant dans la boucle de recherche.
    tags: u16,
    ap_base: u8,
    /// L'arme du build : les % de dommages d'armes au lieu de ceux des sorts.
    arme: bool,
    /// Ce sort peut-il critiquer du tout. Un taux nul ne le dit pas : un sort
    /// à 0 % qui PEUT critiquer gagne les points qu'une ressource lui donne,
    /// un sort déclaré `can_crit: false` n'en gagne jamais.
    can_crit: bool,
    cost_reduction: Option<(usize, u8, u8)>,
    /// `(compteur, PA par point, plafond du surcoût)`. Le symétrique de
    /// `cost_reduction`, pour un sort qui coûte plus cher une fois lancé.
    cost_increase: Option<(usize, u8, u8)>,
    /// Effective single-target cap: the lower of the per-turn and per-target
    /// limits, which are different numbers and only coincide while there is one
    /// target. Gelure is 4 per turn but 2 per target.
    cast_cap: u8,
    /// Les sorts compilés qui sont des MODES du même sort du jeu, celui-ci
    /// compris, en masque d'indices. Un seul bit pour un sort sans mode.
    ///
    /// Ils se partagent les lancers du tour et la relance : trois
    /// Accumulations dans le tour, qu'elles visent un ennemi ou le lanceur.
    groupe: u32,
    /// Les lancers par tour du sort du jeu, que ses modes se partagent.
    /// `cast_cap` reste le plafond de CE mode, par cible comprise.
    quota: u8,
    /// Lancers supplémentaires gagnés dans le tour, selon une ressource :
    /// `(indice, par point, plafond)`. Le Régulateur du Xélor en gagne un
    /// quand la cible porte un Téléfrag.
    extra_casts: Option<(usize, u8, u8)>,
    cooldown: u8,
    /// Tours à attendre au début du combat avant le premier lancer
    /// (`initialCooldown`), autre chose que `cooldown`, l'intervalle entre deux
    /// lancers. Dix-neuf sorts du parc en portent un, tous à 1, dont le Pacte
    /// Bestial de l'Osamodas.
    initial_cooldown: u8,
    /// The schedule this spell starts, if any. A second cast is blocked while
    /// one is already pending.
    schedule: Option<usize>,
    crit_rate: CritRate,
    /// Casting range, straight from the ruleset. Decides melee vs ranged.
    range: Option<(u8, u8)>,
    /// Proved never worth casting on this build, so the search skips it.
    /// See [`Engine::prune_dominated`] for what "proved" means here.
    dominated_by: Option<String>,
    /// Reads no resource, writes no resource, and deals damage that depends on
    /// none: the only spells a static damage comparison can rank, so the only ones
    /// the reduction may drop. They also commute, but walking a single ordering of
    /// them was measured to buy nothing more.
    inert: bool,
    requires: Option<Exigence>,
    /// La condition de lancement de la donnée : Porteur pour le Pandawa, un
    /// masque pour le Zobal, la Lance en main pour le Forgelance.
    critere: Option<CritereCompile>,
    /// Chaque poussée du sort : sa condition sur l'état du lanceur, ses dommages
    /// aux Dommages Poussée du build, et ses cases, pour les recalculer quand un
    /// buff en ajoute. Vide si le joueur ne déclare pas que ses poussées butent.
    poussees: Vec<(Option<CritereCompile>, Damage, i32)>,
    /// Interdit tant que cette ressource est la, l'inverse de `requires`. Les
    /// huit glyphes elementaires du Feca s'excluent ainsi les uns les autres
    /// dans le tour.
    blocked_by: Option<usize>,
    /// Interdit tant que cette ressource est à son plafond.
    bloque_au_plafond: Option<usize>,
    /// `(a, b)` : n'est permis que tant que `a` dépasse `b`.
    exige_surplus: Option<(usize, usize)>,
    effects: Vec<Eff>,
    lines: Vec<Line>,
    /// Resources whose value changes this spell's damage. Almost always empty:
    /// only a spell like Glas, whose base scales with a stack count, has any.
    damage_deps: Vec<usize>,
    /// Damage for every reachable combination of `damage_deps`, one table per
    /// turn parity. The pipeline runs once per entry at compile time instead of
    /// once per node visited, which is several million times per solve.
    damage_odd: Vec<Damage>,
    damage_even: Vec<Damage>,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// What survives from one turn to the next.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct State {
    res: [u8; MAX_RESOURCES],
    timer: [u8; MAX_RESOURCES],
    cooldown: [u8; MAX_SPELLS],
    carry: u8,
    /// Zero means nothing pending. Otherwise the countdown, offset by one, so
    /// that a value of 1 means "resolves at the start of this turn".
    pending: [u8; MAX_SCHEDULES],
    /// Les etats a tick de debut de tour dont la duree s'acheve : encore la
    /// pour le tick, retires juste apres. Voir `advance`.
    expiring: u32,
}

/// A node partway through a turn.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Node {
    ap: i16,
    res: [u8; MAX_RESOURCES],
    used: [u8; MAX_SPELLS],
    ap_bonus_claimed: u32,
    budget_used: [u8; MAX_BUDGETS],
    carry: u8,
    applied: u32,
    fired: u32,
    pending: [u8; MAX_SCHEDULES],
    /// Charges dont le sort a ete lance ce tour-ci et dont la condition se lira
    /// une fois le tour joue. Le Gousset du Xelor en est : il consomme le
    /// Telefrag qu'il exige, donc seul l'etat final dit si sa glyphe se pose.
    arming: u32,
    /// State triggers that have already fired this turn, for the ones capped at
    /// once a turn. Turn-local, so it never reaches the outcome.
    state_proc: u32,
    /// Compteurs deja montes ce tour-ci, pour ceux plafonnes a un gain par
    /// tour. Un bit par ressource, et non par sort : le Choeur Strident de
    /// l'Eniripsa monte sur QUATRE sorts differents qui se partagent la meme
    /// permission. Turn-local comme `state_proc`.
    gain_proc: u32,
    /// Les sorts dont un lancer de ce tour a remis la relance a zero : ils
    /// repartent dans le tour meme, et `fired` ne garde que les lancers qui
    /// ont suivi la remise.
    remises: u32,
}

/// The part of a turn's outcome that the next turn can tell apart. Everything
/// else is collapsed, which is what stops cast orderings multiplying.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Outcome {
    res: [u8; MAX_RESOURCES],
    carry: u8,
    applied: u32,
    fired: u32,
    pending: [u8; MAX_SCHEDULES],
    arming: u32,
    /// Voir `Node::remises`.
    remises: u32,
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Cast {
    pub id: String,
    pub spell: String,
    pub ap_cost: u8,
    /// AP left once this cast is resolved, gains and steals included. What a
    /// player actually watches while playing a turn.
    pub ap_left: i16,
    /// Tout ce que ce lancer a fait tomber, procs d'états compris.
    pub damage: Damage,
    /// La part de `damage` qui revient à un autre sort : l'état qu'un tiers a
    /// posé et que ce lancer déclenche, `(id du sort source, dégâts)`. La
    /// répartition par sort la rend à qui de droit, plutôt qu'au sort qui
    /// consomme l'état.
    pub procs: Vec<(String, Damage)>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TurnPlan {
    pub turn: u8,
    pub odd: bool,
    /// Damage that lands before anything is cast: a scheduled payload coming
    /// due, a state ticking.
    pub opening_damage: Damage,
    /// D'où viennent ces dégâts d'ouverture : `(id du sort source, dégâts)`.
    pub opening_sources: Vec<(String, Damage)>,
    pub opening: Vec<String>,
    pub damage: Damage,
    pub casts: Vec<Cast>,
    /// AP left unspent at the end of the turn. A rotation that ends a turn with
    /// spare AP is one a player will not follow: every build runs 12 AP
    /// precisely so that nothing is left over.
    pub ap_left: i16,
}

#[derive(Clone, Debug)]
pub struct Solution {
    pub total: Damage,
    pub turns: Vec<TurnPlan>,
    pub inter_turn_states: usize,
}

#[derive(Debug)]
pub enum EngineError {
    UnknownSpell(String),
    TooMany(String),
    MissingData(Vec<String>),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::UnknownSpell(s) => {
                write!(f, "the deck names `{s}`, which the ruleset does not define")
            }
            EngineError::TooMany(s) => write!(f, "{s}"),
            EngineError::MissingData(gaps) => {
                writeln!(
                    f,
                    "cannot compute expected damage, {} value(s) are not recorded:",
                    gaps.len()
                )?;
                for g in gaps {
                    writeln!(f, "  - {g}")?;
                }
                write!(
                    f,
                    "read them off the in-game tooltips, or solve in prototype-compat mode"
                )
            }
        }
    }
}

impl std::error::Error for EngineError {}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Counters, so that optimisation work is aimed rather than guessed.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    /// Nodes visited inside turn searches.
    pub nodes: u64,
    /// Turn searches run. One per reachable (turn, state) pair, plus one per
    /// turn again when the winning plan is rebuilt.
    pub turn_searches: u64,
    /// Distinct end-of-turn outcomes kept across all turn searches.
    pub outcomes: u64,
    /// Nodes cut because an identical node had already been reached with at
    /// least as much damage.
    pub revisits: u64,
    /// End-of-turn outcomes dropped because another was at least as good on
    /// every dimension that survives the turn.
    pub dominated: u64,
}

/// Les mêmes compteurs que [`Stats`], mais partageables entre fils, pour
/// confier deux recherches à deux cœurs.
#[derive(Default)]
struct Counters {
    nodes: std::sync::atomic::AtomicU64,
    turn_searches: std::sync::atomic::AtomicU64,
    outcomes: std::sync::atomic::AtomicU64,
    revisits: std::sync::atomic::AtomicU64,
    dominated: std::sync::atomic::AtomicU64,
}

impl Counters {
    #[inline]
    fn add(champ: &std::sync::atomic::AtomicU64, n: u64) {
        champ.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
    }
    fn snapshot(&self) -> Stats {
        use std::sync::atomic::Ordering::Relaxed;
        Stats {
            nodes: self.nodes.load(Relaxed),
            turn_searches: self.turn_searches.load(Relaxed),
            outcomes: self.outcomes.load(Relaxed),
            revisits: self.revisits.load(Relaxed),
            dominated: self.dominated.load(Relaxed),
        }
    }
}

pub struct Engine {
    /// Recherches intra-tour déjà faites, indexées par l'état de départ et la
    /// parité du tour. Voir [`Engine::turn_outcomes`].
    outcomes_cache: std::sync::Mutex<OutcomeCache>,
    /// Spells the search skips, each with the spell that made it pointless.
    /// Surfaced rather than silently dropped: a tool that quietly ignores half
    /// a deck is indistinguishable from one that is broken.
    pruned: Vec<(String, String)>,
    resources: Vec<Resource>,
    spells: Vec<Spell>,
    schedules: Vec<Schedule>,
    state_triggers: Vec<StateTrigger>,
    budget_limits: Vec<u8>,
    /// `(compteur, valeur)` des réglages que le joueur renseigne, gardés pour les
    /// reposer au début de chaque tour : un compteur déclaré dit ce que le joueur
    /// fait dans un tour. La Chausse-trappe du Sram monte de huit dégâts de base par
    /// piège déclenché, et « les effets sont retirés après utilisation du sort » :
    /// sans remise à zéro, ses trois lancers du tour toucheraient le bonus ; sans
    /// ce repos, les tours suivants n'en toucheraient plus.
    declares: Vec<(usize, u8)>,
    build: Build,
    scenario: Scenario,
    duration_mask: u32,
    /// Les ressources qui portent un tick de debut de tour.
    ticking: u32,
    cooldown_mask: u32,
    /// Les états dont un sort du deck fait une condition de lancer : seuls ceux-là
    /// sont annotés dans la rotation, car eux seuls expliquent pourquoi un sort
    /// passe avant un autre.
    gating_mask: u32,
    /// Les compteurs qu'un autre remplit en glissant à la fin du tour.
    glissement_cibles: u32,
    stats: Counters,
}

impl Engine {
    pub fn new(ruleset: &Ruleset, build: Build, scenario: Scenario) -> Result<Engine, EngineError> {
        // Le placement l'emporte sur le nombre d'ennemis et l'étalement
        // déclarés, et retombe sur eux forme par forme. Construit une seule
        // fois : rien ne bouge pendant la résolution.
        let cibles = Cibles::depuis(&scenario);
        // Chaque réglage à sa valeur saisie, à défaut à celle que la classe
        // déclare, à défaut à celle du jeu : pleine vie, 10 % d'érosion.
        let reglage = |nom: &str, faute: u8| -> u8 {
            scenario
                .etats_declares
                .iter()
                .find(|(n, _)| n == nom)
                .map(|(_, v)| *v)
                .or_else(|| ruleset.resource(nom).map(|r| r.default))
                .unwrap_or(faute)
        };
        let vie = Vie {
            max: build.profile.life,
            restante: reglage("vie_restante", 100).min(100),
            erosion: reglage("erosion", 10).min(100),
        };
        // Un modificateur d'état tel que le moteur le lit. Une fermeture, et non
        // une boucle en place : les paliers d'un compteur (voir
        // `ResourceDef::paliers`) se compilent de la même façon.
        let compiler = |m: &DamageModifier| match m {
            DamageModifier::FinalMultiplier {
                percent,
                critical_percent,
                finaux,
            } => Modifier::FinalMultiplier(
                percent.known().copied().unwrap_or(100),
                *critical_percent,
                *finaux,
            ),
            DamageModifier::Characteristic {
                amount,
                critical_amount,
                element,
                per_target,
                per_target_cap,
                per_mp_used,
            } => {
                // Un bonus par cible touchée vaut autant de fois
                // que le scénario compte d'ennemis, sans dépasser
                // son Cumul, que seul DofusBook porte. Un bonus par
                // PM dépensé vaut autant de fois que le joueur en
                // déclare ; les deux ne se cumulent sur aucun sort
                // du parc.
                let fois = if *per_target {
                    let plafond = per_target_cap.unwrap_or(1);
                    i32::from(cibles.nombre().min(plafond))
                } else if *per_mp_used {
                    i32::from(scenario.pm_depenses)
                } else {
                    1
                };
                Modifier::Characteristic(
                    amount.known().copied().unwrap_or(0) * fois,
                    critical_amount.map(|c| c * fois),
                    *element,
                )
            }
            DamageModifier::BaseDamage {
                amount,
                critical_amount,
            } => Modifier::BaseDamage(
                amount.known().copied().unwrap_or(0),
                *critical_amount,
            ),
            DamageModifier::FlatDamage {
                amount,
                critical_amount,
            } => Modifier::FlatDamage(
                amount.known().copied().unwrap_or(0),
                *critical_amount,
            ),
            DamageModifier::CriticalRate {
                percent,
                critical_percent,
            } => Modifier::CritRate(
                percent.known().copied().unwrap_or(0),
                *critical_percent,
            ),
            DamageModifier::CriticalDamage {
                amount,
                critical_amount,
            } => Modifier::CritDamage(
                amount.known().copied().unwrap_or(0),
                *critical_amount,
            ),
            DamageModifier::CriticalResistance {
                amount,
                critical_amount,
            } => Modifier::CritResistance(
                amount.known().copied().unwrap_or(0),
                *critical_amount,
            ),
            DamageModifier::PushDamage {
                amount,
                critical_amount,
            } => Modifier::PushDamage(
                amount.known().copied().unwrap_or(0),
                *critical_amount,
            ),
            DamageModifier::DomainPercent { delivery, reach, percent } => {
                Modifier::DomainPercent(*delivery, *reach, *percent)
            }
        };
        let mut res_index = HashMap::new();
        let mut resources = Vec::new();
        for (i, r) in ruleset.resources.iter().enumerate() {
            if i >= MAX_RESOURCES {
                return Err(EngineError::TooMany(format!(
                    "ruleset has more than {MAX_RESOURCES} resources"
                )));
            }
            res_index.insert(r.id.clone(), i);
            resources.push(Resource {
                label: r.id.clone(),
                max: if r.capped_by_targets {
                    r.max.min(cibles.nombre())
                } else {
                    r.max
                },
                default: r.default,
                monotone: r.monotone,
                duration: r
                    .duration
                    .as_ref()
                    .map(|d| (d.turns, d.refresh, d.on_expire)),
                gain_per_turn: r.gain_per_turn.unwrap_or(0),
                gain_per_turn_while: r.gain_per_turn_while.as_ref().map(|g| res_index[g]),
                gain_at_turn_end: r.gain_at_turn_end.unwrap_or(0),
                gain_once_per_turn: r.gain_once_per_turn,
                grants_ap: r.grants_ap.unwrap_or(0),
                // Résolus après la boucle : la chaîne peut viser un compteur
                // déclaré plus bas, et les relances, des sorts pas encore
                // compilés.
                glisse_vers: None,
                puis: None,
                remis_sous: None,
                perd_sous: None,
                vide: None,
                relances_a_l_expiration: 0,
                tirage_parmi: Vec::new(),
                modifiers: r.modifies_damage.iter().map(&compiler).collect(),
                paliers: r.paliers.iter().map(|p| p.iter().map(&compiler).collect()).collect(),
                pa_par_valeur: r.pa_par_valeur.clone(),
                cyclique: r.cyclique,
                poussees: if scenario.poussees_bloquees {
                    r.poussees_par_tour.iter().map(|c| i32::from(*c)).collect()
                } else {
                    Vec::new()
                },
            });
        }

        for (i, r) in ruleset.resources.iter().enumerate() {
            resources[i].glisse_vers = r.shifts_into.as_ref().map(|t| res_index[t]);
            resources[i].puis = r
                .duration
                .as_ref()
                .and_then(|d| d.then_gain.as_ref())
                .map(|t| res_index[t]);
            resources[i].remis_sous = r.reset_at_turn_start_while.as_ref().map(|t| res_index[t]);
            resources[i].perd_sous = r.lose_at_turn_start_while.as_ref().map(|t| res_index[t]);
            resources[i].vide = r.drains.as_ref().map(|t| res_index[t]);
            resources[i].tirage_parmi = r.random_among.iter().map(|t| res_index[t]).collect();
        }
        let glissement_cibles = resources
            .iter()
            .filter_map(|r| r.glisse_vers)
            .fold(0u32, |m, t| m | (1 << t));

        let mut budget_names: Vec<String> = Vec::new();

        let mut tag_names: Vec<String> = Vec::new();
        let mut schedule_specs: Vec<ScheduleSpec> = Vec::new();
        let mut spells = Vec::new();
        let defs = build
            .deck
            .iter()
            .map(|id| {
                ruleset
                    .spells
                    .iter()
                    .find(|s| &s.id == id)
                    .ok_or_else(|| EngineError::UnknownSpell(id.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Le refus se prononce avant la boucle, et nomme le nombre à
        // retirer. Un sort à modes en occupe un par mode : c'est ce total
        // qui se compare au plafond, et le compte à retirer reste en sorts
        // du deck.
        let places = defs.iter().map(|d| 1 + d.modes.len()).sum::<usize>();
        if places > MAX_SPELLS {
            let mut a_retirer = 0;
            let mut reste = places;
            for d in defs.iter().rev() {
                if reste <= MAX_SPELLS {
                    break;
                }
                reste -= 1 + d.modes.len();
                a_retirer += 1;
            }
            return Err(EngineError::TooMany(format!(
                "votre deck compte {} sorts et le calculateur en prend {MAX_SPELLS} au \
                 plus, modes de lancer compris : décochez-en {a_retirer}",
                build.deck.len(),
            )));
        }
        // Quels compteurs disent quels états du jeu sur le lanceur.
        let mut etats_du_lanceur: HashMap<u32, Vec<Porteur>> = HashMap::new();
        for (i, r) in ruleset.resources.iter().enumerate() {
            for e in &r.game_states {
                etats_du_lanceur
                    .entry(*e)
                    .or_default()
                    .push(Porteur::Compteur(i, true));
            }
            for e in &r.game_states_when_zero {
                etats_du_lanceur
                    .entry(*e)
                    .or_default()
                    .push(Porteur::Compteur(i, false));
            }
        }
        // Ceux que porte un groupe de paliers : présents dès que l'un tient.
        for t in &ruleset.tier_states {
            let paliers: Vec<Vec<(Vec<usize>, u8)>> = ruleset
                .spells
                .iter()
                .flat_map(SpellDef::alternatives)
                .flat_map(|s| s.lines)
                .filter_map(|l| l.active_at)
                .filter(|a| a.group.as_deref() == Some(t.group.as_str()))
                .map(|a| {
                    a.conditions()
                        .iter()
                        .map(|c| {
                            (
                                c.resources().iter().map(|r| res_index[*r]).collect(),
                                c.exactly,
                            )
                        })
                        .collect()
                })
                .collect();
            // Une carte tirée au hasard parmi ce que les paliers lisent peut en
            // faire tenir un : l'état passe pour posé tant qu'elle attend, et
            // la table de dégâts en compte la moyenne.
            let lus: Vec<usize> = paliers
                .iter()
                .flat_map(|p| p.iter().flat_map(|(rs, _)| rs.iter().copied()))
                .collect();
            let tirages: Vec<usize> = resources
                .iter()
                .enumerate()
                .filter(|(_, r)| r.tirage_parmi.iter().any(|c| lus.contains(c)))
                .map(|(i, _)| i)
                .collect();
            for e in &t.game_states {
                let porteurs = etats_du_lanceur.entry(*e).or_default();
                porteurs.push(Porteur::Paliers(paliers.clone()));
                porteurs.extend(tirages.iter().map(|&r| Porteur::Compteur(r, true)));
            }
        }
        for def in defs {
            let debut = spells.len();
            for alt in def.alternatives() {
                let mut sort = compile_spell(
                    &alt,
                    &res_index,
                    &mut budget_names,
                    &mut schedule_specs,
                    &mut tag_names,
                    &build,
                    scenario.mode,
                    &cibles,
                    scenario.pm_depenses,
                    scenario.reach,
                    &vie,
                )?;
                sort.critere = alt
                    .cast_criterion
                    .as_ref()
                    .map(|c| CritereCompile::de(c, &etats_du_lanceur));
                // Toute la poussée comptée bloquée, sur la cible visée seule,
                // sans Résistance Poussée : ce que le joueur déclare.
                if scenario.poussees_bloquees {
                    sort.poussees = alt
                        .pushes
                        .iter()
                        .map(|p| {
                            (
                                p.caster.as_ref().map(|c| CritereCompile::de(c, &etats_du_lanceur)),
                                Damage::from_int(i64::from(dofus_damage::degats_de_poussee(
                                    build.profile.level,
                                    build.profile.push_damage,
                                    0,
                                    i32::from(p.cells),
                                    false,
                                ))),
                                i32::from(p.cells),
                            )
                        })
                        .collect();
                }
                // Un état qui interdit de lancer ferme tout sort dont la
                // condition ne l'exige pas : le Pandawa qui porte ne fait que
                // jeter.
                for (r, def) in ruleset.resources.iter().enumerate() {
                    if !def.prevents_spell_cast
                        || alt
                            .cast_criterion
                            .as_ref()
                            .is_some_and(|c| c.exige_un_de(&def.game_states))
                    {
                        continue;
                    }
                    let garde = CritereCompile::Etat {
                        porteurs: vec![Porteur::Compteur(r, true)],
                        present: false,
                    };
                    sort.critere = Some(match sort.critere.take() {
                        Some(c) => CritereCompile::Et(vec![c, garde]),
                        None => garde,
                    });
                }
                spells.push(sort);
            }
            let groupe = (debut..spells.len()).fold(0u32, |m, i| m | (1 << i));
            for s in &mut spells[debut..] {
                s.groupe = groupe;
            }
        }
        // Les relances que remettent un lancer ou la fin d'un état, en masque
        // des sorts compilés : tous les modes d'un sort nommé. Un sort nommé
        // hors du deck ne remet rien.
        let masque_de = |ids: &[String]| {
            spells
                .iter()
                .enumerate()
                .filter(|(_, s)| ids.contains(&s.id))
                .fold(0u32, |m, (i, _)| m | (1 << i))
        };
        let remises: Vec<Vec<u32>> = spells
            .iter()
            .map(|s| {
                s.effects
                    .iter()
                    .map(|e| match e {
                        Eff::RemiseDesRelances { sorts, .. } => masque_de(sorts),
                        _ => 0,
                    })
                    .collect()
            })
            .collect();
        for (i, r) in ruleset.resources.iter().enumerate() {
            if let Some(d) = &r.duration {
                resources[i].relances_a_l_expiration = masque_de(&d.then_reset_cooldowns);
            }
        }
        for (s, masques) in spells.iter_mut().zip(remises) {
            for (e, m) in s.effects.iter_mut().zip(masques) {
                if let Eff::RemiseDesRelances { masque, .. } = e {
                    *masque = m;
                }
            }
        }
        if budget_names.len() > MAX_BUDGETS {
            return Err(EngineError::TooMany(format!(
                "ruleset uses more than {MAX_BUDGETS} budgets"
            )));
        }

        // Calcule AVANT que le scenario ne parte dans la structure.
        let declares: Vec<(usize, u8)> = scenario
            .etats_declares
            .iter()
            .filter_map(|(nom, valeur)| {
                let i = resources.iter().position(|r| &r.label == nom)?;
                Some((i, (*valeur).min(resources[i].max)))
            })
            .collect();
        let budget_limits = budget_names
            .iter()
            .map(|name| {
                scenario
                    .budgets
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, v)| *v)
                    .unwrap_or(u8::MAX)
            })
            .collect();

        let odd: Vec<u32> = build
            .modifiers
            .iter()
            .filter(|m| m.when != When::EvenTurns)
            .map(|m| m.percent)
            .collect();
        let even: Vec<u32> = build
            .modifiers
            .iter()
            .filter(|m| m.when != When::OddTurns)
            .map(|m| m.percent)
            .collect();

        let duration_mask = resources
            .iter()
            .enumerate()
            .filter(|(_, r)| r.duration.is_some())
            .fold(0u32, |m, (i, _)| m | (1 << i));
        let cooldown_mask = spells
            .iter()
            .enumerate()
            .filter(|(_, s)| s.cooldown > 0)
            .fold(0u32, |m, (i, _)| m | (1 << i));
        // Un etat compte comme commandant un lancer des qu'UN sort du deck en
        // fait sa condition. C'est le deck qui decide, pas le fichier : la
        // Lance n'explique rien dans une rotation ou aucun sort ne l'exige.
        let gating_mask = spells
            .iter()
            .flat_map(|s| {
                s.requires
                    .into_iter()
                    .flat_map(Exigence::compteurs)
                    .chain(s.bloque_au_plafond)
                    .chain(s.exige_surplus.map(|(a, _)| a))
            })
            .fold(0u32, |m, r| m | (1 << r));

        // The build's own conditional final-damage modifiers, and NOTHING else.
        // The cast-context factor is deliberately kept out of these two: it
        // differs per spell, and folding it in here as well as per spell would
        // apply it twice, which is exactly the bug this split exists to stop.
        let odd_mult = FinalMultiplier::from_percents(&odd);
        let even_mult = FinalMultiplier::from_percents(&even);

        // Damage a state or a schedule deals later has no spell of its own to
        // take a range from, so it uses the player's usual position. Whether a
        // delayed payload should instead inherit the reach of the cast that
        // started it is an open question; nothing in the data answers it, and
        // assuming it silently would be worse than saying so.
        let lancer_ambiant = CastContext {
            delivery: Delivery::Spell,
            reach: scenario.reach,
        };
        let ambient = FinalMultiplier::for_cast(&build.profile, &scenario.resistance, lancer_ambiant);
        let odd_ambient = odd_mult.and(ambient);
        let even_ambient = even_mult.and(ambient);
        let zero = [0u8; MAX_RESOURCES];
        let args = (&build.profile, scenario.mode, &scenario.resistance);

        // A state's damage figures belong to the spell that applies it, so its
        // critical rate does too. Nothing in the data says whether a state's own
        // damage can crit at all; this assumes it behaves like its source, which
        // is an assumption and not an observation.
        let mut state_triggers: Vec<StateTrigger> = Vec::new();
        for (i, def) in ruleset.resources.iter().enumerate() {
            if def.while_present.is_empty() {
                continue;
            }
            // Le sort qui applique l'état : il donne son taux de critique à
            // l'état, et c'est aussi à lui que les dégâts de l'état reviennent.
            let applicant = spells.iter().find(|s| {
                s.effects
                    .iter()
                    .any(|e| matches!(e, Eff::Gain { resource, .. } if *resource == i))
            });
            let crit_rate = applicant.map_or(CritRate::NEVER, |s| s.crit_rate);
            let source = applicant.map_or_else(|| def.id.clone(), |s| s.id.clone());
            for (effect_index, effect) in def.while_present.iter().enumerate() {
                // Quand la donnee donne a ces degats leur propre taux de base,
                // c'est lui qui compte, et non celui du sort qui pose l'etat.
                let crit_rate = match (effect.crit_base_percent, scenario.mode) {
                    (None, _) => crit_rate,
                    (Some(_), Mode::PrototypeCompat) => CritRate::ALWAYS,
                    (Some(base), Mode::Expected) => {
                        taux_critique(i32::from(base), build.crit_bonus_percent)
                    }
                };
                if state_triggers.len() >= MAX_STATE_TRIGGERS {
                    return Err(EngineError::TooMany(format!(
                        "ruleset has more than {MAX_STATE_TRIGGERS} state triggers"
                    )));
                }
                let (kind, label) = match &effect.trigger {
                    StateTriggerDef::TurnStart => (
                        TriggerKind::TurnStart,
                        format!("{} (début de tour)", def.id),
                    ),
                    StateTriggerDef::ResourceConsumed { resource } => (
                        TriggerKind::Consumed(res_index[resource]),
                        format!("{} (proc)", def.id),
                    ),
                    StateTriggerDef::SpellTagged { tag, .. } => {
                        // Une étiquette qu'aucun sort du deck ne porte donne un
                        // bit qui ne s'allumera jamais : la détente existe et
                        // ne part pas, ce qui est le comportement voulu quand
                        // le joueur n'a pas pris le sort qui la déclenche.
                        let bit = tag_bit(&mut tag_names, tag)?;
                        (TriggerKind::Tagged(bit), format!("{} (proc)", def.id))
                    }
                    StateTriggerDef::PushDamage { .. } => {
                        (TriggerKind::Poussee, format!("{} (poussée)", def.id))
                    }
                };
                let spends = match &effect.trigger {
                    StateTriggerDef::SpellTagged { spends, .. }
                    | StateTriggerDef::PushDamage { spends } => *spends,
                    _ => 0,
                };
                let mut missing = Vec::new();
                let mut paliers = Paliers::default();
                let lines: Vec<Line> = effect
                    .lines
                    .iter()
                    .enumerate()
                    .map(|(j, l)| {
                        if l.critical.known().is_none() {
                            missing.push(format!("{}.while_present.lines[{j}].critical", def.id));
                        }
                        if l.normal.known().is_none() && scenario.mode == Mode::Expected {
                            missing.push(format!("{}.while_present.lines[{j}].normal", def.id));
                        }
                        compile_line(
                            l,
                            &res_index,
                            &mut paliers,
                            l.critical.known().copied(),
                            l.normal.known().copied(),
                            crit_rate,
                            &build.profile,
                            &cibles,
                            scenario.pm_depenses,
                            build.base_mp,
                            &vie,
                        )
                    })
                    .zip(tirages(&effect.lines))
                    .map(|(mut l, t)| {
                        l.tirage = t;
                        l
                    })
                    .collect();
                if !missing.is_empty() {
                    return Err(EngineError::MissingData(missing));
                }
                // Le compteur dont les lignes lisent le palier, s'il y en a
                // un : le montant d'une détente se tabule sur le domaine du
                // compteur au lieu d'être figé à compteurs nuls, où une ligne
                // sous `active_at` ne serait jamais active. Les bonus de base
                // comptent aussi (`while_resource`, `per_resource`, `steps`).
                let mut compteurs: Vec<usize> = lines
                    .iter()
                    .flat_map(|l| {
                        l.active_at
                            .iter()
                            .flat_map(|(rs, _)| rs.iter().copied())
                            .chain(l.while_resource.iter().map(|(r, _)| *r))
                            .chain(l.per_resource.iter().map(|(r, _)| *r))
                            .chain(l.steps.iter().map(|(r, _)| *r))
                            // Deux noms, donc deux compteurs : une detente qui
                            // en porterait un tomberait sur la limite d'un seul
                            // palier, avec un message, plutot que de rendre zero
                            // en silence.
                            .chain(l.per_resource_gated.iter().flat_map(|(r, g, _)| [*r, *g]))
                            // Et la répétition par cran : deux Vertèbres, deux
                            // poisons. Sans elle, la table se lirait à zéro
                            // répétition.
                            .chain(l.repeats_per.iter().copied())
                    })
                    .collect();
                compteurs.sort_unstable();
                compteurs.dedup();
                let palier = match compteurs.len() {
                    0 => None,
                    1 => Some(compteurs[0]),
                    _ => {
                        return Err(EngineError::TooMany(format!(
                            "l'etat `{}` fait dependre ses degats de {} compteurs ; \
                             un seul palier est supporte",
                            def.id,
                            compteurs.len()
                        )))
                    }
                };
                let etendue = palier.map_or(1, |r| usize::from(resources[r].max) + 1);
                let mut damage_odd = Vec::with_capacity(etendue);
                let mut damage_even = Vec::with_capacity(etendue);
                for k in 0..etendue {
                    let mut res = zero;
                    if let Some(r) = palier {
                        res[r] = k as u8;
                    }
                    damage_odd.push(compute_lines(
                        &lines,
                        crit_rate,
                        false,
                        &res,
                        odd_ambient,
                        args.0,
                        args.1,
                        args.2,
                        &resources,
                        lancer_ambiant,
                    ));
                    damage_even.push(compute_lines(
                        &lines,
                        crit_rate,
                        false,
                        &res,
                        even_ambient,
                        args.0,
                        args.1,
                        args.2,
                        &resources,
                        lancer_ambiant,
                    ));
                }
                state_triggers.push(StateTrigger {
                    effect: effect_index,
                    lines,
                    crit_rate,
                    requires: effect
                        .requires
                        .as_ref()
                        .map(|c| compile_condition(c, &res_index)),
                    spends,
                    source: source.clone(),
                    palier,
                    damage_odd,
                    damage_even,
                    label,
                    state: i,
                    kind,
                    once_per_turn: effect.once_per_turn,
                });
            }
        }

        let mut schedules: Vec<Schedule> = schedule_specs
            .into_iter()
            .map(|s| Schedule {
                source: s.source.clone(),
                grants: s.grants,
                requires_at_turn_end: s.requires_at_turn_end,
                damage_odd: compute_lines(
                    &s.lines,
                    s.crit_rate,
                    false,
                    &zero,
                    odd_ambient,
                    args.0,
                    args.1,
                    args.2,
                    &resources,
                    lancer_ambiant,
                ),
                damage_even: compute_lines(
                    &s.lines,
                    s.crit_rate,
                    false,
                    &zero,
                    even_ambient,
                    args.0,
                    args.1,
                    args.2,
                    &resources,
                    lancer_ambiant,
                ),
                label: s.label,
                delay: s.delay,
                acceleration: s.acceleration,
            })
            .collect();

        // Un état qui ne peut JAMAIS monter ne mérite pas une dimension dans
        // les tables de dégâts. Elles sont denses, produit des domaines de
        // leurs dépendances : quatre compteurs à six crans que personne ne pose
        // multiplieraient chaque table par deux mille quatre cents.
        let mut atteignable: Vec<bool> = resources
            .iter()
            .enumerate()
            .map(|(i, r)| {
                r.default > 0
                    || r.gain_per_turn > 0
                    || r.gain_at_turn_end > 0
                    || declares.iter().any(|(d, v)| *d == i && *v > 0)
                    || schedules.iter().any(|s| s.grants.is_some_and(|(g, _)| g == i))
                    || spells.iter().any(|s| {
                        s.effects
                            .iter()
                            .any(|e| matches!(e, Eff::Gain { resource, .. } if *resource == i))
                    })
            })
            .collect();
        // Et ce qu'un compteur atteignable verse dans un autre en fin de tour,
        // ou pose en expirant, l'est aussi, de proche en proche.
        loop {
            let mut change = false;
            for i in 0..resources.len() {
                for t in [resources[i].glisse_vers, resources[i].puis].into_iter().flatten() {
                    if atteignable[i] && !atteignable[t] {
                        atteignable[t] = true;
                        change = true;
                    }
                }
            }
            if !change {
                break;
            }
        }

        for spell in &mut spells {
            let mut deps: Vec<usize> = spell
                .lines
                .iter()
                .flat_map(|l| {
                    l.per_resource
                        .iter()
                        .chain(l.while_resource.iter())
                        .map(|(r, _)| *r)
                        // Les paliers comptent aussi : sans eux, la table de
                        // degats ne varie pas avec la ressource et le bonus ne
                        // se voit jamais, meme applique.
                        .chain(l.steps.iter().map(|(r, _)| *r))
                        // Et la répétition par pose au sol : sans elle, la
                        // table de la Cadence du Roublard resterait de
                        // taille un, lue à « zéro bombe ».
                        .chain(l.repeats_per.iter().copied())
                        // Le plafond de cibles aussi : la Runification sur soi
                        // frappe une fois par rune vivante, et sans lui sa
                        // table se lisait toujours a l'entree « aucune rune ».
                        .chain(l.plafond_cibles.iter().copied())
                        // Le compteur et sa garde, pour la même raison : la
                        // Muselière de l'Ouginak monte par ennemi au contact
                        // « si la cible est la Proie », et sans la Proie ici
                        // sa table se lirait toujours à « pas de Proie ».
                        .chain(l.per_resource_gated.iter().flat_map(|(r, g, _)| [*r, *g]))
                        .chain(l.active_at.iter().flat_map(|(rs, _)| rs.iter().copied()))
                        .collect::<Vec<_>>()
                })
                .collect();
            // A state that changes every spell's damage is a dependency of
            // every spell, whether or not that spell mentions it.
            deps.extend(
                resources
                    .iter()
                    .enumerate()
                    .filter(|(i, r)| r.change_les_lignes() && atteignable[*i])
                    .map(|(i, _)| i),
            );
            // Des cartes tirées au hasard parmi ce que le sort lit : leur
            // compteur en est aussi, et ses entrées se lisent à la moyenne des
            // tirages (`moyenner_les_tirages`).
            let tirages: Vec<usize> = resources
                .iter()
                .enumerate()
                .filter(|(i, r)| atteignable[*i] && r.tirage_parmi.iter().any(|t| deps.contains(t)))
                .map(|(i, _)| i)
                .collect();
            deps.extend(tirages);
            deps.sort_unstable();
            deps.dedup();
            let size = deps
                .iter()
                .map(|&d| radix(&resources, d))
                .product::<usize>()
                .max(1);
            let mut odd_table = Vec::with_capacity(size);
            let mut even_table = Vec::with_capacity(size);
            // `% Dommages aux sorts` (`% Dommages d'armes` pour l'arme) and
            // whichever of `% mêlée` / `% distance` this spell's range selects.
            // Composed ONCE per spell here rather than per node visited: it
            // cannot change during a fight, and the search walks these tables
            // millions of times.
            let lancer = CastContext {
                delivery: if spell.arme { Delivery::Weapon } else { Delivery::Spell },
                reach: reach_for(spell.range, scenario.reach),
            };
            let context = FinalMultiplier::for_cast(&build.profile, &scenario.resistance, lancer);
            let odd_mult = odd_mult.and(context);
            let even_mult = even_mult.and(context);
            for index in 0..size {
                let res = decode(index, &deps, &resources);
                // Une carte tirée en attente : l'entrée se calcule après, sur
                // celles qui n'en ont pas.
                if deps.iter().any(|&d| !resources[d].tirage_parmi.is_empty() && res[d] > 0) {
                    odd_table.push(Damage::ZERO);
                    even_table.push(Damage::ZERO);
                    continue;
                }
                let args = (&build.profile, scenario.mode, &scenario.resistance);
                odd_table.push(compute_damage(
                    spell, &res, odd_mult, args.0, args.1, args.2, &resources, lancer,
                ));
                even_table.push(compute_damage(
                    spell, &res, even_mult, args.0, args.1, args.2, &resources, lancer,
                ));
            }
            moyenner_les_tirages(&deps, &resources, &mut odd_table, &mut even_table);
            spell.damage_deps = deps;
            spell.damage_odd = odd_table;
            spell.damage_even = even_table;
        }

        // Un compteur que rien ne lit ne doit pas démultiplier les états : chaque
        // sort élémentaire du Huppermage compte sa rune pour la Surcharge Runique,
        // inutilement quand elle n'est pas au deck. Leurs écritures sont retirées,
        // sous le même drapeau que les autres réductions : `dominance: false` garde
        // tout, et un test compare.
        if scenario.dominance {
            let lus = compteurs_lus(&spells, &resources, &schedules, &state_triggers);
            for spell in &mut spells {
                spell.effects.retain(|e| match e {
                    // Un gain qui donne des PA ou use un budget compte, lu ou non.
                    Eff::Gain {
                        resource,
                        ap_bonus: None,
                        budget: None,
                        ..
                    }
                    | Eff::Consume { resource, .. }
                    | Eff::Reset { resource, .. }
                    | Eff::Toggle { resource } => lus[*resource],
                    Eff::ConsumeAcross { resources, .. } => resources.iter().any(|r| lus[*r]),
                    _ => true,
                });
            }
            for (i, r) in resources.iter_mut().enumerate() {
                if !lus[i] {
                    r.gain_per_turn = 0;
                    r.gain_at_turn_end = 0;
                }
            }
            for s in &mut schedules {
                if s.grants.is_some_and(|(r, _)| !lus[r]) {
                    s.grants = None;
                }
            }
        }

        // Sous le même drapeau que l'élagage de fin de tour : `dominance: false`
        // donne la recherche complète, contre laquelle la réduction se vérifie. Un
        // sort « inerte » ne lit ni n'écrit aucune ressource, et ses dégâts n'en
        // dépendent d'aucune.
        for spell in spells.iter_mut() {
            spell.inert = spell.effects.iter().all(|e| matches!(e, Eff::Damage))
                && spell.requires.is_none()
                && spell.critere.is_none()
                && spell.poussees.is_empty()
                && spell.schedule.is_none()
                && spell.cost_reduction.is_none()
                // Un sort qui RENCHERIT ne peut pas être jugé sur `ap_base` :
                // l'élagage ne compare que des coûts fixes, et le laisser
                // passer ici le ferait paraître moins cher qu'il n'est dès son
                // deuxième lancer.
                && spell.cost_increase.is_none()
                && spell.extra_casts.is_none()
                && spell.damage_deps.is_empty()
                && spell.bloque_au_plafond.is_none()
                && spell.exige_surplus.is_none()
                // Un mode partage ses lancers avec les autres : le comparer seul
                // à un sort voisin ignorerait ce que ses frères lui prennent.
                && spell.groupe.count_ones() == 1;
        }

        let pruned = if scenario.dominance && scenario.prune_spells {
            // Ce que les etats peuvent ajouter au budget d'un tour, chacun a
            // son plafond. Sans ce terme le plafond d'elagage serait
            // sous-estime et un sort qu'il fallait garder tomberait.
            let pa_des_etats: i32 = resources
                .iter()
                .map(|r| {
                    i32::from(r.grants_ap) * i32::from(r.max)
                        + r.pa_par_valeur.iter().copied().max().map_or(0, |p| i32::from(p.max(0)))
                })
                .sum();
            prune_dominated(&mut spells, build.base_ap, pa_des_etats)
        } else {
            Vec::new()
        };

        let ticking = state_triggers
            .iter()
            .filter(|t| t.kind == TriggerKind::TurnStart)
            .fold(0u32, |m, t| m | (1 << t.state));

        Ok(Engine {
            pruned,
            resources,
            spells,
            schedules,
            state_triggers,
            budget_limits,
            build,
            scenario,
            duration_mask,
            ticking,
            cooldown_mask,
            gating_mask,
            glissement_cibles,
            declares,
            stats: Counters::default(),
            outcomes_cache: std::sync::Mutex::new(Map::default()),
        })
    }

    fn initial_state(&self) -> State {
        let mut s = State::default();
        for (i, r) in self.resources.iter().enumerate() {
            s.res[i] = r.default;
        }
        // Les compteurs que le joueur renseigne : ils partent de sa valeur et
        // n'en bougent plus, rien ne les gagne ni ne les consomme.
        for (nom, valeur) in &self.scenario.etats_declares {
            if let Some(i) = self.resources.iter().position(|r| &r.label == nom) {
                s.res[i] = (*valeur).min(self.resources[i].max);
            }
        }
        // Un sort à délai initial commence le combat déjà en attente. Le
        // compteur de relance sert des deux côtés : le poser ici suffit,
        // la décrémentation de fin de tour fait le reste.
        for (i, spell) in self.spells.iter().enumerate() {
            s.cooldown[i] = spell.initial_cooldown;
        }
        s
    }

    fn turn_is_odd(&self, turn: u8) -> bool {
        if turn % 2 == 0 {
            self.scenario.starting_turn_is_odd
        } else {
            !self.scenario.starting_turn_is_odd
        }
    }

    /// Le nombre de lancers permis dans le tour, ressource comprise.
    ///
    /// Lu AVANT les effets du lancer, comme la réduction de coût : le
    /// Régulateur consomme le Téléfrag qui lui accorde son second lancer, et
    /// le lire après le lui retirerait au moment même où il le gagne.
    fn cap_of(&self, spell: &Spell, res: &[u8; MAX_RESOURCES]) -> u8 {
        match spell.extra_casts {
            Some((r, per_stack, cap)) => spell
                .cast_cap
                .saturating_add(res[r].saturating_mul(per_stack).min(cap)),
            None => spell.cast_cap,
        }
    }

    /// Les Dommages Poussée que les états portés ajoutent à ceux du build, à la
    /// valeur critique au seuil habituel, lue au taux du sort qui pousse.
    fn dommages_poussee_en_plus(&self, res: &[u8; MAX_RESOURCES], taux: CritRate) -> i32 {
        let mut bonus = 0;
        for (i, resource) in self.resources.iter().enumerate() {
            if res[i] == 0 {
                continue;
            }
            for (modifier, fois) in resource.bonus(res[i]) {
                if let Modifier::PushDamage(amount, critique) = modifier {
                    bonus += magnitude(*amount, *critique, taux) * fois;
                }
            }
        }
        bonus
    }

    fn cost_of(&self, spell: &Spell, res: &[u8; MAX_RESOURCES]) -> u8 {
        // Read BEFORE this cast's own effects: a spell that grants its own
        // reduction stack does not discount the cast that grants it. Le
        // surcoût se lit au même moment et pour la même raison : la Paume
        // Explosive pose elle-même le compteur qui la renchérit, et le lancer
        // qui le pose se paie encore au prix plein.
        let cost = match spell.cost_reduction {
            Some((r, per_stack, floor)) => spell
                .ap_base
                .saturating_sub(res[r].saturating_mul(per_stack))
                .max(floor),
            None => spell.ap_base,
        };
        match spell.cost_increase {
            Some((r, per_stack, ceiling)) => {
                cost.saturating_add(res[r].saturating_mul(per_stack).min(ceiling))
            }
            None => cost,
        }
    }

    #[inline]
    fn line_damage(&self, spell: &Spell, res: &[u8; MAX_RESOURCES], odd: bool) -> Damage {
        let index = encode(&spell.damage_deps, &self.resources, res);
        if odd {
            spell.damage_odd[index]
        } else {
            spell.damage_even[index]
        }
    }

    // -- within a turn ------------------------------------------------------

    /// Everything that lands before a single spell is cast: a scheduled payload
    /// coming due, a state ticking. Returns the damage and the state as the
    /// turn actually starts, with resolved schedules cleared.
    fn turn_start(&self, state: &State, odd: bool) -> (Damage, State) {
        let mut opened = *state;
        let mut damage = Damage::ZERO;
        for (i, schedule) in self.schedules.iter().enumerate() {
            if opened.pending[i] == 1 {
                damage += if odd {
                    schedule.damage_odd
                } else {
                    schedule.damage_even
                };
                if let Some((r, amount)) = schedule.grants {
                    opened.res[r] = (opened.res[r] + amount).min(self.resources[r].max);
                    // Un gain differe vit a partir du tour ou il arrive : sans
                    // ce minuteur, la Puissance de l'Age d'Or, qui arrive au
                    // tour suivant, ne serait jamais repartie.
                    if let Some((turns, _, _)) = self.resources[r].duration {
                        opened.timer[r] = turns + 1;
                    }
                }
                opened.pending[i] = 0;
            }
        }
        // Les reglages du joueur reviennent a leur valeur a chaque tour. Ils
        // decrivent ce qu'il fait DANS un tour, pas une reserve qui s'epuise :
        // un sort qui les consomme les retrouve au tour suivant.
        for (i, valeur) in &self.declares {
            opened.res[*i] = *valeur;
        }
        // Stacks that climb on their own, before anything is cast.
        for (i, resource) in self.resources.iter().enumerate() {
            // Un compteur peut n'avoir le droit de monter que tant qu'un
            // autre etat tient : c'est ce qui distingue une MATURATION, qui
            // part d'un lancer, d'une horloge qui tourne depuis le debut du
            // combat.
            let autorise = match resource.gain_per_turn_while {
                Some(garde) => opened.res[garde] > 0,
                None => true,
            };
            if resource.gain_per_turn > 0 && autorise {
                opened.res[i] = if resource.cyclique && opened.res[i] >= resource.max {
                    1
                } else {
                    (opened.res[i] + resource.gain_per_turn).min(resource.max)
                };
            }
        }
        for trigger in &self.state_triggers {
            if trigger.kind == TriggerKind::TurnStart
                && opened.res[trigger.state] > 0
                && condition_met(&trigger.requires, &opened.res)
            {
                damage += trigger.damage(&opened.res, odd);
            }
        }
        for (_, d) in self.poussees_de_tour(&opened.res) {
            damage += d;
        }
        // Les etats dont la duree s'est achevee viennent de tiquer une
        // derniere fois : ils partent avant le premier lancer du tour.
        let mut partants = opened.expiring;
        while partants != 0 {
            let i = partants.trailing_zeros() as usize;
            partants &= partants - 1;
            let r = &self.resources[i];
            if let Some((_, _, on_expire)) = r.duration {
                opened.res[i] = match on_expire {
                    OnExpire::ResetToDefault => r.default,
                    OnExpire::DecrementOne => opened.res[i].saturating_sub(1),
                };
            }
        }
        opened.expiring = 0;
        // Les compteurs qui meurent au début du tour tant qu'un état tient :
        // après leurs effets de début de tour, qu'ils ont donc joués.
        for (i, r) in self.resources.iter().enumerate() {
            if r.remis_sous.is_some_and(|garde| opened.res[garde] > 0) {
                opened.res[i] = r.default;
            }
            if let Some(garde) = r.perd_sous {
                opened.res[i] = opened.res[i].saturating_sub(opened.res[garde]);
            }
        }
        (damage, opened)
    }

    /// Les poussées que des compteurs infligent d'eux-mêmes au début du tour,
    /// une par entrée : `(compteur, dégâts)`. Les Bottes du Cul Botté. Lues sur
    /// l'état du début de tour, ses Dommages Poussée compris.
    fn poussees_de_tour<'a>(&'a self, res: &'a [u8; MAX_RESOURCES]) -> impl Iterator<Item = (usize, Damage)> + 'a {
        self.resources
            .iter()
            .enumerate()
            .filter(move |(i, r)| !r.poussees.is_empty() && res[*i] > 0)
            .flat_map(move |(i, r)| {
                // Les Dommages Poussée à valeur critique se lisent au critique
                // du build : aucun sort ne pousse ici.
                let taux = taux_critique(0, self.build.crit_bonus_percent);
                let bonus = self.dommages_poussee_en_plus(res, taux);
                r.poussees.iter().map(move |cases| {
                    (
                        i,
                        Damage::from_int(i64::from(dofus_damage::degats_de_poussee(
                            self.build.profile.level,
                            self.build.profile.push_damage + bonus,
                            0,
                            *cases,
                            false,
                        ))),
                    )
                })
            })
    }

    /// The same thing, described. Kept apart from `turn_start` because that one
    /// runs once per reachable state and must not allocate.
    fn turn_start_notes(&self, state: &State, odd: bool) -> Vec<String> {
        let _ = odd;
        let mut notes = Vec::new();
        for (i, schedule) in self.schedules.iter().enumerate() {
            if state.pending[i] == 1 {
                notes.push(format!("{} arrive à échéance", schedule.label));
            }
        }
        for trigger in &self.state_triggers {
            if trigger.kind == TriggerKind::TurnStart
                && state.res[trigger.state] > 0
                && condition_met(&trigger.requires, &state.res)
            {
                notes.push(trigger.label.clone());
            }
        }
        for (i, _) in self.poussees_de_tour(&state.res) {
            let note = format!("{} (poussée)", self.resources[i].label);
            if notes.last() != Some(&note) {
                notes.push(note);
            }
        }
        notes
    }

    /// À quel sort revient chaque part des dégâts d'ouverture. Même découpage
    /// que `turn_start`, mais nommé.
    fn turn_start_sources(&self, state: &State, odd: bool) -> Vec<(String, Damage)> {
        let mut out = Vec::new();
        for (i, schedule) in self.schedules.iter().enumerate() {
            if state.pending[i] == 1 {
                out.push((
                    schedule.source.clone(),
                    if odd {
                        schedule.damage_odd
                    } else {
                        schedule.damage_even
                    },
                ));
            }
        }
        // Les états qui montent d'eux-mêmes le font avant, donc l'état lu ici
        // est celui d'après ce gain : même ordre que `turn_start`.
        let mut opened = *state;
        for (i, resource) in self.resources.iter().enumerate() {
            // Un compteur peut n'avoir le droit de monter que tant qu'un
            // autre etat tient : c'est ce qui distingue une MATURATION, qui
            // part d'un lancer, d'une horloge qui tourne depuis le debut du
            // combat.
            let autorise = match resource.gain_per_turn_while {
                Some(garde) => opened.res[garde] > 0,
                None => true,
            };
            if resource.gain_per_turn > 0 && autorise {
                opened.res[i] = if resource.cyclique && opened.res[i] >= resource.max {
                    1
                } else {
                    (opened.res[i] + resource.gain_per_turn).min(resource.max)
                };
            }
        }
        for trigger in &self.state_triggers {
            if trigger.kind == TriggerKind::TurnStart
                && opened.res[trigger.state] > 0
                && condition_met(&trigger.requires, &opened.res)
            {
                out.push((trigger.source.clone(), trigger.damage(&opened.res, odd)));
            }
        }
        // Une ligne par compteur, ses poussées additionnées : les deux des
        // Bottes du Cul Botté se liraient sinon comme un doublon.
        for (i, d) in self.poussees_de_tour(&opened.res) {
            match out.last_mut() {
                Some((source, total)) if *source == self.resources[i].label => *total += d,
                _ => out.push((self.resources[i].label.clone(), d)),
            }
        }
        out
    }

    /// Enumerate every reachable end-of-turn outcome.
    ///
    /// The value search records no cast sequences at all. Building them costs a
    /// vector of owned strings at every node visited, which on this shape of
    /// problem dominates the arithmetic it is meant to be measuring. The chosen
    /// sequences are rebuilt afterwards, once per turn on the winning path.
    fn start_node(&self, state: &State) -> Node {
        Node {
            // Les PA du tour : ceux du build, le report de la veille, et ce que
            // les etats portes donnent. Le troisieme terme est nul partout sauf
            // sur le Pacte Bestial de l'Osamodas.
            ap: i16::from(self.build.base_ap)
                + i16::from(state.carry)
                + self
                    .resources
                    .iter()
                    .enumerate()
                    .map(|(i, r)| r.pa(state.res[i]))
                    .sum::<i16>(),
            res: state.res,
            used: [0; MAX_SPELLS],
            ap_bonus_claimed: 0,
            budget_used: [0; MAX_BUDGETS],
            carry: 0,
            applied: 0,
            fired: 0,
            pending: state.pending,
            arming: 0,
            state_proc: 0,
            gain_proc: 0,
            remises: 0,
        }
    }

    /// Les fins de tour possibles depuis un état, mémorisées : `solve` et
    /// `steady_state` parcourent le même espace d'états.
    ///
    /// La clé oublie les minuteurs : le tour ne les lit jamais (seuls `turn_start`
    /// et `advance` s'en servent), donc deux états qui ne diffèrent que par eux ont
    /// les mêmes fins de tour. Les fins dominées sont écartées : triées par dégâts
    /// d'abord, une fin ne peut être dominée que par une fin déjà gardée.
    fn turn_outcomes(&self, state: &State, odd: bool) -> std::sync::Arc<Vec<(Damage, Outcome, u8)>> {
        let cle = (
            State {
                timer: [0; MAX_RESOURCES],
                ..*state
            },
            odd,
        );
        // Le verrou n'est tenu que le temps de lire ou d'écrire un pointeur.
        if let Some(deja) = self.outcomes_cache.lock().expect("cache lock").get(&cle) {
            return std::sync::Arc::clone(deja);
        }
        // Calculé sur la clé et non sur l'état reçu : quel que soit celui des
        // variantes qui remplit le cache en premier, le résultat est le même.
        let calcule = std::sync::Arc::new(self.compute_turn_outcomes(&cle.0, odd));
        self.outcomes_cache
            .lock()
            .expect("cache lock")
            .insert(cle, std::sync::Arc::clone(&calcule));
        calcule
    }

    /// Les issues du tour, chacune avec ses dégâts et le nombre de lancers qui
    /// y mènent.
    fn compute_turn_outcomes(&self, state: &State, odd: bool) -> Vec<(Damage, Outcome, u8)> {
        // Bloc-notes local : partagé, il empêcherait deux états d'être traités
        // en parallèle.
        let mut scratch = Search {
            best: Map::default(),
            plans: None,
            seen: Map::default(),
            script: None,
            fin: None,
            atteint: 0,
        };
        let mut found: Vec<(Damage, Outcome, u8)> = {
            let start = self.start_node(state);
            let mut seq = Vec::new();
            self.walk(start, Damage::ZERO, &mut seq, state, odd, &mut scratch);

            Counters::add(&self.stats.turn_searches, 1);
            Counters::add(&self.stats.outcomes, scratch.best.len() as u64);

            scratch.best.iter().map(|(o, (d, l))| (*d, *o, *l)).collect()
        };
        if !self.scenario.dominance {
            return found;
        }
        found.sort_unstable_by_key(|(damage, _, _)| std::cmp::Reverse(*damage));

        let before = found.len();
        let mut kept: Vec<(Damage, Outcome, u8)> = Vec::with_capacity(found.len());
        for (damage, outcome, lancers) in found {
            if !kept.iter().any(|(_, k, _)| self.state_dominates(k, &outcome)) {
                kept.push((damage, outcome, lancers));
            }
        }
        Counters::add(&self.stats.dominated, (before - kept.len()) as u64);
        kept
    }

    /// Rebuild one turn's cast sequence. Runs once per turn on the winning
    /// path, so it allocates freely rather than sharing the scratch tables.
    fn turn_plan(&self, state: &State, odd: bool, wanted: &Outcome) -> (Damage, Vec<Cast>, i16) {
        let mut search = Search {
            best: Map::default(),
            plans: Some(Map::default()),
            seen: Map::default(),
            script: None,
            fin: None,
            atteint: 0,
        };
        let mut seq = Vec::new();
        self.walk(
            self.start_node(state),
            Damage::ZERO,
            &mut seq,
            state,
            odd,
            &mut search,
        );
        let damage = search.best[wanted].0;
        let (casts, ap_left) = search.plans.expect("recording was requested")[wanted].clone();
        (damage, casts, ap_left)
    }

    fn walk(
        &self,
        node: Node,
        damage: Damage,
        seq: &mut Vec<Cast>,
        incoming: &State,
        odd: bool,
        search: &mut Search,
    ) {
        // Collapse cast orderings that arrive at the same node. Without this the
        // enumeration is factorial in the number of casts per turn.
        Counters::add(&self.stats.nodes, 1);

        match search.seen.get(&node) {
            Some(&previous) if previous >= damage => {
                Counters::add(&self.stats.revisits, 1);
                return;
            }
            _ => {
                search.seen.insert(node, damage);
            }
        }

        let outcome = Outcome {
            res: node.res,
            carry: node.carry,
            applied: node.applied & self.duration_mask,
            fired: node.fired & self.cooldown_mask,
            pending: node.pending,
            arming: node.arming,
            remises: node.remises & self.cooldown_mask,
        };
        if let Some(script) = &search.script {
            search.atteint = search.atteint.max(seq.len());
            if seq.len() == script.len() {
                search.fin = Some((outcome, damage, seq.clone(), node.ap));
                return;
            }
        }
        let lancers = node.used.iter().fold(0u8, |n, &u| n.saturating_add(u));
        let improves = match search.best.get(&outcome) {
            Some(&(d, l)) => damage > d || (damage == d && lancers < l),
            None => true,
        };
        if improves {
            search.best.insert(outcome, (damage, lancers));
            if let Some(plans) = &mut search.plans {
                plans.insert(outcome, (seq.clone(), node.ap));
            }
        }

        let recording = search.plans.is_some();

        for (i, spell) in self.spells.iter().enumerate() {
            // Un tour rejoué ne lance que le lancer imposé, élagué ou non.
            match &search.script {
                Some(script) => {
                    if script.get(seq.len()) != Some(&i) {
                        continue;
                    }
                }
                None => {
                    if spell.dominated_by.is_some() {
                        continue;
                    }
                }
            }
            if node.used[i] >= self.cap_of(spell, &node.res) {
                continue;
            }
            // Les modes d'un même sort puisent dans les mêmes lancers.
            if spell.groupe.count_ones() > 1 {
                let mut freres = spell.groupe;
                let mut lances = 0u8;
                while freres != 0 {
                    lances = lances.saturating_add(node.used[freres.trailing_zeros() as usize]);
                    freres &= freres - 1;
                }
                let quota = match spell.extra_casts {
                    Some((r, per_stack, cap)) => spell
                        .quota
                        .saturating_add(node.res[r].saturating_mul(per_stack).min(cap)),
                    None => spell.quota,
                };
                if lances >= quota {
                    continue;
                }
            }
            // Deux blocages distincts sur le même compteur : le délai initial
            // interdit le sort tant qu'il court, l'intervalle interdit en plus
            // de le relancer dans le tour où il vient de partir. L'un comme
            // l'autre valent pour tous les modes du sort.
            // Une relance remise à zéro dans le tour ne bloque plus.
            if incoming.cooldown[i] > 0 && node.remises & (1 << i) == 0 {
                continue;
            }
            if spell.cooldown > 0 && node.fired & spell.groupe != 0 {
                continue;
            }
            // One pending payload at a time: recasting while one is in flight
            // would either overwrite it or need a second state dimension.
            if let Some(s) = spell.schedule {
                if node.pending[s] > 0 {
                    continue;
                }
            }
            if !self.conditions_tiennent(spell, &node.res) {
                continue;
            }
            let cost = self.cost_of(spell, &node.res);
            if i16::from(cost) > node.ap {
                continue;
            }

            let mut next = node;
            next.ap -= i16::from(cost);
            next.used[i] += 1;
            next.fired |= 1 << i;
            let mut cast_damage = Damage::ZERO;
            // Les dommages de poussée, quand le joueur déclare que ses poussées
            // butent : lus sur l'état d'AVANT le lancer, comme le masque du jeu.
            let mut pousse = false;
            if !spell.poussees.is_empty() {
                let bonus = self.dommages_poussee_en_plus(&node.res, spell.crit_rate);
                for (critere, degats, cases) in &spell.poussees {
                    if critere.as_ref().is_none_or(|c| c.tient(&node.res)) {
                        cast_damage += if bonus == 0 {
                            *degats
                        } else {
                            Damage::from_int(i64::from(dofus_damage::degats_de_poussee(
                                self.build.profile.level,
                                self.build.profile.push_damage + bonus,
                                0,
                                *cases,
                                false,
                            )))
                        };
                        pousse = true;
                    }
                }
            }
            let mut notes = Vec::new();
            // Dire ce que le lancer EXIGEAIT rend lisible l'ordre choisi : sans
            // ça, un sort placé en deuxième position a l'air d'y être par hasard.
            if recording {
                if let Some(exigence) = spell.requires {
                    notes.push(format!("exige {}", self.resources[exigence.res].label));
                }
            }
            // Ce que ce lancer déclenche chez d'AUTRES sorts. Compté dans
            // `cast_damage` parce que c'est bien ce tour-là qu'il tombe, mais
            // listé à part pour que la répartition par sort le rende à qui de
            // droit.
            let mut procs: Vec<(String, Damage)> = Vec::new();

            for eff in &spell.effects {
                match eff {
                    Eff::Damage => {
                        cast_damage += self.line_damage(spell, &next.res, odd);
                    }
                    Eff::Gain {
                        resource,
                        amount,
                        ap_bonus,
                        budget,
                        at_cap,
                        requires,
                        jusqu_a,
                        si_critique,
                    } => {
                        // Un gain de coup critique ne part que d'un lancer qui
                        // critique : au seuil de la règle, comme les
                        // valeurs critiques des effets.
                        if *si_critique
                            && spell.crit_rate.permille() < dofus_ruleset::CRITICAL_EFFECT_THRESHOLD * 10
                        {
                            continue;
                        }
                        // La condition porte sur le GAIN, pas sur le lancer :
                        // Poussière frappe toujours mais ne téléporte que ce qui
                        // est déjà Téléfrag, donc n'en génère que s'il y en a un.
                        if !condition_met(requires, &next.res) {
                            if recording {
                                notes.push("sans génération".to_string());
                            }
                            continue;
                        }
                        let at_max = next.res[*resource] >= self.resources[*resource].max;
                        let budget_left = match budget {
                            Some(b) => next.budget_used[*b] < self.budget_limits[*b],
                            None => true,
                        };
                        // Un compteur plafonne a un gain par tour : le bit
                        // porte sur la RESSOURCE, pas sur le sort, quatre
                        // sorts pouvant se partager la meme permission.
                        let deja_monte = self.resources[*resource].gain_once_per_turn
                            && next.gain_proc & (1 << resource) != 0;
                        if (at_max && *at_cap == AtCap::Skip) || !budget_left || deja_monte {
                            if recording && (budget.is_some() || deja_monte) {
                                notes.push("sans génération".to_string());
                            }
                            continue;
                        }
                        let avant = next.res[*resource];
                        let monte = if *jusqu_a {
                            next.res[*resource].max(*amount)
                        } else {
                            next.res[*resource].saturating_add(*amount)
                        };
                        next.res[*resource] = monte.min(self.resources[*resource].max);
                        next.applied |= 1 << resource;
                        // Ce qui OUVRE les sorts suivants mérite d'être dit.
                        if recording
                            && self.gating_mask & (1 << resource) != 0
                            && next.res[*resource] > avant
                        {
                            notes.push(format!("pose {}", self.resources[*resource].label));
                        }
                        if self.resources[*resource].gain_once_per_turn {
                            next.gain_proc |= 1 << resource;
                        }
                        if let Some(b) = budget {
                            next.budget_used[*b] += 1;
                        }
                        if let Some((bonus, once_per_spell)) = ap_bonus {
                            let claimed = next.ap_bonus_claimed & (1 << i) != 0;
                            if !once_per_spell || !claimed {
                                next.ap += i16::from(*bonus);
                                next.ap_bonus_claimed |= 1 << i;
                                if recording {
                                    notes.push(format!("+{bonus} PA"));
                                }
                            }
                        }
                    }
                    Eff::Consume {
                        resource,
                        amount,
                        requires,
                    } => {
                        if !condition_met(requires, &next.res) {
                            continue;
                        }
                        let avant = next.res[*resource];
                        next.res[*resource] = next.res[*resource].saturating_sub(*amount);
                        // Ce qui FERME les sorts suivants aussi.
                        if recording
                            && self.gating_mask & (1 << resource) != 0
                            && next.res[*resource] < avant
                        {
                            notes.push(format!("retire {}", self.resources[*resource].label));
                        }

                        // A consumption can bring a pending payload forward.
                        for (si, schedule) in self.schedules.iter().enumerate() {
                            if next.pending[si] == 0 {
                                continue;
                            }
                            for (source, reduce, immediate) in &schedule.acceleration {
                                if source != resource {
                                    continue;
                                }
                                next.pending[si] = next.pending[si].saturating_sub(*reduce).max(1);
                                if next.pending[si] == 1 && *immediate {
                                    cast_damage += if odd {
                                        schedule.damage_odd
                                    } else {
                                        schedule.damage_even
                                    };
                                    next.pending[si] = 0;
                                    if recording {
                                        notes.push("charge déclenchée".to_string());
                                    }
                                }
                            }
                        }

                        // A state can answer the loss.
                        for (ti, trigger) in self.state_triggers.iter().enumerate() {
                            if trigger.kind != TriggerKind::Consumed(*resource)
                                || next.res[trigger.state] == 0
                                || !condition_met(&trigger.requires, &next.res)
                            {
                                continue;
                            }
                            if trigger.once_per_turn && next.state_proc & (1 << ti) != 0 {
                                continue;
                            }
                            let du_proc = trigger.damage(&next.res, odd);
                            cast_damage += du_proc;
                            next.state_proc |= 1 << ti;
                            if recording {
                                notes.push(trigger.label.clone());
                                procs.push((trigger.source.clone(), du_proc));
                            }
                        }
                    }
                    Eff::StealAp { amount, requires } => {
                        // Le vol de PA n'aboutit pas : voir AP_THEFT_LANDS. On
                        // garde la trace du lancer, sans le PA.
                        if AP_THEFT_LANDS && condition_met(requires, &next.res) {
                            next.ap += i16::from(*amount);
                            if recording {
                                notes.push(format!("vole {amount} PA"));
                            }
                        }
                    }
                    Eff::CarryAp {
                        amount,
                        cap,
                        requires,
                    } => {
                        if condition_met(requires, &next.res) {
                            next.carry = (next.carry + amount).min(*cap);
                            if recording {
                                notes.push(format!("reporte {amount} PA"));
                            }
                        }
                    }
                    Eff::Reset { resource, requires } => {
                        if condition_met(requires, &next.res) {
                            next.res[*resource] = self.resources[*resource].default;
                        }
                    }
                    // Un seul cran : le chargement du fichier le vérifie. Rendu à
                    // un, l'état repart pour toute sa durée, comme un gain.
                    Eff::Toggle { resource } => {
                        next.res[*resource] = u8::from(next.res[*resource] == 0);
                        if next.res[*resource] == 1 {
                            next.applied |= 1 << resource;
                        }
                    }
                    // Plus rien ne part après lui : ses PA restants sont perdus.
                    Eff::FinDuTour => {
                        next.ap = 0;
                        if recording {
                            notes.push("termine le tour".to_string());
                        }
                    }
                    // Les sorts remis repartent dans le tour ; leurs lancers
                    // d'avant la remise ne comptent plus pour la relance, ceux
                    // d'après la reposent. Les lancers par tour, eux, restent
                    // comptés.
                    Eff::RemiseDesRelances { masque, requires, .. } => {
                        if condition_met(requires, &next.res) && *masque != 0 {
                            next.remises |= masque;
                            next.fired &= !masque;
                            if recording {
                                notes.push("relances remises".to_string());
                            }
                        }
                    }
                    // Aucun état ni aucune charge n'écoute ces compteurs, le
                    // chargement du fichier le vérifie : un simple retrait.
                    Eff::ConsumeAcross {
                        resources, amount, ..
                    } => {
                        let mut reste = *amount;
                        for &r in resources {
                            let pris = next.res[r].min(reste);
                            next.res[r] -= pris;
                            reste -= pris;
                        }
                    }
                    Eff::Schedule { index, requires } => {
                        // La condition se lit sur l'état AU MOMENT DU LANCER.
                        // Non remplie, rien n'est armé : le sort se réduit à sa
                        // ligne immédiate, ce qui est exactement ce qui se passe
                        // en jeu quand la cible ne porte pas l'état attendu.
                        // Une charge dont la condition se lit en fin de tour
                        // s'arme toujours ICI : c'est `advance` qui tranchera,
                        // sur l'etat final, si elle tient.
                        if self.schedules[*index].requires_at_turn_end.is_some() {
                            next.arming |= 1 << index;
                            if recording {
                                notes.push("glyphe posée en fin de tour".to_string());
                            }
                            continue;
                        }
                        let armable = requires.is_none_or(|e| e.tient(&next.res));
                        if armable {
                            next.pending[*index] = self.schedules[*index].delay + 1;
                            if recording {
                                let d = self.schedules[*index].delay;
                                notes.push(if d == 1 {
                                    "retardé d'un tour".to_string()
                                } else {
                                    format!("retardé de {d} tours")
                                });
                            }
                        } else if recording {
                            notes.push("sans effet différé".to_string());
                        }
                    }
                }
            }

            // Les marques qui répondent au sort lui-même, une fois tous ses
            // effets résolus. L'Enutrof pose Éboulement, puis chaque sort qui
            // tente un retrait de Portée fait répondre la marque et lui coûte
            // une charge ; à zéro la marque disparaît. Après les effets, et non
            // avant : un sort qui poserait la marque et porterait l'étiquette ne
            // doit pas se déclencher sur lui-même au tour de sa pose.
            if spell.tags != 0 {
                for (ti, trigger) in self.state_triggers.iter().enumerate() {
                    let TriggerKind::Tagged(bit) = trigger.kind else {
                        continue;
                    };
                    if spell.tags & bit == 0
                        || next.res[trigger.state] == 0
                        || !condition_met(&trigger.requires, &next.res)
                    {
                        continue;
                    }
                    if trigger.once_per_turn && next.state_proc & (1 << ti) != 0 {
                        continue;
                    }
                    let du_proc = trigger.damage(&next.res, odd);
                    cast_damage += du_proc;
                    next.state_proc |= 1 << ti;
                    next.res[trigger.state] =
                        next.res[trigger.state].saturating_sub(trigger.spends);
                    if recording {
                        notes.push(trigger.label.clone());
                        procs.push((trigger.source.clone(), du_proc));
                    }
                }
            }

            // Les états que les dommages de poussée de CE lancer consomment :
            // la Flibuste du Steamer, la Noa du Forgelance. Après les effets,
            // comme les marques, pour la même raison d'ordre.
            if pousse {
                for (ti, trigger) in self.state_triggers.iter().enumerate() {
                    if trigger.kind != TriggerKind::Poussee
                        || next.res[trigger.state] == 0
                        || !condition_met(&trigger.requires, &next.res)
                    {
                        continue;
                    }
                    if trigger.once_per_turn && next.state_proc & (1 << ti) != 0 {
                        continue;
                    }
                    let du_proc = trigger.damage(&next.res, odd);
                    cast_damage += du_proc;
                    next.state_proc |= 1 << ti;
                    next.res[trigger.state] =
                        next.res[trigger.state].saturating_sub(trigger.spends);
                    if recording {
                        notes.push(trigger.label.clone());
                        procs.push((trigger.source.clone(), du_proc));
                    }
                }
            }

            if recording {
                seq.push(Cast {
                    id: spell.id.clone(),
                    procs,
                    spell: spell.name.clone(),
                    ap_cost: cost,
                    ap_left: next.ap,
                    damage: cast_damage,
                    notes,
                });
            }
            self.walk(next, damage + cast_damage, seq, incoming, odd, search);
            if recording {
                seq.pop();
            }
        }
    }

    // -- across turns -------------------------------------------------------

    fn advance(&self, previous: &State, outcome: &Outcome) -> State {
        let mut s = State {
            res: outcome.res,
            timer: previous.timer,
            cooldown: previous.cooldown,
            carry: outcome.carry,
            pending: outcome.pending,
            expiring: 0,
        };
        // Les etats que l'expiration d'un autre pose, poses apres la boucle :
        // la cible peut etre plus bas dans la liste, et la boucle lui
        // retirerait aussitot le tour qu'on vient de lui donner. Un cran par
        // etat qui expire : deux poupees qui partent au meme tour en posent
        // deux.
        let mut suites = [0u8; MAX_RESOURCES];
        // Les sorts dont la fin d'un etat remet la relance, remis une fois les
        // relances du tour posees.
        let mut relances_remises: u32 = 0;
        for (i, r) in self.resources.iter().enumerate() {
            let Some((turns, _refresh, on_expire)) = r.duration else {
                continue;
            };
            let expire = if outcome.applied & (1 << i) != 0 {
                // Une durée de zéro veut dire « le tour de la pose, et lui
                // seul » : l'état sert aux lancers qui suivent dans le même
                // tour, puis disparaît avant le suivant. Les vulnérabilités en
                // sont (Fer Rouge, Décimation, Coupe-gorge, Volcan : « jusqu'au
                // début du prochain tour du lanceur ») : pour un solveur solo,
                // ce tour-ci et pas le suivant.
                //
                // `turns: 1` ne dit pas cela : les durées se décomptent en fin
                // de tour, et un état posé au tour N avec `turns: 1` vit encore
                // tout le tour N+1 (voir `DurationDef::turns`).
                if turns == 0 {
                    true
                } else {
                    s.timer[i] = turns;
                    false
                }
            } else if s.timer[i] > 0 {
                s.timer[i] -= 1;
                s.timer[i] == 0
            } else {
                false
            };
            if !expire {
                continue;
            }
            if let Some(t) = r.puis {
                if s.res[i] > 0 {
                    suites[t] = suites[t].saturating_add(1);
                }
            }
            // Un état encore là qui s'achève : ses sorts reviennent. Retiré
            // avant son terme par un lancer, il ne remet plus rien.
            if s.res[i] > 0 {
                relances_remises |= r.relances_a_l_expiration;
            }
            // ⚠️ UN POISON POSE POUR d TOURS FRAPPE d FOIS. Le jeu decompte les
            // durees au debut du tour du lanceur, APRES le tour de la cible :
            // le poison pose au tour N tique encore au tour de la cible qui
            // suit le tour N+d-1, puis part. Ce moteur compte ce tick au debut
            // du tour N+d ; l'etat reste donc la pour lui, et `turn_start` le
            // retire aussitot apres, avant tout lancer.
            if self.ticking & (1 << i) != 0 && s.res[i] > 0 {
                s.expiring |= 1 << i;
            } else {
                s.res[i] = match on_expire {
                    OnExpire::ResetToDefault => r.default,
                    OnExpire::DecrementOne => s.res[i].saturating_sub(1),
                };
            }
        }
        for (t, n) in suites.into_iter().enumerate().filter(|(_, n)| *n > 0) {
            let r = &self.resources[t];
            s.res[t] = s.res[t].saturating_add(n).min(r.max);
            // Pose pour le tour qui s'ouvre : il vit ce tour-la et ceux que
            // sa propre duree ajoute.
            if let Some((turns, _, _)) = r.duration {
                s.timer[t] = turns + 1;
            }
        }
        // Les compteurs qui glissent : ce que le tour a posé passe dans le
        // compteur du tour d'avant, et ainsi de suite. Tout se lit sur l'état
        // de FIN de tour, pour que la chaîne se décale d'un seul cran. Un
        // compteur qui glisse sans que rien ne glisse en lui repart de zéro.
        //
        // Le bout de la chaîne perd sa valeur : ce sont des cumuls arrivés à
        // leur terme, qui quittent aussi le total qui les porte.
        for (i, r) in self.resources.iter().enumerate() {
            if let Some(t) = r.vide {
                s.res[t] = s.res[t].saturating_sub(outcome.res[i]);
            }
        }
        for (i, r) in self.resources.iter().enumerate() {
            if let Some(t) = r.glisse_vers {
                s.res[t] = outcome.res[i];
            }
        }
        for (i, r) in self.resources.iter().enumerate() {
            if r.glisse_vers.is_some() && self.glissement_cibles & (1 << i) == 0 {
                s.res[i] = r.default;
            }
        }
        for (i, spell) in self.spells.iter().enumerate() {
            if spell.cooldown > 0 || spell.initial_cooldown > 0 {
                // Un mode lancé met en relance tous les modes du sort.
                s.cooldown[i] = if outcome.fired & spell.groupe != 0 {
                    // `cooldown` est l'intervalle entre deux lancers
                    // (`minCastInterval`) : lancé au tour 3 avec un intervalle de
                    // 3, le sort revient au tour 6. Ce compteur est déjà
                    // décrémenté à la fin de chaque tour suivant, d'où le `- 1`.
                    spell.cooldown.saturating_sub(1)
                } else if outcome.remises & (1 << i) != 0 {
                    // Remise dans le tour, et pas relancé depuis.
                    0
                } else {
                    s.cooldown[i].saturating_sub(1)
                };
            }
            // La fin d'un état remet la relance au tour qui s'ouvre, même
            // d'un sort lancé dans ce tour-ci.
            if relances_remises & (1 << i) != 0 {
                s.cooldown[i] = 0;
            }
        }
        for i in 0..self.schedules.len() {
            if s.pending[i] > 1 {
                s.pending[i] -= 1;
            }
        }
        // Les charges qui attendaient la fin du tour. La condition se lit sur
        // l'etat FINAL, apres tout ce que le tour a consomme et reapplique :
        // un Gousset qui mange son propre Telefrag ne pose rien, sauf si un
        // lancer ulterieur en a remis un. Armee ici a 1, donc resolue des le
        // debut du tour suivant, sans le decrement ci-dessus.
        for (i, schedule) in self.schedules.iter().enumerate() {
            if outcome.arming & (1 << i) == 0 {
                continue;
            }
            let Some(exigence) = schedule.requires_at_turn_end else {
                continue;
            };
            if exigence.tient(&outcome.res) {
                s.pending[i] = 1;
            }
        }
        // Acquis en fin de tour, donc absent au tour un.
        for (i, resource) in self.resources.iter().enumerate() {
            if resource.gain_at_turn_end > 0 {
                s.res[i] = (s.res[i] + resource.gain_at_turn_end).min(resource.max);
            }
        }
        s
    }

    /// Whether `a` is at least as good as `b` for every turn that follows.
    ///
    /// Conservative on purpose. Equality is demanded on the dimensions whose
    /// ordering is not obvious: which durations were refreshed, which cooldowns
    /// were started, and how far a pending payload has left to run, where a
    /// countdown of zero means nothing is pending at all and does not sit at
    /// either end of the scale. Getting this wrong does not crash anything, it
    /// silently returns a suboptimal rotation, so it only claims what it can
    /// justify.
    fn state_dominates(&self, a: &Outcome, b: &Outcome) -> bool {
        // Refreshing a duration is never worse, so `a` may refresh a superset.
        if b.applied & !a.applied != 0 {
            return false;
        }
        // Starting a cooldown is never better, so `a` may have started a subset.
        if a.fired & !b.fired != 0 {
            return false;
        }
        // Remettre une relance n'est jamais pire : `a` en remet au moins autant.
        if b.remises & !a.remises != 0 {
            return false;
        }
        // A pending payload is worth guaranteed future damage, so having one
        // beats having none. Between two, the one landing sooner also frees its
        // spell sooner, so a lower countdown wins. Zero means nothing pending
        // and sits below every live countdown rather than at one end of them.
        for i in 0..self.schedules.len() {
            let ok = match (a.pending[i], b.pending[i]) {
                (_, 0) => true,
                (0, _) => false,
                (x, y) => x <= y,
            };
            if !ok {
                return false;
            }
        }
        if a.carry < b.carry {
            return false;
        }
        for (i, resource) in self.resources.iter().enumerate() {
            let ok = match resource.monotone {
                Monotone::Increasing => a.res[i] >= b.res[i],
                Monotone::Decreasing => a.res[i] <= b.res[i],
                Monotone::None => a.res[i] == b.res[i],
            };
            if !ok {
                return false;
            }
        }
        true
    }

    pub fn stats(&self) -> Stats {
        self.stats.snapshot()
    }

    /// Spells the search skipped, and what made each one pointless.
    pub fn pruned(&self) -> &[(String, String)] {
        &self.pruned
    }

    /// Résout, ou rend `None` si [`ARRET`] a été demandé en cours de route.
    ///
    /// C'est ce que l'application appelle : elle a un bouton pour interrompre.
    /// Le CLI et les tests passent par [`Self::solve`], qui ne s'interrompt
    /// pas et ne peut donc pas rendre `None`.
    pub fn solve_annulable(&self) -> Option<Solution> {
        self.resoudre(true)
    }

    pub fn solve(&self) -> Solution {
        // `false` : aucun arrêt n'est lu sur ce chemin, donc `None` est
        // impossible et ce déballage ne peut pas tomber.
        self.resoudre(false)
            .expect("une résolution non annulable rend toujours un résultat")
    }

    fn resoudre(&self, annulable: bool) -> Option<Solution> {
        // Toutes les recherches intra-tour d'abord, en largeur : la descente
        // récursive qui suit indexe par (tour, état), et le même état atteint à
        // deux tours y serait deux nœuds qui reparcourent chacun leurs fins de tour.
        ARRET.oublier();
        self.chauffer(annulable, Some(self.scenario.horizon), true, None);
        // La largeur s'est-elle arrêtée en chemin ? Alors la profondeur qui
        // suit referait tout le travail qu'elle n'a pas fait, et le bouton
        // « Annuler » n'annulerait rien.
        if annulable && ARRET.demande() {
            AVANCEMENT.termine();
            return None;
        }
        let mut memo: Map<(u8, State), (Damage, u16, Option<Outcome>)> = Map::default();
        let (total, _) = self.search(0, self.initial_state(), &mut memo);
        if annulable && ARRET.demande() {
            AVANCEMENT.termine();
            return None;
        }
        let turns = self.reconstruct(&memo);
        AVANCEMENT.termine();
        Some(Solution {
            total,
            turns,
            inter_turn_states: memo.len(),
        })
    }

    /// Les meilleurs dégâts d'ici à la fin, et le nombre de lancers qu'ils
    /// coûtent. À dégâts égaux, la rotation la plus courte l'emporte : un
    /// lancer gratuit qui ne rapporte rien, une Bonne Pioche sur une Main que
    /// la Redistribution videra, ne s'y ajoute pas. Un lancer utile, lui, ne
    /// recule pas pour autant : il coûte le même nombre de lancers à tout tour.
    fn search(
        &self,
        turn: u8,
        state: State,
        memo: &mut Map<(u8, State), (Damage, u16, Option<Outcome>)>,
    ) -> (Damage, u16) {
        if turn == self.scenario.horizon {
            return (Damage::ZERO, 0);
        }
        if let Some(&(damage, lancers, _)) = memo.get(&(turn, state)) {
            return (damage, lancers);
        }

        let odd = self.turn_is_odd(turn);
        let (opening, opened) = self.turn_start(&state, odd);
        let mut best: (Damage, u16, Option<Outcome>) = (Damage(i64::MIN), u16::MAX, None);
        for &(damage, outcome, lancers) in self.turn_outcomes(&opened, odd).iter() {
            let (future, ensuite) = self.search(turn + 1, self.advance(&opened, &outcome), memo);
            let total = opening + damage + future;
            let lancers = u16::from(lancers).saturating_add(ensuite);
            if total > best.0 || (total == best.0 && lancers < best.1) {
                best = (total, lancers, Some(outcome));
            }
        }

        memo.insert((turn, state), best);
        (best.0, best.1)
    }

    /// Walk the winning chain of outcomes and rebuild the cast sequences, once
    /// per turn rather than once per node.
    fn reconstruct(&self, memo: &Map<(u8, State), (Damage, u16, Option<Outcome>)>) -> Vec<TurnPlan> {
        let mut state = self.initial_state();
        let mut out = Vec::new();
        for turn in 0..self.scenario.horizon {
            let Some((_, _, Some(outcome))) = memo.get(&(turn, state)) else {
                break;
            };
            let odd = self.turn_is_odd(turn);
            let (opening_damage, opened) = self.turn_start(&state, odd);
            let opening = self.turn_start_notes(&state, odd);
            let opening_sources = self.turn_start_sources(&state, odd);
            let (damage, casts, ap_left) = self.turn_plan(&opened, odd, outcome);
            out.push(TurnPlan {
                turn: turn + 1,
                odd,
                opening_damage,
                opening_sources,
                opening,
                damage,
                casts,
                ap_left,
            });
            state = self.advance(&opened, outcome);
        }
        out
    }

    /// Rejoue une rotation donnée, tour par tour : les lancers de chaque tour
    /// dans l'ordre, nommés comme la rotation les nomme (le mode entre
    /// parenthèses) ou par l'identifiant du sort, qui désigne alors son propre
    /// mode.
    ///
    /// Le chemin des conseils par tour : ce qu'un sort écarté vaudrait à un
    /// instant se lit en rejouant la rotation avec lui. Les règles sont celles
    /// de la recherche, lancer par lancer ; un lancer qui ne peut pas partir à
    /// son tour (PA, relance, lancers par tour, condition) arrête tout et se
    /// nomme.
    pub fn replay(&self, tours: &[Vec<String>]) -> Result<Solution, String> {
        let mut state = self.initial_state();
        let mut turns = Vec::new();
        let mut total = Damage::ZERO;
        for (t, lancers) in tours.iter().enumerate() {
            let turn = u8::try_from(t).map_err(|_| "trop de tours à rejouer".to_string())?;
            let script = lancers
                .iter()
                .enumerate()
                .map(|(k, nom)| {
                    self.spells
                        .iter()
                        .position(|s| &s.name == nom)
                        .or_else(|| self.spells.iter().position(|s| &s.id == nom))
                        .ok_or_else(|| format!("tour {}, lancer {} : « {nom} » n'est pas dans le deck", t + 1, k + 1))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let odd = self.turn_is_odd(turn);
            let (opening_damage, opened) = self.turn_start(&state, odd);
            let opening = self.turn_start_notes(&state, odd);
            let opening_sources = self.turn_start_sources(&state, odd);
            let mut search = Search {
                best: Map::default(),
                plans: Some(Map::default()),
                seen: Map::default(),
                script: Some(script),
                fin: None,
                atteint: 0,
            };
            let mut seq = Vec::new();
            self.walk(self.start_node(&opened), Damage::ZERO, &mut seq, &opened, odd, &mut search);
            let Some((outcome, damage, casts, ap_left)) = search.fin else {
                let k = search.atteint;
                return Err(format!(
                    "tour {}, lancer {} : « {} » ne peut pas partir à cet instant (PA, relance, lancers \
                     par tour ou condition)",
                    t + 1,
                    k + 1,
                    lancers[k]
                ));
            };
            total += opening_damage + damage;
            turns.push(TurnPlan {
                turn: turn + 1,
                odd,
                opening_damage,
                opening_sources,
                opening,
                damage,
                casts,
                ap_left,
            });
            state = self.advance(&opened, &outcome);
        }
        Ok(Solution {
            total,
            turns,
            inter_turn_states: 0,
        })
    }

    /// Ce que chaque sort du deck vaudrait à la place d'un lancer d'une
    /// rotation donnée : la rotation rejouée jusqu'à lui, puis ce sort à sa
    /// place. Rendus du plus fort au plus faible, ceux qui ne peuvent pas
    /// partir à cet instant écartés.
    ///
    /// La valeur est celle du LANCER, à cet instant : un sort qui prépare les
    /// suivants (une Puissance) y vaut ce qu'il frappe lui-même, et la suite
    /// de la rotation n'est pas rejouée.
    pub fn remplacants(&self, tours: &[Vec<String>], tour: usize, lancer: usize) -> Result<Vec<Cast>, String> {
        let courant = tours.get(tour).ok_or_else(|| format!("la rotation n'a pas de tour {}", tour + 1))?;
        if lancer >= courant.len() {
            return Err(format!("le tour {} n'a pas de lancer {}", tour + 1, lancer + 1));
        }
        let mut prefixe: Vec<Vec<String>> = tours[..tour].to_vec();
        prefixe.push(courant[..lancer].to_vec());
        let mut vus = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for s in &self.spells {
            if !vus.insert(s.name.clone()) {
                continue;
            }
            let mut script = prefixe.clone();
            if let Some(dernier) = script.last_mut() {
                dernier.push(s.name.clone());
            }
            if let Ok(rejeu) = self.replay(&script) {
                if let Some(c) = rejeu.turns.last().and_then(|t| t.casts.last()) {
                    out.push(c.clone());
                }
            }
        }
        out.sort_by(|a, b| b.damage.cmp(&a.damage).then_with(|| a.spell.cmp(&b.spell)));
        Ok(out)
    }
}

struct Search {
    /// Pour chaque issue, ses meilleurs dégâts et le nombre de lancers qui y
    /// mènent : à dégâts égaux, le plus court l'emporte.
    best: Map<Outcome, (Damage, u8)>,
    plans: Option<Map<Outcome, (Vec<Cast>, i16)>>,
    seen: Map<Node, Damage>,
    /// Les lancers imposés, dans l'ordre, pour rejouer un tour donné (voir
    /// [`Engine::replay`]). `None` pour une recherche.
    script: Option<Vec<usize>>,
    /// Le tour rejoué jusqu'au bout : son issue, ses dégâts, ses lancers et
    /// les PA qui restent.
    fin: Option<(Outcome, Damage, Vec<Cast>, i16)>,
    /// Combien de lancers imposés ont pu partir, pour dire lequel bloque.
    atteint: usize,
}

/// Ce qu'un lancer ou un gain exige : un compteur, un seuil, et s'il faut
/// l'atteindre EXACTEMENT.
///
/// L'exactitude sert aux compteurs qui codent autre chose qu'une quantité :
/// l'élément de la rune du Huppermage vaut 1 pour la Terre et 2 pour le Feu,
/// et « au moins 1 » y voudrait dire « Terre ou mieux », ce qui n'a pas de
/// sens.
///
/// Une conjonction en porte jusqu'à trois (`Condition::All`) : `res` est le
/// compteur de la première, celui que la rotation nomme quand un lancer
/// l'exige, et les autres suivent dans `et`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Exigence {
    res: usize,
    seuil: u8,
    cmp: Comparaison,
    et: [Option<(usize, u8, Comparaison)>; 2],
    /// `none_of` : ces compteurs, en masque, valent tous zéro.
    aucun: u32,
}

/// Comment un compteur se compare à son seuil.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Comparaison {
    AuMoins,
    Exactement,
    AuPlus,
}

impl Comparaison {
    fn tient(self, valeur: u8, seuil: u8) -> bool {
        match self {
            Comparaison::AuMoins => valeur >= seuil,
            Comparaison::Exactement => valeur == seuil,
            Comparaison::AuPlus => valeur <= seuil,
        }
    }
}

impl Exigence {
    fn tient(self, res: &[u8; MAX_RESOURCES]) -> bool {
        self.cmp.tient(res[self.res], self.seuil)
            && self.et.iter().flatten().all(|&(r, seuil, cmp)| cmp.tient(res[r], seuil))
            && (0..MAX_RESOURCES).all(|r| self.aucun & (1 << r) == 0 || res[r] == 0)
    }

    /// Les compteurs que la condition lit, la première clause en tête.
    fn compteurs(self) -> impl Iterator<Item = usize> {
        std::iter::once(self.res)
            .chain(self.et.into_iter().flatten().map(|(r, _, _)| r))
            .chain((0..MAX_RESOURCES).filter(move |r| self.aucun & (1 << r) != 0))
    }
}

fn condition_met(requires: &Option<Exigence>, res: &[u8; MAX_RESOURCES]) -> bool {
    requires.is_none_or(|e| e.tient(res))
}

/// La condition de lancement de la donnée, compilée sur les compteurs qui
/// représentent chaque état du jeu (voir `ResourceDef::game_states`).
///
/// ⚠️ UN ÉTAT QU'AUCUN COMPTEUR NE REPRÉSENTE, LE LANCEUR NE L'A JAMAIS :
/// `porteurs` est vide, `HS=X` est faux et `HS!X` vrai. Un sort qui exige un
/// état que la rotation ne sait pas produire n'est donc jamais proposé, ce qui
/// est la seule erreur acceptable : trop peu plutôt qu'injouable.
#[derive(Clone, Debug)]
enum CritereCompile {
    /// Le lanceur porte l'état si l'un de ces porteurs le dit.
    Etat {
        porteurs: Vec<Porteur>,
        present: bool,
    },
    Et(Vec<CritereCompile>),
    Ou(Vec<CritereCompile>),
}

/// Ce qui dit qu'un état du jeu est sur le lanceur.
#[derive(Clone, Debug)]
enum Porteur {
    /// `(r, true)` quand le compteur `r` vaut au moins un, `(r, false)` quand
    /// il vaut zéro.
    Compteur(usize, bool),
    /// Quand l'un de ces paliers tient : chacun est une liste de sommes de
    /// compteurs, chacune à sa valeur exacte, comme une ligne `active_at`.
    Paliers(Vec<Vec<(Vec<usize>, u8)>>),
}

impl Porteur {
    fn porte(&self, res: &[u8; MAX_RESOURCES]) -> bool {
        match self {
            Porteur::Compteur(r, si_present) => {
                if *si_present {
                    res[*r] > 0
                } else {
                    res[*r] == 0
                }
            }
            Porteur::Paliers(paliers) => paliers.iter().any(|palier| {
                palier.iter().all(|(compteurs, attendu)| {
                    compteurs.iter().map(|r| u32::from(res[*r])).sum::<u32>()
                        == u32::from(*attendu)
                })
            }),
        }
    }

    fn lus(&self, v: &mut Vec<usize>) {
        match self {
            Porteur::Compteur(r, _) => v.push(*r),
            Porteur::Paliers(paliers) => v.extend(
                paliers
                    .iter()
                    .flat_map(|p| p.iter().flat_map(|(rs, _)| rs.iter().copied())),
            ),
        }
    }
}

impl CritereCompile {
    fn de(c: &dofus_ruleset::Critere, etats: &HashMap<u32, Vec<Porteur>>) -> Self {
        match c {
            dofus_ruleset::Critere::Etat { etat, present } => CritereCompile::Etat {
                porteurs: etats.get(etat).cloned().unwrap_or_default(),
                present: *present,
            },
            dofus_ruleset::Critere::Et(t) => CritereCompile::Et(t.iter().map(|c| Self::de(c, etats)).collect()),
            dofus_ruleset::Critere::Ou(t) => CritereCompile::Ou(t.iter().map(|c| Self::de(c, etats)).collect()),
        }
    }

    fn tient(&self, res: &[u8; MAX_RESOURCES]) -> bool {
        match self {
            CritereCompile::Etat { porteurs, present } => {
                porteurs.iter().any(|p| p.porte(res)) == *present
            }
            CritereCompile::Et(t) => t.iter().all(|c| c.tient(res)),
            CritereCompile::Ou(t) => t.iter().any(|c| c.tient(res)),
        }
    }

    /// Les compteurs que la condition lit.
    fn lus(&self, v: &mut Vec<usize>) {
        match self {
            CritereCompile::Etat { porteurs, .. } => porteurs.iter().for_each(|p| p.lus(v)),
            CritereCompile::Et(t) | CritereCompile::Ou(t) => t.iter().for_each(|c| c.lus(v)),
        }
    }
}

#[allow(clippy::too_many_arguments)]
/// Les étiquettes rencontrées, dans l'ordre de première apparition : l'indice
/// devient le bit du masque. Seize au plus, ce qui laisse de la marge sur les
/// trois qui existent.
const MAX_TAGS: usize = 16;

fn tag_bit(tags: &mut Vec<String>, nom: &str) -> Result<u16, EngineError> {
    if let Some(i) = tags.iter().position(|t| t == nom) {
        return Ok(1 << i);
    }
    if tags.len() >= MAX_TAGS {
        return Err(EngineError::TooMany(format!(
            "ruleset has more than {MAX_TAGS} spell tags"
        )));
    }
    tags.push(nom.to_string());
    Ok(1 << (tags.len() - 1))
}

#[allow(clippy::too_many_arguments)]
fn compile_spell(
    def: &SpellDef,
    res_index: &HashMap<String, usize>,
    budgets: &mut Vec<String>,
    schedules: &mut Vec<ScheduleSpec>,
    tags: &mut Vec<String>,
    build: &Build,
    mode: Mode,
    cibles: &Cibles,
    pm_depenses: u8,
    reach: Reach,
    vie: &Vie,
) -> Result<Spell, EngineError> {
    let mut missing = Vec::new();

    let crit_rate = match (mode, &def.crit.base_rate) {
        // A spell that cannot critical at all never does, whatever the build
        // carries. This is not the same as a base rate of zero, which still
        // takes the build's critical bonus.
        (_, _) if !def.crit.can_crit => CritRate::NEVER,
        (Mode::PrototypeCompat, _) => CritRate::ALWAYS,
        (Mode::Expected, Maybe::Known(base)) => {
            taux_critique(i32::from(*base), build.crit_bonus_percent)
        }
        (Mode::Expected, Maybe::Unknown(_)) => {
            missing.push(format!("{}.crit.base_rate", def.id));
            CritRate::NEVER
        }
    };

    let mut lines = Vec::new();
    let mut paliers = Paliers::default();
    let tirages_du_sort = tirages(&def.lines);
    for (i, line) in def.lines.iter().enumerate() {
        let critical = line.critical.known().copied();
        let normal = line.normal.known().copied();
        // Une ligne en pourcentage de vie n'a pas de fourchette : sa valeur se
        // lit sur la vie du lanceur, et `compile_line` la chiffre. Un sort qui
        // ne peut pas faire de coup critique n'a pas de valeur critique à
        // fournir : son taux est `NEVER`, et des sorts comme Runification
        // n'ont aucune ligne critique.
        if critical.is_none() && def.crit.can_crit && !line.sans_fourchette() {
            missing.push(format!("{}.lines[{i}].critical", def.id));
        }
        if normal.is_none() && mode == Mode::Expected && !line.sans_fourchette() {
            missing.push(format!("{}.lines[{i}].normal", def.id));
        }
        lines.push(compile_line(
            line,
            res_index,
            &mut paliers,
            critical,
            normal,
            crit_rate,
            &build.profile,
            cibles,
            pm_depenses,
            build.base_mp,
            vie,
        ));
        if let Some(l) = lines.last_mut() {
            l.tirage = tirages_du_sort[i];
        }
    }
    // Un rebond frappe un ennemi que les AUTRES lignes n'ont pas atteint : son
    // compte ne se lit qu'une fois toutes les lignes connues.
    let principales: Vec<Option<&AreaDef>> = def
        .lines
        .iter()
        .filter(|l| l.rebound_within.is_none())
        .map(|l| l.area.as_ref())
        .collect();
    for (line, source) in lines.iter_mut().zip(&def.lines) {
        if let Some(rayon) = source.rebound_within {
            line.par_cible = rebond(rayon, &principales, cibles);
        }
    }
    // Combien d'ennemis la zone du sort atteint, pour les compteurs qui
    // montent jusqu'à elle.
    let zone = lines
        .iter()
        .zip(&def.lines)
        .filter(|(_, source)| source.rebound_within.is_none())
        .map(|(line, _)| line.par_cible.len())
        .max()
        .unwrap_or(1);

    for effect in &def.effects {
        if let Effect::Schedule {
            id,
            delay,
            accelerated_by,
            payload,
            ..
        } = effect
        {
            if delay.is_unknown() {
                missing.push(format!("{}.schedule[{id}].delay", def.id));
            }
            for a in accelerated_by {
                if a.reduce_by.is_unknown() {
                    missing.push(format!(
                        "{}.schedule[{id}].accelerated_by.reduce_by",
                        def.id
                    ));
                }
            }
            for (j, line) in payload.iter().enumerate() {
                if line.critical.known().is_none() {
                    missing.push(format!("{}.schedule[{id}].payload[{j}].critical", def.id));
                }
                if line.normal.known().is_none() && mode == Mode::Expected {
                    missing.push(format!("{}.schedule[{id}].payload[{j}].normal", def.id));
                }
            }
        }
    }

    if !missing.is_empty() {
        return Err(EngineError::MissingData(missing));
    }

    let cost_increase = def
        .ap_cost
        .increased_by
        .as_ref()
        .map(|ci| (res_index[&ci.resource], ci.per_stack, ci.ceiling));

    let cost_reduction = match &def.ap_cost.reduced_by {
        Some(cr) => {
            let per_stack = *cr.per_stack.known().ok_or_else(|| {
                EngineError::MissingData(vec![format!("{}.ap_cost.reduced_by.per_stack", def.id)])
            })?;
            Some((res_index[&cr.resource], per_stack, cr.floor))
        }
        None => None,
    };

    let mut schedule = None;
    let effects = def
        .effects
        .iter()
        .map(|e| {
            if let Effect::Schedule {
                delay,
                accelerated_by,
                payload,
                requires,
                requires_at,
                ..
            } = e
            {
                let index = schedules.len();
                schedule = Some(index);
                schedules.push(ScheduleSpec {
                    source: def.id.clone(),
                    // Le libellé sert à l'affichage : le nom du sort parle au
                    // joueur, l'identifiant du schedule ne parle qu'au fichier.
                    label: def.name.fr.clone(),
                    delay: delay.known().copied().unwrap_or(0),
                    acceleration: accelerated_by
                        .iter()
                        .map(|a| {
                            (
                                res_index[&a.on_consumed],
                                a.reduce_by.known().copied().unwrap_or(0),
                                a.resolve_immediately_at_zero,
                            )
                        })
                        .collect(),
                    lines: {
                        let mut paliers = Paliers::default();
                        payload
                            .iter()
                            .map(|l| {
                                compile_line(
                                    l,
                                    res_index,
                                    &mut paliers,
                                    l.critical.known().copied(),
                                    l.normal.known().copied(),
                                    crit_rate,
                                    &build.profile,
                                    cibles,
                                    pm_depenses,
                                    build.base_mp,
                                    vie,
                                )
                            })
                            .zip(tirages(payload))
                            .map(|(mut l, t)| {
                                l.tirage = t;
                                l
                            })
                            .collect()
                    },
                    crit_rate,
                    grants: None,
                    requires_at_turn_end: match requires_at {
                        RequiresAt::TurnEnd => {
                            requires.as_ref().map(|c| compile_condition(c, res_index))
                        }
                        RequiresAt::Cast => None,
                    },
                });
                Eff::Schedule {
                    index,
                    // Lue en fin de tour, la condition ne bloque pas le lancer :
                    // c'est l'etat final qui decidera si la charge s'arme.
                    requires: match requires_at {
                        RequiresAt::TurnEnd => None,
                        RequiresAt::Cast => {
                            requires.as_ref().map(|c| compile_condition(c, res_index))
                        }
                    },
                }
            } else if let Effect::ScheduleGain {
                resource,
                delay,
                amount,
                requires,
                ..
            } = e
            {
                // Même machinerie que `Schedule`, avec une charge sans dégâts :
                // le compte à rebours vit dans `pending`, se résout au début du
                // tour, et rend la ressource au lieu de frapper.
                //
                // Le budget de Téléfrag n'est pas consommé : il chiffre ce que
                // le joueur met en place par tour en se déplaçant, et la Fuite
                // du Temps arme un tour à l'avance, une route de plus. Deux
                // charges arrivant au même début de tour ne donnent qu'un
                // Téléfrag, la ressource étant plafonnée à un.
                let index = schedules.len();
                schedule = Some(index);
                schedules.push(ScheduleSpec {
                    source: def.id.clone(),
                    label: def.name.fr.clone(),
                    delay: *delay,
                    acceleration: Vec::new(),
                    lines: Vec::new(),
                    crit_rate,
                    grants: Some((res_index[resource], *amount)),
                    requires_at_turn_end: None,
                });
                Eff::Schedule {
                    index,
                    requires: requires.as_ref().map(|c| compile_condition(c, res_index)),
                }
            } else {
                let mut eff = compile_effect(e, res_index, budgets);
                match &mut eff {
                    Eff::Gain {
                        amount,
                        jusqu_a: true,
                        ..
                    }
                    | Eff::ConsumeAcross {
                        amount,
                        zone: true,
                        ..
                    } => *amount = u8::try_from(zone).unwrap_or(u8::MAX),
                    _ => {}
                }
                eff
            }
        })
        .collect();

    let mut masque = 0u16;
    for t in &def.tags {
        masque |= tag_bit(tags, t)?;
    }

    Ok(Spell {
        id: def.id.clone(),
        name: def.name.fr.clone(),
        tags: masque,
        ap_base: def.ap_cost.base,
        arme: def.arme,
        can_crit: def.crit.can_crit,
        cost_reduction,
        cost_increase,
        // Deux plafonds distincts, le second par cible : la Gelure du Xélor
        // part quatre fois dans le tour mais deux fois au plus sur la même
        // cible, donc deux lancers sur un ennemi, quatre sur deux. Un sort
        // lancé sur soi n'a qu'une cible ; un sort dont la case visée ne porte
        // pas ce qu'il exige ne part pas.
        cast_cap: if visee_refusee(def.aim, cibles) {
            0
        } else {
            def.casts_per_target.map_or(def.casts_per_turn, |t| {
                let cibles_du_sort = if def.on_self { 1 } else { cibles.nombre() };
                def.casts_per_turn.min(t.saturating_mul(cibles_du_sort))
            })
        },
        // Posés par `Engine::new`, qui seul voit les modes d'un même sort.
        groupe: 0,
        quota: def.casts_per_turn,
        extra_casts: def
            .extra_casts
            .as_ref()
            .map(|e| (res_index[&e.resource], e.per_stack, e.cap)),
        cooldown: def.cooldown_turns,
        initial_cooldown: def.initial_cooldown,
        schedule,
        crit_rate,
        range: def.range,
        dominated_by: None,
        inert: false,
        // Une exigence marquee `requires_only_at_range` tombe au corps a
        // corps : le sort y touche dans ses DEUX modes, et l'exiger quand
        // meme interdisait des rotations parfaitement jouables.
        requires: def
            .requires
            .as_ref()
            .filter(|_| !def.requires_only_at_range || reach != Reach::Melee)
            .map(|c| compile_condition(c, res_index)),
        // Rempli par l'appelant, qui connaît les états que chaque compteur
        // représente.
        critere: None,
        poussees: Vec::new(),
        blocked_by: def.blocked_by.as_ref().map(|r| res_index[r]),
        bloque_au_plafond: def.blocked_at_max.as_ref().map(|r| res_index[r]),
        exige_surplus: def
            .requires_more_than
            .as_ref()
            .map(|s| (res_index[&s.resource], res_index[&s.than])),
        effects,
        lines,
        damage_deps: Vec::new(),
        damage_odd: Vec::new(),
        damage_even: Vec::new(),
    })
}

impl Resource {
    /// Ce compteur change-t-il les lignes de dégâts ? Des Dommages Poussée ne
    /// touchent que la formule de poussée, que le lancer recalcule : un compteur
    /// qui ne porte qu'eux n'entre pas dans les tables, qu'il multiplierait par son
    /// nombre de valeurs.
    fn change_les_lignes(&self) -> bool {
        self.modifiers
            .iter()
            .chain(self.paliers.iter().flatten())
            .any(|m| !matches!(m, Modifier::PushDamage(..)))
    }

    /// Les bonus du compteur à la valeur `v`, chacun avec le nombre de fois
    /// qu'il compte : ses modificateurs par charge, puis le palier de cette
    /// valeur, une fois (`ResourceDef::paliers`).
    fn bonus(&self, v: u8) -> impl Iterator<Item = (&Modifier, i32)> {
        let palier = usize::from(v).checked_sub(1).and_then(|k| self.paliers.get(k));
        self.modifiers
            .iter()
            .map(move |m| (m, i32::from(v)))
            .chain(palier.into_iter().flatten().map(|m| (m, 1)))
    }

    /// Les PA que donne la valeur `v` : par charge, plus ceux de son palier.
    fn pa(&self, v: u8) -> i16 {
        let palier = usize::from(v).checked_sub(1).and_then(|k| self.pa_par_valeur.get(k));
        i16::from(self.grants_ap) * i16::from(v) + palier.map_or(0, |p| i16::from(*p))
    }
}

fn radix(resources: &[Resource], resource: usize) -> usize {
    usize::from(resources[resource].max) + 1
}

fn encode(deps: &[usize], resources: &[Resource], res: &[u8; MAX_RESOURCES]) -> usize {
    deps.iter().fold(0usize, |idx, &d| {
        idx * radix(resources, d) + usize::from(res[d])
    })
}

/// Les entrées d'une table de dégâts où des cartes tirées au hasard attendent
/// ([`dofus_ruleset::ResourceDef::random_among`]) : la moyenne, sur chaque
/// carte que le premier tirage peut donner, de l'entrée qui la tient avec un
/// tirage de moins. Une carte déjà à son plafond ne s'ajoute pas ; une carte
/// que le sort ne lit pas laisse son entrée telle quelle.
///
/// Les entrées sans tirage sont déjà calculées : on remonte par nombre de
/// tirages croissant, chacune lisant des entrées d'un tirage de moins.
fn moyenner_les_tirages(deps: &[usize], resources: &[Resource], odd: &mut [Damage], even: &mut [Damage]) {
    let tirages: Vec<usize> = deps
        .iter()
        .copied()
        .filter(|&d| !resources[d].tirage_parmi.is_empty())
        .collect();
    if tirages.is_empty() {
        return;
    }
    // Le pas de chaque compteur dans l'indice, comme `encode` le compose ;
    // zéro pour un compteur que la table ne lit pas.
    let mut pas = [0usize; MAX_RESOURCES];
    let mut p = 1usize;
    for &d in deps.iter().rev() {
        pas[d] = p;
        p *= radix(resources, d);
    }
    let valeur = |index: usize, r: usize| -> u8 {
        if pas[r] == 0 {
            0
        } else {
            ((index / pas[r]) % radix(resources, r)) as u8
        }
    };
    let total: usize = tirages.iter().map(|&t| usize::from(resources[t].max)).sum();
    for niveau in 1..=total {
        for index in 0..odd.len() {
            if tirages.iter().map(|&t| usize::from(valeur(index, t))).sum::<usize>() != niveau {
                continue;
            }
            let t = tirages
                .iter()
                .copied()
                .find(|&t| valeur(index, t) > 0)
                .expect("un tirage en attente");
            let parmi = &resources[t].tirage_parmi;
            let (mut a, mut b) = (0i64, 0i64);
            for &c in parmi {
                let mut apres = index - pas[t];
                if pas[c] != 0 && valeur(index, c) < resources[c].max {
                    apres += pas[c];
                }
                a += odd[apres].0;
                b += even[apres].0;
            }
            let n = parmi.len() as i64;
            odd[index] = Damage(a / n);
            even[index] = Damage(b / n);
        }
    }
}

fn decode(mut idx: usize, deps: &[usize], resources: &[Resource]) -> [u8; MAX_RESOURCES] {
    let mut res = [0u8; MAX_RESOURCES];
    for &d in deps.iter().rev() {
        let r = radix(resources, d);
        res[d] = (idx % r) as u8;
        idx /= r;
    }
    res
}

#[allow(clippy::too_many_arguments)]
fn compute_damage(
    spell: &Spell,
    res: &[u8; MAX_RESOURCES],
    mult: FinalMultiplier,
    profile: &DamageProfile,
    mode: Mode,
    resistance: &Resistance,
    resources: &[Resource],
    lancer: CastContext,
) -> Damage {
    compute_lines(
        &spell.lines,
        spell.crit_rate,
        spell.can_crit,
        res,
        mult,
        profile,
        mode,
        resistance,
        resources,
        lancer,
    )
}

/// À partir de ce critique au build, tout coup qui peut critiquer critique ;
/// en dessous, le taux reste celui du sort plus celui du build, plafonné à
/// cent. Le même seuil que celui des valeurs critiques des effets : c'est la
/// même règle.
pub const CRITIQUE_CERTAIN: i32 = dofus_ruleset::CRITICAL_EFFECT_THRESHOLD;

/// Le taux d'un sort qui peut critiquer, vu le critique du build.
///
/// Public pour que les Zones de sorts disent le même taux que la rotation.
pub fn taux_critique(base: i32, critique_du_build: i32) -> CritRate {
    if critique_du_build >= CRITIQUE_CERTAIN {
        CritRate::ALWAYS
    } else {
        CritRate::from_percent(base + critique_du_build)
    }
}

#[allow(clippy::too_many_arguments)]
fn compute_lines(
    lines: &[Line],
    crit_rate: CritRate,
    // `crit_modifiable` : le taux peut-il être relevé par une ressource. Faux
    // pour un sort qui ne critique pas, et pour les charges utiles précalculées
    // à compteurs nuls, où aucun modificateur ne s'applique de toute façon.
    crit_modifiable: bool,
    res: &[u8; MAX_RESOURCES],
    mult: FinalMultiplier,
    profile: &DamageProfile,
    mode: Mode,
    resistance: &Resistance,
    resources: &[Resource],
    // Sort ou arme, mêlée ou distance : ce que lisent les % d'un domaine
    // (`Modifier::DomainPercent`).
    lancer: CastContext,
) -> Damage {
    // A state's buff can land at three different stages of the pipeline, and
    // the same nominal figure is worth different amounts at each of them.
    let mut mult = mult;
    let mut adjusted = *profile;
    let mut flat_base = 0i32;
    let mut resistance_ajustee = *resistance;

    // PREMIÈRE PASSE : le taux de coup critique, et lui seul.
    //
    // Il faut le connaître avant d'appliquer le reste, parce que plusieurs
    // modificateurs ont une valeur DIFFÉRENTE sur un jet critique et que le
    // choix entre les deux se fait au seuil. Les Tirs Puissants du Crâ portent
    // les deux à la fois, 250 Puissance ou 300 en critique ET quinze points de
    // taux : les traiter dans une seule boucle ferait dépendre le résultat de
    // l'ordre de déclaration.
    let mut crit_bonus = 0i32;
    for (i, resource) in resources.iter().enumerate() {
        if res[i] == 0 {
            continue;
        }
        for (modifier, fois) in resource.bonus(res[i]) {
            // Sa propre valeur critique se choisit au taux d'AVANT les bonus :
            // c'est celui du lancer qui pose l'état que le seuil regarde.
            if let Modifier::CritRate(percent, critique) = modifier {
                crit_bonus += magnitude(*percent, *critique, crit_rate) * fois;
            }
        }
    }
    let crit_rate = if crit_modifiable && crit_bonus != 0 && crit_rate != CritRate::ALWAYS {
        CritRate::from_percent(crit_rate.permille() / 10 + crit_bonus)
    } else {
        crit_rate
    };

    // SECONDE PASSE : tout le reste, au taux désormais connu.
    for (i, resource) in resources.iter().enumerate() {
        if res[i] == 0 {
            continue;
        }
        // Un buff cumulable vaut son montant par charge : l'Arcanin de
        // l'Ouginak donne 100 Puissance et se cumule deux fois, donc 200 à
        // deux charges.
        for (modifier, charges) in resource.bonus(res[i]) {
            match modifier {
                Modifier::FinalMultiplier(percent, critique, finaux) => {
                    let p = match critique {
                        Some(c)
                            if crit_rate.permille()
                                >= dofus_ruleset::CRITICAL_EFFECT_THRESHOLD * 10 =>
                        {
                            *c
                        }
                        _ => *percent,
                    };
                    // Les crans d'un même état s'additionnent, ils ne se
                    // composent pas : « % Dommages finaux » est une statistique
                    // du personnage, et dix projections de portail à 2 % font
                    // 20 %, appliqués une fois. Deux états différents, eux, se
                    // composent, ce que fait la boucle extérieure.
                    //
                    // En i64 et pas en `saturating_sub` : un état qui réduit les
                    // dégâts porte un pourcentage sous cent (les 65 % du
                    // Holmgang), qu'une soustraction saturée ramènerait à un
                    // multiplicateur neutre.
                    //
                    // Les % de dommages finaux s'additionnent aussi à ceux des
                    // Dofus et de l'équipement ; des dommages subis par la cible,
                    // eux, multiplient.
                    if charges > 0 {
                        let bonus = (i64::from(p) - 100) * i64::from(charges);
                        mult = if *finaux {
                            mult.plus_finaux(bonus)
                        } else {
                            mult.times(u32::try_from((100 + bonus).max(0)).unwrap_or(0))
                        };
                    }
                }
                Modifier::Characteristic(amount, critique, element) => {
                    let a = magnitude(*amount, *critique, crit_rate);
                    match element {
                        // Pas d'element : c'est de la Puissance, elle leve les
                        // cinq lignes.
                        None => {
                            for stats in adjusted.elements.iter_mut() {
                                stats.characteristic += a * charges;
                            }
                        }
                        // Un element : c'est un vol de caracteristique. ⚠️ La
                        // Terre emporte le Neutre, les deux tirant de la Force.
                        Some(e) => {
                            adjusted.elements[e.index()].characteristic += a * charges;
                            if *e == Element::Earth {
                                adjusted.elements[Element::Neutral.index()].characteristic +=
                                    a * charges;
                            }
                        }
                    }
                }
                Modifier::BaseDamage(amount, critique) => {
                    flat_base += magnitude(*amount, *critique, crit_rate) * charges
                }
                // Au même étage que les « Dommages » de l'équipement : après
                // l'arrondi caractéristique, et non dedans.
                Modifier::FlatDamage(amount, critique) => {
                    let a = magnitude(*amount, *critique, crit_rate);
                    for stats in adjusted.elements.iter_mut() {
                        stats.flat_damage += a * charges;
                    }
                }
                Modifier::CritRate(..) => {} // traité dans la première passe
                Modifier::CritDamage(amount, critique) => {
                    adjusted.flat_crit_damage += magnitude(*amount, *critique, crit_rate) * charges
                }
                // La résistance critique se SOUSTRAIT du bonus critique de
                // l'attaquant : en retirer à la cible en rend au lanceur.
                Modifier::CritResistance(amount, critique) => {
                    resistance_ajustee.critical -= magnitude(*amount, *critique, crit_rate) * charges
                }
                // Hors des lignes : le lancer qui pousse le lit, voir
                // `Engine::poussee_du_lancer`.
                Modifier::PushDamage(..) => {}
                // Des % d'un domaine : seulement sur un lancer de ce domaine,
                // en facteur à part (voir `DamageModifier::DomainPercent`).
                Modifier::DomainPercent(livraison, portee, pourcent) => {
                    if livraison.is_none_or(|l| l == lancer.delivery)
                        && portee.is_none_or(|r| r == lancer.reach)
                    {
                        mult = mult.times_bonus(pourcent * charges);
                    }
                }
            }
        }
    }
    let profile = &adjusted;
    let mut total = Damage::ZERO;
    let gagnants = paliers_gagnants(lines, res);
    for line in lines {
        // Une ligne de palier ne frappe qu'a son cran, ni en dessous ni
        // au-dessus : c'est ce qui distingue des paliers de largeurs
        // differentes d'un simple bonus qui decalerait la fourchette. Et quand
        // les paliers se chevauchent, seul le vainqueur du groupe frappe.
        if !ligne_active(line, res, &gagnants) {
            continue;
        }
        // Une ligne qui se repete par point de ressource ne frappe pas du tout
        // quand la ressource est a zero.
        let repetitions = repetitions(line, res);
        if repetitions == 0 {
            continue;
        }
        // Une ligne en pourcentage de vie est BRUTE : ni bonus de base, ni
        // fourchette, ni critique, ni multiplicateur. Seules les résistances de
        // la cible la réduisent. Ses cibles et ses répétitions, elles, restent
        // celles de toute ligne.
        let une = if let Some(valeur) = line.brut {
            raw_line(valeur, line.element, &resistance_ajustee)
        } else {
            let bonus: i32 = line
                .per_resource
                .iter()
                .map(|(r, a)| a * i32::from(res[*r]))
                .sum::<i32>()
                + line
                    .per_resource_gated
                    .iter()
                    .map(|(r, garde, a)| {
                        if res[*garde] > 0 {
                            a * i32::from(res[*r])
                        } else {
                            0
                        }
                    })
                    .sum::<i32>()
                + line
                    .steps
                    .iter()
                    .map(|(r, paliers)| {
                        (0..usize::from(res[*r]))
                            .map(|i| paliers[i.min(paliers.len().saturating_sub(1))])
                            .sum::<i32>()
                    })
                    .sum::<i32>()
                + line
                    .while_resource
                    .iter()
                    .map(|(r, a)| if res[*r] > 0 { *a } else { 0 })
                    .sum::<i32>()
                + flat_base;

            let shift = |range: Option<(i32, i32)>| range.map(|(lo, hi)| (lo + bonus, hi + bonus));
            // Un sort qui ne peut pas critiquer n'a pas de fourchette critique : son
            // taux est `NEVER`, et on retombe sur la fourchette normale plutôt que de
            // paniquer.
            let Some(critical) = shift(line.critical).or_else(|| shift(line.normal)) else {
                continue;
            };

            // Le facteur propre a la ligne multiplie avec les autres %, au meme
            // arrondi ; une ligne sans critique garde son jet normal. Une attaque
            // d'invocation ne prend que la part transmise des caracteristiques,
            // sans aucun % ni dommage critique.
            let transmis = line.invocation.map(|f| profil_transmis(profile, f));
            let profile = transmis.as_ref().unwrap_or(profile);
            let propre = dommages_propres(line, profile);
            let profile = propre.as_ref().unwrap_or(profile);
            let base = if line.invocation.is_some() { FinalMultiplier::NEUTRAL } else { mult };
            let mult = line.facteur.map_or(base, |f| base.times(f));
            let crit_rate = if line.sans_critique { CritRate::NEVER } else { line.crit.unwrap_or(crit_rate) };
            let critical = if line.sans_critique { shift(line.normal).unwrap_or(critical) } else { critical };

            // Chaque instance traverse le pipeline pour son compte : arrondir une
            // fois sur un jet multiplie ne donne pas le meme nombre qu'arrondir N
            // fois sur le jet nominal.
            //
            // Vaut aussi entre cibles : la deuxieme prend son propre arrondi, et
            // son propre taux quand la zone est degressive.
            match mode {
                Mode::PrototypeCompat => compat_midpoint_crit(
                    &SpellLine {
                        element: line.element,
                        normal: critical,
                        critical,
                    },
                    profile,
                    mult,
                ),
                Mode::Expected => {
                    let normal = shift(line.normal).unwrap_or(critical);
                    expected_line(
                        &SpellLine {
                            element: line.element,
                            normal,
                            critical,
                        },
                        profile,
                        mult,
                        crit_rate,
                        &resistance_ajustee,
                    )
                }
            }
        };
        // Une issue d'un tirage compte pour sa chance : la moyenne des issues.
        let une = match line.tirage {
            Some(Tirage { part: (n, d), .. }) => Damage(une.0 * i64::from(n) / i64::from(d)),
            None => une,
        };
        // Le plafond de cibles : la ligne ne frappe pas plus de fois que la
        // somme de ses compteurs. Vide, elle frappe toutes ses cibles, et la
        // borne vaut alors leur nombre.
        let cibles_frappees = if line.plafond_cibles.is_empty() {
            line.par_cible.len()
        } else {
            line.plafond_cibles
                .iter()
                .map(|r| usize::from(res[*r]))
                .sum::<usize>()
                .min(line.par_cible.len())
        };
        for _ in 0..repetitions {
            // Une ligne mono-cible porte `[100]` : la boucle tourne une fois et
            // le taux ne retire rien, ce qui rend le cas courant gratuit.
            for taux in line.par_cible.iter().take(cibles_frappees) {
                total += if *taux == 100 {
                    une
                } else {
                    Damage(une.0 * i64::from(*taux) / 100)
                };
            }
        }
    }
    total
}

/// Le profil que voit une ligne qui porte ses propres Dommages
/// (`LineDef::dommages_fixes`) ou sa propre Puissance
/// (`LineDef::puissance_propre`), ou rien quand elle n'en porte pas.
///
/// Les Dommages au même étage que ceux de l'équipement, et comme eux sur
/// l'élément de la ligne : la statistique « Dommages » s'ajoute une fois à
/// chaque élément. La Puissance avec celle du build, dans le multiplicateur de
/// la caractéristique.
fn dommages_propres(line: &Line, profil: &DamageProfile) -> Option<DamageProfile> {
    (line.dommages_fixes != 0 || line.puissance_propre != 0).then(|| {
        let mut p = *profil;
        p.elements[line.element.index()].flat_damage += line.dommages_fixes;
        p.power += line.puissance_propre;
        p
    })
}

/// Which magnitude an effect uses when the game gives two.
///
/// A cast either crits or it does not, and an effect like "grants 250 Puissance,
/// 300 on a critical" has one value, not an average. Averaging would invent a
/// bonus the game never grants, so the choice is a threshold, the one of
/// [`dofus_ruleset::CRITICAL_EFFECT_THRESHOLD`] (78 %, the rule).
///
/// Damage lines are untouched by this: they carry both ranges and the pipeline
/// weights them by the real rate.
fn magnitude(normal: i32, critical: Option<i32>, rate: CritRate) -> i32 {
    match critical {
        Some(c) if rate.permille() >= dofus_ruleset::CRITICAL_EFFECT_THRESHOLD * 10 => c,
        _ => normal,
    }
}

/// L'élément sur lequel le build frappe le plus fort : celui dont la
/// caractéristique, qui multiplie le jet, est la plus haute. À égalité, les
/// dommages fixes de l'élément départagent, puis l'ordre Terre, Neutre, Feu,
/// Eau, Air : un build à 300 en Force et 300 en Agilité, avec des dommages Air,
/// frappe en Air.
fn best_element(profile: &DamageProfile) -> Element {
    let ordre = [
        Element::Earth,
        Element::Neutral,
        Element::Fire,
        Element::Water,
        Element::Air,
    ];
    let mut retenu = ordre[0];
    for e in ordre.into_iter().skip(1) {
        let (actuel, candidat) = (profile.elements[retenu.index()], profile.elements[e.index()]);
        if candidat.characteristic > actuel.characteristic
            || (candidat.characteristic == actuel.characteristic
                && candidat.flat_damage > actuel.flat_damage)
        {
            retenu = e;
        }
    }
    retenu
}

/// Le meilleur élément d'un build, pour qui calcule hors de ce moteur.
///
/// Le simulateur de pièges en a besoin pour la Concentration de Chakra, qui
/// vole de la vie « dans le meilleur élément du lanceur ». Exposer la règle
/// plutôt que la recopier : deux copies finiraient par désigner deux éléments
/// différents pour le même personnage.
pub fn meilleur_element(profile: &DamageProfile) -> Element {
    best_element(profile)
}

/// L'element sur lequel le build frappe le plus faiblement (effet 2832).
fn worst_element(profile: &DamageProfile) -> Element {
    extreme_element(profile, false)
}

fn extreme_element(profile: &DamageProfile, haut: bool) -> Element {
    let mut retenu = Element::ALL[0];
    let mut score = profile.elements[retenu as usize].characteristic;
    for e in Element::ALL {
        let s = profile.elements[e as usize].characteristic;
        if (haut && s > score) || (!haut && s < score) {
            score = s;
            retenu = e;
        }
    }
    retenu
}

/// Les distances à l'impact des ennemis que cette zone atteint, triées du plus
/// proche au plus lointain. `None` quand il n'y a pas de placement, ou quand la
/// grille ne sait pas dessiner cette forme : l'appelant retombe alors sur le
/// régime déclaré.
fn eloignements(area: &AreaDef, cibles: &Cibles) -> Option<Vec<u16>> {
    let Cibles::Placees {
        lanceur,
        visee,
        ennemis,
    } = cibles
    else {
        return None;
    };
    let t = dofus_grid::tir::tir(
        area.shape,
        u16::from(area.size.unwrap_or(0)),
        u16::from(area.size2),
        *lanceur,
        *visee,
        ennemis,
        dofus_grid::tir::Contraintes::libre(0, u16::MAX),
    )
    .ok()?;
    Some(t.touches.iter().map(|x| x.eloignement).collect())
}

/// Le taux auquel chaque cible touchée prend cette ligne, la première d'abord.
/// Une zone dont la forme n'est pas décodée suit le nombre d'ennemis annoncé :
/// la ramener à une cible serait faux à coup sûr sur un sort « en zone ».
fn taux_par_cible(area: Option<&dofus_ruleset::AreaDef>, cibles: &Cibles) -> Vec<u32> {
    let Some(a) = area else {
        return vec![100];
    };
    let plafond = a.max_targets.map_or(u32::MAX, u32::from).max(1);
    // Le régime PLACÉ : la zone se dessine, on regarde qui tombe dedans, et on
    // lit chaque distance au lieu de la déduire.
    //
    // ⚠️ IL NE S'IMPOSE PAS QUAND LA FORME NE SE DESSINE PAS. `eloignements`
    // rend `None` dans ce cas, et le régime déclaré reprend la main. Un
    // placement qui rendrait alors une zone vide effacerait des dégâts réels
    // sans le dire ; `Cibles::placement_ignore` nomme ces lignes.
    if let Some(distances) = eloignements(a, cibles) {
        let combien = (distances.len() as u32).min(plafond) as usize;
        if a.falloff_percent == 0 || a.falloff_steps == 0 {
            return vec![100; combien];
        }
        return distances[..combien]
            .iter()
            .map(|d| {
                let crans = u32::from(*d).min(u32::from(a.falloff_steps));
                100u32.saturating_sub(crans * u32::from(a.falloff_percent))
            })
            .collect();
    }
    let Cibles::Declarees {
        combien: cibles,
        etalement,
    } = *cibles
    else {
        // Placement présent mais zone indessinable : on retombe sur le nombre
        // d'ennemis posés, tous à l'impact, ce qui est un plafond, annoncé.
        let combien = (u32::from(cibles.nombre())).min(plafond) as usize;
        return vec![100; combien];
    };
    let cibles = u32::from(cibles.max(1));
    let combien = cibles.min(plafond) as usize;
    // À étalement nul, personne ne s'éloigne : c'est le plafond d'avant, et
    // c'est le défaut.
    if etalement == 0 || a.falloff_percent == 0 || a.falloff_steps == 0 {
        return vec![100; combien];
    }
    // La première cible est sur l'impact et prend tout ; chaque suivante
    // s'éloigne de `etalement` cases, et le jeu retire `falloff_percent` par
    // cran, au plus `falloff_steps` fois. Toucher un ennemi de plus rapporte
    // toujours plus au total, simplement moins que le précédent : un test
    // vérifie que la courbe ne décroît pas.
    (0..combien)
        .map(|i| {
            let crans = (i as u32 * u32::from(etalement)).min(u32::from(a.falloff_steps));
            100u32.saturating_sub(crans * u32::from(a.falloff_percent))
        })
        .collect()
}

/// Combien de fois un rebond frappe : une fois s'il reste un ennemi que les
/// autres lignes du sort n'atteignent pas, à `rayon` cases au plus de la case
/// visée, et zéro sinon.
///
/// Sans placement, les ennemis déclarés sont tous supposés dans la zone des
/// autres lignes tant qu'elle a de la place, comme partout ailleurs : le
/// rebond ne trouve alors quelqu'un que s'ils sont plus nombreux qu'elle n'en
/// frappe.
fn rebond(rayon: u8, principales: &[Option<&AreaDef>], cibles: &Cibles) -> Vec<u32> {
    let declare = || {
        let deja = principales
            .iter()
            .map(|a| taux_par_cible(*a, cibles).len())
            .max()
            .unwrap_or(0);
        if usize::from(cibles.nombre()) > deja {
            vec![100]
        } else {
            Vec::new()
        }
    };
    let Cibles::Placees {
        lanceur,
        visee,
        ennemis,
    } = cibles
    else {
        return declare();
    };
    let libre = || dofus_grid::tir::Contraintes::libre(0, u16::MAX);
    let mut atteints = vec![false; ennemis.len()];
    for zone in principales {
        match zone {
            // Une ligne sans zone frappe la case visée.
            None => {
                for (i, e) in ennemis.iter().enumerate() {
                    if e == visee {
                        atteints[i] = true;
                    }
                }
            }
            Some(a) => {
                let Ok(t) = dofus_grid::tir::tir(
                    a.shape,
                    u16::from(a.size.unwrap_or(0)),
                    u16::from(a.size2),
                    *lanceur,
                    *visee,
                    ennemis,
                    libre(),
                ) else {
                    // Une zone que la grille ne dessine pas : on retombe sur
                    // le régime déclaré, comme les lignes elles-mêmes.
                    return declare();
                };
                for x in t.touches {
                    atteints[x.indice] = true;
                }
            }
        }
    }
    let Ok(cercle) = dofus_grid::tir::tir(
        'C',
        u16::from(rayon),
        0,
        *lanceur,
        *visee,
        ennemis,
        libre(),
    ) else {
        return declare();
    };
    if cercle.touches.iter().any(|x| !atteints[x.indice]) {
        vec![100]
    } else {
        Vec::new()
    }
}

/// La case visée ne porte pas ce que le sort exige.
///
/// Ne se lit qu'avec un placement : sans grille, le joueur est supposé viser
/// ce qu'il faut, et chaque mode reste ouvert au solveur.
fn visee_refusee(aim: Option<Aim>, cibles: &Cibles) -> bool {
    let Cibles::Placees { visee, ennemis, .. } = cibles else {
        return false;
    };
    let occupee = ennemis.contains(visee);
    match aim {
        Some(Aim::EmptyCell) => occupee,
        Some(Aim::Enemy) => !occupee,
        None => false,
    }
}

/// Les groupes de paliers rencontres a la compilation, et leur ordre.
///
/// Le nom d'un groupe et celui d'un palier ne servent qu'a les rapprocher entre
/// lignes ; le solveur, lui, compare des indices. L'ordre d'apparition EST la
/// priorite : le premier palier declare qui tient l'emporte, ce qui met la
/// hierarchie des combinaisons dans le ruleset et non dans ce fichier.
#[derive(Default)]
struct Paliers {
    groupes: Vec<(String, Vec<String>)>,
}

impl Paliers {
    fn index(&mut self, groupe: &str, palier: &str) -> (usize, usize) {
        let g = match self.groupes.iter().position(|(n, _)| n == groupe) {
            Some(i) => i,
            None => {
                self.groupes.push((groupe.to_string(), Vec::new()));
                self.groupes.len() - 1
            }
        };
        let t = match self.groupes[g].1.iter().position(|n| n == palier) {
            Some(i) => i,
            None => {
                self.groupes[g].1.push(palier.to_string());
                self.groupes[g].1.len() - 1
            }
        };
        (g, t)
    }
}

/// Les compteurs dont la valeur change quelque chose : un lancer permis ou
/// interdit, un coût, des dégâts, une charge, un état qui frappe.
///
/// Tout le reste n'est qu'écrit, et ses écritures ne font que multiplier les
/// états de la recherche.
fn compteurs_lus(
    spells: &[Spell],
    resources: &[Resource],
    schedules: &[Schedule],
    triggers: &[StateTrigger],
) -> [bool; MAX_RESOURCES] {
    let mut lus = [false; MAX_RESOURCES];
    for s in spells {
        let lecteurs = s
            .requires
            .into_iter()
            .flat_map(Exigence::compteurs)
            .chain(s.blocked_by)
            .chain(s.bloque_au_plafond)
            .chain(s.exige_surplus.into_iter().flat_map(|(a, b)| [a, b]))
            .chain(s.cost_reduction.map(|(r, ..)| r))
            .chain(s.cost_increase.map(|(r, ..)| r))
            .chain(s.extra_casts.map(|(r, ..)| r))
            .chain(s.damage_deps.iter().copied())
            .chain(s.effects.iter().filter_map(|e| match e {
                Eff::Gain { requires, .. }
                | Eff::StealAp { requires, .. }
                | Eff::CarryAp { requires, .. }
                | Eff::Reset { requires, .. }
                | Eff::Consume { requires, .. }
                | Eff::Schedule { requires, .. }
                | Eff::RemiseDesRelances { requires, .. } => *requires,
                _ => None,
            }).flat_map(Exigence::compteurs));
        for r in lecteurs {
            lus[r] = true;
        }
        // Une condition de lancement LIT les compteurs qui disent ses états :
        // sans cette ligne, un compteur que seule elle regarde verrait ses
        // écritures retirées, et la condition deviendrait muette.
        if let Some(c) = &s.critere {
            let mut v = Vec::new();
            c.lus(&mut v);
            for r in v {
                lus[r] = true;
            }
        }
        for c in s.poussees.iter().filter_map(|(c, _, _)| c.as_ref()) {
            let mut v = Vec::new();
            c.lus(&mut v);
            for r in v {
                lus[r] = true;
            }
        }
    }
    for s in schedules {
        for (r, ..) in &s.acceleration {
            lus[*r] = true;
        }
        for r in s.requires_at_turn_end.into_iter().flat_map(Exigence::compteurs) {
            lus[r] = true;
        }
    }
    for t in triggers {
        lus[t.state] = true;
        if let TriggerKind::Consumed(r) = t.kind {
            lus[r] = true;
        }
        for r in t.requires.into_iter().flat_map(Exigence::compteurs) {
            lus[r] = true;
        }
        if let Some(r) = t.palier {
            lus[r] = true;
        }
    }
    for (i, r) in resources.iter().enumerate() {
        // Un état dont la fin rend des sorts compte, même que rien ne lit.
        if !r.modifiers.is_empty()
            || !r.paliers.is_empty()
            || r.grants_ap > 0
            || !r.pa_par_valeur.is_empty()
            || r.relances_a_l_expiration != 0
            || !r.poussees.is_empty()
        {
            lus[i] = true;
        }
    }
    // Ce qui nourrit un compteur lu est lu à son tour : la garde d'une
    // maturation, la source d'un glissement.
    loop {
        let mut change = false;
        for (i, r) in resources.iter().enumerate() {
            let nourrit = r.gain_per_turn_while.filter(|_| lus[i]);
            if let Some(g) = nourrit {
                if !lus[g] {
                    lus[g] = true;
                    change = true;
                }
            }
            if let Some(t) = r.glisse_vers {
                if lus[t] && !lus[i] {
                    lus[i] = true;
                    change = true;
                }
            }
            // Le bout de chaîne qui vide un total lu.
            if let Some(t) = r.vide {
                if lus[t] && !lus[i] {
                    lus[i] = true;
                    change = true;
                }
            }
            // La garde qui fait retomber un compteur lu, ou lui retire un cran.
            if let Some(g) = r.perd_sous.filter(|_| lus[i]) {
                if !lus[g] {
                    lus[g] = true;
                    change = true;
                }
            }
            if let Some(g) = r.remis_sous.filter(|_| lus[i]) {
                if !lus[g] {
                    lus[g] = true;
                    change = true;
                }
            }
        }
        if !change {
            break;
        }
    }
    // Un etat dont l'expiration en pose un autre, lu, est lu lui aussi : sans
    // lui, le gain de Saoul serait retire et la Gueule de Bois ne verrait
    // jamais le Pandawa sortir de Saoul.
    loop {
        let mut change = false;
        for (i, r) in resources.iter().enumerate() {
            if let Some(t) = r.puis {
                if lus[t] && !lus[i] {
                    lus[i] = true;
                    change = true;
                }
            }
        }
        if !change {
            break;
        }
    }
    lus
}

/// Ce que l'invocateur transmet à son invocation : la part `f` (en pour cent)
/// de chaque caractéristique (jamais sous zéro), de la Puissance et des
/// dommages de chaque élément, et aucun dommage critique.
fn profil_transmis(profil: &DamageProfile, f: u32) -> DamageProfile {
    let part = |v: i32| (i64::from(v) * i64::from(f)).div_euclid(100) as i32;
    let mut p = profil.clone();
    for e in p.elements.iter_mut() {
        e.characteristic = part(e.characteristic.max(0));
        e.flat_damage = part(e.flat_damage);
    }
    p.power = part(p.power);
    p.flat_crit_damage = 0;
    p
}

/// Combien de fois la ligne frappe : une fois, ou la somme de ses compteurs.
fn repetitions(line: &Line, res: &[u8]) -> i64 {
    if line.repeats_per.is_empty() {
        return 1;
    }
    line.repeats_per.iter().map(|r| i64::from(res[*r])).sum()
}

/// Toutes les conditions de la ligne tiennent-elles ?
///
/// Chacune somme un ou plusieurs compteurs et exige un total exact. Une somme
/// d'un seul compteur, c'est le palier ordinaire.
fn condition_tient(line: &Line, res: &[u8]) -> bool {
    line.active_at.iter().all(|(compteurs, attendu)| {
        compteurs.iter().map(|r| u32::from(res[*r])).sum::<u32>() == u32::from(*attendu)
    })
}

/// Le palier vainqueur de chaque groupe : le PREMIER declare dont toutes les
/// conditions tiennent.
///
/// Rend une table vide quand aucune ligne n'appartient a un groupe, ce qui est
/// le cas de tous les sorts sauf la Rekop de l'Ecaflip : le cout est alors nul.
fn paliers_gagnants(lines: &[Line], res: &[u8]) -> Vec<Option<usize>> {
    let combien = lines
        .iter()
        .filter_map(|l| l.groupe)
        .map(|(g, _)| g + 1)
        .max()
        .unwrap_or(0);
    let mut out = vec![None; combien];
    for line in lines {
        let Some((g, t)) = line.groupe else { continue };
        // Deja un palier au moins aussi prioritaire : rien a gagner.
        if out[g].is_some_and(|deja| deja <= t) {
            continue;
        }
        if condition_tient(line, res) {
            out[g] = Some(t);
        }
    }
    out
}

/// Cette ligne frappe-t-elle, vu l'etat des compteurs et le palier vainqueur ?
fn ligne_active(line: &Line, res: &[u8], gagnants: &[Option<usize>]) -> bool {
    if !condition_tient(line, res) {
        return false;
    }
    match line.groupe {
        Some((g, t)) => gagnants[g] == Some(t),
        None => true,
    }
}

#[allow(clippy::too_many_arguments)]
fn compile_line(
    line: &LineDef,
    res_index: &HashMap<String, usize>,
    paliers: &mut Paliers,
    critical: Option<(i32, i32)>,
    normal: Option<(i32, i32)>,
    crit_rate: CritRate,
    profile: &DamageProfile,
    cibles: &Cibles,
    pm_depenses: u8,
    base_mp: u8,
    vie: &Vie,
) -> Line {
    let mut per_resource = Vec::new();
    let mut per_resource_gated = Vec::new();
    let mut steps = Vec::new();
    let mut while_resource = Vec::new();
    // Ce qui ne depend d'aucune ressource et se replie dans la fourchette.
    let mut fixe = 0i32;
    for b in &line.base_bonus {
        match b {
            BaseBonus::Steps {
                resource,
                steps: paliers,
            } => steps.push((res_index[resource], paliers.clone())),
            BaseBonus::PerResource {
                resource,
                amount,
                critical_amount,
            } => per_resource.push((
                res_index[resource],
                magnitude(*amount, *critical_amount, crit_rate),
            )),
            // Même convention que `PerResource` : une valeur par lancer,
            // choisie au seuil de [`CRITICAL_EFFECT_THRESHOLD`].
            BaseBonus::PerResourceGated {
                resource,
                amount,
                gated_by,
                critical_amount,
            } => per_resource_gated.push((
                res_index[resource],
                res_index[gated_by],
                magnitude(*amount, *critical_amount, crit_rate),
            )),
            BaseBonus::PerMpUsed {
                amount,
                cap,
                critical_amount,
            } => {
                let crans = i32::from(pm_depenses).min(i32::from(*cap));
                fixe += magnitude(*amount, *critical_amount, crit_rate) * crans;
            }
            // Le nombre d'ennemis est connu à la compilation : le bonus se
            // replie ici en un simple ajout, sans dimension d'état en plus.
            BaseBonus::PerExtraTarget {
                amount,
                cap,
                critical_amount,
            } => {
                let crans = i32::from(cibles.nombre().saturating_sub(1)).min(i32::from(*cap));
                fixe += magnitude(*amount, *critical_amount, crit_rate) * crans;
            }
            // Le produit se calcule ici, comme pour `PerExtraTarget`, mais il
            // ne se replie PAS dans la fourchette : il rejoint les termes lus a
            // chaque noeud de la recherche, pour ne compter que les tours ou la
            // ressource est la. Sans ressource, zero.
            BaseBonus::PerExtraTargetWhile {
                resource,
                amount,
                cap,
                critical_amount,
            } => {
                let crans = i32::from(cibles.nombre().saturating_sub(1)).min(i32::from(*cap));
                while_resource.push((
                    res_index[resource],
                    magnitude(*amount, *critical_amount, crit_rate) * crans,
                ));
            }
            BaseBonus::WhileResource {
                resource,
                amount,
                critical_amount,
            } => while_resource.push((
                res_index[resource],
                magnitude(*amount, *critical_amount, crit_rate),
            )),
        }
    }
    // Le Zénith du Iop : sa seconde ligne vaut au prorata des PM restants,
    // calculé ici une fois pour toutes, `pm_depenses` et `base_mp` étant fixes
    // pour toute la résolution. La proportion est linéaire, une lecture : les
    // sources ne donnent que les deux bornes (voir `scales_with_mp_left`).
    let prorata = |r: Option<(i32, i32)>| {
        if !line.scales_with_mp_left {
            return r;
        }
        let restants = i32::from(base_mp.saturating_sub(pm_depenses));
        let total = i32::from(base_mp).max(1);
        r.map(|(a, b)| (a * restants / total, b * restants / total))
    };
    let normal = prorata(normal);
    let critical = prorata(critical);
    let decale = |r: Option<(i32, i32)>| r.map(|(a, b)| (a + fixe, b + fixe));
    Line {
        per_resource_gated,
        element: match (line.best_element, line.worst_element) {
            (true, _) => best_element(profile),
            (_, true) => worst_element(profile),
            // `Ruleset::problems()` refuse au chargement toute ligne sans
            // element ET sans l'un des deux drapeaux : ce cas n'arrive pas.
            _ => line.element.unwrap_or(Element::Fire),
        },
        normal: decale(normal),
        critical: decale(critical),
        facteur: line.facteur,
        dommages_fixes: line.dommages_fixes,
        puissance_propre: line.puissance_propre,
        sans_critique: line.sans_critique,
        invocation: line.invocation,
        crit: line.taux_critique.map(|p| CritRate::from_percent(i32::from(p))),
        per_resource,
        steps,
        while_resource,
        repeats_per: line.repeats_per.iter().map(|r| res_index[r]).collect(),
        plafond_cibles: line
            .targets_capped_by
            .iter()
            .map(|r| res_index[r])
            .collect(),
        par_cible: if line.every_target {
            vec![100; usize::from(cibles.nombre())]
        } else {
            taux_par_cible(line.area.as_ref(), cibles)
        },
        active_at: line
            .active_at
            .as_ref()
            .map(|a| {
                a.conditions()
                    .iter()
                    .map(|c| {
                        (
                            c.resources().iter().map(|r| res_index[*r]).collect(),
                            c.exactly,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        groupe: line
            .active_at
            .as_ref()
            .and_then(|a| Some((a.group.as_ref()?, a.tier.as_ref()?)))
            .map(|(g, t)| paliers.index(g, t)),
        brut: line.percent_of_life.map(|p| vie.degats(&p)),
        // La liste entière le dit : `tirages`, une fois ses lignes compilées.
        tirage: None,
    }
}

fn compile_condition(c: &Condition, res_index: &HashMap<String, usize>) -> Exigence {
    // « Aucun de ces compteurs » se lit en masque, à côté des clauses ; sa
    // clause nommée est le premier d'entre eux, à zéro.
    let mut aucun = 0u32;
    let mut clause = |c: &Condition| match c {
        Condition::TargetHas { resource } | Condition::CasterHas { resource } => {
            (res_index[resource], 1, Comparaison::AuMoins)
        }
        Condition::AtLeast {
            resource, amount, ..
        } => (res_index[resource], *amount, Comparaison::AuMoins),
        Condition::Exactly { resource, amount } => (res_index[resource], *amount, Comparaison::Exactement),
        Condition::AtMost { resource, amount } => (res_index[resource], *amount, Comparaison::AuPlus),
        Condition::NoneOf { none_of } => {
            for r in none_of {
                aucun |= 1 << res_index[r];
            }
            // Le chargement du fichier refuse une liste vide.
            (res_index[&none_of[0]], 0, Comparaison::AuPlus)
        }
        // Le chargement du fichier refuse un `all` imbriqué.
        Condition::All { .. } => unreachable!("`all` imbriqué"),
    };
    let clauses: Vec<(usize, u8, Comparaison)> = match c {
        Condition::All { all } => all.iter().map(&mut clause).collect(),
        _ => vec![clause(c)],
    };
    // Le chargement du fichier borne une conjonction à trois clauses ; une
    // conjonction vide ne demande rien, et vaut « au moins zéro ».
    let (res, seuil, cmp) = clauses.first().copied().unwrap_or((0, 0, Comparaison::AuMoins));
    Exigence {
        res,
        seuil,
        cmp,
        et: [clauses.get(1).copied(), clauses.get(2).copied()],
        aucun,
    }
}

/// L'indice d'un budget, en le déclarant au passage s'il est nouveau.
fn budget_index(budgets: &mut Vec<String>, name: &str) -> usize {
    if let Some(i) = budgets.iter().position(|b| b == name) {
        return i;
    }
    budgets.push(name.to_string());
    budgets.len() - 1
}

fn compile_effect(
    e: &Effect,
    res_index: &HashMap<String, usize>,
    budgets: &mut Vec<String>,
) -> Eff {
    let mut budget_index = |name: &String| budget_index(budgets, name);
    match e {
        // Traités en amont par `compile_spell`, qui seul tient le registre des
        // charges en attente.
        Effect::Schedule { .. } | Effect::ScheduleGain { .. } => {
            unreachable!("les charges différées sont compilées par compile_spell")
        }
        Effect::Damage => Eff::Damage,
        Effect::Gain {
            resource,
            amount,
            ap_bonus,
            budget,
            at_cap,
            requires,
            up_to_zone,
            on_critical,
        } => Eff::Gain {
            resource: res_index[resource],
            // Remplacé par la taille de la zone dans `compile_spell`, qui seul
            // connaît les lignes du sort.
            amount: *amount,
            ap_bonus: ap_bonus
                .as_ref()
                .map(|b| (b.amount, b.once_per_spell_per_turn)),
            budget: budget.as_ref().map(&mut budget_index),
            at_cap: *at_cap,
            requires: requires.as_ref().map(|c| compile_condition(c, res_index)),
            jusqu_a: *up_to_zone,
            si_critique: *on_critical,
        },
        Effect::Consume {
            resource,
            amount,
            requires,
        } => Eff::Consume {
            requires: requires.as_ref().map(|c| compile_condition(c, res_index)),
            resource: res_index[resource],
            amount: *amount,
        },
        Effect::ConsumeAcross {
            resources,
            amount,
            by_zone,
        } => Eff::ConsumeAcross {
            resources: resources.iter().map(|r| res_index[r]).collect(),
            amount: *amount,
            zone: *by_zone,
        },
        Effect::StealAp { amount, requires } => Eff::StealAp {
            amount: *amount,
            requires: requires.as_ref().map(|c| compile_condition(c, res_index)),
        },
        Effect::CarryAp {
            amount,
            cap,
            requires,
        } => Eff::CarryAp {
            amount: *amount,
            cap: *cap,
            requires: requires.as_ref().map(|c| compile_condition(c, res_index)),
        },
        Effect::Reset { resource, requires } => Eff::Reset {
            resource: res_index[resource],
            requires: requires.as_ref().map(|c| compile_condition(c, res_index)),
        },
        Effect::Toggle { resource } => Eff::Toggle {
            resource: res_index[resource],
        },
        Effect::EndTurn => Eff::FinDuTour,
        Effect::ResetCooldowns { spells, requires } => Eff::RemiseDesRelances {
            sorts: spells.clone(),
            masque: 0,
            requires: requires.as_ref().map(|c| compile_condition(c, res_index)),
        },
    }
}

impl fmt::Display for Solution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for t in &self.turns {
            let total = t.opening_damage + t.damage;
            writeln!(
                f,
                "Tour {} ({})  {}",
                t.turn,
                if t.odd { "impair" } else { "pair" },
                total
            )?;
            for note in &t.opening {
                writeln!(f, "      {note}")?;
            }
            if t.ap_left > 0 {
                writeln!(f, "      >>> {} PA non dépensés", t.ap_left)?;
            }
            for (i, c) in t.casts.iter().enumerate() {
                let notes = if c.notes.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", c.notes.join(", "))
                };
                writeln!(
                    f,
                    "   {}. {} ({} PA)  {}{}",
                    i + 1,
                    c.spell,
                    c.ap_cost,
                    c.damage,
                    notes
                )?;
            }
        }
        write!(f, "Total {} sur {} tours", self.total, self.turns.len())
    }
}

/// Drop spells that can be proved never worth casting.
///
/// A spell is a candidate only when it does nothing besides damage: no resource
/// gained or spent, no AP stolen or carried, no schedule armed, no cooldown, no
/// cast condition, and a damage figure that depends on no resource. Anything
/// else has a value that depends on the state, which a static comparison cannot
/// see.
///
/// `B` dealing more damage per AP does not make `A` useless, because `B` runs
/// out of casts: `A` is only dropped when the spells beating it can, on their
/// own, absorb every AP the turn could hold.
fn prune_dominated(
    spells: &mut [Spell],
    base_ap: u8,
    // Ce que les etats portes peuvent ajouter au budget, tous a leur plafond.
    // Zero partout sauf sur l'Osamodas.
    pa_des_etats: i32,
) -> Vec<(String, String)> {
    // An upper bound on the AP one turn can hold. What matters is the net: a
    // spell costing 3 and refunding 2 shrinks the turn's budget, so only spells
    // that give back more than they cost raise the ceiling. `CarryAp` hands AP
    // to the next turn and enters through the incoming carry instead.
    let carry_ceiling: i32 = spells
        .iter()
        .flat_map(|s| s.effects.iter())
        .filter_map(|e| match e {
            Eff::CarryAp { cap, .. } => Some(i32::from(*cap)),
            _ => None,
        })
        .max()
        .unwrap_or(0);

    let ap_ceiling: i32 = i32::from(base_ap)
        + carry_ceiling
        + pa_des_etats
        + spells
            .iter()
            .map(|s| {
                let gained: i32 = s
                    .effects
                    .iter()
                    .map(|e| match e {
                        Eff::Gain { ap_bonus, .. } => i32::from(ap_bonus.map_or(0, |(a, _)| a)),
                        Eff::StealAp { amount, .. } if AP_THEFT_LANDS => i32::from(*amount),
                        _ => 0,
                    })
                    .sum();
                let net = gained - i32::from(s.ap_base);
                net.max(0) * i32::from(s.cast_cap)
            })
            .sum::<i32>();

    let plain: Vec<usize> = (0..spells.len())
        .filter(|&i| {
            let s = &spells[i];
            s.inert && s.cooldown == 0 && s.cast_cap > 0 && s.ap_base > 0
        })
        .collect();

    let mut pruned = Vec::new();
    for &a in &plain {
        // Who beats `a`: no dearer, never less damage on either turn parity,
        // and not itself already dropped. Ties are broken by index so two
        // identical spells never eliminate each other.
        let mut absorbable: i32 = 0;
        let mut best_reason: Option<String> = None;
        for &b in &plain {
            if a == b || spells[b].dominated_by.is_some() {
                continue;
            }
            let (x, y) = (&spells[a], &spells[b]);
            let cheaper = y.ap_base <= x.ap_base;
            let stronger =
                y.damage_odd[0] >= x.damage_odd[0] && y.damage_even[0] >= x.damage_even[0];
            let strict = y.ap_base < x.ap_base
                || y.damage_odd[0] > x.damage_odd[0]
                || y.damage_even[0] > x.damage_even[0]
                || b < a;
            if cheaper && stronger && strict {
                absorbable += i32::from(y.ap_base) * i32::from(y.cast_cap);
                if best_reason.is_none() {
                    best_reason = Some(y.name.clone());
                }
            }
        }
        if absorbable >= ap_ceiling {
            if let Some(reason) = best_reason {
                spells[a].dominated_by = Some(reason.clone());
                pruned.push((spells[a].name.clone(), reason));
            }
        }
    }
    pruned
}

// ---------------------------------------------------------------------------
// Steady state: opener plus loop
// ---------------------------------------------------------------------------

/// One node of the between-turn graph.
///
/// Parity is part of the identity, not a detail. A build carrying an odd-turn
/// bonus has genuinely different turns depending on where it stands, so a cycle
/// has to return to the same state AND the same parity. That makes the graph
/// bipartite, so every cycle has even length: a rotation that repeats every
/// turn shows up as a two-turn cycle whose halves happen to be identical.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct LoopNode {
    pub odd: bool,
    state: State,
}

/// Les fins de tour déjà calculées, indexées par l'état de départ et la
/// parité, derrière un `Arc` : lues bien plus souvent qu'écrites, elles se
/// partagent sans recopie sous le verrou.
type OutcomeCache = Map<(State, bool), std::sync::Arc<Vec<(Damage, Outcome, u8)>>>;

/// A walk through the between-turn graph: each step is the state it starts from
/// and the end-of-turn outcome chosen there.
type Walk = Vec<(LoopNode, Outcome)>;

/// Le graphe entre tours en tableaux plats, pour Howard. Les arêtes du nœud
/// `u` sont `debut[u]..debut[u + 1]` ; `issue` dit laquelle des fins de tour
/// du nœud chaque arête prend, pour retrouver le tour joué sans stocker
/// l'`Outcome` lui-même.
struct GrapheCompact {
    noeuds: Vec<LoopNode>,
    debut: Vec<u32>,
    vers: Vec<u32>,
    poids: Vec<i64>,
    issue: Vec<u32>,
}

/// One turn of the between-turn graph: what it is worth, and where it leads.
#[derive(Clone, Copy)]
struct Edge {
    damage: Damage,
    outcome: Outcome,
    to: LoopNode,
}

impl Engine {
    /// Taille du graphe entre tours : nœuds et arêtes. Pour mesurer avant de
    /// choisir un algorithme.
    pub fn graph_size(&self) -> (usize, usize) {
        let (order, edges, _) = self.reachable_graph(usize::MAX);
        let aretes = edges.values().map(Vec::len).sum();
        let puits = order
            .iter()
            .filter(|n| edges.get(n).is_none_or(Vec::is_empty))
            .count();
        if puits > 0 {
            eprintln!("   {puits} états sans aucun successeur");
        }
        (order.len(), aretes)
    }

    /// Every between-turn state the build can reach, and the turns joining them:
    /// the graph a steady-state search runs on, finite because the state is.
    /// Arrêté dès qu'il dépasse `plafond` nœuds, le booléen le disant : la boucle
    /// renonce au-delà de toute façon.
    fn reachable_graph(&self, plafond: usize) -> (Vec<LoopNode>, Map<LoopNode, Vec<Edge>>, bool) {
        let start = LoopNode {
            odd: self.turn_is_odd(0),
            state: self.initial_state(),
        };
        let mut order = vec![start];
        let mut edges: Map<LoopNode, Vec<Edge>> = Map::default();
        let mut queue = vec![start];
        edges.insert(start, Vec::new());

        while let Some(node) = queue.pop() {
            if order.len() > plafond {
                return (order, edges, false);
            }
            let (opening, opened) = self.turn_start(&node.state, node.odd);
            let mut out = Vec::new();
            for &(damage, outcome, _) in self.turn_outcomes(&opened, node.odd).iter() {
                let next = LoopNode {
                    odd: !node.odd,
                    state: self.advance(&opened, &outcome),
                };
                out.push(Edge {
                    damage: opening + damage,
                    outcome,
                    to: next,
                });
                if let std::collections::hash_map::Entry::Vacant(place) = edges.entry(next) {
                    place.insert(Vec::new());
                    order.push(next);
                    queue.push(next);
                }
            }
            edges.insert(node, out);
        }
        (order, edges, true)
    }
}

/// The best repeatable rotation, and the opening that leads into it.
#[derive(Clone, Debug)]
pub struct SteadyState {
    /// Turns played once, before the cycle begins. Possibly empty.
    pub opener: Vec<TurnPlan>,
    /// The cycle itself, repeated for as long as the fight lasts.
    pub cycle: Vec<TurnPlan>,
    /// Damage per turn of the cycle, as an exact fraction: total damage over
    /// the cycle divided by its length. Kept exact rather than rounded, since
    /// the whole comparison between two cycles rests on it.
    pub per_turn: (i64, u32),
    /// Between-turn states explored. Un minimum seulement quand la boucle a
    /// renoncé pour [`GaveUp::TooManyStates`] : l'exploration s'arrête au
    /// plafond.
    pub states: usize,
    /// Arêtes du graphe entre tours. Le coût de Karp est `états x arêtes`, et
    /// c'est presque toujours ce produit qui fait renoncer, pas le nombre
    /// d'états seul.
    pub edges: usize,
    /// Ce qui a fait renoncer, quand la boucle n'a pas été cherchée.
    pub gave_up: Option<GaveUp>,
}

/// Laquelle des deux bornes a été franchie, le nombre d'états ou le produit
/// états x arêtes : le message au joueur dit pourquoi la boucle a renoncé.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GaveUp {
    /// Trop d'états pour que la table de Karp tienne en mémoire.
    TooManyStates,
    /// Assez peu d'états, mais un graphe trop dense : trop d'arêtes pour la
    /// recherche.
    TooMuchWork,
    /// Howard n'a pas convergé dans ses itérations : pas de boucle plutôt
    /// qu'une boucle que rien ne garantit.
    NotConverged,
}

impl SteadyState {
    /// Damage per turn, for display.
    pub fn per_turn_value(&self) -> f64 {
        self.per_turn.0 as f64 / f64::from(self.per_turn.1) / SCALE as f64
    }
}

impl Engine {
    /// Au-delà de ce produit `états x arêtes`, Karp ne cherche pas la boucle : sa
    /// table fait `(n+1) × n` et son coût `n x E`. Cent millions passent sous la
    /// seconde ; un deck Xélor complet (12 235 états, 1 183 497 arêtes) en demande
    /// 14,5 milliards. Howard prend le relais au-delà, jusqu'à
    /// [`Self::MAX_CYCLE_STATES`].
    pub const MAX_LOOP_WORK: usize = 100_000_000;
    /// La table de Karp fait `(n+1) x n` : au-dela, elle ne tient plus en
    /// memoire quel que soit le temps disponible. 4000 etats font 256 Mo.
    pub const MAX_LOOP_STATES: usize = 4000;
    /// Au-dela des bornes de Karp, Howard cherche la boucle jusqu'a ces deux-ci.
    ///
    /// Le Huppermage de quatorze sorts y tient, a trois cibles comprises
    /// (25 203 etats, 3,2 millions d'aretes), ses paliers de runes non. Le
    /// graphe compact coute seize octets par arete, 64 Mo au plafond.
    pub const MAX_CYCLE_STATES: usize = 30_000;
    pub const MAX_CYCLE_EDGES: usize = 4_000_000;
    /// Les iterations de Howard avant de renoncer, plutot que de rendre une
    /// boucle que rien ne garantit.
    const MAX_HOWARD_ITERATIONS: usize = 1_000;

    /// The rotation that maximises damage per turn over an unbounded fight.
    ///
    /// A fixed horizon cannot answer "is it worth delaying this spell": on the
    /// last turns of the window a delay is free, because the fight stops before
    /// the cost lands. So the question is asked over the between-turn graph:
    /// the cycle with the highest mean weight, exactly, by Karp's algorithm in
    /// O(V·E), or past Karp's bounds by Howard's policy iteration (`howard.rs`),
    /// one value per state instead of one per state and path length.
    pub fn steady_state(&self) -> SteadyState {
        // `solve` n'a chauffé que son horizon : le reste du graphe se cherche
        // ici, en parallèle, sans relancer la barre d'avancement. Jamais au-delà
        // du plafond d'états : la boucle y renonce, et `states` ne dit alors
        // qu'un minimum (les pages écrivent « plus de »).
        self.chauffer(
            false,
            None,
            false,
            Some((Self::MAX_CYCLE_STATES, Self::MAX_CYCLE_EDGES)),
        );
        // Karp tant qu'il tient, comme avant : ses boucles n'ont pas bougé.
        // Howard au-delà, qui trouve la même moyenne, les tests les
        // confrontant sur les petits graphes.
        let (order, edges, complet) = self.reachable_graph(Self::MAX_LOOP_STATES);
        let aretes: usize = edges.values().map(Vec::len).sum();
        if complet && order.len().saturating_mul(aretes) <= Self::MAX_LOOP_WORK {
            return self.karp(&order, &edges, aretes);
        }
        drop(edges);
        self.boucle_howard()
    }

    /// La boucle par Howard, sur tout graphe sous [`Self::MAX_CYCLE_STATES`]
    /// et [`Self::MAX_CYCLE_EDGES`].
    ///
    /// Publique pour être confrontée à Karp sur les petits graphes, où les deux
    /// doivent trouver la même moyenne au point près. [`Self::steady_state`]
    /// ne l'appelle qu'au-delà des bornes de Karp.
    pub fn steady_state_howard(&self) -> SteadyState {
        self.chauffer(
            false,
            None,
            false,
            Some((Self::MAX_CYCLE_STATES, Self::MAX_CYCLE_EDGES)),
        );
        self.boucle_howard()
    }

    /// [`Self::steady_state_howard`] sur un cache déjà chauffé.
    fn boucle_howard(&self) -> SteadyState {
        let renoncer = |gave_up, states, edges| SteadyState {
            opener: Vec::new(),
            cycle: Vec::new(),
            per_turn: (0, 1),
            states,
            edges,
            gave_up: Some(gave_up),
        };
        let g = match self.graphe_compact(Self::MAX_CYCLE_STATES, Self::MAX_CYCLE_EDGES) {
            Ok(g) => g,
            Err((gave_up, states, edges)) => return renoncer(gave_up, states, edges),
        };
        let (states, edges) = (g.noeuds.len(), g.vers.len());
        let graphe = howard::Graphe {
            debut: &g.debut,
            vers: &g.vers,
            poids: &g.poids,
        };
        let Some((pol, eta)) = howard::howard(&graphe, Self::MAX_HOWARD_ITERATIONS) else {
            return renoncer(GaveUp::NotConverged, states, edges);
        };
        // La politique suivie depuis le départ : l'ouverture, puis la boucle
        // où elle entre. Le premier nœud revu ferme la boucle.
        let mut position = vec![usize::MAX; states];
        let mut chemin: Vec<usize> = Vec::new();
        let mut u = 0;
        while position[u] == usize::MAX {
            position[u] = chemin.len();
            chemin.push(u);
            u = g.vers[pol[u] as usize] as usize;
        }
        let entree = position[u];
        let pas = |u: usize| -> (LoopNode, Outcome) {
            let node = g.noeuds[u];
            let (_, opened) = self.turn_start(&node.state, node.odd);
            let issue = self.turn_outcomes(&opened, node.odd)[g.issue[pol[u] as usize] as usize].1;
            (node, issue)
        };
        let opener: Walk = chemin[..entree].iter().map(|&u| pas(u)).collect();
        let cycle: Walk = chemin[entree..].iter().map(|&u| pas(u)).collect();
        let moyenne = eta[0];
        SteadyState {
            opener: self.plan_path(&opener),
            cycle: self.plan_path(&cycle),
            per_turn: (
                i64::try_from(moyenne.num).unwrap_or(i64::MAX),
                u32::try_from(moyenne.den).unwrap_or(u32::MAX),
            ),
            states,
            edges,
            gave_up: None,
        }
    }

    /// Le graphe entre tours en indices, pour Howard : seize octets par arête,
    /// au lieu de la centaine qu'une `Edge` porte avec ses deux états. `Err`
    /// dès qu'un plafond est franchi, avec ce qui a été compté jusque-là.
    fn graphe_compact(
        &self,
        max_etats: usize,
        max_aretes: usize,
    ) -> Result<GrapheCompact, (GaveUp, usize, usize)> {
        let depart = LoopNode {
            odd: self.turn_is_odd(0),
            state: self.initial_state(),
        };
        let mut index: Map<LoopNode, u32> = Map::default();
        index.insert(depart, 0);
        let mut g = GrapheCompact {
            noeuds: vec![depart],
            debut: vec![0],
            vers: Vec::new(),
            poids: Vec::new(),
            issue: Vec::new(),
        };
        let mut u = 0;
        while u < g.noeuds.len() {
            let node = g.noeuds[u];
            let (opening, opened) = self.turn_start(&node.state, node.odd);
            for (k, &(damage, outcome, _)) in self.turn_outcomes(&opened, node.odd).iter().enumerate() {
                let next = LoopNode {
                    odd: !node.odd,
                    state: self.advance(&opened, &outcome),
                };
                let v = *index.entry(next).or_insert_with(|| {
                    g.noeuds.push(next);
                    (g.noeuds.len() - 1) as u32
                });
                g.vers.push(v);
                g.poids.push((opening + damage).0);
                g.issue.push(k as u32);
            }
            g.debut.push(g.vers.len() as u32);
            if g.noeuds.len() > max_etats {
                return Err((GaveUp::TooManyStates, g.noeuds.len(), g.vers.len()));
            }
            if g.vers.len() > max_aretes {
                return Err((GaveUp::TooMuchWork, g.noeuds.len(), g.vers.len()));
            }
            u += 1;
        }
        Ok(g)
    }

    /// La boucle par Karp, sur un graphe complet sous ses bornes.
    fn karp(
        &self,
        order: &[LoopNode],
        edges: &Map<LoopNode, Vec<Edge>>,
        aretes: usize,
    ) -> SteadyState {
        let n = order.len();
        let index: Map<LoopNode, usize> = order
            .iter()
            .enumerate()
            .map(|(i, node)| (*node, i))
            .collect();

        // d[k][v]: heaviest walk of EXACTLY k edges from the source to v.
        // `None` where no such walk exists, which is not the same as a walk
        // worth nothing.
        const NONE: Option<i64> = None;
        let mut d = vec![vec![NONE; n]; n + 1];
        // Le prédécesseur qui réalise chaque d[k][v], pour pouvoir remonter le
        // chemin de n arêtes qui contient le cycle.
        let mut pred: Vec<Vec<Option<(usize, Outcome)>>> = vec![vec![None; n]; n + 1];
        d[0][0] = Some(0);
        for k in 1..=n {
            for (u, node) in order.iter().enumerate() {
                let Some(from) = d[k - 1][u] else { continue };
                for edge in &edges[node] {
                    let v = index[&edge.to];
                    let value = from + edge.damage.0;
                    if d[k][v].is_none_or(|current| value > current) {
                        d[k][v] = Some(value);
                        pred[k][v] = Some((u, edge.outcome));
                    }
                }
            }
        }

        // Karp: the best mean over all cycles is
        //   max over v of  min over k<n of  (d[n][v] - d[k][v]) / (n - k)
        // Compared as exact fractions: a rounded division here would pick the
        // wrong cycle whenever two are close, which is exactly when it matters.
        let mut best: Option<(i64, u32, usize)> = None;
        for v in 0..n {
            let Some(dn) = d[n][v] else { continue };
            let mut worst: Option<(i64, u32)> = None;
            for (k, row) in d.iter().enumerate().take(n) {
                let Some(dk) = row[v] else { continue };
                let num = dn - dk;
                let den = (n - k) as u32;
                let lower = worst.is_none_or(|(bn, bd)| {
                    i128::from(num) * i128::from(bd) < i128::from(bn) * i128::from(den)
                });
                if lower {
                    worst = Some((num, den));
                }
            }
            if let Some((num, den)) = worst {
                let better = best.is_none_or(|(bn, bd, _)| {
                    i128::from(num) * i128::from(bd) > i128::from(bn) * i128::from(den)
                });
                if better {
                    best = Some((num, den, v));
                }
            }
        }

        let Some((num, den, winner)) = best else {
            // No cycle at all: nothing repeats, so there is no steady state.
            // Ce n'est PAS un abandon : la recherche est allee au bout et n'a
            // rien trouve, d'ou `gave_up: None`.
            return SteadyState {
                opener: Vec::new(),
                cycle: Vec::new(),
                per_turn: (0, 1),
                states: n,
                edges: aretes,
                gave_up: None,
            };
        };

        let (opener_nodes, cycle_nodes) =
            self.trace_cycle(order, edges, (num, den), winner, &pred);
        SteadyState {
            opener: self.plan_path(&opener_nodes),
            cycle: self.plan_path(&cycle_nodes),
            per_turn: (num, den),
            states: n,
            edges: aretes,
            gave_up: None,
        }
    }
}

impl Engine {
    /// Split the graph into the cycle worth repeating and the walk that leads
    /// into it.
    ///
    /// Karp names a vertex whose heaviest n-edge walk ends on an optimal cycle.
    /// Walking that path backwards, the first vertex that repeats closes the
    /// cycle: everything between the two visits IS the cycle. Following tight
    /// edges forward from the SOURCE instead does not work, because the source
    /// need not lie on the cycle, and the walk can reach a vertex with no tight
    /// edge left to take.
    ///
    /// The opener is then the best way in: with every edge reweighted as
    /// `w * den - num`, every cycle has non-positive weight, so longest paths
    /// are well defined and one Bellman-Ford pass gives the walk from the
    /// starting state to whichever vertex of the cycle is worth entering at.
    fn trace_cycle(
        &self,
        order: &[LoopNode],
        edges: &Map<LoopNode, Vec<Edge>>,
        mean: (i64, u32),
        winner: usize,
        pred: &[Vec<Option<(usize, Outcome)>>],
    ) -> (Walk, Walk) {
        let n = order.len();
        let index: Map<LoopNode, usize> = order
            .iter()
            .enumerate()
            .map(|(i, node)| (*node, i))
            .collect();

        // Remonter le chemin de n arêtes aboutissant au sommet désigné.
        let mut walk: Vec<(usize, Outcome)> = Vec::new();
        let mut here = winner;
        for k in (1..=n).rev() {
            let Some((u, outcome)) = pred[k][here] else {
                break;
            };
            walk.push((u, outcome));
            here = u;
        }
        walk.reverse();

        // Le premier sommet qui se répète ferme le cycle. `walk` ne contient
        // que les sommets de départ de chaque arête : sur un graphe minuscule,
        // le cycle 0 → 1 → 0 laisse [0, 1], où rien ne se répète, et la
        // sentinelle rétablit la suite complète des sommets visités.
        let mut position: Vec<Option<usize>> = vec![None; n];
        let mut cycle: Walk = Vec::new();
        let sommets: Vec<usize> = walk
            .iter()
            .map(|(u, _)| *u)
            .chain(std::iter::once(winner))
            .collect();
        for (i, u) in sommets.iter().enumerate() {
            if let Some(debut) = position[*u] {
                cycle = walk[debut..i]
                    .iter()
                    .map(|(v, o)| (order[*v], *o))
                    .collect();
                break;
            }
            position[*u] = Some(i);
        }
        if cycle.is_empty() {
            return (Vec::new(), Vec::new());
        }

        // L'entrée du cycle, et le meilleur chemin depuis l'état de départ.
        let sur_le_cycle: Map<LoopNode, ()> = cycle.iter().map(|(node, _)| (*node, ())).collect();
        let (num, den) = (i128::from(mean.0), i128::from(mean.1));
        let ajuste = |d: Damage| i128::from(d.0) * den - num;

        let mut potentiel: Vec<Option<i128>> = vec![None; n];
        let mut venant_de: Vec<Option<(usize, Outcome)>> = vec![None; n];
        potentiel[0] = Some(0);
        for _ in 0..n {
            let mut change = false;
            for (u, node) in order.iter().enumerate() {
                let Some(ici) = potentiel[u] else { continue };
                for edge in &edges[node] {
                    let v = index[&edge.to];
                    let valeur = ici + ajuste(edge.damage);
                    if potentiel[v].is_none_or(|courant| valeur > courant) {
                        potentiel[v] = Some(valeur);
                        venant_de[v] = Some((u, edge.outcome));
                        change = true;
                    }
                }
            }
            if !change {
                break;
            }
        }

        // Le sommet du cycle où il vaut le mieux entrer.
        let entree = cycle
            .iter()
            .filter_map(|(node, _)| {
                let i = index[node];
                potentiel[i].map(|p| (p, i))
            })
            .max_by_key(|(p, _)| *p)
            .map(|(_, i)| i);

        let mut opener: Walk = Vec::new();
        if let Some(mut v) = entree {
            let mut vus = vec![false; n];
            while v != 0 && !vus[v] {
                vus[v] = true;
                let Some((u, outcome)) = venant_de[v] else {
                    break;
                };
                opener.push((order[u], outcome));
                v = u;
            }
            opener.reverse();
        }

        // Faire commencer le cycle là où l'opener y entre, pour que la lecture
        // suive l'ordre où les tours se jouent réellement.
        if let Some(v) = entree {
            if let Some(pos) = cycle.iter().position(|(node, _)| index[node] == v) {
                cycle.rotate_left(pos);
            }
        }
        let _ = sur_le_cycle;
        (opener, cycle)
    }

    /// Turn a walk through the graph into readable turns.
    fn plan_path(&self, path: &[(LoopNode, Outcome)]) -> Vec<TurnPlan> {
        path.iter()
            .enumerate()
            .map(|(i, (node, outcome))| {
                let (opening_damage, opened) = self.turn_start(&node.state, node.odd);
                let opening = self.turn_start_notes(&node.state, node.odd);
                let opening_sources = self.turn_start_sources(&node.state, node.odd);
                let (damage, casts, ap_left) = self.turn_plan(&opened, node.odd, outcome);
                TurnPlan {
                    turn: (i + 1) as u8,
                    odd: node.odd,
                    opening_damage,
                    opening_sources,
                    opening,
                    damage,
                    casts,
                    ap_left,
                }
            })
            .collect()
    }
}

/// L'avancement de la recherche en cours, pour l'interface. Un seul compteur
/// pour tout le processus suffit : il ne sert qu'à dessiner une barre,
/// l'interface ne lance jamais deux résolutions, et rien de ce que le moteur
/// calcule n'en dépend. Écrit par [`Engine::warm`], lu par l'application.
pub static AVANCEMENT: Avancement = Avancement::NEUF;

/// La demande d'arrêt de la recherche en cours : un calcul peut durer des
/// minutes. Un seul drapeau pour tout le processus, comme [`AVANCEMENT`] ;
/// chaque résolution le remet à zéro en démarrant, pour qu'un arrêt demandé
/// trop tard ne tue pas la suivante. L'arrêt est coopératif : la recherche le
/// lit entre deux états, s'arrête en quelques dixièmes de seconde, et ne rend
/// rien plutôt qu'un demi-résultat.
pub static ARRET: Arret = Arret(AtomicBool::new(false));

#[derive(Debug)]
pub struct Arret(AtomicBool);

impl Arret {
    /// Demande l'arrêt de la recherche en cours, s'il y en a une.
    pub fn demander(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    fn oublier(&self) {
        self.0.store(false, Ordering::Relaxed);
    }

    fn demande(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// Où en est la recherche : le tour exploré, sur combien, et ce qui a déjà été
/// visité dans ce tour.
#[derive(Debug)]
pub struct Avancement {
    tour: AtomicU32,
    tours: AtomicU32,
    faits: AtomicU32,
    a_faire: AtomicU32,
    etats: AtomicU64,
    en_cours: AtomicBool,
}

impl Avancement {
    const NEUF: Self = Self {
        tour: AtomicU32::new(0),
        tours: AtomicU32::new(0),
        faits: AtomicU32::new(0),
        a_faire: AtomicU32::new(0),
        etats: AtomicU64::new(0),
        en_cours: AtomicBool::new(false),
    };

    fn demarre(&self, tours: u32) {
        self.tours.store(tours.max(1), Ordering::Relaxed);
        self.tour.store(0, Ordering::Relaxed);
        self.faits.store(0, Ordering::Relaxed);
        self.a_faire.store(0, Ordering::Relaxed);
        self.etats.store(0, Ordering::Relaxed);
        self.en_cours.store(true, Ordering::Relaxed);
    }

    fn vague(&self, tour: u32, combien: usize) {
        self.tour.store(tour, Ordering::Relaxed);
        self.faits.store(0, Ordering::Relaxed);
        self.a_faire
            .store(u32::try_from(combien).unwrap_or(u32::MAX), Ordering::Relaxed);
    }

    fn un_de_plus(&self) {
        self.faits.fetch_add(1, Ordering::Relaxed);
        self.etats.fetch_add(1, Ordering::Relaxed);
    }

    fn termine(&self) {
        self.en_cours.store(false, Ordering::Relaxed);
    }

    /// Le tour en cours d'exploration, sur combien, combien d'états visités, et
    /// si une recherche tourne.
    ///
    /// La fraction vaut le tour commencé plus ce qui est fait dedans, sur
    /// l'horizon. Elle ne recule jamais et ne dépasse jamais un, la largeur
    /// d'abord parcourue s'arrêtant dès qu'elle n'ouvre plus rien de neuf.
    pub fn lire(&self) -> (u32, u32, f64, u64, bool) {
        let tours = self.tours.load(Ordering::Relaxed).max(1);
        let tour = self.tour.load(Ordering::Relaxed);
        let a_faire = self.a_faire.load(Ordering::Relaxed);
        let faits = self.faits.load(Ordering::Relaxed);
        let dedans = if a_faire == 0 {
            0.0
        } else {
            f64::from(faits.min(a_faire)) / f64::from(a_faire)
        };
        let fraction = ((f64::from(tour) + dedans) / f64::from(tours)).clamp(0.0, 1.0);
        (
            tour + 1,
            tours,
            fraction,
            self.etats.load(Ordering::Relaxed),
            self.en_cours.load(Ordering::Relaxed),
        )
    }
}

impl Engine {
    /// Compute every reachable turn search up front, several at a time: the
    /// searches for two states share nothing but the ruleset and the build.
    /// Explored in waves, breadth-first from the opening state; `solve` and
    /// `steady_state` then read a warm cache. Nothing here changes what is
    /// computed, only how many cores compute it.
    pub fn warm(&self, annulable: bool) {
        self.chauffer(annulable, None, true, None);
    }

    /// [`Self::warm`], arrêté après `vagues` tours ou au-delà de `plafond`
    /// (états, arêtes) quand ils sont donnés, et sans barre d'avancement quand
    /// `suivi` est faux. `solve` ne chauffe que son horizon : la vague k porte les
    /// états du tour k, et la descente ne lit rien au-delà du dernier tour.
    fn chauffer(
        &self,
        annulable: bool,
        vagues: Option<u8>,
        suivi: bool,
        plafond: Option<(usize, usize)>,
    ) {
        // `DOFUS_MONOFIL` force la version séquentielle, pour mesurer ce que le
        // parallélisme rend réellement.
        let fils = if std::env::var_os("DOFUS_MONOFIL").is_some() {
            1
        } else {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
                .min(16)
        };

        let mut vus: std::collections::HashSet<(State, bool)> = std::collections::HashSet::new();
        let depart = (self.initial_state(), self.turn_is_odd(0));
        let mut vague = vec![depart];
        vus.insert(depart);
        if suivi {
            AVANCEMENT.demarre(u32::from(self.scenario.horizon));
        }

        let mut numero = 0;
        let mut aretes = 0usize;
        while !vague.is_empty() {
            if annulable && ARRET.demande() {
                return;
            }
            if suivi {
                AVANCEMENT.vague(numero, vague.len());
            }
            numero += 1;
            // Une file partagée plutôt qu'un découpage en lots. Deux recherches
            // intra-tour n'ont aucune raison de coûter pareil : découper la
            // vague à l'avance donnait à un fil le lot contenant la plus lourde,
            // et les quinze autres l'attendaient. Chaque fil prend l'état
            // suivant quand il a fini le précédent.
            let curseur = std::sync::atomic::AtomicUsize::new(0);
            let travail = &vague;
            std::thread::scope(|portee| {
                for _ in 0..fils.min(travail.len()) {
                    let curseur = &curseur;
                    portee.spawn(move || loop {
                        let i = curseur.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((etat, odd)) = travail.get(i) else {
                            break;
                        };
                        // L'arrêt se lit ici, et pas seulement entre les
                        // vagues : une vague porte des milliers d'états.
                        if annulable && ARRET.demande() {
                            break;
                        }
                        let (_, ouvert) = self.turn_start(etat, *odd);
                        let _ = self.turn_outcomes(&ouvert, *odd);
                        if suivi {
                            AVANCEMENT.un_de_plus();
                        }
                    });
                }
            });

            if annulable && ARRET.demande() {
                return;
            }
            if vagues.is_some_and(|n| numero >= u32::from(n)) {
                return;
            }
            let mut suivante = Vec::new();
            for (etat, odd) in &vague {
                let (_, ouvert) = self.turn_start(etat, *odd);
                let issues = self.turn_outcomes(&ouvert, *odd);
                aretes += issues.len();
                for &(_, issue, _) in issues.iter() {
                    let prochain = (self.advance(&ouvert, &issue), !odd);
                    if vus.insert(prochain) {
                        suivante.push(prochain);
                    }
                }
            }
            if plafond.is_some_and(|(etats, max_aretes)| vus.len() > etats || aretes > max_aretes) {
                return;
            }
            vague = suivante;
        }
    }
}

#[cfg(test)]
mod seuil_critique {
    use super::*;

    /// Le seuil est une marche, pas une pente.
    ///
    /// Deux builds du meme cote du seuil recoivent la MEME magnitude, quel que
    /// soit l'ecart entre leurs taux. Tester cela sur des totaux de rotation ne
    /// marche pas : le taux de critique pese aussi sur les lignes de degats,
    /// qui varient continument. C'est la regle elle-meme qu'il faut interroger.
    #[test]
    fn le_seuil_est_une_marche() {
        let seuil = dofus_ruleset::CRITICAL_EFFECT_THRESHOLD;
        for taux in [0, 10, 40, seuil - 1] {
            assert_eq!(
                magnitude(250, Some(300), CritRate::from_percent(taux)),
                250,
                "a {taux} % de critique, sous le seuil de {seuil} %, la valeur normale s'applique"
            );
        }
        for taux in [seuil, seuil + 5, 100] {
            assert_eq!(
                magnitude(250, Some(300), CritRate::from_percent(taux)),
                300,
                "a {taux} % de critique, au seuil de {seuil} % ou au-dessus, la valeur critique s'applique"
            );
        }
    }

    /// Sans valeur critique dans les donnees, le taux ne change rien.
    #[test]
    fn sans_valeur_critique_le_taux_ne_change_rien() {
        for taux in [0, 50, 100] {
            assert_eq!(magnitude(250, None, CritRate::from_percent(taux)), 250);
        }
    }

    /// Un sort qui ne peut pas critiquer prend la valeur normale, meme si le
    /// build est a 100 % de critique.
    #[test]
    fn un_sort_sans_critique_prend_la_valeur_normale() {
        assert_eq!(magnitude(250, Some(300), CritRate::NEVER), 250);
    }
}

/// One row of a spell's damage table, as a player reads it.
#[derive(Clone, Debug)]
pub struct DamageRow {
    /// How many charges of the spell's own stack are held. Zero is the plain
    /// cast.
    pub charges: u8,
    /// Which resource those charges are of, for the label.
    pub resource: Option<String>,
    /// Damage range on a normal hit, summed across every line, after the
    /// build's stats and the target's resistance.
    pub normal: (i64, i64),
    /// Same on a critical hit. `None` when the spell cannot critical.
    pub critical: Option<(i64, i64)>,
    /// The same figures split by element, in the order the lines are declared.
    /// A spell striking in four elements at once, like the Xelor's Glas, reads
    /// as a single total otherwise, and the total says nothing about which
    /// resistance is going to eat it.
    pub by_element: Vec<ElementRow>,
}

/// Ce qu'un état frappe à chaque coup, sur ce build. Voir
/// [`Engine::state_damage_table`].
#[derive(Clone, Debug)]
pub struct StateDamageRow {
    /// L'état, par son identifiant dans les règles.
    pub state: String,
    /// La place de la détente parmi les `while_present` de l'état.
    pub effect: usize,
    /// Le taux de critique de ces coups, en pour mille.
    pub crit_permille: i32,
    pub normal: (i64, i64),
    pub critical: Option<(i64, i64)>,
    pub by_element: Vec<ElementRow>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementRow {
    pub element: Element,
    pub normal: (i64, i64),
    pub critical: Option<(i64, i64)>,
}

impl Engine {
    /// Les conditions d'un lancer qui ne tiennent qu'aux compteurs : ni le
    /// coût, ni les limites de lancers, ni les délais.
    #[inline]
    fn conditions_tiennent(&self, spell: &Spell, res: &[u8; MAX_RESOURCES]) -> bool {
        if let Some(exigence) = spell.requires {
            if !exigence.tient(res) {
                return false;
            }
        }
        // La condition de lancement de la donnée, sur les états du lanceur.
        if let Some(critere) = &spell.critere {
            if !critere.tient(res) {
                return false;
            }
        }
        // L'inverse : interdit tant que la ressource est la. Le Feca qui
        // vient de poser un glyphe elementaire porte l'Hypoglyphe, et les
        // sept autres glyphes lui sont fermes jusqu'a la fin du tour.
        if let Some(r) = spell.blocked_by {
            if res[r] > 0 {
                return false;
            }
        }
        // Plus de cible pour ce mode : tous les ennemis portent déjà ce
        // que le sort pose.
        if let Some(r) = spell.bloque_au_plafond {
            if res[r] >= self.resources[r].max {
                return false;
            }
        }
        if let Some((a, b)) = spell.exige_surplus {
            if res[a] <= res[b] {
                return false;
            }
        }
        true
    }

    /// Ce sort se lance-t-il, ces compteurs valant ce qu'on donne et les autres
    /// leur valeur de départ ?
    ///
    /// Les mêmes conditions que la recherche, par la même fonction : exigence,
    /// condition de lancement de la donnée, blocages. Ni le coût, ni les
    /// limites de lancers, ni les délais, qui dépendent du tour. Un sort à
    /// modes se lance si l'un d'eux le peut. `None` pour un sort hors du deck
    /// ou un compteur inconnu.
    pub fn lancable(&self, spell_id: &str, compteurs: &[(&str, u8)]) -> Option<bool> {
        let mut res = [0u8; MAX_RESOURCES];
        for (i, r) in self.resources.iter().enumerate() {
            res[i] = r.default;
        }
        for (nom, v) in compteurs {
            let i = self.resources.iter().position(|r| r.label == *nom)?;
            res[i] = *v;
        }
        let mut modes = self.spells.iter().filter(|s| s.id == spell_id).peekable();
        modes.peek()?;
        Some(modes.any(|s| self.conditions_tiennent(s, &res)))
    }

    /// Les sorts que le moteur a reellement compiles, dans l'ordre du deck.
    pub fn spell_ids(&self) -> Vec<&str> {
        // Un sort à modes est compilé une fois par mode, sous le même
        // identifiant : il ne se nomme qu'une fois.
        let mut ids: Vec<&str> = Vec::new();
        for s in &self.spells {
            if !ids.contains(&s.id.as_str()) {
                ids.push(s.id.as_str());
            }
        }
        ids
    }

    /// Les fourchettes de ces lignes sur ce build, normale et critique, et
    /// leur partage par élément : le calcul d'une ligne de
    /// [`Engine::damage_table`], sorti pour servir aussi aux coups des états.
    fn fourchettes(
        &self,
        lines: &[Line],
        peut_crit: bool,
        mult: FinalMultiplier,
        res: &[u8; MAX_RESOURCES],
    ) -> ((i64, i64), Option<(i64, i64)>, Vec<ElementRow>) {
        let mut peut_crit = peut_crit;
        // Chaque ligne active : son élément, son tirage, ses fourchettes
        // normale et critique.
        let mut coups: Vec<(Element, Option<(usize, usize)>, Option<(i64, i64)>, Option<(i64, i64)>)> = Vec::new();
        let gagnants = paliers_gagnants(lines, res);
        for line in lines {
            // Meme regle que dans le solveur, pour les deux.
            if !ligne_active(line, res, &gagnants) {
                continue;
            }
            // Une ligne qui se repete par point de ressource ne frappe
            // pas quand il n'y en a aucun, et frappe N fois sinon.
            let repetitions = repetitions(line, res);
            if repetitions == 0 {
                continue;
            }
            let echelonne: i32 = line
                .per_resource
                .iter()
                .map(|(r, a)| a * i32::from(res[*r]))
                .sum::<i32>()
                + line
                    .steps
                    .iter()
                    .map(|(r, paliers)| {
                        (0..usize::from(res[*r]))
                            .map(|i| paliers[i.min(paliers.len().saturating_sub(1))])
                            .sum::<i32>()
                    })
                    .sum::<i32>();
            // Un palier « tant que l'etat est la » ne se cumule pas :
            // present ou absent, une seule fois, quel que soit le
            // nombre de crans.
            let bonus: i32 = echelonne
                + line
                    .while_resource
                    .iter()
                    .filter(|(r, _)| res[*r] > 0)
                    .map(|(_, a)| *a)
                    .sum::<i32>();
            let transmis = line.invocation.map(|f| profil_transmis(&self.build.profile, f));
            let profil = transmis.as_ref().unwrap_or(&self.build.profile);
            let propre = dommages_propres(line, profil);
            let profil = propre.as_ref().unwrap_or(profil);
            let base = if line.invocation.is_some() { FinalMultiplier::NEUTRAL } else { mult };
            let mult = line.facteur.map_or(base, |f| base.times(f));
            let normale = line.normal.map(|(lo, hi)| {
                let l = SpellLine {
                    element: line.element,
                    normal: (lo + bonus, hi + bonus),
                    critical: (lo + bonus, hi + bonus),
                };
                let (a, b) = dofus_damage::range(&l, profil, mult, false, &self.scenario.resistance);
                (a * repetitions, b * repetitions)
            });
            // Une ligne sans critique vaut, sur un coup critique, son jet normal.
            let critique = if line.sans_critique { line.normal } else { line.critical };
            let critique = match critique {
                Some((lo, hi)) if peut_crit => {
                    let l = SpellLine {
                        element: line.element,
                        normal: (lo + bonus, hi + bonus),
                        critical: (lo + bonus, hi + bonus),
                    };
                    let (a, b) = dofus_damage::range(
                        &l,
                        profil,
                        mult,
                        !line.sans_critique,
                        &self.scenario.resistance,
                    );
                    Some((a * repetitions, b * repetitions))
                }
                _ => {
                    peut_crit = false;
                    None
                }
            };
            coups.push((line.element, line.tirage.map(|t| (t.groupe, t.issue)), normale, critique));
        }
        // Les issues d'un tirage ne s'additionnent pas : le sort n'en joue
        // qu'une. Les lignes d'une même issue, elles, partent ensemble et
        // s'additionnent ; le tirage prend l'étendue de ses issues, de la plus
        // basse à la plus haute. Chaque élément de même, sur les issues qui le
        // portent.
        let ajouter = |a: Option<(i64, i64)>, b: Option<(i64, i64)>| match (a, b) {
            (Some(a), Some(b)) => Some((a.0 + b.0, a.1 + b.1)),
            (a, b) => a.or(b),
        };
        let etendre = |a: Option<(i64, i64)>, b: Option<(i64, i64)>| match (a, b) {
            (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.max(b.1))),
            (a, b) => a.or(b),
        };
        type Fourchettes = (Option<(i64, i64)>, Option<(i64, i64)>);
        // Réunit des entrées par clé, en gardant l'ordre de première vue.
        fn reunir<K: PartialEq + Copy>(
            entrees: impl Iterator<Item = (K, Fourchettes)>,
            f: impl Fn(Option<(i64, i64)>, Option<(i64, i64)>) -> Option<(i64, i64)>,
        ) -> Vec<(K, Fourchettes)> {
            let mut out: Vec<(K, Fourchettes)> = Vec::new();
            for (k, (n, c)) in entrees {
                match out.iter_mut().find(|(x, _)| *x == k) {
                    Some((_, (xn, xc))) => {
                        *xn = f(*xn, n);
                        *xc = f(*xc, c);
                    }
                    None => out.push((k, (n, c))),
                }
            }
            out
        }
        // Chaque issue additionne ses lignes ; les lignes hors tirage
        // s'additionnent entre elles, par élément.
        let par_issue = reunir(coups.iter().map(|&(e, t, n, c)| ((e, t), (n, c))), ajouter);
        // Chaque tirage prend, élément par élément puis en tout, l'étendue de
        // ses issues.
        let par_tirage = reunir(
            par_issue.iter().filter_map(|((e, t), nc)| t.map(|(g, i)| ((*e, g, i), *nc))),
            ajouter,
        );
        let etendue_par_element = reunir(par_tirage.iter().map(|((e, g, _), nc)| ((*e, *g), *nc)), etendre);
        let par_issue_en_tout = reunir(par_tirage.iter().map(|((_, g, i), nc)| ((*g, *i), *nc)), ajouter);
        let etendue = reunir(par_issue_en_tout.iter().map(|((g, _), nc)| (*g, *nc)), etendre);
        let mut normal = (0i64, 0i64);
        let mut critical = (0i64, 0i64);
        let hors_tirage = par_issue.iter().filter(|((_, t), _)| t.is_none()).map(|(_, nc)| *nc);
        for (n, c) in hors_tirage.chain(etendue.iter().map(|(_, nc)| *nc)) {
            if let Some((a, b)) = n {
                normal = (normal.0 + a, normal.1 + b);
            }
            if let Some((a, b)) = c {
                critical = (critical.0 + a, critical.1 + b);
            }
        }
        // Par élément, dans l'ordre des lignes : chaque tirage une fois, à son
        // étendue. Une zone qui répète produit deux lignes du même élément :
        // elles se cumulent sur une seule entrée, sinon l'infobulle afficherait
        // deux fois « Feu ».
        let mut vus: Vec<(Element, usize)> = Vec::new();
        let mut entrees: Vec<(Element, Fourchettes)> = Vec::new();
        for ((e, t), nc) in &par_issue {
            match t {
                None => entrees.push((*e, *nc)),
                Some((g, _)) if !vus.contains(&(*e, *g)) => {
                    vus.push((*e, *g));
                    if let Some((_, x)) = etendue_par_element.iter().find(|(k, _)| *k == (*e, *g)) {
                        entrees.push((*e, *x));
                    }
                }
                Some(_) => {}
            }
        }
        let mut par_element: Vec<ElementRow> = Vec::new();
        for (element, (n, c)) in entrees {
            if let Some((a, b)) = n {
                match par_element.iter_mut().find(|e| e.element == element) {
                    Some(e) => {
                        e.normal.0 += a;
                        e.normal.1 += b;
                    }
                    None => par_element.push(ElementRow {
                        element,
                        normal: (a, b),
                        critical: None,
                    }),
                }
            }
            if let Some((a, b)) = c {
                if let Some(e) = par_element.iter_mut().find(|e| e.element == element) {
                    match &mut e.critical {
                        Some(x) => {
                            x.0 += a;
                            x.1 += b;
                        }
                        None => e.critical = Some((a, b)),
                    }
                }
            }
        }
        if !peut_crit {
            for e in &mut par_element {
                e.critical = None;
            }
        }
        (normal, peut_crit.then_some(critical), par_element)
    }

    /// Les coups des états que ce sort pose, détente par détente, au palier de
    /// base : l'état tout juste posé, à un cran. Un poison, un glyphe, une
    /// Sentence : le sort qui les pose n'a souvent aucune ligne à lui, et sa
    /// [`Engine::damage_table`] est vide.
    ///
    /// Le multiplicateur est celui qu'`Engine::new` donne aux détentes : les
    /// modificateurs des tours impairs, et le contexte d'un lancer à la
    /// position habituelle du joueur, un état n'ayant pas de portée à lui. Un
    /// test vérifie, sur les dix-neuf classes, que le montant que la rotation
    /// compte à chaque coup tombe au milieu de la fourchette affichée.
    pub fn state_damage_table(&self, spell_id: &str) -> Vec<StateDamageRow> {
        let percents: Vec<u32> = self
            .build
            .modifiers
            .iter()
            .filter(|m| m.when != When::EvenTurns)
            .map(|m| m.percent)
            .collect();
        let mult = FinalMultiplier::from_percents(&percents).and(FinalMultiplier::for_cast(
            &self.build.profile,
            &self.scenario.resistance,
            CastContext {
                delivery: Delivery::Spell,
                reach: self.scenario.reach,
            },
        ));
        self.state_triggers
            .iter()
            .filter(|t| t.source == spell_id)
            .map(|t| {
                let mut res = [0u8; MAX_RESOURCES];
                res[t.state] = 1;
                let (normal, critical, by_element) =
                    self.fourchettes(&t.lines, t.crit_rate != CritRate::NEVER, mult, &res);
                StateDamageRow {
                    state: self.resources[t.state].label.clone(),
                    effect: t.effect,
                    crit_permille: t.crit_rate.permille(),
                    normal,
                    critical,
                    by_element,
                }
            })
            .collect()
    }

    /// What each spell actually hits for on this build, charge by charge: the
    /// computed damage after characteristics, damage bonuses, multipliers and
    /// resistance, normal and critical side by side. Charges are enumerated over
    /// the spell's own stack, the resource its lines read; a spell without one
    /// gets a single row.
    pub fn damage_table(&self, spell_id: &str) -> Vec<DamageRow> {
        let Some(spell) = self.spells.iter().find(|s| s.id == spell_id) else {
            return Vec::new();
        };
        // A spell with no damage line of its own gets no table. The Xelor's
        // Aiguille is the case: it carries `lines: []` and hits through the
        // state it applies, so a table would read "Dégâts 0 - 0" where the
        // honest answer is that the damage is not the spell's own.
        if !spell.lines.iter().any(|l| l.normal.is_some()) {
            return Vec::new();
        }

        // The resource this spell's own damage reads. Only its own stack is
        // enumerated; a build-wide buff belongs to the build, not to the row.
        // `while_resource` compte au même titre que les deux autres : le
        // Carnavalo du Zobal a deux lignes, 24-28 « Lanceur sans
        // Psychopathe » et 39-43 « Lanceur sous Psychopathe ».
        let propre = spell
            .lines
            .iter()
            .flat_map(|l| {
                l.per_resource
                    .iter()
                    .map(|(r, _)| *r)
                    .chain(l.steps.iter().map(|(r, _)| *r))
                    .chain(l.while_resource.iter().map(|(r, _)| *r))
                    .chain(l.repeats_per.iter().copied())
                    .chain(l.plafond_cibles.iter().copied())
                    .chain(l.active_at.iter().flat_map(|(rs, _)| rs.iter().copied()))
            })
            .next();

        let odd = self.turn_is_odd(0);
        let context = FinalMultiplier::for_cast(
            &self.build.profile,
            &self.scenario.resistance,
            CastContext {
                delivery: if spell.arme { Delivery::Weapon } else { Delivery::Spell },
                reach: reach_for(spell.range, self.scenario.reach),
            },
        );
        let percents: Vec<u32> = self
            .build
            .modifiers
            .iter()
            .filter(|m| m.when != When::EvenTurns)
            .map(|m| m.percent)
            .collect();
        let mult = FinalMultiplier::from_percents(&percents).and(context);

        let max_charges = propre.map_or(0, |r| self.resources[r].max);
        (0..=max_charges)
            .map(|charges| {
                let mut res = [0u8; MAX_RESOURCES];
                if let Some(r) = propre {
                    res[r] = charges;
                }
                let (normal, critical, by_element) =
                    self.fourchettes(&spell.lines, spell.crit_rate != CritRate::NEVER, mult, &res);
                let _ = odd;
                DamageRow {
                    charges,
                    resource: None,
                    normal,
                    critical,
                    by_element,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod coups_d_etat {
    //! Les fourchettes d'un coup d'état, que KrozZone affiche, et le montant
    //! que la rotation compte à chaque coup sortent du même calcul.
    use super::*;
    use dofus_damage::ElementStats;
    use dofus_ruleset::{snapshot::Snapshot, Effect as Eff, Ruleset};

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

    /// Sans critique, le montant qu'une détente vaut dans la rotation tombe au
    /// milieu de la fourchette que la table donne, à un point près d'arrondi,
    /// pour chaque sort des dix-neuf classes qui pose un état qui frappe. Le
    /// build porte des dommages aux sorts, à distance et un multiplicateur :
    /// un chemin qui en oublierait un s'écarterait de plusieurs points.
    #[test]
    fn le_coup_affiche_est_celui_que_la_rotation_compte() {
        let racine = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let mut vus = 0;
        for (classe, breed) in CLASSES {
            let mut rs = Ruleset::load(format!("{racine}/data/rulesets/{classe}.yaml"))
                .unwrap_or_else(|e| panic!("{e}"));
            let snap =
                Snapshot::load(format!("{racine}/data/snapshots/breed-{breed}.json")).unwrap();
            rs.merge_snapshot(&snap);
            let frappe = |id: &str| {
                rs.resources.iter().any(|r| r.id == id && !r.while_present.is_empty())
            };
            let deck: Vec<String> = rs
                .spells
                .iter()
                .filter(|s| {
                    s.effects
                        .iter()
                        .any(|e| matches!(e, Eff::Gain { resource, .. } if frappe(resource)))
                })
                .map(|s| s.id.clone())
                .collect();
            if deck.is_empty() {
                continue;
            }
            let build = Build {
                name: "banc".into(),
                profile: DamageProfile {
                    power: 200,
                    percent_spell: 15,
                    percent_ranged: 20,
                    elements: [ElementStats {
                        characteristic: 700,
                        flat_damage: 90,
                    }; 5],
                    ..Default::default()
                },
                base_ap: 12,
                base_mp: 3,
                crit_bonus_percent: -200,
                modifiers: vec![BuildModifier {
                    id: "banc".into(),
                    percent: 110,
                    when: When::Always,
                }],
                deck: deck.clone(),
            };
            let scenario = Scenario {
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
            let moteur =
                Engine::new(&rs, build, scenario).unwrap_or_else(|e| panic!("{classe} : {e}"));
            for id in &deck {
                let table = moteur.state_damage_table(id);
                assert!(!table.is_empty(), "{classe} : {id} pose un état qui frappe, sans coup");
                for row in table {
                    let t = moteur
                        .state_triggers
                        .iter()
                        .find(|t| {
                            t.source == *id
                                && t.effect == row.effect
                                && moteur.resources[t.state].label == row.state
                        })
                        .expect("la ligne vient d'une détente");
                    let k = t.palier.map_or(0, |r| usize::from(r == t.state));
                    let compte = t.damage_odd[k.min(t.damage_odd.len() - 1)].as_f64();
                    let milieu = (row.normal.0 + row.normal.1) as f64 / 2.0;
                    assert!(
                        row.normal.0 > 0 && (compte - milieu).abs() <= 1.0,
                        "{classe} : {id}, {} n°{} : la rotation compte {compte}, \
                         la table donne {:?}",
                        row.state,
                        row.effect,
                        row.normal
                    );
                    vus += 1;
                }
            }
        }
        assert!(vus >= 39, "{vus} détentes seulement");
    }
}
