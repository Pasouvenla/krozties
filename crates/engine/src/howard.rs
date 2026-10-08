//! Le cycle de meilleure moyenne par itération sur les politiques (Howard), en
//! arithmétique exacte.
//!
//! Karp tient une valeur par état et par longueur de chemin : une table en `n²`
//! et un coût en `n × arêtes`. Howard n'en tient qu'une par état.
//!
//! Tout se compare en fractions exactes ; un choix ne change que sur une
//! amélioration stricte, le choix courant est gardé à égalité, et la poignée
//! d'un cycle est toujours son nœud de plus petit indice, pour que les biais
//! d'un cycle qui dure ne bougent pas d'une itération à l'autre. Au bout d'un
//! nombre fixe d'itérations, l'appelant renonce à la boucle plutôt que de rendre
//! un cycle que rien ne garantit.
//!
//! Quand l'itération s'arrête d'elle-même, aucune arête n'améliore plus rien,
//! et cela prouve l'optimum : la moyenne ne croît jamais le long d'une arête,
//! donc tous les nœuds d'un cycle ont la même, et le biais interdit à ce cycle
//! de la dépasser. Le départ atteignant tout le graphe, sa moyenne est la
//! meilleure.

/// Un graphe dont chaque nœud a au moins une arête sortante, en tableaux
/// plats : les arêtes du nœud `u` sont `debut[u]..debut[u + 1]`.
pub(crate) struct Graphe<'a> {
    pub debut: &'a [u32],
    pub vers: &'a [u32],
    pub poids: &'a [i64],
}

/// Une moyenne exacte : fraction réduite, dénominateur positif. Deux moyennes
/// égales ont donc la même écriture, ce que le biais exige.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Moyenne {
    pub num: i128,
    pub den: i128,
}

impl Moyenne {
    fn de(somme: i128, longueur: i128) -> Moyenne {
        let g = pgcd(somme.abs(), longueur).max(1);
        Moyenne {
            num: somme / g,
            den: longueur / g,
        }
    }

    fn depasse(self, autre: Moyenne) -> bool {
        self.num * autre.den > autre.num * self.den
    }
}

