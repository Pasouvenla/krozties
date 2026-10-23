// Partager une rotation : le build, les réglages de la Rotation et ce qu'ils
// donnent, dans un fichier `.krozties`. Celui qui l'ouvre retrouve le même
// build, les mêmes réglages, et la même rotation une fois recalculée.

import { appeler, hote } from '../pont.js';
import { pluriel } from './format.js';
import { orientationDu } from './valeurs.js';

const FORMAT = 'krozties-rotation';
const POIDS_MAX = 1000000;
const EN_FRANCAIS = { Earth: 'Terre', Fire: 'Feu', Water: 'Eau', Air: 'Air', Neutral: 'Neutre' };
const POSITIONS = { distance: 'à distance', melee: 'au corps à corps' };

/// Le fichier d'une rotation calculée.
export function contenuDuPartage(etat, resultat, demande) {
  const r = etat.resolu || {};
  const reglages = etat.rotation || {};
  return JSON.stringify({
    format: FORMAT,
    version: 1,
    build: etat.build,
    // Les sorts cochés sont un ensemble, que le JSON n'écrit pas : une liste.
    reglages: { ...reglages, coches: reglages.coches ? [...reglages.coches] : null },
    resume: {
      classe: r.class_name || null,
      niveau: r.level || null,
      tours: (resultat.turns || []).length,
      ennemis: demande.targets || 1,
      position: demande.position || null,
      total: Math.round(resultat.total || 0),
    },
  });
}

/// « Forgelance-Terre-Feu.krozties » : la classe, puis ses éléments. La Force
/// porte aussi le Neutre, qui ne se nomme que s'il est seul.
export function nomDuFichier(etat) {
  const r = etat.resolu || {};
  const orientation = orientationDu(r.elements);
  const elements = orientation.filter((e) => e !== 'Neutral' || orientation.length === 1).map((e) => EN_FRANCAIS[e] || e);
  return `${[r.class_name || 'Rotation', ...elements].join('-').replace(/[\s/\\:*?"<>|]+/g, '-')}.krozties`;
}

/// Enregistre le fichier : par la fenêtre du système dans l'application de
/// bureau, en téléchargement dans un navigateur. Rend le chemin choisi, ou
/// `null` si le joueur renonce.
export async function enregistrer(contenu, nom) {
  if (hote === 'bureau') return appeler('enregistrerRotation', { nom, contenu });
  const lien = document.createElement('a');
  lien.href = URL.createObjectURL(new Blob([contenu], { type: 'application/json' }));
  lien.download = nom;
  lien.click();
  setTimeout(() => URL.revokeObjectURL(lien.href), 1000);
  return nom;
}

/// Une rotation reçue, vérifiée avant d'être proposée.
export function lireLePartage(texte) {
  if (texte.length > POIDS_MAX) throw new Error('Ce fichier est trop lourd pour une rotation Krozties.');
  let d = null;
  try {
    d = JSON.parse(texte);
  } catch (e) { /* refusé plus bas */ }
  if (!d || d.format !== FORMAT || !d.build || !Number.isInteger(d.build.class)
    || !d.reglages || typeof d.reglages !== 'object') {
    throw new Error("Ce fichier n'est pas une rotation Krozties.");
  }
  if (d.version > 1) throw new Error('Cette rotation vient d\'une version plus récente : mettez Krozties à jour pour l\'ouvrir.');
  return d;
}

/// Les réglages de la Rotation tels qu'elle les tient, les sorts cochés
/// redevenus un ensemble.
export function reglagesRecus(d) {
  const coches = Array.isArray(d.reglages.coches)
    ? new Set(d.reglages.coches.filter((id) => typeof id === 'string')) : null;
  return { ...d.reglages, coches };
}

/// « Forgelance 200 · 10 tours · 4 ennemis · à distance ».
export function resumeDuPartage(d) {
  const r = d.resume || {};
  return [
    [r.classe, r.niveau].filter(Boolean).join(' ') || 'Rotation',
    r.tours ? `${r.tours} ${pluriel(r.tours, 'tour')}` : null,
    r.ennemis ? `${r.ennemis} ${pluriel(r.ennemis, 'ennemi')}` : null,
    POSITIONS[r.position] || null,
  ].filter(Boolean).join(' · ');
}
