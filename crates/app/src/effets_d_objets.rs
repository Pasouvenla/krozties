//! Les effets spéciaux des objets, greffés aux règles de la classe comme des
//! compteurs.
//!
//! Un effet spécial est un sort de l'objet (`data/snapshots/sorts-d-objets.json`).
//! Ceux d'ici changent les dégâts d'une rotation tour par tour : un compteur qui
//! monte d'un cran par tour porte le bonus de chaque tour
//! (`ResourceDef::paliers`), et reboucle quand l'effet est un cycle. Les chiffres
//! et les tours viennent de la donnée de chaque sort, confrontée à son texte : la
//! Surpryz pose 100, 35 puis 15 % de critique avec 0, 1 et 2 tours de délai.
//!
//! Ce qui dépend du combat (le contact d'un ennemi, la vie du porteur) a sa case
//! dans la Rotation, cochée d'office ; le reste se dit dans les remarques du
//! calcul. D'autres répondent aux actions du porteur : un retrait de PA ou de PM
//! (Couronne de Brâm Barbe-Monde), un coup critique (Plume de Buhorado), ce qui
//! part seul au début ou à la fin de son tour (Bottes du Cul Botté, Crocobur,
//! Audace de Dodge). Un retrait ou un critique donne ses crans par les sorts qui
//! les font.

use std::collections::HashMap;

use dofus_build::{BonusConditionnel, BuildInput};
use dofus_ruleset::{Ruleset, SpellDef};
use serde_json::{json, Value};

/// Le combat déclaré, ce dont les effets dépendent.
pub struct Combat {
    pub ennemis: u8,
    /// « Vos poussées butent contre un obstacle » : sans ce réglage, une
    /// poussée ne fait pas de dégâts.
    pub poussees_bloquees: bool,
    /// Les PM que le joueur déclare dépenser à chaque tour.
    pub pm_depenses: u8,
}

/// Ce que les effets lisent dans la donnée de la classe, une fois.
#[derive(Default)]
pub struct Lectures {
    /// Les retraits de PA ou de PM que chaque sort tente sur l'ennemi, par
    /// `dofusdb_id`.
    pub retraits: HashMap<u32, u8>,
    /// Les sorts qui téléportent ou échangent leur lanceur.
    pub deplacements: Vec<u32>,
}

impl Lectures {
    pub fn depuis(instantane: &dofus_ruleset::snapshot::Snapshot) -> Lectures {
        Lectures {
            retraits: instantane.retraits_sur_l_ennemi(),
            deplacements: instantane.sorts_qui_deplacent_le_lanceur(),
        }
    }
}

/// Ce que les effets greffés ont changé, en plus des règles.
#[derive(Default)]
pub struct Greffes {
    /// Les remarques du calcul.
    pub remarques: Vec<String>,
    /// La cible subit des dommages de poussée à chaque tour : le réglage que
    /// déclarent le Crâ, le Steamer et le Forgelance (`poussee_subie`), que les
    /// Bottes du Cul Botté rendent vrai.
    pub poussee_subie: bool,
}

/// Ce qu'un effet change, une fois résolu sur le combat déclaré.
#[derive(Default)]
struct Greffe {
    compteurs: Vec<Value>,
    /// Ce qui, dans un lancer, donne des crans à ces compteurs.
    declencheur: Option<Declencheur>,
    /// Une poussée à chaque tour, que d'autres effets lisent : voir
    /// [`Greffes::poussee_subie`] et l'Éternel Cauchemar.
    pousse_a_chaque_tour: bool,
    /// Des tentatives de retrait de PA ou de PM faites en FIN de tour, sans
    /// sort : autant de cumuls de la Couronne de Brâm pour le tour suivant.
    retraits_de_fin_de_tour: u8,
    remarque: String,
}

/// Ce qui, dans un lancer, déclenche un effet.
enum Declencheur {
    /// Une tentative de retrait de PA ou de PM sur l'ennemi : un cran par
    /// retrait que le sort porte, à chacun de ces compteurs.
    Retrait(&'static [&'static str]),
    /// Un coup critique : un cran à chacun de ces compteurs.
    Critique(&'static [&'static str]),
    /// Une téléportation ou un échange de position du porteur par l'un de ses
    /// sorts : 1 PA tout de suite, au premier tour seulement (`tour` y vaut
    /// 1), une fois (`compteur` plafonne à un).
    Deplacement { compteur: &'static str, tour: &'static str },
}

/// Un effet d'objet modélisé.
struct Effet {
    /// Les objets qui le portent, par leur identifiant.
    objet: u32,
    /// Le nom de l'objet : celui de la case et des remarques.
    nom: &'static str,
    /// Ce dont il dépend en combat, s'il en dépend : sa case dans la Rotation.
    condition: Option<(&'static str, &'static str)>,
    /// Ses compteurs, sur le combat déclaré.
    greffe: fn(&Combat) -> Greffe,
    /// Ce qu'il devient quand sa case est décochée, s'il devient quelque
    /// chose : l'Audace de Dodge donne des Dommages Poussée au porteur qui ne
    /// change pas de case.
    sinon: Option<fn(&Combat) -> Greffe>,
}

/// Les compteurs qui infligent des dégâts d'eux-mêmes, avec l'objet qui les
/// porte : la répartition des dégâts les nomme comme des sorts.
const SOURCES: &[(&str, u32, &str)] =
    &[("objet_crocobur", 20353, "Crocobur"), ("objet_cul_botte", 20364, "Bottes du Cul Botté")];

/// Le nom et l'objet d'un compteur qui inflige des dégâts de lui-même.
pub fn source(id: &str) -> Option<(&'static str, u32)> {
    SOURCES.iter().find(|(c, _, _)| *c == id).map(|(_, objet, nom)| (*nom, *objet))
}

/// Les effets qui ne changent les dégâts qu'avec un autre objet porté : le
/// retrait de fin de tour de la Hachebarde de Guerre et du Frisson de Brumaire
/// ne compte que pour la Couronne de Brâm Barbe-Monde.
const AVEC: &[(u32, u32)] = &[(22368, 20359), (20355, 20359)];

/// L'effet de cet objet compte-t-il avec ces objets portés ?
fn s_applique(objet: u32, portes: &[u32]) -> bool {
    AVEC.iter().filter(|(o, _)| *o == objet).all(|(_, autre)| portes.contains(autre))
}

/// Ce que tous les effets d'un même compteur ont en commun : une remarque.
fn simple(compteur: Value, remarque: impl Into<String>) -> Greffe {
    Greffe { compteurs: vec![compteur], remarque: remarque.into(), ..Default::default() }
}

