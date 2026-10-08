// Poser des étiquettes sur un damier sans chevauchement : ni étiquette contre
// étiquette, ni étiquette contre le trait d'une autre, ni trait contre trait, ni
// étiquette sur une case occupée.

import { HAUTEUR_CASE, versEcran } from './damier.js';

const HAUTEUR_ETIQ = 16;

const rectContreRect = (a, b) =>
  a.x1 < b.x2 && b.x1 < a.x2 && a.y1 < b.y2 && b.y1 < a.y2;

/// Segment contre rectangle, par découpage de Liang-Barsky.
const segmentCoupeRect = (s, r) => {
  let t0 = 0;
  let t1 = 1;
  const dx = s.x2 - s.x1;
  const dy = s.y2 - s.y1;
  const bornes = [[-dx, s.x1 - r.x1], [dx, r.x2 - s.x1], [-dy, s.y1 - r.y1], [dy, r.y2 - s.y1]];
  for (const [p, q] of bornes) {
    if (p === 0) { if (q < 0) return false; continue; }
    const t = q / p;
    if (p < 0) { if (t > t1) return false; if (t > t0) t0 = t; }
    else { if (t < t0) return false; if (t < t1) t1 = t; }
  }
  return t0 <= t1;
};

const sens = (a, b, c) => Math.sign((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x));
const segmentsSeCoupent = (u, v) => {
  const a = { x: u.x1, y: u.y1 };
  const b = { x: u.x2, y: u.y2 };
  const c = { x: v.x1, y: v.y1 };
  const d = { x: v.x2, y: v.y2 };
  return sens(a, b, c) !== sens(a, b, d) && sens(c, d, a) !== sens(c, d, b);
};

/// Place les étiquettes et rend `{ svg, masquees }`.
///
/// `items` : `[{ case: [x, y], texte, couleur }]`. `occupees` : les cases portant
/// un jeton ; une case peut porter en troisième la hauteur de sa figure,
/// `[x, y, h]`. Sans place libre, pas d'étiquette : l'appelant annonce combien
/// manquent.
export function placer(items, occupees = []) {
  const jetons = occupees.map((c) => {
    const [jx, jy] = versEcran(c[0], c[1]);
    // Une figure et la pastille à ses pieds, ou un simple jeton.
    return c[2]
      ? { x1: jx - 14, y1: jy - c[2], x2: jx + 19, y2: jy + 9 }
      : { x1: jx - 8, y1: jy - 7, x2: jx + 8, y2: jy + 7 };
  });
  const hauteurs = new Map(occupees.filter((c) => c[2]).map((c) => [`${c[0]},${c[1]}`, c[2]]));

  const posees = [];
  let svg = '';
  let masquees = 0;

  // Du fond vers l'avant : une étiquette du fond qui monte ne gêne personne,
  // une étiquette de devant qui monte traverse la carte.
  [...items]
    .sort((a, b) => (a.case[0] + a.case[1]) - (b.case[0] + b.case[1]))
    .forEach(({ case: c, texte, couleur }) => {
      const [ex, ey] = versEcran(c[0], c[1]);
      const l = texte.length * 6.6 + 12;
      const tete = hauteurs.get(`${c[0]},${c[1]}`) || 0;
      const ancre = { x: ex, y: tete ? ey - tete : ey - 3 };
      const idealY = tete ? ey - tete - 13 : ey - HAUTEUR_CASE / 2 - 13;

      const candidates = [];
      for (let ky = 0; ky <= 10; ky += 1) {
        for (const kx of [0, 1, -1, 2, -2, 3, -3, 4, -4]) {
          const cx = ex + kx * (l / 2 + 9);
          const cy = idealY - ky * HAUTEUR_ETIQ;
          candidates.push({ cx, cy, cout: Math.hypot(cx - ex, (cy - idealY) * 1.3) });
        }
      }
      candidates.sort((a, b) => a.cout - b.cout);

      const place = candidates.find(({ cx, cy }) => {
        const bulle = { x1: cx - l / 2, y1: cy - 8, x2: cx + l / 2, y2: cy + 8 };
        const trait = { x1: ancre.x, y1: ancre.y, x2: cx, y2: cy + 8 };
        if (jetons.some((j) => rectContreRect(bulle, j))) return false;
        return posees.every((p) =>
          !rectContreRect(bulle, p.bulle)
          && !segmentCoupeRect(p.trait, bulle)
          && !segmentCoupeRect(trait, p.bulle)
          && !segmentsSeCoupent(trait, p.trait));
      });

      if (!place) { masquees += 1; return; }

      const { cx, cy } = place;
      const bulle = { x1: cx - l / 2, y1: cy - 8, x2: cx + l / 2, y2: cy + 8 };
      const trait = { x1: ancre.x, y1: ancre.y, x2: cx, y2: cy + 8 };
      posees.push({ bulle, trait });
      svg += `<line class="etiq-tige" x1="${trait.x1}" y1="${trait.y1}" x2="${trait.x2}" y2="${trait.y2}"/>`
        + `<rect class="etiq-fond" x="${bulle.x1}" y="${bulle.y1}" width="${l}" height="16" rx="8"/>`
        + `<text class="etiq-texte" x="${cx}" y="${cy}" fill="${couleur}">${texte}</text>`;
    });

  return { svg, masquees };
}
