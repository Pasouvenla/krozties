// Les encarts repliables : un clic sur la barre de titre réduit l'encart à
// elle-même, un second le rouvre, pour que le joueur garde sous les yeux ce
// qu'il veut en priorité.
//
// Ce qui est replié le reste d'un affichage à l'autre : `etat.replies` le
// retient, et recalculer une rotation, qui redessine ses encarts, ne rouvre
// pas ce que le joueur a fermé.

const estReplie = (etat, cle) => Boolean(etat.replies && etat.replies[cle]);

/// La barre de titre d'un encart repliable : le titre est un bouton, et
/// `suite`, un compteur par exemple, reste visible l'encart replié.
export function titreRepliable(etat, cle, titre, suite = '') {
  return `<h3 class="titre-repli"><button type="button" data-repli="${cle}"
    aria-expanded="${!estReplie(etat, cle)}">${titre}</button>${suite}</h3>`;
}

/// La classe à ajouter à la `.carte` d'un encart replié.
export const classeRepli = (etat, cle) => (estReplie(etat, cle) ? ' repliee' : '');

/// Branche, une fois, les barres de titre de `hote` : par délégation, elles
/// tiennent quand une partie de la page se redessine.
export function brancherRepli(hote, etat) {
  hote.addEventListener('click', (ev) => {
    const bouton = ev.target.closest('button[data-repli]');
    if (!bouton || !hote.contains(bouton)) return;
    etat.replies = etat.replies || {};
    const cle = bouton.dataset.repli;
    etat.replies[cle] = !etat.replies[cle];
    bouton.closest('.carte').classList.toggle('repliee', etat.replies[cle]);
    bouton.setAttribute('aria-expanded', String(!etat.replies[cle]));
  });
}
