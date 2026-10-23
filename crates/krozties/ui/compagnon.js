// Le compagnon de combat : la rotation calculée, tour par tour, dans une petite
// fenêtre par-dessus le jeu. En barre, comme celle des sorts du jeu, ou en
// liste. Trois touches, réglées dans la Rotation, avancent d'un sort, d'un tour,
// ou reculent ; un clic sur un sort le désigne comme le prochain. Quand le
// combat dévie, la fiche de l'écart dit ce qui a changé, et le compagnon
// recalcule la suite depuis là.

import { appeler, hote } from './pont.js';
import { echapper, nb } from './commun/format.js';
import { CLE, suiviDe } from './commun/suivi.js';
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
/// La fiche de l'écart, quand elle est ouverte.
let fiche = null;

/// La place dans la rotation : le tour, et le prochain sort à lancer, la
/// longueur du tour quand il est fini. Un nouveau calcul repart du tour 1.
function positionDe(s) {
  const p = lire(CLE_POSITION);
  if (s && p && p.calcule_le === s.calcule_le) return p;
  return { calcule_le: s && s.calcule_le, tour: 0, lancer: 0 };
}

function avancer(geste) {
  if (!suivi || !suivi.tours.length || fiche) return;
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

/// Le badge d'une rotation recalculée en cours de combat, depuis le tour où elle
/// l'a été.
const recale = () => (suivi.recale !== undefined && position.tour >= suivi.recale
  ? '<span class="cp-recale">recalé</span>' : '');

const outils = () => `
  <span class="cp-outils">
    ${suivi && suivi.requete && suivi.tours && suivi.tours.length
      ? '<button type="button" data-action="recaler" title="Le combat a dévié ?" aria-label="Recaler">⟲</button>' : ''}
    <button type="button" data-action="disposition" title="${disposition === 'barre' ? 'Afficher en liste' : 'Afficher en barre'}"
      aria-label="${disposition === 'barre' ? 'Afficher en liste' : 'Afficher en barre'}">${disposition === 'barre' ? '☰' : '▭'}</button>
    <button type="button" data-action="fermer" title="Fermer" aria-label="Fermer">×</button>
  </span>`;

function barre() {
  const t = suivi.tours[position.tour];
  const suite = suivi.tours[position.tour + 1];
  return `<div class="cp cp-barre">
    <span class="cp-tour" data-poignee title="Tour ${position.tour + 1} sur ${suivi.tours.length}, ${t.pa} PA · ${
      echapper(GESTES.map((g) => `${libelle(touches[g.id])} ${g.court}`).join(', '))}">T${position.tour + 1}${recale()}</span>
    ${t.lancers.map((l, i) => `<button type="button" class="cp-case${etat(i)}" data-lancer="${i}"
      title="${echapper(l.nom)}, ${l.pa} PA${l.indications.length ? `, ${echapper(l.indications.join(', '))}` : ''}">${icone(l)}<i>${i + 1}</i></button>`).join('')}
    ${suite ? `<span class="cp-sep"></span><span class="cp-suite" data-poignee title="Tour ${position.tour + 2}">${suite.lancers.map(icone).join('')}</span>` : ''}
    ${outils()}
  </div>`;
}

function liste() {
  const t = suivi.tours[position.tour];
  return `<div class="cp cp-liste">
    <div class="cp-tete" data-poignee><b>Tour ${position.tour + 1}</b>${recale()}<span>${t.pa} PA · ${nb(t.degats)}</span>${outils()}</div>
    <ol>${t.lancers.map((l, i) => `
      <li class="${etat(i).trim()}" data-lancer="${i}">${icone(l)}<span class="nom">${echapper(l.nom)}</span>${
        l.indications.map((x) => `<span class="cp-etiquette">${echapper(x)}</span>`).join('')}<span class="pa">${l.pa}</span></li>`).join('')}
    </ol>
    <div class="cp-pied">${GESTES.map((g) => `<span><kbd>${echapper(libelle(touches[g.id]))}</kbd> ${g.court}</span>`).join('')}</div>
  </div>`;
}

const pas = (cle, valeur) => `<span class="cp-pas"><button type="button" data-pas="${echapper(cle)}" data-sens="-1"
  aria-label="Moins">−</button><output>${valeur}</output><button type="button" data-pas="${echapper(cle)}" data-sens="1"
  aria-label="Plus">+</button></span>`;

/// La fiche de l'écart : ce tour-ci, sort par sort, lancé ou non, un sort hors
/// plan en plus, les PA perdus, les ennemis encore debout, les compteurs que
/// la Rotation suit.
function ficheDeLEcart() {
  const f = fiche;
  const jetons = f.lances.map((l, i) => `<button type="button" class="cp-jeton ${l.lance ? 'oui' : 'non'}" data-jeton="${i}"
    title="${echapper(l.nom)}, ${l.lance ? 'lancé' : 'pas lancé'}">${icone(l)}<b>${l.lance ? '✓' : '✗'}</b></button>`).join('');
  const sorts = f.ajout ? `<div class="cp-sorts">${f.sorts.map((s) => `<button type="button" class="cp-jeton"
    data-sort="${echapper(s.id)}" title="${echapper(s.nom)}">${icone({ icone: s.dofusdb_id, objet: s.icone_objet, nom: s.nom })}</button>`)
    .join('')}</div>` : '';
  const compteurs = f.compteurs.map((c) => `<div class="cp-ligne"><span>${echapper(c.nom)}</span>${c.max === 1
    ? `<span class="cp-bascule">${[1, 0].map((v) => `<button type="button" data-compteur="${echapper(c.id)}" data-valeur="${v}"
        class="${c.valeur === v ? 'actif' : ''}">${v ? 'Oui' : 'Non'}</button>`).join('')}</span>`
    : pas(`compteur:${c.id}`, c.valeur)}</div>`).join('');
  return `<div class="cp cp-fiche">
    <div class="cp-tete" data-poignee><b>Le combat a dévié ?</b><span>Tour ${position.tour + 1}</span></div>
    <div class="cp-ligne"><span>Lancés ce tour</span><span class="cp-jetons">${jetons}<button type="button"
      class="cp-jeton cp-plus" data-action="ajouter" title="Un sort hors plan" aria-label="Ajouter un sort">+</button></span></div>
    ${sorts}
    <div class="cp-ligne"><span>PA en moins ce tour</span>${pas('pa', f.pa)}</div>
    <div class="cp-ligne"><span>Ennemis encore debout</span>${pas('ennemis', f.ennemis)}</div>
    ${compteurs}
    ${f.erreur ? `<p class="cp-erreur">${echapper(f.erreur)}</p>` : ''}
    <div class="cp-boutons"><button type="button" class="cp-second" data-action="annuler">Annuler</button><button type="button"
      class="cp-principal" data-action="recalculer"${f.calcul ? ' disabled' : ''}>${f.calcul ? 'Calcul…' : 'Recalculer la suite'}</button></div>
  </div>`;
}

function dessiner() {
  const corps = document.getElementById('compagnon');
  if (fiche) corps.innerHTML = ficheDeLEcart();
  else {
    corps.innerHTML = suivi && suivi.tours && suivi.tours.length
      ? (disposition === 'liste' ? liste() : barre())
      : `<div class="cp cp-vide" data-poignee>Calculez une rotation dans Krozties : elle s'affiche ici.${outils()}</div>`;
  }
  ajusterLaFenetre();
}

/// Les tours déjà joués, et les lancers faits ce tour, comme le moteur les lit :
/// par leur nom dans la rotation, ou par l'identifiant d'un sort hors plan.
const passes = () => suivi.tours.slice(0, position.tour).map((t) => t.lancers.map((l) => l.nom));
const faits = () => fiche.lances.filter((l) => l.lance).map((l) => (l.horsPlan ? l.id : l.nom));

/// La fiche s'ouvre remplie d'après le curseur : ce qui le précède est lancé.
function ouvrirLaFiche() {
  const t = suivi.tours[position.tour];
  fiche = {
    lances: t.lancers.map((l, i) => ({ ...l, lance: i < position.lancer })),
    pa: 0,
    ennemis: suivi.requete.targets || 1,
    compteurs: [],
    corriges: {},
    sorts: [],
    ajout: false,
    calcul: false,
    erreur: '',
  };
  dessiner();
  lireLAEcart();
}

/// Les compteurs à cet instant et les sorts du deck ; un compteur que le joueur
/// a corrigé garde sa valeur.
async function lireLAEcart() {
  const f = fiche;
  try {
    const r = await appeler('reprise', { requete: suivi.requete, tours: passes(), ce_tour: faits(), apercu: true });
    if (fiche !== f) return;
    f.sorts = r.sorts;
    f.compteurs = r.compteurs.map((c) => (c.id in f.corriges ? { ...c, valeur: f.corriges[c.id] } : c));
    f.erreur = '';
  } catch (e) {
    if (fiche !== f) return;
    f.erreur = e.message;
  }
  dessiner();
}

async function recalculer() {
  const f = fiche;
  if (!f || f.calcul) return;
  f.calcul = true;
  f.erreur = '';
  dessiner();
  const tour = position.tour;
  const ceTour = faits();
  try {
    const r = await appeler('reprise', {
      requete: suivi.requete,
      tours: passes(),
      ce_tour: ceTour,
      pa_en_moins: f.pa,
      ennemis: f.ennemis,
      compteurs: f.corriges,
    });
    if (fiche !== f) return;
    suivi = { ...suiviDe(r, suivi.build, { ...suivi.requete, targets: r.targets }), recale: tour };
    position = { calcule_le: suivi.calcule_le, tour, lancer: ceTour.length };
    ecrire(CLE, suivi);
    ecrire(CLE_POSITION, position);
    fiche = null;
  } catch (e) {
    if (fiche !== f) return;
    f.calcul = false;
    f.erreur = e.message;
  }
  dessiner();
}

/// Un clic dans la fiche : un sort basculé, ajouté ou retiré, un pas, un
/// compteur. Rend vrai s'il a servi.
function cliquerDansLaFiche(e) {
  const f = fiche;
  const jeton = e.target.closest('[data-jeton]');
  if (jeton) {
    const i = Number(jeton.dataset.jeton);
    if (f.lances[i].horsPlan) f.lances.splice(i, 1);
    else f.lances[i].lance = !f.lances[i].lance;
    dessiner();
    lireLAEcart();
    return true;
  }
  const sort = e.target.closest('[data-sort]');
  if (sort) {
    const s = f.sorts.find((x) => x.id === sort.dataset.sort);
    if (s) {
      f.lances.push({ id: s.id, nom: s.nom, icone: s.dofusdb_id, objet: s.icone_objet, indications: [], lance: true, horsPlan: true });
    }
    f.ajout = false;
    dessiner();
    lireLAEcart();
    return true;
  }
  const pasCible = e.target.closest('[data-pas]');
  if (pasCible) {
    const sens = Number(pasCible.dataset.sens);
    const cle = pasCible.dataset.pas;
    if (cle === 'pa') f.pa = Math.min(12, Math.max(0, f.pa + sens));
    else if (cle === 'ennemis') f.ennemis = Math.min(8, Math.max(1, f.ennemis + sens));
    else if (cle.startsWith('compteur:')) {
      const c = f.compteurs.find((x) => x.id === cle.slice(9));
      if (c) {
        c.valeur = Math.min(c.max, Math.max(0, c.valeur + sens));
        f.corriges[c.id] = c.valeur;
      }
    }
    dessiner();
    return true;
  }
  const compteur = e.target.closest('[data-compteur]');
  if (compteur) {
    const c = f.compteurs.find((x) => x.id === compteur.dataset.compteur);
    if (c) {
      c.valeur = Number(compteur.dataset.valeur);
      f.corriges[c.id] = c.valeur;
    }
    dessiner();
    return true;
  }
  return false;
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
  const geste = action && action.dataset.action;
  if (geste === 'recaler') {
    ouvrirLaFiche();
    return;
  }
  if (fiche) {
    if (geste === 'annuler') {
      fiche = null;
      dessiner();
    } else if (geste === 'recalculer') recalculer();
    else if (geste === 'ajouter') {
      fiche.ajout = !fiche.ajout;
      dessiner();
    } else cliquerDansLaFiche(e);
    return;
  }
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
  fiche = null;
  dessiner();
});

dessiner();
