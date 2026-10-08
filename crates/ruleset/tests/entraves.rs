//! Les sorts qui entravent leur propre lanceur, lus dans la donnée : un retrait
//! de PA, de PM ou de Portée sur le lanceur, sans délai. L'Œil du Cauchemar veut
//! le porteur « désenvoûté ou entravé » et poussant dans le même tour.

use dofus_ruleset::snapshot::Snapshot;

fn entravants(breed: u32) -> Vec<u32> {
    Snapshot::load(format!("{}/../../data/snapshots/breed-{breed}.json", env!("CARGO_MANIFEST_DIR")))
        .unwrap()
        .sorts_qui_entravent_le_lanceur()
}

/// Deux sorts dans tout le jeu : Sentinelle du Crâ (−1 Portée) et Duel de l'Iop
/// (−100 PM au lanceur comme à sa cible). La Retraite Anticipée de l'Enutrof et
/// Tirs Puissants du Crâ (3.7) retirent au tour suivant et ne comptent pas.
#[test]
fn deux_sorts_entravent_leur_lanceur() {
    let mut tous = Vec::new();
    for breed in (1..=18).chain([20]) {
        for sort in entravants(breed) {
            tous.push((breed, sort));
        }
    }
    assert_eq!(tous, [(8, 13109), (9, 32475)]);
}