fn pgcd(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// La politique optimale, une arête par nœud, et la moyenne de chaque nœud ;
/// `None` si `max_iterations` ne suffit pas.
pub(crate) fn howard(g: &Graphe, max_iterations: usize) -> Option<(Vec<u32>, Vec<Moyenne>)> {
    let n = g.debut.len() - 1;
    let aretes = |u: usize| g.debut[u] as usize..g.debut[u + 1] as usize;
    // Au départ, l'arête la plus lourde de chaque nœud, la première à égalité.
    let mut pol: Vec<u32> = (0..n)
        .map(|u| {
            let mut choix = g.debut[u] as usize;
            for e in aretes(u) {
                if g.poids[e] > g.poids[choix] {
                    choix = e;
                }
            }
            choix as u32
        })
        .collect();
    for _ in 0..max_iterations {
        let (eta, x) = evaluer(g, &pol);
        // Le gain d'abord : une arête qui mène vers une meilleure moyenne.
        let mut change = false;
        for u in 0..n {
            let mut choix = pol[u] as usize;
            let mut meilleure = eta[g.vers[choix] as usize];
            for e in aretes(u) {
                let m = eta[g.vers[e] as usize];
                if m.depasse(meilleure) {
                    meilleure = m;
                    choix = e;
                }
            }
            if choix != pol[u] as usize {
                pol[u] = choix as u32;
                change = true;
            }
        }
        if change {
            continue;
        }
        // Puis le biais, entre arêtes qui mènent à la même moyenne.
        for u in 0..n {
            let m = eta[u];
            let mut choix = pol[u] as usize;
            let mut meilleur = x[u];
            for e in aretes(u) {
                let v = g.vers[e] as usize;
                if eta[v] != m {
                    continue;
                }
                let valeur = i128::from(g.poids[e]) * m.den - m.num + x[v];
                if valeur > meilleur {
                    meilleur = valeur;
                    choix = e;
                }
            }
            if choix != pol[u] as usize {
                pol[u] = choix as u32;
                change = true;
            }
        }
        if !change {
            return Some((pol, eta));
        }
    }
    None
}

/// Ce que vaut une politique : la moyenne du cycle où mène chaque nœud, et son
/// biais multiplié par le dénominateur de cette moyenne, pour rester entier.
fn evaluer(g: &Graphe, pol: &[u32]) -> (Vec<Moyenne>, Vec<i128>) {
    let n = pol.len();
    let suivant = |u: usize| g.vers[pol[u] as usize] as usize;
    let poids = |u: usize| i128::from(g.poids[pol[u] as usize]);
    // 0 : pas encore vu ; 1 : sur le chemin en cours ; 2 : évalué.
    let mut etat = vec![0u8; n];
    let mut eta = vec![Moyenne { num: 0, den: 1 }; n];
    let mut x = vec![0i128; n];
    let mut chemin: Vec<usize> = Vec::new();
    for depart in 0..n {
        if etat[depart] != 0 {
            continue;
        }
        let mut u = depart;
        while etat[u] == 0 {
            etat[u] = 1;
            chemin.push(u);
            u = suivant(u);
        }
        if etat[u] == 1 {
            // Un cycle neuf, de `u` à la fin du chemin.
            let entree = chemin
                .iter()
                .rposition(|&c| c == u)
                .expect("le nœud en cours est sur le chemin");
            let cycle: Vec<usize> = chemin[entree..].to_vec();
            chemin.truncate(entree);
            let somme: i128 = cycle.iter().map(|&c| poids(c)).sum();
            let m = Moyenne::de(somme, cycle.len() as i128);
            // La poignée, au biais nul : le plus petit indice du cycle.
            let p = (0..cycle.len())
                .min_by_key(|&i| cycle[i])
                .expect("un cycle a au moins un nœud");
            let longueur = cycle.len();
            x[cycle[p]] = 0;
            eta[cycle[p]] = m;
            // À rebours depuis la poignée : chaque nœud vaut son arête plus
            // le biais de son successeur.
            for k in 1..longueur {
                let i = (p + longueur - k) % longueur;
                let c = cycle[i];
                let apres = cycle[(i + 1) % longueur];
                x[c] = poids(c) * m.den - m.num + x[apres];
                eta[c] = m;
            }
            for &c in &cycle {
                etat[c] = 2;
            }
        }
        // Le reste du chemin descend vers un nœud déjà évalué.
        while let Some(c) = chemin.pop() {
            let v = suivant(c);
            let m = eta[v];
            eta[c] = m;
            x[c] = poids(c) * m.den - m.num + x[v];
            etat[c] = 2;
        }
    }
    (eta, x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resoudre(aretes: &[(u32, u32, i64)], n: usize) -> (Vec<u32>, Vec<Moyenne>) {
        let mut par_noeud: Vec<Vec<(u32, i64)>> = vec![Vec::new(); n];
        for &(u, v, w) in aretes {
            par_noeud[u as usize].push((v, w));
        }
        let mut debut = vec![0u32];
        let (mut vers, mut poids) = (Vec::new(), Vec::new());
        for liste in &par_noeud {
            for &(v, w) in liste {
                vers.push(v);
                poids.push(w);
            }
            debut.push(vers.len() as u32);
        }
        howard(
            &Graphe {
                debut: &debut,
                vers: &vers,
                poids: &poids,
            },
            1000,
        )
        .expect("converge")
    }

    /// Deux cycles joignables depuis le départ : 2 → 3 → 2 vaut 7 en
    /// moyenne, 1 → 1 vaut 6. L'arête la plus lourde du départ (10, vers le
    /// cycle à 6) est le piège d'une politique gloutonne.
    #[test]
    fn le_meilleur_cycle_l_emporte_sur_la_plus_grosse_arete() {
        let (_, eta) = resoudre(&[(0, 1, 10), (0, 2, 0), (1, 1, 6), (2, 3, 4), (3, 2, 10)], 4);
        assert_eq!(eta[0], Moyenne { num: 7, den: 1 });
    }

    /// Une moyenne non entière reste exacte : 1 → 2 → 1 vaut 5 sur 2 tours.
    #[test]
    fn une_moyenne_se_garde_en_fraction() {
        let (_, eta) = resoudre(&[(0, 1, 0), (1, 2, 2), (2, 1, 3), (0, 0, 2)], 3);
        assert_eq!(eta[0], Moyenne { num: 5, den: 2 });
    }
}
