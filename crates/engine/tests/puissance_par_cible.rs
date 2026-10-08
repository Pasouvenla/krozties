//! Un bonus d'état peut valoir par ennemi touché, pas une fois pour toutes.
//!
//! La Terre du Milieu du Forgelance donne cinquante points de Puissance par
//! ennemi touché, et le bonus décide de l'ordre des sorts. Le nombre de cibles
//! est celui du scénario : ce calculateur croit sur parole que les ennemis
//! annoncés sont dans la zone du sort.
//!
//! Le plafond vient de `maxStack` ; sans lui, huit ennemis rendraient quatre
//! cents points que le jeu ne donne pas. Le `cu` du toolkit vaut 4 aussi, mais
//! ne sert pas de preuve : il se contredit ailleurs (la Flèche Punitive affiche
//! « Cumul : 1 » en montrant deux paliers).

use dofus_damage::{DamageProfile, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, DamageModifier, Maybe, Ruleset};

fn forgelance() -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/forgelance.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    let snap = Snapshot::load(format!("{root}/data/snapshots/breed-20.json")).unwrap();
    rs.merge_snapshot(&snap);
    rs
}

/// Les dégâts d'un lancer de Terre du Milieu, seul, sur `cibles` ennemis.
fn frappe(rs: &Ruleset, cibles: u8) -> f64 {
    let build = Build {
        name: "Forgelance".into(),
        profile: DamageProfile {
            power: 170,
            flat_crit_damage: 0,
            elements: [ElementStats {
                characteristic: 630,
                flat_damage: 56,
            }; 5],
            ..Default::default()
        },
        base_ap: 12,
        base_mp: 3,
        crit_bonus_percent: 0,
        modifiers: vec![],
        // Lance-pierre plante la Lance, que la Terre du Milieu exige à distance.
        deck: vec!["lance_pierre".into(), "terre_du_milieu".into()],
    };
    let sc = Scenario {
        poussees_bloquees: false,
        targets: cibles,
        horizon: 4,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(rs, build, sc)
        .unwrap_or_else(|e| panic!("{e}"))
        .solve()
        .total
        .as_f64()
}

/// Le bonus monte AVEC les cibles, et pas seulement parce que les dégâts montent.
///
/// Comparer les totaux bruts ne prouverait rien : frapper quatre ennemis rend
/// déjà quatre fois plus, `per_target` ou pas. On compare donc le MÊME scénario
/// à quatre cibles, une fois avec le drapeau et une fois sans.
#[test]
fn la_puissance_suit_le_nombre_de_cibles() {
    let avec = frappe(&forgelance(), 4);

    // Contrôle du contrôle : on remet le défaut, un montant fixe.
    let mut sans_drapeau = forgelance();
    for r in sans_drapeau.resources.iter_mut() {
        if r.id == "terre_du_milieu_puissance" {
            for m in r.modifies_damage.iter_mut() {
                if let DamageModifier::Characteristic { per_target, .. } = m {
                    *per_target = false;
                }
            }
        }
    }
    let sans = frappe(&sans_drapeau, 4);

    assert!(
        avec > sans,
        "quatre cibles doivent valoir quatre lots de Puissance, {sans:.0} contre {avec:.0}"
    );
    // Cent cinquante points de Puissance de plus sur un build à 630 et 170 :
    // le facteur passe de 9,0 à 10,5, soit un sixième de plus sur les lignes
    // que l'état lève. L'écart TOTAL est plus petit, tous les sorts n'en
    // profitant pas, mais il se compte en points et non en décimales.
    let ecart = (avec - sans) / sans;
    assert!(
        (0.03..0.25).contains(&ecart),
        "trois lots de Puissance de plus valent quelques points, mesuré {:.1} %",
        ecart * 100.0
    );
}

/// En duel, le drapeau ne change rien : un lot vaut un lot.
#[test]
fn en_duel_le_drapeau_ne_change_rien() {
    let avec = frappe(&forgelance(), 1);
    let mut sans_drapeau = forgelance();
    for r in sans_drapeau.resources.iter_mut() {
        if r.id == "terre_du_milieu_puissance" {
            for m in r.modifies_damage.iter_mut() {
                if let DamageModifier::Characteristic { per_target, .. } = m {
                    *per_target = false;
                }
            }
        }
    }
    assert_eq!(
        avec,
        frappe(&sans_drapeau, 1),
        "sur une cible, cinquante fois un font cinquante"
    );
}

/// Le fichier porte bien cinquante, et c'est la donnée du jeu.
#[test]
fn le_montant_vient_du_fichier() {
    let rs = forgelance();
    let r = rs
        .resources
        .iter()
        .find(|r| r.id == "terre_du_milieu_puissance")
        .expect("l'état est au fichier");
    let m = r.modifies_damage.first().expect("il porte un modificateur");
    match m {
        DamageModifier::Characteristic {
            amount,
            per_target,
            element,
            ..
        } => {
            assert_eq!(amount.known().copied(), Some(50), "cinquante par ennemi");
            assert!(*per_target, "et c'est bien par ennemi");
            assert!(
                element.is_none(),
                "la Puissance ne connaît pas d'élément, elle lève les cinq lignes"
            );
        }
        autre => panic!("attendu une caractéristique, trouvé {autre:?}"),
    }
    let _ = Maybe::<i32>::Unknown;
}

/// Au-delà du Cumul, le bonus ne monte plus : huit ennemis ne valent pas huit
/// lots mais quatre.
#[test]
fn le_cumul_plafonne_le_bonus() {
    let rs = forgelance();
    let quatre = frappe(&rs, 4);
    let huit = frappe(&rs, 8);

    // Huit ennemis rendent plus que quatre, les dégâts de zone doublant.
    assert!(huit > quatre, "{quatre:.0} contre {huit:.0}");

    // Mais la PUISSANCE, elle, ne bouge plus. On le vérifie en comparant huit
    // cibles avec et sans plafond : les deux doivent rendre le même total.
    let mut sans_plafond = forgelance();
    for r in sans_plafond.resources.iter_mut() {
        if r.id == "terre_du_milieu_puissance" {
            for m in r.modifies_damage.iter_mut() {
                if let DamageModifier::Characteristic { per_target_cap, .. } = m {
                    *per_target_cap = Some(8);
                }
            }
        }
    }
    assert!(
        frappe(&sans_plafond, 8) > huit,
        "sans le Cumul, huit ennemis donneraient huit lots : le plafond doit mordre"
    );
}

/// Le plafond écrit est bien celui du jeu.
#[test]
fn le_plafond_vient_du_jeu() {
    let rs = forgelance();
    let r = rs
        .resources
        .iter()
        .find(|r| r.id == "terre_du_milieu_puissance")
        .unwrap();
    match r.modifies_damage.first().unwrap() {
        DamageModifier::Characteristic { per_target_cap, .. } => {
            assert_eq!(
                *per_target_cap,
                Some(4),
                "DofusDB donne `maxStack` à 4 pour ce sort"
            );
        }
        autre => panic!("attendu une caractéristique, trouvé {autre:?}"),
    }
}

/// Le bonus vaut exactement `50 x min(cibles, 4)`, cible par cible. La preuve
/// est une équivalence et non une tendance : pour chaque nombre de cibles, le
/// calcul `per_target` égale un montant écrit à la main, au centième près.
#[test]
fn le_bonus_suit_exactement_le_nombre_de_cibles_saisi() {
    for cibles in 1u8..=6 {
        let attendu = 50 * i32::from(cibles.min(4));

        // Le même état, mais avec le montant posé en dur pour ce nombre de
        // cibles et le drapeau retiré.
        let mut fixe = forgelance();
        for r in fixe.resources.iter_mut() {
            if r.id == "terre_du_milieu_puissance" {
                for m in r.modifies_damage.iter_mut() {
                    if let DamageModifier::Characteristic {
                        amount, per_target, ..
                    } = m
                    {
                        *amount = Maybe::Known(attendu);
                        *per_target = false;
                    }
                }
            }
        }

        let calcule = frappe(&forgelance(), cibles);
        let ecrit = frappe(&fixe, cibles);
        assert!(
            (calcule - ecrit).abs() < 0.01,
            "sur {cibles} cible(s) le bonus doit valoir {attendu} de Puissance : \
             calculé {calcule:.2}, écrit à la main {ecrit:.2}"
        );
    }
}