/// Un compteur qui monte d'un cran par tour, de 1 au premier : `paliers[t - 1]`
/// et `pa[t - 1]` valent au tour `t`, rien au-delà du dernier.
fn par_tour(id: &str, paliers: Vec<Value>, pa: Vec<i8>) -> Value {
    let max = paliers.len().max(pa.len()) + 1;
    json!({
        "id": id, "scope": "caster", "max": max, "default": 0, "monotone": "increasing",
        "gain_per_turn": 1, "paliers": paliers, "pa_par_valeur": pa,
    })
}

/// Le même, en boucle : `paliers[(t - 1) % n]`.
fn en_boucle(id: &str, paliers: Vec<Value>, pa: Vec<i8>) -> Value {
    let max = paliers.len().max(pa.len());
    json!({
        "id": id, "scope": "caster", "max": max, "default": 0, "gain_per_turn": 1,
        "cyclique": true, "paliers": paliers, "pa_par_valeur": pa,
    })
}

/// Un bonus présent à partir du tour `tour`, et jusqu'au bout : le compteur
/// s'arrête sur la valeur qui le porte.
fn a_partir_du_tour(id: &str, tour: u8, bonus: Vec<Value>) -> Value {
    let mut paliers = vec![json!([]); usize::from(tour) - 1];
    paliers.push(Value::Array(bonus));
    json!({
        "id": id, "scope": "caster", "max": tour, "default": 0, "monotone": "increasing",
        "gain_per_turn": 1, "paliers": paliers,
    })
}

/// Un bonus présent à chaque tour.
fn toujours(id: &str, bonus: Vec<Value>) -> Value {
    json!({ "id": id, "scope": "caster", "max": 1, "default": 1, "modifies_damage": bonus })
}

fn critique(p: i32) -> Value {
    json!({ "kind": "critical_rate", "percent": p })
}
/// Des % de dommages finaux, qui s'additionnent aux autres.
fn finaux(p: i32) -> Value {
    json!({ "kind": "final_multiplier", "percent": 100 + p, "finaux": true })
}
/// Des dommages subis par la cible, un facteur à part.
fn subis(p: i32) -> Value {
    json!({ "kind": "final_multiplier", "percent": 100 + p })
}
fn caracteristique(element: &str, n: i32) -> Value {
    json!({ "kind": "characteristic", "amount": n, "element": element })
}
/// Des % d'un domaine : `reach` (`melee`, `ranged`) ou `delivery` (`spell`,
/// `weapon`).
fn domaine(cle: &str, valeur: &str, p: i32) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("kind".into(), json!("domain_percent"));
    m.insert(cle.into(), json!(valeur));
    m.insert("percent".into(), json!(p));
    Value::Object(m)
}
fn dommages_poussee(n: i32) -> Value {
    json!({ "kind": "push_damage", "amount": n })
}

