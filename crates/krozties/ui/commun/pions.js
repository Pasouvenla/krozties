// Les cibles posées sur le plateau : le Poutch Ingball, le mannequin
// d'entraînement du jeu. Il monte au-dessus de sa case : `etiquettes.js` le sait
// par `HAUTEUR_POUTCH`, et une étiquette de dégâts se pose au-dessus de sa tête.

import { losange, versEcran } from './damier.js';

const IMAGE_POUTCH = 'plateau/poutch.png';
const TAILLE = 44;
/// Les pieds tombent à 86 % de la hauteur de l'image, sa pointe à 8 %.
const PIEDS = 0.86;
const POINTE = 0.08;

/// Jusqu'où la figure monte au-dessus du centre de sa case.
export const HAUTEUR_POUTCH = Math.round(TAILLE * (PIEDS - POINTE));

/// Des Poutchs debout sur leurs cases, du fond vers l'avant pour qu'un Poutch
/// de devant passe sur celui de derrière. `rang`, s'il est donné, s'inscrit
/// dans une pastille à ses pieds : c'est lui que le tableau des dégâts reprend.
export function poutchs(liste) {
  return [...liste]
    .sort((a, b) => (a.case[0] + a.case[1]) - (b.case[0] + b.case[1]))
    .map(({ case: [x, y], rang }) => {
      const [cx, cy] = versEcran(x, y);
      let svg = `<image class="poutch" href="${IMAGE_POUTCH}" x="${cx - TAILLE / 2}"`
        + ` y="${cy - TAILLE * PIEDS}" width="${TAILLE}" height="${TAILLE}"/>`;
      if (rang != null) {
        const l = String(rang).length > 1 ? 17 : 13;
        svg += `<rect class="poutch-rang" x="${cx + 6}" y="${cy - 4}" width="${l}" height="13" rx="6.5"/>`
          + `<text class="poutch-rang-texte" x="${cx + 6 + l / 2}" y="${cy + 2.5}">${rang}</text>`;
      }
      return svg;
    })
    .join('');
}

/// L'aperçu d'une pièce : elle-même, en transparence.
export const fantome = (svg) => `<g class="fantome-pose">${svg}</g>`;

/// Un clic qui retirerait la pièce de cette case : la case barrée d'une croix.
export function retrait([x, y]) {
  const [cx, cy] = versEcran(x, y);
  return `<g class="apercu-retrait"><polygon points="${losange(x, y)}"/>`
    + `<text x="${cx}" y="${cy - 16}">×</text></g>`;
}

/// Une case où ce clic ne poserait rien : son contour en rouge.
export const interdit = ([x, y]) => `<g class="apercu-interdit"><polygon points="${losange(x, y)}"/></g>`;

/// Un jeton de lettre, celui du lanceur ou d'un allié. Le lanceur a sa case
/// teintée, comme sur le plateau : sa lettre est blanche.
export function jetonLettre([x, y], classe, texte) {
  const [cx, cy] = versEcran(x, y);
  return (classe === 'lanceur' ? `<polygon class="apercu-case" points="${losange(x, y)}"/>` : '')
    + `<text class="jeton ${classe}" x="${cx}" y="${cy}">${texte}</text>`;
}

/// L'icône d'un sort, au-dessus de la case.
export function icone([x, y], dofusdbId) {
  if (!dofusdbId) return '';
  const [cx, cy] = versEcran(x, y);
  return `<image class="apercu-icone" href="icons/${dofusdbId}.png" x="${cx - 14}" y="${cy - 34}" width="28" height="28"/>`;
}
