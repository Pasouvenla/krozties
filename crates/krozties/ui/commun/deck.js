// Les sorts qu'un build peut jouer, et ceux que la Rotation coche d'office.

import { FR_ELEMENT } from './format.js';

/// Le moteur n'accepte pas plus de trente-deux sorts (masques en `u32`).
export const MAX_SORTS = 32;

/// Les sorts d'un build : ceux de sa classe, avec ce que ses objets de classe
/// leur changent, puis son arme et les sorts que ses objets ajoutent.
export function sortsDuBuild(classe, resolu) {
  if (!classe) return [];
  const r = resolu || {};
  const modifs = r.sorts_modifies || {};
  return [...classe.spells.map((s) => (modifs[s.id] ? { ...s, ...modifs[s.id] } : s)),
    ...(r.arme ? [r.arme] : []), ...(r.sorts_d_objets || [])];
}

/// `resolu.dominant` rend « Eau », les sorts portent « Water » : ne pas les
/// comparer directement.
///
/// Les sorts de l'orientation : ceux qui frappent dans un élément dominant du
/// build ou dans son meilleur élément, pièges et invocations compris, puis ceux qui
/// agissent sans frapper. Les sorts de l'élément d'abord : dans une paire, la
/// variante gardée est la première venue.
export function orientation(sorts, resolu) {
  const dominants = (resolu && resolu.dominant) || [];
  const o = (s) => s.orientation || { elements: s.elements || [], meilleur: false, agit: true };
  const vaut = (s) => o(s).meilleur || o(s).elements.some((e) => dominants.includes(FR_ELEMENT[e] || e));
  const neutre = (s) => !vaut(s) && !o(s).elements.length && o(s).agit;
  // Deux sorts de l'élément dans une paire : celui qui rend le plus par PA
  // sur ce build passe devant, Poisse plutôt que Cruauté sur un Sram Eau.
  const tables = (resolu && resolu.damage_tables) || {};
  const parPa = (s) => {
    const t = tables[s.id] && tables[s.id][0];
    return t && s.ap ? (t.normal[0] + t.normal[1]) / 2 / s.ap : 0;
  };
  return [...sorts.filter(vaut).sort((a, b) => parPa(b) - parPa(a)), ...sorts.filter(neutre)];
}

/// Un défaut ne peut pas être injouable : on garde la première variante de
/// chaque paire, et le joueur change celle qu'il veut.
export function uneParPaire(liste) {
  const vus = new Set();
  return liste.filter((s) => {
    const clef = s.variant_group != null ? `g${s.variant_group}` : `s${s.id}`;
    if (vus.has(clef)) return false;
    vus.add(clef);
    return true;
  });
}

/// Par défaut, ce que le build sait vraiment faire : les sorts de son
/// orientation, une seule variante par paire, bornés au plafond du moteur.
export const cochesParDefaut = (sorts, resolu) => new Set(uneParPaire(orientation(sorts, resolu))
  .slice(0, MAX_SORTS).map((s) => s.id));