const EFFETS: &[Effet] = &[
    // Prysmaradites : leur effet tient aux trois premiers tours.
    Effet { objet: 22001, nom: "Surpryz", condition: None, sinon: None, greffe: |_| simple(
        par_tour("objet_surpryz", vec![json!([critique(100)]), json!([critique(35)]), json!([critique(15)])], vec![]),
        "Surpryz : +100 % de critique au premier tour, +35 % au deuxième, +15 % au troisième",
    ) },
    Effet { objet: 21996, nom: "Pryssion Mate", condition: None, sinon: None, greffe: |_| simple(
        par_tour("objet_pryssion", vec![json!([finaux(-10)]); 3], vec![1; 3]),
        "Pryssion Mate : -10 % de dommages finaux et +1 PA les trois premiers tours",
    ) },
    Effet { objet: 21997, nom: "Pryssion Brillante", condition: None, sinon: None, greffe: |_| simple(
        par_tour("objet_pryssion", vec![json!([finaux(-35)]); 2], vec![2; 2]),
        "Pryssion Brillante : -35 % de dommages finaux et +2 PA les deux premiers tours",
    ) },
    Effet { objet: 21998, nom: "Pryssion Iridescente", condition: None, sinon: None, greffe: |_| simple(
        par_tour("objet_pryssion", vec![json!([finaux(-50)])], vec![3]),
        "Pryssion Iridescente : -50 % de dommages finaux et +3 PA au premier tour",
    ) },
    Effet { objet: 22011, nom: "Prycipithon Mate", condition: None, sinon: None, greffe: |_| simple(
        par_tour("objet_prycipithon", vec![], vec![2]),
        "Prycipithon Mate : +2 PA au premier tour",
    ) },
    Effet { objet: 22012, nom: "Prycipithon Brillante", condition: None, sinon: None, greffe: |_| simple(
        par_tour("objet_prycipithon", vec![], vec![3]),
        "Prycipithon Brillante : +3 PA au premier tour ; ses 2 PM retirés ne changent pas les dégâts",
    ) },
    Effet { objet: 22013, nom: "Prycipithon Iridescente", condition: None, sinon: None, greffe: |_| simple(
        par_tour("objet_prycipithon", vec![], vec![4]),
        "Prycipithon Iridescente : +4 PA au premier tour ; ses 4 PM retirés ne changent pas les dégâts",
    ) },
    Effet { objet: 22004, nom: "Prynyang", condition: None, sinon: None, greffe: |_| simple(
        par_tour("objet_prynyang", vec![json!([finaux(10)]), json!([finaux(3)]), json!([finaux(-10)])], vec![]),
        "Prynyang : +10 % de dommages finaux au premier tour, +3 % au deuxième, -10 % au troisième",
    ) },
    // Posée au début PUIS à la fin du premier tour, pour trois tours chacune :
    // la seconde ne sert qu'aux tours 2 et 3 (règle des durées, un
    // effet posé au tour N pour 3 tours tient jusqu'au tour N + 2).
    Effet {
        objet: 22023,
        nom: "Pryximite",
        condition: Some(("+2 % de dommages en mêlée par ennemi proche", "si les ennemis sont à 3 cases ou moins au premier tour")),
        sinon: None,
        greffe: |c| {
            let p = 2 * i32::from(c.ennemis);
            simple(
                par_tour("objet_pryximite", vec![
                    json!([domaine("reach", "melee", p)]),
                    json!([domaine("reach", "melee", 2 * p)]),
                    json!([domaine("reach", "melee", 2 * p)]),
                ], vec![]),
                format!("Pryximite : +{p} % de dommages en mêlée au premier tour, +{} % aux deux suivants", 2 * p),
            )
        },
    },
    // Légendaires et Dofus.
    Effet { objet: 20360, nom: "Diadème de Ganymède", condition: None, sinon: None, greffe: |_| simple(
        en_boucle("objet_ganymede", vec![], vec![-1, 2]),
        "Diadème de Ganymède : -1 PA les tours impairs, +2 PA les pairs ; son PM retiré ne change pas les dégâts",
    ) },
    // La donnée lance un sous-sort par caractéristique, à 0, 1, 2 puis 3 tours
    // de délai, et le dernier relance le cycle.
    Effet { objet: 958, nom: "Dofusteuse", condition: None, sinon: None, greffe: |_| simple(
        en_boucle("objet_dofusteuse", vec![
            json!([caracteristique("water", 400)]),
            json!([caracteristique("earth", 400)]),
            json!([caracteristique("air", 400)]),
            json!([caracteristique("fire", 400)]),
        ], vec![]),
        "Dofusteuse : +400 de Chance au premier tour, de Force au deuxième, d'Agilité au troisième, d'Intelligence au quatrième, puis de nouveau",
    ) },
    Effet { objet: 32114, nom: "Ardeur d'Oto Mustam", condition: None, sinon: None, greffe: |c| {
        let p = i32::from(c.ennemis.min(4));
        simple(
            toujours("objet_oto_mustam", vec![finaux(p)]),
            format!("Ardeur d'Oto Mustam : +{p} % de dommages finaux, un par ennemi en vie (4 au plus)"),
        )
    } },
    Effet { objet: 32117, nom: "Jugement de Thanatena", condition: None, sinon: None, greffe: |_| simple(
        toujours("objet_thanatena", vec![subis(4)]),
        "Jugement de Thanatena : +4 % de dommages subis par la cible à chacun de vos tours",
    ) },
    Effet {
        objet: 20358,
        nom: "Trompe-la-Mort",
        condition: Some(("+7 % de dommages finaux", "tant que vous gardez plus de 50 % de vie ; il vous en retire 10 % par tour")),
        sinon: None,
        greffe: |_| simple(
            toujours("objet_trompe_la_mort", vec![finaux(7)]),
            "Trompe-la-Mort : +7 % de dommages finaux, plus de 50 % de vie gardés",
        ),
    },
    // Posés en fin de tour sur les ennemis au contact : ils servent dès le tour
    // suivant. Le Courage pose ses % aux sorts pour DEUX tours dans la donnée
    // (effet 2812, durée 2), là où son texte dit un.
    Effet {
        objet: 20354,
        nom: "Ciseaux du Destin",
        condition: Some(("+6 % de dommages subis par les ennemis au contact", "si un ennemi est à votre contact en fin de tour")),
        sinon: None,
        greffe: |_| simple(
            a_partir_du_tour("objet_ciseaux", 2, vec![subis(6)]),
            "Ciseaux du Destin : +6 % de dommages subis par les ennemis au contact, dès le deuxième tour",
        ),
    },
    Effet {
        objet: 20366,
        nom: "Courage de Dame Jhessica",
        condition: Some(("+1 % de dommages aux sorts par ennemi au contact", "si un ennemi est à votre contact en fin de tour")),
        sinon: None,
        greffe: |c| {
            let p = i32::from(c.ennemis.min(4));
            simple(
                a_partir_du_tour("objet_jhessica", 2, vec![domaine("delivery", "spell", p)]),
                format!("Courage de Dame Jhessica : +{p} % de dommages aux sorts dès le deuxième tour, un par ennemi au contact"),
            )
        },
    },
    // Ce qui répond aux actions du porteur.
    //
    // La Couronne : +2 % de dommages finaux pour 2 tours par tentative de
    // retrait de PA ou de PM, 5 cumuls (grade 5 du sort 11360, `maxStack` 5).
    // Posés au tour N, ils tiennent jusqu'au tour N + 1 : le total du tour
    // suivant repart de ceux de ce tour-ci, qui glissent en fin de tour. Au
    // plafond, un cumul de plus remplace le plus ancien.
    Effet {
        objet: 20359,
        nom: "Couronne de Brâm Barbe-Monde",
        condition: None,
        sinon: None,
        greffe: |_| Greffe {
            compteurs: vec![
                json!({ "id": "objet_bram", "scope": "caster", "max": 5, "default": 0, "modifies_damage": [finaux(2)] }),
                json!({ "id": "objet_bram_ce_tour", "scope": "caster", "max": 5, "default": 0, "shifts_into": "objet_bram" }),
            ],
            declencheur: Some(Declencheur::Retrait(&["objet_bram", "objet_bram_ce_tour"])),
            remarque: "Couronne de Brâm Barbe-Monde : +2 % de dommages finaux pour deux tours par retrait de PA ou de PM \
                       que vos sorts tentent sur la cible, 5 au plus"
                .into(),
            ..Default::default()
        },
    },
    // La Hachebarde de Guerre et le Frisson de Brumaire retirent à chaque fin
    // de tour des PM (grade 3 du sort 15739, effet 1080) ou des PA (grade 2 du
    // sort 12147, effet 1079) aux ennemis au contact : une tentative de plus
    // pour la Couronne, dont le cumul sert au tour suivant.
    Effet {
        objet: 22368,
        nom: "Hachebarde de Guerre",
        condition: Some(("un cumul de la Couronne de Brâm à chaque fin de tour", "si un ennemi est à votre contact en fin de tour")),
        sinon: None,
        greffe: |_| Greffe {
            retraits_de_fin_de_tour: 1,
            remarque: "Hachebarde de Guerre : son retrait de PM en fin de tour donne un cumul de la Couronne de Brâm \
                       Barbe-Monde pour le tour suivant"
                .into(),
            ..Default::default()
        },
    },
    Effet {
        objet: 20355,
        nom: "Frisson de Brumaire",
        condition: Some(("un cumul de la Couronne de Brâm à chaque fin de tour", "si un ennemi est à votre contact en fin de tour")),
        sinon: None,
        greffe: |_| Greffe {
            retraits_de_fin_de_tour: 1,
            remarque: "Frisson de Brumaire : son retrait de PA en fin de tour donne un cumul de la Couronne de Brâm \
                       Barbe-Monde pour le tour suivant"
                .into(),
            ..Default::default()
        },
    },
    // La Plume : +10 Dommages Poussée pour 3 tours par coup critique (grade 2
    // du sort 11357), sans plafond en 3.7. Chaque cumul tient ses trois tours
    // pour lui-même : ceux du tour glissent d'un compteur à l'autre et quittent
    // le total au bout du troisième.
    Effet {
        objet: 20356,
        nom: "Plume de Buhorado",
        condition: None,
        sinon: None,
        greffe: |_| Greffe {
            compteurs: vec![
                json!({
                    "id": "objet_buhorado", "scope": "caster", "max": 30, "default": 0,
                    "modifies_damage": [dommages_poussee(10)],
                }),
                json!({
                    "id": "objet_buhorado_ce_tour", "scope": "caster", "max": 10, "default": 0,
                    "shifts_into": "objet_buhorado_tour_1",
                }),
                json!({
                    "id": "objet_buhorado_tour_1", "scope": "caster", "max": 10, "default": 0,
                    "shifts_into": "objet_buhorado_tour_2",
                }),
                json!({
                    "id": "objet_buhorado_tour_2", "scope": "caster", "max": 10, "default": 0,
                    "drains": "objet_buhorado",
                }),
            ],
            declencheur: Some(Declencheur::Critique(&["objet_buhorado", "objet_buhorado_ce_tour"])),
            remarque: "Plume de Buhorado : +10 Dommages Poussée par coup critique, chacun pour trois tours, sans \
                       plafond ; un lancer critique ici à partir de 78 % de critique"
                .into(),
            ..Default::default()
        },
    },
    // Le Lance-Éclair de Menalt en 3.7 : « le porteur gagne 6 Dommages Poussée
    // pendant 2 tours pour chaque PM qu'il utilise », les PM que le joueur
    // déclare dépenser. Ce qu'il donne aux alliés ne change rien ici.
    Effet {
        objet: 32116,
        nom: "Lance-Éclair de Menalt",
        condition: None,
        sinon: None,
        greffe: |c| {
            if c.pm_depenses == 0 {
                return Greffe {
                    remarque: "Lance-Éclair de Menalt : déclarez vos PM dépensés pour compter ses Dommages Poussée".into(),
                    ..Default::default()
                };
            }
            let n = 6 * i32::from(c.pm_depenses);
            simple(
                json!({
                    "id": "objet_menalt", "scope": "caster", "max": 2, "default": 0, "monotone": "increasing",
                    "gain_per_turn": 1, "modifies_damage": [dommages_poussee(n)],
                }),
                format!("Lance-Éclair de Menalt : +{n} Dommages Poussée par tour de PM dépensés, 6 par PM, pour deux tours"),
            )
        },
    },
    // La Ponctualité d'Henual en 3.7 : 1 PA quand le porteur effectue ou subit
    // une téléportation ou un échange (1 fois par tour), et une téléportation à
    // sa position précédente au début de son tour s'il n'en a pas eu au tour
    // d'avant. Soit un PA à chaque tour dès le deuxième, et au premier si un de
    // ses sorts le téléporte ou l'échange.
    Effet {
        objet: 32119,
        nom: "Ponctualité d'Henual",
        condition: None,
        sinon: None,
        greffe: |_| Greffe {
            compteurs: vec![
                json!({
                    "id": "objet_henual", "scope": "caster", "max": 2, "default": 0, "monotone": "increasing",
                    "gain_per_turn": 1, "pa_par_valeur": [0, 1],
                }),
                json!({ "id": "objet_henual_premier_tour", "scope": "caster", "max": 1, "default": 0 }),
            ],
            declencheur: Some(Declencheur::Deplacement { compteur: "objet_henual_premier_tour", tour: "objet_henual" }),
            remarque: "Ponctualité d'Henual : +1 PA à chaque tour dès le deuxième, et au premier si un de vos sorts \
                       vous téléporte ou vous échange"
                .into(),
            ..Default::default()
        },
    },
    // Les Bottes repoussent de 2 cases les entités au contact au début et à la
    // fin du tour (grade 2 du sort 11367, effet 5). Les deux poussées se
    // comptent au début du tour ; la seconde consomme ce que les sorts du
    // porteur ont posé et qu'une poussée consomme.
    Effet {
        objet: 20364,
        nom: "Bottes du Cul Botté",
        condition: Some(("deux poussées de 2 cases par tour", "si un ennemi est à votre contact au début et à la fin du tour")),
        sinon: None,
        greffe: |c| {
            if !c.poussees_bloquees {
                return Greffe {
                    remarque: "Bottes du Cul Botté : leurs poussées ne font de dégâts que si vos poussées butent contre un obstacle"
                        .into(),
                    ..Default::default()
                };
            }
            Greffe {
                compteurs: vec![json!({
                    "id": "objet_cul_botte", "scope": "caster", "max": 1, "default": 1, "poussees_par_tour": [2, 2],
                })],
                pousse_a_chaque_tour: true,
                remarque: "Bottes du Cul Botté : deux poussées de 2 cases par tour sur l'ennemi au contact, contre un obstacle, \
                           comptées au début du tour"
                    .into(),
                ..Default::default()
            }
        },
    },
    // Le Crocobur s'inflige 15 dommages au début du tour (grade 2 du sort
    // 11352, effet 2822) et les vole aux entités au contact à la fin (grade
    // 3, effet 2828), sans critique. Compté au début du tour.
    Effet {
        objet: 20353,
        nom: "Crocobur",
        condition: Some(("vol de vie à chaque fin de tour", "si un ennemi est à votre contact en fin de tour")),
        sinon: None,
        greffe: |_| simple(
            json!({
                "id": "objet_crocobur", "scope": "caster", "max": 1, "default": 1,
                "while_present": [{
                    "trigger": "turn_start",
                    "lines": [{ "best_element": true, "normal": [15, 15], "critical": [15, 15], "sans_critique": true }],
                }],
            }),
            "Crocobur : 15 de vol de vie dans votre meilleur élément sur l'ennemi au contact à chaque fin de tour, \
             compté au début du tour",
        ),
    },
    // L'Audace : au début du tour, le porteur change de case (téléporté ou
    // échangé) et gagne 1 PM et 10 % de critique pour le tour ; s'il ne peut
    // pas bouger, 40 de Fuite et de Dommages Poussée (grade 3 du sort 11368,
    // selon l'état 913 que pose le déplacement).
    Effet {
        objet: 20365,
        nom: "Audace de Dodge",
        condition: Some(("+10 % de critique à chaque tour", "si vous changez de case au début du tour ; décochée, +40 Dommages Poussée")),
        sinon: Some(|_| simple(
            toujours("objet_dodge", vec![dommages_poussee(40)]),
            "Audace de Dodge : +40 Dommages Poussée à chaque tour, sans changer de case",
        )),
        greffe: |_| simple(
            toujours("objet_dodge", vec![critique(10)]),
            "Audace de Dodge : +10 % de critique à chaque tour, en changeant de case",
        ),
    },
];

