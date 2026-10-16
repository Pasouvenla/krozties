// Les réglages de la Rotation, gardés sur cet ordinateur d'un lancement à
// l'autre : tours, ennemis, position, résistances de la cible, invocations et
// ceux de « Pour aller plus loin ». Les sorts cochés et le mur de bombes
// suivent la classe du build chargé, ils ne se gardent pas.

import { neutres } from './resistances.js';

const CLE = 'krozties.rotation';
const POSITIONS = ['libre', 'distance', 'melee'];

const entier = (v, min, max, defaut) => (Number.isInteger(v) && v >= min && v <= max ? v : defaut);

/// Ce qui a été gardé, chaque valeur vérifiée : une valeur abîmée reprend son
/// défaut.
function lus() {
  let d;
  try {
    d = JSON.parse(localStorage.getItem(CLE)) || {};
  } catch (e) {
    return {};
  }
  const r = {};
  if ('horizon' in d) r.horizon = entier(d.horizon, 1, 15, 7);
  if ('cibles' in d) r.cibles = entier(d.cibles, 1, 64, 1);
  if ('position' in d) r.position = POSITIONS.includes(d.position) ? d.position : 'libre';
  if ('pmDepenses' in d) r.pmDepenses = entier(d.pmDepenses, 0, 12, 0);
  if ('etalement' in d) r.etalement = entier(d.etalement, 0, 4, 0);
  if ('maitrise' in d) r.maitrise = [0, 300, 360].includes(d.maitrise) ? d.maitrise : 300;
  if ('pousseesBloquees' in d) r.pousseesBloquees = Boolean(d.pousseesBloquees);
  if ('exhaustif' in d) r.exhaustif = Boolean(d.exhaustif);
  if (d.etats && typeof d.etats === 'object') r.etats = { ...d.etats };
  if (d.resistances && typeof d.resistances === 'object') r.resistances = { ...neutres(), ...d.resistances };
  if ('invocationsDeClasse' in d) r.invocationsDeClasse = d.invocationsDeClasse !== false;
  if ('invocationsCommunes' in d) r.invocationsCommunes = d.invocationsCommunes === true;
  return r;
}

/// Les réglages de la session, créés à la première demande depuis ce qui a
/// été gardé.
export function reglagesRotation(etat) {
  if (!etat.rotation) {
    etat.rotation = {
      coches: null,
      horizon: 7,
      pmDepenses: 0,
      /// La Maîtrise d'arme, en Puissance : la normale par défaut.
      maitrise: 300,
      /// Les poussées butent contre un obstacle : leurs dommages comptent.
      pousseesBloquees: false,
      cibles: 1,
      etalement: 0,
      /// Chercher sur tous les sorts à la fois, au lieu des deux passes : plus
      /// long, rarement meilleur.
      exhaustif: false,
      /// Ce que le joueur déclare de sa situation, par identifiant de réglage.
      etats: {},
      /// Où il se tient : `libre`, `distance` ou `melee`.
      position: 'libre',
      /// Les résistances de la cible, les mêmes pour tous les ennemis.
      resistances: neutres(),
      /// Les invocations de la classe jouent d'office ; les communes (Arakne,
      /// Chaferfu, Cawotte), que les joueurs n'emploient pas, attendent la case.
      invocationsDeClasse: true,
      invocationsCommunes: false,
      ...lus(),
    };
  }
  return etat.rotation;
}

/// Garde les réglages ; un stockage qui refuse ne gêne rien.
export function garder(reglages) {
  const {
    horizon, cibles, position, pmDepenses, etalement, maitrise, pousseesBloquees, exhaustif, etats, resistances,
    invocationsDeClasse, invocationsCommunes,
  } = reglages;
  try {
    localStorage.setItem(CLE, JSON.stringify({
      horizon, cibles, position, pmDepenses, etalement, maitrise, pousseesBloquees, exhaustif, etats, resistances,
      invocationsDeClasse, invocationsCommunes,
    }));
  } catch (e) {
    // Navigation privée ou stockage plein : les réglages valent pour la session.
  }
}
