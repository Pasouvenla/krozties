//! Le meilleur élément d'un build : la caractéristique d'abord, puis, à
//! égalité, les dommages fixes de l'élément. Il décide dans quel élément
//! frappent les dix-neuf sorts « du meilleur élément » et le vol de vie de la
//! Concentration de Chakra.

use dofus_damage::{DamageProfile, Element, ElementStats};
use dofus_engine::meilleur_element;

fn profil(reglages: &[(Element, i32, i32)]) -> DamageProfile {
    let mut p = DamageProfile::default();
    for (e, carac, fixes) in reglages {
        p.elements[e.index()] = ElementStats {
            characteristic: *carac,
            flat_damage: *fixes,
        };
    }
    p
}

#[test]
fn la_caracteristique_la_plus_haute_l_emporte() {
    let p = profil(&[(Element::Earth, 300, 50), (Element::Fire, 400, 0)]);
    assert_eq!(meilleur_element(&p), Element::Fire);
}

/// ⚠️ À ÉGALITÉ, LES DOMMAGES FIXES DÉPARTAGENT. Force et Agilité à 300, vingt
/// dommages Air : le build frappe en Air. L'ancienne règle prenait le premier
/// élément de l'ordre du moteur, sans regarder les dommages fixes.
#[test]
fn a_egalite_les_dommages_fixes_departagent() {
    let p = profil(&[
        (Element::Earth, 300, 0),
        (Element::Neutral, 300, 0),
        (Element::Air, 300, 20),
    ]);
    assert_eq!(meilleur_element(&p), Element::Air);
}

/// Et à égalité parfaite, la Terre, première de l'ordre.
#[test]
fn a_egalite_parfaite_la_terre_passe_devant() {
    let p = profil(&[]);
    assert_eq!(meilleur_element(&p), Element::Earth);
}
