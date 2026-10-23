// Partager une rotation : le build, les réglages de la Rotation et les bonus
// écartés ou inclus, dans un fichier `.krozties` ou dans un lien. Celui qui
// l'ouvre retrouve le même build, les mêmes réglages, et la même rotation une
// fois recalculée.

import { appeler, hote } from '../pont.js';
import { pluriel } from './format.js';
import { chargeDuLien, lireLeLien } from './lien.js';
import { orientationDu } from './valeurs.js';

const FORMAT = 'krozties-rotation';
/// Le fichier que la page d'un lien enregistre : le lien seul.
const FORMAT_LIEN = 'krozties-lien';
const POIDS_MAX = 1000000;
const EN_FRANCAIS = { Earth: 'Terre', Fire: 'Feu', Water: 'Eau', Air: 'Air', Neutral: 'Neutre' };
const POSITIONS = { distance: 'à distance', melee: 'au corps à corps' };

/// Le fichier d'une rotation calculée. Avec un placement, il passe en
/// version 2 : une version qui ne sait pas le lire refuse le fichier, au lieu
/// de calculer sans lui.
export function contenuDuPartage(etat, resultat, demande) {
  const r = etat.resolu || {};
  const reglages = etat.rotation || {};
  const placement = reglages.placement && etat.placement
    ? { lanceur: etat.placement.lanceur, ennemis: etat.placement.ennemis, plateau: etat.plateau || null } : null;
  return JSON.stringify({
    format: FORMAT,
    version: placement ? 2 : 1,
    ...(placement ? { placement } : {}),
    build: etat.build,
    // Les sorts cochés sont un ensemble, que le JSON n'écrit pas : une liste.
    reglages: { ...reglages, coches: reglages.coches ? [...reglages.coches] : null },
    bonus: { ecartes: etat.bonusEcartes || [], inclus: etat.bonusInclus || [] },
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

const lesBonus = (b) => ({
  ecartes: Array.isArray(b && b.ecartes) ? b.ecartes.filter((n) => typeof n === 'string') : [],
  inclus: Array.isArray(b && b.inclus) ? b.inclus.filter((n) => typeof n === 'string') : [],
});

function depuisUnLien(lien) {
  const charge = chargeDuLien(lien);
  if (!charge) throw new Error("Ce lien n'est pas celui d'une rotation Krozties.");
  const lu = lireLeLien(charge);
  return {
    build: lu.build,
    reglages: { ...lu.reglages, classe: lu.build.class, coches: null },
    bonus: lu.bonus,
    placement: lu.placement,
    resume: lu.resume,
    cochesDuLien: lu.coches,
  };
}

/// Une rotation reçue, fichier ou lien, sous une seule forme : le build, les
/// réglages de la Rotation, les bonus écartés ou inclus, le résumé. Les sorts
/// cochés d'un lien attendent la liste que la Rotation proposera pour ce build.
export function lireUneReception({ texte, lien }) {
  if (lien) return depuisUnLien(lien);
  if (texte.length > POIDS_MAX) throw new Error('Ce fichier est trop lourd pour une rotation Krozties.');
  let d = null;
  try {
    d = JSON.parse(texte);
  } catch (e) { /* refusé plus bas */ }
  if (d && d.format === FORMAT_LIEN && typeof d.lien === 'string') return depuisUnLien(d.lien);
  if (!d || d.format !== FORMAT || !d.build || !Number.isInteger(d.build.class)
    || !d.reglages || typeof d.reglages !== 'object') {
    throw new Error("Ce fichier n'est pas une rotation Krozties.");
  }
  if (d.version > 2) throw new Error('Cette rotation vient d\'une version plus récente : mettez Krozties à jour pour l\'ouvrir.');
  const coches = Array.isArray(d.reglages.coches)
    ? new Set(d.reglages.coches.filter((id) => typeof id === 'string')) : null;
  const placement = d.placement && Array.isArray(d.placement.lanceur) && Array.isArray(d.placement.ennemis)
    ? d.placement : null;
  return {
    build: d.build,
    reglages: { ...d.reglages, coches, placement: Boolean(placement) },
    bonus: lesBonus(d.bonus),
    placement,
    resume: d.resume || {},
    cochesDuLien: null,
  };
}

/// « Forgelance 200 · 10 tours · 4 ennemis · à distance ». Un lien ne porte
/// que le numéro de la classe, que `nomDeClasse` traduit.
export function resumeDuPartage(d, nomDeClasse) {
  const r = d.resume || {};
  const classe = typeof r.classe === 'number' ? nomDeClasse(r.classe) : r.classe;
  return [
    [classe, r.niveau].filter(Boolean).join(' ') || 'Rotation',
    r.tours ? `${r.tours} ${pluriel(r.tours, 'tour')}` : null,
    r.ennemis ? `${r.ennemis} ${pluriel(r.ennemis, 'ennemi')}` : null,
    POSITIONS[r.position] || null,
  ].filter(Boolean).join(' · ');
}