/// Ce que des objets lisent sans que les dégâts en dépendent.
const SANS_EFFET: &[(u32, &str)] = &[
    (20363, "Bottes de Mille Lieues : +2 PM les tours impairs, sans effet sur les dégâts"),
    (18853, "Épée Langue de Feu : son effet spécial ne vise que des monstres de quête"),
    (30356, "Jyfus : son effet spécial ne change pas les dégâts"),
    (29133, "Ménologium béni : son effet spécial, Passage des saisons, ne change pas les dégâts"),
    (29135, "Dofus Verdoyant : son effet spécial ne change pas les dégâts"),
    (21506, "Peluche-boule : Phorreur : son effet spécial ne vise que certains monstres"),
    (21507, "Peluche-boule : Bouftou : son effet spécial ne vise que certains monstres"),
    (21508, "Peluche-boule : Tofu : son effet spécial ne vise que certains monstres"),
    (21509, "Peluche-boule : Chacha : son effet spécial ne vise que certains monstres"),
    (21510, "Peluche-boule : Minikrone : son effet spécial ne vise que certains monstres"),
];

/// L'effet spécial de cet objet est-il compté ici ?
pub fn modelise(objet: u32, portes: &[u32]) -> bool {
    EFFETS.iter().any(|e| e.objet == objet) && s_applique(objet, portes)
}

