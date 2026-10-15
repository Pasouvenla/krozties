// Les résistances de la cible, telles que la Rotation les règle : aucune, le
// même pourcentage partout, ou un par élément ; plus les fixes, la critique et
// celles aux sorts, aux armes, en mêlée et à distance. Les mêmes pour tous les
// ennemis.

import { FR_ELEMENT } from './format.js';

/// L'ordre du moteur, celui de ses tableaux de cinq.
const ORDRE_DU_MOTEUR = ['Fire', 'Earth', 'Air', 'Water', 'Neutral'];

/// L'ordre du jeu, pour l'affichage.
export const ELEMENTS = ['Neutral', 'Earth', 'Fire', 'Water', 'Air'].map((e) => [e, FR_ELEMENT[e]]);

export const DOMAINES = [['sorts', 'aux sorts'], ['armes', 'aux armes'], ['melee', 'mêlée'], ['distance', 'distance']];

export const neutres = () => ({
  mode: 'aucune', uniforme: 0, elements: {}, fixes: {}, critiques: 0, sorts: 0, armes: 0, melee: 0, distance: 0,
});

/// Un pourcentage saisi, entier, entre -100 et 100.
export const borne = (v) => Math.max(-100, Math.min(100, Math.round(Number(v) || 0)));

/// Une résistance fixe saisie, entière.
export const fixe = (v) => Math.max(-9999, Math.min(9999, Math.round(Number(v) || 0)));

/// Les champs de la demande de rotation.
export function versMoteur(r, avecArmes) {
  const res = r || neutres();
  const parElement = (e) => {
    if (res.mode === 'uniformes') return res.uniforme || 0;
    return res.mode === 'elements' ? (res.elements[e] || 0) : 0;
  };
  return {
    resistance_percent: ORDRE_DU_MOTEUR.map(parElement),
    resistance_flat: ORDRE_DU_MOTEUR.map((e) => (res.fixes && res.fixes[e]) || 0),
    resistance_critical: res.critiques || 0,
    resistance_spell: res.sorts || 0,
    resistance_weapon: avecArmes ? (res.armes || 0) : 0,
    resistance_melee: res.melee || 0,
    resistance_ranged: res.distance || 0,
  };
}

const signe = (v) => (v < 0 ? `−${-v}` : `${v}`);

/// Ce que la cible d'une demande résiste, en clair ; vide si elle ne résiste à
/// rien.
export function decrire(demande) {
  if (!demande) return '';
  const pc = demande.resistance_percent || [0, 0, 0, 0, 0];
  const parties = [];
  if (pc[0] && pc.every((v) => v === pc[0])) {
    parties.push(`${signe(pc[0])} % partout`);
  } else {
    const els = ELEMENTS
      .map(([e, nom]) => [nom, pc[ORDRE_DU_MOTEUR.indexOf(e)]])
      .filter(([, v]) => v)
      .map(([nom, v]) => `${nom} ${signe(v)} %`);
    parties.push(...els);
  }
  const fixes = demande.resistance_flat || [0, 0, 0, 0, 0];
  ELEMENTS
    .map(([e, nom]) => [nom, fixes[ORDRE_DU_MOTEUR.indexOf(e)]])
    .filter(([, v]) => v)
    .forEach(([nom, v]) => parties.push(`${nom} ${signe(v)} fixes`));
  if (demande.resistance_critical) parties.push(`${signe(demande.resistance_critical)} critiques`);
  const domaine = { sorts: 'resistance_spell', armes: 'resistance_weapon', melee: 'resistance_melee', distance: 'resistance_ranged' };
  DOMAINES.forEach(([d, nom]) => {
    const v = demande[domaine[d]];
    if (v) parties.push(`${signe(v)} % ${nom}`);
  });
  return parties.join(' · ');
}
