//! La géométrie énumère autant de cases que la formule de l'import en compte :
//! deux dérivations indépendantes du même nombre.

use dofus_grid::*;

/// La formule de `zone_cells`, recopiée et non appelée : les deux chemins
/// doivent rester écrits séparément.
fn formule(forme: char, r: u16, p2: u16) -> Option<u16> {
    Some(match forme {
        'P' => 1,
        'C' => 1 + 2 * r * (r + 1),
        'X' | '+' => 1 + 4 * r,
        'O' => (4 * r).max(1),
        '*' => 8 * r + 1,
        'Q' => 4 * (r.saturating_sub(p2.max(1)) + 1) + u16::from(p2 == 0),
        'G' => (2 * r + 1) * (2 * r + 1),
        'W' => {
            if r == 0 {
                1
            } else {
                (2 * r + 1) * (2 * r + 1) - 4
            }
        }
        'T' => 1 + 2 * r,
        'L' | '-' => r.max(1),
        'U' => 2 * r + 1,
        'F' => 3 * (r + 1) + 1,
        'V' => (r + 1) * (r + 1),
        _ => return None,
    })
}

const IMPACT: Case = Case::new(0, 0);
const VERS_LA_DROITE: (i16, i16) = (1, 0);

#[test]
fn chaque_forme_dessine_autant_de_cases_qu_elle_en_compte() {
    let mut vues = 0;
    for forme in [
        'P', 'C', 'X', '+', 'O', '*', 'G', 'W', 'T', 'L', '-', 'U', 'V', 'F',
    ] {
        for r in 0..=4u16 {
            let Some(attendu) = formule(forme, r, 0) else {
                continue;
            };
            let cases = cases_de_zone(forme, r, 0, IMPACT, VERS_LA_DROITE).expect("forme connue");
            vues += 1;
            assert_eq!(
                cases.len(),
                usize::from(attendu),
                "forme `{forme}` de taille {r} : la formule en compte {attendu} et \
                 le dessin en pose {}",
                cases.len()
            );
        }
    }
    // La croix en diagonale se lit avec ses deux paramètres.
    for (r, p2) in [(1u16, 1u16), (3, 1), (1, 0), (2, 0), (2, 1)] {
        let attendu = formule('Q', r, p2).unwrap();
        let cases = cases_de_zone('Q', r, p2, IMPACT, VERS_LA_DROITE).unwrap();
        vues += 1;
        assert_eq!(
            cases.len(),
            usize::from(attendu),
            "croix en diagonale {r}/{p2} : {attendu} contre {}",
            cases.len()
        );
    }
    assert!(vues >= 50, "ce test n'a regardé que {vues} cas");
}

/// Les nombres de cases mesurés en jeu, forme par forme.
#[test]
fn les_formes_mesurees_en_jeu_tombent_juste() {
    for (forme, r, p2, attendu, sort) in [
        ('V', 2u16, 0u16, 9usize, "Estoc Brûlant"),
        ('*', 2, 0, 17, "Effondrement"),
        ('U', 1, 0, 3, "Volée d'Airain"),
        ('U', 2, 0, 5, "Dagues Boomerang"),
        ('F', 1, 0, 7, "Trident de la Mer"),
        ('F', 2, 0, 10, "Carreaux Destructeurs"),
        ('Q', 1, 1, 4, "Souffle"),
        ('Q', 3, 1, 12, "Afflux"),
    ] {
        let cases = cases_de_zone(forme, r, p2, IMPACT, VERS_LA_DROITE).unwrap();
        assert_eq!(
            cases.len(),
            attendu,
            "{sort} : {attendu} cases comptées en jeu, {} dessinées",
            cases.len()
        );
    }
}

/// Une zone orientée tourne avec le tir sans changer de nombre de cases ; sinon,
/// le dessin déborde dans certains sens.
#[test]
fn une_zone_orientee_garde_sa_taille_dans_les_quatre_sens() {
    for forme in ['V', 'U', 'T', 'L', 'F'] {
        for r in 1..=3u16 {
            let tailles: Vec<usize> = DIRECTIONS
                .iter()
                .map(|d| cases_de_zone(forme, r, 0, IMPACT, *d).unwrap().len())
                .collect();
            assert!(
                tailles.windows(2).all(|p| p[0] == p[1]),
                "forme `{forme}` de taille {r} : {tailles:?} selon la direction"
            );
        }
    }
}

/// Aucune case n'est comptée deux fois.
#[test]
fn aucune_case_en_double() {
    for forme in ['C', 'X', '*', 'G', 'W', 'T', 'U', 'V', 'Q', 'O', 'F'] {
        for r in 1..=4u16 {
            let Some(cases) = cases_de_zone(forme, r, 0, IMPACT, VERS_LA_DROITE) else {
                continue;
            };
            let mut triees = cases.clone();
            triees.sort_unstable();
            triees.dedup();
            assert_eq!(
                cases.len(),
                triees.len(),
                "forme `{forme}` de taille {r} pose deux fois la même case"
            );
        }
    }
}

/// `X` et `+` ont le même nombre de cases mais pas la même forme : l'écran
/// tourne le repère de 45 degrés, et une croix sur les directions du jeu apparaît
/// en X.
#[test]
fn la_croix_droite_et_la_croix_diagonale_ne_couvrent_pas_les_memes_cases() {
    let o = Case::new(0, 0);
    for taille in 1..=3u16 {
        let x = cases_de_zone('X', taille, 0, o, (1, 0)).unwrap();
        let plus = cases_de_zone('+', taille, 0, o, (1, 0)).unwrap();
        assert_eq!(x.len(), plus.len(), "même nombre de cases, taille {taille}");
        assert_eq!(x.len(), 4 * taille as usize + 1);
        assert_ne!(x, plus, "et pourtant pas les mêmes cases, taille {taille}");
        // Le `X` suit les axes du repère, le `+` les diagonales.
        assert!(x.contains(&Case::new(taille as i16, 0)));
        assert!(!plus.contains(&Case::new(taille as i16, 0)));
        assert!(plus.contains(&Case::new(taille as i16, taille as i16)));
        assert!(!x.contains(&Case::new(taille as i16, taille as i16)));
        // Les deux gardent le centre.
        assert!(x.contains(&o) && plus.contains(&o));
    }
}
