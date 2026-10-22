// Le compagnon de combat : la rotation calculée, tour par tour, dans une petite
// fenêtre par-dessus le jeu. En barre, comme celle des sorts du jeu, ou en
// liste. Trois touches, réglées dans la Rotation, avancent d'un sort, d'un tour,
// ou reculent ; un clic sur un sort le désigne comme le prochain.

import { appeler, hote } from './pont.js';
import { echapper, nb } from './commun/format.js';
import { CLE } from './commun/suivi.js';
import { CLE_TOUCHES, GESTES, gesteDe, libelle, lireLesTouches } from './commun/touches.js';

const CLE_POSITION = 'krozties.compagnon.position';
const CLE_DISPOSITION = 'krozties.compagnon.disposition';

const lire = (cle) => {
  try {
    return JSON.parse(localStorage.getItem(cle) || 'null');
  } catch (e) {
    return null;
  }
};
const ecrire = (cle, valeur) => {
  try {
    localStorage.setItem(cle, typeof valeur === 'string' ? valeur : JSON.stringify(valeur));
  } catch (e) { /* le compagnon reste utilisable, sans mémoire */ }
};

let suivi = lire(CLE);
let touches = lireLesTouches();
let position = positionDe(suivi);
let disposition = (() => {
  try {
    return localStorage.getItem(CLE_DISPOSITION) === 'liste' ? 'liste' : 'barre';
  } catch (e) {
    return 'barre';
  }
})();
let derniereTaille = '';

/// La place dans la rotation : le tour, et le prochain sort à lancer, la
/// longueur du tour quand il est fini. Un nouveau calcul repart du tour 1.
function positionDe(s) {
  const p = lire(CLE_POSITION);
  if (s && p && p.calcule_le === s.calcule_le) return p;
  return { calcule_le: s && s.calcule_le, tour: 0, lancer: 0 };
}

function avancer(geste) {
  if (!suivi || !suivi.tours.length) return;
  const tours = suivi.tours;
  let { tour, lancer } = position;
  if (geste === 'suivant') {
    if (lancer < tours[tour].lancers.length) lancer += 1;
    else if (tour < tours.length - 1) [tour, lancer] = [tour + 1, 0];
  } else if (geste === 'tour') {
    if (tour < tours.length - 1) [tour, lancer] = [tour + 1, 0];
  } else if (geste === 'precedent') {
    if (lancer > 0) lancer -= 1;
    else if (tour > 0) [tour, lancer] = [tour - 1, tours[tour - 1].lancers.length];
  }
  position = { ...position, tour, lancer };
  ecrire(CLE_POSITION, position);
  dessiner();
}
// Les raccourcis globaux, reçus de l'application de bureau.
window.compagnonTouche = avancer;

const icone = (l) => {
  if (l.objet) return `<img src="objets/${l.objet}.webp" alt="">`;
  if (l.icone) return `<img src="icons/${l.icone}.png" alt="">`;
  return `<span class="cp-sans-icone">${echapper(l.nom.slice(0, 2))}</span>`;
};

const etat = (i) => (i < position.lancer ? ' fait' : i === position.lancer ? ' prochain' : '');

const outils = () => `
  <span class="cp-outils">
    <button type="button" data-action="disposition" title="${disposition === 'barre' ? 'Afficher en liste' : 'Afficher en barre'}"
      aria-label="${disposition === 'barre' ? 'Afficher en liste' : 'Afficher en barre'}">${disposition === 'barre' ? '☰' : '▭'}</button>
    <button type="button" data-action="fermer" title="Fermer" aria-label="Fermer">×</button>
  </span>`;

function barre() {
  const t = suivi.tours[position.tour];
  const suite = suivi.tours[position.tour + 1];
  return `<div class="cp cp-barre">
    <span class="cp-tour" data-poignee title="Tour ${position.tour + 1} sur ${suivi.tours.length}, ${t.pa} PA · ${
      echapper(GESTES.map((g) => `${libelle(touches[g.id])} ${g.court}`).join(', '))}">T${position.tour + 1}</span>
    ${t.lancers.map((l, i) => `<button type="button" class="cp-case${etat(i)}" data-lancer="${i}"
      title="${echapper(l.nom)}, ${l.pa} PA${l.indications.length ? `, ${echapper(l.indications.join(', '))}` : ''}">${icone(l)}<i>${i + 1}</i></button>`).join('')}
    ${suite ? `<span class="cp-sep"></span><span class="cp-suite" data-poignee title="Tour ${position.tour + 2}">${suite.lancers.map(icone).join('')}</span>` : ''}
    ${outils()}
  </div>`;
}