/// Ce que les remarques du calcul disent d'un objet sans effet sur les dégâts,
/// sans son nom en tête : « Son effet spécial ne vise que des monstres de
/// quête ». Rien quand elles ne disent que cela, que la fiche dit déjà.
pub fn sans_effet(objet: u32) -> Option<String> {
    let (_, remarque) = SANS_EFFET.iter().find(|(o, _)| *o == objet)?;
    let (_, suite) = remarque.split_once(" : ")?;
    if suite.ends_with("ne change pas les dégâts") {
        return None;
    }
    let mut c = suite.chars();
    c.next().map(|p| p.to_uppercase().chain(c).collect())
}

fn porte(build: &BuildInput, objet: u32) -> bool {
    build.items.contains(&objet)
}

/// Les cases de la Rotation pour les effets portés qui dépendent du combat.
pub fn bonus_conditionnels(build: &BuildInput) -> Vec<BonusConditionnel> {
    EFFETS
        .iter()
        .filter(|e| porte(build, e.objet) && s_applique(e.objet, &build.items))
        .filter_map(|e| {
            let (effet, condition) = e.condition?;
            Some(BonusConditionnel { nom: e.nom.to_string(), effet: effet.to_string(), condition, coche: true })
        })
        .collect()
}

/// Greffe aux règles les effets des objets portés, sauf ceux que le joueur a
/// écartés (ou leur variante de case décochée), et rend les remarques du
/// calcul. `lectures` : ce que la donnée des sorts de la classe dit de leurs
/// retraits sur l'ennemi et des déplacements de leur lanceur
/// ([`Lectures::depuis`]).
pub fn greffer(
    regles: &mut Ruleset,
    build: &BuildInput,
    ecartes: &[String],
    combat: &Combat,
    lectures: &Lectures,
) -> Greffes {
    let mut out = Greffes::default();
    let mut retraits_de_fin_de_tour = 0u8;
    for e in EFFETS.iter().filter(|e| porte(build, e.objet) && s_applique(e.objet, &build.items)) {
        let greffe = if ecartes.iter().any(|n| n == e.nom) {
            match e.sinon {
                Some(sinon) => sinon(combat),
                None => continue,
            }
        } else {
            (e.greffe)(combat)
        };
        if regles.resources.len() + greffe.compteurs.len() > dofus_engine::MAX_RESOURCES {
            out.remarques.push(format!(
                "{} n'a pas trouvé de place sous le plafond de compteurs du moteur : la rotation s'en passe",
                e.nom
            ));
            continue;
        }
        let compteurs: Result<Vec<dofus_ruleset::ResourceDef>, _> =
            greffe.compteurs.into_iter().map(serde_json::from_value).collect();
        match compteurs {
            Ok(c) => regles.resources.extend(c),
            Err(erreur) => {
                out.remarques.push(format!("{} : effet illisible ({erreur})", e.nom));
                continue;
            }
        }
        if let Some(d) = &greffe.declencheur {
            declencher(regles, d, lectures);
        }
        if greffe.pousse_a_chaque_tour {
            out.poussee_subie = true;
            pousser_le_cauchemar(regles);
        }
        retraits_de_fin_de_tour = retraits_de_fin_de_tour.saturating_add(greffe.retraits_de_fin_de_tour);
        out.remarques.push(greffe.remarque);
    }
    // Les retraits de fin de tour s'ajoutent au total de la Couronne APRÈS que
    // les cumuls du tour y ont glissé (`gain_at_turn_end`) : le tour suivant
    // les porte, puis ils tombent avec eux.
    if retraits_de_fin_de_tour > 0 {
        if let Some(c) = regles.resources.iter_mut().find(|r| r.id == "objet_bram") {
            c.gain_at_turn_end = Some(retraits_de_fin_de_tour);
        }
    }
    out.remarques.extend(SANS_EFFET.iter().filter(|(o, _)| porte(build, *o)).map(|(_, r)| r.to_string()));
    out
}

/// Faut-il lire la donnée des sorts de la classe : un effet porté répond-il
/// à leurs retraits ou à leurs déplacements ?
pub fn lit_la_donnee(build: &BuildInput) -> bool {
    EFFETS.iter().filter(|e| porte(build, e.objet)).any(|e| {
        matches!(
            (e.greffe)(&Combat { ennemis: 1, poussees_bloquees: true, pm_depenses: 0 }).declencheur,
            Some(Declencheur::Retrait(_) | Declencheur::Deplacement { .. })
        )
    })
}

/// Les retraits de PA ou de PM qu'un sort tente sur l'ennemi : ceux de sa
/// donnée, ou, pour l'arme, ceux de ses effets.
fn retraits_du_sort(sort: &SpellDef, retraits: &HashMap<u32, u8>) -> u8 {
    if sort.arme {
        let n = sort.tags.iter().filter(|t| *t == "removes_ap" || *t == "removes_mp").count();
        u8::try_from(n).unwrap_or(u8::MAX)
    } else {
        sort.dofusdb_id.and_then(|id| retraits.get(&id)).copied().unwrap_or(0)
    }
}

