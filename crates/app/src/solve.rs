//! Everything between a request and a rotation, shared by the command line and
//! the local web front end.

use std::collections::BTreeMap;

use dofus_build::{resolve_with_class_spells, BuildInput, Catalogue, Resolved};
use dofus_damage::Resistance;
use dofus_engine::{
    Build, BuildModifier, Case, Engine, Mode, Placement, Reach, Scenario, Solution, When,
};
use dofus_ruleset::{snapshot::Snapshot, Ruleset};
use serde::Deserialize;

/// Les 19 classes, par identifiant de classe : en ajouter une, c'est une ligne
/// ici et un fichier YAML.
pub const CLASSES: &[(u32, &str, &str)] = &[
    (1, "feca", "Féca"),
    (2, "osamodas", "Osamodas"),
    (3, "enutrof", "Enutrof"),
    (4, "sram", "Sram"),
    (5, "xelor", "Xélor"),
    (6, "ecaflip", "Ecaflip"),
    (7, "eniripsa", "Eniripsa"),
    (8, "iop", "Iop"),
    (9, "cra", "Crâ"),
    (10, "sadida", "Sadida"),
    (11, "sacrieur", "Sacrieur"),
    (12, "pandawa", "Pandawa"),
    (13, "roublard", "Roublard"),
    (14, "zobal", "Zobal"),
    (15, "steamer", "Steamer"),
    (16, "eliotrope", "Eliotrope"),
    (17, "huppermage", "Huppermage"),
    (18, "ouginak", "Ouginak"),
    (20, "forgelance", "Forgelance"),
];

/// Le placement tel que la page l'envoie : des paires de coordonnées dans le
/// repère de `dofus-grid`, où les quatre directions du jeu font varier l'un des
/// deux entiers de un, et où la distance est la somme des écarts absolus.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct PlacementInput {
    pub lanceur: (i16, i16),
    pub visee: (i16, i16),
    pub ennemis: Vec<(i16, i16)>,
    /// La carte de boss où tout ce monde se tient, par son identifiant de relevé ;
    /// absente, le damier vide. Seul l'aperçu des zones la lit : ses murs coupent la
    /// vue, et son sol borne les cases où viser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carte: Option<u16>,
    /// Le côté du damier vide, sans carte : quinze cases par défaut, vingt au
    /// plus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damier: Option<u8>,
}

