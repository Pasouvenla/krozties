// Le plateau des KrozTools : un damier vide, de quinze à vingt cases de côté, ou
// une carte de boss. Le choix est partagé entre les outils (`etat.plateau`) ; un
// outil qui ne lit pas les murs passe `avecCartes` à faux.
//
// Le sol des cartes vient du serveur (`crates/app/src/cartes.rs`) ; seul le
// damier vide se calcule ici, avec la règle de `bornes` du serveur : de -7 à 7
// pour quinze, de -10 à 9 pour vingt.

import { distance, memeCase } from './damier.js';
import { echapper } from './format.js';

export const COTE_DEFAUT = 15;
export const COTES = [15, 16, 17, 18, 19, 20];

export function bornes(cote) {
  const bas = -Math.floor(cote / 2);
  return [bas, bas + cote - 1];
}

export function damierVide(cote) {
  const [bas, haut] = bornes(cote);
  const sol = [];
  for (let x = bas; x <= haut; x += 1) {
    for (let y = bas; y <= haut; y += 1) sol.push([x, y]);
  }
  return { cote, sol, murs: [] };
}

const choix = (etat) => etat.plateau || (etat.plateau = { carte: null, damier: COTE_DEFAUT });

/// Les cartes de boss, demandées une fois. Sans elles, le sélecteur ne
/// propose que les damiers, et l'outil reste utilisable.
export async function chargerCartes(etat, appeler) {
  if (!etat.cartesDeBoss) {
    try {
      etat.cartesDeBoss = (await appeler('cartes')).cartes;
    } catch (e) {
      return [];
    }
  }
  return etat.cartesDeBoss;
}

/// Le plateau en vigueur : son sol et ses murs, et `carte` ou `cote` selon
/// qu'il s'agit d'une carte ou du damier vide.
export function plateauActif(etat, avecCartes = true) {
  const c = choix(etat);
  const carte = avecCartes && c.carte != null
    ? (etat.cartesDeBoss || []).find((k) => k.id === c.carte) : null;
  const p = carte
    ? { carte: carte.id, nom: carte.nom, sol: carte.sol, murs: carte.murs }
    : damierVide(c.damier || COTE_DEFAUT);
  const cles = new Set(p.sol.map(([x, y]) => `${x},${y}`));
  p.estSol = (k) => Boolean(k) && cles.has(`${k[0]},${k[1]}`);
  return p;
}

/// Ce que le serveur lit : la carte, ou le côté du damier.
export const pourLeServeur = (p) => (p.carte != null ? { carte: p.carte } : { damier: p.cote });

/// La liste déroulante : les damiers, puis les cartes par catégorie.
export function selecteur(id, etat, avecCartes = true) {
  const c = choix(etat);
  const cartes = avecCartes ? (etat.cartesDeBoss || []) : [];
  const valeur = c.carte != null && cartes.some((k) => k.id === c.carte)
    ? `carte:${c.carte}` : `damier:${c.damier}`;
  const option = (v, texte) =>
    `<option value="${v}"${v === valeur ? ' selected' : ''}>${echapper(texte)}</option>`;
  const damiers = COTES.map((n) => option(`damier:${n}`,
    `${n} × ${n}${n === COTE_DEFAUT ? ', taille moyenne des cartes de boss' : ''}`)).join('');
  const categories = [...new Set(cartes.map((k) => k.categorie))];
  const groupes = categories.map((categorie) => `<optgroup label="${echapper(categorie)}">${
    cartes.filter((k) => k.categorie === categorie)
      .sort((a, b) => a.nom.localeCompare(b.nom, 'fr', { sensitivity: 'base' }))
      .map((k) => option(`carte:${k.id}`, k.nom)).join('')}</optgroup>`).join('');
  return `<label class="choix-plateau" for="${id}">${avecCartes ? 'Plateau' : 'Damier'}
    <select id="${id}"><optgroup label="Damier vide">${damiers}</optgroup>${groupes}</select></label>`;
}

/// Retient le choix lu dans la liste. Sans les cartes, seule la taille du
/// damier change : la carte choisie dans un autre outil reste la sienne.
export function choisir(etat, valeur, avecCartes = true) {
  const c = choix(etat);
  const [genre, n] = valeur.split(':');
  if (genre === 'carte') c.carte = Number(n);
  else {
    c.damier = Number(n);
    if (avecCartes) c.carte = null;
  }
}

/// La case de sol libre la plus proche de `c`, `c` lui-même s'il convient.
export function recaler(c, p, prises = []) {
  if (!c) return c;
  const libre = (k) => !prises.some((q) => memeCase(q, k));
  if (p.estSol(c) && libre(c)) return c;
  let meilleure = null;
  let plusCourte = Infinity;
  p.sol.forEach((k) => {
    const d = distance(k, c);
    if (d < plusCourte && libre(k)) { plusCourte = d; meilleure = k; }
  });
  return meilleure;
}

/// Retire d'une liste, en place, ce qui n'est pas sur le sol ; rend le nombre
/// de retirés. `cle` lit la case d'un élément.
export function garderLeSol(liste, p, cle = (e) => e) {
  const avant = liste.length;
  for (let i = liste.length - 1; i >= 0; i -= 1) {
    if (!p.estSol(cle(liste[i]))) liste.splice(i, 1);
  }
  return avant - liste.length;
}

/// La phrase qui dit ce que le changement de plateau a déplacé ou retiré.
export function bilanDuRecalage(deplaces, retires) {
  const morceaux = [];
  if (retires) morceaux.push(`${retires} ${retires > 1 ? 'pièces posées hors du sol ont été retirées' : 'pièce posée hors du sol a été retirée'}`);
  if (deplaces) morceaux.push(`${deplaces > 1 ? `${deplaces} pièces ont été ramenées` : 'une pièce a été ramenée'} sur la case de sol la plus proche`);
  return morceaux.length ? ` ${morceaux.join(', ')}.`.replace(/^ ./, (m) => m.toUpperCase()) : '';
}