function liste() {
  const t = suivi.tours[position.tour];
  return `<div class="cp cp-liste">
    <div class="cp-tete" data-poignee><b>Tour ${position.tour + 1}</b><span>${t.pa} PA · ${nb(t.degats)}</span>${outils()}</div>
    <ol>${t.lancers.map((l, i) => `
      <li class="${etat(i).trim()}" data-lancer="${i}">${icone(l)}<span class="nom">${echapper(l.nom)}</span>${
        l.indications.map((x) => `<span class="cp-etiquette">${echapper(x)}</span>`).join('')}<span class="pa">${l.pa}</span></li>`).join('')}
    </ol>
    <div class="cp-pied">${GESTES.map((g) => `<span><kbd>${echapper(libelle(touches[g.id]))}</kbd> ${g.court}</span>`).join('')}</div>
  </div>`;
}

function dessiner() {
  const corps = document.getElementById('compagnon');
  corps.innerHTML = suivi && suivi.tours && suivi.tours.length
    ? (disposition === 'liste' ? liste() : barre())
    : `<div class="cp cp-vide" data-poignee>Calculez une rotation dans Krozties : elle s'affiche ici.${outils()}</div>`;
  ajusterLaFenetre();
}

/// La fenêtre de bureau prend la taille de ce qu'elle montre.
function ajusterLaFenetre() {
  if (hote !== 'bureau') return;
  const cp = document.querySelector('.cp');
  if (!cp) return;
  const r = cp.getBoundingClientRect();
  const taille = `${Math.ceil(r.width)}x${Math.ceil(r.height)}`;
  if (taille === derniereTaille) return;
  derniereTaille = taille;
  appeler('compagnonAjuster', { largeur: Math.ceil(r.width), hauteur: Math.ceil(r.height) }).catch(() => {});
}

document.addEventListener('click', (e) => {
  const action = e.target.closest('[data-action]');
  if (action && action.dataset.action === 'disposition') {
    disposition = disposition === 'barre' ? 'liste' : 'barre';
    ecrire(CLE_DISPOSITION, disposition);
    dessiner();
    return;
  }
  if (action && action.dataset.action === 'fermer') {
    if (hote === 'bureau') appeler('compagnonFermer').catch(() => {});
    else window.close();
    return;
  }
  const lancer = e.target.closest('[data-lancer]');
  if (lancer) {
    position = { ...position, lancer: Number(lancer.dataset.lancer) };
    ecrire(CLE_POSITION, position);
    dessiner();
  }
});

// Tirer la poignée déplace la fenêtre sans bordure.
document.addEventListener('mousedown', (e) => {
  if (e.button !== 0 || hote !== 'bureau' || !e.target.closest('[data-poignee]')) return;
  appeler('compagnonDeplacer').catch(() => {});
});

// Fenêtre au premier plan : les mêmes gestes au clavier, et les flèches.
document.addEventListener('keydown', (e) => {
  const geste = gesteDe(e, touches)
    || { ArrowRight: 'suivant', ArrowDown: 'tour', PageDown: 'tour', ArrowLeft: 'precedent' }[e.key];
  if (!geste) return;
  avancer(geste);
  e.preventDefault();
});

// Un nouveau calcul dans la Rotation : le compagnon le reprend au tour 1. De
// nouvelles touches : il les affiche.
window.addEventListener('storage', (e) => {
  if (e.key === CLE_TOUCHES) {
    touches = lireLesTouches();
    dessiner();
    return;
  }
  if (e.key !== CLE) return;
  suivi = lire(CLE);
  position = positionDe(suivi);
  dessiner();
});

dessiner();