/// Donne aux sorts qui le déclenchent le gain d'un effet, modes compris.
fn declencher(regles: &mut Ruleset, d: &Declencheur, lectures: &Lectures) {
    let gain = |compteur: &str, n: u8, critique: bool| -> Option<dofus_ruleset::Effect> {
        serde_json::from_value(json!({ "effect": "gain", "resource": compteur, "amount": n, "on_critical": critique })).ok()
    };
    for sort in regles.spells.iter_mut() {
        let gains: Vec<dofus_ruleset::Effect> = match d {
            Declencheur::Retrait(compteurs) => {
                let n = retraits_du_sort(sort, &lectures.retraits);
                if n == 0 {
                    continue;
                }
                compteurs.iter().filter_map(|c| gain(c, n, false)).collect()
            }
            Declencheur::Critique(compteurs) => {
                if !sort.crit.can_crit {
                    continue;
                }
                compteurs.iter().filter_map(|c| gain(c, 1, true)).collect()
            }
            Declencheur::Deplacement { compteur, tour } => {
                if !sort.dofusdb_id.is_some_and(|id| lectures.deplacements.contains(&id)) {
                    continue;
                }
                serde_json::from_value(json!({
                    "effect": "gain", "resource": compteur, "ap_bonus": { "amount": 1 }, "at_cap": "skip",
                    "requires": { "kind": "exactly", "resource": tour, "amount": 1 },
                }))
                .ok()
                .into_iter()
                .collect()
            }
        };
        sort.effects.extend(gains.iter().cloned());
        for mode in &mut sort.modes {
            mode.effects.extend(gains.iter().cloned());
        }
    }
}

