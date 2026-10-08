// Les conversions qui servent partout, écrites une fois.

export const nb = (v, d = 0) => Number(v).toLocaleString('fr-FR',
  { minimumFractionDigits: d, maximumFractionDigits: d });

export const echapper = (s) => String(s).replace(/[&<>"']/g, (c) =>
  ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

export const FR_ELEMENT = { Fire: 'Feu', Earth: 'Terre', Air: 'Air', Water: 'Eau', Neutral: 'Neutre' };

// Les couleurs d'élément du jeu, lisibles en texte sur fond clair.
export const COULEUR_ELEMENT = {
  Fire: '#c0392b', Feu: '#c0392b',
  Earth: '#8a5a2b', Terre: '#8a5a2b',
  Air: '#2e7d4f',
  Water: '#2a6fb0', Eau: '#2a6fb0',
  Neutral: '#5c6169', Neutre: '#5c6169',
};

export const pluriel = (n, singulier, plur) => (n > 1 ? (plur || `${singulier}s`) : singulier);

/// La phrase avec sa majuscule : toute ligne de l'interface en commence une.
export const majuscule = (s) => (s ? s.charAt(0).toLocaleUpperCase('fr-FR') + s.slice(1) : s);