impl PlacementInput {
    /// Le plafond s'applique ici aussi, pas seulement dans la page : un appelant qui
    /// poste directement cinquante ennemis obtiendrait un total que le champ
    /// « Ennemis touchés » n'aurait jamais laissé saisir. Les ennemis en trop sont
    /// les derniers posés, ce que la page annonce avant de les refuser.
    pub fn en_placement(&self) -> Placement {
        Placement {
            lanceur: Case::new(self.lanceur.0, self.lanceur.1),
            visee: Case::new(self.visee.0, self.visee.1),
            ennemis: self
                .ennemis
                .iter()
                .take(usize::from(MAX_ENNEMIS))
                .map(|&(x, y)| Case::new(x, y))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Request {
    #[serde(flatten)]
    pub build: BuildInput,
    /// Spell ids from the ruleset. DofusBook stores no spell selection, so this
    /// cannot be imported and has to be chosen.
    pub deck: Vec<String>,
    /// Boosts that only apply on odd turns. No source carries this: DofusBook
    /// gives a Dofus bonus's magnitude, never its condition.
    #[serde(default)]
    pub odd_turns_only: Vec<String>,
    /// Les bonus de Dofus conditionnels que le joueur écarte du calcul, par
    /// leur nom : le Rouge Vermeil du Vulbis s'il est tapé entre deux tours.
    #[serde(default)]
    pub bonus_ecartes: Vec<String>,
    /// Ceux que le joueur ajoute, leur case décochée d'office : l'Œil du
    /// Cauchemar, quand un ennemi le désenvoûte ou l'entrave.
    #[serde(default)]
    pub bonus_inclus: Vec<String>,
    /// Le mur de bombes du Roublard que Plombage redéclenche : l'élément de
    /// ses bombes et leurs combos. Absent, deux Explobombes tout juste posées.
    #[serde(default)]
    pub mur_de_bombes: Option<crate::bombes::MurDeBombes>,
    /// Les mécaniques coûteuses à compter, par leur identifiant dans le bloc
    /// `costly` de la classe. Absente, elles comptent toutes ; une liste les
    /// restreint, ce qui permet aux tests de mesurer ce que chacune rapporte.
    #[serde(default)]
    pub mecaniques: Option<Vec<String>>,
    /// Chercher la rotation exhaustivement avec les mécaniques cochées, au lieu des
    /// deux passes. Faux par défaut : avec les runes du Huppermage, la recherche
    /// exhaustive dépasse les cinq minutes, quand les deux passes rendent en quelques
    /// secondes l'essentiel de ce que les mécaniques valent.
    #[serde(default)]
    pub exhaustif: bool,
    #[serde(default = "default_horizon")]
    pub horizon: u8,
    /// Combien d'ennemis le joueur suppose toucher. Absent vaut un.
    #[serde(default)]
    pub targets: Option<u8>,
    /// Combien de PM le joueur compte dépenser dans le tour, zéro par défaut. Une
    /// déclaration, pas une simulation : le solveur ne sait pas où se tiennent les
    /// entités.
    #[serde(default)]
    pub pm_depenses: Option<u8>,
    /// Les poussées de la rotation butent contre un obstacle : chaque sort qui
    /// repousse ajoute ses dommages de poussée. Déclaration du joueur.
    #[serde(default)]
    pub poussees_bloquees: bool,
    /// Combien de cases séparent deux ennemis touchés par une même zone. Absent ou
    /// nul : tout le monde est collé à l'impact, et le total est un plafond.
    #[serde(default)]
    pub etalement: Option<u8>,
    /// Où se tiennent le lanceur et les ennemis, quand le joueur les a posés sur la
    /// grille. Présent, il l'emporte sur `targets` et `etalement` pour toute ligne
    /// dont la zone se dessine ; les autres retombent sur les réglages déclarés, et
    /// la réponse les nomme.
    #[serde(default)]
    pub placement: Option<PlacementInput>,
    /// Les compteurs que le joueur renseigne lui-même, par identifiant : toute
    /// ressource du ruleset marquée `declared_by_player`, que la page affiche en
    /// champs.
    #[serde(default)]
    pub etats_declares: BTreeMap<String, u8>,
    /// Jusqu'à combien d'ennemis balayer, pour tracer la courbe ; absent, aucun
    /// balayage. Chaque valeur demande une résolution complète : la rotation optimale
    /// à deux ennemis n'est pas celle à un.
    #[serde(default)]
    pub targets_sweep: Option<u8>,
    #[serde(default)]
    pub budgets: BTreeMap<String, u8>,
    #[serde(default)]
    pub resistance_percent: Option<[i32; 5]>,
    #[serde(default)]
    pub resistance_flat: Option<[i32; 5]>,
    /// Target's critical damage resistance. Push resistance has no place here:
    /// push damage is not modelled at all, so declaring a resistance to it
    /// would suggest a precision that does not exist.
    #[serde(default)]
    pub resistance_critical: Option<i32>,
    /// Where the player fights, for the spells whose own range does not settle
    /// it. Only matters on a build carrying `% Dommages mêlée` or `% Dommages
    /// distance`; on any other build it changes nothing at all.
    #[serde(default)]
    pub reach: Option<String>,
    /// Target-side domain resistances, the mirror of the attacker's four.
    #[serde(default)]
    pub resistance_spell: Option<i32>,
    #[serde(default)]
    pub resistance_weapon: Option<i32>,
    #[serde(default)]
    pub resistance_melee: Option<i32>,
    #[serde(default)]
    pub resistance_ranged: Option<i32>,
    /// La Maîtrise d'arme, en Puissance sur les coups d'arme : 0, 300 (la
    /// normale, par défaut) ou 360 (la critique).
    #[serde(default)]
    pub maitrise_d_arme: Option<u16>,
}

fn default_horizon() -> u8 {
    7
}

pub fn ruleset_for(class: u32) -> Option<(&'static str, u32)> {
    CLASSES
        .iter()
        .find(|(id, _, _)| *id == class)
        .map(|(id, name, _)| (*name, *id))
}

/// A class's name in French, read from its embedded snapshot rather than from a
/// hard-coded table that would have to be kept in step. `None` only when no
/// snapshot exists for the id.
pub fn class_name(class: u32) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(crate::donnees::instantane(class)?).ok()?;
    value["breed"]["name"].as_str().map(str::to_string)
}

/// The classes a rotation can actually be computed for, by name.
pub fn supported_class_names() -> Vec<String> {
    CLASSES
        .iter()
        .map(|(_, _, label)| label.to_string())
        .collect()
}

/// Why this class cannot be solved, phrased for the person reading it: a
/// consequence, not an internal id.
pub fn unsupported_class_message(class: u32) -> String {
    let nom = class_name(class).unwrap_or_else(|| format!("la classe {class}"));
    let dispo = supported_class_names();
    let liste = match dispo.len() {
        0 => "aucune pour le moment".to_string(),
        1 => dispo[0].clone(),
        _ => format!(
            "{} et {}",
            dispo[..dispo.len() - 1].join(", "),
            dispo[dispo.len() - 1]
        ),
    };
    format!(
        "{nom} n'est pas encore modélisé. Les classes disponibles aujourd'hui sont : {liste}. \
         Votre équipement a bien été lu, il n'y a que les sorts qui manquent."
    )
}

/// L'instantané du jeu pour cette classe.
pub fn snapshot_for(class: u32) -> Result<Snapshot, String> {
    let (_, breed) = ruleset_for(class).ok_or_else(|| unsupported_class_message(class))?;
    Snapshot::from_json(crate::donnees::instantane(breed).ok_or_else(|| unsupported_class_message(class))?)
}

pub fn load_ruleset(class: u32) -> Result<Ruleset, String> {
    let (name, _) = ruleset_for(class).ok_or_else(|| unsupported_class_message(class))?;
    let mut ruleset =
        Ruleset::from_yaml(crate::donnees::regles(name).ok_or_else(|| unsupported_class_message(class))?)
            .map_err(|e| e.to_string())?;
    let snapshot = snapshot_for(class)?;
    let merge = ruleset.merge_snapshot(&snapshot);
    if !merge.conflicts.is_empty() {
        return Err(format!(
            "mécaniques et instantané en désaccord : {:#?}",
            merge.conflicts
        ));
    }
    Ok(ruleset)
}

/// Les sorts communs qui invoquent (`data/communs.yaml`), au deck de chaque
/// classe : l'Arakne et le Chaferfu, et leurs variantes.
pub fn sorts_communs() -> &'static [dofus_ruleset::SpellDef] {
    static COMMUNS: std::sync::OnceLock<Vec<dofus_ruleset::SpellDef>> = std::sync::OnceLock::new();
    COMMUNS.get_or_init(|| {
        Ruleset::from_yaml(crate::donnees::COMMUNS).expect("les sorts communs doivent se lire").spells
    })
}

/// Les paires de variantes des sorts communs, une seule de chaque au deck
/// (`spell-variants` 491 et 492).
const VARIANTES_COMMUNES: [(u32, u32); 4] = [(370, 491), (24027, 491), (373, 492), (24028, 492)];

pub fn resolve_build(input: &BuildInput) -> Result<Resolved, String> {
    let catalogue = Catalogue::from_json(crate::donnees::OBJETS)?;
    // Le ruleset dit quels bonus de classe le solveur sait produire. Sans lui,
    // la resolution les ecarte TOUS en promettant que le solveur s'en chargera,
    // et ceux dont la mecanique manque disparaissent des deux cotes.
    let ruleset = load_ruleset(input.class).ok();
    let modelles = ruleset.as_ref().map(bonus_modelises);
    // Un bonus de classe sans son sort se rattache par son nom (l'import de
    // l'application de bureau perd `effectId`) : sinon ses chiffres
    // compteraient en permanence, quand la rotation doit proposer le sort.
    let mut input = input.clone();
    if let Some(rs) = &ruleset {
        for boost in &mut input.boosts {
            if boost.class_id.is_none() || boost.spell_id().is_some() {
                continue;
            }
            let meme = |a: &str, b: &str| a.trim().to_lowercase() == b.trim().to_lowercase();
            if let Some(id) = rs
                .spells
                .iter()
                .find(|s| meme(&s.name.fr, &boost.name))
                .and_then(|s| s.dofusdb_id)
            {
                boost.effect_id = Some(id.to_string());
            }
        }
    }
    let mut resolu = resolve_with_class_spells(&input, &catalogue, modelles.as_ref());
    // Les effets d'objet qui dépendent du combat ont leur case, comme les
    // bonus de Dofus.
    resolu.bonus_conditionnels.extend(crate::effets_d_objets::bonus_conditionnels(&input.normalise()));
    Ok(resolu)
}

/// Les couples (sort, statistique) qu'un bonus de classe n'a pas besoin de
/// porter, le solveur les produisant lui-même en lançant le sort.
///
/// DofusBook nomme `pu` la Puissance, `deg` un multiplicateur final et `dmg` des
/// dommages plats, les trois formes qu'un état peut prendre ici. Un sort n'est
/// retenu que pour les statistiques qu'il produit vraiment : le Prélude au Fer,
/// sans Puissance au fichier du Forgelance, n'est dans aucun couple, et son bonus
/// se compte.
fn bonus_modelises(ruleset: &dofus_ruleset::Ruleset) -> std::collections::BTreeSet<(u32, String)> {
    use dofus_ruleset::{DamageModifier as D, Effect};
    let mut out = std::collections::BTreeSet::new();
    for spell in &ruleset.spells {
        let Some(id) = spell.dofusdb_id else { continue };
        for effect in &spell.effects {
            let Effect::Gain { resource, .. } = effect else {
                continue;
            };
            let Some(res) = ruleset.resource(resource) else {
                continue;
            };
            for m in &res.modifies_damage {
                match m {
                    // La Puissance ne connait pas d'element ; avec un element
                    // c'est un vol de caracteristique, que DofusBook n'offre
                    // pas en bonus.
                    D::Characteristic { element: None, .. } => {
                        out.insert((id, "pu".to_string()));
                    }
                    D::FinalMultiplier { .. } => {
                        out.insert((id, "deg".to_string()));
                    }
                    D::FlatDamage { .. } => {
                        out.insert((id, "dmg".to_string()));
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

pub fn to_engine_build(request: &Request, resolved: &Resolved) -> Build {
    let ecarte = |n: &String| request.bonus_ecartes.contains(n);
    // La Puissance du Sylvestre et du Cauchemar n'est pas ici : elle se joue
    // dans le tour, voir `avec_les_effets_de_dofus`.
    Build {
        name: format!("classe {}", request.build.class),
        profile: resolved.profile,
        // Les PA d'un effet de Dofus, le Jaune Ocre, s'ajoutent ici : la fiche
        // les tient à part pour rester comparable à celle de DofusBook. Un
        // bonus que le joueur a écarté dans l'onglet Rotation ne compte pas.
        base_ap: resolved
            .pa_des_dofus
            .iter()
            .filter(|(n, _)| !request.bonus_ecartes.contains(n))
            .fold(resolved.base_ap, |pa, (_, n)| pa.saturating_add(*n)),
        // Les PM des Dofus, et ceux d'un bonus écarté qui en donne à la place :
        // l'Abyssal sans ennemi au contact.
        base_mp: resolved
            .pm_des_dofus
            .iter()
            .filter(|(n, _)| !ecarte(n))
            .chain(resolved.pm_si_ecarte.iter().filter(|(n, _)| ecarte(n)))
            .fold(resolved.base_mp, |pm, (_, n)| pm.saturating_add(*n)),
        crit_bonus_percent: resolved.crit_bonus_percent,
        modifiers: resolved
            .damage_multipliers
            .iter()
            .filter(|(name, _)| !request.bonus_ecartes.contains(name))
            .map(|(name, percent)| BuildModifier {
                id: name.clone(),
                percent: *percent,
                when: match resolved.tours.get(name) {
                    Some(dofus_build::Tours::Impairs) => When::OddTurns,
                    Some(dofus_build::Tours::Pairs) => When::EvenTurns,
                    None if request.odd_turns_only.iter().any(|o| o == name) => When::OddTurns,
                    None => When::Always,
                },
            })
            .collect(),
        deck: request.deck.clone(),
    }
}

/// Combien d'ennemis un scénario admet au plus, déclarés ou posés. Le nombre vit
/// ici seul, et l'interface le lit au lieu de le recopier. Trente : un cercle de
/// rayon 6 couvre quatre-vingt-cinq cases.
pub const MAX_ENNEMIS: u8 = 30;

pub struct Outcome {
    pub resolved: Resolved,
    pub solution: Solution,
    /// Les sorts que le moteur a sortis du deck, avec leur raison : un sort écarté
    /// ne disparaît pas de la rotation sans un mot.
    pub ecartes: Vec<(String, String)>,
    /// Les remarques du calcul lui-même (voir `preparer`), que l'onglet Rotation
    /// affiche ; les hypothèses de l'équipement restent à l'onglet Équipement.
    pub remarques: Vec<String>,
    /// Les sorts de la rotation dont la zone n'a pas de capacité connue : leur forme
    /// n'est pas décodée, et ils suivent le nombre d'ennemis annoncé sans plafond.
    /// Vide en duel. Forcer une cible serait faux à coup sûr sur un sort « en
    /// zone » : le joueur qui lit un total de zone doit le savoir.
    pub zones_sans_plafond: Vec<String>,
    /// Les sorts joues dont les degats de zone decroissent avec la distance,
    /// ce que ce calculateur n'applique pas. Vide en duel.
    pub zones_degressives: Vec<String>,
    /// Les sorts dont la zone ne se dessine pas, laissés au régime déclaré : un
    /// joueur qui a posé ses ennemis doit savoir lesquels l'ignorent.
    pub placement_ignore: Vec<String>,
    /// La rotation qui se répète, indépendante de l'horizon demandé.
    pub steady: dofus_engine::SteadyState,
    pub seconds: f64,
}

/// Ce que rend une résolution interrompue : la page le compare au message, un
/// arrêt demandé n'étant pas une panne.
pub const ANNULE: &str = "calcul annulé";

/// Demander l'arrêt de la résolution en cours.
///
/// Sans effet s'il n'y en a pas : chaque résolution oublie les arrêts demandés
/// avant elle en démarrant.
pub fn demander_arret() {
    dofus_engine::ARRET.demander();
}

/// Les sorts dont les dégâts lisent une ressource des mécaniques nommées : ils
/// valent zéro mécanique éteinte, donc la première passe ne les joue jamais, et
/// il faut les rendre au deck de la seconde. Les sorts qui alimentent la
/// mécanique sans la lire n'y sont pas : les vingt-quatre sorts élémentaires du
/// Huppermage posent une rune, mais sont déjà choisis pour leurs propres dégâts.
fn sorts_nourris(ruleset: &Ruleset, mecaniques: &[String]) -> Vec<String> {
    let lues: std::collections::BTreeSet<&str> = ruleset
        .costly
        .iter()
        .filter(|c| mecaniques.iter().any(|m| m == &c.id))
        .flat_map(|c| c.resources.iter().map(String::as_str))
        .collect();
    ruleset
        .spells
        .iter()
        .flat_map(|s| s.alternatives().into_iter().map(move |a| (s.id.clone(), a)))
        .filter(|(_, a)| {
            a.lines.iter().any(|l| {
                l.repeats_per.iter().any(|r| lues.contains(r.as_str()))
                    || l.targets_capped_by.iter().any(|r| lues.contains(r.as_str()))
                    || l.base_bonus.iter().any(|b| {
                        use dofus_ruleset::BaseBonus as B;
                        match b {
                            B::PerResource { resource, .. }
                            | B::WhileResource { resource, .. }
                            | B::PerExtraTargetWhile { resource, .. }
                            | B::Steps { resource, .. } => lues.contains(resource.as_str()),
                            B::PerResourceGated {
                                resource, gated_by, ..
                            } => {
                                lues.contains(resource.as_str())
                                    || lues.contains(gated_by.as_str())
                            }
                            B::PerExtraTarget { .. } | B::PerMpUsed { .. } => false,
                        }
                    })
                    || l.active_at.as_ref().is_some_and(|c| {
                        c.conditions()
                            .iter()
                            .any(|c| c.resources().iter().any(|r| lues.contains(r)))
                    })
            })
        })
        .map(|(id, _)| id)
        .chain(
            // Les sorts qui posent un compteur dont l'effet est un modificateur
            // de dégâts, et ceux dont le coût en PA le lit. Sans eux, la seconde
            // passe ne trouve rien sur le Sram : ses cinq vols relèvent la
            // caractéristique du lanceur pour tous les sorts, et la première passe,
            // qui n'en voit pas le bénéfice, ne les lance pas.
            ruleset.spells.iter().filter(|s| {
                let pose_un_modificateur = s.effects.iter().any(|e| {
                    matches!(e, dofus_ruleset::Effect::Gain { resource, .. }
                        if lues.contains(resource.as_str())
                            && ruleset
                                .resource(resource)
                                .is_some_and(|r| !r.modifies_damage.is_empty()))
                });
                let cout_en_depend = s
                    .ap_cost
                    .reduced_by
                    .as_ref()
                    .is_some_and(|c| lues.contains(c.resource.as_str()))
                    || s.ap_cost
                        .increased_by
                        .as_ref()
                        .is_some_and(|c| lues.contains(c.resource.as_str()));
                // Et ceux qui tirent des cartes au hasard : Bonne Pioche ou
                // Redistribution n'ont pas de dégâts à eux, la première passe
                // ne les lance donc jamais, et la Rekop n'aurait rien à jouer.
                let tire_au_hasard = s.effects.iter().any(|e| {
                    matches!(e, dofus_ruleset::Effect::Gain { resource, .. }
                        | dofus_ruleset::Effect::ScheduleGain { resource, .. }
                        if lues.contains(resource.as_str())
                            && ruleset.resource(resource).is_some_and(|r| !r.random_among.is_empty()))
                });
                pose_un_modificateur || cout_en_depend || tire_au_hasard
            })
            .map(|s| s.id.clone()),
        )
        .collect()
}

/// Les effets de Dofus qui se jouent dans le tour, greffés aux règles de la
/// classe le temps d'une rotation : la fiche du build ne sait porter qu'une
/// Puissance fixe, eux demandent un état.
///
/// * L'Éternel Cauchemar (Dofus du Cauchemar) : +100 Puissance pour un tour
///   quand le porteur inflige des dommages de poussée, « uniquement sur les
///   coups qui suivent la poussée ». Chaque sort qui pousse le donne, les tours
///   où les poussées butent contre un obstacle ; le coup qui pousse ne le prend
///   pas.
/// * L'Œil du Cauchemar, le même Dofus : +10 % de dommages finaux pour le
///   reste du tour, « si le porteur déclenche les 2 effets dans le même tour »,
///   la poussée et le fait d'être « désenvoûté ou entravé ». Un ennemi qui
///   entrave, le joueur le déclare par sa case ; les sorts du porteur qui
///   l'entravent (`entravants`, lus dans la donnée), la rotation les joue, et
///   l'Œil part au second des deux déclencheurs, dans un ordre ou dans l'autre.
/// * Le Garde Champêtre (Dofus Sylvestre) : +8 Puissance par PM utilisé, pour
///   deux tours, donc ceux du tour précédent compris ; deux crans au plus, la
///   limite de cumul du jeu n'étant pas connue.
///
/// Un état ne se greffe que s'il reste de la place sous le plafond de
/// compteurs du moteur ; sinon la rotation s'en passe, et le dit.
fn avec_les_effets_de_dofus(
    mut ruleset: Ruleset,
    request: &Request,
    resolved: &Resolved,
    entravants: &[u32],
) -> Result<(Ruleset, Vec<String>), String> {
    let mut notes = Vec::new();
    if request.poussees_bloquees && resolved.puissance_si_poussee > 0 {
        let declare = request.bonus_inclus.iter().any(|b| b == dofus_build::OEIL_DU_CAUCHEMAR);
        let entrave_son_lanceur = |s: &dofus_ruleset::SpellDef| s.dofusdb_id.is_some_and(|id| entravants.contains(&id));
        // Déclaré, l'Œil ne dépend plus que de la poussée : l'entrave de la
        // classe n'ajoute rien.
        let par_ses_sorts: Vec<String> =
            if declare { Vec::new() } else { ruleset.spells.iter().filter(|s| entrave_son_lanceur(s)).map(|s| s.name.fr.clone()).collect() };
        let pour_un_tour = serde_json::json!({ "turns": 0, "refresh": "on_apply", "on_expire": "reset_to_default" });
        let etat = |id: &str, effets: serde_json::Value| {
            serde_json::json!({
                "id": id, "scope": "caster", "max": 1, "default": 0, "monotone": "increasing",
                "duration": pour_un_tour, "modifies_damage": effets,
            })
        };
        let mut etats = vec![etat(
            "eternel_cauchemar",
            serde_json::json!([{ "kind": "characteristic", "amount": resolved.puissance_si_poussee }]),
        )];
        let oeil = declare || !par_ses_sorts.is_empty();
        if oeil {
            etats.push(etat(
                "oeil_du_cauchemar",
                serde_json::json!([{ "kind": "final_multiplier", "percent": 110, "finaux": true }]),
            ));
        }
        if !par_ses_sorts.is_empty() {
            etats.push(etat("bouclier_bontarien", serde_json::json!([])));
        }
        // Tout ou rien : un Œil sans l'état qu'il attend ne se déclencherait
        // jamais, ou toujours.
        if ruleset.resources.len() + etats.len() > dofus_engine::MAX_RESOURCES {
            notes.push(
                "les états du Dofus Cauchemar n'ont pas trouvé de place sous le plafond de compteurs \
                 du moteur : la rotation s'en passe"
                    .to_string(),
            );
        } else {
            for e in etats {
                ruleset.resources.push(serde_json::from_value(e).map_err(|e| e.to_string())?);
            }
            let gain = |resource: &str, si_le_lanceur_a: Option<&str>| -> Result<dofus_ruleset::Effect, String> {
                let mut g = serde_json::json!({ "effect": "gain", "resource": resource });
                if let Some(r) = si_le_lanceur_a {
                    g["requires"] = serde_json::json!({ "kind": "caster_has", "resource": r });
                }
                serde_json::from_value(g).map_err(|e| e.to_string())
            };
            for sort in ruleset.spells.iter_mut() {
                let pousse = !sort.pushes.is_empty();
                let entrave = !par_ses_sorts.is_empty() && entrave_son_lanceur(sort);
                let mut gains = Vec::new();
                if pousse {
                    gains.push(gain("eternel_cauchemar", None)?);
                }
                if entrave {
                    gains.push(gain("bouclier_bontarien", None)?);
                }
                // L'Œil part avec le second déclencheur du tour.
                match (pousse, entrave) {
                    (true, _) if declare => gains.push(gain("oeil_du_cauchemar", None)?),
                    (true, true) => gains.push(gain("oeil_du_cauchemar", None)?),
                    (true, false) if oeil => {
                        gains.push(gain("oeil_du_cauchemar", Some("bouclier_bontarien"))?)
                    }
                    (false, true) => gains.push(gain("oeil_du_cauchemar", Some("eternel_cauchemar"))?),
                    _ => {}
                }
                sort.effects.extend(gains.iter().cloned());
                for mode in &mut sort.modes {
                    mode.effects.extend(gains.iter().cloned());
                }
            }
            if !par_ses_sorts.is_empty() {
                notes.push(format!(
                    "Œil du Cauchemar : {} vous entrave (retrait de PA, de PM ou de Portée sur vous-même, \
                     d'après la donnée du jeu) ; avec une poussée dans le même tour, +10 % de dommages \
                     finaux pour le reste du tour",
                    par_ses_sorts.join(", ")
                ));
            }
        }
    }
    let mut greffer = |ruleset: &mut Ruleset, etat: serde_json::Value| -> Result<bool, String> {
        if ruleset.resources.len() >= dofus_engine::MAX_RESOURCES {
            notes.push(format!(
                "l'état `{}` n'a pas trouvé de place sous le plafond de compteurs du moteur : \
                 la rotation s'en passe",
                etat["id"]
            ));
            return Ok(false);
        }
        ruleset.resources.push(serde_json::from_value(etat).map_err(|e| e.to_string())?);
        Ok(true)
    };
    let pm = request.pm_depenses.unwrap_or(0).min(resolved.base_mp);
    if resolved.puissance_par_pm > 0 && pm > 0 {
        greffer(&mut ruleset, serde_json::json!({
            "id": "garde_champetre",
            "scope": "caster",
            "max": 2,
            "default": 0,
            "monotone": "increasing",
            "gain_per_turn": 1,
            "modifies_damage": [{ "kind": "characteristic", "amount": resolved.puissance_par_pm * i32::from(pm) }],
        }))?;
    }
    Ok((ruleset, notes))
}

/// Plombage, lancé sur une bombe, redéclenche le mur de bombes où se tient la
/// cible, deux fois par tour au plus (ses deux lancers) : un mode « sur une
/// bombe », qui partage les lancers du sort, ouvert dès que deux bombes sont
/// placées. La Rotation ne pose pas de bombes sur un plateau : le mur est celui
/// que le joueur déclare.
///
/// Le mur frappe aussi de lui-même la cible qui s'y tient : au début de chacun
/// de ses tours, à la base hors du tour du lanceur ; quand il se forme sur elle
/// (la pose qui porte les bombes à deux) et, si le joueur le déclare, quand un
/// sort la déplace dans le mur, à la base du tour du lanceur.
fn avec_le_mur_de_bombes(
    mut ruleset: Ruleset,
    request: &Request,
    niveau: u32,
    classe: u32,
) -> Result<(Ruleset, Vec<String>), String> {
    let mur = request.mur_de_bombes.clone().unwrap_or_default();
    let mode: dofus_ruleset::ModeDef = serde_json::from_value(serde_json::json!({
        "name": { "fr": "sur une bombe", "en": "on a bomb" },
        "requires": { "kind": "at_least", "scope": "caster", "resource": "bombes_en_portee", "amount": 2 },
        "lines": [mur.ligne_de_plombage(niveau)?],
        "effects": [{ "effect": "damage" }],
    }))
    .map_err(|e| e.to_string())?;
    let plombage = ruleset
        .spells
        .iter_mut()
        .find(|s| s.id == "plombage")
        .ok_or("le Roublard n'a plus de Plombage dans ses règles")?;
    plombage.mode_name = Some(dofus_ruleset::Localised { fr: "sur un ennemi".into(), en: "on an enemy".into() });
    plombage.modes.push(mode);

    // Les sorts qui déplacent l'ennemi : poussée, attirance, échange de place,
    // lus dans la donnée (effets 5, 6, 8, 1021, 1041 à 1043 sur une cible « A »).
    const DEPLACEMENTS: [u32; 7] = [5, 6, 8, 1021, 1041, 1042, 1043];
    let deplacent: Vec<u32> = snapshot_for(classe)?
        .spells
        .iter()
        .filter(|s| {
            s.levels.iter().max_by_key(|l| l.grade).is_some_and(|l| {
                l.other_effects
                    .iter()
                    .any(|e| DEPLACEMENTS.contains(&e.id) && e.target.as_deref().unwrap_or("").contains('A'))
            })
        })
        .map(|s| s.id)
        .collect();
    let declare = ruleset.resources.iter().any(|r| r.id == "cible_bougee_dans_le_mur");
    let mut deplaceurs = Vec::new();
    for sort in &mut ruleset.spells {
        let pose = sort
            .effects
            .iter()
            .any(|e| matches!(e, dofus_ruleset::Effect::Gain { resource, .. } if resource == "bombes_en_portee"));
        if pose {
            sort.tags.push("pose_de_bombe".into());
        }
        if declare && sort.dofusdb_id.is_some_and(|id| deplacent.contains(&id)) {
            sort.tags.push("deplace_la_cible".into());
            deplaceurs.push(sort.name.fr.clone());
        }
    }
    let effet = |e: serde_json::Value| -> Result<dofus_ruleset::StateEffect, String> {
        serde_json::from_value(e).map_err(|e| e.to_string())
    };
    let deux_bombes = serde_json::json!({ "kind": "at_least", "scope": "caster", "resource": "bombes_en_portee", "amount": 2 });
    let mut detentes = vec![
        effet(serde_json::json!({
            "trigger": "turn_start", "requires": deux_bombes, "lines": [mur.ligne_du_mur(niveau, false)?],
        }))?,
        effet(serde_json::json!({
            "trigger": "spell_tagged", "tag": "pose_de_bombe", "spends": 0,
            "requires": { "kind": "exactly", "resource": "bombes_en_portee", "amount": 2 },
            "lines": [mur.ligne_du_mur(niveau, true)?],
        }))?,
    ];
    if declare {
        detentes.push(effet(serde_json::json!({
            "trigger": "spell_tagged", "tag": "deplace_la_cible", "spends": 0,
            "requires": { "kind": "all", "all": [deux_bombes, { "kind": "caster_has", "resource": "cible_bougee_dans_le_mur" }] },
            "lines": [mur.ligne_du_mur(niveau, true)?],
        }))?);
    }
    ruleset
        .resources
        .iter_mut()
        .find(|r| r.id == "bombes_en_portee")
        .ok_or("le Roublard n'a plus de bombes dans ses règles")?
        .while_present
        .extend(detentes);
    let mut notes = vec![
        format!(
            "Plombage sur une bombe : il redéclenche le mur de {}, chaque bombe montée d'un cran, \
             dès que deux bombes sont bien placées ; sans critique, comme toute bombe",
            mur.en_clair()
        ),
        "Votre mur de bombes frappe aussi la cible au début de chacun de ses tours, à la base hors de votre \
         tour, et quand il se forme sur elle, à la base de votre tour"
            .to_string(),
    ];
    if request.etats_declares.get("cible_bougee_dans_le_mur").is_some_and(|v| *v > 0) {
        notes.push(format!(
            "Vos déplacements gardent la cible dans le mur : il la frappe à chacun ({})",
            deplaceurs.join(", ")
        ));
    }
    Ok((ruleset, notes))
}

/// Les invocations jouent leur tour dans la rotation : une fois invoquée,
/// chacune joue à chaque tour la meilleure combinaison de ses attaques que ses PA
/// permettent, dès le tour qui suit son invocation, à la part des
/// caractéristiques de l'invocateur qu'elle reçoit, sans aucun % ni dommage
/// critique.
///
/// Un compteur par invocation du deck frappe en début de tour autant de fois
/// qu'il y a d'invocations de ce type en jeu ; un compteur partagé ferme les
/// sorts d'invocation au plafond du build (le total « Invocations » de la fiche,
/// ou 1 sans équipement importé).
///
/// Les invocations communes (`data/communs.yaml`) aussi : l'Arakne, le Chaferfu
/// et leurs variantes, chacune tirant un monstre à 80 % et l'autre de sa paire
/// à 20 %. Le Pacte Bestial ne les sacrifie pas, Cortège Sauvage ne les tue
/// pas, et Piqûre Motivante ne se donne qu'aux invocations de l'Osamodas.
///
/// Les sorts de l'Osamodas qui s'en servent : le Pacte Bestial les sacrifie,
/// Piqûre Motivante donne 2 PA à l'une d'elles pendant trois tours, le Martinet
/// les fait attaquer à 50 %.
fn avec_les_invocations(
    mut ruleset: Ruleset,
    request: &Request,
    build: &dofus_build::BuildInput,
    resolved: &Resolved,
) -> Result<(Ruleset, Vec<String>), String> {
    use crate::invocations::{pa_de, tour_de_l_invocation, AttaqueJouee, LigneJouee};
    let mut notes = Vec::new();
    let releve = crate::invocations::releve();
    // Une attaque d'invocation, au taux propre de l'attaque plus le critique du
    // build, frappant une fois par invocation de ce type en jeu.
    let ligne = |l: &LigneJouee, taux_de_base: u8, facteur: i64, compteur: Option<&str>, reduction: Option<u32>| {
        let taux = dofus_engine::taux_critique(i32::from(taux_de_base), resolved.crit_bonus_percent).permille() / 10;
        let mut l = serde_json::json!({
            "element": l.element,
            "normal": [l.normal.0, l.normal.1],
            "critical": [l.critique.0, l.critique.1],
            "invocation": facteur,
            "taux_critique": taux,
        });
        if let Some(c) = compteur {
            l["repeats_per"] = serde_json::json!([c]);
        }
        if let Some(r) = reduction {
            l["facteur"] = serde_json::json!(r);
        }
        l
    };
    // Un lancer d'attaque : toutes ses lignes, au taux de l'attaque.
    let lignes_d_attaque = |a: &AttaqueJouee, facteur: i64, compteur: Option<&str>, reduction: Option<u32>| {
        a.lignes.iter().map(|l| ligne(l, a.taux_critique, facteur, compteur, reduction)).collect::<Vec<_>>()
    };
    let lignes_du_tour = |tour: &[AttaqueJouee], facteur: i64, compteur: Option<&str>| -> Vec<serde_json::Value> {
        tour.iter()
            .flat_map(|a| (0..a.coups).flat_map(|_| lignes_d_attaque(a, facteur, compteur, None)).collect::<Vec<_>>())
            .collect()
    };
    let en_clair = |tour: &[AttaqueJouee]| -> String {
        tour.iter().map(|a| format!("{} ×{}", a.nom, a.coups)).collect::<Vec<_>>().join(" et ")
    };

    // Les créatures de l'Osamodas, que le Pacte Bestial sacrifie : les
    // monstres 8070 à 8081, visés par famille aux rangs 1 et 4 de son
    // sous-sort 32557. Les Esprits n'en sont pas, et n'ont pas d'attaque
    // relevée.
    const CREATURES_OSAMODAS: std::ops::RangeInclusive<u32> = 8070..=8081;
    /// Une invocation du deck, et ce qu'elle joue.
    struct Jouee {
        i: usize,
        nom: String,
        rang: &'static crate::invocations::Rang,
        facteur: i64,
        tour: Vec<AttaqueJouee>,
        /// Le palier d'une créature de l'Osamodas, que le Pacte sacrifie.
        palier: Option<u8>,
        /// Une invocation commune à toutes les classes.
        commune: bool,
        /// L'autre monstre qu'un sort commun invoque : sa chance, son nom, sa
        /// part des caractéristiques et son tour.
        autre: Option<(u8, String, i64, Vec<AttaqueJouee>)>,
    }
    let mut jouees: Vec<Jouee> = Vec::new();
    for (i, sort) in ruleset.spells.iter().enumerate() {
        if !request.deck.contains(&sort.id) {
            continue;
        }
        let Some(invocation) = sort.dofusdb_id.and_then(|id| {
            releve.invocations.iter().find(|v| v.sort == id && (v.classe == build.class || v.classe == 0))
        }) else {
            continue;
        };
        // Les tourelles du Steamer ont leur propre modèle, à paliers.
        if invocation.monstre.is_some_and(|m| TOURELLES_DU_STEAMER.contains(&m)) {
            continue;
        }
        let Some(rang) = crate::invocations::rang_au_niveau(invocation, build.level) else { continue };
        let facteur = crate::invocations::facteur(invocation.classe, rang.pi);
        let tour = tour_de_l_invocation(rang, &resolved.profile, facteur, pa_de(rang), &[]);
        if tour.is_empty() {
            notes.push(format!(
                "{} : son invocation n'a pas d'attaque à jouer (coût en PA inconnu ou PA insuffisants), elle ne frappe pas",
                sort.name.fr
            ));
            continue;
        }
        let nom = rang.nom.clone().unwrap_or_else(|| invocation.nom.clone());
        // Le palier d'une créature, qui fixe ce que son sacrifice rapporte.
        let palier = rang.pi.filter(|_| invocation.monstre.is_some_and(|m| CREATURES_OSAMODAS.contains(&m)));
        let autre = invocation.autre.and_then(|a| {
            let entree = releve.invocations.iter().find(|v| v.classe == 0 && v.monstre == Some(a.monstre))?;
            let rang = crate::invocations::rang_au_niveau(entree, build.level)?;
            let facteur = crate::invocations::facteur(entree.classe, rang.pi);
            let tour = tour_de_l_invocation(rang, &resolved.profile, facteur, pa_de(rang), &[]);
            Some((a.chance, rang.nom.clone().unwrap_or_else(|| entree.nom.clone()), facteur, tour))
        });
        jouees.push(Jouee { i, nom, rang, facteur, tour, palier, commune: invocation.classe == 0, autre });
    }
    if jouees.is_empty() {
        return Ok((ruleset, notes));
    }
    let plafond = plafond_d_invocations(build);
    let osamodas = build.class == 2;
    let dans_le_deck = |id: &str| request.deck.iter().any(|s| s == id);
    let piqure = osamodas && ruleset.spells.iter().any(|s| s.id == "piqure_motivante");
    // Le Pacte Bestial et Cortège Sauvage, quand le deck les porte : voir
    // `bestial` et `cortege_sauvage` dans les règles de l'Osamodas.
    let pacte = osamodas && dans_le_deck("pacte_bestial") && ruleset.resources.iter().any(|r| r.id == "bestial");
    let cortege = osamodas
        && dans_le_deck("cortege_sauvage")
        && ruleset.resources.iter().any(|r| r.id == "cortege_sauvage_morts");
    // Cortège Sauvage donne 3 Invocations de plus, deux tours durant.
    let en_jeu_au_plus = plafond + if cortege { 3 } else { 0 };
    // Les places : un compteur partagé, un par invocation, un pour Piqûre
    // Motivante, un pour les places de Cortège Sauvage ; une détente par
    // invocation, une pour Piqûre Motivante.
    let detentes: usize = ruleset.resources.iter().map(|r| r.while_present.len()).sum();
    let en_plus = usize::from(piqure);
    let place = dofus_engine::MAX_RESOURCES
        .saturating_sub(ruleset.resources.len() + 1 + usize::from(cortege) + en_plus)
        .min(dofus_engine::MAX_STATE_TRIGGERS.saturating_sub(detentes + en_plus));
    if jouees.len() > place {
        notes.push(format!(
            "{} invocations du deck n'ont pas trouvé de place sous les plafonds du moteur : la rotation s'en passe",
            jouees.len() - place
        ));
        jouees.truncate(place);
    }
    if jouees.is_empty() {
        return Ok((ruleset, notes));
    }
    let effet = |e: serde_json::Value| -> Result<dofus_ruleset::Effect, String> {
        serde_json::from_value(e).map_err(|e| e.to_string())
    };
    // Sous Cortège Sauvage, « toutes les invocations Osamodas meurent au début
    // du tour du lanceur » : après avoir joué le leur.
    let mortel = |mut r: serde_json::Value| {
        if cortege {
            r["reset_at_turn_start_while"] = serde_json::json!("cortege_sauvage_morts");
        }
        r
    };
    ruleset.resources.push(
        serde_json::from_value(mortel(serde_json::json!({
            "id": "invocations_en_jeu", "scope": "caster", "max": en_jeu_au_plus, "default": 0,
            "monotone": "increasing",
        })))
        .map_err(|e| e.to_string())?,
    );
    if cortege {
        ruleset.resources.push(
            serde_json::from_value(serde_json::json!({
                "id": "places_d_invocation", "scope": "caster", "max": en_jeu_au_plus, "default": plafond,
                "monotone": "increasing",
                "duration": { "turns": 1, "refresh": "on_apply", "on_expire": "reset_to_default" },
            }))
            .map_err(|e| e.to_string())?,
        );
        if let Some(sort) = ruleset.spells.iter_mut().find(|s| s.id == "cortege_sauvage") {
            sort.effects.push(effet(serde_json::json!({
                "effect": "gain", "resource": "places_d_invocation", "amount": 3,
            }))?);
        }
    }
    // Le Pacte Bestial « sacrifie toutes ses présentes et futures invocations
    // Osamodas » : son lancer les retire, et tant qu'il tient, une invocation
    // lancée est sacrifiée aussitôt. Chaque sacrifice rend 3 % de dommages
    // finaux par palier de PI pour trois tours, et relance l'état et ses PA
    // pour trois tours.
    let hors_bestial = serde_json::json!({ "kind": "exactly", "resource": "bestial", "amount": 0 });
    let sous_bestial = serde_json::json!({ "kind": "caster_has", "resource": "bestial" });
    for j in &jouees {
        let id = format!("invocation_{}", ruleset.spells[j.i].id);
        // Un sort commun tire l'un de deux monstres : le tour de chacun, en
        // tirage, à sa chance.
        let lignes = match &j.autre {
            None => lignes_du_tour(&j.tour, j.facteur, Some(&id)),
            Some((chance, _, facteur_autre, tour_autre)) => {
                let issues = [(0, 100 - *chance, &j.tour, j.facteur), (1, *chance, tour_autre, *facteur_autre)];
                let mut lignes = Vec::new();
                for (issue, chance, tour, facteur) in issues {
                    for mut l in lignes_du_tour(tour, facteur, Some(&id)) {
                        l["tirage"] = serde_json::json!(id);
                        l["issue"] = serde_json::json!(issue);
                        l["chance"] = serde_json::json!(chance);
                        lignes.push(l);
                    }
                }
                lignes
            }
        };
        let compteur = serde_json::json!({
            "id": id, "scope": "caster", "max": en_jeu_au_plus, "default": 0, "monotone": "increasing",
            "while_present": [{ "trigger": "turn_start", "lines": lignes }],
        });
        ruleset.resources.push(
            serde_json::from_value(if j.commune { compteur } else { mortel(compteur) }).map_err(|e| e.to_string())?,
        );
        let sort = &mut ruleset.spells[j.i];
        for r in [id.as_str(), "invocations_en_jeu"] {
            let mut g = serde_json::json!({ "effect": "gain", "resource": r });
            if pacte && !j.commune {
                g["requires"] = hors_bestial.clone();
            }
            sort.effects.push(effet(g)?);
        }
        if let (true, Some(p)) = (pacte, j.palier) {
            for (r, n) in [
                ("pacte_bestial_finaux", p),
                ("pacte_bestial_sacrifices", p),
                ("bestial", 1),
                ("pacte_bestial_pa", 1),
            ] {
                sort.effects.push(effet(serde_json::json!({
                    "effect": "gain", "resource": r, "amount": n, "requires": sous_bestial,
                }))?);
            }
        }
        if cortege {
            sort.requires_more_than = Some(dofus_ruleset::Surplus {
                resource: "places_d_invocation".into(),
                than: "invocations_en_jeu".into(),
            });
        } else if sort.blocked_at_max.is_none() {
            sort.blocked_at_max = Some("invocations_en_jeu".into());
        }
        let depense = |tour: &[AttaqueJouee]| -> i64 { tour.iter().map(|a| i64::from(a.pa) * i64::from(a.coups)).sum() };
        let (nom, rang, facteur, tour) = (&j.nom, j.rang, j.facteur, &j.tour);
        notes.push(match &j.autre {
            None => format!(
                "{nom} : {} par tour ({} PA sur {}), à {facteur} % de vos caractéristiques, dès le tour qui \
                 suit son invocation ; ni vos % de dommages ni vos dommages critiques",
                en_clair(tour),
                depense(tour),
                pa_de(rang),
            ),
            Some((chance, nom_autre, _, tour_autre)) => format!(
                "{} : {nom} à {} %, {} par tour ({} PA), ou {nom_autre} à {chance} %, {} par tour ({} PA) ; \
                 à {facteur} % de vos caractéristiques, dès le tour qui suit son invocation ; ni vos % de \
                 dommages ni vos dommages critiques",
                ruleset.spells[j.i].name.fr,
                100 - chance,
                en_clair(tour),
                depense(tour),
                en_clair(tour_autre),
                depense(tour_autre),
            ),
        });
    }
    if pacte {
        // Les créatures en jeu au lancer : un gain par créature de chaque
        // type, avant que les compteurs ne retombent. Lus avant la bascule de
        // l'état, que les règles placent en dernier.
        let creatures: Vec<(String, u8)> = jouees
            .iter()
            .filter_map(|j| j.palier.map(|p| (format!("invocation_{}", ruleset.spells[j.i].id), p)))
            .collect();
        let mut sacrifice = Vec::new();
        for (id, p) in &creatures {
            for k in 1..=en_jeu_au_plus {
                for r in ["pacte_bestial_finaux", "pacte_bestial_sacrifices"] {
                    sacrifice.push(effet(serde_json::json!({
                        "effect": "gain", "resource": r, "amount": p,
                        "requires": { "kind": "at_least", "scope": "caster", "resource": id, "amount": k },
                    }))?);
                }
            }
        }
        // Toutes les invocations de l'Osamodas relevées sont des créatures :
        // le compteur partagé retombe avec elles.
        for id in creatures.iter().map(|(id, _)| id.as_str()).chain(["invocations_en_jeu"]) {
            sacrifice.push(effet(serde_json::json!({ "effect": "reset", "resource": id }))?);
        }
        // Les plafonds : au plus toutes les places en créatures du palier 3,
        // plus une invocation de chaque type lancée sous l'état dans le tour.
        let par_tour = (3 * u32::from(en_jeu_au_plus) + creatures.iter().map(|(_, p)| u32::from(*p)).sum::<u32>())
            .min(u32::from(u8::MAX) / 3) as u8;
        for r in &mut ruleset.resources {
            match r.id.as_str() {
                "pacte_bestial_sacrifices" | "pacte_bestial_sacrifices_1" | "pacte_bestial_sacrifices_2" => {
                    r.max = par_tour
                }
                "pacte_bestial_finaux" => r.max = 3 * par_tour,
                _ => {}
            }
        }
        let sort = ruleset
            .spells
            .iter_mut()
            .find(|s| s.id == "pacte_bestial")
            .ok_or("l'Osamodas n'a plus de Pacte Bestial dans ses règles")?;
        let bascule = sort
            .effects
            .iter()
            .position(|e| matches!(e, dofus_ruleset::Effect::Toggle { resource } if resource == "bestial"))
            .ok_or("le Pacte Bestial ne bascule plus l'état Bestial")?;
        sort.effects.splice(bascule..bascule, sacrifice);
        notes.push(
            "Pacte Bestial : il sacrifie vos invocations en jeu, et celles que vous lancez tant qu'il tient, \
             contre 3 % de dommages finaux par palier de PI pendant trois tours ; chaque invocation lancée \
             sous l'état le prolonge, lui et ses PA ; relancé, il retire tout"
                .to_string(),
        );
    }
    if cortege {
        notes.push(
            "Cortège Sauvage : vos sorts d'invocation coûtent 1 PA de moins et 3 invocations de plus tiennent \
             en jeu, deux tours durant ; vos invocations meurent au début des deux tours qui suivent, après \
             avoir joué"
                .to_string(),
        );
    }
    // Piqûre Motivante : « augmente les PA et les PM de la cible. Les effets
    // sont plus importants sur les invocations » : +2 PA pendant trois tours
    // (effet 111, cible « j,J »). La rotation la donne à l'invocation que ces
    // PA servent le plus, qui joue en plus ce qu'ils paient dans les limites
    // qui lui restent ; trois tours de jeu (durée 3, écrite 2).
    if piqure {
        let meilleure = jouees
            .iter()
            .filter(|j| !j.commune)
            .map(|j| {
                let (i, nom, rang, facteur, tour) = (&j.i, &j.nom, j.rang, &j.facteur, &j.tour);
                let plus = tour_de_l_invocation(rang, &resolved.profile, *facteur, 2, tour);
                let valeur: f64 = plus
                    .iter()
                    .flat_map(|a| a.lignes.iter().map(move |l| (a.coups, l)))
                    .map(|(coups, l)| {
                        let m = |(lo, hi): (i32, i32)| {
                            (crate::invocations::coup(lo, l.element, &resolved.profile, *facteur)
                                + crate::invocations::coup(hi, l.element, &resolved.profile, *facteur)) as f64
                        };
                        f64::from(coups) * m(l.normal)
                    })
                    .sum();
                (valeur, *i, nom.clone(), *facteur, plus)
            })
            .filter(|(_, _, _, _, plus)| !plus.is_empty())
            .max_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, i, nom, facteur, plus)) = meilleure {
            let compteur = format!("invocation_{}", ruleset.spells[i].id);
            ruleset.resources.push(
                serde_json::from_value(serde_json::json!({
                    "id": "piqure_motivante_invocation", "scope": "caster", "max": 1, "default": 0,
                    "monotone": "increasing",
                    "duration": { "turns": 2, "refresh": "on_apply", "on_expire": "reset_to_default" },
                    "while_present": [{
                        "trigger": "turn_start",
                        "requires": { "kind": "caster_has", "resource": compteur },
                        "lines": lignes_du_tour(&plus, facteur, None),
                    }],
                }))
                .map_err(|e| e.to_string())?,
            );
            let mode: dofus_ruleset::ModeDef = serde_json::from_value(serde_json::json!({
                "name": { "fr": "sur une invocation", "en": "on a summon" },
                "requires": { "kind": "caster_has", "resource": "invocations_en_jeu" },
                "effects": [{ "effect": "gain", "resource": "piqure_motivante_invocation" }],
            }))
            .map_err(|e| e.to_string())?;
            if let Some(sort) = ruleset.spells.iter_mut().find(|s| s.id == "piqure_motivante") {
                sort.mode_name = Some(dofus_ruleset::Localised { fr: "sur un allié".into(), en: "on an ally".into() });
                sort.modes.push(mode);
            }
            if request.deck.iter().any(|s| s == "piqure_motivante") {
                notes.push(format!(
                    "Piqûre Motivante sur une invocation : +2 PA pendant trois tours à votre {nom}, qui joue en plus {}",
                    en_clair(&plus)
                ));
            }
        }
    }
    // Le Martinet : « les invocations Osamodas attaquent également [...] Les
    // dommages et soins des invocations sont réduits de 50 % pendant
    // l'utilisation du sort », chacune avec l'attaque que son texte désigne
    // (« Tofu : Bisou Béco »). Une ligne par invocation de ce type en jeu.
    if osamodas {
        let snapshot = snapshot_for(build.class)?;
        let texte = ruleset
            .spells
            .iter()
            .find(|s| s.id == "martinet")
            .and_then(|s| s.dofusdb_id)
            .and_then(|id| snapshot.spells.iter().find(|s| s.id == id))
            .and_then(|s| s.description_fr.clone())
            .unwrap_or_default();
        let designees: Vec<(String, String)> = texte
            .lines()
            .filter_map(|l| l.trim().strip_prefix('•'))
            .filter_map(|l| l.split_once(':'))
            .map(|(qui, quoi)| (qui.trim().to_string(), quoi.trim().to_string()))
            .collect();
        let mut lignes = Vec::new();
        let mut qui_attaque = Vec::new();
        for j in &jouees {
            let (i, nom, rang, facteur) = (&j.i, &j.nom, j.rang, &j.facteur);
            let Some((_, attaque)) = designees.iter().find(|(qui, _)| qui.eq_ignore_ascii_case(nom)) else { continue };
            let Some(a) = crate::invocations::attaque_nommee(rang, &resolved.profile, *facteur, attaque) else { continue };
            let compteur = format!("invocation_{}", ruleset.spells[*i].id);
            lignes.extend(lignes_d_attaque(&a, *facteur, Some(&compteur), Some(50)));
            qui_attaque.push(format!("{nom} : {}", a.nom));
        }
        if !lignes.is_empty() {
            if let Some(sort) = ruleset.spells.iter_mut().find(|s| s.id == "martinet") {
                for l in lignes {
                    sort.lines.push(serde_json::from_value(l).map_err(|e| e.to_string())?);
                }
                if request.deck.iter().any(|s| s == "martinet") {
                    notes.push(format!(
                        "Martinet : chacune de vos invocations en jeu attaque aussi, à 50 % ({})",
                        qui_attaque.join(", ")
                    ));
                }
            }
        }
    }
    notes.push(if build.dofusbook.is_some() {
        format!("{plafond} invocation{} en jeu au plus, d'après la fiche DofusBook", if plafond > 1 { "s" } else { "" })
    } else {
        "1 invocation en jeu au plus, faute d'équipement importé de DofusBook".to_string()
    });
    Ok((ruleset, notes))
}

/// Les pièges et les glyphes que la cible déclenche, N par tour selon le joueur
/// (`pieges_declenches`) : chaque piège posé compte ses dégâts sur la cible
/// debout sur son centre, dans cette limite, par une réserve (`pieges_du_tour`)
/// pleine à chaque début de tour, que chaque pose consomme. Les pièges du Sram,
/// Vendetta du Crâ, la Barrière du Féca : un sort sans ligne ni effet dans les
/// règles, qui pose des dégâts au sol.
///
/// La Concentration de Chakra vole 12 en meilleur élément à chaque piège
/// déclenché tant qu'elle tient ; un poison de piège tombe au début du tour
/// suivant. Les chaînes de plusieurs pièges restent à KrozTrap.
fn avec_les_pieges(
    mut ruleset: Ruleset,
    request: &Request,
    build: &dofus_build::BuildInput,
) -> Result<(Ruleset, Vec<String>), String> {
    let mut notes = Vec::new();
    let Some(plafond) = ruleset.resources.iter().find(|r| r.id == "pieges_declenches").map(|r| r.max) else {
        return Ok((ruleset, notes));
    };
    let n = request.etats_declares.get("pieges_declenches").copied().unwrap_or(0).min(plafond);
    if n == 0 {
        return Ok((ruleset, notes));
    }
    let snapshot = snapshot_for(build.class)?;
    let chakra = ruleset.resources.iter().any(|r| r.id == "concentration_de_chakra");
    let ligne = |element: dofus_damage::Element, (lo, hi): (i64, i64)| {
        serde_json::json!({
            "element": element, "normal": [lo, hi], "critical": [lo, hi], "sans_critique": true,
        })
    };
    let lire = |v: serde_json::Value| -> Result<dofus_ruleset::LineDef, String> {
        serde_json::from_value(v).map_err(|e| e.to_string())
    };
    let mut pieges = Vec::new();
    for (i, sort) in ruleset.spells.iter().enumerate() {
        if !request.deck.contains(&sort.id) || !sort.lines.is_empty() || !sort.effects.is_empty() {
            continue;
        }
        let (lignes, poisons) = if !sort.trap_lines.is_empty() {
            (sort.trap_lines.clone(), Vec::new())
        } else {
            let Some(id) = sort.dofusdb_id else { continue };
            let Some((frappent, poisons)) = crate::reseau::lignes_au_centre(&snapshot, id) else { continue };
            (
                frappent.into_iter().map(|(e, r)| lire(ligne(e, r))).collect::<Result<Vec<_>, _>>()?,
                poisons.into_iter().map(|(e, r)| lire(ligne(e, r))).collect::<Result<Vec<_>, _>>()?,
            )
        };
        if lignes.is_empty() {
            continue;
        }
        pieges.push((i, lignes, poisons));
    }
    if pieges.is_empty() {
        return Ok((ruleset, notes));
    }
    let effet = |e: serde_json::Value| -> Result<dofus_ruleset::Effect, String> {
        serde_json::from_value(e).map_err(|e| e.to_string())
    };
    ruleset.resources.push(
        serde_json::from_value(serde_json::json!({
            "id": "pieges_du_tour", "scope": "caster", "max": n, "default": n, "gain_per_turn": n,
            "monotone": "increasing",
        }))
        .map_err(|e| e.to_string())?,
    );
    let reserve = serde_json::json!({ "kind": "at_least", "scope": "caster", "resource": "pieges_du_tour", "amount": 1 });
    let mut noms = Vec::new();
    for (i, mut lignes, poisons) in pieges {
        let id = ruleset.spells[i].id.clone();
        if chakra {
            lignes.push(lire(serde_json::json!({
                "best_element": true, "normal": [12, 12], "critical": [12, 12], "sans_critique": true,
                "active_at": { "resource": "concentration_de_chakra", "exactly": 1 },
            }))?);
        }
        let mut effets = vec![
            effet(serde_json::json!({ "effect": "damage" }))?,
            effet(serde_json::json!({ "effect": "consume", "resource": "pieges_du_tour" }))?,
        ];
        if !poisons.is_empty() {
            let poison = format!("poison_{id}");
            ruleset.resources.push(
                serde_json::from_value(serde_json::json!({
                    "id": poison, "scope": "target", "max": 1, "default": 0, "monotone": "increasing",
                    "duration": { "turns": 0, "refresh": "on_apply", "on_expire": "reset_to_default" },
                    "while_present": [{ "trigger": "turn_start", "lines": poisons }],
                }))
                .map_err(|e| e.to_string())?,
            );
            effets.push(effet(serde_json::json!({ "effect": "gain", "resource": poison }))?);
        }
        let sort = &mut ruleset.spells[i];
        // Ce que le piège pose en partant, Vendetta depuis la 3.7.
        effets.extend(sort.trap_effects.iter().cloned());
        sort.lines = lignes;
        sort.effects = effets;
        sort.requires = Some(
            serde_json::from_value(match sort.requires.take() {
                Some(c) => serde_json::json!({ "kind": "all", "all": [serde_json::to_value(c).map_err(|e| e.to_string())?, reserve] }),
                None => reserve.clone(),
            })
            .map_err(|e| e.to_string())?,
        );
        noms.push(sort.name.fr.clone());
    }
    notes.push(format!(
        "{} : {n} par tour déclenché{} par la cible, comptés quand vous les posez, la cible debout sur leur centre ; \
         les chaînes de plusieurs pièges se calculent dans KrozTrap",
        noms.join(", "),
        if n > 1 { "s" } else { "" },
    ));
    Ok((ruleset, notes))
}

/// Le Steamer.
const STEAMER: u32 = 15;

/// Les monstres des tourelles du Steamer : Bathyscaphe 5831, Chalutier 5832,
/// Foreuse 5833, Gardienne 5835, Harponneuse 5836, Tactirelle 5837, que
/// l'Évolution vise ensemble (« F5831 » à « F5837 » de sa cible).
const TOURELLES_DU_STEAMER: [u32; 6] = [5831, 5832, 5833, 5835, 5836, 5837];

/// Un sort spécial : le monstre de la tourelle, le nom du sort, sa fourchette
/// à chaque rang de la tourelle.
type SortSpecial = (u32, &'static str, &'static [(i32, i32)]);

/// Les sorts spéciaux des tourelles offensives, qu'Embuscade et Sonar font
/// lancer sur la cible par chaque tourelle à l'Évolution III (« Activation
/// Steamer », sous-sort 23872, cible « E135 ») : Armada pour la Harponneuse
/// (sous-sort 13893), Excavation pour la Foreuse (13888). Au grade du rang de
/// la tourelle (états 3495 à 3497), dans le meilleur élément, sans critique.
const SPECIAUX_DES_TOURELLES: [SortSpecial; 2] = [
    (5836, "Armada", &[(20, 23), (27, 30), (34, 39)]),
    (5833, "Excavation", &[(30, 34), (34, 39)]),
];

/// Les tourelles du Steamer dans la rotation, une de chaque type au plus.
///
/// Un compteur par tourelle du deck dit si elle est en jeu et à quel palier :
/// 0 absente, 1 à 3 pour l'Évolution I à III. Une tourelle offensive
/// (Harponneuse, Foreuse) joue au début de chaque tour la meilleure
/// combinaison des attaques de son palier ; les autres ne frappent pas, mais
/// comptent pour les sorts qui exigent une tourelle.
///
/// * Évolution et Surtension visent une tourelle offensive : un mode par
///   tourelle. L'Évolution la monte d'un palier, la Surtension à l'Évolution
///   III, puis elle redescend d'un palier après avoir joué son tour suivant
///   (« -1 Évolution », délai 1). Une tourelle n'évolue qu'une fois par tour
///   (« Évolution bloquée », 6094).
/// * Court-circuit frappe autour d'une tourelle, qu'il fait évoluer : sans
///   tourelle en jeu, il ne part pas.
/// * Au contact de la cible, que le joueur déclare : Vapor et Turbine font
///   évoluer les tourelles, le Sabotage frappe plus fort par tourelle évoluée
///   puis les rétrograde, Ancrage et Gouvernail rendent 1 PA.
/// * Embuscade et Sonar font lancer leur sort spécial aux tourelles
///   offensives à l'Évolution III.
fn avec_les_tourelles(
    mut ruleset: Ruleset,
    request: &Request,
    build: &dofus_build::BuildInput,
    resolved: &Resolved,
) -> Result<(Ruleset, Vec<String>), String> {
    use crate::invocations::{pa_de, paliers, tour_au_palier, AttaqueJouee};
    let mut notes = Vec::new();
    if build.class != STEAMER {
        return Ok((ruleset, notes));
    }
    let releve = crate::invocations::releve();
    let dans_le_deck = |id: &str| request.deck.iter().any(|s| s == id);
    struct Tourelle {
        indice: usize,
        id: String,
        nom: String,
        /// Les attaques de chaque palier, et la part transmise.
        offensive: Option<(Vec<Vec<AttaqueJouee>>, i64)>,
        special: Option<(&'static str, (i32, i32))>,
    }
    let mut tourelles = Vec::new();
    for (i, sort) in ruleset.spells.iter().enumerate() {
        if !dans_le_deck(&sort.id) {
            continue;
        }
        let Some(invocation) = sort
            .dofusdb_id
            .and_then(|id| releve.invocations.iter().find(|v| v.sort == id && v.classe == build.class))
        else {
            continue;
        };
        let Some(monstre) = invocation.monstre.filter(|m| TOURELLES_DU_STEAMER.contains(m)) else { continue };
        let Some(rang) = crate::invocations::rang_au_niveau(invocation, build.level) else { continue };
        let facteur = crate::invocations::facteur(invocation.classe, rang.pi);
        let tours: Vec<Vec<AttaqueJouee>> =
            (1..=3).map(|k| tour_au_palier(rang, &resolved.profile, facteur, pa_de(rang), &[], k)).collect();
        let offensive = (paliers(rang) == 3 && tours.iter().all(|t| !t.is_empty())).then_some((tours, facteur));
        let special = SPECIAUX_DES_TOURELLES
            .iter()
            .find(|(m, _, _)| *m == monstre)
            .and_then(|(_, nom, grades)| grades.get(usize::from(rang.rang).saturating_sub(1)).map(|g| (*nom, *g)));
        tourelles.push(Tourelle {
            indice: i,
            id: sort.id.clone(),
            nom: rang.nom.clone().unwrap_or_else(|| invocation.nom.clone()),
            offensive,
            special,
        });
    }
    let court_circuit = dans_le_deck("court_circuit");
    if tourelles.is_empty() && !court_circuit {
        return Ok((ruleset, notes));
    }
    let au_contact = request.etats_declares.get("tourelles_au_contact").is_some_and(|v| *v > 0);
    let ancrage = au_contact && (dans_le_deck("ancrage") || dans_le_deck("gouvernail"));
    let offensives = tourelles.iter().filter(|t| t.offensive.is_some()).count();
    let detentes: usize = ruleset.resources.iter().map(|r| r.while_present.len()).sum();
    let ajoutees = 1 + tourelles.len() + 2 * offensives + usize::from(ancrage);
    if ruleset.resources.len() + ajoutees > dofus_engine::MAX_RESOURCES
        || detentes + offensives > dofus_engine::MAX_STATE_TRIGGERS
    {
        notes.push("les tourelles n'ont pas trouvé de place sous les plafonds du moteur : la rotation s'en passe".into());
        return Ok((ruleset, notes));
    }
    let effet = |e: serde_json::Value| -> Result<dofus_ruleset::Effect, String> {
        serde_json::from_value(e).map_err(|e| e.to_string())
    };
    let condition = |c: serde_json::Value| -> Result<dofus_ruleset::Condition, String> {
        serde_json::from_value(c).map_err(|e| e.to_string())
    };
    let pousser = |ruleset: &mut Ruleset, r: serde_json::Value| -> Result<(), String> {
        ruleset.resources.push(serde_json::from_value(r).map_err(|e| e.to_string())?);
        Ok(())
    };
    // Une attaque de tourelle : la part transmise de vos caractéristiques, au
    // taux propre de l'attaque plus le critique du build, à son palier.
    let lignes_du_palier = |tour: &[AttaqueJouee], facteur: i64, compteur: &str, palier: usize| {
        tour.iter()
            .flat_map(|a| {
                let taux =
                    dofus_engine::taux_critique(i32::from(a.taux_critique), resolved.crit_bonus_percent).permille() / 10;
                (0..a.coups).flat_map(move |_| {
                    a.lignes.iter().map(move |l| {
                        serde_json::json!({
                            "element": l.element,
                            "normal": [l.normal.0, l.normal.1],
                            "critical": [l.critique.0, l.critique.1],
                            "invocation": facteur,
                            "taux_critique": taux,
                            "active_at": { "resource": compteur, "exactly": palier },
                        })
                    })
                })
            })
            .collect::<Vec<_>>()
    };
    let plafond = plafond_d_invocations(build);
    pousser(
        &mut ruleset,
        serde_json::json!({
            "id": "invocations_en_jeu", "scope": "caster", "max": plafond, "default": 0, "monotone": "increasing",
        }),
    )?;
    let en_clair = |tour: &[AttaqueJouee]| -> String {
        tour.iter().map(|a| format!("{} ×{}", a.nom, a.coups)).collect::<Vec<_>>().join(" et ")
    };
    for t in &tourelles {
        let compteur = format!("tourelle_{}", t.id);
        match &t.offensive {
            Some((tours, facteur)) => {
                let lignes: Vec<serde_json::Value> = tours
                    .iter()
                    .enumerate()
                    .flat_map(|(k, tour)| lignes_du_palier(tour, *facteur, &compteur, k + 1))
                    .collect();
                pousser(
                    &mut ruleset,
                    serde_json::json!({
                        "id": compteur, "scope": "caster", "max": 3, "default": 0, "monotone": "increasing",
                        "lose_at_turn_start_while": format!("surtension_{}", t.id),
                        "while_present": [{ "trigger": "turn_start", "lines": lignes }],
                    }),
                )?;
                pousser(
                    &mut ruleset,
                    serde_json::json!({
                        "id": format!("evolution_bloquee_{}", t.id), "scope": "caster", "max": 1, "default": 0,
                        "duration": { "turns": 0, "refresh": "on_apply", "on_expire": "reset_to_default" },
                    }),
                )?;
                pousser(
                    &mut ruleset,
                    serde_json::json!({
                        "id": format!("surtension_{}", t.id), "scope": "caster", "max": 1, "default": 0,
                        "duration": { "turns": 1, "refresh": "on_apply", "on_expire": "reset_to_default" },
                    }),
                )?;
                notes.push(format!(
                    "{} : {} par tour à l'Évolution I, {} à la II, {} à la III, à {facteur} % de vos \
                     caractéristiques, dès le tour qui suit sa pose ; ni vos % de dommages ni vos dommages critiques",
                    t.nom,
                    en_clair(&tours[0]),
                    en_clair(&tours[1]),
                    en_clair(&tours[2]),
                ));
            }
            None => pousser(
                &mut ruleset,
                serde_json::json!({ "id": compteur, "scope": "caster", "max": 1, "default": 0, "monotone": "increasing" }),
            )?,
        }
        // Une tourelle de chaque type, posée à l'Évolution I.
        let sort = &mut ruleset.spells[t.indice];
        sort.effects.push(effet(serde_json::json!({ "effect": "gain", "resource": compteur }))?);
        sort.effects.push(effet(serde_json::json!({ "effect": "gain", "resource": "invocations_en_jeu" }))?);
        sort.blocked_by.get_or_insert_with(|| compteur.clone());
        sort.blocked_at_max.get_or_insert_with(|| "invocations_en_jeu".into());
    }
    if ancrage {
        pousser(
            &mut ruleset,
            serde_json::json!({
                "id": "tourelle_dans_la_zone", "scope": "caster", "max": 1, "default": 0,
                "duration": { "turns": 0, "refresh": "on_apply", "on_expire": "reset_to_default" },
            }),
        )?;
    }
    let offensives: Vec<&Tourelle> = tourelles.iter().filter(|t| t.offensive.is_some()).collect();
    // Faire évoluer une tourelle : un palier de plus (deux pour la
    // Surtension, plafonnés à l'Évolution III), une fois par tour.
    let evoluer = |t: &Tourelle, paliers: u8, condition: Option<serde_json::Value>| -> Result<Vec<dofus_ruleset::Effect>, String> {
        let mut g = serde_json::json!({
            "effect": "gain", "resource": format!("tourelle_{}", t.id), "amount": paliers, "at_cap": "skip",
        });
        if let Some(c) = condition {
            g["requires"] = c;
        }
        Ok(vec![
            effet(g)?,
            effet(serde_json::json!({
                "effect": "gain", "resource": format!("evolution_bloquee_{}", t.id),
                "requires": { "kind": "at_least", "scope": "caster", "resource": format!("tourelle_{}", t.id), "amount": 1 },
            }))?,
        ])
    };
    // Une tourelle en jeu, pas à l'Évolution III, pas encore évoluée ce tour.
    let evoluable = |t: &Tourelle| {
        serde_json::json!({ "kind": "all", "all": [
            { "kind": "at_least", "scope": "caster", "resource": format!("tourelle_{}", t.id), "amount": 1 },
            { "kind": "at_most", "resource": format!("tourelle_{}", t.id), "amount": 2 },
            { "kind": "exactly", "resource": format!("evolution_bloquee_{}", t.id), "amount": 0 },
        ]})
    };
    let mode = |nom: String, requires: serde_json::Value, lines: Vec<dofus_ruleset::LineDef>, effects: Vec<dofus_ruleset::Effect>| {
        let mut m: dofus_ruleset::ModeDef = serde_json::from_value(serde_json::json!({
            "name": { "fr": nom, "en": nom }, "requires": requires, "lines": [], "effects": [],
        }))
        .map_err(|e| e.to_string())?;
        m.lines = lines;
        m.effects = effects;
        Ok::<_, String>(m)
    };
    // Évolution et Surtension : un mode par tourelle offensive, le sort
    // lui-même visant la première.
    for (sort_id, paliers) in [("evolution", 1u8), ("surtension", 2)] {
        let Some(i) = ruleset.spells.iter().position(|s| s.id == sort_id) else { continue };
        for (k, t) in offensives.iter().enumerate() {
            let mut effets = evoluer(t, paliers, None)?;
            if sort_id == "surtension" {
                effets.push(effet(serde_json::json!({ "effect": "gain", "resource": format!("surtension_{}", t.id) }))?);
            }
            let nom = format!("sur la {}", t.nom);
            if k == 0 {
                let sort = &mut ruleset.spells[i];
                sort.requires = Some(condition(evoluable(t))?);
                sort.effects = effets;
                if offensives.len() > 1 {
                    sort.mode_name = Some(dofus_ruleset::Localised { fr: nom.clone(), en: nom });
                }
            } else {
                let m = mode(nom, evoluable(t), Vec::new(), effets)?;
                ruleset.spells[i].modes.push(m);
            }
        }
    }
    // Court-circuit frappe autour d'une tourelle et la fait évoluer.
    if let Some(i) = ruleset.spells.iter().position(|s| s.id == "court_circuit") {
        let lignes = ruleset.spells[i].lines.clone();
        let degats = effet(serde_json::json!({ "effect": "damage" }))?;
        let mut alternatives = Vec::new();
        for t in &offensives {
            let mut effets = vec![degats.clone()];
            effets.extend(evoluer(
                t,
                1,
                Some(serde_json::json!({ "kind": "exactly", "resource": format!("evolution_bloquee_{}", t.id), "amount": 0 })),
            )?);
            alternatives.push((
                format!("autour de la {}", t.nom),
                serde_json::json!({ "kind": "at_least", "scope": "caster", "resource": format!("tourelle_{}", t.id), "amount": 1 }),
                effets,
            ));
        }
        // Une autre tourelle, ou aucune : sans tourelle en jeu, il ne part pas.
        if offensives.len() < tourelles.len() || offensives.is_empty() {
            alternatives.push((
                "autour d'une tourelle".to_string(),
                serde_json::json!({ "kind": "at_least", "scope": "caster", "resource": "invocations_en_jeu", "amount": 1 }),
                vec![degats.clone()],
            ));
        }
        let plusieurs = alternatives.len() > 1;
        for (k, (nom, requires, effets)) in alternatives.into_iter().enumerate() {
            if k == 0 {
                let sort = &mut ruleset.spells[i];
                sort.requires = Some(condition(requires)?);
                sort.effects = effets;
                if plusieurs {
                    sort.mode_name = Some(dofus_ruleset::Localised { fr: nom.clone(), en: nom });
                }
            } else {
                let m = mode(nom, requires, lignes.clone(), effets)?;
                ruleset.spells[i].modes.push(m);
            }
        }
        if tourelles.is_empty() {
            notes.push("Court-circuit : il frappe autour d'une tourelle, et aucune n'est au deck".to_string());
        }
    }
    if au_contact {
        // Vapor et Turbine font évoluer les tourelles de leur zone.
        for sort_id in ["vapor", "turbine"] {
            let Some(sort) = ruleset.spells.iter_mut().find(|s| s.id == sort_id) else { continue };
            for t in &offensives {
                let c = serde_json::json!({ "kind": "all", "all": [
                    { "kind": "at_least", "scope": "caster", "resource": format!("tourelle_{}", t.id), "amount": 1 },
                    { "kind": "exactly", "resource": format!("evolution_bloquee_{}", t.id), "amount": 0 },
                ]});
                sort.effects.extend(evoluer(t, 1, Some(c))?);
            }
        }
        // Le Sabotage : +9 de base par tourelle évoluée, lus avant ses
        // effets, puis chacune perd un palier.
        if let Some(sort) = ruleset.spells.iter_mut().find(|s| s.id == "sabotage") {
            for t in &offensives {
                let compteur = format!("tourelle_{}", t.id);
                for l in &mut sort.lines {
                    l.base_bonus.push(dofus_ruleset::BaseBonus::Steps { resource: compteur.clone(), steps: vec![0, 9, 0] });
                }
                sort.effects.push(effet(serde_json::json!({
                    "effect": "consume", "resource": compteur,
                    "requires": { "kind": "at_least", "scope": "caster", "resource": compteur, "amount": 2 },
                }))?);
            }
        }
        // Ancrage et Gouvernail rendent 1 PA quand une tourelle est dans
        // leur zone.
        if ancrage {
            for sort_id in ["ancrage", "gouvernail"] {
                let Some(sort) = ruleset.spells.iter_mut().find(|s| s.id == sort_id) else { continue };
                sort.effects.push(effet(serde_json::json!({
                    "effect": "gain", "resource": "tourelle_dans_la_zone", "ap_bonus": { "amount": 1 },
                    "requires": { "kind": "at_least", "scope": "caster", "resource": "invocations_en_jeu", "amount": 1 },
                }))?);
            }
        }
        if !tourelles.is_empty() {
            notes.push(
                "Vos tourelles au contact de la cible : Vapor et Turbine les font évoluer, le Sabotage frappe \
                 plus fort pour chacune qui a évolué puis la rétrograde, Ancrage et Gouvernail vous rendent 1 PA"
                    .to_string(),
            );
        }
    }
    // Embuscade et Sonar : le sort spécial de chaque tourelle offensive à
    // l'Évolution III, sur la cible.
    let speciaux: Vec<(String, &'static str, (i32, i32), i64)> = tourelles
        .iter()
        .filter_map(|t| Some((format!("tourelle_{}", t.id), t.special?.0, t.special?.1, t.offensive.as_ref()?.1)))
        .collect();
    for sort_id in ["embuscade", "sonar"] {
        let Some(sort) = ruleset.spells.iter_mut().find(|s| s.id == sort_id) else { continue };
        for (compteur, _, (lo, hi), facteur) in &speciaux {
            sort.lines.push(
                serde_json::from_value(serde_json::json!({
                    "best_element": true, "normal": [lo, hi], "critical": [lo, hi],
                    "invocation": facteur, "taux_critique": 0,
                    "active_at": { "resource": compteur, "exactly": 3 },
                }))
                .map_err(|e| e.to_string())?,
            );
        }
    }
    if !speciaux.is_empty() && (dans_le_deck("embuscade") || dans_le_deck("sonar")) {
        notes.push(format!(
            "Embuscade et Sonar : chaque tourelle offensive à l'Évolution III lance aussi son sort spécial sur la \
             cible ({})",
            speciaux.iter().map(|(_, nom, (lo, hi), _)| format!("{nom} {lo} à {hi}")).collect::<Vec<_>>().join(", ")
        ));
    }
    if !tourelles.is_empty() {
        notes.push(format!(
            "{plafond} invocation{} en jeu au plus, une tourelle de chaque type",
            if plafond > 1 { "s" } else { "" }
        ));
    }
    Ok((ruleset, notes))
}

/// Combien d'invocations le build tient en jeu : le total « Invocations » de
/// la fiche, ou 1 sans équipement importé.
fn plafond_d_invocations(build: &dofus_build::BuildInput) -> u8 {
    build
        .dofusbook
        .as_ref()
        .map(|v| crate::fiche::calculer(v, &build.boosts).stats.get("ic").copied().unwrap_or(1.0))
        .map_or(1, |n| n.clamp(1.0, 20.0) as u8)
}

/// Ce que la Rotation et le rejeu partagent : le build résolu, les règles de la
/// classe greffées de tout ce que le build et le deck y ajoutent (Dofus,
/// invocations, mur de bombes), le scénario, et les remarques du calcul
/// (invocations, pièges, tourelles, arme, Dofus) que l'onglet Rotation affiche,
/// jointes aux hypothèses du build après celles de l'équipement.
fn preparer(
    request: &Request,
) -> Result<(dofus_build::BuildInput, Resolved, Ruleset, Scenario, Vec<String>), String> {
    let build = request.build.normalise();
    let mut resolved = resolve_build(&build)?;
    let depart = resolved.assumptions.len();
    // Les objets que la dernière note de sortie change, pas encore relevés :
    // les remarques du calcul le disent aussi.
    resolved.assumptions.extend(crate::donnees::objets_a_relever(&build.items));
    let mut ruleset = load_ruleset(build.class)?;
    crate::objets_de_classe::sur_les_regles(&mut ruleset, &resolved);
    let mut ruleset = if ruleset.costly.is_empty() {
        ruleset
    } else {
        ruleset.avec_mecaniques(&request.mecaniques_de(&ruleset))
    };
    ruleset.spells.extend(sorts_communs().iter().cloned());
    let entravants = if request.poussees_bloquees && resolved.puissance_si_poussee > 0 {
        snapshot_for(build.class)?.sorts_qui_entravent_le_lanceur()
    } else {
        Vec::new()
    };
    let (ruleset, places_manquees) = avec_les_effets_de_dofus(ruleset, request, &resolved, &entravants)?;
    resolved.assumptions.extend(places_manquees);
    let (ruleset, notes_des_invocations) = avec_les_invocations(ruleset, request, &build, &resolved)?;
    resolved.assumptions.extend(notes_des_invocations);
    let (ruleset, notes_des_tourelles) = avec_les_tourelles(ruleset, request, &build, &resolved)?;
    resolved.assumptions.extend(notes_des_tourelles);
    let (ruleset, notes_des_pieges) = avec_les_pieges(ruleset, request, &build)?;
    resolved.assumptions.extend(notes_des_pieges);
    let mut ruleset = if build.class == crate::bombes::ROUBLARD {
        let (ruleset, notes) = avec_le_mur_de_bombes(ruleset, request, build.level, build.class)?;
        resolved.assumptions.extend(notes);
        ruleset
    } else {
        ruleset
    };
    let reach = match request.reach.as_deref() {
        Some("melee") => Reach::Melee,
        _ => Reach::Ranged,
    };
    // L'arme du build, un sort de plus que le joueur met au deck ou non.
    let maitrise = request.maitrise_d_arme.map_or(crate::armes::MAITRISE_PAR_DEFAUT, i32::from);
    if let Some(arme) = crate::armes::sort(&resolved, maitrise, reach) {
        if request.deck.iter().any(|d| d == crate::armes::ID) {
            resolved.assumptions.extend(crate::armes::hypotheses(&resolved, maitrise, reach));
        }
        ruleset.spells.push(arme);
    }
    // Les sorts temporaires des objets portés, que le joueur met au deck ou non.
    resolved.assumptions.extend(crate::sorts_d_objets::greffer(&mut ruleset, &resolved));
    // Les effets spéciaux des objets portés : tour par tour, et en réponse à
    // ce que font les sorts.
    let combat = crate::effets_d_objets::Combat {
        ennemis: request.targets.unwrap_or(1).clamp(1, MAX_ENNEMIS),
        poussees_bloquees: request.poussees_bloquees,
        pm_depenses: request.pm_depenses.unwrap_or(0).min(resolved.base_mp),
    };
    let lectures = if crate::effets_d_objets::lit_la_donnee(&build) {
        crate::effets_d_objets::Lectures::depuis(&snapshot_for(build.class)?)
    } else {
        crate::effets_d_objets::Lectures::default()
    };
    let greffes =
        crate::effets_d_objets::greffer(&mut ruleset, &build, &request.bonus_ecartes, &combat, &lectures);
    resolved.assumptions.extend(greffes.remarques);
    let mut scenario = Scenario {
        poussees_bloquees: request.poussees_bloquees,
        horizon: request.horizon.clamp(1, 20),
        // Une cible par défaut : c'est le combat que décrit le reste de la
        // page, et un joueur qui ne demande rien ne doit pas se voir servir
        // des chiffres de zone.
        targets: request.targets.unwrap_or(1).clamp(1, MAX_ENNEMIS),
        // Borne au nombre de PM que le build porte : on ne depense pas ce
        // qu'on n'a pas.
        pm_depenses: request.pm_depenses.unwrap_or(0).min(resolved.base_mp),
        // Trois cases d'écart épuisent la dégressivité de tous les sorts du
        // parc, qui retirent au plus quatre crans : la borne évite de laisser
        // croire qu'au-delà le réglage change quelque chose.
        etalement: request.etalement.unwrap_or(0).min(4),
        placement: request.placement.as_ref().map(PlacementInput::en_placement),
        etats_declares: request
            .etats_declares
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        reach,
        starting_turn_is_odd: true,
        resistance: Resistance {
            percent: request.resistance_percent.unwrap_or([0; 5]),
            flat: request.resistance_flat.unwrap_or([0; 5]),
            critical: request.resistance_critical.unwrap_or(0),
            percent_spell: request.resistance_spell.unwrap_or(0),
            percent_weapon: request.resistance_weapon.unwrap_or(0),
            percent_melee: request.resistance_melee.unwrap_or(0),
            percent_ranged: request.resistance_ranged.unwrap_or(0),
        },
        budgets: request
            .budgets
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        dominance: true,
        mode: Mode::Expected,
        prune_spells: true,
    };
    // Les Bottes du Cul Botté poussent la cible à chaque tour : ce que le
    // réglage « La cible subit des dommages de poussée à chaque tour » déclare,
    // pour les classes qui l'ont.
    if greffes.poussee_subie && ruleset.resource("poussee_subie").is_some() {
        scenario.etats_declares.retain(|(id, _)| id != "poussee_subie");
        scenario.etats_declares.push(("poussee_subie".to_string(), 1));
        resolved.assumptions.push(
            "Bottes du Cul Botté : « La cible subit des dommages de poussée à chaque tour » compté coché".to_string(),
        );
    }
    let remarques = resolved.assumptions[depart..].to_vec();
    Ok((build, resolved, ruleset, scenario, remarques))
}

/// Rejoue une rotation donnée, tour par tour, sur le build et le deck de la
/// requête : voir [`Engine::replay`]. Rien n'est élagué, un sort que la
/// recherche aurait écarté se rejoue comme un autre.
pub fn rejouer(request: &Request, tours: &[Vec<String>]) -> Result<dofus_engine::Solution, String> {
    let (build, resolved, ruleset, mut scenario, _) = preparer(request)?;
    scenario.prune_spells = false;
    let mut engine_build = to_engine_build(request, &resolved);
    engine_build.name = format!("classe {}", build.class);
    Engine::new(&ruleset, engine_build, scenario).map_err(|e| e.to_string())?.replay(tours)
}

/// Une demande de conseil : la requête de la rotation affichée, ses lancers
/// tour par tour (par le nom que la rotation leur donne), et le lancer dont on
/// veut les remplaçants.
#[derive(Debug, Deserialize)]
pub struct DemandeDeConseil {
    pub requete: Request,
    pub tours: Vec<Vec<String>>,
    pub tour: usize,
    pub lancer: usize,
}

/// Ce que chaque sort du deck vaudrait à la place d'un lancer de la rotation,
/// à cet instant : voir [`Engine::remplacants`].
pub fn conseil_json(demande: &DemandeDeConseil) -> Result<String, String> {
    let (build, resolved, ruleset, mut scenario, _) = preparer(&demande.requete)?;
    scenario.prune_spells = false;
    let mut engine_build = to_engine_build(&demande.requete, &resolved);
    engine_build.name = format!("classe {}", build.class);
    let engine = Engine::new(&ruleset, engine_build, scenario).map_err(|e| e.to_string())?;
    let remplacants = engine.remplacants(&demande.tours, demande.tour, demande.lancer)?;
    let icones: BTreeMap<&str, Option<u32>> = ruleset.spells.iter().map(|s| (s.id.as_str(), s.dofusdb_id)).collect();
    let joue = demande.tours[demande.tour][demande.lancer].clone();
    Ok(serde_json::json!({
        "joue": joue,
        "remplacants": remplacants.iter().map(|c| serde_json::json!({
            "id": c.id,
            "name": c.spell,
            "ap": c.ap_cost,
            "damage": c.damage.as_f64(),
            "dofusdb_id": icones.get(c.id.as_str()).copied().flatten(),
            "icone_objet": if c.id == crate::armes::ID {
                crate::armes::icone(&resolved)
            } else {
                crate::sorts_d_objets::icone(&resolved, &c.id)
            },
        })).collect::<Vec<_>>(),
    })
    .to_string())
}

impl Request {
    /// Les mécaniques coûteuses comptées : toutes celles de la classe, sauf
    /// liste donnée.
    fn mecaniques_de(&self, ruleset: &Ruleset) -> Vec<String> {
        self.mecaniques
            .clone()
            .unwrap_or_else(|| ruleset.costly.iter().map(|c| c.id.clone()).collect())
    }
}

pub fn run(request: &Request) -> Result<Outcome, String> {
    let (build, resolved, ruleset, scenario, remarques) = preparer(request)?;
    // Deux passes quand un palier est coché, sauf demande contraire. La
    // première cherche sans les mécaniques coûteuses : immédiate, elle dit quels
    // sorts portent la rotation. La seconde les rallume sur ces sorts-là, plus
    // ceux que les mécaniques font vivre, et cherche à nouveau : sans ces
    // derniers, un deck réduit à ce que la première a joué n'aurait aucune rune
    // à déclencher.
    let mecaniques = request.mecaniques_de(&ruleset);
    let deux_passes = !mecaniques.is_empty() && !request.exhaustif;
    let deck_affine = if deux_passes {
        let pauvre = ruleset.sans_mecaniques_couteuses();
        let mut build_pauvre = to_engine_build(request, &resolved);
        build_pauvre.name = format!("classe {}", build.class);
        let moteur = Engine::new(&pauvre, build_pauvre, scenario.clone())
            .map_err(|e| e.to_string())?;
        let Some(premiere) = moteur.solve_annulable() else {
            return Err(ANNULE.to_string());
        };
        let mut retenus: std::collections::BTreeSet<String> = premiere
            .turns
            .iter()
            .flat_map(|t| t.casts.iter())
            .map(|c| c.id.clone())
            .collect();
        retenus.extend(sorts_nourris(&ruleset, &mecaniques));
        Some(retenus)
    } else {
        None
    };
    let mut engine_build = to_engine_build(request, &resolved);
    engine_build.name = format!("classe {}", build.class);
    if let Some(retenus) = &deck_affine {
        engine_build.deck.retain(|s| retenus.contains(s));
    }
    let cibles = scenario.targets;
    // `Engine::new` consomme le scénario, et le placement se relit après le
    // solve pour dire ce qu'il a ignoré.
    let scenario_pour_ignores = scenario.clone();
    let engine = Engine::new(&ruleset, engine_build, scenario).map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    // ⚠️ LE CHEMIN ANNULABLE, et pas `solve()`. Dix tours de Huppermage passent
    // les six minutes : le joueur doit pouvoir reprendre la main, et il le fait
    // par le bouton qui appelle `demander_arret`.
    let Some(solution) = engine.solve_annulable() else {
        return Err(ANNULE.to_string());
    };
    // La boucle se calcule sur le même moteur : le graphe entre tours est
    // minuscule (une centaine d'états), donc c'est gratuit à côté du solve.
    let steady = engine.steady_state();
    let zones = zones_sans_plafond(&ruleset, &solution, cibles);
    let degressives = zones_degressives(&ruleset, &solution, cibles);
    // Restreint aux sorts que la rotation joue vraiment : lister la forme non
    // dessinée d'un sort qui n'est pas dans le deck serait du bruit.
    let joues: std::collections::BTreeSet<&str> = solution
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .map(|c| c.id.as_str())
        .collect();
    let ignores: Vec<String> = dofus_engine::Cibles::depuis(&scenario_pour_ignores)
        .placement_ignore(&ruleset)
        .into_iter()
        .filter(|libelle| {
            ruleset
                .spells
                .iter()
                .any(|s| joues.contains(s.id.as_str()) && libelle.starts_with(&s.name.fr))
        })
        .collect();
    Ok(Outcome {
        resolved,
        remarques,
        steady,
        solution,
        ecartes: engine
            .pruned()
            .iter()
            .map(|(s, r)| (s.clone(), r.clone()))
            .collect(),
        zones_sans_plafond: zones,
        zones_degressives: degressives,
        placement_ignore: ignores,
        seconds: started.elapsed().as_secs_f64(),
    })
}

/// Les sorts joués dont les dégâts de zone décroissent avec la distance
/// (`falloff_percent` par cran d'éloignement, au plus `falloff_steps` fois),
/// pour que le joueur sache quels totaux de zone en dépendent ; vide sur une
/// seule cible. Un sort dont le texte dit ses dommages de zone non dégressifs est
/// écarté : le texte prime sur la donnée.
fn zones_degressives(
    ruleset: &dofus_ruleset::Ruleset,
    solution: &dofus_engine::Solution,
    cibles: u8,
) -> Vec<String> {
    if cibles <= 1 {
        return Vec::new();
    }
    let joues: std::collections::BTreeSet<&str> = solution
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .map(|c| c.id.as_str())
        .collect();
    let mut out: Vec<String> = ruleset
        .spells
        .iter()
        .filter(|s| joues.contains(s.id.as_str()))
        .filter(|s| {
            // Le texte du sort prime sur la donnee.
            let dit_non = s.note.as_deref().is_some_and(|n| {
                let n = n.to_lowercase();
                n.contains("ne sont pas dégressif") || n.contains("non dégressif")
            });
            !dit_non
                && s.lines.iter().any(|l| {
                    l.area.as_ref().is_some_and(|a| {
                        a.shape != 'P' && a.falloff_percent > 0 && a.falloff_steps > 0
                    })
                })
        })
        .map(|s| s.name.fr.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

fn zones_sans_plafond(
    ruleset: &dofus_ruleset::Ruleset,
    solution: &dofus_engine::Solution,
    cibles: u8,
) -> Vec<String> {
    if cibles <= 1 {
        return Vec::new();
    }
    let joues: std::collections::BTreeSet<&str> = solution
        .turns
        .iter()
        .flat_map(|t| t.casts.iter())
        .map(|c| c.id.as_str())
        .collect();
    let mut out: Vec<String> = ruleset
        .spells
        .iter()
        .filter(|s| joues.contains(s.id.as_str()))
        .filter(|s| {
            s.lines.iter().any(|l| {
                l.area
                    .as_ref()
                    .is_some_and(|a| a.max_targets.is_none() && a.shape != 'P')
            })
        })
        .map(|s| s.name.fr.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The spell list a chooser needs, since no import can supply it.
pub fn classes_json() -> Result<String, String> {
    let mut classes = Vec::new();
    for (id, _, label) in CLASSES {
        let ruleset = load_ruleset(*id)?;
        let snapshot = snapshot_for(*id)?;
        let variants: BTreeMap<u32, Option<u32>> = snapshot
            .spells
            .iter()
            .map(|s| (s.id, s.variant_group))
            .collect();
        // Ce que le sort fait, dans les mots du jeu. Le ruleset dit comment il
        // se comporte pour le solveur ; cette phrase-là dit au joueur ce qu'il
        // lit en jeu, et c'est celle qui lui parle.
        let descriptions: BTreeMap<u32, String> = snapshot
            .spells
            .iter()
            .filter_map(|s| s.description_fr.clone().map(|d| (s.id, d)))
            .collect();
        let spells: Vec<_> = ruleset
            .spells
            .iter()
            .chain(sorts_communs())
            .map(|s| {
                // Les effets dont la magnitude change sur un coup critique : le
                // joueur doit savoir lequel des deux chiffres s'applique à son
                // build, le solveur n'en retenant qu'un.
                let mut critical_effects: Vec<serde_json::Value> = Vec::new();
                for effect in &s.effects {
                    if let dofus_ruleset::Effect::Gain { resource, .. } = effect {
                        if let Some(r) = ruleset.resource(resource) {
                            for m in &r.modifies_damage {
                                use dofus_ruleset::DamageModifier as D;
                                let (label, normal, crit) = match m {
                                    // Sans élément c'est de la Puissance, avec
                                    // c'est un vol de la caractéristique de cet
                                    // élément : le libellé doit le dire.
                                    D::Characteristic {
                                        amount,
                                        critical_amount,
                                        element,
                                        // Le libelle du bonus ne dit pas le
                                        // nombre de cibles ni les PM depenses :
                                        // c'est le moteur qui les applique, pas
                                        // cette liste.
                                        per_target: _,
                                        per_mp_used: _,
                                        per_target_cap: _,
                                    } => (
                                        match element {
                                            None => "Puissance",
                                            Some(dofus_damage::Element::Fire) => "Intelligence",
                                            Some(dofus_damage::Element::Earth) => "Force",
                                            Some(dofus_damage::Element::Air) => "Agilité",
                                            Some(dofus_damage::Element::Water) => "Chance",
                                            Some(dofus_damage::Element::Neutral) => "Force",
                                        },
                                        amount.known().copied(),
                                        *critical_amount,
                                    ),
                                    D::BaseDamage {
                                        amount,
                                        critical_amount,
                                    } => (
                                        "dégâts de base",
                                        amount.known().copied(),
                                        *critical_amount,
                                    ),
                                    D::FlatDamage {
                                        amount,
                                        critical_amount,
                                    } => ("Dommages", amount.known().copied(), *critical_amount),
                                    D::FinalMultiplier {
                                        percent,
                                        critical_percent,
                                        finaux,
                                    } => (
                                        // Deux statistiques s'écrivent ainsi :
                                        // les % finaux du lanceur, et les
                                        // dommages subis par la cible.
                                        if *finaux { "% dommages finaux" } else { "Dommages subis" },
                                        percent.known().copied().map(|p| p as i32),
                                        critical_percent.map(|p| p as i32),
                                    ),
                                    D::CriticalRate {
                                        percent,
                                        critical_percent,
                                    } => (
                                        "% de coup critique",
                                        percent.known().copied(),
                                        *critical_percent,
                                    ),
                                    D::CriticalDamage {
                                        amount,
                                        critical_amount,
                                    } => (
                                        "Dommages Critiques",
                                        amount.known().copied(),
                                        *critical_amount,
                                    ),
                                    D::CriticalResistance {
                                        amount,
                                        critical_amount,
                                    } => (
                                        "Résistance Critique retirée",
                                        amount.known().copied(),
                                        *critical_amount,
                                    ),
                                    D::PushDamage {
                                        amount,
                                        critical_amount,
                                    } => (
                                        "Dommages Poussée",
                                        amount.known().copied(),
                                        *critical_amount,
                                    ),
                                    // Sans valeur critique : la liste ne le
                                    // montre pas.
                                    D::DomainPercent { percent, .. } => ("% de dommages", Some(*percent), None),
                                };
                                if let (Some(n), Some(c)) = (normal, crit) {
                                    critical_effects.push(serde_json::json!({
                                        "label": label, "normal": n, "critical": c,
                                    }));
                                }
                            }
                        }
                    }
                }
                // Le nom d'une ressource tel que la bulle l'écrit : celui du
                // jeu, sinon son identifiant en clair.
                let nom = |id: &str| {
                    ruleset
                        .resource(id)
                        .and_then(|r| r.name.as_ref())
                        .map_or_else(
                            || {
                                let clair = id.replace('_', " ");
                                let mut lettres = clair.chars();
                                lettres
                                    .next()
                                    .map(|p| p.to_uppercase().collect::<String>() + lettres.as_str())
                                    .unwrap_or_default()
                            },
                            |n| n.fr.clone(),
                        )
                };
                for line in &s.lines {
                    for b in &line.base_bonus {
                        use dofus_ruleset::BaseBonus as B;
                        // Ce qui multiplie le bonus se dit :
                        // « Dégâts de base 15 » seul laisserait
                        // deviner s'il compte par PM, par cible ou
                        // sous un état.
                        let suite = match b {
                            B::PerResource { resource, .. } | B::PerResourceGated { resource, .. } => {
                                format!("par {}", nom(resource))
                            }
                            B::WhileResource { resource, .. } => format!("avec {}", nom(resource)),
                            B::PerMpUsed { .. } => "par PM utilisé".to_string(),
                            B::PerExtraTarget { .. } => "par cible en plus".to_string(),
                            B::PerExtraTargetWhile { resource, .. } => {
                                format!("par cible en plus avec {}", nom(resource))
                            }
                            B::Steps { .. } => String::new(),
                        };
                        let (n, c) = match b {
                            B::PerResource {
                                amount,
                                critical_amount,
                                ..
                            }
                            | B::WhileResource {
                                amount,
                                critical_amount,
                                ..
                            }
                            | B::PerExtraTarget {
                                amount,
                                critical_amount,
                                ..
                            }
                            | B::PerMpUsed {
                                amount,
                                critical_amount,
                                ..
                            }
                            | B::PerExtraTargetWhile {
                                amount,
                                critical_amount,
                                ..
                            }
                            | B::PerResourceGated {
                                amount,
                                critical_amount,
                                ..
                            } => (*amount, *critical_amount),
                            // Les paliers portent leurs valeurs dans la liste,
                            // pas un couple normal/critique.
                            B::Steps { .. } => (0, None),
                        };
                        if let Some(c) = c {
                            critical_effects.push(serde_json::json!({
                                "label": "dégâts de base", "normal": n, "critical": c, "suite": suite,
                            }));
                        }
                    }
                }

                // L'élément, pas son `Option` : `format!("{:?}", l.element)`
                // rendrait « Some(Fire) » et « None », que la page compare
                // à « Feu ».
                let nom = |l: &dofus_ruleset::LineDef| l.element.map(|e| format!("{e:?}"));
                let mut elements: Vec<String> = s.lines.iter().filter_map(nom).collect();
                for effect in &s.effects {
                    match effect {
                        dofus_ruleset::Effect::Schedule { payload, .. } => {
                            elements.extend(payload.iter().filter_map(nom))
                        }
                        dofus_ruleset::Effect::Gain { resource, .. } => {
                            if let Some(r) = ruleset.resource(resource) {
                                elements.extend(
                                    r.while_present
                                        .iter()
                                        .flat_map(|e| e.lines.iter())
                                        .filter_map(nom),
                                );
                            }
                        }
                        _ => {}
                    }
                }
                elements.sort();
                elements.dedup();

                // Ce que lit « Limiter à l'orientation » : les éléments où le
                // sort frappe vraiment, pièges et invocations compris, s'il
                // frappe dans le meilleur élément du lanceur, et s'il agit dans
                // le modèle. Les pièges n'ont pas de ligne dans les règles et
                // les invocations frappent par leur relevé : sans eux, ils
                // passeraient le filtre comme des sorts sans élément.
                let gagnees: Vec<&dofus_ruleset::LineDef> = s
                    .effects
                    .iter()
                    .filter_map(|e| match e {
                        dofus_ruleset::Effect::Gain { resource, .. } => ruleset.resource(resource),
                        _ => None,
                    })
                    .flat_map(|r| r.while_present.iter().flat_map(|e| e.lines.iter()))
                    .collect();
                let toutes: Vec<&dofus_ruleset::LineDef> = s
                    .lines
                    .iter()
                    .chain(&s.trap_lines)
                    .chain(s.modes.iter().flat_map(|m| m.lines.iter()))
                    .chain(s.effects.iter().flat_map(|e| match e {
                        dofus_ruleset::Effect::Schedule { payload, .. } => payload.iter().collect::<Vec<_>>(),
                        _ => Vec::new(),
                    }))
                    .chain(gagnees)
                    .collect();
                let mut degats: Vec<String> = toutes.iter().filter_map(|l| nom(l)).collect();
                // La Concentration de Chakra vole dans le meilleur élément à
                // chaque piège déclenché : la greffe des pièges le lit.
                let mut meilleur = toutes.iter().any(|l| l.best_element)
                    || s.effects.iter().any(|e| {
                        matches!(e, dofus_ruleset::Effect::Gain { resource, .. } if resource == "concentration_de_chakra")
                    });
                let piege = (s.lines.is_empty() && s.effects.is_empty())
                    .then(|| s.dofusdb_id.and_then(|d| crate::reseau::lignes_au_centre(&snapshot, d)))
                    .flatten();
                if let Some((frappent, poisons)) = &piege {
                    degats.extend(frappent.iter().chain(poisons).map(|(e, _)| format!("{e:?}")));
                }
                let releve = crate::invocations::releve();
                let invocation = s.dofusdb_id.and_then(|d| {
                    releve.invocations.iter().find(|v| v.sort == d && (v.classe == *id || v.classe == 0))
                });
                if let Some(inv) = invocation {
                    let autre = inv
                        .autre
                        .and_then(|a| releve.invocations.iter().find(|v| v.classe == 0 && v.monstre == Some(a.monstre)));
                    for v in std::iter::once(inv).chain(autre) {
                        let (es, m) = crate::invocations::elements_de(v, 200);
                        degats.extend(es.iter().map(|e| format!("{e:?}")));
                        meilleur |= m;
                    }
                }
                degats.sort();
                degats.dedup();
                let agit = !s.lines.is_empty()
                    || !s.effects.is_empty()
                    || !s.trap_lines.is_empty()
                    || !s.modes.is_empty()
                    || piege.is_some()
                    || invocation.is_some();
                serde_json::json!({
                    "id": s.id,
                    "dofusdb_id": s.dofusdb_id,
                    "orientation": { "elements": degats, "meilleur": meilleur, "agit": agit },
                    "variant_group": s.dofusdb_id.and_then(|d| {
                        variants.get(&d).copied().flatten().or_else(|| {
                            VARIANTES_COMMUNES.iter().find(|(id, _)| *id == d).map(|(_, g)| *g)
                        })
                    }),
                    "name": s.name.fr,
                    "ap": s.ap_cost.base,
                    "casts_per_turn": s.casts_per_turn,
                    "cooldown": s.cooldown_turns,
                    "elements": elements,
                    "open_questions": s.open_questions,
                    "assumptions": s.assumptions,
                    // De quoi bâtir l'infobulle côté page. Le taux de critique
                    // est donné en base : le taux appliqué dépend du bonus
                    // critique du build importé, que la page connaît. Un sort
                    // commun n'est pas dans l'instantané de la classe : sa note
                    // dit ce qu'il invoque.
                    "description": s.dofusdb_id.and_then(|d| descriptions.get(&d)).cloned().or_else(|| {
                        sorts_communs().iter().any(|c| c.id == s.id).then(|| s.note.clone()).flatten()
                    }),
                    "critical_effects": critical_effects,
                    "crit_base": s.crit.base_rate.known().copied(),
                    "can_crit": s.crit.can_crit,
                    "range": s.range,
                    "cast": s.cast,
                    "casts_per_target": s.casts_per_target,
                    "lines": s.lines.iter().map(|l| serde_json::json!({
                        // `Fire`, ou l'élément que le build désigne : un
                        // `Some(Fire)` s'affichait tel quel dans la fiche.
                        "element": match l.element {
                            Some(e) => format!("{e:?}"),
                            None if l.worst_element => "worst".into(),
                            None => "best".into(),
                        },
                        "normal": l.normal.known().copied(),
                        "critical": l.critical.known().copied(),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();

        let budgets: Vec<_> = ruleset
            .budgets
            .iter()
            .map(|b| {
                serde_json::json!({
                    "id": b.id, "name": b.name.fr, "default": b.default,
                    "max": b.max, "note": b.note,
                })
            })
            .collect();

        // Combien de cette classe est réellement modélisé, mesuré contre
        // l'instantané à chaque appel : une classe à moitié faite est
        // annoncée, pas cachée.
        let coverage = snapshot.coverage(&ruleset);
        classes.push(serde_json::json!({
            "id": id, "label": label, "spells": spells, "budgets": budgets,
            // Les compteurs que le joueur renseigne lui-même sur cette classe.
            // La page en fait un champ chacun : elle ne les connaît pas
            // d'avance, elle les DÉCOUVRE ici.
            "etats_declares": ruleset
                .resources
                .iter()
                .filter(|r| r.declared_by_player)
                .map(|r| serde_json::json!({
                    "id": r.id,
                    "label": r.player_label.clone().unwrap_or_else(|| r.id.clone()),
                    "max": r.max,
                    // La valeur de départ du champ : « Vie restante (%) » part
                    // de 100, pas de zéro.
                    "defaut": r.default,
                    "aide": r.note.clone().unwrap_or_default(),
                }))
                .collect::<Vec<_>>(),
            // Les mécaniques coûteuses de cette classe, comptées d'office : la
            // page n'en tire que la case du calcul exhaustif. Vide pour quinze
            // classes sur dix-neuf.
            "couteuses": ruleset
                .costly
                .iter()
                .map(|c| serde_json::json!({
                    "id": c.id,
                    "label": c.name.fr,
                    "aide": c.note,
                    "defaut": c.default_on,
                }))
                .collect::<Vec<_>>(),
            "coverage": {
                "complete": coverage.is_complete(),
                // `modelled` dit que les chiffres sont là ; `fully_modelled`
                // que le sort est compris. Le premier seul annoncerait 100 %
                // pour une classe dont aucun sort n'est complet.
                "modelled": coverage.modelled,
                "fully_modelled": coverage.fully_modelled,
                "damaging": coverage.damaging,
                "percent": coverage.percent(),
                "percent_complete": coverage.percent_complete(),
                "missing": coverage.missing,
                "partial": coverage.partial.iter().map(|(nom, n)| serde_json::json!({
                    "name": nom, "gaps": n,
                })).collect::<Vec<_>>(),
            },
        }));
    }
    // Les limites partent avec le catalogue, chargé une fois au
    // démarrage : l'interface les lit au lieu de les recopier.
    Ok(serde_json::json!({
        "classes": classes,
        "limites": {
            "ennemis": MAX_ENNEMIS,
            "critique_certain": dofus_engine::CRITIQUE_CERTAIN,
        },
    })
    .to_string())
}

// ---------------------------------------------------------------------------
// La fiche du build
// ---------------------------------------------------------------------------
//
// Quatre encarts de chiffres, chacun en deux colonnes, dans l'ordre de la page
// d'équipement de DofusBook. « PP » et « PI » s'écrivent en entier : sans
// l'icône de leur page, ils ne se lisent pas.

type Colonne = &'static [(&'static str, &'static str)];

const ENCART_IDENTITE: [Colonne; 2] = [
    &[("pv", "PdV"), ("pp", "Prospection"), ("pa", "PA"), ("pm", "PM"), ("po", "PO")],
    &[("ii", "Initiative"), ("cc", "Critique"), ("ic", "Invocations"), ("so", "Soin")],
];
const ENCART_MOBILITE: [Colonne; 2] = [
    &[("fu", "Fuite"), ("epa", "Esq. PA"), ("epm", "Esq. PM"), ("pd", "Pods")],
    &[("ta", "Tacle"), ("rpa", "Ret. PA"), ("rpm", "Ret. PM")],
];
const ENCART_DOMMAGES: [Colonne; 2] = [
    &[
        ("dnf", "Do Neutre"), ("dtf", "Do Terre"), ("dff", "Do Feu"),
        ("def", "Do Eau"), ("daf", "Do Air"), ("dmg", "Dommages"),
    ],
    &[
        ("dc", "Do Critique"), ("dp", "Do Poussée"), ("dw", "% Do Armes"),
        ("ds", "% Do Sorts"), ("dm", "% Do Mêlée"), ("dd", "% Do Dist"),
    ],
];
const ENCART_RESISTANCES: [Colonne; 2] = [
    &[
        ("rn", "Ré Neutre"), ("rt", "Ré Terre"), ("rf", "Ré Feu"), ("re", "Ré Eau"),
        ("ra", "Ré Air"), ("rc", "Ré Critique"), ("rm", "% Ré Mêlée"), ("rw", "% Ré Armes"),
    ],
    &[
        ("rnp", "% Ré Neutre"), ("rtp", "% Ré Terre"), ("rfp", "% Ré Feu"),
        ("rep", "% Ré Eau"), ("rap", "% Ré Air"), ("rp", "Ré Poussée"), ("rd", "% Ré Dist"),
    ],
];

/// Les caractéristiques : code DofusBook, libellé, et si la Puissance s'y
/// ajoute pour les dégâts (la colonne « + Puissance » : la Force à 1060
/// frappe comme 1170 avec 110 de Puissance).
const CARACTERISTIQUES: [(&str, &str, bool); 6] = [
    ("vi", "Vitalité", false),
    ("sa", "Sagesse", false),
    ("fo", "Force", true),
    ("in", "Intelligence", true),
    ("ch", "Chance", true),
    ("ag", "Agilité", true),
];

/// Compose la fiche à partir de ce que DofusBook a calculé.
fn fiche_dofusbook(input: &BuildInput, dofusbook: &serde_json::Value) -> serde_json::Value {
    let stats = &dofusbook["stats"];
    let details = &dofusbook["details"];
    // Une statistique absente vaut zéro : DofusBook affiche « 0 Soin », pas un
    // trou. Le calcul n'écrit pas les totaux nuls, l'affichage les rétablit.
    let valeur = |code: &str| Some(stats[code].as_f64().unwrap_or(0.0));
    let ligne = |code: &str, label: &str| {
        serde_json::json!({
            "label": label,
            "value": valeur(code),
            // La provenance, telle que DofusBook la détaille : « Capital 300,
            // Parchemin 100, Corne de Torkélonia 70 ».
            "detail": details[code].clone(),
        })
    };
    let encart = |colonnes: &[Colonne; 2]| -> serde_json::Value {
        colonnes
            .iter()
            .map(|c| c.iter().map(|(code, label)| ligne(code, label)).collect::<Vec<_>>())
            .collect::<Vec<_>>()
            .into()
    };

    // Le capital et le parchemin viennent de la charge d'origine, ni du
    // détail ni de la charge normalisée : `normalise` replie `carac` dans
    // `invested` puis le vide, pour que le résolveur ne compte pas deux fois
    // les mêmes points.
    let carac = if input.carac.is_empty() {
        input.raw_stuff.as_ref().map(|r| &r.stuff_carac).unwrap_or(&input.carac)
    } else {
        &input.carac
    };
    let puissance = valeur("pu").unwrap_or(0.0);
    let caracteristiques: Vec<_> = CARACTERISTIQUES
        .iter()
        .map(|(code, label, avec_puissance)| {
            let total = valeur(code);
            serde_json::json!({
                "label": label,
                "value": total,
                "puissance": if *avec_puissance { total.map(|t| t + puissance) } else { None },
                "base": carac.get(&format!("base_{code}")),
                "parcho": carac.get(&format!("scroll_{code}")),
                "detail": details[*code].clone(),
            })
        })
        .collect();

    serde_json::json!({
        "identite": encart(&ENCART_IDENTITE),
        "caracteristiques": caracteristiques,
        "puissance": ligne("pu", "Puissance"),
        "mobilite": encart(&ENCART_MOBILITE),
        "dommages": encart(&ENCART_DOMMAGES),
        "resistances": encart(&ENCART_RESISTANCES),
        "dommages_finaux": valeur("deg"),
    })
}

/// Les chiffres où notre calcul et celui de DofusBook divergent : si notre
/// catalogue a vieilli sur un objet de ce build, le joueur le lit sur sa fiche
/// plutôt que sur un total faux. Seules les lignes qui entrent dans un calcul de
/// dégâts sont comparées.
fn ecarts_avec_dofusbook(
    r: &Resolved,
    dofusbook: &serde_json::Value,
    fiche: &crate::fiche::Fiche,
) -> Vec<serde_json::Value> {
    let stats = &dofusbook["stats"];
    // Les dommages d'un Dofus porté sans son bonus dans DofusBook se retirent :
    // l'Harmonie de Pandala du Tacheté est comptée ici, pas dans leur fiche, et
    // la fiche le dit déjà dans ce qui est supposé.
    let dommages = |i: usize| f64::from(r.profile.elements[i].flat_damage - r.dommages_des_dofus_portes);
    // Indices du profil : Feu, Terre, Air, Eau, Neutre.
    let paires: [(&str, &str, f64); 14] = [
        ("fo", "Force", f64::from(r.profile.elements[1].characteristic)),
        ("in", "Intelligence", f64::from(r.profile.elements[0].characteristic)),
        ("ag", "Agilité", f64::from(r.profile.elements[2].characteristic)),
        ("ch", "Chance", f64::from(r.profile.elements[3].characteristic)),
        ("dtf", "Do Terre", dommages(1)),
        ("dff", "Do Feu", dommages(0)),
        ("daf", "Do Air", dommages(2)),
        ("def", "Do Eau", dommages(3)),
        ("dnf", "Do Neutre", dommages(4)),
        ("dc", "Do Critique", f64::from(r.profile.flat_crit_damage)),
        ("cc", "Critique", f64::from(r.crit_bonus_percent)),
        ("pu", "Puissance", f64::from(r.profile.power)),
        ("pa", "PA", f64::from(r.base_ap)),
        ("pm", "PM", f64::from(r.base_mp)),
    ];
    paires
        .iter()
        .filter_map(|(code, label, nous)| {
            // Une ligne touchée par un sort de classe ne se compare pas :
            // DofusBook compte le Prélude au Fer du Forgelance dans la
            // Puissance, notre moteur le lance lui-même. L'écart ne dirait
            // rien d'un catalogue vieilli, seule chose que ce contrôle cherche.
            if fiche.touchees_par_un_sort.iter().any(|c| c == code) {
                return None;
            }
            let eux = stats[*code].as_f64()?;
            ((eux - nous).abs() > 0.5).then(|| {
                serde_json::json!({ "label": label, "dofusbook": eux, "krozties": nous })
            })
        })
        .collect()
}

/// The build alone, with the elemental orientation it implies: a player picks
/// the orientation first and tunes a rotation around it, so the equipment
/// resolves before any spell is chosen.
pub fn resolve_json(input: &BuildInput) -> Result<String, String> {
    // La page ne doit rien interpréter de la charge : elle change de forme au
    // gré de DofusBook, et une page qui la lit se casse en silence à chaque
    // fois. Tout ce qu'elle affiche vient d'ici.
    let normalise = input.normalise();
    let mut r = resolve_build(&normalise)?;
    // Les objets que la dernière note de sortie change, pas encore relevés.
    r.assumptions.extend(crate::donnees::objets_a_relever(&normalise.items));
    let names = ["Feu", "Terre", "Air", "Eau", "Neutre"];
    let elements: Vec<_> = (0..5)
        .map(|i| {
            serde_json::json!({
                "name": names[i],
                "characteristic": r.profile.elements[i].characteristic,
                "flat_damage": r.profile.elements[i].flat_damage,
            })
        })
        .collect();

    // Whatever carries at least half of the highest characteristic counts as
    // part of the build's orientation. A tri-element build keeps three, a mono
    // keeps one, and nothing has to be declared by hand.
    let top = (0..5)
        .map(|i| r.profile.elements[i].characteristic)
        .max()
        .unwrap_or(0);
    let dominant: Vec<&str> = (0..5)
        .filter(|i| top > 0 && r.profile.elements[*i].characteristic * 2 >= top)
        .map(|i| names[i])
        .collect();

    // La fiche : ce que DofusBook montre, relu depuis NOTRE catalogue. Le
    // relire coûte quelques millisecondes sur quatre mille cinq cents objets,
    // et c'est le prix de n'avoir qu'une source pour les chiffres.
    let catalogue = Catalogue::from_json(crate::donnees::OBJETS)?;
    let par_id: BTreeMap<u32, &dofus_build::Item> =
        catalogue.items.iter().map(|i| (i.id, i)).collect();
    // La table des icônes est à côté du catalogue, pas dedans : son
    // absence dégrade l'affichage et ne casse rien.
    let icones: BTreeMap<String, u32> =
        serde_json::from_str(crate::donnees::ICONES_D_OBJETS).unwrap_or_default();
    let fiche: Vec<_> = BuildInput::SLOTS
        .iter()
        .enumerate()
        .map(|(i, slot)| {
            let id = normalise.items.get(i).copied().unwrap_or(0);
            let objet = par_id.get(&id);
            let forgemagie: Vec<_> = normalise
                .forgemagic
                .get(*slot)
                .map(|m| {
                    m.iter()
                        .map(|(code, valeur)| {
                            let (label, compte) = dofus_build::libelle_forgemagie(code);
                            serde_json::json!({
                                "label": label, "amount": valeur, "compte": compte,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let (rang, section) = dofus_build::section_emplacement(slot);
            serde_json::json!({
                "slot": slot,
                "label": dofus_build::libelle_emplacement(slot),
                "section": section,
                "rang": rang,
                "icon": icones.get(&id.to_string()),
                "id": id,
                "name": objet.map(|o| o.name.clone()),
                "level": objet.map(|o| o.level),
                "set": objet.and_then(|o| o.set_id).and_then(|s| {
                    catalogue.sets.iter().find(|x| x.id == s).map(|x| x.name.clone())
                }),
                "stats": objet
                    .map(|o| {
                        o.stats
                            .iter()
                            .map(|(nom, [min, max])| {
                                serde_json::json!({
                                    "label": dofus_build::libelle_stat(nom),
                                    "min": min, "max": max,
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
                "forgemagie": forgemagie,
                // ⚠️ UN OBJET INCONNU N'EST PAS UN EMPLACEMENT VIDE. Le premier
                // est une lacune du catalogue dont les statistiques manquent au
                // total ; le second est un choix du joueur.
                "inconnu": id != 0 && objet.is_none(),
                "non_interprete": objet.is_some_and(|o| !o.unmapped_effect_ids.is_empty()),
                // Ce que l'objet fait hors du combat, et ses sorts : la fiche
                // de l'objet au survol.
                "infos": objet.map(|o| o.infos.clone()).unwrap_or_default(),
                "effets": objet.map(|o| crate::sorts_d_objets::sur_la_fiche(o, &normalise.items)).unwrap_or_default(),
                // « Lapement : +35 % Critique », comme l'objet l'écrit en jeu.
                "sorts": objet.map(|o| o.spell_modifiers.iter().map(|m| {
                    let sort = crate::objets_de_classe::nom_du_sort(m.spell)
                        .map_or_else(|| format!("sort {}", m.spell), str::to_string);
                    format!("{sort} : {}", m.libelle())
                }).collect::<Vec<_>>()).unwrap_or_default(),
            })
        })
        .collect();

    // « Exo / Over des items » chez DofusBook : la forgemagie de tous les
    // emplacements, sommée par ligne.
    let mut forgemagie_totale: BTreeMap<String, (i32, bool)> = BTreeMap::new();
    for champ in normalise.forgemagic.values() {
        for (code, valeur) in champ {
            let (label, compte) = dofus_build::libelle_forgemagie(code);
            let entree = forgemagie_totale.entry(label.to_string()).or_insert((0, compte));
            entree.0 += valeur;
        }
    }
    let forgemagie_totale: Vec<_> = forgemagie_totale
        .into_iter()
        .filter(|(_, (v, _))| *v != 0)
        .map(|(label, (valeur, compte))| {
            serde_json::json!({ "label": label, "value": valeur, "compte": compte })
        })
        .collect();

    let investies: Vec<_> = normalise
        .invested
        .iter()
        .filter(|(_, v)| **v != 0)
        .map(|(nom, v)| serde_json::json!({ "label": dofus_build::libelle_stat(nom), "value": v }))
        .collect();
    // Groupées par famille et dans l'ordre du jeu : vingt-huit lignes d'affilée
    // obligent à tout parcourir pour trouver une résistance.
    let mut totaux: Vec<_> = r
        .totals
        .iter()
        .filter(|(_, v)| **v != 0)
        .map(|(nom, v)| {
            let (rang, famille) = dofus_build::famille_stat(nom);
            (rang, dofus_build::libelle_stat(nom).to_string(), famille, *v)
        })
        .collect();
    totaux.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let totaux: Vec<_> = totaux
        .into_iter()
        .map(|(rang, label, famille, valeur)| {
            serde_json::json!({ "label": label, "value": valeur, "famille": famille, "rang": rang })
        })
        .collect();

    // La fiche calculée comme DofusBook la calcule, depuis ses variables.
    let calculee = input.dofusbook.as_ref().map(|variables| {
        let fiche = crate::fiche::calculer(variables, &input.boosts);
        let json = fiche.en_json();
        (fiche, json)
    });

    // La classe se juge ICI, pas au moment de résoudre la rotation : l'équipement
    // vient d'être lu, le joueur est à l'étape 2, et lui dire non maintenant lui
    // épargne de choisir des sorts et de décrire une cible pour rien.
    let supported = ruleset_for(normalise.class).is_some();

    // Les dégâts que chaque sort inflige réellement sur ce build, palier par
    // palier, et non le jet de base, que le joueur ne voit jamais.
    let tables = if supported {
        table_de_degats(&normalise, &r).unwrap_or_default()
    } else {
        serde_json::Map::new()
    };

    // Ce que les objets de classe changent aux sorts, pour que les infobulles
    // montrent le sort tel que le build le lance.
    let sorts_modifies = if supported { sorts_modifies(normalise.class, &r) } else { serde_json::Map::new() };

    Ok(serde_json::json!({
        "damage_tables": tables,
        "sorts_modifies": sorts_modifies,
        // L'arme du build, avec les champs d'un sort du catalogue : le deck de
        // la Rotation et l'infobulle la lisent comme les autres.
        "arme": if supported { crate::armes::fiche(&r) } else { None },
        // Les sorts temporaires des objets, de même.
        "sorts_d_objets": if supported { crate::sorts_d_objets::fiches(&r) } else { Vec::new() },
        "class": normalise.class,
        "class_name": class_name(normalise.class),
        "class_supported": supported,
        "class_message": if supported { serde_json::Value::Null }
            else { serde_json::json!(unsupported_class_message(normalise.class)) },
        "level": normalise.level,
        // Les emplacements POURVUS, et parmi eux ceux que le catalogue n'a pas
        // su résoudre. Compter les seconds avec les premiers annonce un stuff
        // complet là où la résolution s'est faite sur un personnage nu.
        "items_equipped": normalise.items.iter().filter(|i| **i != 0).count(),
        "items_missing": r.items_missing.len(),
        // La liste, pas seulement le compte : elle dit quel objet manque à
        // un import.
        "items_missing_ids": r.items_missing,
        "ap": r.base_ap,
        "mp": r.base_mp,
        "crit": r.crit_bonus_percent,
        "power": r.profile.power,
        "crit_damage": r.profile.flat_crit_damage,
        "percent_spell": r.profile.percent_spell,
        "percent_weapon": r.profile.percent_weapon,
        "percent_melee": r.profile.percent_melee,
        "percent_ranged": r.profile.percent_ranged,
        "elements": elements,
        "dominant": dominant,
        "multipliers": r.damage_multipliers.iter().map(|(n, p)| {
            serde_json::json!({ "name": n, "percent": p })
        }).collect::<Vec<_>>(),
        // Les bonus de Dofus qui dépendent du combat, ceux des Dofus portés :
        // l'onglet Rotation leur donne une case pour les écarter du calcul.
        "bonus_conditionnels": r.bonus_conditionnels.iter().map(|b| {
            serde_json::json!({ "nom": b.nom, "effet": b.effet, "condition": b.condition, "coche": b.coche })
        }).collect::<Vec<_>>(),
        "sets": r.sets_active.iter().map(|(n, c)| {
            serde_json::json!({ "name": n, "pieces": c })
        }).collect::<Vec<_>>(),
        "assumptions": r.assumptions,
        "uninterpreted": r.uninterpreted,
        // La fiche du build, emplacement par emplacement : son objet, son
        // niveau, ses lignes et sa forgemagie.
        "stuff": fiche,
        "forgemagie_totale": forgemagie_totale,
        "invested": investies,
        "totals": totaux,
        // Présentes seulement quand l'import a transmis les variables de
        // DofusBook : une charge écrite à la main n'en porte pas.
        "fiche_dofusbook": calculee.as_ref().map(|(_, json)| fiche_dofusbook(input, json)),
        "ecarts": calculee.as_ref().map(|(fiche, json)| ecarts_avec_dofusbook(&r, json, fiche)),
    })
    .to_string())
}

/// Les dégâts par tour pour un ennemi, deux, trois, jusqu'à `jusqu_a`. Une
/// résolution complète par valeur : la rotation change avec le nombre
/// d'ennemis, et multiplier le total ne donnerait qu'une règle de trois.
fn balayage_cibles(request: &Request, jusqu_a: u8) -> Vec<serde_json::Value> {
    (1..=jusqu_a.clamp(1, MAX_ENNEMIS))
        .map(|cibles| {
            let mut r = request.clone();
            r.targets = Some(cibles);
            r.targets_sweep = None;
            match run(&r) {
                Ok(o) => serde_json::json!({
                    "targets": cibles,
                    "per_turn": o.steady.per_turn_value(),
                    "total": o.solution.total.as_f64(),
                    // La rotation elle-même, en identifiants : c'est ce qui
                    // permet de dire au joueur QUOI change, et pas seulement
                    // que le chiffre monte.
                    "cycle": o.steady.cycle.iter()
                        .flat_map(|t| t.casts.iter().map(|c| c.id.clone()))
                        .collect::<Vec<_>>(),
                }),
                // Un point qui échoue se dit, il ne disparaît pas : une courbe
                // à trous ressemblerait à un plateau.
                Err(e) => serde_json::json!({ "targets": cibles, "error": e }),
            }
        })
        .collect()
}

pub fn solve_json(request: &Request) -> Result<String, String> {
    let outcome = run(request)?;
    let balayage = request.targets_sweep.map(|n| balayage_cibles(request, n));
    // Spell id to `dofusdb_id`, so the timeline can show an icon per cast;
    // common spells included, for their name and icon.
    let mut ruleset = load_ruleset(request.build.normalise().class)?;
    ruleset.spells.extend(sorts_communs().iter().cloned());
    // L'arme, pour son nom ; son icône est celle de l'objet.
    ruleset.spells.extend(crate::armes::sort(&outcome.resolved, crate::armes::MAITRISE_PAR_DEFAUT, Reach::Ranged));
    // Les sorts d'objet aussi, l'icône de leur objet.
    ruleset.spells.extend(crate::sorts_d_objets::sorts(&outcome.resolved));
    let icone_arme = crate::armes::icone(&outcome.resolved);
    // Les dégâts qu'un objet inflige de lui-même (Crocobur, Bottes du Cul
    // Botté) portent l'icône de l'objet.
    let icones_d_objets: BTreeMap<String, u32> =
        serde_json::from_str(crate::donnees::ICONES_D_OBJETS).unwrap_or_default();
    let icone_objet = |id: &str| {
        if id == crate::armes::ID {
            icone_arme
        } else if let Some((_, objet)) = crate::effets_d_objets::source(id) {
            icones_d_objets.get(&objet.to_string()).copied()
        } else {
            crate::sorts_d_objets::icone(&outcome.resolved, id)
        }
    };
    let icons: BTreeMap<&str, Option<u32>> = ruleset
        .spells
        .iter()
        .map(|s| (s.id.as_str(), s.dofusdb_id))
        .collect();
    let nom_de_sort = |id: &str| -> String {
        ruleset.spells.iter().find(|s| s.id == id).map_or_else(
            || crate::effets_d_objets::source(id).map_or_else(|| id.to_string(), |(nom, _)| nom.to_string()),
            |s| s.name.fr.clone(),
        )
    };
    let rendre = |tours: &[dofus_engine::TurnPlan]| -> Vec<serde_json::Value> {
        tours
            .iter()
            .map(|t| {
                serde_json::json!({
                    "turn": t.turn,
                    "odd": t.odd,
                    "damage": (t.opening_damage + t.damage).as_f64(),
                    "ap_left": t.ap_left,
                    "opening": t.opening,
                    // Ce qui tombe en début de tour, poisons et effets à
                    // retardement : la frise en fait une ligne, avec l'icône
                    // du sort qui l'a posé.
                    "opening_sources": t.opening_sources.iter().map(|(source, d)| {
                        serde_json::json!({
                            "id": source, "name": nom_de_sort(source), "damage": d.as_f64(),
                            "dofusdb_id": icons.get(source.as_str()).copied().flatten(),
                            "icone_objet": icone_objet(source),
                        })
                    }).collect::<Vec<_>>(),
                    "casts": t.casts.iter().map(|c| {
                        // Deux lignes de dégâts, pas une somme : ce que le sort
                        // inflige lui-même, et ce qu'il fait tomber d'un état
                        // qu'un autre sort a posé. Les additionner cache que la
                        // Réfraction doit une part de ses chiffres à l'Aiguille.
                        let des_procs: f64 = c.procs.iter().map(|(_, d)| d.as_f64()).sum();
                        serde_json::json!({
                            "id": c.id, "name": c.spell, "ap": c.ap_cost, "ap_left": c.ap_left,
                            "dofusdb_id": icons.get(c.id.as_str()).copied().flatten(),
                            "icone_objet": icone_objet(&c.id),
                            "damage": c.damage.as_f64(),
                            "own": c.damage.as_f64() - des_procs,
                            "procs": c.procs.iter().map(|(source, d)| serde_json::json!({
                                "id": source,
                                "name": nom_de_sort(source),
                                "damage": d.as_f64(),
                            })).collect::<Vec<_>>(),
                            "notes": c.notes,
                        })
                    }).collect::<Vec<_>>(),
                })
            })
            .collect()
    };
    // Répartition des dégâts par sort, procs rendus à leur source. Sans ça
    // l'Aiguille ressort à 0 %, alors qu'elle pose l'état dont un autre sort
    // récolte les dégâts en le consommant.
    let mut par_sort: BTreeMap<String, f64> = BTreeMap::new();
    for tour in &outcome.solution.turns {
        for (source, degats) in &tour.opening_sources {
            *par_sort.entry(source.clone()).or_default() += degats.as_f64();
        }
        for lancer in &tour.casts {
            let des_procs: f64 = lancer.procs.iter().map(|(_, d)| d.as_f64()).sum();
            *par_sort.entry(lancer.id.clone()).or_default() += lancer.damage.as_f64() - des_procs;
            for (source, degats) in &lancer.procs {
                *par_sort.entry(source.clone()).or_default() += degats.as_f64();
            }
        }
    }
    let total_reparti: f64 = par_sort.values().sum();
    let mut repartition: Vec<_> = par_sort
        .into_iter()
        .map(|(id, degats)| {
            let nom = nom_de_sort(&id);
            serde_json::json!({
                "id": id,
                "name": nom,
                "damage": degats,
                "share": if total_reparti > 0.0 { degats / total_reparti } else { 0.0 },
                "dofusdb_id": icons.get(id.as_str()).copied().flatten(),
                "icone_objet": icone_objet(&id),
            })
        })
        .collect();
    repartition.sort_by(|a, b| {
        b["damage"]
            .as_f64()
            .unwrap_or(0.0)
            .total_cmp(&a["damage"].as_f64().unwrap_or(0.0))
    });

    let turns = rendre(&outcome.solution.turns);
    let opener = rendre(&outcome.steady.opener);
    let cycle = rendre(&outcome.steady.cycle);

    // Built outside the macro: `json!` reads a bracketed expression as a JSON
    // array rather than as Rust.
    let elements: Vec<_> = ["Feu", "Terre", "Air", "Eau"]
        .iter()
        .enumerate()
        .map(|(i, name)| {
            serde_json::json!({
                "name": name,
                "characteristic": outcome.resolved.profile.elements[i].characteristic,
                "flat_damage": outcome.resolved.profile.elements[i].flat_damage,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "build": {
            // Les PA que la rotation joue : ceux de la fiche, plus ceux d'un
            // Dofus que le joueur n'a pas écarté (le Jaune Ocre).
            "ap": to_engine_build(request, &outcome.resolved).base_ap,
            "mp": outcome.resolved.base_mp,
            "crit": outcome.resolved.crit_bonus_percent,
            "power": outcome.resolved.profile.power,
            "crit_damage": outcome.resolved.profile.flat_crit_damage,
            "percent_spell": outcome.resolved.profile.percent_spell,
            "percent_weapon": outcome.resolved.profile.percent_weapon,
            "percent_melee": outcome.resolved.profile.percent_melee,
            "percent_ranged": outcome.resolved.profile.percent_ranged,
            "elements": elements,
            "multipliers": outcome.resolved.damage_multipliers.iter().map(|(n, p)| {
                serde_json::json!({
                    "name": n, "percent": p,
                    "odd_only": request.odd_turns_only.iter().any(|o| o == n),
                })
            }).collect::<Vec<_>>(),
            "sets": outcome.resolved.sets_active.iter().map(|(n, c)| {
                serde_json::json!({ "name": n, "pieces": c })
            }).collect::<Vec<_>>(),
            "assumptions": outcome.resolved.assumptions,
            "uninterpreted": outcome.resolved.uninterpreted,
        },
        "rotation": {
            "remarques": outcome.remarques,
            "total": outcome.solution.total.as_f64(),
            "per_turn": outcome.solution.total.as_f64() / f64::from(request.horizon.max(1)),
            "ap_wasted": outcome.solution.turns.iter().map(|t| i32::from(t.ap_left)).sum::<i32>(),
            "states": outcome.solution.inter_turn_states,
            "seconds": outcome.seconds,
            "turns": turns,
            "repartition": repartition,
            "targets": request.targets.unwrap_or(1),
            "targets_sweep": balayage,
            "zones_sans_plafond": outcome.zones_sans_plafond,
            "zones_degressives": outcome.zones_degressives,
            "placement_ignore": outcome.placement_ignore,
            "ecartes": outcome.ecartes.iter().map(|(s, r)| {
                serde_json::json!({ "sort": s, "raison": r })
            }).collect::<Vec<_>>(),
            // L'opener et la boucle : la vraie réponse à « qu'est-ce que je
            // joue », l'horizon n'étant qu'une fenêtre arbitraire.
            "steady": {
                "opener": opener,
                "cycle": cycle,
                "per_turn": outcome.steady.per_turn_value(),
                "cycle_length": outcome.steady.cycle.len(),
                "states": outcome.steady.states,
                "edges": outcome.steady.edges,
                // Une boucle absente parce que le graphe est trop grand n'est
                // pas une absence de boucle : le dire, et dire laquelle des
                // deux bornes a cédé.
                "gave_up": match outcome.steady.gave_up {
                    Some(dofus_engine::GaveUp::TooManyStates) => Some("states"),
                    Some(dofus_engine::GaveUp::TooMuchWork) => Some("work"),
                    Some(dofus_engine::GaveUp::NotConverged) => Some("convergence"),
                    None => None,
                },
                "too_large": outcome.steady.gave_up.is_some(),
                // Les plafonds de Howard, qui cherche la boucle au-delà de ceux
                // de Karp : ce sont eux qui font renoncer.
                "max_states": dofus_engine::Engine::MAX_CYCLE_STATES,
                "max_edges": dofus_engine::Engine::MAX_CYCLE_EDGES,
            },
        },
    })
    .to_string())
}

/// Les sorts que les objets portés modifient, par identifiant de sort des
/// règles : leurs limites et règles de lancer modifiées, le critique ajouté à
/// leur taux de base, et d'où ça vient.
fn sorts_modifies(classe: u32, resolu: &Resolved) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    let Ok(mut regles) = load_ruleset(classe) else { return out };
    crate::objets_de_classe::sur_les_regles(&mut regles, resolu);
    for s in &regles.spells {
        let Some(id) = s.dofusdb_id else { continue };
        let sources = crate::objets_de_classe::sources(resolu, id);
        if sources.is_empty() {
            continue;
        }
        out.insert(
            s.id.clone(),
            serde_json::json!({
                "ap": s.ap_cost.base,
                "range": s.range,
                "cast": s.cast,
                "cooldown": s.cooldown_turns,
                "casts_per_turn": s.casts_per_turn,
                "casts_per_target": s.casts_per_target,
                "crit_objets": crate::objets_de_classe::critique_des_objets(resolu, id),
                "sources": sources,
            }),
        );
    }
    out
}

/// Les fourchettes de dégâts de chaque sort, pour ce build, palier par palier,
/// par un moteur au deck complet de la classe : les mêmes chiffres que la
/// rotation, sans seconde implémentation côté page.
fn table_de_degats(
    build: &BuildInput,
    resolved: &Resolved,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    tables_par_lots(build, resolved, dofus_engine::MAX_SPELLS)
}

/// La même chose, en lots d'au plus `places_max` places de moteur : un seul
/// sort par moteur à `1`, la référence du test qui vérifie que le découpage ne
/// perd aucune table.
fn tables_par_lots(
    build: &BuildInput,
    resolved: &Resolved,
    places_max: usize,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut ruleset = load_ruleset(build.class).ok()?;
    crate::objets_de_classe::sur_les_regles(&mut ruleset, resolved);
    // L'arme aussi, à la Maîtrise par défaut et à distance, comme le reste de
    // la table : son infobulle le dit.
    ruleset.spells.extend(crate::armes::sort(resolved, crate::armes::MAITRISE_PAR_DEFAUT, Reach::Ranged));
    // Les sorts d'objet, avec le compteur de leur renfort s'ils en posent un.
    crate::sorts_d_objets::greffer(&mut ruleset, resolved);
    let mut out = serde_json::Map::new();
    // Par lots qui tiennent dans le moteur : il plafonne à 32 places (ses
    // masques sont des u32), et un sort à modes de lancer en prend une par mode
    // en plus de la sienne. Chaque lot est un moteur à part, sans coût ici
    // puisqu'aucune rotation n'est cherchée. Un lot que le moteur refuse encore
    // se reprend sort par sort.
    let mut lots: Vec<Vec<&dofus_ruleset::SpellDef>> = Vec::new();
    let mut places = 0;
    for s in &ruleset.spells {
        let prend = 1 + s.modes.len();
        if lots.is_empty() || places + prend > places_max {
            lots.push(Vec::new());
            places = 0;
        }
        if let Some(lot) = lots.last_mut() {
            lot.push(s);
        }
        places += prend;
    }
    let moteur_pour = |lot: &[&dofus_ruleset::SpellDef]| {
        let deck: Vec<String> = lot.iter().map(|s| s.id.clone()).collect();
        Engine::new(
            &ruleset,
            Build {
                name: String::new(),
                profile: resolved.profile,
                base_ap: resolved.base_ap,
                base_mp: 3,
                crit_bonus_percent: resolved.crit_bonus_percent,
                // Les bonus multiplicatifs du build, Dofus compris : sans
                // eux, l'infobulle contredirait la rotation affichée juste
                // en dessous. En `Always`, sauf ceux que le jeu décrit tour
                // par tour : la table lisant un tour impair, l'infobulle
                // montre le Rêve Nébuleux à +20 %, comme la rotation ses
                // tours impairs.
                modifiers: resolved
                    .damage_multipliers
                    .iter()
                    .map(|(name, percent)| BuildModifier {
                        id: name.clone(),
                        percent: *percent,
                        when: match resolved.tours.get(name) {
                            Some(dofus_build::Tours::Impairs) => When::OddTurns,
                            Some(dofus_build::Tours::Pairs) => When::EvenTurns,
                            None => When::Always,
                        },
                    })
                    .collect(),
                deck,
            },
            Scenario {
                poussees_bloquees: false,
                horizon: 1,
                pm_depenses: 0,
                etalement: 0,
                placement: None,
                etats_declares: vec![],
                // L'infobulle d'un sort dit ce qu'il fait SUR UNE CIBLE. Le
                // multi-cibles se lit sur le graphique, pas ici : une
                // fourchette qui changerait avec un réglage fait ailleurs sur
                // la page serait illisible.
                targets: 1,
                starting_turn_is_odd: true,
                resistance: Resistance::NONE,
                prune_spells: false,
                budgets: Vec::new(),
                dominance: true,
                mode: Mode::Expected,
                reach: Reach::Ranged,
            },
        )
    };
    let mut lire = |moteur: &Engine, lot: &[&dofus_ruleset::SpellDef]| {
        for spell in lot {
            let lignes: Vec<serde_json::Value> = moteur
                .damage_table(&spell.id)
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                        "charges": r.charges,
                        "normal": [r.normal.0, r.normal.1],
                        "critical": r.critical.map(|(a, b)| vec![a, b]),
                        "by_element": r.by_element.iter().map(|e| serde_json::json!({
                            "element": format!("{:?}", e.element),
                            "normal": [e.normal.0, e.normal.1],
                            "critical": e.critical.map(|(a, b)| vec![a, b]),
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            if !lignes.is_empty() {
                out.insert(spell.id.clone(), serde_json::Value::Array(lignes));
            }
        }
    };
    for lot in &lots {
        match moteur_pour(lot) {
            Ok(moteur) => lire(&moteur, lot),
            Err(_) => {
                for spell in lot {
                    if let Ok(moteur) = moteur_pour(&[*spell]) {
                        lire(&moteur, &[*spell]);
                    }
                }
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {

    /// Le Prélude au Fer renseigné dans le build importé ne compte pas dans les
    /// caractéristiques : c'est à la rotation de le proposer dans ses cycles. Son
    /// nom suffit à le rattacher quand l'import perd l'identifiant du sort.
    #[test]
    fn un_bonus_de_classe_sans_identifiant_de_sort_reste_au_solveur() {
        let charge = |effect_id: Option<&str>| -> dofus_build::BuildInput {
            let mut boost = serde_json::json!({
                "name": "Prélude au Fer", "stat": "pu", "percent": 150, "class_id": 20,
            });
            if let Some(e) = effect_id {
                boost["effect_id"] = e.into();
            }
            serde_json::from_value(serde_json::json!({
                "class": 20, "level": 200, "items": [], "boosts": [boost],
            }))
            .unwrap()
        };
        let nu: dofus_build::BuildInput = serde_json::from_value(serde_json::json!({
            "class": 20, "level": 200, "items": [],
        }))
        .unwrap();
        let sans = super::resolve_build(&nu).unwrap().profile.power;
        for e in [None, Some("23841-1-3-1560-0")] {
            let r = super::resolve_build(&charge(e)).unwrap();
            assert_eq!(r.profile.power, sans, "{e:?} : {:#?}", r.assumptions);
            assert!(
                r.assumptions.iter().any(|a| a.contains("non compté ici")),
                "{e:?} : {:#?}",
                r.assumptions
            );
        }
    }

    /// Les deux passes trouvent ce que la première ne pouvait pas voir.
    ///
    /// Sram, quatorze sorts, quatre tours, trois cibles : palier éteint, le
    /// solveur ne lance aucun des cinq voleurs, battus par les sorts de dégâts
    /// purs. Rendus au deck de la seconde passe, ils remontent le total de moitié,
    /// au niveau de la recherche exhaustive. Il faut assez de sorts pour que la
    /// première passe les écarte : sur un deck court, elle les joue faute de mieux.
    #[test]
    fn les_deux_passes_rendent_au_deck_les_sorts_que_la_mecanique_fait_vivre() {
        let requete = |mecaniques: &[&str]| -> Request {
            serde_json::from_value(serde_json::json!({
                "class": 4,
                "level": 200,
                "items": [],
                "invested": { "strength": 250, "intelligence": 250,
                              "chance": 250, "agility": 250 },
                // Douze PA, comme un personnage équipé : à six, les voleurs
                // passent faute de mieux et le test ne dit plus rien.
                "forgemagic": { "a1": {"pa": 1}, "a2": {"pa": 1}, "am": {"pa": 1},
                                "ar": {"pa": 1}, "bo": {"pa": 1} },
                "deck": ["truanderie", "sournoiserie", "fourberie", "arsenic",
                         "fourvoiement", "arnaque", "attaque_mortelle", "cruaute",
                         "poisse", "chausse_trappe", "toxines", "pillage",
                         "coupe_gorge", "larcin"],
                "horizon": 4,
                "targets": 3,
                "mecaniques": mecaniques,
            }))
            .expect("requete valide")
        };
        let joue = |r: Request| -> (f64, usize) {
            let out = run(&r).unwrap_or_else(|e| panic!("{e}"));
            let voleurs = out
                .solution
                .turns
                .iter()
                .flat_map(|t| t.casts.iter())
                .filter(|c| {
                    matches!(c.id.as_str(), "fourberie" | "arnaque" | "pillage" | "truanderie")
                })
                .count();
            (out.solution.total.as_f64(), voleurs)
        };
        let (eteint, sans_voleurs) = joue(requete(&[]));
        let (deux, avec_voleurs) = joue(requete(&["vols_de_caracteristique"]));

        assert_eq!(
            sans_voleurs, 0,
            "palier éteint, les voleurs ne valent pas leur place : {eteint:.1}"
        );
        assert!(
            avec_voleurs > 0,
            "la seconde passe doit pouvoir les lancer, elle en a joué {avec_voleurs}"
        );
        assert!(
            deux > eteint * 1.3,
            "les vols valent la moitié du total du Sram : {deux:.1} contre {eteint:.1}"
        );
    }

    /// Sans liste, toutes les mécaniques comptent : la requête de la page rend
    /// le total vols compris.
    #[test]
    fn sans_liste_toutes_les_mecaniques_comptent() {
        let requete = |mecaniques: Option<&[&str]>| -> Request {
            let mut v = serde_json::json!({
                "class": 4,
                "level": 200,
                "items": [],
                "invested": { "strength": 250, "intelligence": 250,
                              "chance": 250, "agility": 250 },
                "forgemagic": { "a1": {"pa": 1}, "a2": {"pa": 1}, "am": {"pa": 1},
                                "ar": {"pa": 1}, "bo": {"pa": 1} },
                "deck": ["truanderie", "sournoiserie", "fourberie", "arsenic",
                         "fourvoiement", "arnaque", "attaque_mortelle", "cruaute",
                         "poisse", "chausse_trappe", "toxines", "pillage",
                         "coupe_gorge", "larcin"],
                "horizon": 4,
                "targets": 3,
            });
            if let Some(m) = mecaniques {
                v["mecaniques"] = serde_json::json!(m);
            }
            serde_json::from_value(v).expect("requete valide")
        };
        let total = |r: Request| run(&r).unwrap_or_else(|e| panic!("{e}")).solution.total.as_f64();
        let defaut = total(requete(None));
        assert_eq!(defaut, total(requete(Some(&["vols_de_caracteristique"]))));
        let sans = total(requete(Some(&[])));
        assert!(defaut > sans * 1.3, "les vols comptent d'office : {defaut:.1} contre {sans:.1}");
    }

    /// Et elles ne dépassent jamais la recherche exhaustive : elles cherchent
    /// dans un sous-ensemble des sorts. Sur un deck court, l'exhaustif tient en
    /// une fraction de seconde, ce qui permet de comparer les deux ici.
    #[test]
    fn les_deux_passes_ne_depassent_pas_la_recherche_exhaustive() {
        let requete = |exhaustif: bool| -> Request {
            serde_json::from_value(serde_json::json!({
                "class": 4,
                "level": 200,
                "invested": { "agility": 400 },
                "deck": ["fourberie", "arnaque", "pillage", "truanderie", "sournoiserie"],
                "horizon": 4,
                "targets": 2,
                "mecaniques": ["vols_de_caracteristique"],
                "exhaustif": exhaustif,
            }))
            .expect("requete valide")
        };
        let total = |r: Request| {
            run(&r)
                .unwrap_or_else(|e| panic!("{e}"))
                .solution
                .total
                .as_f64()
        };
        let deux = total(requete(false));
        let tout = total(requete(true));
        assert!(
            deux <= tout + 0.01,
            "les deux passes ne peuvent pas battre l'exhaustif : {deux:.1} contre {tout:.1}"
        );
    }

    /// A refusal shown to a player names the class and never leaks internal
    /// vocabulary, such as an id or an implementation detail.
    #[test]
    fn the_refusal_names_the_class_and_avoids_jargon() {
        let m = unsupported_class_message(14);
        assert!(m.starts_with("Zobal"), "must open with the class name: {m}");
        for jargon in [
            "fichier de mécaniques",
            "ruleset",
            "snapshot",
            "classe 14",
            "breed",
            "None",
            "Err",
        ] {
            assert!(
                !m.contains(jargon),
                "leaks internal vocabulary {jargon:?}: {m}"
            );
        }
        // It has to say what IS possible, not only what is not.
        assert!(m.contains("Xélor"), "must list what works: {m}");
        // And say the import itself succeeded, so the player does not think
        // their equipment was lost.
        assert!(m.contains("équipement"), "must say the stuff was read: {m}");
    }

    /// L'élément d'une ligne de dégâts arrive à la page en clair : `Earth`,
    /// ou `best` pour une ligne dans le meilleur élément du lanceur, jamais
    /// `Some(Fire)`.
    #[test]
    fn le_catalogue_nomme_l_element_des_lignes() {
        let v: serde_json::Value = serde_json::from_str(&classes_json().unwrap()).unwrap();
        let elements: Vec<String> = v["classes"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|c| c["spells"].as_array().unwrap().clone())
            .flat_map(|s| s["lines"].as_array().unwrap().clone())
            .map(|l| l["element"].as_str().unwrap_or("absent").to_string())
            .collect();
        assert!(!elements.is_empty());
        let permis = ["Fire", "Earth", "Air", "Water", "Neutral", "best", "worst"];
        let hors: Vec<&String> = elements.iter().filter(|e| !permis.contains(&e.as_str())).collect();
        assert!(hors.is_empty(), "{hors:?}");
        assert!(elements.iter().any(|e| e == "best"), "aucune ligne en meilleur élément");
    }

    #[test]
    fn every_class_can_be_named() {
        let sans_nom: Vec<u32> = (1..=20)
            .filter(|id| *id != 19)
            .filter(|id| class_name(*id).is_none())
            .collect();
        assert!(sans_nom.is_empty(), "no snapshot to name: {sans_nom:?}");
    }
    use super::*;

    /// The exact payload the bookmarklet sends, in DofusBook's own shapes. The
    /// command line never exercises them: it feeds a hand-written file with
    /// `class` at the top level, normalised once.
    const CHARGE: &str = r#"{
      "v": 2, "src": "dofusbook",
      "stuff": {"id": 1, "short_url": "XXXXX", "name": "T",
                "character_class": 5, "character_level": 200,
                "stuffItem": {"ch":6988,"am":6597,"a1":6742,"a2":5118,"ce":6598,"bo":3572,
                              "ca":6990,"br":6744,"ar":6989,"fa":2654,"mo":null,
                              "d1":3330,"d2":2829,"d3":1040,"d4":258,"d5":1241,"d6":3992},
                "stuffCarac": {"base_ch": 398}},
      "items_table": [{"id":6988,"official":34330},{"id":6597,"official":31761},
                      {"id":6742,"official":32234},{"id":5118,"official":24035},
                      {"id":6598,"official":31762},{"id":3572,"official":17575},
                      {"id":6990,"official":34332},{"id":6744,"official":32236},
                      {"id":6989,"official":34331},{"id":2654,"official":13673},
                      {"id":3330,"official":8698},{"id":2829,"official":7043},
                      {"id":1040,"official":6980},{"id":258,"official":739},
                      {"id":1241,"official":7754},{"id":3992,"official":7115}],
      "fmItems": {"a1":{"pa":1},"a2":{"cc":5},"am":{"cc":8},"ar":{"cc":6},"bo":{"cc":6},
                  "br":{"dc":8},"ca":{"cc":7},"ce":{"cc":6},"ch":{"cc":6}},
      "fmGlobal": {"pa":1,"pm":1,"po":1},
      "boosts": [{"active":true,"boostName":"Rêve Nébuleux","effectName":"deg",
                  "effectValue":20,"count":1,"classId":null},
                 {"active":true,"boostName":"Bleu Turquoise","effectName":"deg",
                  "effectValue":1,"count":10,"classId":null}],
      "deck": ["gelure","ralentissement","compte_goutte","permutation","petrification",
               "clepsydre","glas"],
      "budgets": {"telefrag_per_turn": 3},
      "mecaniques": ["remise_petrification"],
      "odd_turns_only": ["Rêve Nébuleux"],
      "horizon": 7
    }"#;

    /// L'infobulle annonce les mêmes dégâts que la rotation, bonus
    /// multiplicatifs du build compris : un Rêve Nébuleux à +20 % et un Bleu
    /// Turquoise à +10 % font 1,32.
    #[test]
    fn the_tooltip_agrees_with_the_rotation() {
        let request: Request = serde_json::from_str(CHARGE).expect("charge lisible");
        let resolu: serde_json::Value =
            serde_json::from_str(&resolve_json(&request.build).expect("résolution")).unwrap();
        let rotation: serde_json::Value =
            serde_json::from_str(&solve_json(&request).expect("résolution")).unwrap();

        let table = &resolu["damage_tables"]["gelure"][0];
        let (lo, hi) = (
            table["critical"][0].as_f64().unwrap(),
            table["critical"][1].as_f64().unwrap(),
        );
        // Ce build est à 100 % de critique une fois plafonné, donc chaque
        // lancer critique et la rotation doit tomber au milieu de la fourchette.
        let milieu = (lo + hi) / 2.0;

        let joue = rotation["rotation"]["turns"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|t| t["casts"].as_array().unwrap())
            .find(|c| c["id"] == "gelure")
            .map(|c| c["damage"].as_f64().unwrap())
            .expect("Gelure doit être jouée");

        assert!(
            (joue - milieu).abs() <= 1.0,
            "l'infobulle annonce {milieu} pour Gelure, la rotation en joue {joue}"
        );
        // Et la valeur nue, sans les bonus, ne doit plus apparaître.
        assert!(
            lo > 500.0,
            "les bonus du build ne sont pas appliqués : {lo}-{hi}"
        );
    }

    #[test]
    fn resolving_reports_what_the_page_displays() {
        let input: BuildInput = serde_json::from_str(CHARGE).expect("charge lisible");
        let json = resolve_json(&input).expect("résolution");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(v["class"], 5);
        assert_eq!(v["level"], 200);
        assert_eq!(v["items_equipped"], 16);
        assert_eq!(v["ap"], 11);
        assert_eq!(v["crit"], 127);
        assert_eq!(v["dominant"][0], "Eau");
        // Chance, which the double normalisation used to inflate to 1516.
        assert_eq!(v["elements"][3]["characteristic"], 1118);
    }

    #[test]
    fn solving_from_the_bookmarklets_payload() {
        let request: Request = serde_json::from_str(CHARGE).expect("charge lisible");
        let json = solve_json(&request).expect("résolution");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(v["build"]["elements"][3]["characteristic"], 1118);
        assert_eq!(v["rotation"]["turns"].as_array().unwrap().len(), 7);
        assert_eq!(v["rotation"]["ap_wasted"], 0);

        // Turn one cannot generate a Telefrag from nothing: only Permutation
        // teleports unconditionally in this deck, so it is cast before anything
        // that reads a Telefrag can pay off. Its rank among the casts is not
        // asserted: with the AP steal assumed to fail (see AP_THEFT_LANDS), either
        // order is optimal.
        let premier = &v["rotation"]["turns"][0]["casts"];
        let ids: Vec<&str> = premier
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect();
        assert!(
            ids.contains(&"permutation"),
            "le premier tour doit poser un Telefrag : {ids:?}"
        );
        // And every cast reports the AP left once it resolved.
        assert!(premier[0]["ap_left"].as_i64().unwrap() >= 0);
    }

    /// Un total de zone repose sur une capacité que la donnée ne donne pas
    /// toujours : le taire laisserait lire comme exact un chiffre trop haut.
    mod zones_sans_plafond {
        use super::super::*;

        fn requete(cibles: u8) -> Request {
            // `Request` se deserialise, donc on passe par du JSON plutot que
            // d'exiger un `Default` que rien d'autre n'utiliserait.
            let brut = serde_json::json!({
                "class": 9,
                "level": 200,
                "invested": { "strength": 400 },
                "deck": ["fleche_ralentissante", "carreaux_destructeurs"],
                "horizon": 3,
                "targets": cibles,
            });
            serde_json::from_value(brut).expect("requete valide")
        }

        /// En duel, la question ne se pose pas : rien n'est signalé.
        #[test]
        fn le_duel_ne_signale_rien() {
            let out = run(&requete(1)).unwrap_or_else(|e| panic!("{e}"));
            assert!(
                out.zones_sans_plafond.is_empty(),
                "sans zone, pas d'avertissement : {:?}",
                out.zones_sans_plafond
            );
        }

        /// En zone non plus, depuis que la fourche est décodée : les Carreaux
        /// Destructeurs portaient la dernière forme sans compte.
        #[test]
        fn la_fourche_des_carreaux_a_son_plafond() {
            let out = run(&requete(4)).unwrap_or_else(|e| panic!("{e}"));
            assert!(
                out.zones_sans_plafond.is_empty(),
                "plus aucune forme sans compte : {:?}",
                out.zones_sans_plafond
            );
        }

        /// La dégressivité se dit aussi. Le Forgelance est l'exception : ses sorts de
        /// zone disent presque tous « les dommages de zone ne sont pas dégressifs », et
        /// le test le vérifie des deux côtés, pour que l'avertissement ne devienne pas
        /// un bruit de fond.
        #[test]
        fn la_degressivite_se_dit_quand_elle_existe() {
            // Forgelance : ses sorts de zone annoncent ne pas être dégressifs.
            let out = run(&requete(4)).unwrap_or_else(|e| panic!("{e}"));
            assert!(
                out.zones_degressives.is_empty(),
                "les sorts de zone du Forgelance ne sont pas dégressifs : {:?}",
                out.zones_degressives
            );

            // Xélor : le Glas l'est, et le dit.
            let brut = serde_json::json!({
                "class": 5,
                "level": 200,
                "invested": { "chance": 398 },
                "deck": ["glas", "gelure", "petrification"],
                "horizon": 4,
                "targets": 3,
                // ⚠️ LE PALIER DES CUMULS DE GLAS, sinon le sort ne vaut plus
                // la peine d'être lancé et la rotation ne le joue pas : la
                // liste des zones dégressives ne parle que des sorts joués.
                "mecaniques": ["cumuls_de_glas"],
            });
            let xelor: Request = serde_json::from_value(brut).expect("requete valide");
            let out = run(&xelor).unwrap_or_else(|e| panic!("{e}"));
            assert!(
                !out.zones_degressives.is_empty(),
                "le Glas du Xélor perd 10 % par case et doit être signalé"
            );
        }

        /// En duel, la dégressivité ne se pose pas non plus.
        #[test]
        fn le_duel_ne_dit_rien_de_la_degressivite() {
            let out = run(&requete(1)).unwrap_or_else(|e| panic!("{e}"));
            assert!(
                out.zones_degressives.is_empty(),
                "{:?}",
                out.zones_degressives
            );
        }

        /// Seuls les sorts réellement joués sont nommés : une liste qui nommerait tout
        /// le fichier passerait le test précédent sans rien apprendre au joueur.
        #[test]
        fn seuls_les_sorts_joues_sont_nommes() {
            let out = run(&requete(4)).unwrap_or_else(|e| panic!("{e}"));
            let joues: std::collections::BTreeSet<String> = out
                .solution
                .turns
                .iter()
                .flat_map(|t| t.casts.iter())
                .map(|c| c.spell.clone())
                .collect();
            for nom in &out.zones_sans_plafond {
                assert!(
                    joues.contains(nom),
                    "{nom} est signalé sans avoir été joué : {joues:?}"
                );
            }
        }
    }

    /// Le Rêve Nébuleux arrive au moteur en deux modificateurs, +20 % les tours
    /// impairs et −10 % les tours pairs, comme le décrit le sort du jeu (5454).
    #[test]
    fn le_reve_nebuleux_vaut_un_tour_sur_deux() {
        let request: Request = serde_json::from_str(CHARGE).expect("charge lisible");
        let resolved = resolve_build(&request.build.normalise()).expect("résolution");
        let build = to_engine_build(&request, &resolved);
        let de = |id: &str| build.modifiers.iter().find(|m| m.id == id).map(|m| (m.percent, m.when));
        assert_eq!(de("Rêve Nébuleux, tours impairs"), Some((120, When::OddTurns)));
        assert_eq!(de("Rêve Nébuleux, tours pairs"), Some((90, When::EvenTurns)));
        assert_eq!(de("Bleu Turquoise"), Some((110, When::Always)));
    }

    /// Un bonus de Dofus écarté dans l'onglet Rotation sort du calcul : ce
    /// Xélor porte le Vulbis et l'Ocre, qui donnent +10 % et +1 PA contre une
    /// cible passive ; écartés, ni l'un ni l'autre.
    #[test]
    fn un_bonus_de_dofus_ecarte_sort_du_calcul() {
        let request: Request = serde_json::from_str(CHARGE).expect("charge lisible");
        let resolved = resolve_build(&request.build.normalise()).expect("résolution");
        let garde = to_engine_build(&request, &resolved);
        assert_eq!(garde.base_ap, resolved.base_ap + 1, "le PA du Jaune Ocre");
        assert!(garde.modifiers.iter().any(|m| m.id == "Rouge Vermeil" && m.percent == 110));
        let mut ecarte = request.clone();
        ecarte.bonus_ecartes = vec!["Rouge Vermeil".into(), "Jaune Ocre".into()];
        let sans = to_engine_build(&ecarte, &resolved);
        assert_eq!(sans.base_ap, resolved.base_ap);
        assert!(!sans.modifiers.iter().any(|m| m.id == "Rouge Vermeil"));
        assert_eq!(sans.modifiers.len(), garde.modifiers.len() - 1);
    }

    /// L'Abyssal, le Sylvestre et le Cauchemar, portés par un Roublard : +1 PA
    /// (ou +1 PM, sa case décochée), +8 Puissance par PM dépensé, +100
    /// Puissance les tours où les poussées butent contre un obstacle.
    #[test]
    fn les_autres_dofus_passent_au_moteur() {
        let mut objets = vec![0u32; 17];
        objets[11] = 18043;
        objets[12] = 29136;
        objets[13] = 26066;
        let requete = |extra: serde_json::Value| -> Request {
            let mut v = serde_json::json!({
                "class": 13, "level": 200, "items": objets, "deck": [], "horizon": 1,
                "pm_depenses": 3, "poussees_bloquees": true,
            });
            v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            serde_json::from_value(v).expect("requête lisible")
        };
        let base = requete(serde_json::json!({}));
        let resolved = resolve_build(&base.build.normalise()).expect("résolution");
        let b = to_engine_build(&base, &resolved);
        // La Puissance du Sylvestre et du Cauchemar ne passe pas par la fiche.
        assert_eq!(b.profile.power, resolved.profile.power);
        assert_eq!((b.base_ap, b.base_mp), (resolved.base_ap + 1, resolved.base_mp));
        let sans_contact = requete(serde_json::json!({ "bonus_ecartes": ["Descente aux Abysses"] }));
        let b = to_engine_build(&sans_contact, &resolved);
        assert_eq!((b.base_ap, b.base_mp), (resolved.base_ap, resolved.base_mp + 1));
        // Elle se greffe en états : le Cauchemar gagné par les sorts qui
        // poussent, le Sylvestre à 3 × 8 par cran, deux crans au plus.
        let entravants = snapshot_for(13).unwrap().sorts_qui_entravent_le_lanceur();
        assert!(entravants.is_empty(), "aucun sort du Roublard ne l'entrave : {entravants:?}");
        let (regles, notes) =
            avec_les_effets_de_dofus(load_ruleset(13).unwrap(), &base, &resolved, &entravants).expect("greffe");
        assert!(notes.is_empty(), "{notes:?}");
        let etat = |id: &str| regles.resources.iter().find(|r| r.id == id).expect(id);
        assert_eq!((etat("garde_champetre").max, etat("garde_champetre").gain_per_turn), (2, Some(1)));
        assert_eq!(etat("eternel_cauchemar").max, 1);
        let gagne = |s: &str| {
            regles.spells.iter().find(|x| x.id == s).unwrap().effects.iter().any(|e| {
                matches!(e, dofus_ruleset::Effect::Gain { resource, .. } if resource == "eternel_cauchemar")
            })
        };
        assert!(gagne("espingole") && !gagne("dagues_boomerang"));
        // L'Œil du Cauchemar : sans entrave, ni de ses sorts ni déclarée, pas
        // d'Œil. Sa case cochée (un ennemi entrave), il part avec la poussée :
        // +10 % finaux sur les mêmes coups que les 100 Puissance.
        let oeil = |r: &Ruleset| r.resources.iter().find(|x| x.id == "oeil_du_cauchemar").cloned();
        assert!(oeil(&regles).is_none());
        let avec_oeil = requete(serde_json::json!({ "bonus_inclus": ["Œil du Cauchemar"] }));
        let (regles, _) =
            avec_les_effets_de_dofus(load_ruleset(13).unwrap(), &avec_oeil, &resolved, &entravants).expect("greffe");
        assert!(oeil(&regles).expect("l'Œil déclaré").modifies_damage.iter().any(|m| matches!(
            m,
            dofus_ruleset::DamageModifier::FinalMultiplier { percent: dofus_ruleset::Maybe::Known(110), finaux: true, .. }
        )));
        assert_eq!(
            gains_du_cauchemar(&regles, "espingole"),
            [("eternel_cauchemar".to_string(), None), ("oeil_du_cauchemar".to_string(), None)]
        );
        assert!(resolved.bonus_conditionnels.iter().any(|b| b.nom == "Œil du Cauchemar" && !b.coche));
    }

    /// Les états du Cauchemar qu'un sort donne, et celui qu'il faut déjà
    /// porter pour chacun.
    fn gains_du_cauchemar(regles: &Ruleset, sort: &str) -> Vec<(String, Option<String>)> {
        regles
            .spells
            .iter()
            .find(|x| x.id == sort)
            .unwrap()
            .effects
            .iter()
            .filter_map(|e| match e {
                dofus_ruleset::Effect::Gain { resource, requires, .. }
                    if ["eternel_cauchemar", "bouclier_bontarien", "oeil_du_cauchemar"].contains(&resource.as_str()) =>
                {
                    let apres = match requires {
                        Some(dofus_ruleset::Condition::CasterHas { resource }) => Some(resource.clone()),
                        _ => None,
                    };
                    Some((resource.clone(), apres))
                }
                _ => None,
            })
            .collect()
    }

    /// Le Crâ se donne l'Œil du Cauchemar seul : Sentinelle lui retire 1 de
    /// Portée, une entrave, et une poussée dans le même tour fait le reste. La
    /// donnée le dit, sort par sort ; l'Œil part avec le second des deux
    /// déclencheurs, quel qu'il soit. Tirs Puissants, qui reporte son retrait au
    /// tour suivant depuis la 3.7, ne compte pas.
    #[test]
    fn le_cra_s_entrave_lui_meme() {
        let entravants = snapshot_for(9).unwrap().sorts_qui_entravent_le_lanceur();
        let mut objets = vec![0u32; 17];
        objets[11] = 26066;
        let requete: Request = serde_json::from_value(serde_json::json!({
            "class": 9, "level": 200, "items": objets, "deck": [], "horizon": 1, "poussees_bloquees": true,
        }))
        .unwrap();
        let resolved = resolve_build(&requete.build.normalise()).expect("résolution");
        let (regles, notes) =
            avec_les_effets_de_dofus(load_ruleset(9).unwrap(), &requete, &resolved, &entravants).expect("greffe");
        assert!(notes.iter().any(|n| n.contains(": Sentinelle vous entrave")), "{notes:?}");
        let de = |s: &str| Some(s.to_string());
        assert_eq!(
            gains_du_cauchemar(&regles, "sentinelle"),
            [("bouclier_bontarien".to_string(), None), ("oeil_du_cauchemar".to_string(), de("eternel_cauchemar"))]
        );
        assert!(gains_du_cauchemar(&regles, "tirs_puissants").is_empty());
        assert_eq!(
            gains_du_cauchemar(&regles, "fleche_de_recul"),
            [("eternel_cauchemar".to_string(), None), ("oeil_du_cauchemar".to_string(), de("bouclier_bontarien"))]
        );
        // Un sort qui ne pousse ni n'entrave ne touche à rien.
        assert!(gains_du_cauchemar(&regles, "fleche_d_abolition").is_empty());
    }

    /// Et les chiffres suivent : Sentinelle (qui ne part pas avant le tour 2),
    /// une Flèche de Recul qui bute, puis une Flèche Cinglante. La flèche qui
    /// pousse ne prend rien ; la Cinglante frappe exactement comme avec l'Œil
    /// déclaré, au-dessus de la même flèche armée des seules 100 Puissance.
    #[test]
    fn l_oeil_du_cra_monte_les_coups_qui_suivent() {
        let joue = |objets: Vec<u32>, puissance: i32, inclus: &[&str]| -> Vec<(String, f64)> {
            let requete: Request = serde_json::from_value(serde_json::json!({
                "class": 9, "level": 200, "items": objets,
                "deck": ["sentinelle", "fleche_de_recul", "fleche_cinglante"],
                "horizon": 2, "poussees_bloquees": true, "invested": { "agility": 100, "power": puissance },
                "bonus_inclus": inclus,
            }))
            .unwrap();
            let tours = vec![
                vec![],
                vec!["sentinelle".to_string(), "fleche_de_recul".to_string(), "fleche_cinglante".to_string()],
            ];
            rejouer(&requete, &tours).expect("rotation").turns[1]
                .casts
                .iter()
                .map(|c| (c.id.clone(), c.damage.as_f64()))
                .collect()
        };
        let mut cauchemar = vec![0u32; 17];
        cauchemar[11] = 26066;
        let seul = joue(cauchemar.clone(), 0, &[]);
        let declare = joue(cauchemar, 0, &["Œil du Cauchemar"]);
        let nu = joue(vec![0u32; 17], 0, &[]);
        let arme = joue(vec![0u32; 17], 100, &[]);
        let ordre: Vec<&str> = seul.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(ordre, ["sentinelle", "fleche_de_recul", "fleche_cinglante"], "{seul:?}");
        for (a, b) in seul.iter().zip(&declare) {
            assert!((a.1 - b.1).abs() < 0.01, "comme l'Œil déclaré : {seul:?} / {declare:?}");
        }
        assert!((seul[1].1 - nu[1].1).abs() < 0.01, "la flèche qui pousse ne prend rien : {seul:?} / {nu:?}");
        assert!(
            seul[2].1 > arme[2].1 * 1.01 && seul[2].1 <= arme[2].1 * 1.10 + 0.01,
            "+10 % au plus au-dessus des 100 Puissance : {seul:?} / {arme:?}"
        );
    }

    /// Plombage, lancé sur une bombe, redéclenche le mur : un mode greffé au
    /// sort, à la base du mur au tour du lanceur, le combo monté d'un cran en
    /// facteur, sans critique, ouvert à deux bombes bien placées.
    #[test]
    fn plombage_sur_une_bombe_redeclenche_le_mur() {
        let greffe = |mur: serde_json::Value| {
            let requete: Request = serde_json::from_value(serde_json::json!({
                "class": 13, "level": 200, "items": vec![0u32; 17], "deck": [], "horizon": 1,
                "mur_de_bombes": mur,
            }))
            .unwrap();
            avec_le_mur_de_bombes(load_ruleset(13).unwrap(), &requete, 200, 13).expect("greffe")
        };
        let (regles, notes) = greffe(serde_json::json!({ "element": "feu", "combos": [3, 2] }));
        let note = notes.join(" ");
        let plombage = regles.spells.iter().find(|s| s.id == "plombage").unwrap();
        assert!(plombage.lines.iter().all(|l| l.repeats_per.is_empty()), "plus de refrappe par bombe");
        let mode = plombage.modes.last().unwrap();
        let ligne = &mode.lines[0];
        assert_eq!(ligne.element, Some(dofus_damage::Element::Fire));
        assert!(matches!(ligne.normal, dofus_ruleset::Maybe::Known((30, 33))), "{:?}", ligne.normal);
        assert!(matches!(ligne.critical, dofus_ruleset::Maybe::Known((30, 33))));
        // Combos IV et III une fois Plombage passé : (60 + 40) / 2, 50 %.
        assert_eq!((ligne.facteur, ligne.sans_critique), (Some(150), true));
        assert!(matches!(
            &mode.requires,
            Some(dofus_ruleset::Condition::AtLeast { resource, amount: 2, .. }) if resource == "bombes_en_portee"
        ));
        assert!(note.contains("Explobombes aux combos III et II"), "{note}");
        // Un mur de Tornabombes frappe en Air, sur la base des murs Air.
        let (regles, _) = greffe(serde_json::json!({ "element": "air", "combos": [15, 15] }));
        let ligne = &regles.spells.iter().find(|s| s.id == "plombage").unwrap().modes.last().unwrap().lines[0];
        assert_eq!(ligne.element, Some(dofus_damage::Element::Air));
        assert!(matches!(ligne.normal, dofus_ruleset::Maybe::Known((24, 27))), "{:?}", ligne.normal);
        assert_eq!(ligne.facteur, Some(460), "XV plafonne : 360 %");
    }

    /// Et le chiffre est celui de KrozBoom : un Roublard nu pose ses deux
    /// Explobombes au fil des tours, puis chaque Plombage sur une bombe vaut, en
    /// moyenne sur le jet, le mur au tour du lanceur de KrozBoom aux combos montés
    /// d'un cran. Sur trois tours, poser les bombes ne paie pas encore, et le
    /// solveur s'en tient à Plombage sur l'ennemi.
    #[test]
    fn le_mur_de_la_rotation_est_celui_de_krozboom() {
        let v = serde_json::json!({
            "class": 13, "level": 200, "items": vec![0u32; 17], "deck": ["explobombe", "plombage"],
            "horizon": 6, "mur_de_bombes": { "element": "feu", "combos": [3, 2] },
        });
        let r: serde_json::Value =
            serde_json::from_str(&solve_json(&serde_json::from_value(v).unwrap()).expect("rotation")).unwrap();
        let sur_une_bombe: Vec<f64> = r["rotation"]["turns"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|t| t["casts"].as_array().unwrap().iter())
            .filter(|c| c["name"].as_str().unwrap_or("").contains("sur une bombe"))
            .map(|c| c["damage"].as_f64().unwrap())
            .collect();
        assert!(!sur_une_bombe.is_empty(), "le solveur doit lancer Plombage sur une bombe : {r}");
        let attendu = (30..=33)
            .map(|b| crate::bombes::mur(b, 0, 0, 0, 50, dofus_damage::FinalMultiplier::NEUTRAL) as f64)
            .sum::<f64>()
            / 4.0;
        for d in &sur_une_bombe {
            assert!((d - attendu).abs() < 0.01, "{d} contre {attendu} : {sur_une_bombe:?}");
        }
    }

    /// Les invocations jouent leur tour. Un Osamodas à 100 d'Agilité invoque son
    /// Tofu ; dès le tour suivant, le Tofu remplit ses 4 PA : un Béco-béco (21-23
    /// d'Air, 25-28 en critique à 5 %) et un Bisou Béco (22-25, 26-30 à 10 %, une
    /// fois par cible), à la moitié de cette Agilité, sans dommages critiques. Une
    /// seule invocation sans équipement importé.
    #[test]
    fn le_tofu_joue_son_tour() {
        let v = serde_json::json!({
            "class": 2, "level": 200, "items": vec![0u32; 17], "deck": ["tofu"], "horizon": 3,
            "invested": { "agility": 100 },
        });
        let requete: Request = serde_json::from_value(v).unwrap();
        let r: serde_json::Value = serde_json::from_str(&solve_json(&requete).expect("rotation")).unwrap();
        let profil = resolve_build(&requete.build.normalise()).unwrap().profile;
        let moyenne = |bornes: std::ops::RangeInclusive<i32>| {
            let n = bornes.clone().count() as f64;
            bornes.map(|b| crate::invocations::coup(b, dofus_damage::Element::Air, &profil, 50) as f64).sum::<f64>() / n
        };
        let par_tour = (0.95 * moyenne(21..=23) + 0.05 * moyenne(25..=28))
            + (0.90 * moyenne(22..=25) + 0.10 * moyenne(26..=30));
        let tours = r["rotation"]["turns"].as_array().unwrap();
        let degats: Vec<f64> = tours.iter().map(|t| t["damage"].as_f64().unwrap()).collect();
        assert!(degats[0].abs() < 0.01, "le tour de l'invocation ne frappe pas : {degats:?}");
        for d in &degats[1..] {
            assert!((d - par_tour).abs() < 0.01, "{d} contre {par_tour} : {degats:?}");
        }
        let invocations = tours
            .iter()
            .flat_map(|t| t["casts"].as_array().unwrap())
            .filter(|c| c["id"] == "tofu")
            .count();
        assert_eq!(invocations, 1, "{r}");
        let notes = r["build"]["assumptions"].to_string();
        assert!(notes.contains("Tofu : Béco-béco ×1 et Bisou Béco ×1 par tour (4 PA sur 4), à 50 %"), "{notes}");
        assert!(notes.contains("1 invocation en jeu au plus"), "{notes}");
    }

    /// Les invocations communes, à toutes les classes : un Iop à 100 de Force lance
    /// l'Invocation de l'Arakne au tour 2, son délai initial passé. Dès le tour
    /// suivant, l'Arakne (80 %) joue une Frapperie (16-20 de Terre, 26-30 en
    /// critique à 10 %) et l'Arakne Majeure (20 %) un Coude boule (11-15, 16-20 à
    /// 15 %), à toute sa Force.
    #[test]
    fn l_arakne_joue_son_tour() {
        let requete: Request = serde_json::from_value(serde_json::json!({
            "class": 8, "level": 200, "items": vec![0u32; 17], "deck": ["invocation_de_l_arakne"],
            "horizon": 4, "invested": { "strength": 100 },
        }))
        .unwrap();
        let r: serde_json::Value = serde_json::from_str(&solve_json(&requete).expect("rotation")).unwrap();
        let profil = resolve_build(&requete.build.normalise()).unwrap().profile;
        let moyenne = |bornes: std::ops::RangeInclusive<i32>| {
            let n = bornes.clone().count() as f64;
            bornes.map(|b| crate::invocations::coup(b, dofus_damage::Element::Earth, &profil, 100) as f64).sum::<f64>() / n
        };
        let par_tour = 0.8 * (0.90 * moyenne(16..=20) + 0.10 * moyenne(26..=30))
            + 0.2 * (0.85 * moyenne(11..=15) + 0.15 * moyenne(16..=20));
        let tours = r["rotation"]["turns"].as_array().unwrap();
        let degats: Vec<f64> = tours.iter().map(|t| t["damage"].as_f64().unwrap()).collect();
        assert_eq!(degats.len(), 4, "{r}");
        assert!(degats[0].abs() < 0.01 && degats[1].abs() < 0.01, "{degats:?}");
        for d in &degats[2..] {
            assert!((d - par_tour).abs() < 0.01, "{d} contre {par_tour} : {degats:?}");
        }
        assert_eq!(tours[1]["casts"][0]["id"], "invocation_de_l_arakne", "{r}");
        // La répartition le nomme, avec son icône.
        let part = &r["rotation"]["repartition"][0];
        assert_eq!((&part["name"], &part["dofusdb_id"]), (&serde_json::json!("Invocation de l'Arakne"), &serde_json::json!(370)), "{r}");
        let notes = r["build"]["assumptions"].to_string();
        assert!(
            notes.contains("Invocation de l'Arakne : Arakne à 80 %, Frapperie ×1 par tour (5 PA), ou Arakne Majeure à 20 %, Coude boule ×1 par tour (4 PA)"),
            "{notes}"
        );
    }

    /// Le Tacheté porté ne fait pas un écart avec DofusBook : son Harmonie de
    /// Pandala, +20 Dommages, est comptée ici même inactive dans les bonus du build
    /// importé, que leur fiche ne compte pas. La comparaison retire ces 20, et un
    /// vrai écart reste signalé.
    #[test]
    fn le_tachete_porte_ne_fait_pas_un_ecart_avec_dofusbook() {
        let build: BuildInput =
            serde_json::from_value(serde_json::json!({ "class": 4, "level": 200, "items": [7112] })).unwrap();
        let r = resolve_build(&build.normalise()).unwrap();
        assert_eq!(r.dommages_des_dofus_portes, 20);
        // La fiche de DofusBook : les dommages d'avant le bonus, sur les cinq
        // éléments, et une Puissance qui diffère vraiment.
        let mut stats = serde_json::Map::new();
        for (code, i) in [("dff", 0), ("dtf", 1), ("daf", 2), ("def", 3), ("dnf", 4)] {
            stats.insert(code.into(), serde_json::json!(r.profile.elements[i].flat_damage - 20));
        }
        stats.insert("pu".into(), serde_json::json!(r.profile.power + 40));
        let ecarts = ecarts_avec_dofusbook(&r, &serde_json::json!({ "stats": stats }), &crate::fiche::Fiche::default());
        let labels: Vec<&str> = ecarts.iter().filter_map(|e| e["label"].as_str()).collect();
        assert_eq!(labels, ["Puissance"], "{ecarts:?}");
    }

    /// Le découpage en lots ne perd aucune table de dégâts : pour l'Iop et le Sram,
    /// les tables en lots sont exactement celles d'un moteur par sort (les
    /// dix-neuf classes prendraient une minute hors release).
    #[test]
    fn le_decoupage_en_lots_ne_perd_aucune_table() {
        for classe in [8u32, 4] {
            let build: BuildInput = serde_json::from_value(
                serde_json::json!({ "class": classe, "level": 200, "invested": { "strength": 400 } }),
            )
            .unwrap();
            let r = resolve_build(&build).unwrap();
            let en_lots = table_de_degats(&build, &r).unwrap();
            let un_par_un = tables_par_lots(&build, &r, 1).unwrap();
            assert!(!un_par_un.is_empty(), "classe {classe} : aucune table");
            assert_eq!(en_lots, un_par_un, "classe {classe}");
        }
    }

    /// L'orientation lit les vrais éléments de dégâts : ceux des pièges, que les
    /// règles n'écrivent pas, et ceux des invocations ; le meilleur élément ; et si
    /// le sort agit seulement.
    #[test]
    fn l_orientation_lit_les_vrais_elements_de_degats() {
        let catalogue: serde_json::Value = serde_json::from_str(&classes_json().unwrap()).unwrap();
        let sram = catalogue["classes"].as_array().unwrap().iter().find(|c| c["id"] == 4).unwrap();
        let o = |id: &str| {
            sram["spells"].as_array().unwrap().iter().find(|s| s["id"] == id).unwrap_or_else(|| panic!("{id}"))
                ["orientation"]
                .clone()
        };
        assert_eq!(o("piege_sournois")["elements"], serde_json::json!(["Fire"]));
        assert_eq!(o("piege_scelerat")["elements"], serde_json::json!(["Water"]));
        assert_eq!(o("calamite")["elements"], serde_json::json!(["Water"]));
        assert_eq!(o("guet_apens")["elements"], serde_json::json!(["Water"]));
        assert_eq!(o("invocation_de_l_arakne")["elements"], serde_json::json!(["Earth"]));
        assert_eq!(o("comploteur")["meilleur"], true);
        assert_eq!(o("concentration_de_chakra")["meilleur"], true);
        assert_eq!(o("peur")["agit"], false, "Peur ne fait rien dans le modèle");
        assert_eq!(o("pillage")["agit"], true);
    }

    /// Les quatre sorts communs sont au deck de chaque classe, par paires de
    /// variantes : l'Arakne ou l'Arakne Majeure, le Chaferfu ou le Chaferfu
    /// Lancier.
    #[test]
    fn les_sorts_communs_sont_au_deck_de_chaque_classe() {
        let catalogue: serde_json::Value = serde_json::from_str(&classes_json().unwrap()).unwrap();
        for classe in catalogue["classes"].as_array().unwrap() {
            let groupe = |id: &str| {
                classe["spells"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["id"] == id)
                    .unwrap_or_else(|| panic!("{} : pas de {id}", classe["label"]))["variant_group"]
                    .clone()
            };
            assert_eq!(groupe("invocation_de_l_arakne"), groupe("invocation_de_l_arakne_majeure"));
            assert_eq!(groupe("invocation_de_chaferfu"), groupe("invocation_de_chaferfu_lancier"));
            assert_ne!(groupe("invocation_de_l_arakne"), groupe("invocation_de_chaferfu"));
        }
    }

    /// La greffe elle-même : un compteur par invocation du deck, qui frappe en
    /// début de tour une fois par invocation en jeu (`repeats_per`) et par
    /// coup, à la part transmise ; un compteur partagé au plafond, qui ferme le
    /// sort d'invocation. Hors du deck, rien.
    #[test]
    fn la_greffe_des_invocations() {
        let requete: Request = serde_json::from_value(serde_json::json!({
            "class": 2, "level": 200, "items": vec![0u32; 17], "deck": ["tofu", "dragoune"], "horizon": 1,
        }))
        .unwrap();
        let build = requete.build.normalise();
        let resolved = resolve_build(&build).unwrap();
        let (regles, _) = avec_les_invocations(load_ruleset(2).unwrap(), &requete, &build, &resolved).expect("greffe");
        let etat = |id: &str| regles.resources.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("{id}"));
        assert_eq!(etat("invocations_en_jeu").max, 1);
        let tofu = etat("invocation_tofu");
        let lignes = &tofu.while_present[0].lines;
        assert_eq!(lignes.len(), 2, "un Béco-béco et un Bisou Béco par tour");
        assert!(lignes.iter().all(|l| l.invocation == Some(50) && l.repeats_per == ["invocation_tofu"]));
        let taux: Vec<Option<u8>> = lignes.iter().map(|l| l.taux_critique).collect();
        assert_eq!(taux, [Some(5), Some(10)], "le taux propre de chaque attaque");
        assert!(regles.resources.iter().any(|r| r.id == "invocation_dragoune"));
        assert!(!regles.resources.iter().any(|r| r.id == "invocation_bouftou"), "hors du deck");
        let sort = regles.spells.iter().find(|s| s.id == "tofu").unwrap();
        assert_eq!(sort.blocked_at_max.as_deref(), Some("invocations_en_jeu"));
        // Sans Pacte Bestial ni Cortège Sauvage au deck, une invocation reste
        // en jeu sans condition, et rien ne la fait mourir.
        assert!(sort.effects.iter().all(|e| !matches!(e, dofus_ruleset::Effect::Gain { requires: Some(_), .. })));
        assert!(tofu.reset_at_turn_start_while.is_none());
        let pacte = regles.spells.iter().find(|s| s.id == "pacte_bestial").unwrap();
        assert!(!pacte.effects.iter().any(|e| matches!(e, dofus_ruleset::Effect::Reset { resource, .. } if resource == "invocation_tofu")));
        // Le Martinet fait attaquer chaque invocation en jeu à 50 %, avec
        // l'attaque que son texte lui donne : Bisou Béco pour le Tofu,
        // Dracorage pour la Dragoune.
        let martinet = regles.spells.iter().find(|s| s.id == "martinet").unwrap();
        let des_invocations: Vec<_> = martinet.lines.iter().filter(|l| l.invocation.is_some()).collect();
        assert_eq!(des_invocations.len(), 2, "{des_invocations:?}");
        assert!(des_invocations.iter().all(|l| l.facteur == Some(50)));
        assert!(des_invocations.iter().any(|l| l.repeats_per == ["invocation_tofu"]
            && matches!(l.normal, dofus_ruleset::Maybe::Known((22, 25)))));
        // Piqûre Motivante : un mode sur une invocation, +2 PA pendant trois
        // tours (durée 2) à celle qu'ils servent le plus, tant qu'elle est là.
        let piqure = regles.spells.iter().find(|s| s.id == "piqure_motivante").unwrap();
        assert!(piqure.modes.iter().any(|m| m.name.fr == "sur une invocation"));
        let boost = etat("piqure_motivante_invocation");
        assert_eq!(boost.duration.as_ref().map(|d| d.turns), Some(2));
        assert!(matches!(
            &boost.while_present[0].requires,
            Some(dofus_ruleset::Condition::CasterHas { resource }) if resource.starts_with("invocation_")
        ));
        assert!(!boost.while_present[0].lines.is_empty());
    }

    /// Le Pacte Bestial et Cortège Sauvage dans la greffe : le sacrifice des
    /// créatures en jeu au lancer du Pacte, avant sa bascule, à 3 % par palier
    /// de PI ; une créature lancée sous Bestial, sacrifiée aussitôt ; trois
    /// places de plus sous Cortège, et des invocations qui meurent au début
    /// du tour tant qu'il tient.
    #[test]
    fn le_pacte_et_le_cortege_dans_la_greffe() {
        use dofus_ruleset::{Condition, Effect};
        let requete: Request = serde_json::from_value(serde_json::json!({
            "class": 2, "level": 200, "items": vec![0u32; 17], "horizon": 1,
            "deck": ["tofu", "craquolosse", "pacte_bestial", "cortege_sauvage"],
        }))
        .unwrap();
        let build = requete.build.normalise();
        let resolved = resolve_build(&build).unwrap();
        let (regles, notes) =
            avec_les_invocations(load_ruleset(2).unwrap(), &requete, &build, &resolved).expect("greffe");
        let etat = |id: &str| regles.resources.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("{id}"));
        // Une place sans équipement, trois de plus sous Cortège Sauvage.
        assert_eq!(etat("invocations_en_jeu").max, 4);
        let places = etat("places_d_invocation");
        assert_eq!((places.default, places.max), (1, 4));
        assert_eq!(places.duration.as_ref().map(|d| d.turns), Some(1));
        for id in ["invocations_en_jeu", "invocation_tofu", "invocation_craquolosse"] {
            assert_eq!(etat(id).max, 4, "{id}");
            assert_eq!(etat(id).reset_at_turn_start_while.as_deref(), Some("cortege_sauvage_morts"), "{id}");
        }
        let cortege = regles.spells.iter().find(|s| s.id == "cortege_sauvage").unwrap();
        assert!(cortege.effects.iter().any(|e| matches!(
            e,
            Effect::Gain { resource, amount: 3, .. } if resource == "places_d_invocation"
        )));
        // Un sort d'invocation : au plafond des places, et en jeu hors de
        // Bestial seulement ; sous Bestial, son palier en dommages finaux, et
        // l'état et ses PA relancés.
        let gains = |sort: &str| -> Vec<(String, u8, Option<Condition>)> {
            regles
                .spells
                .iter()
                .find(|s| s.id == sort)
                .unwrap()
                .effects
                .iter()
                .filter_map(|e| match e {
                    Effect::Gain { resource, amount, requires, .. } => Some((resource.clone(), *amount, requires.clone())),
                    _ => None,
                })
                .collect()
        };
        let craquolosse = regles.spells.iter().find(|s| s.id == "craquolosse").unwrap();
        let surplus = craquolosse.requires_more_than.as_ref().expect("plafond des places");
        assert_eq!((surplus.resource.as_str(), surplus.than.as_str()), ("places_d_invocation", "invocations_en_jeu"));
        assert!(craquolosse.blocked_at_max.is_none());
        let hors = |r: &Option<Condition>| {
            matches!(r, Some(Condition::Exactly { resource, amount: 0 }) if resource == "bestial")
        };
        let sous = |r: &Option<Condition>| matches!(r, Some(Condition::CasterHas { resource }) if resource == "bestial");
        let g = gains("craquolosse");
        assert!(g.iter().any(|(r, _, c)| r == "invocation_craquolosse" && hors(c)));
        assert!(g.iter().any(|(r, _, c)| r == "invocations_en_jeu" && hors(c)));
        for (r, n) in [("pacte_bestial_finaux", 3), ("pacte_bestial_sacrifices", 3), ("bestial", 1), ("pacte_bestial_pa", 1)] {
            assert!(g.iter().any(|(x, m, c)| x == r && *m == n && sous(c)), "{r} : {g:?}");
        }
        assert!(gains("tofu").iter().any(|(x, m, c)| x == "pacte_bestial_finaux" && *m == 1 && sous(c)));
        // Le Pacte : les gains du sacrifice, une par créature en jeu, puis les
        // compteurs qui retombent, le tout juste avant la bascule de l'état.
        let pacte = regles.spells.iter().find(|s| s.id == "pacte_bestial").unwrap();
        let bascule = pacte
            .effects
            .iter()
            .position(|e| matches!(e, Effect::Toggle { resource } if resource == "bestial"))
            .unwrap();
        assert_eq!(bascule, pacte.effects.len() - 1, "la bascule reste en dernier");
        let avant: Vec<&Effect> = pacte.effects[..bascule].iter().collect();
        for k in 1..=4u8 {
            assert!(avant.iter().any(|e| matches!(
                e,
                Effect::Gain { resource, amount: 3, requires: Some(Condition::AtLeast { resource: c, amount, .. }), .. }
                    if resource == "pacte_bestial_finaux" && c == "invocation_craquolosse" && *amount == k
            )), "le Craquolosse n°{k}");
        }
        let derniers: Vec<&str> = avant
            .iter()
            .rev()
            .take(3)
            .filter_map(|e| match e {
                Effect::Reset { resource, requires: None } => Some(resource.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(derniers, ["invocations_en_jeu", "invocation_craquolosse", "invocation_tofu"]);
        // Les plafonds de la chaîne : quatre places au palier 3, plus un Tofu
        // et un Craquolosse lancés sous l'état, 16 crans par tour au plus.
        assert_eq!(etat("pacte_bestial_sacrifices").max, 16);
        assert_eq!(etat("pacte_bestial_sacrifices_2").max, 16);
        assert_eq!(etat("pacte_bestial_finaux").max, 48);
        assert!(notes.iter().any(|n| n.starts_with("Pacte Bestial :")));
        assert!(notes.iter().any(|n| n.starts_with("Cortège Sauvage :")));
    }

    /// Une requête d'Osamodas sans équipement, une invocation en jeu au plus.
    fn osamodas(deck: &[&str]) -> Request {
        serde_json::from_value(serde_json::json!({
            "class": 2, "level": 200, "items": vec![0u32; 17], "horizon": 1, "deck": deck,
            "invested": { "strength": 100, "intelligence": 100, "chance": 100, "agility": 100 },
        }))
        .unwrap()
    }

    fn tours(t: &[&[&str]]) -> Vec<Vec<String>> {
        t.iter().map(|l| l.iter().map(|s| s.to_string()).collect()).collect()
    }

    /// Les dégâts du Fouet à chaque tour où il part, et qui frappe en début de
    /// tour.
    fn fouets_et_ouvertures(s: &dofus_engine::Solution) -> (Vec<f64>, Vec<Vec<String>>) {
        (
            s.turns
                .iter()
                .map(|t| t.casts.iter().filter(|c| c.id == "fouet").map(|c| c.damage.as_f64()).sum())
                .collect(),
            s.turns.iter().map(|t| t.opening_sources.iter().map(|(id, _)| id.clone()).collect()).collect(),
        )
    }

    /// Le Pacte Bestial rejoué : un Craquolosse (palier 3) sacrifié au lancer,
    /// +9 % ; un Tofu (palier 1) lancé sous l'état, sacrifié aussitôt, +3 % ;
    /// chacun pour trois tours. Chaque invocation lancée sous l'état le
    /// prolonge de trois tours : le second Tofu, cinquième tour, est encore
    /// sacrifié. Les invocations sacrifiées ne frappent plus.
    #[test]
    fn le_pacte_sacrifie_et_rend_ses_dommages_finaux() {
        let r = osamodas(&["tofu", "craquolosse", "pacte_bestial", "fouet"]);
        let s = rejouer(
            &r,
            &tours(&[
                &["craquolosse", "fouet"],
                &["pacte_bestial", "fouet"],
                &["tofu", "fouet"],
                &["fouet"],
                &["tofu", "fouet"],
                &["fouet"],
                &["fouet"],
                &["fouet"],
            ]),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let (f, ouvertures) = fouets_et_ouvertures(&s);
        // Le Craquolosse frappe au deuxième tour, avant le Pacte ; plus
        // personne ensuite.
        assert!(ouvertures[1].iter().any(|o| o == "craquolosse"), "{ouvertures:?}");
        assert!(ouvertures[2..].iter().all(|o| o.is_empty()), "{ouvertures:?}");
        // Les bonus, tour par tour : 0, 9, 12, 12, 6, 3, 3, 0 %. Les planchers
        // de chaque coup mangent un demi-point sur des coups d'une trentaine.
        assert_eq!(f[0], f[7], "{f:?}");
        assert_eq!(f[2], f[3], "{f:?}");
        assert_eq!(f[5], f[6], "{f:?}");
        let part = |x: f64| x / f[0];
        assert!((1.06..1.10).contains(&part(f[1])), "+9 % : {f:?}");
        assert!((1.09..1.13).contains(&part(f[2])), "+12 % : {f:?}");
        assert!((1.03..1.07).contains(&part(f[4])) && f[4] < f[1], "+6 % : {f:?}");
        assert!((1.00..1.04).contains(&part(f[5])) && f[5] > f[0] && f[5] < f[4], "+3 % : {f:?}");
    }

    /// Relancé sous l'état, le Pacte retire tout : le Fouet qui suit frappe
    /// sans bonus. Et il remet la relance des invocations : le Craquolosse,
    /// sacrifié au tour d'avant et encore en relance, repart dans le tour et
    /// reste en jeu.
    #[test]
    fn le_pacte_relance_retire_tout_et_rend_les_invocations() {
        let r = osamodas(&["craquolosse", "pacte_bestial", "fouet"]);
        let s = rejouer(
            &r,
            &tours(&[
                &["fouet"],
                &["pacte_bestial", "fouet"],
                &["craquolosse", "fouet"],
                &["pacte_bestial", "fouet", "craquolosse"],
                &[],
            ]),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let (f, ouvertures) = fouets_et_ouvertures(&s);
        // Rien à sacrifier au premier lancer ; le Craquolosse lancé sous
        // l'état, +9 %.
        assert_eq!(f[1], f[0], "{f:?}");
        assert!(f[2] > f[0], "{f:?}");
        assert_eq!(f[3], f[0], "plus aucun bonus après la relance : {f:?}");
        assert!(ouvertures[4].iter().any(|o| o == "craquolosse"), "{ouvertures:?}");
        // Sans la relance du Pacte, le Craquolosse reste en relance.
        let e = rejouer(&r, &tours(&[&["fouet"], &["pacte_bestial"], &["craquolosse"], &["craquolosse"]])).unwrap_err();
        assert!(e.starts_with("tour 4, lancer 1"), "{e}");
    }

    /// Cortège Sauvage rejoué : trois places de plus et des invocations à 1 PA
    /// de moins ; elles jouent leur tour au début du suivant, puis meurent,
    /// celles d'avant le Cortège comprises. Le Martinet n'a plus personne à
    /// faire attaquer.
    #[test]
    fn cortege_sauvage_fait_mourir_les_invocations_apres_leur_tour() {
        let r = osamodas(&["tofu", "dragoune", "craquolosse", "cortege_sauvage", "fouet", "martinet"]);
        // Sans le Cortège, une seule place.
        let e = rejouer(&r, &tours(&[&["craquolosse"], &["tofu"]])).unwrap_err();
        assert!(e.starts_with("tour 2, lancer 1"), "{e}");
        let s = rejouer(
            &r,
            &tours(&[
                &["craquolosse", "fouet"],
                &["cortege_sauvage", "tofu", "dragoune"],
                &["martinet", "tofu"],
                &["fouet"],
                &["fouet"],
            ]),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let couts: Vec<u8> = s.turns[1].casts.iter().map(|c| c.ap_cost).collect();
        assert_eq!(couts, [2, 2, 2], "le Cortège, puis le Tofu et la Dragoune à 2 PA au lieu de 3");
        let (_, ouvertures) = fouets_et_ouvertures(&s);
        let mut au_troisieme = ouvertures[2].clone();
        au_troisieme.sort();
        assert_eq!(au_troisieme, ["craquolosse", "dragoune", "tofu"]);
        // Le second tour du Cortège : un Tofu encore à 2 PA, qui joue au
        // début du quatrième et meurt à son tour.
        assert_eq!(s.turns[2].casts[1].ap_cost, 2);
        assert_eq!(ouvertures[3], ["tofu"]);
        assert!(ouvertures[4].is_empty(), "toutes mortes : {ouvertures:?}");
        // Le Martinet du troisième tour vaut celui d'un tour sans invocation.
        let seul = rejouer(&r, &tours(&[&["martinet"]])).unwrap_or_else(|e| panic!("{e}"));
        let martinet = |s: &dofus_engine::Solution, t: usize| {
            s.turns[t].casts.iter().find(|c| c.id == "martinet").map(|c| c.damage.as_f64()).unwrap()
        };
        assert_eq!(martinet(&s, 2), martinet(&seul, 0));
    }

    /// Une requête de Steamer sans équipement, une invocation en jeu au plus,
    /// tourelles au contact de la cible ou non.
    fn steamer(deck: &[&str], au_contact: bool) -> Request {
        serde_json::from_value(serde_json::json!({
            "class": 15, "level": 200, "items": vec![0u32; 17], "horizon": 1, "deck": deck,
            "invested": { "strength": 100, "intelligence": 100, "chance": 100, "agility": 100 },
            "etats_declares": { "tourelles_au_contact": u8::from(au_contact) },
        }))
        .unwrap()
    }

    /// Ce que la tourelle frappe au début de chaque tour.
    fn tourelle_par_tour(s: &dofus_engine::Solution) -> Vec<f64> {
        s.turns.iter().map(|t| t.opening_damage.as_f64()).collect()
    }

    /// Les dégâts d'un sort à chaque tour où il part.
    fn sort_par_tour(s: &dofus_engine::Solution, id: &str) -> Vec<f64> {
        s.turns.iter().map(|t| t.casts.iter().filter(|c| c.id == id).map(|c| c.damage.as_f64()).sum()).collect()
    }

    /// La Harponneuse rejouée : posée, elle frappe au palier I ; chaque
    /// Évolution la monte d'un palier, une fois par tour ; à l'Évolution III,
    /// Embuscade lui fait lancer son Armada sur la cible.
    #[test]
    fn la_harponneuse_evolue_et_lance_son_armada() {
        let r = steamer(&["harponneuse", "evolution", "embuscade"], false);
        let s = rejouer(
            &r,
            &tours(&[
                &["embuscade"],
                &["harponneuse"],
                &["evolution", "embuscade"],
                &["evolution", "embuscade"],
                &["embuscade"],
            ]),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        // Posée au deuxième tour, elle joue dès le début du troisième un
        // Espadon par tour : palier I, II, puis III. Au rang 3, 18-20 puis
        // 23-27 puis 30-34 (21-24, 28-32, 36-41 en critique, 15 %), doublés
        // par les 100 de caractéristique : 39,05, 51,5, 65,95.
        assert_eq!(tourelle_par_tour(&s), [0.0, 0.0, 39.05, 51.5, 65.95]);
        let e = sort_par_tour(&s, "embuscade");
        // L'Armada ne part qu'à l'Évolution III, atteinte au quatrième tour.
        assert_eq!(e[0], e[2], "{e:?}");
        assert!(e[3] > e[0] && e[4] == e[3], "{e:?}");
        // Une seule évolution par tour et par tourelle : montée par
        // Court-circuit, elle ne peut plus évoluer dans le tour.
        let r = steamer(&["harponneuse", "evolution", "court_circuit"], false);
        assert!(rejouer(&r, &tours(&[&["harponneuse"], &["evolution"]])).is_ok());
        let err = rejouer(&r, &tours(&[&["harponneuse"], &["court_circuit", "evolution"]])).unwrap_err();
        assert!(err.starts_with("tour 2, lancer 2"), "{err}");
        // Une Harponneuse à la fois.
        let (_, _, regles, _, _) = preparer(&r).unwrap();
        let sort = regles.spells.iter().find(|s| s.id == "harponneuse").unwrap();
        assert_eq!(sort.blocked_by.as_deref(), Some("tourelle_harponneuse"));
    }

    /// La Surtension porte la tourelle à l'Évolution III d'un coup ; elle
    /// joue son tour à ce palier, puis redescend à l'Évolution II. Une
    /// tourelle surtensée ne peut plus évoluer dans le tour.
    #[test]
    fn la_surtension_puis_le_cran_perdu() {
        let r = steamer(&["harponneuse", "evolution", "surtension"], false);
        let s = rejouer(&r, &tours(&[&["harponneuse"], &["surtension"], &[], &[]])).unwrap_or_else(|e| panic!("{e}"));
        let t = tourelle_par_tour(&s);
        let palier = |k: usize| {
            let p = rejouer(&r, &tours(&[&["harponneuse"], &["evolution"], &["evolution"], &[]])).unwrap();
            tourelle_par_tour(&p)[k]
        };
        // Palier I au deuxième tour ; III au troisième ; II au quatrième.
        assert_eq!(t[2], palier(3), "le palier III : {t:?}");
        assert_eq!(t[3], palier(2), "redescendue au palier II : {t:?}");
        assert!(t[2] > t[3] && t[3] > t[1], "{t:?}");
        let err = rejouer(&r, &tours(&[&["harponneuse"], &["surtension", "evolution"]])).unwrap_err();
        assert!(err.starts_with("tour 2, lancer 2"), "{err}");
    }

    /// Court-circuit frappe autour d'une tourelle : sans elle, il ne part pas ;
    /// avec elle, il la fait évoluer.
    #[test]
    fn court_circuit_exige_une_tourelle_et_la_fait_evoluer() {
        let r = steamer(&["harponneuse", "court_circuit"], false);
        let err = rejouer(&r, &tours(&[&["court_circuit"]])).unwrap_err();
        assert!(err.starts_with("tour 1, lancer 1"), "{err}");
        let s = rejouer(&r, &tours(&[&["harponneuse", "court_circuit"], &[]])).unwrap_or_else(|e| panic!("{e}"));
        let avec = tourelle_par_tour(&s)[1];
        let sans = tourelle_par_tour(&rejouer(&r, &tours(&[&["harponneuse"], &[]])).unwrap())[1];
        assert!(avec > sans, "palier II contre palier I : {avec} / {sans}");
    }

    /// Au contact de la cible : Vapor fait évoluer la tourelle, le Sabotage
    /// frappe plus fort pour elle une fois évoluée puis la rétrograde, Ancrage
    /// rend 1 PA. Sans le contact déclaré, rien de tout cela.
    #[test]
    fn les_tourelles_au_contact_de_la_cible() {
        let deck = ["harponneuse", "vapor", "sabotage", "ancrage"];
        let jouer = |au_contact: bool, t: &[&[&str]]| rejouer(&steamer(&deck, au_contact), &tours(t)).unwrap();
        let s = jouer(true, &[&["harponneuse", "vapor"], &[]]);
        let n = jouer(false, &[&["harponneuse", "vapor"], &[]]);
        assert!(tourelle_par_tour(&s)[1] > tourelle_par_tour(&n)[1], "Vapor fait évoluer");
        // Le Sabotage : +9 de base par tourelle évoluée.
        let s = jouer(true, &[&["harponneuse", "vapor"], &["sabotage"], &[]]);
        let n = jouer(true, &[&["harponneuse"], &["sabotage"], &[]]);
        let (evoluee, nue) = (sort_par_tour(&s, "sabotage")[1], sort_par_tour(&n, "sabotage")[1]);
        assert!(evoluee > nue + 9.0, "{evoluee} / {nue}");
        // Puis la rétrograde : le tour suivant, elle frappe au palier I.
        assert_eq!(tourelle_par_tour(&s)[2], tourelle_par_tour(&n)[2]);
        // Ancrage rend 1 PA quand une tourelle est dans sa zone.
        let pa = |au_contact: bool, t: &[&[&str]]| jouer(au_contact, t).turns.last().unwrap().ap_left;
        assert_eq!(pa(true, &[&["harponneuse", "ancrage"]]), pa(true, &[&["ancrage"]]) - 2 + 1);
        assert_eq!(pa(false, &[&["harponneuse", "ancrage"]]), pa(false, &[&["ancrage"]]) - 2);
    }

    /// Le conseil d'un lancer, depuis la page : la rotation rejouée jusqu'à
    /// lui, chaque sort essayé à sa place.
    #[test]
    fn le_conseil_d_un_lancer() {
        let r = osamodas(&["fouet", "cri_du_corbac", "pics_du_prespic"]);
        let d = DemandeDeConseil {
            requete: r,
            tours: tours(&[&["fouet", "cri_du_corbac"], &["fouet"]]),
            tour: 0,
            lancer: 1,
        };
        let v: serde_json::Value = serde_json::from_str(&conseil_json(&d).unwrap()).unwrap();
        assert_eq!(v["joue"], "cri_du_corbac");
        let noms: Vec<&str> = v["remplacants"].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap()).collect();
        assert!(noms.contains(&"Cri du Corbac") && noms.contains(&"Pics du Prespic"), "{v}");
        assert!(v["remplacants"][0]["dofusdb_id"].is_u64(), "{v}");
    }

    /// Une requête de Roublard sans équipement, son mur déclaré : deux
    /// Explobombes aux combos III et II, deux bombes bien placées par tour.
    fn roublard(deck: &[&str], deplace: bool) -> Request {
        serde_json::from_value(serde_json::json!({
            "class": 13, "level": 200, "items": vec![0u32; 17], "horizon": 1, "deck": deck,
            "mur_de_bombes": { "element": "feu", "combos": [3, 2] },
            "budgets": { "bombes_par_tour": 2 },
            "etats_declares": { "cible_bougee_dans_le_mur": u8::from(deplace) },
        }))
        .unwrap()
    }

    /// Le mur frappe de lui-même la cible qui s'y tient : quand il se forme sur
    /// elle, à la base du tour du lanceur, 30 à 33 au rang 3, à 30 % de combos, 39
    /// à 42 (40,5 en moyenne, sans critique) ; puis au début de chacun de ses tours,
    /// hors du tour du lanceur, 15 à 17, soit 19, 20 ou 22 (61/3 en moyenne). Les
    /// fourchettes de KrozBoom.
    #[test]
    fn le_mur_frappe_de_lui_meme() {
        let hors_tour = 61.0 / 3.0;
        let proche = |a: &[f64], b: &[f64]| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01);
        let r = roublard(&["explobombe", "tornabombe"], false);
        let s = rejouer(&r, &tours(&[&["explobombe"], &["explobombe"], &[], &[]])).unwrap_or_else(|e| panic!("{e}"));
        let ouvertures: Vec<f64> = s.turns.iter().map(|t| t.opening_damage.as_f64()).collect();
        // Une bombe : pas de mur. La seconde le forme sur la cible.
        assert_eq!(ouvertures[1], 0.0, "{ouvertures:?}");
        assert_eq!(s.turns[0].casts[0].damage.as_f64(), 0.0);
        assert_eq!(s.turns[1].casts[0].damage.as_f64(), 40.5);
        assert!(proche(&ouvertures[2..], &[hors_tour, hors_tour]), "{ouvertures:?}");
        // Deux poses dans le même tour : le mur se forme une fois.
        let s = rejouer(&r, &tours(&[&["explobombe", "tornabombe"], &[]])).unwrap_or_else(|e| panic!("{e}"));
        let poses: Vec<f64> = s.turns[0].casts.iter().map(|c| c.damage.as_f64()).collect();
        assert_eq!(poses, [0.0, 40.5]);
        assert!(proche(&[s.turns[1].opening_damage.as_f64()], &[hors_tour]));
        // Une troisième bombe ne le reforme pas : la cible s'y tenait déjà.
        let s = rejouer(&r, &tours(&[&["explobombe", "tornabombe"], &["explobombe"]])).unwrap();
        assert_eq!(s.turns[1].casts[0].damage.as_f64(), 0.0);
    }

    /// Déclaré, un sort qui déplace la cible dans le mur le redéclenche à la
    /// base de votre tour ; sans le réglage, ou sans mur, rien.
    #[test]
    fn le_mur_frappe_la_cible_qu_on_y_deplace() {
        let pulsar = |deplace: bool, t: &[&[&str]]| {
            let s = rejouer(&roublard(&["explobombe", "tornabombe", "pulsar"], deplace), &tours(t))
                .unwrap_or_else(|e| panic!("{e}"));
            s.turns.last().unwrap().casts.iter().filter(|c| c.id == "pulsar").map(|c| c.damage.as_f64()).sum::<f64>()
        };
        let avec_mur = &[&["explobombe", "tornabombe"][..], &["pulsar"][..]];
        assert_eq!(pulsar(true, avec_mur), pulsar(false, avec_mur) + 40.5);
        let sans_mur = &[&["explobombe"][..], &["pulsar"][..]];
        assert_eq!(pulsar(true, sans_mur), pulsar(false, sans_mur));
    }

    /// Une requête de niveau 200 sans équipement, ses réglages déclarés.
    fn classe_nue(classe: u32, deck: &[&str], etats: serde_json::Value) -> Request {
        serde_json::from_value(serde_json::json!({
            "class": classe, "level": 200, "items": vec![0u32; 17], "horizon": 1, "deck": deck,
            "invested": { "agility": 300, "strength": 300, "intelligence": 200, "chance": 200 },
            "etats_declares": etats,
        }))
        .unwrap()
    }

    fn degats_du_sort(s: &dofus_engine::Solution, tour: usize, id: &str) -> f64 {
        s.turns[tour].casts.iter().filter(|c| c.id == id).map(|c| c.damage.as_f64()).sum()
    }

    /// Les pièges du Sram que la cible déclenche : deux par tour déclarés, deux
    /// poses comptées, la troisième refusée ; une réserve pleine au tour suivant.
    /// Le Piège Fangeux vaut ce que KrozTrap lui donne, la cible entrée sur sa
    /// case.
    #[test]
    fn les_pieges_que_la_cible_declenche() {
        let deck = ["piege_fangeux", "piege_sournois", "piege_mortel"];
        let r = classe_nue(4, &deck, serde_json::json!({ "pieges_declenches": 2 }));
        let s = rejouer(&r, &tours(&[&["piege_fangeux", "piege_sournois"], &["piege_mortel"]]))
            .unwrap_or_else(|e| panic!("{e}"));
        let fangeux = degats_du_sort(&s, 0, "piege_fangeux");
        assert!(fangeux > 0.0 && degats_du_sort(&s, 0, "piege_sournois") > 0.0 && degats_du_sort(&s, 1, "piege_mortel") > 0.0);
        // Trois pièges à 2 PA tiennent dans les PA du tour : c'est la réserve
        // de deux qui refuse le troisième.
        let legers = classe_nue(4, &["piege_sournois", "piege_repulsif", "piege_insidieux"], serde_json::json!({ "pieges_declenches": 2 }));
        let err = rejouer(&legers, &tours(&[&["piege_sournois", "piege_repulsif", "piege_insidieux"]])).unwrap_err();
        assert!(err.starts_with("tour 1, lancer 3"), "{err}");
        let trois = classe_nue(4, &["piege_sournois", "piege_repulsif", "piege_insidieux"], serde_json::json!({ "pieges_declenches": 3 }));
        assert!(rejouer(&trois, &tours(&[&["piege_sournois", "piege_repulsif", "piege_insidieux"]])).is_ok());
        // Sans réglage, un piège posé ne compte rien.
        let nu = rejouer(&classe_nue(4, &deck, serde_json::json!({})), &tours(&[&["piege_fangeux"]])).unwrap();
        assert_eq!(degats_du_sort(&nu, 0, "piege_fangeux"), 0.0);
        // KrozTrap : le même build, le Sram à deux cases, la cible sur le piège.
        // Le Fangeux, et le Piège à Fragmentation, dont les trois anneaux
        // épargnent la cible debout sur son centre.
        for sort in ["piege_fangeux", "piege_a_fragmentation"] {
            let kroz: crate::reseau::RequeteReseau = serde_json::from_value(serde_json::json!({
                "class": 4, "level": 200,
                "invested": { "agility": 300, "strength": 300, "intelligence": 200, "chance": 200 },
                "lanceur": [0, 0], "poses": [{ "sort": sort, "case": [2, 0] }], "entree": [2, 0],
            }))
            .unwrap();
            let v: serde_json::Value = serde_json::from_str(&crate::reseau::reseau_json(&kroz).unwrap()).unwrap();
            let (lo, hi) = (v["chaine"]["total"][0].as_f64().unwrap(), v["chaine"]["total"][1].as_f64().unwrap());
            let r = classe_nue(4, &[sort], serde_json::json!({ "pieges_declenches": 1 }));
            let rotation = degats_du_sort(&rejouer(&r, &tours(&[&[sort]])).unwrap(), 0, sort);
            assert!(
                lo <= rotation && rotation <= hi && (rotation - (lo + hi) / 2.0).abs() <= 1.0,
                "{sort} : {rotation} hors de {lo}-{hi}"
            );
        }
    }

    /// La Concentration de Chakra vole à chaque piège déclenché tant qu'elle
    /// tient ; le poison du Piège Insidieux tombe au début du tour suivant.
    #[test]
    fn la_chakra_et_le_poison_des_pieges() {
        let deck = ["concentration_de_chakra", "piege_sournois", "piege_insidieux"];
        let r = classe_nue(4, &deck, serde_json::json!({ "pieges_declenches": 2 }));
        let avec = rejouer(&r, &tours(&[&[], &["concentration_de_chakra", "piege_sournois"]])).unwrap_or_else(|e| panic!("{e}"));
        let sans = rejouer(&r, &tours(&[&[], &["piege_sournois"]])).unwrap();
        let vol = degats_du_sort(&avec, 1, "piege_sournois") - degats_du_sort(&sans, 1, "piege_sournois");
        assert!(vol >= 12.0, "12 de base montés par le build, mesuré {vol}");
        let p = rejouer(&r, &tours(&[&["piege_insidieux"], &[]])).unwrap();
        assert!(p.turns[1].opening_damage.as_f64() > 0.0, "le poison du piège");
    }

    /// Les limites de lancer des règles sont celles de la donnée, pour tous les
    /// sorts de toutes les classes. Deux écritures seulement s'en écartent, et
    /// elles disent la même chose : « sans limite » s'écrit 1 par tour pour un sort
    /// qui a une relance, et 6 ou plus pour un autre, que les PA bornent.
    #[test]
    fn les_limites_de_lancer_suivent_la_donnee() {
        let mut ecarts = Vec::new();
        let mut vus = 0;
        for (classe, _, _) in CLASSES {
            let regles = load_ruleset(*classe).unwrap();
            let snap = snapshot_for(*classe).unwrap();
            for s in &regles.spells {
                let Some(niveau) = s
                    .dofusdb_id
                    .and_then(|id| snap.spells.iter().find(|x| x.id == id))
                    .and_then(|x| x.levels.iter().max_by_key(|l| l.grade))
                else {
                    continue;
                };
                vus += 1;
                let (tour, cible, relance) = (
                    niveau.max_cast_per_turn.unwrap_or(0),
                    niveau.max_cast_per_target.unwrap_or(0),
                    niveau.min_cast_interval.unwrap_or(0),
                );
                let tour_ok = match tour {
                    0 if relance > 0 => s.casts_per_turn == 1,
                    0 => s.casts_per_turn >= 6,
                    n => s.casts_per_turn == n,
                };
                if !tour_ok || s.casts_per_target.unwrap_or(0) != cible {
                    ecarts.push(format!(
                        "{} : règles {}/{:?}, donnée {tour}/{cible}",
                        s.id, s.casts_per_turn, s.casts_per_target
                    ));
                }
            }
        }
        assert!(vus > 800, "{vus}");
        assert!(ecarts.is_empty(), "{ecarts:#?}");
    }

    /// Vendetta du Crâ et la Barrière du Féca, déclarées déclenchées : leurs
    /// dégâts tombent à la pose ; sans le réglage, rien.
    #[test]
    fn vendetta_et_la_barriere() {
        for (classe, sort) in [(9, "vendetta"), (1, "barriere")] {
            let compte = |n: u8| {
                let r = classe_nue(classe, &[sort], serde_json::json!({ "pieges_declenches": n }));
                degats_du_sort(&rejouer(&r, &tours(&[&[sort]])).unwrap_or_else(|e| panic!("{sort} : {e}")), 0, sort)
            };
            assert!(compte(1) > 0.0, "{sort}");
            assert_eq!(compte(0), 0.0, "{sort}");
        }
    }

    /// Vendetta en 3.7 : son piège, en partant, pose ×110 % de dommages subis pour
    /// le tour. La Flèche Perforante qui suit frappe 10 % plus fort ; sans le piège
    /// déclenché, pareil qu'à nu.
    #[test]
    fn le_piege_de_vendetta_monte_les_coups_qui_suivent() {
        let perforante = |pieges: u8, tour: &[&str]| {
            let r = classe_nue(9, &["vendetta", "fleche_perforante"], serde_json::json!({ "pieges_declenches": pieges }));
            degats_du_sort(&rejouer(&r, &tours(&[tour])).unwrap(), 0, "fleche_perforante")
        };
        let nue = perforante(1, &["fleche_perforante"]);
        let apres = perforante(1, &["vendetta", "fleche_perforante"]);
        assert!((apres / nue - 1.10).abs() < 0.01, "{nue} {apres}");
        assert_eq!(perforante(0, &["vendetta", "fleche_perforante"]), nue);
    }

    /// Tout sort que les règles laissent sans ligne ni effet, dans une classe
    /// qui déclare des pièges, et qui frappe au sol dans la donnée ou porte
    /// `trap_lines`, reçoit ses lignes quand la cible le déclenche. C'est
    /// l'autre moitié de l'excuse que `classes_completes_verifiees` lui fait :
    /// aucun ne reste compté à zéro.
    #[test]
    fn tout_piege_declare_recoit_ses_lignes() {
        let mut vus = 0;
        for (classe, _, _) in CLASSES {
            let regles = load_ruleset(*classe).unwrap();
            let Some(max) = regles.resources.iter().find(|r| r.id == "pieges_declenches").map(|r| r.max) else {
                continue;
            };
            let snap = snapshot_for(*classe).unwrap();
            let au_sol = |id: Option<u32>| {
                id.and_then(|id| snap.spells.iter().find(|s| s.id == id)).is_some_and(|s| {
                    s.levels.iter().any(|l| !l.placed_lines.is_empty() || !l.placed_critical_lines.is_empty())
                })
            };
            let pieges: Vec<&str> = regles
                .spells
                .iter()
                .filter(|s| s.lines.is_empty() && s.effects.is_empty() && (!s.trap_lines.is_empty() || au_sol(s.dofusdb_id)))
                .map(|s| s.id.as_str())
                .collect();
            let (_, _, greffees, _, _) =
                preparer(&classe_nue(*classe, &pieges, serde_json::json!({ "pieges_declenches": max }))).unwrap();
            for id in &pieges {
                let s = greffees.spells.iter().find(|s| s.id == *id).unwrap();
                assert!(!s.lines.is_empty(), "classe {classe} : {id} reste sans ligne");
                vus += 1;
            }
        }
        // Les onze pièges du Sram, Vendetta du Crâ, la Barrière du Féca.
        assert_eq!(vus, 13);
    }

    /// Les pioches au hasard de l'Ecaflip nourrissent la Rekop : la seconde passe
    /// rend au deck Redistribution, qui n'a pas de dégâts à elle, et la Rekop part
    /// sur la moyenne de quatre cartes tirées. Une Bonne Pioche gratuite qui ne
    /// rapporte rien ne s'ajoute pas. Sans la Main de poker cochée, rien.
    #[test]
    fn les_pioches_au_hasard_nourrissent_la_rekop() {
        let joue = |mecaniques: &[&str]| -> (f64, Vec<String>) {
            let r: Request = serde_json::from_value(serde_json::json!({
                "class": 6, "level": 200, "items": vec![0u32; 17], "horizon": 1,
                "deck": ["rekop", "bonne_pioche", "redistribution"],
                "invested": { "agility": 300, "strength": 300, "intelligence": 200, "chance": 200 },
                "mecaniques": mecaniques,
            }))
            .unwrap();
            let v: serde_json::Value = serde_json::from_str(&solve_json(&r).expect("rotation")).unwrap();
            let casts = v["rotation"]["turns"][0]["casts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["id"].as_str().unwrap().to_string())
                .collect();
            (v["rotation"]["total"].as_f64().unwrap(), casts)
        };
        let (total, casts) = joue(&["main_de_poker"]);
        assert!(total > 0.0);
        assert_eq!(casts, ["redistribution", "rekop"]);
        assert_eq!(joue(&[]).0, 0.0);
    }

    /// Le Holmgang du Forgelance : son aura frappe une fois au début du tour
    /// suivant, si la cible la traverse ; il termine le tour. La Parade riposte
    /// une fois si la cible frappe au contact.
    #[test]
    fn le_holmgang_et_la_parade() {
        let ouverture = |sort: &str, etats: serde_json::Value| {
            let r = classe_nue(20, &[sort], etats);
            rejouer(&r, &tours(&[&[sort], &[]])).unwrap_or_else(|e| panic!("{sort} : {e}")).turns[1].opening_damage.as_f64()
        };
        assert!(ouverture("holmgang", serde_json::json!({ "pieges_declenches": 1 })) > 0.0);
        assert_eq!(ouverture("holmgang", serde_json::json!({})), 0.0);
        assert!(ouverture("parade", serde_json::json!({ "coups_de_melee_subis": 1 })) > 0.0);
        assert_eq!(ouverture("parade", serde_json::json!({})), 0.0);
        // Rien ne part après le Holmgang.
        let r = classe_nue(20, &["holmgang", "parade"], serde_json::json!({}));
        let s = rejouer(&r, &tours(&[&["holmgang"]])).unwrap();
        assert_eq!(s.turns[0].ap_left, 0);
        assert!(rejouer(&r, &tours(&[&["holmgang", "parade"]])).unwrap_err().starts_with("tour 1, lancer 2"));
    }

    /// Le Cauchemar ne donne ses +100 Puissance qu'aux coups qui suivent la
    /// poussée : sur un tour de deux Espingoles, la première, qui pousse, frappe
    /// comme sans le Dofus, et la seconde exactement comme avec 100 Puissance de
    /// plus.
    #[test]
    fn le_cauchemar_arme_les_coups_qui_suivent_la_poussee() {
        let joue = |objets: Vec<u32>, puissance: i32| -> Vec<f64> {
            let v = serde_json::json!({
                "class": 13, "level": 200, "items": objets, "deck": ["espingole"], "horizon": 1,
                "poussees_bloquees": true, "invested": { "agility": 100, "power": puissance },
            });
            let r: serde_json::Value =
                serde_json::from_str(&solve_json(&serde_json::from_value(v).unwrap()).expect("rotation")).unwrap();
            r["rotation"]["turns"][0]["casts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["damage"].as_f64().unwrap())
                .collect()
        };
        let mut cauchemar = vec![0u32; 17];
        cauchemar[11] = 26066;
        let avec = joue(cauchemar, 0);
        let sans = joue(vec![0u32; 17], 0);
        let armee = joue(vec![0u32; 17], 100);
        assert_eq!(avec.len(), 2, "deux Espingoles : {avec:?}");
        assert!((avec[0] - sans[0]).abs() < 0.01, "la première sans bonus : {avec:?} / {sans:?}");
        assert!((avec[1] - armee[1]).abs() < 0.01, "la seconde à +100 : {avec:?} / {armee:?}");
        assert!(avec[1] > avec[0] + 1.0, "{avec:?}");
    }
}

/// Où en est la recherche en cours, pour la barre de l'interface.
///
/// Lecture seule et sans verrou : la page l'interroge pendant que le calcul
/// tourne, sur un autre fil que celui qui résout.
pub fn avancement_json() -> String {
    let (tour, tours, fraction, etats, en_cours) = dofus_engine::AVANCEMENT.lire();
    serde_json::json!({
        "turn": tour,
        "turns": tours,
        "fraction": fraction,
        "states": etats,
        "running": en_cours,
    })
    .to_string()
}