/// Une poussée au début de chaque tour : l'Éternel Cauchemar (+100 Puissance
/// sur les coups qui suivent une poussée) tient donc dès le premier coup, et
/// l'Œil du Cauchemar aussi quand le joueur a déclaré l'entrave, seul état
/// qu'il attende alors (`solve::avec_les_effets_de_dofus`). Déclarée, l'entrave
/// ne laisse pas de Bouclier Bontarien aux règles.
fn pousser_le_cauchemar(regles: &mut Ruleset) {
    let entrave_declaree =
        regles.resource("oeil_du_cauchemar").is_some() && regles.resource("bouclier_bontarien").is_none();
    for r in regles.resources.iter_mut() {
        if r.id == "eternel_cauchemar" || (entrave_declaree && r.id == "oeil_du_cauchemar") {
            r.gain_per_turn = Some(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solve::load_ruleset;

    fn porte_seul(objet: u32) -> BuildInput {
        let mut build = BuildInput { class: 8, level: 200, ..Default::default() };
        build.items = vec![0; 17];
        build.items[16] = objet;
        build
    }

    fn combat(ennemis: u8, poussees_bloquees: bool) -> Combat {
        Combat { ennemis, poussees_bloquees, pm_depenses: 0 }
    }

    /// Chaque effet se lit dans les règles, sur les deux nombres d'ennemis
    /// extrêmes, poussées bloquées ou non, case cochée ou non : un compteur mal
    /// écrit ne passerait pas `serde`.
    #[test]
    fn chaque_effet_se_greffe() {
        for e in EFFETS {
            for (n, bloquees) in [(1, true), (30, false)] {
                for ecartes in [vec![], vec![e.nom.to_string()]] {
                    let c = combat(n, bloquees);
                    let attendus = match (ecartes.is_empty(), e.sinon) {
                        (true, _) => (e.greffe)(&c).compteurs.len(),
                        (false, Some(sinon)) => sinon(&c).compteurs.len(),
                        (false, None) => 0,
                    };
                    let mut regles = load_ruleset(8).unwrap();
                    let avant = regles.resources.len();
                    let g = greffer(&mut regles, &porte_seul(e.objet), &ecartes, &c, &Lectures::default());
                    assert_eq!(regles.resources.len(), avant + attendus, "{} : {:?}", e.nom, g.remarques);
                    assert!(g.remarques.iter().all(|r| !r.contains("illisible")), "{:?}", g.remarques);
                }
            }
        }
    }

    /// Un effet écarté dans la Rotation ne se greffe pas.
    #[test]
    fn un_effet_ecarte_ne_compte_pas() {
        let mut regles = load_ruleset(8).unwrap();
        let avant = regles.resources.len();
        greffer(&mut regles, &porte_seul(20358), &["Trompe-la-Mort".to_string()], &combat(1, false), &Lectures::default());
        assert_eq!(regles.resources.len(), avant);
    }

    /// Un Crâ qui porte l'objet et lance la Flèche Évasive, `lancers` fois par
    /// tour, cinq tours : les dégâts et les PA restants de chaque tour.
    fn rejoue(objet: u32, lancers: usize, ecartes: &[&str]) -> Vec<(f64, i32)> {
        let mut items = vec![0; 17];
        items[16] = objet;
        let requete: crate::solve::Request = serde_json::from_value(json!({
            "class": 9, "level": 200, "items": items,
            "invested": { "chance": 300, "strength": 300, "intelligence": 300, "agility": 300 },
            "deck": ["fleche_evasive"], "horizon": 5, "bonus_ecartes": ecartes,
        }))
        .unwrap();
        let tours: Vec<Vec<String>> = (0..5).map(|_| vec!["fleche_evasive".to_string(); lancers]).collect();
        crate::solve::rejouer(&requete, &tours)
            .unwrap()
            .turns
            .iter()
            .map(|t| (t.damage.as_f64(), i32::from(t.ap_left)))
            .collect()
    }

    fn degats(tours: &[(f64, i32)]) -> Vec<f64> {
        tours.iter().map(|t| t.0).collect()
    }

    fn pa(tours: &[(f64, i32)]) -> Vec<i32> {
        tours.iter().map(|t| t.1).collect()
    }

    /// La Surpryz : 100 % de critique au tour 1, 35 au tour 2, 15 au tour 3,
    /// puis plus rien.
    #[test]
    fn la_surpryz_decroit_sur_trois_tours() {
        let d = degats(&rejoue(22001, 1, &[]));
        assert!(d[0] > d[1] && d[1] > d[2] && d[2] > d[3], "{d:?}");
        assert_eq!(d[3], d[4], "{d:?}");
    }

    /// La Dofusteuse : la Chance du tour 1 lève la Flèche Évasive, la Force,
    /// l'Agilité et l'Intelligence des trois tours suivants non, et le tour 5
    /// revient à la Chance.
    #[test]
    fn la_dofusteuse_tourne_sur_quatre() {
        let d = degats(&rejoue(958, 1, &[]));
        assert!(d[0] > d[1], "{d:?}");
        assert_eq!((d[1], d[2]), (d[3], d[3]), "{d:?}");
        assert_eq!(d[4], d[0], "{d:?}");
    }

    /// Le Diadème de Ganymède : -1 PA aux tours impairs, +2 aux pairs. Sans
    /// lancer, l'écart d'un tour à l'autre est de trois PA.
    #[test]
    fn le_diademe_alterne_les_pa() {
        let a = pa(&rejoue(20360, 0, &[]));
        assert_eq!((a[1] - a[0], a[2] - a[1], a[3] - a[2]), (3, -3, 3), "{a:?}");
    }

    /// La Pryssion Brillante : +2 PA et -35 % de dommages finaux aux deux
    /// premiers tours.
    #[test]
    fn la_pryssion_echange_des_finaux_contre_des_pa() {
        let a = pa(&rejoue(21997, 0, &[]));
        assert_eq!((a[0] - a[2], a[1] - a[2], a[3] - a[2]), (2, 2, 0), "{a:?}");
        let d = degats(&rejoue(21997, 1, &[]));
        assert!(d[0] < d[2] && d[1] < d[2], "{d:?}");
    }

    /// Les Ciseaux du Destin : +6 % de dommages subis dès le tour 2, au
    /// contact ; écartés, rien.
    #[test]
    fn les_ciseaux_comptent_des_le_deuxieme_tour() {
        let d = degats(&rejoue(20354, 1, &[]));
        assert!((d[1] / d[0] - 1.06).abs() < 0.01, "{d:?}");
        assert_eq!(d[1], d[2], "{d:?}");
        let sans = degats(&rejoue(20354, 1, &["Ciseaux du Destin"]));
        assert_eq!(sans[0], sans[1], "{sans:?}");
    }

    /// Un Crâ qui porte l'objet rejoue ces lancers à chaque tour, cinq tours,
    /// poussées bloquées ou non.
    fn rejoue_en_detail(objet: u32, tour: &[&str], ecartes: &[&str], poussees_bloquees: bool) -> dofus_engine::Solution {
        let mut items = vec![0; 17];
        items[16] = objet;
        let mut deck = tour.to_vec();
        deck.sort_unstable();
        deck.dedup();
        let requete: crate::solve::Request = serde_json::from_value(json!({
            "class": 9, "level": 200, "items": items,
            "invested": { "chance": 300, "strength": 300, "intelligence": 300, "agility": 300 },
            "deck": deck, "horizon": 5, "bonus_ecartes": ecartes, "poussees_bloquees": poussees_bloquees,
        }))
        .unwrap();
        let tours: Vec<Vec<String>> = (0..5).map(|_| tour.iter().map(|s| s.to_string()).collect()).collect();
        crate::solve::rejouer(&requete, &tours).unwrap()
    }

    fn lancers(s: &dofus_engine::Solution, tour: usize) -> Vec<f64> {
        s.turns[tour].casts.iter().map(|c| c.damage.as_f64()).collect()
    }

    fn ouvertures(s: &dofus_engine::Solution) -> Vec<f64> {
        s.turns.iter().map(|t| t.opening_damage.as_f64()).collect()
    }

    /// La Couronne de Brâm Barbe-Monde : la Flèche Cinglante tente de retirer
    /// des PM, un cumul de +2 % par lancer, qui tient aussi le tour suivant et
    /// tombe ensuite.
    #[test]
    fn la_couronne_cumule_les_retraits() {
        let s = rejoue_en_detail(20359, &["fleche_cinglante", "fleche_cinglante"], &[], false);
        let (t1, t2, t3) = (lancers(&s, 0), lancers(&s, 1), lancers(&s, 2));
        let x = t1[0];
        assert!((t1[1] / x - 1.02).abs() < 0.005, "{t1:?}");
        assert!((t2[0] / x - 1.04).abs() < 0.005 && (t2[1] / x - 1.06).abs() < 0.005, "{t2:?}");
        assert_eq!(t3, t2);
    }

    /// Un Crâ qui porte ces objets, chacun à sa place, et lance la Flèche
    /// Perforante à chaque tour, un sort qui ne retire rien et frappe toujours
    /// pareil : les dégâts de chaque tour.
    fn perforantes(objets: &[(usize, u32)], ecartes: &[&str]) -> Vec<f64> {
        let mut items = vec![0; 17];
        for (place, objet) in objets {
            items[*place] = *objet;
        }
        let requete: crate::solve::Request = serde_json::from_value(json!({
            "class": 9, "level": 200, "items": items,
            "invested": { "chance": 300, "strength": 300, "intelligence": 300, "agility": 300 },
            "deck": ["fleche_perforante"], "horizon": 4, "bonus_ecartes": ecartes,
        }))
        .unwrap();
        let tours: Vec<Vec<String>> = (0..4).map(|_| vec!["fleche_perforante".to_string()]).collect();
        crate::solve::rejouer(&requete, &tours).unwrap().turns.iter().map(|t| t.damage.as_f64()).collect()
    }

    /// La Hachebarde de Guerre retire des PM en fin de tour : avec la
    /// Couronne de Brâm, un cumul de +2 % pour le tour suivant, à chaque tour
    /// sauf le premier, même sans sort qui retire. Case décochée, ou sans la
    /// Couronne, rien.
    #[test]
    fn la_couronne_compte_les_retraits_de_fin_de_tour() {
        let avec = perforantes(&[(0, 20359), (8, 22368)], &[]);
        assert!((avec[1] / avec[0] - 1.02).abs() < 0.005, "{avec:?}");
        assert_eq!((avec[2], avec[3]), (avec[1], avec[1]), "{avec:?}");
        let ecartee = perforantes(&[(0, 20359), (8, 22368)], &["Hachebarde de Guerre"]);
        assert!(ecartee.iter().all(|d| *d == ecartee[0]), "{ecartee:?}");
        let seule = perforantes(&[(8, 22368)], &[]);
        assert!(seule.iter().all(|d| *d == seule[0]), "{seule:?}");
        let mut build = porte_seul(22368);
        assert!(bonus_conditionnels(&build).is_empty());
        build.items[0] = 20359;
        assert_eq!(bonus_conditionnels(&build).len(), 1);
    }

    /// La Ponctualité d'Henual : un Iop qui saute (Bond, 4 PA) au premier tour
    /// y gagne 1 PA ; ensuite 1 PA à chaque tour, qu'il saute ou non, une
    /// fois.
    #[test]
    fn henual_donne_un_pa_par_tour() {
        let pa = |tours: &[&[&str]]| -> Vec<i32> {
            let mut items = vec![0; 17];
            items[16] = 32119;
            let requete: crate::solve::Request = serde_json::from_value(json!({
                "class": 8, "level": 200, "items": items,
                "invested": { "strength": 300 }, "deck": ["bond"], "horizon": 3,
            }))
            .unwrap();
            let t: Vec<Vec<String>> = tours.iter().map(|l| l.iter().map(|s| s.to_string()).collect()).collect();
            crate::solve::rejouer(&requete, &t).unwrap().turns.iter().map(|t| i32::from(t.ap_left)).collect()
        };
        let vide = pa(&[&[], &[], &[]]);
        assert_eq!((vide[1] - vide[0], vide[2] - vide[1]), (1, 0), "{vide:?}");
        let saut = pa(&[&["bond"], &[], &[]]);
        assert_eq!(vide[0] - saut[0], 3, "Bond coûte 4 et rend 1 : {saut:?}");
        let deux = pa(&[&["bond"], &["bond"], &[]]);
        assert_eq!(vide[1] - deux[1], 4, "au deuxième tour, rien de plus : {deux:?}");
    }

    /// Le Lance-Éclair de Menalt : 6 Dommages Poussée par PM dépensé, pour
    /// deux tours. Deux PM par tour et une Flèche de Recul qui bute : sa
    /// poussée de 2 cases prend 12 × 2 / 4 = 6 de plus au tour 1, puis 12.
    #[test]
    fn menalt_pousse_avec_les_pm_depenses() {
        let recul = |pm: u8| -> Vec<f64> {
            let mut items = vec![0; 17];
            items[8] = 32116;
            let requete: crate::solve::Request = serde_json::from_value(json!({
                "class": 9, "level": 200, "items": items,
                "invested": { "chance": 300, "strength": 300, "intelligence": 300, "agility": 300 },
                "deck": ["fleche_de_recul"], "horizon": 3, "poussees_bloquees": true, "pm_depenses": pm,
            }))
            .unwrap();
            let tours: Vec<Vec<String>> = (0..3).map(|_| vec!["fleche_de_recul".to_string()]).collect();
            crate::solve::rejouer(&requete, &tours).unwrap().turns.iter().map(|t| t.damage.as_f64()).collect()
        };
        let (sans, avec) = (recul(0), recul(2));
        let ecarts: Vec<f64> = avec.iter().zip(&sans).map(|(a, s)| a - s).collect();
        assert_eq!(ecarts, [6.0, 12.0, 12.0], "{sans:?} {avec:?}");
    }

    /// La Plume de Buhorado : un gain de coup critique sur chaque sort qui
    /// critique, et sur eux seuls.
    #[test]
    fn la_plume_repond_aux_coups_critiques() {
        let mut regles = load_ruleset(9).unwrap();
        greffer(&mut regles, &porte_seul(20356), &[], &combat(1, true), &Lectures::default());
        for s in &regles.spells {
            let gagne = s.effects.iter().any(|e| {
                matches!(e, dofus_ruleset::Effect::Gain { resource, on_critical: true, .. } if resource == "objet_buhorado")
            });
            assert_eq!(gagne, s.crit.can_crit, "{}", s.id);
        }
    }

    /// Les Bottes du Cul Botté : deux poussées de 2 cases au début de chaque
    /// tour contre un obstacle, `(100 + 32 + 40) × 2 / 4` chacune avec les 40
    /// Dommages Poussée des Bottes. Sans poussées bloquées, ou écartées, rien.
    #[test]
    fn les_bottes_poussent_deux_fois_par_tour() {
        let s = rejoue_en_detail(20364, &["fleche_cinglante"], &[], true);
        assert_eq!(ouvertures(&s), [172.0; 5]);
        let sans = rejoue_en_detail(20364, &["fleche_cinglante"], &[], false);
        assert_eq!(ouvertures(&sans), [0.0; 5]);
        let ecartees = rejoue_en_detail(20364, &["fleche_cinglante"], &["Bottes du Cul Botté"], true);
        assert_eq!(ouvertures(&ecartees), [0.0; 5]);
    }

    /// Le Crocobur vole à chaque tour l'ennemi au contact ; écarté, rien.
    #[test]
    fn le_crocobur_vole_a_chaque_tour() {
        let s = rejoue_en_detail(20353, &["fleche_cinglante"], &[], false);
        assert!(ouvertures(&s).iter().all(|d| *d > 0.0), "{:?}", ouvertures(&s));
        let sans = rejoue_en_detail(20353, &["fleche_cinglante"], &["Crocobur"], false);
        assert_eq!(ouvertures(&sans), [0.0; 5]);
    }

    /// L'Audace de Dodge relève le critique à chaque tour ; décochée, ses
    /// Dommages Poussée ne changent rien sans poussée bloquée.
    #[test]
    fn l_audace_releve_le_critique() {
        let avec = degats(&rejoue(20365, 1, &[]));
        let sans = degats(&rejoue(20365, 1, &["Audace de Dodge"]));
        assert!(avec.iter().zip(&sans).all(|(a, s)| a > s), "{avec:?} {sans:?}");
    }

    /// Chaque objet que les remarques nomment existe au catalogue sous ce nom.
    #[test]
    fn les_objets_sans_effet_sont_au_catalogue() {
        let catalogue = dofus_build::Catalogue::from_json(crate::donnees::OBJETS).unwrap();
        for (objet, remarque) in SANS_EFFET {
            let nom = catalogue.item(*objet).map(|o| o.name.as_str()).unwrap_or("?");
            assert!(remarque.starts_with(&format!("{nom} : ")), "{objet} {nom} : {remarque}");
        }
    }

    /// Chaque objet nommé ici existe au catalogue sous ce nom.
    #[test]
    fn les_objets_sont_au_catalogue() {
        let catalogue = dofus_build::Catalogue::from_json(crate::donnees::OBJETS).unwrap();
        for e in EFFETS {
            assert_eq!(catalogue.item(e.objet).map(|o| o.name.as_str()), Some(e.nom), "{}", e.objet);
        }
    }
}
