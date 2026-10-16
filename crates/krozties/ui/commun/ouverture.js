// L'outil à l'écran et l'onglet de KrozTools, retrouvés au lancement suivant.

const CLE = 'krozties.ouverture';

export function lireOuverture() {
  try {
    const d = JSON.parse(localStorage.getItem(CLE));
    return d && typeof d === 'object' ? d : {};
  } catch (e) {
    return {};
  }
}

/// Un stockage qui refuse ne gêne rien : l'application rouvre sur Équipement.
export function garderOuverture(etat) {
  try {
    localStorage.setItem(CLE, JSON.stringify({ outil: etat.outil, kroztool: etat.kroztool || null }));
  } catch (e) {
    // Navigation privée ou stockage plein.
  }
}
