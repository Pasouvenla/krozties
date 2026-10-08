//! Le calcul confronté à une implémentation indépendante. La concordance
//! prouve que la caractéristique et la Puissance s'additionnent puis multiplient
//! le jet, et que les dommages fixes s'ajoutent après : l'ordre inverse donnerait
//! 621 au lieu de 173 sur le premier cas.

use dofus_damage::*;

/// Le build des mesures du parc : 630 dans la caractéristique, 170 de
/// Puissance, 56 de dommages fixes. Le multiplicateur vaut donc NEUF.
fn build() -> DamageProfile {
    DamageProfile {
        power: 170,
        flat_crit_damage: 0,
        elements: [ElementStats {
            characteristic: 630,
            flat_damage: 56,
        }; 5],
        ..Default::default()
    }
}

/// `(base, dégâts)` en jet normal, pour Force 630, Puissance 170, Dommages terre
/// 56 et un sort Terre de 13-16 (17-20 en critique) : 173 à 200 en normal, 209 à
/// 236 en critique.
const RELEVE: &[(i32, i64)] = &[(13, 173), (16, 200), (17, 209), (20, 236)];

#[test]
fn les_quatre_bornes_tombent_sur_celles_du_releve() {
    let profile = build();
    let ligne = SpellLine {
        element: Element::Earth,
        normal: (13, 16),
        critical: (17, 20),
    };
    let (lo, hi) = range(
        &ligne,
        &profile,
        FinalMultiplier::NEUTRAL,
        false,
        &Resistance::NONE,
    );
    assert_eq!((lo, hi), (173, 200), "jet normal 13-16");
    let (clo, chi) = range(
        &ligne,
        &profile,
        FinalMultiplier::NEUTRAL,
        true,
        &Resistance::NONE,
    );
    assert_eq!((clo, chi), (209, 236), "jet critique 17-20");
}

/// Et la même chose base par base, pour que l'échec dise LAQUELLE diverge.
#[test]
fn chaque_borne_prise_a_part() {
    let profile = build();
    for (base, attendu) in RELEVE {
        let ligne = SpellLine {
            element: Element::Earth,
            normal: (*base, *base),
            critical: (*base, *base),
        };
        let (obtenu, _) = range(
            &ligne,
            &profile,
            FinalMultiplier::NEUTRAL,
            false,
            &Resistance::NONE,
        );
        assert_eq!(
            obtenu, *attendu,
            "un jet de {base} doit rendre {attendu}, le relevé le dit"
        );
    }
}

/// L'ordre des deux étages : les dommages fixes s'ajoutent après la
/// multiplication, 13 × 9 + 56 = 173, quand (13 + 56) × 9 vaudrait 621.
#[test]
fn les_dommages_fixes_s_ajoutent_apres_la_multiplication() {
    let profile = build();
    let ligne = SpellLine {
        element: Element::Earth,
        normal: (13, 13),
        critical: (13, 13),
    };
    let (obtenu, _) = range(
        &ligne,
        &profile,
        FinalMultiplier::NEUTRAL,
        false,
        &Resistance::NONE,
    );
    assert_eq!(obtenu, 13 * 9 + 56);
    assert_ne!(obtenu, (13 + 56) * 9);
}

// ---------------------------------------------------------------------------
// La géométrie des zones, confrontée au simulateur de pièges
// ---------------------------------------------------------------------------

/// Le nombre de cases d'une zone, confronté à une seconde implémentation : un
/// cercle de taille N fait `2N(N+1)+1` cases (13 pour 2, 41 pour 4), un carré de
/// taille 2 en fait 25. Le cône, le demi-cercle, la fourche et la ligne depuis le
/// lanceur restent hors de ce contrôle.
#[test]
fn la_geometrie_des_zones_tombe_sur_celle_du_simulateur() {
    let cercle = |n: i32| 2 * n * (n + 1) + 1;
    let carre = |n: i32| (2 * n + 1) * (2 * n + 1);
    let croix = |n: i32| 4 * n + 1;
    let anneau = |n: i32| 4 * n;

    // Relevé dans `data/snapshots/`, forme par forme et taille par taille.
    assert_eq!(cercle(2), 13, "cercle de taille 2");
    assert_eq!(cercle(3), 25, "cercle de taille 3");
    assert_eq!(cercle(4), 41, "cercle de taille 4");
    assert_eq!(
        cercle(63),
        8065,
        "le cercle géant d'un sort sans vraie zone"
    );
    assert_eq!(carre(1), 9, "carré de taille 1");
    assert_eq!(carre(2), 25, "carré de taille 2");
    assert_eq!(croix(1), 5, "croix de taille 1");
    assert_eq!(croix(2), 9, "croix de taille 2");
    assert_eq!(croix(3), 13, "croix de taille 3");
    assert_eq!(anneau(2), 8, "anneau de taille 2");
    assert_eq!(anneau(3), 12, "anneau de taille 3");

    // Et la croix de taille 1 se confond avec le cercle de taille 1, ce que les
    // deux sources disent aussi : cinq cases.
    assert_eq!(croix(1), cercle(1));
}
